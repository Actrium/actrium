import AppKit
import SwiftUI
import Combine

// MARK: - Custom Titlebar Manager

@MainActor
private final class ToolbarLayoutState: ObservableObject {
    @Published var isSidebarExpanded: Bool = true
}

@MainActor
final class TitlebarManager: NSObject {
    static let shared = TitlebarManager()

    private var customToolbarView: ToolbarContainerView?
    private var externalLeadingButtonsView: NSHostingView<AnyView>?
    private var externalLeadingButtonsLeadingConstraint: NSLayoutConstraint?
    private let toolbarLayoutState = ToolbarLayoutState()
    private var cancellables = Set<AnyCancellable>()
    private weak var viewModel: BrowserViewModel?
    private weak var bookmarkVM: BookmarkViewModel?

    private override init() {
        super.init()
    }

    private var leadingConstraint: NSLayoutConstraint?
    private weak var window: NSWindow?
    private weak var titlebarContainerView: NSView?
    private var lastReportedContentLeadingXInWindow: CGFloat?

    /// 与正文区对齐的统一 inset（正文区左右/底部都是 4px）
    private let contentInset: CGFloat = 4
    /// SwiftUI 量到的 sidebar 宽度不包含 split divider 的厚度；这里补偿 1px 以对齐正文区。
    private let splitDividerCompensation: CGFloat = 1
    /// 红绿灯到 + 的间距（收起状态）
    private let trafficLightsToPlusSpacing: CGFloat = 24
    /// ToggleSidebar 为基点，Overview 在其左侧（展开状态 24px，收起状态 16px）
    private let toggleToOverviewOffsetExpanded: CGFloat = 24
    private let toggleToOverviewOffsetCollapsed: CGFloat = 16
    /// Overview 为基点，+ 在其左侧 16px
    private let overviewToPlusOffset: CGFloat = 16
    /// Sidebar 收起时 ToggleSidebar 的固定 x（titlebar 坐标）
    private let collapsedToggleLeadingX: CGFloat = 170
    /// LiquidGlassToolbar 自身的左内边距（.padding(.leading, 10)）
    private let toolbarContentLeadingPadding: CGFloat = 10

    /// 🔑 分离布局更新，确保同步响应
    /// 使用约束更新 + 即时布局的策略
    private func applyLeadingConstraintAndLayout(_ newConstant: CGFloat) {
        guard let leadingConstraint else { return }
        if abs(leadingConstraint.constant - newConstant) <= 0.25 {
            return
        }

        // 🔑 更新约束值
        leadingConstraint.constant = newConstant
        updateToolbarContentLeadingInset(containerLeadingConstant: newConstant)
        updateExternalLeadingButtonsPosition(containerLeadingConstant: newConstant)

        // 🔑 在同一 runloop 中延迟触发布局，避免 layout 重入问题
        // 使用 CFRunLoopPerformBlock 确保在当前 runloop 迭代内执行
        CFRunLoopPerformBlock(CFRunLoopGetMain(), CFRunLoopMode.commonModes.rawValue) { [weak self] in
            guard let self = self else { return }
            self.customToolbarView?.superview?.layoutSubtreeIfNeeded()
        }
        CFRunLoopWakeUp(CFRunLoopGetMain())
    }

    func setupTitlebar(for window: NSWindow, viewModel: BrowserViewModel, bookmarkVM: BookmarkViewModel) {
        self.viewModel = viewModel
        self.bookmarkVM = bookmarkVM
        self.window = window

        // Configure window for unified style
        window.styleMask.insert(.unifiedTitleAndToolbar)
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.toolbarStyle = .unified

        // Create empty toolbar (required for unified style & sidebar integration)
        let toolbar = NSToolbar(identifier: "EmptyToolbar")
        toolbar.displayMode = .iconOnly
        toolbar.showsBaselineSeparator = false
        window.toolbar = toolbar

        // Find the titlebar container view in window hierarchy
        guard let titlebarView = findTitlebarContainerView(in: window) else {
            print("⚠️ Could not find titlebar container view")
            return
        }
        self.titlebarContainerView = titlebarView

        // Use default sidebar width initially (will be updated from SwiftUI)
        let sidebarWidth: CGFloat = 250
        let effectiveWidth = max(sidebarWidth, minTogglePosition)

        // Create main toolbar view (navigation, address bar, actions, including sidebar toggle)
        let containerView = ToolbarContainerView(
            viewModel: viewModel,
            bookmarkVM: bookmarkVM,
            layoutState: toolbarLayoutState,
            onOverview: { [weak self] in self?.onToggleFocusOverview?() },
            onToggleSidebar: { [weak self] in self?.toggleSidebar() }
        )
        containerView.translatesAutoresizingMaskIntoConstraints = false
        self.customToolbarView = containerView

        // Add to titlebar
        // 🔑 必须放在最上层：否则会出现“视觉可见但点击被系统 titlebar 覆盖”的情况，导致按钮不可点
        // 红绿灯与左侧留白区域通过 ToolbarContainerView.hitTest 做事件穿透
        titlebarView.addSubview(containerView)

        // Main toolbar - 与正文区左边对齐
        // 正文区: NavigationSplitView detail 从 sidebar 右边开始，然后有 4px padding
        let leading = containerView.leadingAnchor.constraint(equalTo: titlebarView.leadingAnchor, constant: desiredLeadingConstant(fallbackSidebarWidth: effectiveWidth))
        leading.isActive = true
        self.leadingConstraint = leading

        // 右边留 4px 边距，与正文区对称
        NSLayoutConstraint.activate([
            containerView.trailingAnchor.constraint(equalTo: titlebarView.trailingAnchor, constant: -contentInset),
            containerView.topAnchor.constraint(equalTo: titlebarView.topAnchor),
            containerView.bottomAnchor.constraint(equalTo: titlebarView.bottomAnchor),
        ])

        // Sidebar 展开时：[+][Overview] 在黑底外（sidebar 区域）
        externalLeadingButtonsView?.removeFromSuperview()
        let externalLeadingButtons = NSHostingView(
            rootView: AnyView(
                LeadingTabButtonsView(
                    viewModel: viewModel,
                    onOverview: { [weak self] in self?.onToggleFocusOverview?() }
                )
            )
        )
        externalLeadingButtons.translatesAutoresizingMaskIntoConstraints = false
        externalLeadingButtons.wantsLayer = true
        externalLeadingButtons.layer?.backgroundColor = .clear
        titlebarView.addSubview(externalLeadingButtons)
        let extSize = externalLeadingButtons.fittingSize
        let extLeading = externalLeadingButtons.leadingAnchor.constraint(
            equalTo: titlebarView.leadingAnchor,
            constant: leadingButtonsLeadingFor(containerLeadingConstant: leading.constant)
        )
        externalLeadingButtonsLeadingConstraint = extLeading
        NSLayoutConstraint.activate([
            extLeading,
            externalLeadingButtons.centerYAnchor.constraint(equalTo: containerView.centerYAnchor),
            externalLeadingButtons.widthAnchor.constraint(equalToConstant: extSize.width),
            externalLeadingButtons.heightAnchor.constraint(equalToConstant: extSize.height),
        ])
        externalLeadingButtonsView = externalLeadingButtons

        // 初始时同步一次 inset（通常为 0，但避免某些窗口初始状态下越界）
        refreshLeadingLayoutForCurrentState()

        // Observe viewModel changes
        setupObservers(viewModel: viewModel, bookmarkVM: bookmarkVM)
    }

