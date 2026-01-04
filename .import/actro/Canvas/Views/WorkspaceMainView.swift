import SwiftUI
import WebKit

private let logger = AppLogger.ui
import AppKit
import UniformTypeIdentifiers

// MARK: - Main View

/// 大画布架构 - 统一的 Canvas 视图
///
/// 核心概念：
/// - 所有 Tab 卡片都在一个大画布上，位置固定
/// - Overview 模式：缩小画布，显示所有卡片网格
/// - Focus 模式：放大画布，让目标卡片填满屏幕
/// - 只通过 scale + offset 变换实现模式切换
struct WorkspaceMainView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel
    @ObservedObject var bookmarkVM: BookmarkViewModel

    // Viewport size from WorkspaceMainViewport (via Environment)
    @Environment(\.viewportSize) private var viewportSize

    @State private var isACTRDropTarget: Bool = false
    @State private var showingBookmarkSheet: Bool = false

    // Toolbar 状态
    @State private var addressInputText: String = ""
    @State private var isAddressBarEditing: Bool = false
    @State private var isAddressBarFirstResponder: Bool = false
    @State private var keyEventMonitor: Any?
    @State private var mouseEventMonitor: Any?

    // Web loading state
    @State private var isWebLoading: Bool = false

    // Search Tabs 状态
    @State private var searchTabsText: String = ""
    @FocusState private var isSearchTabsFocused: Bool
    @State private var shouldAutoFocusSearchTabs: Bool = false

    // Find in Page 状态
    @State private var showFindBar: Bool = false
    @State private var findText: String = ""
    @State private var findResultCount: Int = 0
    @State private var findCurrentIndex: Int = 0
    @FocusState private var isFindBarFocused: Bool

    // 根据搜索文本过滤 tabs
    private var filteredTabs: [Tab] {
        guard !searchTabsText.isEmpty else {
            return viewModel.tabs
        }
        return viewModel.tabs.filter { tab in
            tab.title.localizedCaseInsensitiveContains(searchTabsText)
        }
    }

    // 为过滤后的 tabs 创建新的位置映射（重新排列，从 0 开始连续）
    private var filteredTabPositions: [UUID: Int] {
        guard !searchTabsText.isEmpty else {
            return viewModel.tabSlotPositions
        }
        var newPositions: [UUID: Int] = [:]
        for (index, tab) in filteredTabs.enumerated() {
            newPositions[tab.id] = index
        }
        return newPositions
    }

    var body: some View {
        // viewportSize comes from Environment (set by WorkspaceMainViewport)
        let _ = {
            NSLog("📐 [VIEWPORT] viewportSize=%.1fx%.1f, mode=%@",
                  viewportSize.width, viewportSize.height, String(describing: viewModel.canvasMode))
        }()

        // Safe viewport size (avoid zero dimensions)
        let safeViewportSize = CGSize(
            width: max(viewportSize.width, 1),
            height: max(viewportSize.height, 1)
        )

        // Toolbar height estimation (for grid layout calculations)
        let toolbarHeight: CGFloat = 52
        let contentSize = safeViewportSize
            // 标题栏已移除，卡片比例 = 视口比例
            let titleBarRatio: CGFloat = 0
            let cardAspectRatio = safeViewportSize.width / safeViewportSize.height
            let gridResult = gridLayout(
                for: contentSize,
                cardAspectRatio: cardAspectRatio,
                cardCount: filteredTabs.count
            )
            let canvasSize = gridResult.canvasSize

            // 计算变换
            let transform: (scale: CGFloat, offset: CGPoint) = {
                switch viewModel.canvasMode {
                case .overview:
                    return calculateOverviewTransform(
                        canvasSize: canvasSize,
                        viewportSize: viewportSize
                    )
                case .focus(let tabId):
                    return calculateFocusTransform(
                        for: tabId,
                        gridResult: gridResult,
                        viewportSize: viewportSize,
                        titleBarRatio: titleBarRatio
                    )
                }
            }()

            ZStack(alignment: .topTrailing) {
                // Safari 风格：深灰色内容区域背景（不延伸到 toolbar）
                Color(nsColor: NSColor(white: 0.12, alpha: 1.0))

                // Canvas 视图 - 包含所有 Tab 卡片
                // ViewBridge 始终在 Canvas 内部，通过快照实现平滑动画
                TabGridView(
                    tabs: filteredTabs,
                    tabSlotPositions: filteredTabPositions,
                    positions: gridResult.positions,
                    cardWidth: gridResult.cardWidth,
                    cardHeight: gridResult.cardHeight,
                    titleBarRatio: titleBarRatio,
                    toolbarHeight: toolbarHeight,
                    currentScale: transform.scale,
                    mode: viewModel.canvasMode,
                    showNewTabButton: searchTabsText.isEmpty,
                    viewportSize: safeViewportSize,
                    bookmarkVM: bookmarkVM,
                    historyVM: historyVM,
                    // ━━━━ Mode Switching Trigger #3: 卡片点击 ━━━━
                    onSelectTab: { tabId in
                        enterFocus(tabId: tabId)
                    },
                    onSwapPositions: { tabId1, tabId2 in
                        viewModel.swapSlotPositions(tabId1: tabId1, tabId2: tabId2)
                    },
                    onNewTab: {
                        viewModel.addNewTab()
                    },
                    onCloseTab: { tabId in
                        withAnimation {
                            viewModel.closeTab(id: tabId)
                        }
                    },
                    onNavigate: { tabId, url in
                        // Navigate the tab to the URL from Start Page
                        viewModel.currentTabId = tabId
                        viewModel.navigateCurrentTab(to: url.absoluteString)
                        enterFocus(tabId: tabId)
                    }
                )
                .frame(width: canvasSize.width, height: canvasSize.height)
                // 防止 scale 为 0 导致奇异矩阵
                .scaleEffect(max(transform.scale, 0.001), anchor: .topLeading)
                .offset(x: transform.offset.x, y: transform.offset.y)
                .animation(.spring(response: 0.35, dampingFraction: 1.0, blendDuration: 0), value: viewModel.canvasMode)
                .contentShape(Rectangle())
                // 地址栏退出焦点已由全局鼠标事件监听处理（mouseEventMonitor）

                // Focus 模式下的 Web 内容层（canvas 变换之外）
                // scaleEffect 对 NSViewRepresentable 不生效，所以 Web 内容需要独立渲染
                if case .focus(let focusedTabId) = viewModel.canvasMode,
                   let focusedTab = viewModel.tabs.first(where: { $0.id == focusedTabId }),
                   case .web(let url) = focusedTab.contentType {
                    FocusedWebOverlay(
                        tab: focusedTab,
                        url: url,
                        viewportSize: safeViewportSize,
                        onNavigationFinished: { title, newURL in
                            // Update tab title and record history
                            if let title = title, !title.isEmpty {
                                viewModel.updateTabTitle(for: focusedTabId, title: title)
                            }
                            if let newURL = newURL {
                                viewModel.updateTabURL(for: focusedTabId, url: newURL)
                                // Record to history
                                if let tab = viewModel.tabs.first(where: { $0.id == focusedTabId }) {
                                    historyVM.recordVisit(tab: tab)
                                }
                            }
                            // Update snapshot for Overview mode (delayed to allow rendering)
                            Task {
                                try? await Task.sleep(nanoseconds: 500_000_000) // 0.5s delay
                                await WebViewHolderManager.shared.takeSnapshot(for: focusedTabId)
                            }
                        },
                        onLoadingStateChanged: { loading in
                            isWebLoading = loading
                        },
                        onOpenNewWindow: { newURL in
                            // Open target="_blank" links in a new tab
                            let newTab = viewModel.addNewTab()
                            viewModel.navigateCurrentTab(to: newURL.absoluteString)
                            // Enter focus mode for the new tab
                            enterFocus(tabId: newTab.id)
                        }
                    )
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)

                    // Find in Page Bar
                    if showFindBar {
                        FindInPageBar(
                            findText: $findText,
                            resultCount: findResultCount,
                            currentIndex: findCurrentIndex,
                            isFocused: $isFindBarFocused,
                            onFind: { text in
                                performFind(text: text, forward: true)
                            },
                            onFindNext: {
                                performFind(text: findText, forward: true)
                            },
                            onFindPrevious: {
                                performFind(text: findText, forward: false)
                            },
                            onClose: {
                                showFindBar = false
                                findText = ""
                                clearFindHighlights()
                            }
                        )
                        .transition(.move(edge: .top).combined(with: .opacity))
                    }
                }

                // Glass View 覆盖层处理拖拽（透明层，hitTest 返回 nil 让点击穿透）
                GlassViewOnlyRepresentable(
                    onDropACTR: { url in
                        Task { @MainActor in
                            if case .overview = viewModel.canvasMode {
                                searchTabsText = ""
                            }

                            // Check if it's a directory or .actr file
                            var isDirectory: ObjCBool = false
                            if FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory) {
                                if isDirectory.boolValue {
                                    viewModel.loadFromDevDirectory(url)
                                } else {
                                    viewModel.importACTRPackage(from: url)
                                }
                            }

                            // ━━━━ Mode Switching Trigger #5: 导入 ACTR ━━━━
                            if let newTabId = viewModel.currentTabId {
                                enterFocus(tabId: newTabId)
                            }
                        }
                    }
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)

                // 注意：RN 内容层已移至每个 TabPanel 内部
                // 每个 Tab 都有独立的 RCTRootView，实现真正的缩放

                // Search Tabs 搜索框 - 仅在 Overview 模式显示
                if case .overview = viewModel.canvasMode {
                    HStack(spacing: 8) {
                        Image(systemName: "magnifyingglass")
                            .foregroundColor(.secondary)
                            .font(.system(size: 14))

                        TextField("Search Tabs", text: $searchTabsText)
                            .textFieldStyle(.plain)
                            .frame(width: 150)
                            .focused($isSearchTabsFocused)
                            .onAppear {
                                if shouldAutoFocusSearchTabs {
                                    shouldAutoFocusSearchTabs = false
                                    isSearchTabsFocused = true
                                }
                            }
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 6)
                    .background(Color(nsColor: NSColor.controlBackgroundColor).opacity(0.9))
                    .cornerRadius(8)
                    .padding(.top, 10)
                    .padding(.trailing, 10)
                }

                // Toolbar 底部分隔线（最上层，通栏）
                VStack {
                    Rectangle()
                        .fill(Color(nsColor: NSColor.separatorColor))
                        .frame(height: 1)
                    Spacer()
                }
                .allowsHitTesting(false)
            }
            .toolbar {
                // 使用单个 principal 布局实现地址栏居中
                ToolbarItem(placement: .principal) {
                    GeometryReader { geo in
                        let addressBarWidth = geo.size.width * 0.5

                        HStack(spacing: 0) {
                            // 左侧：导航按钮
                            HStack(spacing: 4) {
                                Button(action: { viewModel.goBack() }) {
                                    Image(systemName: "chevron.left")
                                }
                                .disabled(!viewModel.canGoBack)
                                .help("后退")

                                Button(action: { viewModel.goForward() }) {
                                    Image(systemName: "chevron.right")
                                }
                                .disabled(!viewModel.canGoForward)
                                .help("前进")
                            }

                            Spacer()

                            // 中间：地址栏（50% 宽度，居中）
                            if case .overview = viewModel.canvasMode {
                                // Overview 模式下的空占位
                                Color.clear
                                    .frame(width: addressBarWidth, height: 28)
                            } else {
                            // Determine left icon state based on tab type
                            let leftIconState: AddressBarLeftIconState = {
                                guard let tab = viewModel.currentTab else { return .none }
                                switch tab.contentType {
                                case .reactNative:
                                    return .menu  // RN app shows menu icon
                                case .web:
                                    return isWebLoading ? .loading : .none  // Web shows loading or nothing
                                case .empty:
                                    return .none
                                }
                            }()

                            // Get URL string for edit mode
                            let currentURLString: String = {
                                guard let tab = viewModel.currentTab else { return "" }
                                return tab.url?.absoluteString ?? ""
                            }()

                            // Safari-style: web tab shows main domain only, other tabs show title
                            let displayTitle: String = {
                                guard let tab = viewModel.currentTab else { return "New Tab" }
                                if case .web = tab.contentType, let host = tab.url?.host {
                                    // Strip www. prefix
                                    return host.hasPrefix("www.") ? String(host.dropFirst(4)) : host
                                }
                                return tab.title
                            }()

                            AddressBarContainer(
                                titleText: .constant(displayTitle),
                                urlText: currentURLString,
                                inputText: $addressInputText,
                                isEditing: $isAddressBarEditing,
                                isFirstResponder: $isAddressBarFirstResponder,
                                isBookmarked: viewModel.currentTab.map { bookmarkVM.isBookmarked(tab: $0) } ?? false,
                                leftIconState: leftIconState,
                                toggleLeftPanel: {
                                    withAnimation(.easeInOut(duration: 0.2)) {
                                        viewModel.toggleLeftPanelForCurrentTab()
                                    }
                                },
                                showingBookmarkPopover: $showingBookmarkSheet,
                                shouldAutoFocus: $viewModel.shouldAutoFocusAddressBar,
                                refreshAction: {
                                    viewModel.reloadCurrentTab()
                                },
                                onNavigate: { input in
                                    // 导航到 URL 或 app id
                                    viewModel.navigateCurrentTab(to: input)
                                    // 退出编辑状态（设置 isFirstResponder=false 会自动退出编辑模式）
                                    isAddressBarFirstResponder = false
                                },
                                bookmarkPopoverContent: {
                                    if let tab = viewModel.currentTab {
                                        let existingBookmark = bookmarkVM.getBookmark(for: tab)
                                        BookmarkEditSheet(
                                            viewModel: bookmarkVM,
                                            tab: existingBookmark == nil ? tab : nil,
                                            existingBookmark: existingBookmark
                                        )
                                    }
                                }
                            )
                            .frame(width: addressBarWidth)
                            }

                            Spacer()

                        // 右侧：操作按钮
                        HStack(spacing: 4) {
                            // Downloads button
                            DownloadToolbarButton()

                            // QR Code & Share buttons - only visible in Focus mode with valid URL
                            if case .focus = viewModel.canvasMode,
                               let currentTab = viewModel.currentTab,
                               currentTab.url != nil {
                                ShareToolbarButton(url: currentTab.url)
                                QRCodeToolbarButton(url: currentTab.url)
                            }

                            Button(action: {
                                withAnimation(.spring(response: 0.35, dampingFraction: 1.0, blendDuration: 0)) {
                                    _ = viewModel.addNewTab()
                                }
                            }) {
                                Image(systemName: "plus")
                            }
                            .help("新建标签页")

                            // ━━━━ Mode Switching Trigger #2: Toolbar 按钮 ━━━━
                            Button(action: toggleFocusOverview) {
                                Image(systemName: "square.on.square")
                            }
                            .help("标签总览 (⌘⇧\\)")
                        }
                        }
                    }
                }
            }
        .onChange(of: viewModel.canvasMode) { oldValue, newValue in
            // 从 Focus 切换到 Overview 时，先截图当前页面
            if case .focus(let focusedTabId) = oldValue, case .overview = newValue {
                // 检查是否为 web tab
                if let tab = viewModel.tabs.first(where: { $0.id == focusedTabId }),
                   case .web = tab.contentType {
                    Task {
                        await WebViewHolderManager.shared.takeSnapshot(for: focusedTabId)
                    }
                }
            }

            // 进入 Overview 模式时标记需要自动聚焦搜索框
            if case .overview = newValue {
                shouldAutoFocusSearchTabs = true
            } else {
                // 离开 Overview 模式时清空搜索文本
                searchTabsText = ""
                isSearchTabsFocused = false
            }
        }
        // shouldAutoFocusAddressBar 的处理移到 AddressBarContainer 的 onAppear 中
        // 这样当地址栏实际渲染时才触发，无需延迟
        .onAppear {
            fputs("🟡 onAppear: shouldStartInFocusMode=\(viewModel.shouldStartInFocusMode), currentTabId=\(viewModel.currentTabId?.uuidString.prefix(8) ?? "nil")\n", stderr)
            fflush(stderr)

            // 🔴 开发调试：如果设置了启动时进入 Focus 模式，延迟切换
            // 延迟切换确保渲染进程完全初始化后再切换模式
            // 注意：不要在这里立即修改 @Published 属性，否则会触发视图重建
            if viewModel.shouldStartInFocusMode, let tabId = viewModel.currentTabId {
                logger.notice("🚀 [WorkspaceMainView] 启动时延迟进入 Focus 模式")
                fputs("🟡 onAppear: scheduling focus mode after 1.5s\n", stderr)
                fflush(stderr)
                // 使用本地变量捕获标志，延迟后再重置，避免立即触发视图重建
                DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) {
                    fputs("🟡 onAppear delayed: entering focus mode now\n", stderr)
                    fflush(stderr)
                    viewModel.shouldStartInFocusMode = false
                    enterFocus(tabId: tabId, skipDebounce: true)
                }
            }
            // Note: canvasMode defaults to .overview, no need to set explicitly

            // ━━━━ Mode Switching Trigger #1: 键盘快捷键 ━━━━
            // Cmd+Shift+\ (keyCode 42) → toggleFocusOverview()
            // 参见 MARK: - Mode Switching (统一入口)
            keyEventMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
                let cmdShift: NSEvent.ModifierFlags = [.command, .shift]
                if event.keyCode == 42 && event.modifierFlags.intersection(.deviceIndependentFlagsMask).contains(cmdShift) {
                    toggleFocusOverview()
                    return nil
                }

                // Cmd+F → Toggle find bar (only in Focus mode with web tab)
                if event.keyCode == 3 && event.modifierFlags.intersection(.deviceIndependentFlagsMask).contains(.command) {
                    if case .focus(let tabId) = viewModel.canvasMode,
                       let tab = viewModel.tabs.first(where: { $0.id == tabId }),
                       case .web = tab.contentType {
                        showFindBar.toggle()
                        if showFindBar {
                            isFindBarFocused = true
                        } else {
                            findText = ""
                            // Clear find highlights
                            if let holder = WebViewHolderManager.shared.existingHolder(for: tabId) {
                                holder.webView.evaluateJavaScript("window.getSelection().removeAllRanges()")
                            }
                        }
                        return nil
                    }
                }

                // Escape → Close find bar
                if event.keyCode == 53 && showFindBar {
                    showFindBar = false
                    findText = ""
                    // Clear find highlights
                    if case .focus(let tabId) = viewModel.canvasMode {
                        if let holder = WebViewHolderManager.shared.existingHolder(for: tabId) {
                            holder.webView.evaluateJavaScript("window.getSelection().removeAllRanges()")
                        }
                    }
                    return nil
                }

                return event
            }

            // 点击主窗口区域时退出地址栏编辑状态
            mouseEventMonitor = NSEvent.addLocalMonitorForEvents(matching: .leftMouseDown) { event in
                if isAddressBarEditing {
                    // 延迟执行，让系统先处理点击事件
                    // 只需设置 isFirstResponder=false，onChange 会自动退出编辑模式
                    DispatchQueue.main.async {
                        isAddressBarFirstResponder = false
                    }
                }
                return event
            }

            // ━━━━ Mode Switching Trigger #4: 双指捏合 ━━━━
            // 捏合手势在 CanvasViewService 端处理 (ResizableContainerView.magnify)
            // 通过 DistributedNotificationCenter 发送 "com.canvas.pinchToOverview" 通知
            // 监听在下方 .onReceive(DistributedNotificationCenter...)
        }
        .onDisappear {
            // 清理事件监听
            if let monitor = keyEventMonitor {
                NSEvent.removeMonitor(monitor)
            }
            if let monitor = mouseEventMonitor {
                NSEvent.removeMonitor(monitor)
            }
        }
        // ━━━━ Mode Switching Trigger #4: 双指捏合 (分布式通知监听) ━━━━
        // 参见 MARK: - Mode Switching (统一入口)
        .onReceive(
            DistributedNotificationCenter.default().publisher(for: Notification.Name("com.canvas.pinchToOverview"))
        ) { _ in
            guard case .focus = viewModel.canvasMode else { return }
            fputs("🤏 Received pinchToOverview → exitToOverview()\n", stderr)
            exitToOverview()
        }
    }

    // MARK: - Mode Switching (统一入口)
    //
    // ┌─────────────────────────────────────────────────────────────────────┐
    // │                   Focus ↔ Overview 模式切换架构                       │
    // ├─────────────────────────────────────────────────────────────────────┤
    // │ 触发方式:                                                            │
    // │   1. Cmd+Shift+\    - 快捷键，Toggle 切换                            │
    // │   2. Toolbar 按钮    - 点击网格图标，Toggle 切换                       │
    // │   3. 卡片点击        - Overview 模式下点击卡片进入 Focus               │
    // │   4. 双指捏合        - Focus 模式下捏合手势退出到 Overview              │
    // │                      (由 CanvasViewService 端处理并发送分布式通知)     │
    // │   5. 导入 ACTR       - 导入后自动进入 Focus 模式                       │
    // ├─────────────────────────────────────────────────────────────────────┤
    // │ 核心函数 (所有切换必须通过以下之一):                                    │
    // │   - toggleFocusOverview() : Toggle 切换                              │
    // │   - enterFocus(tabId:)    : 进入 Focus 模式                          │
    // │   - exitToOverview()      : 退出到 Overview 模式                      │
    // ├─────────────────────────────────────────────────────────────────────┤
    // │ 统一处理: 防抖(0.5s)、动画、lastFocusedTabId 记录、日志                │
    // └─────────────────────────────────────────────────────────────────────┘

    /// 上次 mode 切换时间，用于防抖
    @State private var lastModeChangeTime: Date = .distantPast

    /// 上次 focused 的 tab ID，用于快速返回
    @State private var lastFocusedTabId: UUID?

    /// Toggle 切换 Focus <-> Overview 模式
    /// - 从 Focus → Overview：记住当前 tab，切换到 Overview
    /// - 从 Overview → Focus：切换到 determineFocusTarget() 确定的 tab
    private func toggleFocusOverview() {
        switch viewModel.canvasMode {
        case .focus:
            exitToOverview()
        case .overview:
            if let targetTabId = determineFocusTarget() {
                enterFocus(tabId: targetTabId)
            }
        }
    }

    /// 进入 Focus 模式
    /// - Parameter tabId: 要 focus 的 tab ID
    /// - Parameter skipDebounce: 是否跳过防抖（仅用于初始化）
    private func enterFocus(tabId: UUID, skipDebounce: Bool = false) {
        // 已经在目标 Focus 状态，无需切换
        guard viewModel.canvasMode != .focus(tabId: tabId) else { return }

        // 防抖：忽略 0.5 秒内的重复切换
        if !skipDebounce {
            let now = Date()
            guard now.timeIntervalSince(lastModeChangeTime) > 0.5 else {
                fputs("🟢 enterFocus() DEBOUNCED (too fast)\n", stderr)
                return
            }
            lastModeChangeTime = now
        }

        fputs("🟢 enterFocus(\(tabId.uuidString.prefix(8)))\n", stderr)

        // 记录为上次 focused 的 tab
        lastFocusedTabId = tabId

        // 同步设置 currentTabId，确保地址栏和内容显示同一个 tab
        viewModel.currentTabId = tabId

        withAnimation(.spring(response: 0.35, dampingFraction: 1.0, blendDuration: 0)) {
            viewModel.canvasMode = .focus(tabId: tabId)
        }
    }

    /// 退出到 Overview 模式
    private func exitToOverview() {
        // 记住当前 focused 的 tab（在切换前）
        if case .focus(let currentTabId) = viewModel.canvasMode {
            lastFocusedTabId = currentTabId
        } else {
            // 已经在 Overview 模式
            return
        }

        // 防抖：忽略 0.5 秒内的重复切换
        let now = Date()
        guard now.timeIntervalSince(lastModeChangeTime) > 0.5 else {
            fputs("🔴 exitToOverview() DEBOUNCED (too fast)\n", stderr)
            return
        }
        lastModeChangeTime = now

        fputs("🔴 exitToOverview()\n", stderr)

        withAnimation(.spring(response: 0.35, dampingFraction: 1.0, blendDuration: 0)) {
            viewModel.canvasMode = .overview
        }
    }

    /// 确定 Focus 目标 tab
    /// 优先级：上次 focused 的 tab > 当前选中的 tab > 唯一的 tab
    /// 如果没有明确目标（多个 tab 但无法确定选哪个），返回 nil
    private func determineFocusTarget() -> UUID? {
        let tabs = viewModel.tabs

        // 没有 tab，无法 focus
        guard !tabs.isEmpty else { return nil }

        // 只有一个 tab，直接 focus 它
        if tabs.count == 1 {
            return tabs.first?.id
        }

        // 上次 focused 的 tab 仍然存在，返回它
        if let lastId = lastFocusedTabId, tabs.contains(where: { $0.id == lastId }) {
            return lastId
        }

        // 当前选中的 tab 存在，返回它
        if let currentId = viewModel.currentTabId, tabs.contains(where: { $0.id == currentId }) {
            return currentId
        }

        // 多个 tab 但没有明确目标，不自动 focus
        return nil
    }

    // MARK: - Find in Page

    /// Perform find in current web page
    private func performFind(text: String, forward: Bool) {
        guard !text.isEmpty,
              case .focus(let tabId) = viewModel.canvasMode,
              let holder = WebViewHolderManager.shared.existingHolder(for: tabId) else {
            return
        }

        // Use WKWebView's find API
        let config = WKFindConfiguration()
        config.backwards = !forward
        config.wraps = true

        holder.webView.find(text, configuration: config) { [self] result in
            Task { @MainActor in
                self.findResultCount = result.matchFound ? 1 : 0  // WKFindResult doesn't expose count
                // Note: WKWebView's find API doesn't expose the match count or current index
                // For a more complete implementation, JavaScript-based find would be needed
            }
        }
    }

    /// Clear find highlights
    private func clearFindHighlights() {
        guard case .focus(let tabId) = viewModel.canvasMode,
              let holder = WebViewHolderManager.shared.existingHolder(for: tabId) else {
            return
        }

        // Clear selection
        holder.webView.evaluateJavaScript("window.getSelection().removeAllRanges()")
    }

    // 判断 Tab 是否有需要 ViewBridge 渲染的内容
    private func hasRNContentForTab(_ tab: Tab) -> Bool {
        tab.systemTabType != nil ||
        tab.unifiedScript != nil ||
        tab.leftPanelScript != nil ||
        tab.guestScript != nil
    }

    // MARK: - Transform Calculations

    /// Overview 模式变换：缩放画布以适应视口，居中显示
    private func calculateOverviewTransform(
        canvasSize: CGSize,
        viewportSize: CGSize
    ) -> (scale: CGFloat, offset: CGPoint) {
        guard canvasSize.width > 0 && canvasSize.height > 0 else {
            return (1.0, .zero)
        }

        // 缩放：让 Canvas 适应视口（不放大，只缩小）
        let scaleX = viewportSize.width / canvasSize.width
        let scaleY = viewportSize.height / canvasSize.height
        let scale = max(min(scaleX, scaleY, 1.0), 0.001)

        // 居中
        let scaledWidth = canvasSize.width * scale
        let scaledHeight = canvasSize.height * scale
        let offsetX = (viewportSize.width - scaledWidth) / 2
        let offsetY = (viewportSize.height - scaledHeight) / 2

        guard offsetX.isFinite && offsetY.isFinite && scale.isFinite else {
            return (1.0, .zero)
        }

        return (scale, CGPoint(x: offsetX, y: offsetY))
    }

    /// Focus 模式变换：放大画布，让目标卡片的内容区域填满视口
    private func calculateFocusTransform(
        for tabId: UUID,
        gridResult: (positions: [CGPoint], cardWidth: CGFloat, cardHeight: CGFloat, canvasSize: CGSize),
        viewportSize: CGSize,
        titleBarRatio: CGFloat
    ) -> (scale: CGFloat, offset: CGPoint) {
        guard let position = viewModel.tabSlotPositions[tabId],
              position < gridResult.positions.count else {
            return (1.0, .zero)
        }

        let cardPosition = gridResult.positions[position]
        let cardFullRect = CGRect(
            x: cardPosition.x,
            y: cardPosition.y,
            width: gridResult.cardWidth,
            height: gridResult.cardHeight
        )

        // 卡片内容区域（去掉标题栏）
        // 使用传入的 titleBarRatio，已经包含了增强系数
        let titleBarHeight = cardFullRect.height * titleBarRatio
        let cardContentRect = CGRect(
            x: cardFullRect.origin.x,
            y: cardFullRect.origin.y + titleBarHeight,
            width: cardFullRect.width,
            height: cardFullRect.height - titleBarHeight
        )

        guard cardContentRect.width > 0 else {
            return (1.0, .zero)
        }

        // 缩放：让卡片内容区域宽度 = 视口宽度
        let scale = max(viewportSize.width / cardContentRect.width, 0.001)
        NSLog("📐 [FOCUS] viewportSize=%.1fx%.1f, cardRect=%.1fx%.1f, scale=%.3f",
              viewportSize.width, viewportSize.height, cardContentRect.width, cardContentRect.height, scale)

        // 偏移：对齐左上角
        let offsetX = -cardContentRect.origin.x * scale
        let offsetY = -cardContentRect.origin.y * scale

        guard offsetX.isFinite && offsetY.isFinite && scale.isFinite else {
            return (1.0, .zero)
        }

        return (scale, CGPoint(x: offsetX, y: offsetY))
    }

    // MARK: - Grid Layout Calculations

    /// 悬浮标题栏高度（Overview 模式下显示在卡片上方）
    private let floatingTitleBarHeight: CGFloat = 24

    /// 计算间距（卡片之间）和垂直边距
    /// - 水平间距 = windowWidth / (卡片数 × 3) × 2.5，下限 40px（2.5x 原值）
    /// - 垂直间距 = 基础间距 + 悬浮标题栏高度（为标题栏留出空间）
    /// - 垂直边距 = 基础间距 × 1.25 + 悬浮标题栏高度
    private func calculateSpacing(windowWidth: CGFloat, totalCards: Int) -> (hSpacing: CGFloat, vSpacing: CGFloat, verticalMargin: CGFloat) {
        let effectiveCards = max(totalCards, 4)
        // 基础间距用于垂直方向和边距计算
        let baseSpacing = max(16, windowWidth / (CGFloat(effectiveCards) * 7))
        // 水平间距增加到 2.5 倍
        let hSpacing = max(40, baseSpacing * 2.5)
        let vSpacing = baseSpacing + floatingTitleBarHeight  // 纵向间距包含标题栏高度
        let verticalMargin = baseSpacing * 1.25 + floatingTitleBarHeight  // 顶部边距也要留空间给第一行标题栏
        return (hSpacing, vSpacing, verticalMargin)
    }

    /// 网格布局计算
    /// - 初始容量 4（2×2），卡片数 ≤ 4 时强制 2×2 布局
    /// - 水平边距 = spacing × 1.25（通过卡片尺寸计算保证）
    private func gridLayout(
        for size: CGSize,
        cardAspectRatio: CGFloat,
        cardCount: Int
    ) -> (positions: [CGPoint], cardWidth: CGFloat, cardHeight: CGFloat, canvasSize: CGSize) {
        let totalCards = cardCount + 1  // 包含 [+] 按钮
        let minCardWidth: CGFloat = 120  // 防止窗口极窄时卡片宽度为 0 导致变换矩阵退化

        guard cardAspectRatio.isFinite && cardAspectRatio > 0 else {
            return gridLayout(for: size, cardAspectRatio: 16.0 / 9.0, cardCount: cardCount)
        }
        guard totalCards > 0 else {
            return ([], 0, 0, .zero)
        }

        let initialCapacity = 4
        let isForced2x2 = totalCards <= initialCapacity
        let (hSpacing, vSpacing, verticalMargin) = calculateSpacing(windowWidth: size.width, totalCards: max(totalCards, initialCapacity))

        // 计算最佳布局
        func calculateOptimalLayout() -> (columns: Int, rows: Int, cardWidth: CGFloat, cardHeight: CGFloat) {
            var bestColumns = 1
            var bestCardWidth: CGFloat = 0
            var bestCardHeight: CGFloat = 0

            let minColumns = isForced2x2 ? 2 : 1
            let maxColumns = isForced2x2 ? 2 : totalCards

            for columns in minColumns...maxColumns {
                let rows = isForced2x2 ? 2 : Int(ceil(Double(totalCards) / Double(columns)))

                // 水平约束：边距 = 1.25 × hSpacing
                // cardWidth = (width - (columns + 1.5) × hSpacing) / columns
                let cardWidthFromH = (size.width - CGFloat(columns + 1) * hSpacing - 0.5 * hSpacing) / CGFloat(columns)

                // 垂直约束（使用 vSpacing，包含悬浮标题栏空间）
                let availableHeight = size.height - verticalMargin * 2
                let cardHeightFromV = (availableHeight - CGFloat(rows - 1) * vSpacing) / CGFloat(rows)
                let cardWidthFromV = cardHeightFromV * cardAspectRatio

                let cardWidth = min(cardWidthFromH, cardWidthFromV)
                let cardHeight = cardWidth / cardAspectRatio

                if cardWidth > bestCardWidth {
                    bestColumns = columns
                    bestCardWidth = cardWidth
                    bestCardHeight = cardHeight
                }
            }

            let rows = isForced2x2 ? 2 : Int(ceil(Double(totalCards) / Double(bestColumns)))
            return (bestColumns, rows, bestCardWidth, bestCardHeight)
        }

        let layout = calculateOptimalLayout()
        let columns = layout.columns
        let rows = layout.rows
        // 窗口极窄时 cardWidth 可能趋近于 0，导致 scale 矩阵奇异；加下限兜底
        let cardWidth = max(layout.cardWidth, minCardWidth)
        let cardHeight = cardWidth / cardAspectRatio

        // 2×2 强制布局时预留 4 个位置
        let positionCount = isForced2x2 ? initialCapacity : totalCards

        // 水平边距：填满视口宽度，消除居中偏移
        let horizontalMargin = max((size.width - CGFloat(columns) * cardWidth - CGFloat(columns - 1) * hSpacing) / 2, 0)

        // 计算卡片位置（使用 vSpacing 作为行间距）
        var positions: [CGPoint] = []
        for index in 0..<positionCount {
            let col = index % columns
            let row = index / columns
            let x = horizontalMargin + CGFloat(col) * (cardWidth + hSpacing)
            let y = verticalMargin + CGFloat(row) * (cardHeight + vSpacing)
            positions.append(CGPoint(x: x, y: y))
        }

        // Canvas 宽度 = viewport 宽度
        let canvasSize = CGSize(
            width: size.width,
            height: verticalMargin * 2 + CGFloat(rows) * cardHeight + CGFloat(rows - 1) * vSpacing
        )

        return (positions, cardWidth, cardHeight, canvasSize)
    }

}

