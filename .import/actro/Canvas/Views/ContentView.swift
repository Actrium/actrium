import SwiftUI
import UniformTypeIdentifiers
import AppKit
import Combine

// MARK: - Sidebar Width Preference Key

private struct SidebarWidthKey: PreferenceKey {
    static var defaultValue: CGFloat = 250
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = nextValue()
    }
}

struct ContentView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel
    @ObservedObject var bookmarkVM: BookmarkViewModel
    @ObservedObject var devTools: DevToolsManager = .shared
    @ObservedObject var windowState: WindowStateManager = .shared
    @State private var columnVisibility: NavigationSplitViewVisibility = .all
    @State private var isACTRDropTarget: Bool = false
    @State private var useNativeToolbar: Bool = true  // 使用原生 NSToolbar
    @State private var sidebarWidth: CGFloat = 250

    var body: some View {
        ZStack {
            NavigationSplitView(columnVisibility: $columnVisibility) {
                SidebarView(viewModel: viewModel, bookmarkVM: bookmarkVM, historyVM: historyVM)
                    .toolbar(removing: .sidebarToggle)
                    .overlay(
                        GeometryReader { geo in
                            Color.clear
                                .onChange(of: geo.size.width) { _, newWidth in
                                    sidebarWidth = newWidth
                                }
                                .onAppear {
                                    sidebarWidth = geo.size.width
                                }
                        }
                    )
            } detail: {
                // Workspace: main canvas + optional docked DevTools panel
                // 🎨 Dia/ARC 风格：主内容区域（下圆角，上直角）
                // 四边都有 4px 边距
                WorkspaceView(devTools: devTools, viewModel: viewModel) {
                    WorkspaceMainView(viewModel: viewModel, historyVM: historyVM, bookmarkVM: bookmarkVM, useNativeToolbar: useNativeToolbar)
                }
                .clipShape(BottomRoundedRectangle(bottomRadius: 16))
                // 🔑 让 titlebar 的 AppKit toolbar 精确对齐正文区本体（拖动分割线时也实时对齐）
                // 注意：必须放在 padding 之前，这样探针测到的是“可见内容”的左边界，而不是 detail 列的左边界。
                .background(TitlebarContentLeadingReporter())
                .padding(.leading, 4)
                .padding(.trailing, 4)
                .padding(.bottom, 4)
            }
            // 让 toolbar 背景透明，内容区域背景通栏
            .toolbarBackground(.hidden, for: .windowToolbar)
            // 顶层兜底：拖入 .actr 或目录时创建新标签
            .dropDestination(for: URL.self) { urls, location in
                guard let url = urls.first else {
                    return false
                }

                // Check if it's a .actr file or a directory with manifest.json
                var isDirectory: ObjCBool = false
                guard FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory) else {
                    return false
                }

                if isDirectory.boolValue {
                    // Directory - check if it has manifest.json
                    let manifestPath = url.appendingPathComponent("manifest.json").path
                    guard FileManager.default.fileExists(atPath: manifestPath) else {
                        return false
                    }
                    Task { @MainActor in
                        viewModel.loadFromDevDirectory(url)
                    }
                    return true
                } else if url.pathExtension.lowercased() == "actr" {
                    // .actr file
                    Task { @MainActor in
                        viewModel.importACTRPackage(from: url)
                    }
                    return true
                }

                return false
            } isTargeted: { isTargeted in
                isACTRDropTarget = isTargeted
            }
            // 配置窗口：透明标题栏，让背景通栏
            .background(WindowAccessor(viewModel: viewModel, bookmarkVM: bookmarkVM, useNativeToolbar: useNativeToolbar))
            // 强制清空窗口标题
            .navigationTitle("")
            // 监听 sidebar 宽度变化
            .onChange(of: sidebarWidth) { _, newWidth in
                TitlebarManager.shared.updateSidebarWidth(newWidth)
            }
            // 监听 sidebar 显示/隐藏
            .onChange(of: columnVisibility) { _, newValue in
                let isVisible = newValue != .detailOnly
                TitlebarManager.shared.updateSidebarVisibility(isVisible)
            }
        }
    }
}

