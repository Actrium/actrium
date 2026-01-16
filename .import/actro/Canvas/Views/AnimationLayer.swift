//
//  AnimationLayer.swift
//  Canvas
//
//  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  动画层 - VirtualGrid 方案
//  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//
//  架构：
//  ┌─────────────────────────────────────────────────────────────────┐
//  │  AnimationLayerNSView (NSView, isFlipped=true)                  │
//  │  ├── backgroundLayer: 背景色填充层 (窗口背景色)                   │
//  │  └── virtualGridLayer: 虚拟网格层 (与真实 Grid 布局完全一致)       │
//  │      ├── cardSlot[tab0.id]: Tab 0 的缩略图                       │
//  │      ├── cardSlot[tab1.id]: Tab 1 的缩略图                       │
//  │      └── ...                                                    │
//  └─────────────────────────────────────────────────────────────────┘
//
//  设计原则：
//  - VirtualGrid 与真实 Grid 布局完全同步 (positions, cardWidth, cardHeight)
//  - 直接使用 TabSnapshotCache 中的缩略图，无需截取整个视口
//  - virtualGridLayer 的变换与真实 canvas 的变换完全一致
//  - 动画时 VirtualGrid 覆盖真实 Grid，动画完成后隐藏
//
//  数据流：
//  1. WorkspaceMainView 计算 gridResult (positions, cardWidth, cardHeight, canvasSize)
//  2. 同步到 AnimationLayerRepresentable
//  3. AnimationLayerNSView 更新 cardSlot 的 frame 和 contents
//  4. 动画时应用与 canvas 相同的 transform
//

import SwiftUI
import QuartzCore

// MARK: - Virtual Grid Card Info

/// 单个卡片槽位的信息
struct VirtualGridCardInfo: Equatable {
    let tabId: UUID
    let position: CGPoint      // 卡片在 canvas 坐标系中的位置
    let thumbnail: NSImage?    // 缩略图 (来自 TabSnapshotCache)
}

// MARK: - Animation Layer NSView

/// 高性能动画层 - VirtualGrid 方案
///
/// 使用 CALayer 构建与真实 Grid 布局完全一致的虚拟网格，
/// 每个卡片槽位显示对应 Tab 的缩略图。
final class AnimationLayerNSView: NSView {

    // MARK: - CALayer 层级

    /// 背景层 - 填充窗口背景色
    private let backgroundLayer = CALayer()

    /// 虚拟网格层 - 承载所有卡片槽位，接受 canvas 变换
    private let virtualGridLayer = CALayer()

    /// 卡片槽位层 - 按 Tab ID 索引
    private var cardSlotLayers: [UUID: CALayer] = [:]

    // MARK: - Grid 布局数据

    /// Canvas 尺寸 (网格内容的总大小)
    private var canvasSize: CGSize = .zero

    /// 卡片尺寸
    private var cardSize: CGSize = .zero

    /// 当前显示的卡片信息
    private var currentCards: [VirtualGridCardInfo] = []

    // MARK: - 状态

    /// 是否可见
    private var isVisible: Bool = false

    // MARK: - 坐标系

    /// 使用翻转坐标系匹配 SwiftUI (原点在左上角)
    override var isFlipped: Bool { true }