// MARK: - Canvas View

private struct TabGridView: View {
    let tabs: [Tab]
    let tabSlotPositions: [UUID: Int]
    let positions: [CGPoint]
    let cardWidth: CGFloat
    let cardHeight: CGFloat
    let titleBarRatio: CGFloat  // 标题栏高度占卡片高度的比例
    let toolbarHeight: CGFloat  // toolbar 实际高度
    let currentScale: CGFloat
    let mode: CanvasMode
    let showNewTabButton: Bool  // 是否显示 [+] 按钮（搜索过滤时隐藏）
    let viewportSize: CGSize    // 视口尺寸，用于 RN 渲染
    @ObservedObject var bookmarkVM: BookmarkViewModel
    @ObservedObject var historyVM: HistoryViewModel
    let onSelectTab: (UUID) -> Void
    let onSwapPositions: (UUID?, UUID?) -> Void
    let onNewTab: () -> Void
    let onCloseTab: (UUID) -> Void
    var onNavigate: ((UUID, URL) -> Void)?  // tabId, url

    @State private var draggingTabId: UUID?
    @State private var hoveringOverTabId: UUID?
    @State private var swappingTabId: UUID?  // 正在交换过程中的卡片（用于淡入动画）

    // 固定的槽位索引（拖动开始时记录，不随 tabs 交换而改变）
    @State private var dragSourceSlotIndexFixed: Int?  // 拖动源槽位 a
    @State private var dropTargetSlotIndexFixed: Int?  // 目标槽位 b