// MARK: - Titlebar Alignment Reporter

/// 报告“正文区外框（含 padding）”在窗口坐标系中的精确 x，用来对齐 AppKit titlebar 的 toolbar。
/// 直接测量实际布局，避免依赖 sidebarWidth 推导导致的像素误差。
private struct TitlebarContentLeadingReporter: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        ReporterView()
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        // no-op: layout() 会在尺寸变化时被调用
    }

    private final class ReporterView: NSView {
        private var lastReportedX: CGFloat = .nan

        override func hitTest(_ point: NSPoint) -> NSView? {
            // 透明探针：不拦截任何交互
            nil
        }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            reportIfNeeded()
        }

        override func layout() {
            super.layout()
            reportIfNeeded()
        }

        private func reportIfNeeded() {
            guard window != nil else { return }
            let originInWindow = convert(NSPoint(x: 0, y: 0), to: nil)
            let x = originInWindow.x

            if lastReportedX.isFinite, abs(lastReportedX - x) <= 0.25 {
                return
            }
            lastReportedX = x

            Task { @MainActor in
                TitlebarManager.shared.updateContentLeadingXInWindow(x)
            }
        }
    }
}

// MARK: - Bottom Rounded Rectangle (Dia/ARC Style)

/// 下圆角上直角的矩形（左右下角都有圆角）
/// 与 toolbar 的 TopRoundedRectangle 形成对接
private struct BottomRoundedRectangle: Shape {
    let bottomRadius: CGFloat
    
    func path(in rect: CGRect) -> Path {
        var path = Path()
        
        // 从左上角开始，顺时针
        path.move(to: CGPoint(x: 0, y: 0))
        // 顶边（直角）
        path.addLine(to: CGPoint(x: rect.maxX, y: 0))
        // 右边 - 直线下来到圆角开始处
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.maxY - bottomRadius))
        // 右下圆角
        path.addArc(
            center: CGPoint(x: rect.maxX - bottomRadius, y: rect.maxY - bottomRadius),
            radius: bottomRadius,
            startAngle: .degrees(0),
            endAngle: .degrees(90),
            clockwise: false
        )
        // 底边
        path.addLine(to: CGPoint(x: bottomRadius, y: rect.maxY))
        // 左下圆角
        path.addArc(
            center: CGPoint(x: bottomRadius, y: rect.maxY - bottomRadius),
            radius: bottomRadius,
            startAngle: .degrees(90),
            endAngle: .degrees(180),
            clockwise: false
        )
        // 左边 - 直线上去
        path.closeSubpath()
        
        return path
    }
}

// MARK: - Window Accessor (配置 NSWindow 透明标题栏)

private struct WindowAccessor: NSViewRepresentable {
    let viewModel: BrowserViewModel
    let bookmarkVM: BookmarkViewModel
    let useNativeToolbar: Bool

    func makeNSView(context: Context) -> NSView {
        let view = WindowObserverView()
        view.viewModel = viewModel
        view.bookmarkVM = bookmarkVM
        view.useNativeToolbar = useNativeToolbar
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        if let observer = nsView as? WindowObserverView {
            observer.configureWindow()
        }
    }
}

// MARK: - Window Delegate Proxy (intercept resize BEFORE it happens)

/// Delegate proxy that intercepts windowWillResize to enforce max size limits
/// before the resize reaches ViewBridge/remote service
private class WindowDelegateProxy: NSObject, NSWindowDelegate {
    weak var originalDelegate: NSWindowDelegate?
    var maxSize: NSSize = NSSize(width: 4096, height: 4096)


