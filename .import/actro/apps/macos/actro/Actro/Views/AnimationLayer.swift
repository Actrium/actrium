//
//  AnimationLayer.swift
//  Actro
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
//  - virtualGridLayer 的变换与真实 actro 的变换完全一致
//  - 动画时 VirtualGrid 覆盖真实 Grid，动画完成后隐藏
//
//  数据流：
//  1. WorkspaceMainView 计算 gridResult (positions, cardWidth, cardHeight, actroSize)
//  2. 同步到 AnimationLayerRepresentable
//  3. AnimationLayerNSView 更新 cardSlot 的 frame 和 contents
//  4. 动画时应用与 actro 相同的 transform
//

import SwiftUI
import QuartzCore

// MARK: - Animation Direction

/// 动画方向枚举
enum AnimationDirection {
    case focusToOverview  // Focus → Overview: 从大图淡入小图
    case overviewToFocus  // Overview → Focus: 从小图淡入大图
}

// MARK: - Virtual Grid Card Info

/// 单个卡片槽位的信息 (双缓冲：Focus大图 + Overview小图)
struct VirtualGridCardInfo: Equatable {
    let tabId: UUID
    let position: CGPoint           // 卡片在 actro 坐标系中的位置
    let focusSnapshot: NSImage?     // Focus 模式快照 (大图，viewport 尺寸)
    let overviewSnapshot: NSImage?  // Overview 模式快照 (小图，card 尺寸)
    let title: String               // Tab 标题 (用于渲染悬浮标题栏)
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

    /// 虚拟网格层 - 承载所有卡片槽位，接受 actro 变换
    private let virtualGridLayer = CALayer()

    /// 卡片槽位 - 双层结构 (Focus + Overview) + 标题栏
    private struct CardSlot {
        let container: CALayer       // 容器层 (包含卡片内容)
        let focusLayer: CALayer      // Focus 大图层 (底层)
        let overviewLayer: CALayer   // Overview 小图层 (顶层)
        let titleBar: CALayer        // 悬浮标题栏 (卡片上方)
        let titleLabel: CATextLayer  // 标题文本
        let closeIcon: CALayer       // 关闭按钮图标
    }

    /// 缓存的关闭图标 (使用 CardStyle 共享实现)
    private lazy var cachedCloseIcon = CardStyle.renderCloseIconImage()

    /// 卡片槽位 - 按 Tab ID 索引
    private var cardSlots: [UUID: CardSlot] = [:]

    /// 当前 crossfade 进度 (0.0 = 显示 Focus, 1.0 = 显示 Overview)
    private var crossfadeProgress: CGFloat = 0.0

    /// 当前动画方向 (用于决定 crossfade 方向)
    private var animationDirection: AnimationDirection = .focusToOverview

    // MARK: - Grid 布局数据

    /// Actro 尺寸 (网格内容的总大小)
    private var actroSize: CGSize = .zero

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

    /// 🔑 重写 makeBackingLayer 确保 layer 创建时坐标系就正确
    /// 在 init 中设置 layer?.isGeometryFlipped 可能无效，因为 layer 可能还不存在
    override func makeBackingLayer() -> CALayer {
        let layer = CALayer()
        layer.isGeometryFlipped = true
        return layer
    }

    /// 初始化 CALayer 层级
    private func setupLayers() {
        wantsLayer = true
        layer?.masksToBounds = true

        // 🔑 翻转坐标系：确保在 layer 存在后再次设置（防御性编程）
        layer?.isGeometryFlipped = true

        // ━━━━ 背景层 (与内容区域背景色一致) ━━━━
        // 使用与 WorkspaceMainView 相同的深灰色背景，避免淡出时闪烁
        backgroundLayer.backgroundColor = NSColor(white: 0.12, alpha: 1.0).cgColor
        backgroundLayer.opacity = 0  // 初始隐藏，与 isVisible=false 保持一致
        layer?.addSublayer(backgroundLayer)

        // ━━━━ 虚拟网格层 (包含卡片槽位，整体缩放) ━━━━
        // anchorPoint = (0,0) 表示从左上角进行缩放变换
        virtualGridLayer.anchorPoint = CGPoint(x: 0, y: 0)
        virtualGridLayer.position = CGPoint(x: 0, y: 0)
        virtualGridLayer.opacity = 0  // 初始隐藏，动画时显示
        virtualGridLayer.masksToBounds = true  // 裁剪子层到 bounds 范围
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

        // 虚拟网格层：使用 actroSize，从 (0,0) 开始
        // 注意：设置 bounds 而不是 frame，因为 anchorPoint=(0,0) 时 frame 和 bounds 的关系不同
        virtualGridLayer.bounds = CGRect(origin: .zero, size: actroSize.width > 0 ? actroSize : bounds.size)
        virtualGridLayer.position = .zero

        CATransaction.commit()
    }

