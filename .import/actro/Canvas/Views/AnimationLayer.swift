//
//  AnimationLayer.swift
//  Canvas
//
//  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  动画层 - Focus/Overview 模式切换的两阶段动画
//  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//
//  设计目标：
//  - 使用 CALayer 实现高性能动画，避免 SwiftUI 状态变化触发视图树重绘
//  - 通过快照 + 变换实现平滑的缩放/淡入淡出过渡
//
//  架构：
//  ┌─────────────────────────────────────────────────────────────────┐
//  │  AnimationLayerNSView (NSView, isFlipped=true)                  │
//  │  ├── backgroundLayer: 背景色填充层 (默认隐藏)                     │
//  │  └── containerLayer: 内容容器层 (承载缩放/平移变换)                │
//  │      ├── primaryLayer: 主截图层 (Overview 或 Focus 快照)         │
//  │      └── secondaryLayer: 次截图层 (Cross-dissolve 用)           │
//  └─────────────────────────────────────────────────────────────────┘
//
//  变换公式：
//  - 目标：将 cardRect 区域缩放填满整个视口
//  - scale = min(viewport.width / card.width, viewport.height / card.height)
//  - offset = -cardRect.origin * scale
//  - 变换：先缩放(原点)，后平移 → Point(x,y) → (x*scale + offset.x, y*scale + offset.y)
//
//  使用方式：
//  1. WorkspaceMainView 中嵌入 AnimationLayerRepresentable
//  2. ModeTransitionController 通过 animationLayerView 引用直接操作 CALayer
//  3. 调试模式：通过 debugShowWithScale(_:cardRect:) 手动控制显示
//

import SwiftUI
import QuartzCore

// MARK: - Animation Layer NSView

/// 高性能动画层 - 使用 CALayer 绑过 SwiftUI 状态更新
///
/// Layer 层级：
/// - backgroundLayer: 窗口背景色填充 (动画/调试时才显示)
/// - containerLayer: 内容容器，承载缩放+平移变换
///   - primaryLayer: 主截图 (Overview 快照)
///   - secondaryLayer: 次截图 (Focus 快照，用于 Cross-dissolve)
final class AnimationLayerNSView: NSView {

    // MARK: - CALayer 层级

    /// 背景层 - 填充窗口背景色，默认 opacity=0 隐藏
    private let backgroundLayer = CALayer()

    /// 容器层 - 承载缩放和平移变换，anchorPoint=(0,0) 即左上角
    private let containerLayer = CALayer()

    /// 主截图层 - 显示 Overview 网格快照
    private let primaryLayer = CALayer()

    /// 次截图层 - 显示 Focus 快照，用于 Cross-dissolve 过渡
    private let secondaryLayer = CALayer()

    // MARK: - 视口配置

    /// 视口相对窗口左边缘的偏移量 (即侧边栏宽度)
    private var viewportOffset: CGFloat = 0

    /// 视口尺寸 (主内容区大小)
    private var viewportSize: CGSize = .zero

    // MARK: - 坐标系

    /// 使用翻转坐标系匹配 SwiftUI (原点在左上角)
    /// 注意：CALayer 的 anchorPoint Y 轴仍然是底部为 0
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

    /// 初始化 CALayer 层级结构
    private func setupLayers() {
        wantsLayer = true
        layer?.masksToBounds = true

        // ━━━━ 背景层 ━━━━
        // 填充窗口背景色，默认隐藏 (opacity=0)
        // 仅在动画播放或调试模式时显示
        backgroundLayer.backgroundColor = NSColor.windowBackgroundColor.cgColor
        backgroundLayer.opacity = 0
        layer?.addSublayer(backgroundLayer)

        // ━━━━ 容器层 ━━━━
        // 承载所有内容的变换 (缩放 + 平移)
        // anchorPoint=(0,0) 表示以左上角为变换原点
        // masksToBounds=false 允许缩放时内容溢出边界
        containerLayer.masksToBounds = false
        containerLayer.anchorPoint = .zero
        containerLayer.position = .zero
        layer?.addSublayer(containerLayer)

        // ━━━━ 截图层 ━━━━
        // contentsGravity=.topLeft 确保图片从左上角开始绘制
        primaryLayer.contentsGravity = .topLeft
        primaryLayer.masksToBounds = false
        containerLayer.addSublayer(primaryLayer)

        secondaryLayer.contentsGravity = .topLeft
        secondaryLayer.masksToBounds = false
        containerLayer.addSublayer(secondaryLayer)

        // 禁用 CALayer 隐式动画，我们手动控制每一帧
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        primaryLayer.opacity = 0
        secondaryLayer.opacity = 0
        CATransaction.commit()
    }

    override func layout() {
        super.layout()
        updateLayerFrames()
    }