    /// Intercept resize BEFORE it happens - this prevents the oversized frame
    /// from ever reaching ViewBridge or remote service
    func windowWillResize(_ sender: NSWindow, to frameSize: NSSize) -> NSSize {
        var newSize = frameSize
        
        // Clamp to min size (use both minSize & contentMinSize for safety)
        let minSize = sender.minSize
        if newSize.width < minSize.width {
            newSize.width = minSize.width
        }
        if newSize.height < minSize.height {
            newSize.height = minSize.height
        }
        let contentMin = sender.contentMinSize
        if contentMin.width > 0, newSize.width < contentMin.width {
            newSize.width = contentMin.width
        }
        if contentMin.height > 0, newSize.height < contentMin.height {
            newSize.height = contentMin.height
        }

        // Clamp to max size
        if newSize.width > maxSize.width {
            newSize.width = maxSize.width
        }
        if newSize.height > maxSize.height {
            newSize.height = maxSize.height
        }

        // Also let original delegate process (if it wants to further constrain)
        if let original = originalDelegate,
           original.responds(to: #selector(NSWindowDelegate.windowWillResize(_:to:))) {
            newSize = original.windowWillResize!(sender, to: newSize)
        }

        return newSize
    }

    // MARK: - Forward all other delegate methods to original

    func windowDidResize(_ notification: Notification) {
        // 🔑 如果不是在 live resize 过程中（比如双击最大化/zoom），
        // 需要触发尺寸同步，因为不会有 windowDidEndLiveResize 事件
        // 延迟发送，等待 SwiftUI 布局更新完成
        if !WindowResizeState.shared.isLiveResizing {
            // 获取当前窗口尺寸用于日志
            let windowSize = (notification.object as? NSWindow)?.frame.size ?? .zero
            NSLog("🔔 [ZOOM] windowDidResize (non-live), window=%.0fx%.0f, scheduling delayed notification", windowSize.width, windowSize.height)

            // 延迟后触发尺寸同步
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) {
                // 再次检查，确保不是在 live resize 过程中
                if !WindowResizeState.shared.isLiveResizing {
                    NSLog("🔔 [ZOOM] Posting windowLiveResizeEnded after delay")
                    NotificationCenter.default.post(name: .windowLiveResizeEnded, object: nil)
                }
            }

            // 🔑 额外延迟后再次触发，确保捕获动画结束后的最终状态
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                if !WindowResizeState.shared.isLiveResizing {
                    NSLog("🔔 [ZOOM] Posting second windowLiveResizeEnded after 500ms")
                    NotificationCenter.default.post(name: .windowLiveResizeEnded, object: nil)
                }
            }
        }
        originalDelegate?.windowDidResize?(notification)
    }

    func windowWillStartLiveResize(_ notification: Notification) {
        WindowResizeState.shared.isLiveResizing = true
        // 通知 coordinator 截取快照
        NotificationCenter.default.post(name: .windowLiveResizeStarted, object: nil)
        originalDelegate?.windowWillStartLiveResize?(notification)
    }

    func windowDidEndLiveResize(_ notification: Notification) {
        WindowResizeState.shared.isLiveResizing = false
        // 通知所有 coordinator 发送最终尺寸
        NotificationCenter.default.post(name: .windowLiveResizeEnded, object: nil)
        originalDelegate?.windowDidEndLiveResize?(notification)
    }

    func windowWillMove(_ notification: Notification) {
        originalDelegate?.windowWillMove?(notification)
    }

    func windowDidMove(_ notification: Notification) {
        originalDelegate?.windowDidMove?(notification)
    }

    func windowDidBecomeKey(_ notification: Notification) {
        originalDelegate?.windowDidBecomeKey?(notification)
    }

    func windowDidResignKey(_ notification: Notification) {
        originalDelegate?.windowDidResignKey?(notification)
    }

    func windowDidBecomeMain(_ notification: Notification) {
        originalDelegate?.windowDidBecomeMain?(notification)
    }

    func windowDidResignMain(_ notification: Notification) {
        originalDelegate?.windowDidResignMain?(notification)
    }

    func windowWillClose(_ notification: Notification) {
        originalDelegate?.windowWillClose?(notification)
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        return originalDelegate?.windowShouldClose?(sender) ?? true
    }

    func windowWillMiniaturize(_ notification: Notification) {
        originalDelegate?.windowWillMiniaturize?(notification)
    }

    func windowDidMiniaturize(_ notification: Notification) {
        originalDelegate?.windowDidMiniaturize?(notification)
    }

    func windowDidDeminiaturize(_ notification: Notification) {
        originalDelegate?.windowDidDeminiaturize?(notification)
    }

    func windowDidChangeScreen(_ notification: Notification) {
        originalDelegate?.windowDidChangeScreen?(notification)
    }

    func windowDidChangeScreenProfile(_ notification: Notification) {
        originalDelegate?.windowDidChangeScreenProfile?(notification)
    }

    func windowDidChangeBackingProperties(_ notification: Notification) {
        originalDelegate?.windowDidChangeBackingProperties?(notification)
    }

    func windowWillEnterFullScreen(_ notification: Notification) {
        originalDelegate?.windowWillEnterFullScreen?(notification)
    }

    func windowDidEnterFullScreen(_ notification: Notification) {
        originalDelegate?.windowDidEnterFullScreen?(notification)
    }

    func windowWillExitFullScreen(_ notification: Notification) {
        originalDelegate?.windowWillExitFullScreen?(notification)
    }

    func windowDidExitFullScreen(_ notification: Notification) {
        originalDelegate?.windowDidExitFullScreen?(notification)
    }
}