    // ViewBridge coordinators 管理器：确保在 mode 切换时不会被重建
    @StateObject private var coordinatorManager = ViewBridgeCoordinatorManager()

    // 构建显示项列表（按位置排序）
    // [+] 按钮紧随最后一个 tab（位置 = tabs.count）
    // 2×2 布局示例（1 tab）：Tab 在位置 0，[+] 在位置 1，位置 2-3 为空
    private var displayItems: [(position: Int, item: DisplayItem)] {
        var items: [(Int, DisplayItem)] = []

        // 添加所有 tabs
        for tab in tabs {
            if let position = tabSlotPositions[tab.id] {
                items.append((position, .tab(tab)))
            }
        }

        // 按位置排序
        let sorted = items.sorted { $0.0 < $1.0 }

        // [+] 按钮紧随最后一个 tab（仅在未搜索时显示）
        var result = sorted
        if showNewTabButton {
            result.append((tabs.count, .newTabButton))
        }

        return result
    }


    var body: some View {
        let _ = NSLog("🔄 [CANVAS-BODY] TabGridView.body evaluated: tabs=%d, mode=%@", tabs.count, String(describing: mode))
        ZStack(alignment: .topLeading) {
            // 画布背景
            Color.clear

            // 第一层：卡片层（可移动，带动画）
            ForEach(displayItems, id: \.item.id) { displayItem in
                let itemPosition = displayItem.position
                let position = positions[itemPosition]

                switch displayItem.item {
                case .tab(let tab):
                    let isBeingDragged = draggingTabId == tab.id
                    let isHovering = hoveringOverTabId == tab.id
                    let isSwapping = swappingTabId == tab.id

                    TabPanel(
                        tab: tab,
                        cardWidth: cardWidth,
                        cardHeight: cardHeight,
                        titleBarRatio: titleBarRatio,
                        toolbarHeight: toolbarHeight,
                        currentScale: currentScale,
                        isFocused: {
                            if case .focus(let tabId) = mode {
                                return tabId == tab.id
                            }
                            return false
                        }(),
                        isBeingDragged: isBeingDragged,
                        isHovering: isHovering,
                        isSwapping: isSwapping,
                        viewportSize: viewportSize,
                        viewBridgeCoordinator: coordinatorManager.coordinator(for: tab.id),
                        bookmarkVM: bookmarkVM,
                        historyVM: historyVM,
                        onSelect: {
                            onSelectTab(tab.id)
                        },
                        onDragStart: {
                            // 清除所有旧状态（防止残留）
                            swappingTabId = nil
                            dropTargetSlotIndexFixed = nil
                            hoveringOverTabId = nil

                            // 设置新的拖动状态
                            draggingTabId = tab.id
                            dragSourceSlotIndexFixed = tabSlotPositions[tab.id]
                            return NSItemProvider(object: tab.id.uuidString as NSString)
                        },
                        onClose: {
                            onCloseTab(tab.id)
                        },
                        onNavigate: { url in
                            onNavigate?(tab.id, url)
                        }
                    )
                    .zIndex(isBeingDragged ? 1000 : (draggingTabId != nil ? 100 : Double(itemPosition)))
                    .onDrop(of: [.text], delegate: TabSwapDropDelegate(
                        targetTabId: tab.id,
                        draggingTabId: $draggingTabId,
                        hoveringOverTabId: $hoveringOverTabId,
                        dropTargetSlotIndexFixed: $dropTargetSlotIndexFixed,
                        tabSlotPositions: tabSlotPositions,
                        onSwapPositions: { tabId1, tabId2 in
                            // 标记目标卡片为交换中（触发淡入动画）
                            // 注意：此时 hoveringOverTabId 还是 tabId2，所以 B 卡片还是半透明的
                            swappingTabId = tabId2

                            // 使用动画执行交换（B 从 b 移动到 a，300ms + 从 0.5 淡入到 1.0）
                            withAnimation(.spring(response: 0.35, dampingFraction: 1.0, blendDuration: 0)) {
                                onSwapPositions(tabId1, tabId2)
                                // 清除 hovering 状态，让 B 卡片开始淡入
                                hoveringOverTabId = nil
                            }

                            // 延迟清除状态
                            DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) {
                                draggingTabId = nil
                                swappingTabId = nil
                                dragSourceSlotIndexFixed = nil
                                dropTargetSlotIndexFixed = nil
                            }
                        }
                    ))
                    .position(
                        x: position.x + cardWidth / 2,
                        y: position.y + cardHeight / 2
                    )

                case .newTabButton:
                    NewTabPanel(
                        cardWidth: cardWidth,
                        cardHeight: cardHeight,
                        onTap: onNewTab
                    )
                    // 处理拖到 NewTab 卡片上的情况，清理状态
                    .onDrop(of: [.text], delegate: NoOpDropDelegate(
                        draggingTabId: $draggingTabId,
                        hoveringOverTabId: $hoveringOverTabId
                    ))
                    .position(
                        x: position.x + cardWidth / 2,
                        y: position.y + cardHeight / 2
                    )
                }
            }

            // 第二层：槽位边框层（最上层，不被卡片遮挡）
            // 禁用所有动画：槽位和边框不应该有任何动画效果
            ForEach(0..<positions.count, id: \.self) { slotIndex in
                SlotView(
                    cardWidth: cardWidth,
                    cardHeight: cardHeight,
                    isDragSource: dragSourceSlotIndexFixed == slotIndex,
                    isDropTarget: dropTargetSlotIndexFixed == slotIndex
                )
                .position(
                    x: positions[slotIndex].x + cardWidth / 2,
                    y: positions[slotIndex].y + cardHeight / 2
                )
                .zIndex(10000)  // 超高 zIndex，确保边框始终在最上层，不被任何卡片遮挡
                .animation(nil, value: dragSourceSlotIndexFixed)
                .animation(nil, value: dropTargetSlotIndexFixed)
                .allowsHitTesting(false)  // 边框不响应点击，让点击穿透到下层卡片
            }
        }
        // 全局拖放监听，处理拖到空白区域的情况
        .onDrop(of: [.text], delegate: CancelDragDropDelegate(
            draggingTabId: $draggingTabId,
            hoveringOverTabId: $hoveringOverTabId
        ))
    }
}