    // MARK: - Public API: Grid 配置

    /// 配置网格布局
    /// - Parameters:
    ///   - actroSize: Actro 总尺寸
    ///   - cardSize: 卡片尺寸 (width, height)
    func configureGrid(actroSize: CGSize, cardSize: CGSize) {
        self.actroSize = actroSize
        self.cardSize = cardSize

        CATransaction.begin()
        CATransaction.setDisableActions(true)

        // 设置 virtualGridLayer 的 bounds（不是 frame，因为 anchorPoint=(0,0)）
        virtualGridLayer.bounds = CGRect(origin: .zero, size: actroSize)
        virtualGridLayer.position = .zero

        CATransaction.commit()
    }

    /// 更新卡片数据
    /// - Parameter cards: 所有卡片的信息 (tabId, position, focusSnapshot, overviewSnapshot, title)
    func updateCards(_ cards: [VirtualGridCardInfo]) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)

        let newTabIds = Set(cards.map { $0.tabId })
        let existingTabIds = Set(cardSlots.keys)

        // 移除不再存在的卡片
        for tabId in existingTabIds.subtracting(newTabIds) {
            cardSlots[tabId]?.container.removeFromSuperlayer()
            cardSlots[tabId]?.titleBar.removeFromSuperlayer()
            cardSlots.removeValue(forKey: tabId)
        }

        // 添加或更新卡片
        for card in cards {
            let slot: CardSlot
            if let existing = cardSlots[card.tabId] {
                slot = existing
            } else {
                // ━━━ 创建卡片容器层 ━━━
                // 容器不裁剪，用于承载阴影
                let container = CALayer()
                container.masksToBounds = false  // 不裁剪，允许阴影显示

                // 添加阴影 (与 TabPanel 一致: radius=5, x=0, y=2, opacity=0.3)
                container.shadowColor = NSColor.black.cgColor
                container.shadowOpacity = 0.3
                container.shadowOffset = CGSize(width: 0, height: 2)
                container.shadowRadius = 5

                // 创建 Focus 层 (底层，显示大图)
                // Focus 模式下无圆角 (与 TabPanel cornerRadius(isFocused ? 0 : 10) 一致)
                let focusLayer = CALayer()
                focusLayer.contentsGravity = .resize  // 完全填充，与 SwiftUI .frame() 行为一致
                focusLayer.masksToBounds = true
                focusLayer.cornerRadius = 0  // Focus 模式无圆角
                focusLayer.opacity = 0  // 初始隐藏，由 crossfade 控制
                container.addSublayer(focusLayer)

                // 创建 Overview 层 (顶层，显示小图)
                // Overview 模式下有圆角
                let overviewLayer = CALayer()
                overviewLayer.contentsGravity = .resize  // 完全填充，与 SwiftUI .frame() 行为一致
                overviewLayer.masksToBounds = true
                overviewLayer.cornerRadius = CardStyle.cornerRadius  // Overview 模式有圆角
                overviewLayer.opacity = 1  // 初始显示，由 crossfade 控制
                container.addSublayer(overviewLayer)

                // ━━━ 创建悬浮标题栏 ━━━
                let titleBar = CALayer()
                titleBar.masksToBounds = false

                // 关闭按钮图标 (使用 SwiftUI 渲染，与真实 Grid 完全一致)
                let closeIcon = CALayer()
                closeIcon.contents = cachedCloseIcon.image
                closeIcon.contentsGravity = .resize
                closeIcon.contentsScale = NSScreen.main?.backingScaleFactor ?? 2.0
                titleBar.addSublayer(closeIcon)

                // 标题文本 (使用 CardStyle 共享样式)
                let titleLabel = CATextLayer()
                titleLabel.font = CardStyle.titleFont
                titleLabel.fontSize = CardStyle.titleFontSize
                titleLabel.foregroundColor = CardStyle.titleColor
                titleLabel.truncationMode = .end
                titleLabel.alignmentMode = .left
                titleLabel.contentsScale = NSScreen.main?.backingScaleFactor ?? 2.0
                titleBar.addSublayer(titleLabel)

                virtualGridLayer.addSublayer(container)
                virtualGridLayer.addSublayer(titleBar)

                slot = CardSlot(
                    container: container,
                    focusLayer: focusLayer,
                    overviewLayer: overviewLayer,
                    titleBar: titleBar,
                    titleLabel: titleLabel,
                    closeIcon: closeIcon
                )
                cardSlots[card.tabId] = slot
            }

            // ━━━ 更新卡片 frame ━━━
            let cardFrame = CGRect(origin: card.position, size: cardSize)
            slot.container.frame = cardFrame
            slot.focusLayer.frame = CGRect(origin: .zero, size: cardSize)
            slot.overviewLayer.frame = CGRect(origin: .zero, size: cardSize)

            // 更新阴影路径 (圆角矩形，与内容层一致)
            slot.container.shadowPath = CGPath(
                roundedRect: CGRect(origin: .zero, size: cardSize),
                cornerWidth: CardStyle.cornerRadius,
                cornerHeight: CardStyle.cornerRadius,
                transform: nil
            )

            // ━━━ 更新标题栏 frame ━━━
            let titleBarHeight = CardStyle.titleBarHeight
            let titleBarY = card.position.y - titleBarHeight - CardStyle.titleBarGap
            slot.titleBar.frame = CGRect(
                x: card.position.x,
                y: titleBarY,
                width: cardSize.width,
                height: titleBarHeight
            )

            // 关闭按钮位置 (左侧，垂直居中)
            let iconSize = cachedCloseIcon.size
            let hPadding = CardStyle.titleBarHorizontalPadding
            slot.closeIcon.frame = CGRect(
                x: hPadding,
                y: (titleBarHeight - iconSize.height) / 2,
                width: iconSize.width,
                height: iconSize.height
            )

            // 标题文本位置 (关闭按钮右侧)
            // CATextLayer 从 frame 顶部渲染文本，使用共享的 lineHeight 计算垂直居中
            let titleX = hPadding + iconSize.width + CardStyle.titleBarItemSpacing
            let titleWidth = cardSize.width - hPadding - iconSize.width - CardStyle.titleBarItemSpacing - hPadding
            let titleY = (titleBarHeight - CardStyle.titleLineHeight) / 2
            slot.titleLabel.frame = CGRect(
                x: titleX,
                y: titleY,
                width: titleWidth,
                height: CardStyle.titleLineHeight
            )

            // ━━━ 更新内容 ━━━
            slot.focusLayer.contents = card.focusSnapshot
            slot.overviewLayer.contents = card.overviewSnapshot

            // 更新标题栏内容（[+] 按钮等无标题的卡片隐藏标题栏）
            let hasTitle = !card.title.isEmpty
            slot.titleBar.isHidden = !hasTitle
            if hasTitle {
                slot.titleLabel.string = card.title
            }

            // NOTE: 不在这里应用 crossfade 状态
            // crossfade 由 applyTransform() 或 setAnimationDirection() 统一控制
            // 避免 SwiftUI 更新时机导致的方向错误
        }

        currentCards = cards
        CATransaction.commit()
    }

    /// 应用当前 crossfade 状态到指定槽位
    private func applyCurrentCrossfade(to slot: CardSlot) {
        switch animationDirection {
        case .focusToOverview:
            // Focus → Overview: 从 Focus 大图渐变到 Overview 小图
            // progress 0.0: Focus 100%, Overview 0%
            // progress 1.0: Focus 0%, Overview 100%
            slot.focusLayer.opacity = Float(1.0 - crossfadeProgress)
            slot.overviewLayer.opacity = Float(crossfadeProgress)

        case .overviewToFocus:
            // Overview → Focus: 从 Overview 小图渐变到 Focus 大图
            // progress 0.0: Overview 100%, Focus 0%
            // progress 1.0: Overview 0%, Focus 100%
            slot.overviewLayer.opacity = Float(1.0 - crossfadeProgress)
            slot.focusLayer.opacity = Float(crossfadeProgress)
        }
    }

    // MARK: - Public API: 动画方向设置

    /// 设置动画方向 (在动画开始前调用)
    /// - Parameter direction: 动画方向
    func setAnimationDirection(_ direction: AnimationDirection) {
        animationDirection = direction

        // 重置 crossfade 到起始状态
        crossfadeProgress = 0.0
        updateAllCrossfade()

        NSLog("🎬 VirtualGrid: Animation direction set to %@",
              direction == .focusToOverview ? "Focus→Overview" : "Overview→Focus")
    }

    // MARK: - Public API: 变换控制

    /// 应用 Actro 变换和 crossfade 进度 (与真实 Grid 的变换完全一致)
    /// - Parameters:
    ///   - scale: 缩放因子
    ///   - offset: 平移偏移量
    ///   - crossfade: crossfade 进度 (0.0 到 1.0)
    func applyTransform(scale: CGFloat, offset: CGPoint, crossfade: CGFloat = 0.0) {
        applyTransform(scale: scale, offset: offset, opacity: 1.0 - crossfade)
    }

    /// 应用变换、透明度和圆角（新方案：淡出 + 圆角渐变）
    /// - Parameters:
    ///   - scale: 缩放比例
    ///   - offset: 偏移量
    ///   - opacity: 整体透明度 (1.0 = 完全不透明, 0.0 = 完全透明)
    ///   - cornerRadius: 卡片圆角 (Overview=CardStyle.cornerRadius, Focus=0)
    func applyTransform(scale: CGFloat, offset: CGPoint, opacity: CGFloat, cornerRadius: CGFloat = CardStyle.cornerRadius) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)

        // 应用变换
        var transform = CATransform3DIdentity
        transform = CATransform3DScale(transform, scale, scale, 1.0)
        transform = CATransform3DTranslate(transform, offset.x / scale, offset.y / scale, 0)
        virtualGridLayer.transform = transform

        // 应用整体透明度（淡出效果）
        backgroundLayer.opacity = Float(opacity)
        virtualGridLayer.opacity = Float(opacity)

        // 应用圆角渐变到所有卡片
        for slot in cardSlots.values {
            slot.overviewLayer.cornerRadius = cornerRadius
            slot.focusLayer.cornerRadius = cornerRadius
            slot.container.cornerRadius = cornerRadius
            // 更新阴影路径以匹配新圆角
            if let bounds = slot.container.bounds as CGRect? {
                slot.container.shadowPath = CGPath(
                    roundedRect: bounds,
                    cornerWidth: cornerRadius,
                    cornerHeight: cornerRadius,
                    transform: nil
                )
            }
        }

        CATransaction.commit()
    }

    /// 更新所有卡片槽位的 crossfade 状态
    private func updateAllCrossfade() {
        for slot in cardSlots.values {
            applyCurrentCrossfade(to: slot)
        }
    }

    // MARK: - Public API: 显示/隐藏

    /// 原子性显示动画层并设置初始 transform（避免闪烁）
    /// 在一个 CATransaction 中完成所有设置，确保第一帧就是正确状态
    func showWithTransform(scale: CGFloat, offset: CGPoint, cornerRadius: CGFloat) {
        // 🔍 调试：检查调用前的状态
        NSLog("🔍 VirtualGrid: BEFORE showWithTransform - bgOpacity=%.1f, gridOpacity=%.1f, isVisible=%d, transform=%.2f,%.2f,%.2f",
              backgroundLayer.opacity, virtualGridLayer.opacity, isVisible ? 1 : 0,
              virtualGridLayer.transform.m11, virtualGridLayer.transform.m41, virtualGridLayer.transform.m42)

        isVisible = true

        CATransaction.begin()
        CATransaction.setDisableActions(true)

        // 1. 先设置 transform（在设置 opacity 之前）
        var transform = CATransform3DIdentity
        transform = CATransform3DScale(transform, scale, scale, 1.0)
        transform = CATransform3DTranslate(transform, offset.x / scale, offset.y / scale, 0)
        virtualGridLayer.transform = transform

        // 2. 设置圆角
        for slot in cardSlots.values {
            slot.overviewLayer.cornerRadius = cornerRadius
            slot.focusLayer.cornerRadius = cornerRadius
            slot.container.cornerRadius = cornerRadius
            if let bounds = slot.container.bounds as CGRect? {
                slot.container.shadowPath = CGPath(
                    roundedRect: bounds,
                    cornerWidth: cornerRadius,
                    cornerHeight: cornerRadius,
                    transform: nil
                )
            }
        }

        // 3. 确保 crossfade 状态正确
        updateAllCrossfade()

        // 4. 最后设置 opacity 使其可见
        backgroundLayer.opacity = 1.0
        virtualGridLayer.opacity = 1.0

        // 5. 确保动画层在所有其他内容之上
        // 设置极高的 zPosition 确保 CALayer 层级在最上面
        layer?.zPosition = 10000

        CATransaction.commit()

        NSLog("🎬 VirtualGrid: ShowWithTransform scale=%.2f, offset=(%.1f, %.1f), cornerRadius=%.1f, cardSlots=%d, layer=%@",
              scale, offset.x, offset.y, cornerRadius, cardSlots.count,
              layer != nil ? "OK" : "NIL")
    }

    /// 显示动画层
    func show() {
        guard !isVisible else {
            NSLog("⚠️ VirtualGrid: show() SKIPPED (already visible)")
            return
        }
        isVisible = true

        // 🔍 调试：记录 show() 被调用时的状态
        NSLog("🎬 VirtualGrid: show() called - transform=%.2f,%.2f,%.2f",
              virtualGridLayer.transform.m11, virtualGridLayer.transform.m41, virtualGridLayer.transform.m42)

        CATransaction.begin()
        CATransaction.setDisableActions(true)
        backgroundLayer.opacity = 1.0
        virtualGridLayer.opacity = 1.0

        // 确保所有卡片的 crossfade 状态正确（防止 SwiftUI 更新时机问题）
        updateAllCrossfade()

        CATransaction.commit()

        NSLog("🎬 VirtualGrid: Show")
    }

    /// 隐藏动画层（立即）
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

    /// 淡出动画层
    /// - Parameters:
    ///   - duration: 淡出时长
    ///   - completion: 完成回调
    func fadeOut(duration: CFTimeInterval = 0.15, completion: (() -> Void)? = nil) {
        guard isVisible else {
            completion?()
            return
        }
        isVisible = false

        NSLog("🎬 VirtualGrid: FadeOut (%.2fs)", duration)

        CATransaction.begin()
        CATransaction.setAnimationDuration(duration)
        CATransaction.setAnimationTimingFunction(CAMediaTimingFunction(name: .easeOut))
        CATransaction.setCompletionBlock { [weak self] in
            // 淡出完成后重置 transform
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            self?.virtualGridLayer.transform = CATransform3DIdentity
            CATransaction.commit()

            NSLog("🎬 VirtualGrid: FadeOut complete")
            completion?()
        }

        backgroundLayer.opacity = 0
        virtualGridLayer.opacity = 0

        CATransaction.commit()
    }

    /// 重置状态（立即隐藏）
    func reset() {
        hide()

        CATransaction.begin()
        CATransaction.setDisableActions(true)
        virtualGridLayer.transform = CATransform3DIdentity
        CATransaction.commit()
    }

    /// 重置状态（淡出后隐藏）
    /// - Parameter completion: 完成回调
    func resetWithFadeOut(completion: (() -> Void)? = nil) {
        fadeOut(duration: 0.15, completion: completion)
    }

    // MARK: - Debug API

    /// 调试：获取当前状态信息
    func debugInfo() -> String {
        return """
        VirtualGrid Debug:
        - isVisible: \(isVisible)
        - actroSize: \(actroSize)
        - cardSize: \(cardSize)
        - cardCount: \(cardSlots.count)
        - animationDirection: \(animationDirection)
        - crossfadeProgress: \(crossfadeProgress)
        - transform: scale=\(virtualGridLayer.transform.m11), tx=\(virtualGridLayer.transform.m41), ty=\(virtualGridLayer.transform.m42)
        """
    }
}