    // MARK: - Initialization

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setupLayers()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        setupLayers()
    }

    /// 初始化 CALayer 层级
    private func setupLayers() {
        wantsLayer = true
        layer?.masksToBounds = true

        // ━━━━ 背景层 ━━━━
        backgroundLayer.backgroundColor = NSColor.windowBackgroundColor.cgColor
        backgroundLayer.opacity = 0
        layer?.addSublayer(backgroundLayer)

        // ━━━━ 虚拟网格层 ━━━━
        // anchorPoint=(0,0) 以左上角为变换原点，与 SwiftUI 的 .topLeading 一致
        virtualGridLayer.anchorPoint = .zero
        virtualGridLayer.position = .zero
        virtualGridLayer.opacity = 0
        layer?.addSublayer(virtualGridLayer)
    }

    override func layout() {
        super.layout()
        updateLayerFrames()
    }

    private func updateLayerFrames() {
        CATransaction.begin()
        CATransaction.setDisableActions(true)

        // 背景层填满整个视图
        backgroundLayer.frame = bounds

        // 虚拟网格层使用 canvasSize
        virtualGridLayer.frame = CGRect(origin: .zero, size: canvasSize)

        CATransaction.commit()
    }

    // MARK: - Public API: Grid 配置

    /// 配置网格布局
    /// - Parameters:
    ///   - canvasSize: Canvas 总尺寸
    ///   - cardSize: 卡片尺寸 (width, height)
    func configureGrid(canvasSize: CGSize, cardSize: CGSize) {
        self.canvasSize = canvasSize
        self.cardSize = cardSize

        CATransaction.begin()
        CATransaction.setDisableActions(true)
        virtualGridLayer.frame = CGRect(origin: .zero, size: canvasSize)
        CATransaction.commit()
    }

    /// 更新卡片数据
    /// - Parameter cards: 所有卡片的信息 (tabId, position, thumbnail)
    func updateCards(_ cards: [VirtualGridCardInfo]) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)

        let newTabIds = Set(cards.map { $0.tabId })
        let existingTabIds = Set(cardSlotLayers.keys)

        // 移除不再存在的卡片
        for tabId in existingTabIds.subtracting(newTabIds) {
            cardSlotLayers[tabId]?.removeFromSuperlayer()
            cardSlotLayers.removeValue(forKey: tabId)
        }

        // 添加或更新卡片
        for card in cards {
            let slotLayer: CALayer
            if let existing = cardSlotLayers[card.tabId] {
                slotLayer = existing
            } else {
                slotLayer = CALayer()
                slotLayer.contentsGravity = .resizeAspectFill
                slotLayer.masksToBounds = true
                slotLayer.cornerRadius = 8  // 与 TabPanel 的圆角一致
                virtualGridLayer.addSublayer(slotLayer)
                cardSlotLayers[card.tabId] = slotLayer
            }

            // 更新 frame 和内容
            slotLayer.frame = CGRect(origin: card.position, size: cardSize)
            slotLayer.contents = card.thumbnail
        }

        currentCards = cards
        CATransaction.commit()

        NSLog("🎴 VirtualGrid: Updated %d cards, cardSize=%.0fx%.0f",
              cards.count, cardSize.width, cardSize.height)
    }

    // MARK: - Public API: 变换控制

    /// 应用 Canvas 变换 (与真实 Grid 的变换完全一致)
    /// - Parameters:
    ///   - scale: 缩放因子
    ///   - offset: 平移偏移量
    func applyTransform(scale: CGFloat, offset: CGPoint) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)

        var transform = CATransform3DIdentity
        transform = CATransform3DScale(transform, scale, scale, 1.0)
        transform = CATransform3DTranslate(transform, offset.x / scale, offset.y / scale, 0)
        virtualGridLayer.transform = transform

        CATransaction.commit()
    }

    // MARK: - Public API: 显示/隐藏

    /// 显示动画层
    func show() {
        guard !isVisible else { return }
        isVisible = true

        CATransaction.begin()
        CATransaction.setDisableActions(true)
        backgroundLayer.opacity = 1.0
        virtualGridLayer.opacity = 1.0
        CATransaction.commit()

        NSLog("🎬 VirtualGrid: Show")
    }

    /// 隐藏动画层
    func hide() {
        guard isVisible else { return }
        isVisible = false

        CATransaction.begin()
        CATransaction.setDisableActions(true)
        backgroundLayer.opacity = 0
        virtualGridLayer.opacity = 0
        CATransaction.commit()

        NSLog("🎬 VirtualGrid: Hide")
    }

    /// 重置状态
    func reset() {
        hide()

        CATransaction.begin()
        CATransaction.setDisableActions(true)
        virtualGridLayer.transform = CATransform3DIdentity
        CATransaction.commit()
    }

    // MARK: - Debug API

    /// 调试：获取当前状态信息
    func debugInfo() -> String {
        return """
        VirtualGrid Debug:
        - isVisible: \(isVisible)
        - canvasSize: \(canvasSize)
        - cardSize: \(cardSize)
        - cardCount: \(cardSlotLayers.count)
        - transform: scale=\(virtualGridLayer.transform.m11), tx=\(virtualGridLayer.transform.m41), ty=\(virtualGridLayer.transform.m42)
        """
    }
}

// MARK: - SwiftUI Representable

/// SwiftUI 包装器
struct AnimationLayerRepresentable: NSViewRepresentable {

    /// ModeTransitionController 引用 (用于存储 NSView 引用)
    let controller: ModeTransitionController

    /// Canvas 尺寸
    let canvasSize: CGSize

    /// 卡片尺寸
    let cardSize: CGSize

    /// 卡片信息列表
    let cards: [VirtualGridCardInfo]

    /// Canvas 变换
    let transform: (scale: CGFloat, offset: CGPoint)

    /// 是否显示
    let isVisible: Bool

    func makeNSView(context: Context) -> AnimationLayerNSView {
        let view = AnimationLayerNSView(frame: .zero)
        DispatchQueue.main.async {
            controller.animationLayerView = view
        }
        return view
    }

    func updateNSView(_ nsView: AnimationLayerNSView, context: Context) {
        // 更新网格配置
        nsView.configureGrid(canvasSize: canvasSize, cardSize: cardSize)

        // 更新卡片数据
        nsView.updateCards(cards)

        // 应用变换
        nsView.applyTransform(scale: transform.scale, offset: transform.offset)

        // 显示/隐藏
        if isVisible {
            nsView.show()
        } else {
            nsView.hide()
        }
    }
}

// MARK: - Animation Constants

/// 动画常量
enum ModeTransitionConstants {
    /// 动画总时长 (调试: 6.0s, 正常: 0.6s)
    static let totalTransitionDuration: Double = 6.0

    /// Spring 响应参数
    static let springResponse: Double = 0.35
    static let springDamping: Double = 1.0
}