// MARK: - Slot View (槽位视图，显示拖拽时的边框)

private struct SlotView: View {
    let cardWidth: CGFloat
    let cardHeight: CGFloat
    let isDragSource: Bool  // 是否为拖动源槽位（显示虚线边框）
    let isDropTarget: Bool  // 是否为目标槽位（显示实线边框）

    var body: some View {
        Rectangle()
            .fill(Color.clear)
            .frame(width: cardWidth, height: cardHeight)
            .overlay(
                Group {
                    if isDragSource {
                        // 拖动源：蓝色虚线
                        RoundedRectangle(cornerRadius: 10)
                            .strokeBorder(
                                Color.blue.opacity(0.6),
                                style: StrokeStyle(lineWidth: 2, dash: [5, 5])
                            )
                    } else if isDropTarget {
                        // 目标：绿色实线
                        RoundedRectangle(cornerRadius: 10)
                            .strokeBorder(
                                Color.green.opacity(0.7),
                                style: StrokeStyle(lineWidth: 2)
                            )
                    }
                }
            )
            .transaction { tx in
                // 禁用所有隐式动画（替代已废弃的 .animation(nil)）
                tx.animation = nil
            }
    }
}

// 显示项类型
private enum DisplayItem {
    case tab(Tab)
    case newTabButton