    private func updateLayerFrames() {
        CATransaction.begin()
        CATransaction.setDisableActions(true)

        let bounds = self.bounds

        // Background fills entire view
        backgroundLayer.frame = bounds

        // Container starts at viewport offset, sized to viewport
        containerLayer.frame = CGRect(
            x: viewportOffset,
            y: 0,
            width: viewportSize.width > 0 ? viewportSize.width : bounds.width - viewportOffset,
            height: viewportSize.height > 0 ? viewportSize.height : bounds.height
        )

        // Image layers fill container
        let containerBounds = containerLayer.bounds
        primaryLayer.frame = containerBounds
        secondaryLayer.frame = containerBounds

        CATransaction.commit()
    }

    // MARK: - Configuration

    /// Configure viewport layout
    /// - Parameters:
    ///   - offset: Horizontal offset from left edge (sidebar width)
    ///   - size: Viewport size
    func configureViewport(offset: CGFloat, size: CGSize) {
        self.viewportOffset = offset
        self.viewportSize = size
        updateLayerFrames()
    }

    // MARK: - Public API

    /// Set primary image
    func setPrimaryImage(_ image: NSImage?) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        primaryLayer.contents = image
        CATransaction.commit()
    }

    /// Set secondary image
    func setSecondaryImage(_ image: NSImage?) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        secondaryLayer.contents = image
        CATransaction.commit()
    }

    /// 更新动画状态 (每帧调用)
    ///
    /// 由 ModeTransitionController 的 CVDisplayLink 回调驱动
    ///
    /// - Parameters:
    ///   - primaryOpacity: 主截图层透明度 (0-1)
    ///   - secondaryOpacity: 次截图层透明度 (0-1)，用于 Cross-dissolve
    ///   - scale: 缩放因子
    ///   - offset: 平移偏移量 (像素)
    ///
    /// 变换公式：
    /// ```
    /// 最终位置 = 原始位置 × scale + offset
    /// ```
    /// 要让 cardRect.origin 移动到 (0,0)：
    /// ```
    /// offset = -cardRect.origin × scale
    /// ```
    func updateAnimationState(
        primaryOpacity: Float,
        secondaryOpacity: Float,
        scale: CGFloat,
        offset: CGPoint
    ) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)

        // 动画时显示背景层
        let isAnimating = primaryOpacity > 0 || secondaryOpacity > 0
        backgroundLayer.opacity = isAnimating ? 1.0 : 0

        primaryLayer.opacity = primaryOpacity
        secondaryLayer.opacity = secondaryOpacity

        // 应用变换：先缩放(以原点为中心)，再平移
        // CATransform3D 的矩阵乘法顺序是 Translate × Scale
        // 对于点 (x,y)：先缩放 → (x×scale, y×scale)，再平移 → (x×scale+offset.x, y×scale+offset.y)
        var transform = CATransform3DIdentity
        transform = CATransform3DScale(transform, scale, scale, 1.0)
        transform = CATransform3DTranslate(transform, offset.x, offset.y, 0)
        containerLayer.transform = transform

        CATransaction.commit()
    }

    /// Reset animation state (hide layers, reset transform)
    func resetState() {
        CATransaction.begin()
        CATransaction.setDisableActions(true)

        backgroundLayer.opacity = 0
        primaryLayer.opacity = 0
        secondaryLayer.opacity = 0
        primaryLayer.contents = nil
        secondaryLayer.contents = nil
        containerLayer.transform = CATransform3DIdentity

        CATransaction.commit()
    }

    // MARK: - 调试 API

    /// 调试模式：显示动画层并应用指定缩放
    ///
    /// 用于手动观察动画层的缩放效果，由底部浮动面板的滑条控制
    ///
    /// - Parameters:
    ///   - scale: 缩放因子 (1.0 = 完整 Overview, targetScale = 卡片填满视口)
    ///   - cardRect: 目标卡片在视口坐标系中的矩形位置
    ///
    /// 工作原理：
    /// ```
    /// scale=1.0 时：offset=(0,0)，显示完整 Overview 网格
    /// scale=2.0 时：offset=(-cardRect.x×2, -cardRect.y×2)，卡片开始放大
    /// scale=targetScale 时：卡片正好填满整个视口
    /// ```
    func debugShowWithScale(_ scale: CGFloat, cardRect: CGRect) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)

        // 显示背景和主截图层
        backgroundLayer.opacity = 1.0
        primaryLayer.opacity = 1.0
        secondaryLayer.opacity = 0

        // 计算偏移量：让 cardRect 的左上角在缩放后位于视口原点
        // offset = -cardRect.origin × scale
        let offset = CGPoint(
            x: -cardRect.origin.x * scale,
            y: -cardRect.origin.y * scale
        )

        // 应用变换
        var transform = CATransform3DIdentity
        transform = CATransform3DScale(transform, scale, scale, 1.0)
        transform = CATransform3DTranslate(transform, offset.x, offset.y, 0)
        containerLayer.transform = transform

        CATransaction.commit()

        NSLog("🎚️ AnimationLayer debug: scale=%.3f, cardRect=(%.1f, %.1f, %.1f, %.1f), offset=(%.1f, %.1f)",
              scale, cardRect.origin.x, cardRect.origin.y, cardRect.width, cardRect.height,
              offset.x, offset.y)
    }

    /// 隐藏调试视图
    func debugHide() {
        resetState()
    }
}