/// 监听窗口并持续隐藏标题，同时限制窗口最大尺寸
private class WindowObserverView: NSView {
    private var titleObserver: NSKeyValueObservation?
    private var screenObserver: NSObjectProtocol?
    private var mouseMonitor: Any?
    private var delegateProxy: WindowDelegateProxy?
    weak var viewModel: BrowserViewModel?
    weak var bookmarkVM: BookmarkViewModel?
    var useNativeToolbar: Bool = false
    private var toolbarConfigured: Bool = false
    private var observersConfigured: Bool = false
    private var cancellables = Set<AnyCancellable>()

    /// Current maximum window size based on connected screens
    private var currentMaxSize: NSSize = NSSize(width: 4096, height: 4096)

    /// Edge detection threshold (resize handle area)
    private let edgeThreshold: CGFloat = 8

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        configureWindow()
        setupObservers()
        setupThemeObserver()
    }

    func configureWindow() {
        guard let window = window else { return }
        updateWindowBackgroundColor()
        window.titlebarSeparatorStyle = .line
        window.titleVisibility = .hidden
        window.title = ""
        // 设置窗口最小尺寸（备用，主要由 SwiftUI frame 控制）
        window.minSize = NSSize(width: 640, height: 480)
        
        // macOS 26+: 设置窗口圆角
        // 通过 setValue:forKey: 访问私有属性
        window.setValue(18.0, forKey: "cornerRadius")

        // 配置自定义 AppKit 标题栏（只执行一次）
        if useNativeToolbar && !toolbarConfigured,
           let viewModel = viewModel,
           let bookmarkVM = bookmarkVM {
            toolbarConfigured = true
            TitlebarManager.shared.setupTitlebar(for: window, viewModel: viewModel, bookmarkVM: bookmarkVM)
        }
    }
    
    private func setupThemeObserver() {
        guard let viewModel = viewModel else { return }
        
        // Observe tabs changes to update background color when current tab changes or its color updates
        viewModel.$tabs
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                self?.updateWindowBackgroundColor()
            }
            .store(in: &cancellables)
            
        viewModel.$currentTabId
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                self?.updateWindowBackgroundColor()
            }
            .store(in: &cancellables)
    }
    
    private func updateWindowBackgroundColor() {
        guard let window = window, let viewModel = viewModel else { return }
        // 调试用：蓝色背景方便观察
        window.backgroundColor = NSColor.systemBlue
    }

    private func setupObservers() {
        guard let window = window, !observersConfigured else { return }
        observersConfigured = true

        // 计算初始最大尺寸
        updateMaxSize()

        // 安装 delegate 代理，在 resize 发生前拦截（防止超大尺寸传递到 ViewBridge/remote service）
        let proxy = WindowDelegateProxy()
        proxy.originalDelegate = window.delegate
        proxy.maxSize = currentMaxSize
        window.delegate = proxy
        self.delegateProxy = proxy

        // 监听标题变化，立即重置为空
        titleObserver = window.observe(\.title, options: [.new]) { window, _ in
            if !window.title.isEmpty {
                DispatchQueue.main.async {
                    window.title = ""
                    window.titleVisibility = .hidden
                }
            }
        }

        // 监听显示器配置变化（插拔显示器）
        screenObserver = NotificationCenter.default.addObserver(
            forName: NSApplication.didChangeScreenParametersNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            self?.updateMaxSize()
        }

        // 监听鼠标移动，在窗口边缘且到达最大尺寸时切换光标
        mouseMonitor = NSEvent.addLocalMonitorForEvents(matching: [.mouseMoved, .leftMouseDragged]) { [weak self] event in
            self?.updateCursorForEdge(event: event)
            return event
        }

        NSLog("🪟 [Window] Delegate proxy installed, initial maxSize: %@", NSStringFromSize(currentMaxSize))
    }

    /// Detect which edge the mouse is on (if any)
    private struct WindowEdge: OptionSet {
        let rawValue: Int
        static let left   = WindowEdge(rawValue: 1 << 0)
        static let right  = WindowEdge(rawValue: 1 << 1)
        static let top    = WindowEdge(rawValue: 1 << 2)
        static let bottom = WindowEdge(rawValue: 1 << 3)
    }

    /// Update cursor based on mouse position and window size limits
    private func updateCursorForEdge(event: NSEvent) {
        guard let window = window, event.window == window else { return }

        let frame = window.frame
        let mouseLocation = NSEvent.mouseLocation  // Screen coordinates

        // Check if mouse is in the edge area
        let edges = detectEdges(mouseLocation: mouseLocation, windowFrame: frame)
        guard !edges.isEmpty else { return }

        // Check if we're at max size for any edge direction
        let atMaxWidth = frame.size.width >= currentMaxSize.width - 1
        let atMaxHeight = frame.size.height >= currentMaxSize.height - 1

        // Determine if cursor should be disabled for this edge combination
        var shouldDisableCursor = false

        if edges.contains(.right) && atMaxWidth {
            shouldDisableCursor = true
        }
        if edges.contains(.top) && atMaxHeight {
            shouldDisableCursor = true
        }
        // Corner cases: if both directions are maxed
        if edges.contains(.right) && edges.contains(.top) && atMaxWidth && atMaxHeight {
            shouldDisableCursor = true
        }
        if edges.contains(.right) && edges.contains(.bottom) && atMaxWidth {
            shouldDisableCursor = true
        }
        if edges.contains(.left) && edges.contains(.top) && atMaxHeight {
            shouldDisableCursor = true
        }

        if shouldDisableCursor {
            NSCursor.arrow.set()
        }
    }

    /// Detect which edges the mouse is near
    private func detectEdges(mouseLocation: NSPoint, windowFrame: NSRect) -> WindowEdge {
        var edges: WindowEdge = []

        // Check horizontal edges
        if mouseLocation.x >= windowFrame.maxX - edgeThreshold && mouseLocation.x <= windowFrame.maxX + edgeThreshold {
            edges.insert(.right)
        }
        if mouseLocation.x >= windowFrame.minX - edgeThreshold && mouseLocation.x <= windowFrame.minX + edgeThreshold {
            edges.insert(.left)
        }

        // Check vertical edges (macOS screen coordinates: origin at bottom-left)
        if mouseLocation.y >= windowFrame.maxY - edgeThreshold && mouseLocation.y <= windowFrame.maxY + edgeThreshold {
            edges.insert(.top)
        }
        if mouseLocation.y >= windowFrame.minY - edgeThreshold && mouseLocation.y <= windowFrame.minY + edgeThreshold {
            edges.insert(.bottom)
        }

        // Only return edges if mouse is actually within/near the window bounds
        let expandedFrame = windowFrame.insetBy(dx: -edgeThreshold, dy: -edgeThreshold)
        if !expandedFrame.contains(mouseLocation) {
            return []
        }

        return edges
    }

    /// Update maximum size based on current screen configuration
    private func updateMaxSize() {
        currentMaxSize = Self.computeMaximumWindowSize()

        // 同步更新 delegate proxy 的 maxSize
        delegateProxy?.maxSize = currentMaxSize

        NSLog("🪟 [Window] maxSize updated to %@", NSStringFromSize(currentMaxSize))

        // 如果当前窗口已超过新的最大尺寸（比如断开大显示器），立即调整
        if let window = window {
            shrinkWindowIfNeeded(window)
        }
    }

    /// Shrink window if it exceeds the new max size (e.g., after disconnecting a large monitor)
    private func shrinkWindowIfNeeded(_ window: NSWindow) {
        let currentFrame = window.frame
        var needsResize = false
        var newSize = currentFrame.size

        if currentFrame.size.width > currentMaxSize.width {
            newSize.width = currentMaxSize.width
            needsResize = true
        }
        if currentFrame.size.height > currentMaxSize.height {
            newSize.height = currentMaxSize.height
            needsResize = true
        }

        if needsResize {
            // 保持窗口左上角位置不变（macOS 窗口坐标系原点在左下角）
            var newFrame = currentFrame
            newFrame.size = newSize
            newFrame.origin.y = currentFrame.origin.y + (currentFrame.size.height - newSize.height)

            NSLog("🪟 [Window] Shrinking to new maxSize: %@ -> %@", NSStringFromSize(currentFrame.size), NSStringFromSize(newSize))
            window.setFrame(newFrame, display: true, animate: false)
        }
    }

    deinit {
        titleObserver?.invalidate()
        if let observer = screenObserver {
            NotificationCenter.default.removeObserver(observer)
        }
        if let monitor = mouseMonitor {
            NSEvent.removeMonitor(monitor)
        }
    }

    private static func computeMaximumWindowSize() -> NSSize {
        let screens = NSScreen.screens
        guard !screens.isEmpty else {
            return NSSize(width: 4096, height: 4096)
        }

        var maxWidth: CGFloat = 0
        var maxHeight: CGFloat = 0
        for screen in screens {
            let size = screen.frame.size
            maxWidth = max(maxWidth, size.width)
            maxHeight = max(maxHeight, size.height)
        }

        // 防御性上限：避免异常值把窗口系统拖死
        maxWidth = min(maxWidth, 16384)
        maxHeight = min(maxHeight, 16384)

        return NSSize(width: maxWidth, height: maxHeight)
    }
}