    // Minimum position for toggle button (after traffic lights)
    // Left edge at 110px, button is 28px wide, so right edge at 138px
    private let minTogglePosition: CGFloat = 138

    private func contentXToTitlebarX(_ xInContentView: CGFloat) -> CGFloat? {
        guard let window,
              let contentView = window.contentView,
              let titlebarView = titlebarContainerView
        else { return nil }

        // Convert a point from contentView space -> window -> titlebar, then take the x component.
        let pointInWindow = contentView.convert(NSPoint(x: xInContentView, y: 0), to: nil)
        let pointInTitlebar = titlebarView.convert(pointInWindow, from: nil)
        guard pointInTitlebar.x.isFinite else { return nil }
        return pointInTitlebar.x
    }

    private func desiredLeadingConstant(fallbackSidebarWidth: CGFloat) -> CGFloat {
        // SwiftUI 只量到 sidebar 宽度（不包含 split divider），正文区还额外有 4px padding（contentInset）。
        let effective = max(fallbackSidebarWidth, minTogglePosition)
        let xInContent = effective + splitDividerCompensation + contentInset
        let xInTitlebar = contentXToTitlebarX(xInContent) ?? xInContent
        return max(xInTitlebar, minTogglePosition + contentInset)
    }

    /// SwiftUI/NavigationSplitView 里"正文区外框（含 4px padding）"的窗口坐标系 x。
    /// 用这个值对齐 AppKit titlebar 的 toolbar，可以消除各种系统 inset/divider/hit-area 带来的像素偏差。
    func updateContentLeadingXInWindow(_ xInWindow: CGFloat) {
        guard let titlebarView = titlebarContainerView else { return }

        lastReportedContentLeadingXInWindow = xInWindow

        // 🔑 计算 titlebarView 的 leading 在窗口坐标系中的位置
        let titlebarLeadingInWindow = titlebarView.convert(NSPoint(x: 0, y: 0), to: nil).x

        // 🔑 toolbar 的 leading 约束是相对于 titlebarView 的
        // 所以约束值 = 内容区域位置（窗口坐标）- titlebarView 的位置（窗口坐标）
        let newConstant = xInWindow - titlebarLeadingInWindow
        guard newConstant.isFinite else { return }

        // 背景衬底应当严格对齐正文区（包括 sidebar 收起宽度为 0 的情况）
        // 内容（图标）的位置通过 hostingView 的 inset 来保证不会跑到红绿灯下面
        let clampedConstant = max(0, newConstant)
        // 🔑 同步更新约束并立即布局
        applyLeadingConstraintAndLayout(clampedConstant)
    }

    /// Called from SwiftUI when sidebar width changes (including drag resize)
    func updateSidebarWidth(_ width: CGFloat) {
        // Sidebar 已收起时不更新（避免收起动画过程中把 lastKnownSidebarWidth 覆盖成很小的值）
        guard isSidebarExpanded else { return }
        // Sidebar 收起（宽度=0）时不要覆盖 lastKnownSidebarWidth，否则展开会丢失用户拖动宽度
        if width <= 0.5 {
            return
        }
        // Clamp to minimum toggle position so toolbar 不会压到红绿灯
        let effectiveWidth = max(width, minTogglePosition)
        lastKnownSidebarWidth = effectiveWidth

        // 🔑 如果已经拿到了正文区真实 leading（测量法），就不要再用 sidebarWidth 推导（会把对齐值覆盖回去）
        if isSidebarExpanded, lastReportedContentLeadingXInWindow?.isFinite == true {
            return
        }

        // Update toolbar position immediately when sidebar is expanded
        if isSidebarExpanded {
            let newConstant = desiredLeadingConstant(fallbackSidebarWidth: effectiveWidth)
            // 🔑 同步更新约束并立即布局
            applyLeadingConstraintAndLayout(newConstant)
        }
    }

    private var lastKnownSidebarWidth: CGFloat = 250

    /// Called from SwiftUI when sidebar visibility changes
    func updateSidebarVisibility(_ visible: Bool) {
        // Handle sidebar toggled by other means (keyboard shortcut, menu, etc.)
        if isSidebarExpanded != visible {
            isSidebarExpanded = visible
            toolbarLayoutState.isSidebarExpanded = visible
            refreshLeadingLayoutForCurrentState()

            if visible {
                if let xInWindow = lastReportedContentLeadingXInWindow, xInWindow.isFinite {
                    // 优先用测量值，避免刚展开时出现像素偏差
                    updateContentLeadingXInWindow(xInWindow)
                    return
                }
                let newConstant = desiredLeadingConstant(fallbackSidebarWidth: lastKnownSidebarWidth)
                applyLeadingConstraintAndLayout(newConstant)
                return
            }
            // visible == false:
            // 不在这里强行设置位置，交给 TitlebarContentLeadingReporter 的真实测量值驱动，
            // 这样在 sidebar 收起动画/布局变化过程中 toolbar 会持续对齐正文区。
        }
    }