// MARK: - SwiftUI Representable

/// SwiftUI wrapper for AnimationLayerNSView
struct AnimationLayerRepresentable: NSViewRepresentable {
    /// Reference to the controller for accessing the NSView
    let controller: ModeTransitionController

    /// Viewport offset from left edge (sidebar width)
    let viewportOffset: CGFloat

    /// Viewport size
    let viewportSize: CGSize

    /// Debug mode: show animation layer with slider-controlled scale
    let debugMode: Bool

    /// Debug scale value (1.0 = overview, targetScale = focus)
    let debugScale: CGFloat

    /// Card rect for debug mode (first tab card position in viewport)
    let debugCardRect: CGRect

    /// Screenshot image for debug mode
    let debugImage: NSImage?

    func makeNSView(context: Context) -> AnimationLayerNSView {
        let view = AnimationLayerNSView(frame: .zero)
        // Store reference in controller
        DispatchQueue.main.async {
            controller.animationLayerView = view
        }
        return view
    }

    func updateNSView(_ nsView: AnimationLayerNSView, context: Context) {
        // Update viewport configuration
        nsView.configureViewport(offset: viewportOffset, size: viewportSize)

        // Handle debug mode
        if debugMode {
            if let image = debugImage {
                nsView.setPrimaryImage(image)
            }
            nsView.debugShowWithScale(debugScale, cardRect: debugCardRect)
        }
    }
}

// MARK: - Convenience initializer for non-debug mode
extension AnimationLayerRepresentable {
    init(controller: ModeTransitionController, viewportOffset: CGFloat, viewportSize: CGSize) {
        self.controller = controller
        self.viewportOffset = viewportOffset
        self.viewportSize = viewportSize
        self.debugMode = false
        self.debugScale = 1.0
        self.debugCardRect = .zero
        self.debugImage = nil
    }
}

// MARK: - Animation Constants

/// Constants for mode transition animations
enum ModeTransitionConstants {
    /// Total transition duration (both phases combined)
    /// DEBUG: 10x slower for observation (normal: 0.6)
    static let totalTransitionDuration: Double = 6.0

    /// Spring animation response (for reference, not used in manual animation)
    static let springResponse: Double = 0.35

    /// Spring animation damping
    static let springDamping: Double = 1.0

    /// Legacy: Animation Layer duration (equals total duration)
    static let animationDuration: Double = totalTransitionDuration

    /// Blur radius for Overview mode thumbnails
    static let overviewBlurRadius: CGFloat = 4.0

    /// Saturation for Overview mode thumbnails (0.0-1.0)
    static let overviewSaturation: Double = 0.7

    /// Blur radius for Focus mode (clear)
    static let focusBlurRadius: CGFloat = 0.0

    /// Saturation for Focus mode (full color)
    static let focusSaturation: Double = 1.0
}

// MARK: - Legacy SwiftUI View (for reference/fallback)

/// Legacy SwiftUI Animation Layer (kept for reference)
/// Use AnimationLayerRepresentable instead for better performance
struct AnimationLayerLegacy: View {
    let primaryImage: NSImage?
    let secondaryImage: NSImage?
    let primaryOpacity: Double
    let secondaryOpacity: Double
    let scale: CGFloat
    let anchor: UnitPoint
    let frameSize: CGSize

    var body: some View {
        ZStack {
            if let image = primaryImage {
                Image(nsImage: image)
                    .resizable()
                    .aspectRatio(contentMode: .fill)
                    .frame(width: frameSize.width, height: frameSize.height)
                    .clipped()
                    .opacity(primaryOpacity)
            }
            if let image = secondaryImage {
                Image(nsImage: image)
                    .resizable()
                    .aspectRatio(contentMode: .fill)
                    .frame(width: frameSize.width, height: frameSize.height)
                    .clipped()
                    .opacity(secondaryOpacity)
            }
        }
        .scaleEffect(scale, anchor: anchor)
        .frame(width: frameSize.width, height: frameSize.height)
        .clipped()
        .allowsHitTesting(false)
    }
}