    // 提供稳定的 ID，让 SwiftUI 能追踪视图移动而不是内容替换
    var id: String {
        switch self {
        case .tab(let tab):
            return tab.id.uuidString
        case .newTabButton:
            return "newTabButton"
        }
    }
}

// MARK: - Tab Swap Drop Delegate

private struct TabSwapDropDelegate: DropDelegate {
    let targetTabId: UUID?
    @Binding var draggingTabId: UUID?
    @Binding var hoveringOverTabId: UUID?
    @Binding var dropTargetSlotIndexFixed: Int?
    let tabSlotPositions: [UUID: Int]
    let onSwapPositions: (UUID?, UUID?) -> Void

    func validateDrop(info: DropInfo) -> Bool {
        // 始终返回 true 以确保 performDrop 被调用来清理状态
        // 实际的有效性检查在 performDrop 中进行
        return draggingTabId != nil
    }

    func performDrop(info: DropInfo) -> Bool {
        guard let draggingId = draggingTabId else {
            return false
        }

        // 拖到自己身上，不执行交换
        if let targetId = targetTabId, targetId == draggingId {
            clearAllStates()
            return false
        }

        // 执行交换（带动画）
        // 注意：onSwapPositions 回调中会处理状态清理和动画
        onSwapPositions(draggingId, targetTabId)

        // 不在这里清除 hoveringOverTabId，因为需要保持半透明状态用于淡入动画
        // hoveringOverTabId 会在 onSwapPositions 回调的动画块中清除

        return true
    }