    private var isSidebarExpanded: Bool = true

    @objc private func toggleSidebar() {
        // Toggle state; 位置由 TitlebarContentLeadingReporter 的真实测量值驱动
        isSidebarExpanded.toggle()
        toolbarLayoutState.isSidebarExpanded = isSidebarExpanded
        refreshLeadingLayoutForCurrentState()

        // Trigger sidebar toggle at the same time
        NSApp.keyWindow?.firstResponder?.tryToPerform(#selector(NSSplitViewController.toggleSidebar(_:)), with: nil)
    }

    /// 允许背景衬底对齐正文区，但 toolbar 内容（图标/地址栏）必须避开红绿灯区域：
    /// 留出红绿灯后，再留白，再出现 Toggle Sidebar icon 和其它控件。
    private func updateToolbarContentLeadingInset(containerLeadingConstant: CGFloat) {
        let inset = toolbarContentInsetFor(containerLeadingConstant: containerLeadingConstant)
        customToolbarView?.setContentLeftInset(inset)
    }

    private func updateExternalLeadingButtonsPosition(containerLeadingConstant: CGFloat) {
        guard let extLeading = externalLeadingButtonsLeadingConstraint else { return }
        let targetX = leadingButtonsLeadingFor(containerLeadingConstant: containerLeadingConstant)
        if abs(extLeading.constant - targetX) <= 0.25 {
            return
        }
        extLeading.constant = targetX
    }

    private func refreshLeadingLayoutForCurrentState() {
        guard let leadingConstraint else { return }
        let currentLeading = leadingConstraint.constant
        updateToolbarContentLeadingInset(containerLeadingConstant: currentLeading)
        updateExternalLeadingButtonsPosition(containerLeadingConstant: currentLeading)
        customToolbarView?.superview?.needsLayout = true
    }

    private func trafficLightsRightInTitlebar() -> CGFloat? {
        guard let window, let titlebarView = titlebarContainerView else { return nil }
        let buttons: [NSWindow.ButtonType] = [.closeButton, .miniaturizeButton, .zoomButton]
        var maxX: CGFloat = -.greatestFiniteMagnitude
        var hasValue = false

        for type in buttons {
            guard let button = window.standardWindowButton(type) else { continue }
            let rectInWindow = button.convert(button.bounds, to: nil)
            let rectInTitlebar = titlebarView.convert(rectInWindow, from: nil)
            guard rectInTitlebar.maxX.isFinite else { continue }
            maxX = max(maxX, rectInTitlebar.maxX)
            hasValue = true
        }

        return hasValue ? maxX : nil
    }

    private func minimumPlusLeadingInTitlebar() -> CGFloat {
        let fallback = minTogglePosition + contentInset
        guard let trafficRight = trafficLightsRightInTitlebar() else { return fallback }
        return max(trafficRight + trafficLightsToPlusSpacing, contentInset)
    }

    private var toggleToPlusOffset: CGFloat {
        let toggleToOverview = isSidebarExpanded ? toggleToOverviewOffsetExpanded : toggleToOverviewOffsetCollapsed
        return toggleToOverview + overviewToPlusOffset
    }

    private func expandedToggleLeadingInTitlebar() -> CGFloat {
        minimumPlusLeadingInTitlebar() + toggleToPlusOffset
    }

    private func desiredToggleLeadingInTitlebar(containerLeadingConstant: CGFloat) -> CGFloat {
        if isSidebarExpanded {
            return expandedToggleLeadingInTitlebar()
        }
        return collapsedToggleLeadingX
    }

    private func toolbarContentInsetFor(containerLeadingConstant: CGFloat) -> CGFloat {
        let desiredToggleX = desiredToggleLeadingInTitlebar(containerLeadingConstant: containerLeadingConstant)
        return max(0, desiredToggleX - containerLeadingConstant - toolbarContentLeadingPadding)
    }

    private func leadingButtonsLeadingFor(containerLeadingConstant: CGFloat) -> CGFloat {
        let desiredToggleX = desiredToggleLeadingInTitlebar(containerLeadingConstant: containerLeadingConstant)
        return desiredToggleX - toggleToPlusOffset
    }

    private func findTitlebarContainerView(in window: NSWindow) -> NSView? {
        // The titlebar container is typically window.contentView.superview's first subview
        // that contains the traffic lights and toolbar
        guard let themeFrame = window.contentView?.superview else { return nil }

        // Look for NSTitlebarContainerView
        for subview in themeFrame.subviews {
            let className = String(describing: type(of: subview))
            if className.contains("TitlebarContainerView") || className.contains("Titlebar") {
                // Find the actual titlebar view inside
                for innerView in subview.subviews {
                    let innerClassName = String(describing: type(of: innerView))
                    if innerClassName.contains("TitlebarView") {
                        return innerView
                    }
                }
                return subview
            }
        }

        // Fallback: use themeFrame itself
        return themeFrame
    }

    private func setupObservers(viewModel: BrowserViewModel, bookmarkVM: BookmarkViewModel) {
        viewModel.objectWillChange
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                self?.customToolbarView?.updateState()
            }
            .store(in: &cancellables)
    }

    var onToggleFocusOverview: (() -> Void)?

    var onToggleBookmark: (() -> Void)? {
        get { customToolbarView?.onBookmarkTap }
        set { customToolbarView?.onBookmarkTap = newValue }
    }

    var onAddressBarTap: (() -> Void)? {
        get { customToolbarView?.onAddressBarTap }
        set { customToolbarView?.onAddressBarTap = newValue }
    }
}

// MARK: - Toolbar Container View (SwiftUI with Liquid Glass)

private class ToolbarContainerView: NSView {

    weak var viewModel: BrowserViewModel?
    weak var bookmarkVM: BookmarkViewModel?
    private let layoutState: ToolbarLayoutState
    private var hostingView: NSHostingView<AnyView>?
    private var hostingLeadingConstraint: NSLayoutConstraint?
    private var backgroundLayer: CAShapeLayer?  // 🔑 AppKit 原生背景层