// MARK: - NSColor Extension

private extension NSColor {
    convenience init?(hexString: String) {
        var hexSanitized = hexString.trimmingCharacters(in: .whitespacesAndNewlines)
        hexSanitized = hexSanitized.replacingOccurrences(of: "#", with: "")

        var rgb: UInt64 = 0
        guard Scanner(string: hexSanitized).scanHexInt64(&rgb) else {
            return nil
        }

        let length = hexSanitized.count
        let r, g, b, a: CGFloat

        if length == 6 {
            r = CGFloat((rgb & 0xFF0000) >> 16) / 255.0
            g = CGFloat((rgb & 0x00FF00) >> 8) / 255.0
            b = CGFloat(rgb & 0x0000FF) / 255.0
            a = 1.0
        } else if length == 8 {
            r = CGFloat((rgb & 0xFF000000) >> 24) / 255.0
            g = CGFloat((rgb & 0x00FF0000) >> 16) / 255.0
            b = CGFloat((rgb & 0x0000FF00) >> 8) / 255.0
            a = CGFloat(rgb & 0x000000FF) / 255.0
        } else {
            return nil
        }

        self.init(red: r, green: g, blue: b, alpha: a)
    }
}

#Preview {
    ContentView(viewModel: BrowserViewModel(), historyVM: HistoryViewModel(), bookmarkVM: BookmarkViewModel())
}