    func dropEntered(info: DropInfo) {
        if let targetId = targetTabId,
           let draggingId = draggingTabId,
           targetId != draggingId {  // 忽略拖到自己身上
            hoveringOverTabId = targetId
            // 更新目标槽位索引（跟随 hover 目标变化）
            dropTargetSlotIndexFixed = tabSlotPositions[targetId]
        }
    }

    func dropExited(info: DropInfo) {
        if let targetId = targetTabId {
            if hoveringOverTabId == targetId {
                hoveringOverTabId = nil
                // 清除目标槽位标记
                dropTargetSlotIndexFixed = nil
            }
        }
    }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        // bug3 修复：返回 .move 操作，避免显示复制指针
        return DropProposal(operation: .move)
    }

    func dropSessionDidEnd(_ session: DropSession) {
        // 清理悬停状态（draggingTabId 由 onSwapPositions 回调延迟清除）
        hoveringOverTabId = nil
    }

    private func clearAllStates() {
        draggingTabId = nil
        hoveringOverTabId = nil
        dropTargetSlotIndexFixed = nil
    }
}

// MARK: - No-Op Drop Delegate (bug3 修复：空白区域不响应拖放，保持原始鼠标样式)

private struct NoOpDropDelegate: DropDelegate {
    @Binding var draggingTabId: UUID?
    @Binding var hoveringOverTabId: UUID?

    func validateDrop(info: DropInfo) -> Bool {
        return draggingTabId != nil
    }

    func performDrop(info: DropInfo) -> Bool {
        draggingTabId = nil
        hoveringOverTabId = nil
        return false
    }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        return DropProposal(operation: .move)
    }

    func dropSessionDidEnd(_ session: DropSession) {
        draggingTabId = nil
        hoveringOverTabId = nil
    }
}

// MARK: - Cancel Drag Drop Delegate (空白区域拖放处理)

private struct CancelDragDropDelegate: DropDelegate {
    @Binding var draggingTabId: UUID?
    @Binding var hoveringOverTabId: UUID?

    func validateDrop(info: DropInfo) -> Bool {
        return true
    }

    func performDrop(info: DropInfo) -> Bool {
        draggingTabId = nil
        hoveringOverTabId = nil
        return false
    }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        return DropProposal(operation: .move)
    }

    func dropSessionDidEnd(_ session: DropSession) {
        draggingTabId = nil
        hoveringOverTabId = nil
    }

    // 注意：不要在 dropExited 中清理状态！
    // 因为当拖动从父视图进入子视图时，父视图的 dropExited 会被调用
    // 这会导致状态被错误清空
}

// MARK: - Address Bar Left Icon State

/// State for address bar left icon
private enum AddressBarLeftIconState {
    case none      // No icon (web tab not loading)
    case loading   // Loading spinner (web tab loading)
    case menu      // Menu icon (RN app tab)
}

/// Left icon view for address bar
private struct AddressBarLeftIcon: View {
    let state: AddressBarLeftIconState
    let action: () -> Void

    @State private var rotation: Double = 0

    var body: some View {
        switch state {
        case .none:
            // Empty space to maintain layout
            Color.clear.frame(width: 16, height: 16)
        case .loading:
            // Spinning progress ring
            Circle()
                .trim(from: 0, to: 0.7)
                .stroke(
                    Color.white.opacity(0.7),
                    style: StrokeStyle(lineWidth: 1.2, lineCap: .round)
                )
                .frame(width: 12, height: 12)
                .rotationEffect(.degrees(rotation))
                .onAppear {
                    withAnimation(.linear(duration: 1).repeatForever(autoreverses: false)) {
                        rotation = 360
                    }
                }
        case .menu:
            // Menu button for RN apps
            Button(action: action) {
                Image(systemName: "list.bullet")
                    .foregroundColor(.white.opacity(0.7))
            }
            .buttonStyle(.borderless)
            .help("切换左侧面板")
        }
    }
}

// MARK: - Address Bar Container

private struct AddressBarContainer<BookmarkContent: View>: View {
    @Binding var titleText: String
    var urlText: String  // URL for edit mode
    @Binding var inputText: String
    @Binding var isEditing: Bool
    @Binding var isFirstResponder: Bool
    var isBookmarked: Bool
    var leftIconState: AddressBarLeftIconState
    var toggleLeftPanel: () -> Void
    @Binding var showingBookmarkPopover: Bool
    @Binding var shouldAutoFocus: Bool  // 自动聚焦标志
    var refreshAction: () -> Void
    var onNavigate: ((String) -> Void)?  // URL 导航回调
    @ViewBuilder var bookmarkPopoverContent: () -> BookmarkContent

    private let placeholder = "Enter URL (http/https) or app id"
    @State private var focusRingScale: CGFloat = 1.0
    @State private var focusRingOpacity: Double = 0.0

    var body: some View {
        Group {
            if isEditing {
                // 编辑状态：放大镜 + 输入框 + 右侧按钮
                HStack(spacing: 10) {
                    Image(systemName: "magnifyingglass")
                        .foregroundStyle(Color.white.opacity(0.7))
                        .frame(width: 16, height: 16)

                    FocusableTextField(
                        text: $inputText,
                        placeholder: placeholder,
                        isFirstResponder: $isFirstResponder,
                        onSubmit: { text in
                            onNavigate?(text)
                        }
                    )

                    // 保留右侧按钮
                    Button(action: { showingBookmarkPopover.toggle() }) {
                        Image(systemName: isBookmarked ? "star.fill" : "star")
                            .foregroundColor(isBookmarked ? .yellow : .white.opacity(0.7))
                    }
                    .buttonStyle(.borderless)
                    .help(isBookmarked ? "Edit Bookmark" : "Add Bookmark")
                    .popover(isPresented: $showingBookmarkPopover, arrowEdge: .bottom) {
                        bookmarkPopoverContent()
                    }

                    Button(action: refreshAction) {
                        Image(systemName: "arrow.clockwise")
                            .foregroundColor(.white.opacity(0.7))
                    }
                    .buttonStyle(.borderless)
                    .help("刷新")
                }
            } else {
                // 非编辑状态：左图标 + 中间标题 + 右图标
                HStack(spacing: 10) {
                    AddressBarLeftIcon(state: leftIconState, action: toggleLeftPanel)

                    Text(titleText)
                        .foregroundColor(.white)
                        .multilineTextAlignment(.center)
                        .frame(maxWidth: .infinity)
                        .contentShape(Rectangle())
                        .onTapGesture {
                            if !isEditing {
                                // 设置 isEditing，onChange 会自动处理 URL 复制和焦点
                                isEditing = true
                            }
                        }

                    // Star button for bookmarking
                    Button(action: { showingBookmarkPopover.toggle() }) {
                        Image(systemName: isBookmarked ? "star.fill" : "star")
                            .foregroundColor(isBookmarked ? .yellow : .white.opacity(0.7))
                    }
                    .buttonStyle(.borderless)
                    .help(isBookmarked ? "Edit Bookmark" : "Add Bookmark")
                    .popover(isPresented: $showingBookmarkPopover, arrowEdge: .bottom) {
                        bookmarkPopoverContent()
                    }

                    Button(action: refreshAction) {
                        Image(systemName: "arrow.clockwise")
                    }
                    .buttonStyle(.borderless)
                    .help("刷新")
                }
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 4)
        .frame(maxWidth: .infinity, minHeight: 28)
        .overlay(
            Capsule()
                .stroke(
                    Color(red: 0.18, green: 0.56, blue: 0.96)
                        .opacity(focusRingOpacity),
                    lineWidth: 2.8
                )
                .scaleEffect(focusRingScale)
                .padding(.vertical, -4)
        )
        .onChange(of: isFirstResponder) { _, newValue in
            if !newValue && isEditing {
                // 失焦时退出编辑模式
                isEditing = false
            }
        }
        .onChange(of: isEditing) { _, newValue in
            if newValue {
                // 进入编辑模式时，复制 URL 并延迟设置焦点（等待 TextField 渲染）
                inputText = urlText
                // 从略大的、透明的蓝圈收缩到正常大小并渐显
                focusRingScale = 1.08
                focusRingOpacity = 0.0
                withAnimation(.spring(response: 0.25, dampingFraction: 0.7, blendDuration: 0.1)) {
                    focusRingScale = 1.0
                    focusRingOpacity = 1.0
                }
                // 延迟一帧，确保 FocusableTextField 已渲染
                DispatchQueue.main.async {
                    isFirstResponder = true
                }
            } else {
                withAnimation(.easeOut(duration: 0.15)) {
                    focusRingOpacity = 0.0
                }
            }
        }
        .onAppear {
            // 地址栏渲染完成后，检查是否需要自动聚焦
            if shouldAutoFocus {
                shouldAutoFocus = false
                isEditing = true
            }
        }
    }

    private func endEditing() {
        isEditing = false
        isFirstResponder = false
    }
}

// MARK: - Focusable TextField

private struct FocusableTextField: NSViewRepresentable {
    @Binding var text: String
    var placeholder: String
    @Binding var isFirstResponder: Bool
    var onSubmit: ((String) -> Void)?  // 回车键提交回调