    // Callbacks
    var onAddressBarTap: (() -> Void)?
    var onBookmarkTap: (() -> Void)?
    var onOverview: (() -> Void)?
    var onToggleSidebar: (() -> Void)?

    init(viewModel: BrowserViewModel, bookmarkVM: BookmarkViewModel, layoutState: ToolbarLayoutState, onOverview: @escaping () -> Void, onToggleSidebar: @escaping () -> Void) {
        self.viewModel = viewModel
        self.bookmarkVM = bookmarkVM
        self.layoutState = layoutState
        self.onOverview = onOverview
        self.onToggleSidebar = onToggleSidebar

        super.init(frame: .zero)
        
        // 🔑 设置 AppKit 原生背景层（比 SwiftUI GeometryReader 更快响应）
        setupBackgroundLayer()
        setupSwiftUIView()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
    
    // 🔑 使用 CAShapeLayer 绘制圆角背景，完全同步于 NSView frame
    private func setupBackgroundLayer() {
        wantsLayer = true
        layer?.masksToBounds = false
        
        let bgLayer = CAShapeLayer()
        bgLayer.fillColor = NSColor(white: 0.10, alpha: 1.0).cgColor
        layer?.insertSublayer(bgLayer, at: 0)  // 插入到最底层
        self.backgroundLayer = bgLayer
    }

    // MARK: - Event Passthrough

    /// 让红绿灯区域、以及左侧留白（hostingView 之外）完全点击穿透到系统 titlebar。
    /// 否则自定义 view 放在最上层时会挡住系统按钮，或导致 titlebar 拖动/双击失效。
    override func hitTest(_ point: NSPoint) -> NSView? {
        // 左侧留白（hostingView 之外）直接穿透
        if let hostingView, !hostingView.frame.contains(point) {
            return nil
        }

        // 红绿灯区域穿透（防止遮挡）
        if let trafficRect = trafficLightsUnionRectInSelf(padding: 6), trafficRect.contains(point) {
            return nil
        }

        // 其余区域交给 SwiftUI/NSHostingView 正常处理，否则会出现按钮不可点的问题
        return super.hitTest(point)
    }

    private func trafficLightsUnionRectInSelf(padding: CGFloat) -> CGRect? {
        guard let window else { return nil }
        let buttons: [NSWindow.ButtonType] = [.closeButton, .miniaturizeButton, .zoomButton]
        let views = buttons.compactMap { window.standardWindowButton($0) }
        guard !views.isEmpty else { return nil }

        var union = CGRect.null
        for v in views {
            let rectInWindow = v.convert(v.bounds, to: nil)
            let rectInSelf = convert(rectInWindow, from: nil)
            union = union.union(rectInSelf)
        }

        if union.isNull {
            return nil
        }
        let isFiniteRect = union.origin.x.isFinite && union.origin.y.isFinite && union.size.width.isFinite && union.size.height.isFinite
        if !isFiniteRect {
            return nil
        }
        return union.insetBy(dx: -padding, dy: -padding)
    }

    private func setupSwiftUIView() {
        guard let viewModel = viewModel, let bookmarkVM = bookmarkVM else { return }

        let swiftUIView = LiquidGlassToolbar(
            viewModel: viewModel,
            bookmarkVM: bookmarkVM,
            layoutState: layoutState,
            onOverview: { [weak self] in self?.onOverview?() },
            onBookmarkTap: { [weak self] in self?.onBookmarkTap?() },
            onToggleSidebar: { [weak self] in self?.onToggleSidebar?() }
        )

        let hosting = NSHostingView(rootView: AnyView(swiftUIView))
        hosting.translatesAutoresizingMaskIntoConstraints = false
        // Transparent background for Liquid Glass effect
        hosting.wantsLayer = true
        hosting.layer?.backgroundColor = .clear
        addSubview(hosting)

        let leading = hosting.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 0)
        self.hostingLeadingConstraint = leading

        NSLayoutConstraint.activate([
            leading,
            hosting.trailingAnchor.constraint(equalTo: trailingAnchor),
            hosting.topAnchor.constraint(equalTo: topAnchor),
            hosting.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])

        self.hostingView = hosting
    }

    func setContentLeftInset(_ inset: CGFloat) {
        guard let hostingLeadingConstraint else { return }
        if abs(hostingLeadingConstraint.constant - inset) <= 0.25 {
            return
        }
        hostingLeadingConstraint.constant = inset
    }

    func updateState() {
        // SwiftUI handles updates via @ObservedObject
    }

    func forceLayoutUpdate() {
        // Force layout by invalidating and letting system handle it
        // 避免在 layout 过程中调用 layoutSubtreeIfNeeded() 导致重入
        needsLayout = true
        hostingView?.needsLayout = true
        // 立即更新背景层
        updateBackgroundPath()
    }
    
    // 🔑 立即更新背景（跳过 layout cycle，直接重绘）
    func updateBackgroundImmediately() {
        updateBackgroundPath()
    }
    