// MARK: - SwiftUI Representable

/// SwiftUI 包装器
struct AnimationLayerRepresentable: NSViewRepresentable {

    /// ModeTransitionController 引用 (用于存储 NSView 引用)
    let controller: ModeTransitionController

    /// Actro 尺寸
    let actroSize: CGSize

    /// 卡片尺寸
    let cardSize: CGSize

    /// 卡片信息列表
    let cards: [VirtualGridCardInfo]

    /// 是否显示
    let isVisible: Bool

    func makeNSView(context: Context) -> AnimationLayerNSView {
        let view = AnimationLayerNSView(frame: .zero)
        // 同步设置引用，避免时序问题
        // 注意：此处已在主线程，无需 async
        controller.animationLayerView = view
        return view
    }

    func updateNSView(_ nsView: AnimationLayerNSView, context: Context) {
        // 🔑 动画期间不要更新配置，避免干扰 ModeTransitionController 设置的 transform
        // 动画由 CVDisplayLink 驱动，这里的更新可能会覆盖动画状态
        let isTransitioning = controller.phase.isTransitioning
        NSLog("🔄 AnimationLayer.updateNSView: isTransitioning=%d, isVisible=%d, virtualGridLayer.opacity=%.1f",
              isTransitioning ? 1 : 0, isVisible ? 1 : 0, nsView.layer?.sublayers?.last?.opacity ?? -1)

        guard !isTransitioning else {
            NSLog("🔄 AnimationLayer.updateNSView: SKIPPED (transitioning)")
            return
        }

        // 更新网格配置（仅在非动画期间）
        nsView.configureGrid(actroSize: actroSize, cardSize: cardSize)

        // 更新卡片数据（仅在非动画期间）
        nsView.updateCards(cards)

        // 显示/隐藏（仅在非动画期间）
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
    /// 动画总时长
    static let totalTransitionDuration: Double = 0.35

    /// Spring 响应参数
    static let springResponse: Double = 0.35
    static let springDamping: Double = 1.0
}