    func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }

    func makeNSView(context: Context) -> NSTextField {
        let field = NSTextField()
        field.isBordered = false
        field.isBezeled = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.font = NSFont.systemFont(ofSize: NSFont.systemFontSize)
        field.placeholderString = placeholder
        field.delegate = context.coordinator
        field.usesSingleLineMode = true
        field.maximumNumberOfLines = 1
        field.cell?.wraps = false
        field.cell?.isScrollable = true
        // 设置 action 以接收回车键
        field.target = context.coordinator
        field.action = #selector(Coordinator.handleAction(_:))
        return field
    }

    func updateNSView(_ nsView: NSTextField, context: Context) {
        if nsView.stringValue != text {
            nsView.stringValue = text
        }
        nsView.placeholderString = placeholder

        if isFirstResponder,
           let window = nsView.window,
           window.firstResponder != nsView.currentEditor() {
            window.makeFirstResponder(nsView)
            // 选中所有文本
            DispatchQueue.main.async {
                if let editor = nsView.currentEditor() {
                    editor.selectAll(nil)
                }
            }
        }
    }

    final class Coordinator: NSObject, NSTextFieldDelegate {
        var parent: FocusableTextField

        init(_ parent: FocusableTextField) {
            self.parent = parent
        }

        func controlTextDidChange(_ obj: Notification) {
            guard let field = obj.object as? NSTextField else { return }
            // 过滤掉换行符，保持为单行内容
            let raw = field.stringValue
            let cleaned = raw
                .components(separatedBy: .newlines)
                .joined(separator: " ")
            if cleaned != raw {
                field.stringValue = cleaned
            }
            parent.text = cleaned
        }

        func controlTextDidEndEditing(_ notification: Notification) {
            parent.isFirstResponder = false
        }

        @objc func handleAction(_ sender: NSTextField) {
            let text = sender.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
            if !text.isEmpty {
                parent.onSubmit?(text)
            }
            parent.isFirstResponder = false
        }
    }
}

// MARK: - Tab Card View

private struct TabPanel: View {
    let tab: Tab
    let cardWidth: CGFloat
    let cardHeight: CGFloat
    let titleBarRatio: CGFloat  // 精确的标题栏高度比例
    let toolbarHeight: CGFloat  // toolbar 实际高度，用作缩放基准
    let currentScale: CGFloat
    let isFocused: Bool
    let isBeingDragged: Bool
    let isHovering: Bool
    let isSwapping: Bool  // 正在交换过程中（用于淡入动画）
    let viewportSize: CGSize  // 视口尺寸，用于 RN 渲染
    let viewBridgeCoordinator: ViewBridgeCoordinator  // 从外部传入，确保稳定
    @ObservedObject var bookmarkVM: BookmarkViewModel
    @ObservedObject var historyVM: HistoryViewModel
    let onSelect: () -> Void
    let onDragStart: () -> NSItemProvider
    let onClose: () -> Void
    var onNavigate: ((URL) -> Void)?

    // 悬浮标题栏高度（固定值，Overview 模式下显示在卡片上方）
    private let floatingTitleBarHeight: CGFloat = 24

    var body: some View {
        let _ = NSLog("🔄 [TABCARD-BODY] TabPanel.body: tabId=%@, isFocused=%d", tab.id.uuidString, isFocused ? 1 : 0)

        // 外层尺寸：始终使用 cardWidth/cardHeight
        // Canvas 变换负责在 Focus 模式下缩放到 viewportSize
        // 如果这里使用 viewportSize，会与 canvas scale 叠加导致过度放大
        let panelWidth = cardWidth
        let panelHeight = cardHeight

        // 使用 ZStack 允许标题栏悬浮在卡片上方
        ZStack(alignment: .top) {
            // 卡片主体（纯内容）
            // 圆角仅 Overview 模式显示，Focus 模式不要圆角
            contentView
                .frame(width: panelWidth, height: panelHeight)
                .cornerRadius(isFocused ? 0 : 10)
                .shadow(color: isFocused ? .clear : Color.black.opacity(0.3), radius: 5, x: 0, y: 2)

            // 悬浮标题栏 - 仅 Overview 模式显示（Focus 模式隐藏）
            if !isFocused {
                floatingTitleBarView
                    .frame(width: cardWidth, height: floatingTitleBarHeight)
                    .offset(y: -floatingTitleBarHeight - 4)  // 悬浮在卡片上方，留 4px 间隙
            }
        }
        // 被拖动的卡片：隐藏（由拖拽预览显示）
        .opacity(isBeingDragged ? 0.001 : 1.0)
        // hover 时半透明，交换过程中淡入到完全不透明
        .opacity({
            if isSwapping {
                return 1.0  // 交换中：淡入到完全不透明
            } else if isHovering && !isBeingDragged && !isFocused {
                return 0.5  // hover 时半透明
            } else {
                return 1.0  // 默认完全不透明
            }
        }())
        // 整个卡片作为拖动源
        .onDrag {
            if isFocused {
                return NSItemProvider()
            }
            return onDragStart()
        } preview: {
            // 自定义拖动预览：整个卡片内容
            dragPreviewView
        }
    }

    // 悬浮标题栏视图（简化版，仅包含标题和关闭按钮）
    private var floatingTitleBarView: some View {
        HStack(spacing: 8) {
            // 关闭按钮
            Button(action: onClose) {
                Image(systemName: "xmark.circle.fill")
                    .font(.system(size: 14))
                    .foregroundColor(.white.opacity(0.6))
            }
            .buttonStyle(.plain)

            // 标题
            Text(tab.title.isEmpty ? "New Tab" : tab.title)
                .font(.system(size: 12, weight: .medium))
                .foregroundColor(.white.opacity(0.9))
                .lineLimit(1)

            Spacer()
        }
        .padding(.horizontal, 8)
    }

    // 拖动预览视图 - 显示卡片 + 悬浮标题栏
    private var dragPreviewView: some View {
        ZStack(alignment: .top) {
            // 卡片主体
            ZStack {
                Color(hslString: tab.backgroundColor)
                Image(systemName: "globe")
                    .font(.system(size: 32))
                    .foregroundColor(.white.opacity(0.3))
            }
            .frame(width: cardWidth, height: cardHeight)
            .cornerRadius(10)
            .overlay(
                RoundedRectangle(cornerRadius: 10)
                    .stroke(Color.white.opacity(0.3), lineWidth: 1)
            )
            .shadow(color: Color.black.opacity(0.5), radius: 10, x: 0, y: 5)

            // 悬浮标题栏
            HStack(spacing: 8) {
                Image(systemName: "xmark.circle.fill")
                    .font(.system(size: 14))
                    .foregroundColor(.white.opacity(0.6))

                Text(tab.title.isEmpty ? "New Tab" : tab.title)
                    .font(.system(size: 12, weight: .medium))
                    .foregroundColor(.white.opacity(0.9))
                    .lineLimit(1)

                Spacer()
            }
            .padding(.horizontal, 8)
            .frame(width: cardWidth, height: floatingTitleBarHeight)
            .offset(y: -floatingTitleBarHeight - 4)
        }
    }