    // 🔑 重写 setFrameSize，在 frame 变化时立即更新背景（参考 ViewBridge 的模式）
    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        // 立即更新背景，不等待 layout()
        updateBackgroundPath()
    }
    
    override func layout() {
        super.layout()
        // 🔑 每次 layout 时立即更新背景层路径（完全同步，无延迟）
        updateBackgroundPath()
        // 注意：不在这里设置 hostingView?.needsLayout = true
        // 因为这会导致 "NSHostingView is being laid out reentrantly" 警告
        // NSHostingView 会自动响应父视图的 frame 变化
    }
    
    // 🔑 更新背景层的圆角路径（上左、上右两角弧 16，下两角直角）
    // 与正文区对称：正文区是下左、下右圆角，上直角
    private func updateBackgroundPath() {
        guard let bgLayer = backgroundLayer else { return }

        let topInset: CGFloat = 4
        let cornerRadius: CGFloat = 16

        // NSView 坐标系：原点在左下角，y 向上
        // 背景区域：顶部 4px 边距，左右底部无边距
        let adjustedRect = CGRect(
            x: 0,
            y: 0,  // 底部无边距，与正文区衔接
            width: bounds.width,
            height: bounds.height - topInset  // 顶部 4px 边距
        )
        
        // 创建上圆角下直角的路径（NSView 坐标系：y=0 在底部）
        // 视觉效果：上左、上右两角是圆弧，下两角是直角
        let path = CGMutablePath()
        
        // 从左下角开始（直角）
        path.move(to: CGPoint(x: adjustedRect.minX, y: adjustedRect.minY))
        // 底边（直角，与正文区衔接）
        path.addLine(to: CGPoint(x: adjustedRect.maxX, y: adjustedRect.minY))
        // 右边直线上去到圆角开始处
        path.addLine(to: CGPoint(x: adjustedRect.maxX, y: adjustedRect.maxY - cornerRadius))
        // 右上圆角
        path.addArc(center: CGPoint(x: adjustedRect.maxX - cornerRadius, y: adjustedRect.maxY - cornerRadius),
                    radius: cornerRadius,
                    startAngle: 0,
                    endAngle: .pi / 2,
                    clockwise: false)
        // 顶边
        path.addLine(to: CGPoint(x: adjustedRect.minX + cornerRadius, y: adjustedRect.maxY))
        // 左上圆角
        path.addArc(center: CGPoint(x: adjustedRect.minX + cornerRadius, y: adjustedRect.maxY - cornerRadius),
                    radius: cornerRadius,
                    startAngle: .pi / 2,
                    endAngle: .pi,
                    clockwise: false)
        // 左边直线下去
        path.closeSubpath()

        // 禁用隐式动画，确保完全同步
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        bgLayer.path = path
        CATransaction.commit()
    }
}

private struct LeadingTabButtonsView: View {
    @ObservedObject var viewModel: BrowserViewModel
    var onOverview: () -> Void
    private let overviewOffsetFromPlus: CGFloat = 16
    private let iconPointSize: CGFloat = 16

    var body: some View {
        ZStack(alignment: .leading) {
            Button(action: {
                NSLog("🔘 [Toolbar] New tab button tapped (leading)")
                _ = viewModel.addNewTab()
            }) {
                Image(systemName: "plus")
                    .font(.system(size: iconPointSize, weight: .medium))
                    .frame(width: 30, height: 30)
                    .contentShape(Circle())
            }
            .buttonStyle(.plain)

            Button(action: {
                NSLog("🔘 [Toolbar] Overview button tapped (leading)")
                onOverview()
            }) {
                Image(systemName: "square.on.square")
                    .font(.system(size: iconPointSize, weight: .medium))
                    .frame(width: 30, height: 30)
                    .contentShape(Circle())
            }
            .buttonStyle(.plain)
            .offset(x: overviewOffsetFromPlus)
        }
        .frame(width: 50, height: 36, alignment: .leading)
        .foregroundStyle(.secondary)
    }
}

// MARK: - SwiftUI Liquid Glass Toolbar