    // 卡片内容区域尺寸（画布坐标系中的尺寸）
    private var cardContentSize: CGSize {
        CGSize(width: cardWidth, height: cardHeight)
    }

    // 内容区域视图 - 多态容器
    // 根据 Tab.contentType 决定使用 ViewBridge (RN) 还是 WKWebView (Web)
    private var contentView: some View {
        // 始终使用 cardSize - Canvas 变换负责缩放
        // 这确保内容尺寸与 canvas 变换计算一致，避免过度放大
        let panelSize = CGSize(width: max(cardWidth, 1), height: max(cardHeight, 1))

        let _ = NSLog("📐 TabPanel.contentView: type=%@, isFocused=%d, panelSize=%.0fx%.0f",
                      String(describing: tab.contentType), isFocused ? 1 : 0, cardWidth, cardHeight)

        return ZStack {
            // 底层背景 - 使用 Tab 的随机背景颜色
            Color(hslString: tab.backgroundColor)

            // 多态渲染：根据 contentType 选择渲染引擎
            switch tab.contentType {
            case .reactNative(let fingerprint):
                // React Native 应用 - 使用 ViewBridge XPC
                // .id(tab.id) 确保 SwiftUI 保持 View identity，防止 @State 被重置
                ViewBridgeMultiProcessTabView(
                    tab: tab,
                    fingerprint: fingerprint,
                    containerSize: panelSize,
                    coordinator: viewBridgeCoordinator,
                    isFocused: isFocused
                )
                .id(tab.id)

            case .web(let url):
                // Web 内容 - 使用 WKWebView
                // Focus 模式由 FocusedWebOverlay 独立渲染（canvas 变换外）
                WebViewMultiProcessTabView(
                    tab: tab,
                    url: url,
                    containerSize: panelSize,
                    isFocused: isFocused
                )

            case .empty:
                // Empty placeholder - new tabs now use system startPage
                EmptyView()
            }
        }
        .frame(maxHeight: .infinity)
        .contentShape(Rectangle())
        .onTapGesture {
            onSelect()
        }
    }
}

// MARK: - Close Button View

private struct CloseButtonView: View {
    let iconSize: CGFloat
    let containerWidth: CGFloat
    let onClose: () -> Void

    @State private var isHovering = false

    var body: some View {
        Button(action: {
            onClose()
        }) {
            Image(systemName: "xmark")
                .font(.system(size: iconSize, weight: .semibold))
                .foregroundColor(isHovering ? .white : .white.opacity(0.6))
                .frame(width: iconSize * 1.6, height: iconSize * 1.6)
                .background(
                    Circle()
                        .fill(isHovering ? Color.white.opacity(0.15) : Color.clear)
                )
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .frame(width: containerWidth)
        .onHover { hovering in
            isHovering = hovering
        }
    }
}

// MARK: - New Tab Card View

private struct NewTabPanel: View {
    let cardWidth: CGFloat
    let cardHeight: CGFloat
    var onTap: (() -> Void)? = nil

    var body: some View {
        ZStack {
            Rectangle()
                .fill(Color(nsColor: NSColor(white: 0.15, alpha: 1.0)))

            Image(systemName: "plus")
                .font(.system(size: 48))
                .foregroundColor(.white.opacity(0.5))
        }
        .frame(width: cardWidth, height: cardHeight)
        .cornerRadius(10)
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(Color.white.opacity(0.15), lineWidth: 1)
        )
        .shadow(color: Color.black.opacity(0.3), radius: 5, x: 0, y: 2)
        .contentShape(Rectangle())
        .onTapGesture {
            onTap?()
        }
    }
}

// MARK: - Find in Page Bar

private struct FindInPageBar: View {
    @Binding var findText: String
    let resultCount: Int
    let currentIndex: Int
    var isFocused: FocusState<Bool>.Binding
    let onFind: (String) -> Void
    let onFindNext: () -> Void
    let onFindPrevious: () -> Void
    let onClose: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            // Search field
            HStack(spacing: 6) {
                Image(systemName: "magnifyingglass")
                    .font(.system(size: 12))
                    .foregroundColor(.secondary)

                TextField("Find in page", text: $findText)
                    .textFieldStyle(.plain)
                    .font(.system(size: 13))
                    .focused(isFocused)
                    .onSubmit {
                        onFind(findText)
                    }
                    .onChange(of: findText) { _, newValue in
                        if !newValue.isEmpty {
                            onFind(newValue)
                        }
                    }

                if !findText.isEmpty {
                    Button(action: {
                        findText = ""
                    }) {
                        Image(systemName: "xmark.circle.fill")
                            .font(.system(size: 12))
                            .foregroundColor(.secondary)
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
            .background(Color(nsColor: .controlBackgroundColor))
            .cornerRadius(6)
            .frame(width: 220)

            // Navigation buttons
            Button(action: onFindPrevious) {
                Image(systemName: "chevron.up")
                    .font(.system(size: 12, weight: .medium))
            }
            .buttonStyle(.borderless)
            .disabled(findText.isEmpty)

            Button(action: onFindNext) {
                Image(systemName: "chevron.down")
                    .font(.system(size: 12, weight: .medium))
            }
            .buttonStyle(.borderless)
            .disabled(findText.isEmpty)

            Spacer()

            // Close button
            Button(action: onClose) {
                Image(systemName: "xmark")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundColor(.secondary)
            }
            .buttonStyle(.borderless)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(.ultraThinMaterial)
        .cornerRadius(8)
        .shadow(color: .black.opacity(0.15), radius: 4, y: 2)
        .padding(.horizontal, 16)
        .padding(.top, 8)
        .frame(maxWidth: .infinity, alignment: .top)
    }
}

// MARK: - Share Button (System Picker)

/// Share toolbar button using system NSSharingServicePicker
struct ShareToolbarButton: View {
    let url: URL?
    @State private var triggerShare = false

    var body: some View {
        Button(action: { triggerShare.toggle() }) {
            Image(systemName: "square.and.arrow.up")
        }
        .help("分享")
        .background(ShareAnchor(url: url, trigger: $triggerShare))
    }
}

/// Hidden anchor that shows share picker when triggered
private struct ShareAnchor: NSViewRepresentable {
    let url: URL?
    @Binding var trigger: Bool

    func makeNSView(context: Context) -> NSView {
        NSView()
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        // When trigger changes, show the picker
        if trigger, let url = url {
            DispatchQueue.main.async {
                self.trigger = false
                let picker = NSSharingServicePicker(items: [url])
                picker.show(relativeTo: nsView.bounds, of: nsView, preferredEdge: .minY)
            }
        }
    }
}

// MARK: - QR Code Button

/// QR Code toolbar button with popover
struct QRCodeToolbarButton: View {
    let url: URL?

    @State private var showingPopover = false
    @State private var qrImage: NSImage?

    var body: some View {
        Button(action: { showingPopover.toggle() }) {
            Image("qr-icon")
                .renderingMode(.template)
                .resizable()
                .aspectRatio(contentMode: .fit)
                .frame(width: 16, height: 16)
                .offset(y: 1)
        }
        .help("QR Code")
        .popover(isPresented: $showingPopover, arrowEdge: .bottom) {
            if let url = url {
                VStack(spacing: 12) {
                    if let qrImage = qrImage {
                        Image(nsImage: qrImage)
                            .interpolation(.none)
                            .resizable()
                            .frame(width: 160, height: 160)
                            .background(Color.white)
                            .cornerRadius(8)
                    }

                    Text(url.host ?? url.absoluteString)
                        .font(.caption)
                        .foregroundColor(.secondary)
                        .lineLimit(1)

                    Button("Copy QR Image") {
                        if let qrImage = qrImage {
                            NSPasteboard.general.clearContents()
                            NSPasteboard.general.writeObjects([qrImage])
                        }
                        showingPopover = false
                    }
                    .buttonStyle(.borderless)
                }
                .padding()
                .onAppear {
                    generateQRCode(for: url)
                }
            }
        }
    }

    private func generateQRCode(for url: URL) {
        guard let filter = CIFilter(name: "CIQRCodeGenerator") else { return }
        let data = url.absoluteString.data(using: .utf8)
        filter.setValue(data, forKey: "inputMessage")
        filter.setValue("M", forKey: "inputCorrectionLevel")

        guard let ciImage = filter.outputImage else { return }

        let scale = 10.0
        let transform = CGAffineTransform(scaleX: scale, y: scale)
        let scaledImage = ciImage.transformed(by: transform)

        let rep = NSCIImageRep(ciImage: scaledImage)
        let nsImage = NSImage(size: rep.size)
        nsImage.addRepresentation(rep)
        qrImage = nsImage
    }
}

#Preview {
    WorkspaceMainView(viewModel: BrowserViewModel(), historyVM: HistoryViewModel(), bookmarkVM: BookmarkViewModel())
}