private struct LiquidGlassToolbar: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var bookmarkVM: BookmarkViewModel
    @ObservedObject var layoutState: ToolbarLayoutState
    var onOverview: () -> Void
    var onBookmarkTap: () -> Void
    var onToggleSidebar: () -> Void

    @State private var addressText: String = ""
    @State private var isAddressFocused: Bool = false
    @State private var showingBookmarkPopover: Bool = false
    /// Tracks the focus ring animation scale (starts larger, animates to 1.0)
    @State private var focusRingScaleX: CGFloat = 1.0
    @State private var focusRingScaleY: CGFloat = 1.0
    /// Tracks if user has started typing (including IME composition)
    @State private var hasInputActivity: Bool = false

    // Safari 风格地址栏规则：
    // 1) 非编辑态只显示 host（去掉 scheme 和 www.），系统应用显示 about:xxx
    // 2) 点击/快捷键进入编辑态时，切换为完整 URL 并全选
    // 3) 编辑过程中不被外部的 URL/Title 变化覆盖，失焦后恢复精简展示
    private func displayText(for tab: Tab?) -> String {
        guard let tab = tab else { return "" }
        // System tabs: show about:xxx (e.g., "about:history")
        if let systemType = tab.systemTabType {
            return systemType.aboutURL.absoluteString
        }
        // Web content: show host only
        if case .web = tab.contentType, let url = tab.url {
            let host = url.host ?? url.absoluteString
            return host.hasPrefix("www.") ? String(host.dropFirst(4)) : host
        }
        return tab.title
    }

    private func editingText(for tab: Tab?) -> String {
        guard let tab = tab else { return "" }
        // System tabs: use aboutURL
        if let systemType = tab.systemTabType {
            return systemType.aboutURL.absoluteString
        }
        // Other tabs with URL
        if let url = tab.url {
            return url.absoluteString
        }
        return displayText(for: tab)
    }

    private func enterEditingMode() {
        addressText = editingText(for: viewModel.currentTab)

        // Only animate if not already focused
        guard !isAddressFocused else { return }

        // Set initial expanded scale WITHOUT animation (prevent implicit animation conflict)
        var noAnimation = Transaction()
        noAnimation.disablesAnimations = true
        withTransaction(noAnimation) {
            focusRingScaleX = 1.1
            focusRingScaleY = 1.25
        }

        // Show the ring (opacity will animate via .animation modifier)
        isAddressFocused = true

        // Animate scale contraction after opacity animation starts
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) {
            withAnimation(.spring(response: 0.3, dampingFraction: 0.75)) {
                self.focusRingScaleX = 1.0
                self.focusRingScaleY = 1.0
            }
        }
    }

    private func exitEditingMode() {
        isAddressFocused = false
        // Reset scale without animation
        var noAnimation = Transaction()
        noAnimation.disablesAnimations = true
        withTransaction(noAnimation) {
            focusRingScaleX = 1.0
            focusRingScaleY = 1.0
        }
        hasInputActivity = false  // Reset input activity state
        // addressText will be updated by onChange handlers when URL/Tab changes
    }

    // Breakpoints for responsive layout (Safari-style)
    // Normal: nav + 50% address bar + [share/+/tabs] capsule
    // Compact: nav + flexible address bar + [+] circle + [>>] circle
    // Narrow: flexible address bar + [>>] circle
    private let compactThreshold: CGFloat = 500
    private let narrowThreshold: CGFloat = 380
    private let leadingIconPointSize: CGFloat = 16

    var body: some View {
        GeometryReader { geo in
            let isNarrow = geo.size.width < narrowThreshold
            let isCompact = geo.size.width < compactThreshold && !isNarrow
            let isNormal = !isNarrow && !isCompact
            let addressBarWidth: CGFloat = max(200, geo.size.width * 0.5)

            HStack(spacing: 12) {
                toggleSidebarButton
                
                // === Navigation Buttons (hidden in narrow mode) ===
                if !isNarrow {
                    navButtonsGroup
                }

                Spacer(minLength: 0)

                // === Address Bar ===
                addressBarView
                    .frame(width: isNormal ? addressBarWidth : nil)
                    .frame(maxWidth: isNormal ? nil : .infinity)

                Spacer(minLength: 0)

                // === Right side buttons ===
                if isNormal {
                    // Normal: Share only (New Tab + Overview moved to leading side)
                    shareButton
                } else if isCompact {
                    // Compact: [>>] circle
                    overflowMenuButton(showNavActions: false)
                } else {
                    // Narrow: [>>] circle only
                    overflowMenuButton(showNavActions: true)
                }
            }
            .frame(maxHeight: .infinity)
            .padding(.leading, 10)
            .padding(.trailing, 10)
            .animation(.easeInOut(duration: 0.2), value: isNarrow)
            .animation(.easeInOut(duration: 0.2), value: isCompact)
        }
        .onChange(of: viewModel.currentTab?.url) { _, _ in
            updateDisplayText()
        }
        .onChange(of: viewModel.currentTab?.title) { _, _ in
            updateDisplayText()
        }
        .onChange(of: viewModel.currentTabId) { _, _ in
            updateDisplayText()
        }
        .onChange(of: isAddressFocused) { _, focused in
            if !focused {
                // When exiting edit mode, update to display text
                updateAddressTextFromTab()
            }
        }
        .onAppear {
            updateDisplayText()
        }
        // Listen for Cmd-L shortcut (shouldAutoFocusAddressBar)
        .onChange(of: viewModel.shouldAutoFocusAddressBar) { _, shouldFocus in
            if shouldFocus {
                enterEditingMode()
                viewModel.shouldAutoFocusAddressBar = false
            }
        }
        // Listen for click-outside notification from main content area
        .onReceive(NotificationCenter.default.publisher(for: NSNotification.Name("com.canvas.dismissAddressBarFocus"))) { _ in
            exitEditingMode()
        }
    }

    private func updateAddressTextFromTab() {
        guard let tab = viewModel.currentTab else {
            addressText = ""
            return
        }
        addressText = displayText(for: tab)
    }

    private func updateDisplayText() {
        guard !isAddressFocused else { return }
        updateAddressTextFromTab()
    }

    private var toggleSidebarButton: some View {
        Button(action: onToggleSidebar) {
            Image(systemName: "sidebar.left")
                .font(.system(size: leadingIconPointSize, weight: .medium))
                .frame(width: 30, height: 30)
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(.secondary)
        .help("Toggle Sidebar (⌃⌘S)")
    }
    
    private func handleBlur() {
        exitEditingMode()
    }

    private func submitAddress() {
        let input = addressText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !input.isEmpty else { return }
        viewModel.navigateCurrentTab(to: input)
        exitEditingMode()
    }

    // MARK: - Subviews

    /// Navigation buttons [< | >]
    private var navButtonsGroup: some View {
        HStack(spacing: 0) {
            Button(action: {
                NSLog("🔘 [Toolbar] Back button tapped")
                viewModel.goBack()
            }) {
                Image(systemName: "chevron.left")
                    .font(.system(size: 16, weight: .medium))
                    .frame(width: 30, height: 30)
                    .contentShape(Circle())
            }
            .buttonStyle(.plain)
            .disabled(!viewModel.canGoBack)

            Divider()
                .frame(height: 18)
                .padding(.horizontal, 1)

            Button(action: {
                NSLog("🔘 [Toolbar] Forward button tapped")
                viewModel.goForward()
            }) {
                Image(systemName: "chevron.right")
                    .font(.system(size: 16, weight: .medium))
                    .frame(width: 30, height: 30)
                    .contentShape(Circle())
            }
            .buttonStyle(.plain)
            .disabled(!viewModel.canGoForward)
        }
        .foregroundStyle(.secondary)
        .padding(.horizontal, 5)
        .padding(.vertical, 3)
    }

    /// Unified address bar for Web and RN App tabs
    /// Layout: [loading/icon | text field | bookmark | refresh]
    private var addressBarView: some View {
        let mode = addressBarMode

        return HStack(spacing: 0) {
            // Left area: ONE icon only - loading spinner, mode icon, or search icon
            if mode.showsLoading && viewModel.isCurrentTabLoading {
                // Loading: show arc spinner
                ArcSpinner()
                    .frame(width: 32, height: 32)
            } else if let icon = currentLeftIcon {
                // Has mode icon (globe for web, custom/default app icon for actr)
                Button(action: {
                    NSLog("🔘 [Toolbar] Left button tapped (mode: \(mode))")
                    // Toggle left panel visibility
                    withAnimation(.easeInOut(duration: 0.2)) {
                        viewModel.toggleLeftPanelForCurrentTab()
                    }
                    // Send visibility change to XPC service
                    let tabId: UUID? = {
                        if case .focus(let focusTabId) = viewModel.canvasMode {
                            return focusTabId
                        }
                        return viewModel.currentTabId
                    }()
                    if let tabId = tabId {
                        let newVisible = viewModel.isLeftPanelVisible(for: tabId)
                        let coordinator = ViewBridgeCoordinatorManager.shared.coordinator(for: tabId)
                        coordinator.sendLeftPanelVisibilityChanged(newVisible)
                    }
                }) {
                    Image(systemName: icon)
                        .font(.system(size: 14, weight: .light))
                        .frame(width: 32, height: 32)
                        .contentShape(Circle())
                }
                .buttonStyle(.plain)
                .foregroundStyle(Color.secondary.opacity(0.5))
            } else {
                // Empty mode: show search icon
                Image(systemName: "magnifyingglass")
                    .font(.system(size: 14, weight: .medium))
                    .foregroundStyle(Color.secondary.opacity(0.5))
                    .frame(width: 32, height: 32)
            }

            // Address display/input area (no icon here - icon is in left area)
            if isAddressFocused {
                // Editing: TextField with placeholder
                ZStack(alignment: .leading) {
                    // Placeholder text (hidden when any input activity including IME)
                    if addressText.isEmpty && !hasInputActivity {
                        Text("Search or enter website name")
                            .font(.system(size: 13))
                            .foregroundStyle(.secondary)
                            .allowsHitTesting(false)
                    }

                    // Text field
                    AddressInputField(
                        text: $addressText,
                        onSubmit: submitAddress,
                        onBlur: handleBlur,
                        onInputActivity: { hasInputActivity = true }
                    )
                }
                .frame(maxWidth: .infinity, maxHeight: 22)
                .padding(.horizontal, 8)
            } else {
                // Display: Label with host/app name, centered
                Group {
                    if addressText.isEmpty {
                        // Empty state: just placeholder text (icon is in left area)
                        Text("Search or enter website name")
                            .font(.system(size: 13))
                            .foregroundStyle(.secondary)
                    } else {
                        // Has content: show address
                        Text(addressText)
                            .font(.system(size: 13))
                            .foregroundStyle(.primary)
                            .lineLimit(1)
                            .truncationMode(.tail)
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: 22)
                .padding(.horizontal, 8)
                .contentShape(Rectangle())
                .onTapGesture {
                    enterEditingMode()
                }
            }

            // Bookmark button (Chrome-style: click always adds, popover to edit/remove)
            if mode.showsBookmark {
                Button(action: {
                    NSLog("🔘 [Toolbar] Bookmark button tapped, isBookmarked=\(isBookmarked)")
                    if !isBookmarked, let tab = viewModel.currentTab {
                        let bookmark = Bookmark.from(tab: tab, manufacturer: nil)
                        bookmarkVM.addBookmark(bookmark)
                    }
                    showingBookmarkPopover = true
                }) {
                    Image(systemName: isBookmarked ? "star.fill" : "star")
                        .font(.system(size: 14, weight: .medium))
                        .frame(width: 32, height: 32)
                        .contentShape(Circle())
                }
                .buttonStyle(.plain)
                .foregroundStyle(isBookmarked ? Color.secondary : Color.secondary.opacity(0.5))
                .popover(isPresented: $showingBookmarkPopover, arrowEdge: .bottom) {
                    bookmarkPopoverContent
                }
            }

            // Refresh button
            if mode.showsRefresh {
                Button(action: {
                    NSLog("🔘 [Toolbar] Refresh button tapped")
                    viewModel.reloadCurrentTab()
                }) {
                    Image(systemName: "arrow.clockwise")
                        .font(.system(size: 14, weight: .medium))
                        .frame(width: 32, height: 32)
                        .contentShape(Circle())
                }
                .buttonStyle(.plain)
                .foregroundStyle(Color.secondary.opacity(0.5))
            }
        }
        .frame(height: 30)  // Match nav buttons height
        .padding(.horizontal, 4)
        .padding(.vertical, 3)  // Match nav buttons padding
        .background(Color.white.opacity(0.08))
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .overlay {
            RoundedRectangle(cornerRadius: 8)
                .strokeBorder(Color.blue.opacity(0.75), lineWidth: 3)
                .opacity(isAddressFocused ? 1 : 0)
                .animation(.easeOut(duration: 0.12), value: isAddressFocused)
        }
    }

    // MARK: - Tab Type Helpers

    private enum AddressBarMode {
        case web       // Web content (http/https)
        case app       // React Native app (.actr)
        case empty     // Empty/new tab

        var leftIcon: String? {
            switch self {
            case .web: return "globe"  // Web: globe icon
            case .app: return "list.bullet"
            case .empty: return nil
            }
        }

        var showsLoading: Bool {
            switch self {
            case .web: return true
            case .app: return true  // RN apps can also show loading
            case .empty: return false
            }
        }

        var showsBookmark: Bool {
            switch self {
            case .web, .app: return true
            case .empty: return false
            }
        }

        var showsRefresh: Bool {
            switch self {
            case .web: return true
            case .app: return true  // RN apps can be refreshed too
            case .empty: return false
            }
        }
    }

    private var addressBarMode: AddressBarMode {
        guard let tab = viewModel.currentTab else { return .empty }
        switch tab.contentType {
        case .web: return .web
        case .reactNative: return .app
        case .native: return .app  // Native tabs (like StartPage) show as app mode
        case .empty: return .empty
        }
    }

    /// Get the actual left icon, considering tab's custom appIcon
    private var currentLeftIcon: String? {
        let mode = addressBarMode
        guard let tab = viewModel.currentTab else { return mode.leftIcon }

        // For app mode, use custom appIcon if available
        if case .app = mode, let customIcon = tab.appIcon {
            return customIcon
        }

        return mode.leftIcon
    }

    private var isBookmarked: Bool {
        guard let tab = viewModel.currentTab else { return false }
        return bookmarkVM.isBookmarked(tab: tab)
    }

    @ViewBuilder
    private var bookmarkPopoverContent: some View {
        if let tab = viewModel.currentTab {
            let existingBookmark = bookmarkVM.getBookmark(for: tab)
            BookmarkEditSheet(
                viewModel: bookmarkVM,
                tab: tab,
                existingBookmark: existingBookmark
            )
        } else {
            Text("No tab selected")
                .padding()
        }
    }

    /// Action buttons group [Share + Tabs] in capsule (normal mode)
    private var shareButton: some View {
        ShareLink(item: viewModel.currentTab?.url ?? URL(string: "about:blank")!) {
            Image(systemName: "square.and.arrow.up")
                .font(.system(size: 15, weight: .medium))
                .frame(width: 30, height: 30)
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(.secondary)
        .padding(.horizontal, 5)
        .padding(.vertical, 3)
    }

    /// New tab button (circular, for compact mode)
    private var newTabButton: some View {
        Button(action: {
            NSLog("🔘 [Toolbar] New tab button tapped (compact mode)")
            _ = viewModel.addNewTab()
        }) {
            Image(systemName: "plus")
                .font(.system(size: 16, weight: .medium))
                .frame(width: 36, height: 36)
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(.secondary)
    }

    /// Overflow menu button (circular)
    @ViewBuilder
    private func overflowMenuButton(showNavActions: Bool) -> some View {
        Menu {
            if showNavActions {
                Button(action: { viewModel.goBack() }) {
                    Label("Go Back", systemImage: "chevron.left")
                }
                .disabled(!viewModel.canGoBack)

                Button(action: { viewModel.goForward() }) {
                    Label("Go Forward", systemImage: "chevron.right")
                }
                .disabled(!viewModel.canGoForward)

                Divider()
            }

            if let url = viewModel.currentTab?.url {
                Button(action: {
                    let picker = NSSharingServicePicker(items: [url])
                    if let window = NSApp.keyWindow,
                       let contentView = window.contentView {
                        picker.show(relativeTo: .zero, of: contentView, preferredEdge: .maxY)
                    }
                }) {
                    Label("Share...", systemImage: "square.and.arrow.up")
                }
            }

            Button(action: { _ = viewModel.addNewTab() }) {
                Label("New Tab", systemImage: "plus")
            }

            Button(action: onOverview) {
                Label("Tab Overview", systemImage: "square.on.square")
            }
        } label: {
            Image(systemName: "chevron.right.2")
                .font(.system(size: 14, weight: .medium))
                .frame(width: 36, height: 36)
                .contentShape(Circle())
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .foregroundStyle(.secondary)
    }

}

// MARK: - Address Input Field (for editing state)

/// Simple text field that auto-focuses and selects all on appear
private struct AddressInputField: NSViewRepresentable {
    @Binding var text: String
    var onSubmit: () -> Void
    var onBlur: () -> Void
    var onInputActivity: (() -> Void)?  // Called when any input activity (including IME)

    func makeNSView(context: Context) -> NSTextField {
        let textField = NSTextField()
        textField.font = .systemFont(ofSize: 13)
        textField.isBordered = false
        textField.drawsBackground = false
        textField.focusRingType = .none
        textField.delegate = context.coordinator
        textField.alignment = .left
        textField.lineBreakMode = .byTruncatingTail
        textField.maximumNumberOfLines = 1
        textField.cell?.isScrollable = true
        textField.cell?.wraps = false
        textField.usesSingleLineMode = true
        textField.isAutomaticTextCompletionEnabled = false  // Disable autocomplete popup
        textField.stringValue = text
        context.coordinator.textField = textField

        // Auto-focus and select all on appear, then start observing firstResponder
        DispatchQueue.main.async {
            textField.window?.makeFirstResponder(textField)
            textField.currentEditor()?.selectAll(nil)
            // Start observing after focus is set
            context.coordinator.startObservingFirstResponder()
        }

        return textField
    }

    func updateNSView(_ nsView: NSTextField, context: Context) {
        // Don't update text while editing to avoid cursor issues
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }

    class Coordinator: NSObject, NSTextFieldDelegate {
        var parent: AddressInputField
        weak var textField: NSTextField?
        private var firstResponderObservation: NSKeyValueObservation?

        init(_ parent: AddressInputField) {
            self.parent = parent
            super.init()
        }

        deinit {
            stopObservingFirstResponder()
        }

        func startObservingFirstResponder() {
            guard let window = textField?.window else { return }
            // Use KVO to observe window's firstResponder changes
            firstResponderObservation = window.observe(\.firstResponder, options: [.new]) { [weak self] window, _ in
                self?.checkFirstResponder()
            }
        }

        func stopObservingFirstResponder() {
            firstResponderObservation?.invalidate()
            firstResponderObservation = nil
        }

        private func checkFirstResponder() {
            guard let textField = textField,
                  let window = textField.window else { return }

            // Check if first responder is still our text field or its field editor
            let firstResponder = window.firstResponder
            let isOurTextField = (firstResponder === textField)
            let isFieldEditor = (firstResponder as? NSTextView)?.delegate === textField

            if !isOurTextField && !isFieldEditor {
                // Focus moved away from our text field - trigger blur
                DispatchQueue.main.async {
                    self.parent.onBlur()
                }
            }
        }

        func controlTextDidEndEditing(_ obj: Notification) {
            stopObservingFirstResponder()
            parent.onBlur()
        }

        func controlTextDidBeginEditing(_ obj: Notification) {
            // Start observing first responder changes
            startObservingFirstResponder()
            // Notify when editing starts (user typed something)
            parent.onInputActivity?()
        }

        func controlTextDidChange(_ obj: Notification) {
            guard let tf = obj.object as? NSTextField else { return }
            parent.text = tf.stringValue
            // Also notify on text change (covers IME composition)
            parent.onInputActivity?()
        }

        func control(_ control: NSControl, textView: NSTextView, doCommandBy commandSelector: Selector) -> Bool {
            if commandSelector == #selector(NSResponder.insertNewline(_:)) {
                parent.onSubmit()
                textField?.window?.makeFirstResponder(nil)
                return true
            }
            if commandSelector == #selector(NSResponder.cancelOperation(_:)) {
                textField?.window?.makeFirstResponder(nil)
                return true
            }
            return false
        }
    }
}

// MARK: - Arc Spinner (Safari-style loading indicator)

/// A rotating arc spinner using TimelineView for stable animation
/// Avoids animation restart issues when view reappears rapidly
private struct ArcSpinner: View {
    var body: some View {
        TimelineView(.animation) { timeline in
            let seconds = timeline.date.timeIntervalSinceReferenceDate
            let rotation = (seconds.truncatingRemainder(dividingBy: 0.8)) / 0.8 * 360

            Circle()
                .trim(from: 0, to: 0.7)
                .stroke(Color.secondary, style: StrokeStyle(lineWidth: 2, lineCap: .round))
                .frame(width: 14, height: 14)
                .rotationEffect(.degrees(rotation))
        }
    }
}
