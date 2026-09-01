import SwiftUI
import WebKit

private let logger = AppLogger.ui
import AppKit
import UniformTypeIdentifiers

// MARK: - Notification Names

extension Notification.Name {
    static let toggleFocusOverviewRequested = Notification.Name("toggleFocusOverviewRequested")
}

// MARK: - Main View

/// 大画布架构 - 统一的 Actro 视图
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
    var useNativeToolbar: Bool = false  // 使用原生 NSToolbar 时隐藏 SwiftUI toolbar

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
    @State private var magnifyEventMonitor: Any?
    @State private var cumulativeMagnification: CGFloat = 0

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
    // Use shared singleton to ensure setOverviewMode works correctly
    private var viewBridgeCoordinatorManager: ViewBridgeCoordinatorManager { ViewBridgeCoordinatorManager.shared }

    // 模式切换过渡控制器
    @StateObject private var modeTransitionController = ModeTransitionController()

    // 🔑 拖动状态（从 TabGridView 提升，让 UnifiedReactNativeLayer 也能访问）
    @State private var draggingTabId: UUID?
    @State private var dropTargetTabId: UUID?  // 当前 drop target 的 tab ID

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
                  viewportSize.width, viewportSize.height, String(describing: viewModel.actroMode))
        }()

        // Safe viewport size (avoid zero/small dimensions that cause layout issues)
        // 使用更大的最小值，避免极端宽高比导致 cardHeight 异常大
        // 当 viewportSize.width=0 时，cardAspectRatio=1/100=0.01，cardHeight=120/0.01=12000
        // 这会导致 ViewBridge 的 NSServiceViewControllerWindow 布局循环错误
        let safeViewportSize = CGSize(
            width: max(viewportSize.width, 100),
            height: max(viewportSize.height, 100)
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
            let actroSize = gridResult.actroSize

            // 计算变换
            // 使用 safeViewportSize 避免 viewportSize=0 导致 scale=0.001
            // scale=0.001 会导致 ViewBridge 窗口计算出极端尺寸
            let transform: (scale: CGFloat, offset: CGPoint) = {
                switch viewModel.actroMode {
                case .overview:
                    return calculateOverviewTransform(
                        actroSize: actroSize,
                        viewportSize: safeViewportSize
                    )
                case .focus(let tabId):
                    return calculateFocusTransform(
                        for: tabId,
                        gridResult: gridResult,
                        viewportSize: safeViewportSize,
                        titleBarRatio: titleBarRatio
                    )
                }
            }()

            ZStack(alignment: .topTrailing) {
                // Safari 风格：深灰色内容区域背景（不延伸到 toolbar）
                Color(nsColor: NSColor(white: 0.12, alpha: 1.0))

                // Actro 视图 - 包含所有 Tab 卡片
                // ViewBridge 始终在 Actro 内部，通过快照实现平滑动画
                TabGridView(
                    tabs: filteredTabs,
                    tabSlotPositions: filteredTabPositions,
                    positions: gridResult.positions,
                    cardWidth: gridResult.cardWidth,
                    cardHeight: gridResult.cardHeight,
                    titleBarRatio: titleBarRatio,
                    toolbarHeight: toolbarHeight,
                    currentScale: transform.scale,
                    mode: viewModel.actroMode,
                    showNewTabButton: searchTabsText.isEmpty,
                    viewportSize: safeViewportSize,
                    viewBridgeCoordinatorManager: viewBridgeCoordinatorManager,
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
                    },
                    draggingTabId: $draggingTabId,
                    dropTargetTabId: $dropTargetTabId
                )
                .frame(width: actroSize.width, height: actroSize.height)
                // 防止 scale 为 0 导致奇异矩阵
                .scaleEffect(max(transform.scale, 0.001), anchor: .topLeading)
                .offset(x: transform.offset.x, y: transform.offset.y)
                // 🔑 禁用 SwiftUI 默认动画，由 ModeTransitionController 手动控制
                .animation(nil, value: viewModel.actroMode)
                .contentShape(Rectangle())
                // 地址栏退出焦点已由全局鼠标事件监听处理（mouseEventMonitor）

                // Focus 模式下的 Native 内容层（actro 变换之外）
                // Native SwiftUI views need to render at viewportSize in Focus mode,
                // otherwise they would be scaled up (blurry) by actro transform
                if case .focus(let focusedTabId) = viewModel.actroMode,
                   let focusedTab = viewModel.tabs.first(where: { $0.id == focusedTabId }),
                   case .native(let nativeType) = focusedTab.contentType {
                    switch nativeType {
                    case .startPage:
                        StartPageView(
                            bookmarks: bookmarkVM.bookmarks,
                            recentHistory: historyVM.recentItems,
                            containerSize: safeViewportSize,
                            isCompactMode: false,
                            onNavigate: { url in
                                viewModel.navigateCurrentTab(to: url.absoluteString)
                            }
                        )
                        .frame(width: safeViewportSize.width, height: safeViewportSize.height)
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                    }
                }

                // Focus 模式下的 Web 内容层（actro 变换之外）
                // scaleEffect 对 NSViewRepresentable 不生效，所以 Web 内容需要独立渲染
                if case .focus(let focusedTabId) = viewModel.actroMode,
                   let focusedTab = viewModel.tabs.first(where: { $0.id == focusedTabId }),
                   case .web(let url) = focusedTab.contentType {
                    ZStack(alignment: .top) {
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
                            onURLChanged: { newURL in
                                // Update tab URL when page uses History.pushState/replaceState
                                if let newURL = newURL {
                                    NSLog("🔗 WorkspaceMainView: Updating tab \(focusedTabId) URL to: \(newURL.absoluteString)")
                                    viewModel.updateTabURL(for: focusedTabId, url: newURL)
                                }
                            },
                            onLoadingStateChanged: { loading in
                                isWebLoading = loading
                                viewModel.isCurrentTabLoading = loading
                            },
                            onOpenNewWindow: { newURL in
                                // Open target="_blank" links in a new tab (don't focus address bar)
                                let newTab = viewModel.addNewTab(focusAddressBar: false)
                                viewModel.navigateCurrentTab(to: newURL.absoluteString)
                                // Enter focus mode for the new tab
                                enterFocus(tabId: newTab.id)
                            },
                            onThemeColorChanged: { color in
                                viewModel.updateTabBackgroundColor(for: focusedTabId, color: color)
                            },
                            onProgressChanged: { progress in
                                viewModel.loadingProgress = progress
                            }
                        )
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)

                        // Safari-style loading progress bar
                        if viewModel.isCurrentTabLoading {
                            GeometryReader { geo in
                                Rectangle()
                                    .fill(Color.accentColor)
                                    .frame(width: geo.size.width * viewModel.loadingProgress, height: 2)
                                    .animation(.linear(duration: 0.2), value: viewModel.loadingProgress)
                            }
                            .frame(height: 2)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .transition(.opacity)
                        }

                        SavePasswordBannerView()
                            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topTrailing)
                            .padding(12)
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                }

                // 🔑 Live 层：统一的 ViewBridge 内容层（actro 变换之外）
                // 始终渲染，不论 Focus/Overview 模式，避免模式切换时连接断开
                // 注意：Live 层在 actro 外部，需要将 actro 坐标转换为视口坐标
                // 🔑 使用固定 frame 的 ZStack 限制坐标系，否则父 ZStack 的 actroSize 会导致 .position() 错误
                ZStack(alignment: .topLeading) {
                    ForEach(viewModel.tabs.filter { tab in
                        if case .reactNative = tab.contentType { return true }
                        return false
                    }) { tab in
                        if case .reactNative(let fingerprint) = tab.contentType {
                            let isFocused = {
                                if case .focus(let focusedId) = viewModel.actroMode {
                                    return focusedId == tab.id
                                }
                                return false
                            }()

                            let isOverviewMode = {
                                if case .overview = viewModel.actroMode {
                                    return true
                                }
                                return false
                            }()

                            // 计算卡片在 actro 坐标系中的位置和尺寸
                            let slotPosition = filteredTabPositions[tab.id] ?? 0
                            let actroCardTopLeft = slotPosition < gridResult.positions.count
                                ? gridResult.positions[slotPosition]
                                : .zero
                            let actroCardSize = CGSize(
                                width: gridResult.cardWidth,
                                height: gridResult.cardHeight
                            )

                            // 🔑 将 actro 坐标转换为视口坐标
                            // Live 层在 actro 外部，需要手动应用 actro transform
                            let viewportCardPosition = CGPoint(
                                x: actroCardTopLeft.x * transform.scale + transform.offset.x,
                                y: actroCardTopLeft.y * transform.scale + transform.offset.y
                            )
                            let viewportCardSize = CGSize(
                                width: actroCardSize.width * transform.scale,
                                height: actroCardSize.height * transform.scale
                            )

                            UnifiedReactNativeLayer(
                                tab: tab,
                                fingerprint: fingerprint,
                                viewportSize: safeViewportSize,
                                cardSize: viewportCardSize,
                                cardPosition: viewportCardPosition,
                                isFocused: isFocused,
                                isOverviewMode: isOverviewMode,
                                isTransitioning: modeTransitionController.phase.isTransitioning,
                                isDragSource: draggingTabId == tab.id,
                                isDropTarget: dropTargetTabId == tab.id,
                                coordinator: viewBridgeCoordinatorManager.coordinator(for: tab.id),
                                bookmarks: bookmarksForRN,
                                recentHistory: recentHistoryForRN
                            )
                        }
                    }
                }
                .frame(width: safeViewportSize.width, height: safeViewportSize.height)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                .clipped()

                // Search Tabs 搜索框 - 仅在 Overview 模式显示
                // 放在 AnimationLayer 之前，让动画层能遮盖它
                if case .overview = viewModel.actroMode {
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

                // 🔑 Animation 层：VirtualGrid 方案
                // 与真实 Grid 布局完全一致的虚拟网格，使用 Tab 缩略图
                // 动画时覆盖真实 Grid 和 Search Tabs，完成后隐藏
                // 注意：实际变换由 ModeTransitionController 通过 CVDisplayLink 驱动
                // 注意：actroSize 必须与 TabGridView 的 frame 一致（gridResult.actroSize）
                AnimationLayerRepresentable(
                    controller: modeTransitionController,
                    actroSize: actroSize,  // 使用 gridResult.actroSize，与 TabGridView 一致
                    cardSize: CGSize(width: gridResult.cardWidth, height: gridResult.cardHeight),
                    cards: buildVirtualGridCards(
                        tabs: filteredTabs,
                        positions: gridResult.positions,
                        tabSlotPositions: filteredTabPositions,
                        cardWidth: gridResult.cardWidth,
                        cardHeight: gridResult.cardHeight
                    ),
                    isVisible: modeTransitionController.phase.isTransitioning
                )
                .frame(width: safeViewportSize.width, height: safeViewportSize.height)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                .allowsHitTesting(false)
                // 🔑 确保动画层在所有其他内容之上（包括 Live 层的 NSView）
                .zIndex(1000)

                // Glass View 覆盖层处理拖拽（透明层，hitTest 返回 nil 让点击穿透）
                GlassViewOnlyRepresentable(
                    onDropACTR: { url in
                        Task { @MainActor in
                            if case .overview = viewModel.actroMode {
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
                // 使用原生 NSToolbar 时隐藏 SwiftUI toolbar 内容
                if !useNativeToolbar {
                    // 左侧：导航按钮
                    ToolbarItemGroup(placement: .navigation) {
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

                // 中间：地址栏（50% viewportSize 宽度，但限制最大宽度以保持居中）
                ToolbarItem(placement: .principal) {
                    // 左右按钮组各约 100px，需要预留空间才能保持居中
                    // 可居中的最大宽度 = viewportSize.width - 2 * 较大一侧的按钮宽度
                    let sideButtonsWidth: CGFloat = 140
                    let maxCenterableWidth = max(200, viewportSize.width - 2 * sideButtonsWidth)
                    // 最小宽度 200，确保地址栏始终可见
                    let addressBarWidth = max(200, min(viewportSize.width * 0.5, maxCenterableWidth))

                    if case .overview = viewModel.actroMode {
                        Color.clear
                            .frame(width: addressBarWidth, height: 28)
                    } else {
                        let leftIconState: AddressBarLeftIconState = {
                            guard let tab = viewModel.currentTab else { return .none }
                            switch tab.contentType {
                            case .reactNative: return .menu
                            case .web: return isWebLoading ? .loading : .none
                            case .native, .empty: return .none
                            }
                        }()

                        let currentURLString = viewModel.currentTab?.url?.absoluteString ?? ""

                        let displayTitle: String = {
                            guard let tab = viewModel.currentTab else { return "New Tab" }
                            if case .web = tab.contentType, let host = tab.url?.host {
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
                                // Send visibility change to XPC service
                                let tabId: UUID? = {
                                    if case .focus(let focusTabId) = viewModel.actroMode {
                                        return focusTabId
                                    }
                                    return viewModel.currentTabId
                                }()
                                if let tabId = tabId {
                                    let newVisible = viewModel.isLeftPanelVisible(for: tabId)
                                    let coordinator = viewBridgeCoordinatorManager.coordinator(for: tabId)
                                    coordinator.sendLeftPanelVisibilityChanged(newVisible)
                                }
                            },
                            showingBookmarkPopover: $showingBookmarkSheet,
                            shouldAutoFocus: $viewModel.shouldAutoFocusAddressBar,
                            refreshAction: { viewModel.reloadCurrentTab() },
                            onNavigate: { input in
                                viewModel.navigateCurrentTab(to: input)
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
                }

                // 右侧：操作按钮
                ToolbarItemGroup(placement: .primaryAction) {
                    DownloadToolbarButton()

                    if case .focus = viewModel.actroMode,
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

                    Button(action: toggleFocusOverview) {
                        Image(systemName: "square.on.square")
                    }
                    .help("标签总览 (⌘⇧\\)")
                }
                }  // end if !useNativeToolbar
            }
        // 🔑 处理原生 toolbar 按钮的模式切换请求（通过 ModeTransitionController）
        .onReceive(NotificationCenter.default.publisher(for: .toggleFocusOverviewRequested)) { _ in
            toggleFocusOverview()
        }
        .onChange(of: viewModel.actroMode) { oldValue, newValue in
            // 从 Focus 切换到 Overview 时，先截图当前页面
            if case .focus(let focusedTabId) = oldValue, case .overview = newValue {
                if let tab = viewModel.tabs.first(where: { $0.id == focusedTabId }) {
                    switch tab.contentType {
                    case .web:
                        // Web tab: 异步截图
                        Task {
                            await WebViewHolderManager.shared.takeSnapshot(for: focusedTabId)
                        }
                    case .reactNative:
                        // ViewBridge tab: 同步截图
                        ViewBridgeHostViewManager.shared.takeSnapshot(for: focusedTabId)
                    case .native, .empty:
                        // Native SwiftUI and empty tabs don't need snapshots
                        break
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
            // Note: isOverviewMode 由 ViewBridgeCoordinatorManager 订阅 actroMode 自动同步
        }
        // shouldAutoFocusAddressBar 的处理移到 AddressBarContainer 的 onAppear 中
        // 这样当地址栏实际渲染时才触发，无需延迟
        .onAppear {
            // Setup native titlebar toggle callback
            // 🔑 发送通知由 view 处理，确保通过 ModeTransitionController 进行过渡
            TitlebarManager.shared.onToggleFocusOverview = {
                NotificationCenter.default.post(name: .toggleFocusOverviewRequested, object: nil)
            }

            // 🔑 绑定 XPCBridgeService 到 ViewModel（集中状态管理）
            // XPCBridgeService 订阅 actroMode 变化，自动同步 isOverviewMode
            XPCBridgeService.shared.bindToViewModel(viewModel)
            // 同时保持 ViewBridgeCoordinatorManager 的绑定（向后兼容，逐步迁移）
            ViewBridgeCoordinatorManager.shared.bindToViewModel(viewModel)

            // ━━━━ Mode Switching Trigger #1: 键盘快捷键 ━━━━
            // Cmd+Shift+\ (keyCode 42) → 发送通知 → toggleFocusOverview()
            // 🔑 重要：不能直接调用 toggleFocusOverview()，因为 closure 捕获的是旧的 View 状态
            // 必须通过通知让 .onReceive 在 SwiftUI 更新周期中调用，确保使用最新的 viewportSize
            // 参见 MARK: - Mode Switching (统一入口)
            keyEventMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
                let cmdShift: NSEvent.ModifierFlags = [.command, .shift]
                if event.keyCode == 42 && event.modifierFlags.intersection(.deviceIndependentFlagsMask).contains(cmdShift) {
                    // 发送通知，与原生工具栏按钮使用相同的代码路径
                    NotificationCenter.default.post(name: .toggleFocusOverviewRequested, object: nil)
                    return nil
                }

                // Cmd+F → Toggle find bar (only in Focus mode with web tab)
                if event.keyCode == 3 && event.modifierFlags.intersection(.deviceIndependentFlagsMask).contains(.command) {
                    if case .focus(let tabId) = viewModel.actroMode,
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
                    if case .focus(let tabId) = viewModel.actroMode {
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
                // 通知 LiquidGlassToolbar 取消焦点 (for native titlebar)
                let shouldNotifyDismiss: Bool = {
                    if useNativeToolbar {
                        if let window = event.window ?? NSApp.keyWindow,
                           let contentView = window.contentView {
                            let pointInContent = contentView.convert(event.locationInWindow, from: nil)
                            return contentView.bounds.contains(pointInContent)
                        }
                        return false
                    } else {
                        return true
                    }
                }()

                if shouldNotifyDismiss {
                    NotificationCenter.default.post(name: NSNotification.Name("com.actro.dismissAddressBarFocus"), object: nil)
                }

                // 旧逻辑：SwiftUI toolbar 的地址栏
                if !useNativeToolbar && isAddressBarEditing {
                    // 延迟执行，让系统先处理点击事件
                    // 只需设置 isFirstResponder=false，onChange 会自动退出编辑模式
                    DispatchQueue.main.async {
                        isAddressBarFirstResponder = false
                    }
                }
                return event
            }

            // ━━━━ Mode Switching Trigger #4: 双指捏合 ━━━━
            // - ACTR/RN 内容: 由 ActroViewService 端处理并发送 DistributedNotification
            // - Web 内容: 由本地 magnify 事件监听器处理
            magnifyEventMonitor = NSEvent.addLocalMonitorForEvents(matching: .magnify) { [self] event in
                guard case .focus = viewModel.actroMode else { return event }

                switch event.phase {
                case .began:
                    cumulativeMagnification = 0
                case .changed:
                    cumulativeMagnification += event.magnification
                    // Pinch-in (zoom out) threshold to enter Overview
                    if cumulativeMagnification < -0.3 {
                        cumulativeMagnification = 0
                        fputs("🤏 Local magnify threshold reached → exitToOverview()\n", stderr)
                        exitToOverview()
                    }
                case .ended, .cancelled:
                    cumulativeMagnification = 0
                default:
                    break
                }
                return event
            }
        }
        .onDisappear {
            // 清理事件监听
            if let monitor = keyEventMonitor {
                NSEvent.removeMonitor(monitor)
            }
            if let monitor = mouseEventMonitor {
                NSEvent.removeMonitor(monitor)
            }
            if let monitor = magnifyEventMonitor {
                NSEvent.removeMonitor(monitor)
            }
        }
        // ━━━━ Mode Switching Trigger #4: 双指捏合 (分布式通知监听) ━━━━
        // 参见 MARK: - Mode Switching (统一入口)
        .onReceive(
            DistributedNotificationCenter.default().publisher(for: Notification.Name("com.actro.pinchToOverview"))
        ) { _ in
            guard case .focus = viewModel.actroMode else { return }
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
    // │                      (由 ActroViewService 端处理并发送分布式通知)     │
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
        switch viewModel.actroMode {
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
        guard viewModel.actroMode != .focus(tabId: tabId) else { return }

        // 过渡进行中，忽略
        guard !modeTransitionController.phase.isTransitioning else {
            fputs("🟢 enterFocus() IGNORED (transition in progress)\n", stderr)
            return
        }

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

        // 🔑 计算卡片在视口中的位置（用于 Zoom 动画）
        let safeViewportSize = CGSize(
            width: max(viewportSize.width, 100),
            height: max(viewportSize.height, 100)
        )
        let cardAspectRatio = safeViewportSize.width / safeViewportSize.height
        let gridResult = gridLayout(
            for: safeViewportSize,
            cardAspectRatio: cardAspectRatio,
            cardCount: filteredTabs.count
        )

        // 计算卡片在 actro 坐标系中的 rect（VirtualGrid 使用 actro 坐标）
        let slotIndex = filteredTabPositions[tabId] ?? 0
        let actroCardPosition = slotIndex < gridResult.positions.count
            ? gridResult.positions[slotIndex]
            : .zero
        let actroCardRect = CGRect(
            x: actroCardPosition.x,
            y: actroCardPosition.y,
            width: gridResult.cardWidth,
            height: gridResult.cardHeight
        )

        // 🔑 从缓存获取 Overview 快照（在上次 exitToOverview 时设置的卡片尺寸截图）
        // 注意：不要截取 viewport，因为 Overview 模式下 viewport 显示的是整个网格，不是单个卡片
        let overviewSnapshot = TabSnapshotCacheManager.shared.overviewSnapshot(for: tabId)

        // 🔑 计算当前 Overview 模式的 transform（用于动画起点）
        let overviewTransform = calculateOverviewTransform(
            actroSize: gridResult.actroSize,
            viewportSize: safeViewportSize
        )

        // 🔑 使用 ModeTransitionController 管理过渡动画
        // 传递 actro 坐标的 cardRect，与 VirtualGrid 坐标系一致
        // 注意：actroMode 在动画完成后才切换，避免动画期间触发 SwiftUI 重绘
        modeTransitionController.startOverviewToFocusTransition(
            tabId: tabId,
            cardRect: actroCardRect,
            viewportSize: safeViewportSize,
            overviewTransform: overviewTransform,
            overviewSnapshot: overviewSnapshot
        ) { [weak viewModel] in
            // 动画完成后切换 actroMode
            viewModel?.actroMode = .focus(tabId: tabId)
        }
    }

    /// 退出到 Overview 模式
    private func exitToOverview() {
        // 记住当前 focused 的 tab（在切换前）
        guard case .focus(let currentTabId) = viewModel.actroMode else {
            // 已经在 Overview 模式
            return
        }
        lastFocusedTabId = currentTabId

        // 过渡进行中，忽略
        guard !modeTransitionController.phase.isTransitioning else {
            fputs("🔴 exitToOverview() IGNORED (transition in progress)\n", stderr)
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

        // 🔑 截取当前 Focus 模式下的 Tab 内容快照（大图）
        // 这是 Focus → Overview 动画的起点画面
        // 注意：applyBlur=false 确保同步设置，动画开始时立即可用
        let focusSnapshot = captureViewportSnapshot(viewportSize: viewportSize)
        if let snapshot = focusSnapshot {
            TabSnapshotCacheManager.shared.setFocusSnapshot(snapshot, for: currentTabId, applyBlur: false)

            // 同时设置 Overview 快照（缩放到卡片尺寸）
            // 先计算卡片尺寸
            let safeViewportSizeForCard = CGSize(
                width: max(viewportSize.width, 100),
                height: max(viewportSize.height, 100)
            )
            let cardAspectRatio = safeViewportSizeForCard.width / safeViewportSizeForCard.height
            let gridResultForCard = gridLayout(
                for: safeViewportSizeForCard,
                cardAspectRatio: cardAspectRatio,
                cardCount: filteredTabs.count
            )
            let cardSize = CGSize(width: gridResultForCard.cardWidth, height: gridResultForCard.cardHeight)
            // applyBlur=false 确保同步设置
            TabSnapshotCacheManager.shared.setOverviewSnapshot(snapshot, for: currentTabId, targetSize: cardSize, applyBlur: false)
        }

        // 🔑 计算卡片在 Overview 模式下视口中的位置（用于 Zoom 动画）
        let safeViewportSize = CGSize(
            width: max(viewportSize.width, 100),
            height: max(viewportSize.height, 100)
        )
        let cardAspectRatio = safeViewportSize.width / safeViewportSize.height
        let gridResult = gridLayout(
            for: safeViewportSize,
            cardAspectRatio: cardAspectRatio,
            cardCount: filteredTabs.count
        )

        // 计算卡片在 actro 坐标系中的 rect（VirtualGrid 使用 actro 坐标）
        let slotIndex = filteredTabPositions[currentTabId] ?? 0
        let actroCardPosition = slotIndex < gridResult.positions.count
            ? gridResult.positions[slotIndex]
            : .zero
        let actroCardRect = CGRect(
            x: actroCardPosition.x,
            y: actroCardPosition.y,
            width: gridResult.cardWidth,
            height: gridResult.cardHeight
        )

        // 🔑 计算 Overview 模式的 transform（用于动画终点）
        let overviewTransform = calculateOverviewTransform(
            actroSize: gridResult.actroSize,
            viewportSize: safeViewportSize
        )

        // 🔑 使用 ModeTransitionController 管理过渡动画
        // 注意：actroMode 在动画完成后才切换，避免动画期间触发 SwiftUI 重绘
        // 这与 enterFocus() 保持一致的模式
        modeTransitionController.startFocusToOverviewTransition(
            tabId: currentTabId,
            cardRect: actroCardRect,
            viewportSize: safeViewportSize,
            overviewTransform: overviewTransform,
            overviewSnapshot: nil  // VirtualGrid 方案使用缩略图，不需要截图
        ) { [weak viewModel] in
            // 动画完成后切换 actroMode
            viewModel?.actroMode = .overview
        }
    }

    /// 截取当前视口的快照（用于过渡动画）
    /// 只截取视口区域，不包括侧边栏和工具栏
    /// 优化：直接截取视口区域，避免截取整个窗口再裁剪
    /// - Parameter viewportSize: 视口尺寸
    private func captureViewportSnapshot(viewportSize: CGSize) -> NSImage? {
        let startTime = CACurrentMediaTime()

        guard let window = NSApp.keyWindow ?? NSApp.mainWindow,
              let contentView = window.contentView else {
            return nil
        }

        // 计算视口在窗口内容视图中的区域
        let windowSize = contentView.bounds.size
        let sidebarWidth = windowSize.width - viewportSize.width

        // contentView 可能包含 toolbar (高度差 = toolbar 高度)
        // 需要根据 contentView 的坐标系确定视口的 y 位置
        let toolbarHeight = windowSize.height - viewportSize.height
        let isFlipped = contentView.isFlipped

        // 排除顶部 1px 的 toolbar 分隔线
        let separatorHeight: CGFloat = 1

        // 视口的 y 坐标：
        // - 非 flipped (原点左下角): y = 0 (视口在底部)，排除顶部分隔线需要减少 height
        // - flipped (原点左上角): y = toolbarHeight + 1 (跳过分隔线)
        let viewportY: CGFloat = isFlipped ? (toolbarHeight + separatorHeight) : 0
        let captureHeight: CGFloat = viewportSize.height - separatorHeight

        let captureRect = CGRect(
            x: sidebarWidth,
            y: viewportY,
            width: viewportSize.width,
            height: captureHeight
        )

        NSLog("📸 captureViewportSnapshot: windowSize=%.0fx%.0f, viewportSize=%.0fx%.0f, toolbarHeight=%.0f, isFlipped=%d, captureRect=(%.0f,%.0f,%.0f,%.0f)",
              windowSize.width, windowSize.height,
              viewportSize.width, viewportSize.height,
              toolbarHeight, isFlipped ? 1 : 0,
              captureRect.origin.x, captureRect.origin.y, captureRect.width, captureRect.height)

        // 直接为截取区域创建 bitmap
        guard let bitmapRep = contentView.bitmapImageRepForCachingDisplay(in: captureRect) else {
            return nil
        }

        let cacheTime = CACurrentMediaTime()
        contentView.cacheDisplay(in: captureRect, to: bitmapRep)
        let captureTime = CACurrentMediaTime()

        // 创建完整 viewportSize 的图像，绘制时拉伸填满（排除的 1px 几乎不可见）
        let image = NSImage(size: viewportSize)
        image.lockFocus()
        if let ctx = NSGraphicsContext.current {
            ctx.imageInterpolation = .high
            bitmapRep.draw(in: NSRect(origin: .zero, size: viewportSize))
        }
        image.unlockFocus()

        let endTime = CACurrentMediaTime()
        NSLog("📸 captureViewportSnapshot: %.1fms (cache: %.1fms)",
              (endTime - startTime) * 1000,
              (captureTime - cacheTime) * 1000)

        return image
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
              case .focus(let tabId) = viewModel.actroMode,
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
        guard case .focus(let tabId) = viewModel.actroMode,
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

    // MARK: - Data Conversion for React Native

    /// Convert bookmarks to dictionary array for RN layer
    private var bookmarksForRN: [[String: Any]] {
        bookmarkVM.bookmarks.map { bookmark in
            [
                "id": bookmark.id.uuidString,
                "title": bookmark.title,
                "appURL": bookmark.appURL?.absoluteString ?? "",
                "createdAt": Int(bookmark.createdAt.timeIntervalSince1970 * 1000)
            ] as [String: Any]
        }
    }

    /// Convert recent history to dictionary array for RN layer
    private var recentHistoryForRN: [[String: Any]] {
        historyVM.recentItems.map { item in
            [
                "id": item.id.uuidString,
                "title": item.title,
                "appURL": item.appURL?.absoluteString ?? "",
                "visitTime": Int(item.visitTime.timeIntervalSince1970 * 1000)
            ] as [String: Any]
        }
    }

    // MARK: - Transform Calculations

    /// Overview 模式变换：缩放画布以适应视口，居中显示
    private func calculateOverviewTransform(
        actroSize: CGSize,
        viewportSize: CGSize
    ) -> (scale: CGFloat, offset: CGPoint) {
        guard actroSize.width > 0 && actroSize.height > 0 else {
            return (1.0, .zero)
        }

        // 缩放：让 Actro 适应视口（不放大，只缩小）
        let scaleX = viewportSize.width / actroSize.width
        let scaleY = viewportSize.height / actroSize.height
        let scale = max(min(scaleX, scaleY, 1.0), 0.001)

        // 居中
        let scaledWidth = actroSize.width * scale
        let scaledHeight = actroSize.height * scale
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
        gridResult: (positions: [CGPoint], cardWidth: CGFloat, cardHeight: CGFloat, actroSize: CGSize),
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

    /// 悬浮标题栏高度（Overview 模式下显示在卡片上方）- 使用共享样式
    private var floatingTitleBarHeight: CGFloat { CardStyle.titleBarHeight }

    /// 计算间距（卡片之间）和垂直边距
    // MARK: - VirtualGrid Helper

    /// 构建 VirtualGrid 卡片信息数组
    /// - Parameters:
    ///   - tabs: Tab 列表
    ///   - positions: Grid 布局位置数组
    ///   - tabSlotPositions: Tab ID 到槽位索引的映射
    /// - Returns: VirtualGridCardInfo 数组
    /// [+] 按钮的固定 UUID
    private static let addButtonUUID = UUID(uuidString: "00000000-0000-0000-0000-000000000001")!

    /// 缓存的 [+] 按钮图像及其尺寸（避免每次 body 评估时重新创建）
    private static var cachedAddButtonImage: (image: NSImage, size: CGSize)?

    private func buildVirtualGridCards(
        tabs: [Tab],
        positions: [CGPoint],
        tabSlotPositions: [UUID: Int],
        cardWidth: CGFloat,
        cardHeight: CGFloat
    ) -> [VirtualGridCardInfo] {
        let cacheManager = TabSnapshotCacheManager.shared

        var cards = tabs.compactMap { tab -> VirtualGridCardInfo? in
            guard let slotIndex = tabSlotPositions[tab.id],
                  slotIndex < positions.count else {
                return nil
            }

            // 从 TabSnapshotCache 获取双缓冲快照
            let focusSnapshot = cacheManager.focusSnapshot(for: tab.id)
            let overviewSnapshot = cacheManager.overviewSnapshot(for: tab.id)

            return VirtualGridCardInfo(
                tabId: tab.id,
                position: positions[slotIndex],
                focusSnapshot: focusSnapshot,
                overviewSnapshot: overviewSnapshot,
                title: tab.title
            )
        }

        // 添加 [+] 按钮卡片（在最后一个位置）
        // [+] 按钮没有标题栏
        let addButtonIndex = tabs.count
        if addButtonIndex < positions.count {
            let addButtonImage = createAddButtonImage(width: cardWidth, height: cardHeight)
            cards.append(VirtualGridCardInfo(
                tabId: Self.addButtonUUID,
                position: positions[addButtonIndex],
                focusSnapshot: addButtonImage,
                overviewSnapshot: addButtonImage,
                title: ""  // [+] 按钮无标题
            ))
        }

        return cards
    }

    /// 创建 [+] 按钮的图像（匹配 NewTabPanel 的外观）
    /// 使用静态缓存，只在尺寸变化时才重新创建
    /// - Parameters:
    ///   - width: 卡片宽度
    ///   - height: 卡片高度
    /// - Returns: 渲染后的图像
    private func createAddButtonImage(width: CGFloat, height: CGFloat) -> NSImage {
        let size = CGSize(width: width, height: height)

        // 检查缓存：如果尺寸相同，直接返回缓存的图像
        if let cached = Self.cachedAddButtonImage,
           abs(cached.size.width - size.width) < 1 && abs(cached.size.height - size.height) < 1 {
            return cached.image
        }

        // 创建新图像
        let image = NSImage(size: size)
        image.lockFocus()

        let rect = NSRect(origin: .zero, size: size)
        let cornerRadius = CardStyle.cornerRadius

        // 背景：深灰色 (匹配 NSColor(white: 0.15, alpha: 1.0))
        let backgroundPath = NSBezierPath(roundedRect: rect, xRadius: cornerRadius, yRadius: cornerRadius)
        NSColor(white: 0.15, alpha: 1.0).setFill()
        backgroundPath.fill()

        // 边框：白色 0.15 透明度
        NSColor.white.withAlphaComponent(0.15).setStroke()
        backgroundPath.lineWidth = 1.0
        backgroundPath.stroke()

        // + 号：使用 SF Symbol "plus"
        // 计算 + 号尺寸：与真实 NewTabPanel 的 48pt 字体大小成比例
        // 真实卡片通常约 300px 宽，48pt 约为 16%
        let plusSize = min(width, height) * 0.16
        let plusConfig = NSImage.SymbolConfiguration(pointSize: plusSize, weight: .regular)
        if let plusImage = NSImage(systemSymbolName: "plus", accessibilityDescription: nil)?
            .withSymbolConfiguration(plusConfig) {
            // 居中绘制
            let plusRect = NSRect(
                x: (size.width - plusImage.size.width) / 2,
                y: (size.height - plusImage.size.height) / 2,
                width: plusImage.size.width,
                height: plusImage.size.height
            )
            // 设置白色 0.5 透明度
            NSColor.white.withAlphaComponent(0.5).set()
            plusImage.draw(in: plusRect, from: .zero, operation: .sourceOver, fraction: 0.5)
        }

        image.unlockFocus()

        // 缓存新创建的图像
        Self.cachedAddButtonImage = (image: image, size: size)

        return image
    }

    // MARK: - Spacing Calculation

    /// - 水平间距 = windowWidth / (卡片数 × 3) × 2.5，下限 40px（2.5x 原值）
    /// - 垂直间距 = 基础间距 + 悬浮标题栏高度（为标题栏留出空间）
    /// - 垂直边距 = 基础间距 × 1.25 + 悬浮标题栏高度
    private func calculateSpacing(windowWidth: CGFloat, totalCards: Int) -> (hSpacing: CGFloat, vSpacing: CGFloat, verticalMargin: CGFloat) {
        let effectiveCards = max(totalCards, 4)
        // 基础间距用于垂直方向和边距计算
        let baseSpacing = max(16, windowWidth / (CGFloat(effectiveCards) * 7))
        // 水平间距（减小下限让卡片更大）
        let hSpacing = max(25, baseSpacing * 2.0)
        let vSpacing = baseSpacing + floatingTitleBarHeight  // 纵向间距包含标题栏高度
        // 顶部边距：包含标题栏高度 + 少量额外空间避免与 Search Tabs 重叠
        let verticalMargin = baseSpacing * 1.25 + floatingTitleBarHeight + 15
        return (hSpacing, vSpacing, verticalMargin)
    }

    /// 网格布局计算
    /// - 初始容量 4（2×2），卡片数 ≤ 4 时强制 2×2 布局
    /// - 水平边距 = spacing × 1.25（通过卡片尺寸计算保证）
    private func gridLayout(
        for size: CGSize,
        cardAspectRatio: CGFloat,
        cardCount: Int
    ) -> (positions: [CGPoint], cardWidth: CGFloat, cardHeight: CGFloat, actroSize: CGSize) {
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

        // Actro 宽度 = viewport 宽度
        let actroSize = CGSize(
            width: size.width,
            height: verticalMargin * 2 + CGFloat(rows) * cardHeight + CGFloat(rows - 1) * vSpacing
        )

        return (positions, cardWidth, cardHeight, actroSize)
    }

}

// MARK: - Actro View

private struct TabGridView: View {
    let tabs: [Tab]
    let tabSlotPositions: [UUID: Int]
    let positions: [CGPoint]
    let cardWidth: CGFloat
    let cardHeight: CGFloat
    let titleBarRatio: CGFloat  // 标题栏高度占卡片高度的比例
    let toolbarHeight: CGFloat  // toolbar 实际高度
    let currentScale: CGFloat
    let mode: ActroMode
    let showNewTabButton: Bool  // 是否显示 [+] 按钮（搜索过滤时隐藏）
    let viewportSize: CGSize    // 视口尺寸，用于 RN 渲染
    let viewBridgeCoordinatorManager: ViewBridgeCoordinatorManager
    @ObservedObject var bookmarkVM: BookmarkViewModel
    @ObservedObject var historyVM: HistoryViewModel
    let onSelectTab: (UUID) -> Void
    let onSwapPositions: (UUID?, UUID?) -> Void
    let onNewTab: () -> Void
    let onCloseTab: (UUID) -> Void
    var onNavigate: ((UUID, URL) -> Void)?  // tabId, url

    // 🔑 拖动状态绑定（从 WorkspaceMainView 传入，让 UnifiedReactNativeLayer 也能访问）
    @Binding var draggingTabId: UUID?
    @Binding var dropTargetTabId: UUID?

    @State private var hoveringOverTabId: UUID?
    @State private var swappingTabId: UUID?  // 正在交换过程中的卡片（用于淡入动画）
    @State private var isDraggingActive = false  // 跟踪拖动是否正在进行（用于抑制点击）

    // 固定的槽位索引（拖动开始时记录，不随 tabs 交换而改变）
    @State private var dragSourceSlotIndexFixed: Int?  // 拖动源槽位 a
    @State private var dropTargetSlotIndexFixed: Int?  // 目标槽位 b

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
        let _ = NSLog("🔄 [ACTRO-BODY] TabGridView.body evaluated: tabs=%d, mode=%@", tabs.count, String(describing: mode))
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
                        viewBridgeCoordinator: viewBridgeCoordinatorManager.coordinator(for: tab.id),
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
                            isDraggingActive = true
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
                        dropTargetTabId: $dropTargetTabId,
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
                                isDraggingActive = false
                                draggingTabId = nil
                                swappingTabId = nil
                                dragSourceSlotIndexFixed = nil
                                dropTargetSlotIndexFixed = nil
                                dropTargetTabId = nil
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
                        RoundedRectangle(cornerRadius: CardStyle.cornerRadius)
                            .strokeBorder(
                                Color.blue.opacity(0.6),
                                style: StrokeStyle(lineWidth: 2, dash: [5, 5])
                            )
                    } else if isDropTarget {
                        // 目标：绿色实线
                        RoundedRectangle(cornerRadius: CardStyle.cornerRadius)
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
    @Binding var dropTargetTabId: UUID?  // 🔑 新增：用于 RN 层显示边框
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
            // 🔑 更新 dropTargetTabId 用于 RN 层边框
            dropTargetTabId = targetId
        }
    }

    func dropExited(info: DropInfo) {
        if let targetId = targetTabId {
            if hoveringOverTabId == targetId {
                hoveringOverTabId = nil
                // 清除目标槽位标记
                dropTargetSlotIndexFixed = nil
                // 🔑 清除 dropTargetTabId
                dropTargetTabId = nil
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
        dropTargetTabId = nil
    }

    private func clearAllStates() {
        draggingTabId = nil
        hoveringOverTabId = nil
        dropTargetSlotIndexFixed = nil
        dropTargetTabId = nil
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
        .frame(maxWidth: .infinity)
        .frame(height: 28)
        .overlay(
            RoundedRectangle(cornerRadius: 14)
                .stroke(
                    Color(red: 0.18, green: 0.56, blue: 0.96)
                        .opacity(focusRingOpacity),
                    lineWidth: 2.8
                )
                .scaleEffect(focusRingScale)
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
                // Safari 风格：从较大的蓝圈收缩到边框
                // 1. 先设置初始状态（大圈、完全可见）
                focusRingScale = 1.2
                focusRingOpacity = 1.0
                // 2. 延迟一帧后开始收缩动画，确保初始状态已渲染
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.02) {
                    withAnimation(.easeOut(duration: 0.2)) {
                        focusRingScale = 1.0
                    }
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

    @State private var isLocalDragging = false  // 本地拖动状态，用于抑制点击手势

    // 悬浮标题栏高度 - 使用共享样式
    private var floatingTitleBarHeight: CGFloat { CardStyle.titleBarHeight }

    var body: some View {
        let _ = NSLog("🔄 [TABCARD-BODY] TabPanel.body: tabId=%@, isFocused=%d", tab.id.uuidString, isFocused ? 1 : 0)

        // 外层尺寸：始终使用 cardWidth/cardHeight
        // Actro 变换负责在 Focus 模式下缩放到 viewportSize
        // 如果这里使用 viewportSize，会与 actro scale 叠加导致过度放大
        let panelWidth = cardWidth
        let panelHeight = cardHeight

        // 卡片主体（纯内容）
        // 圆角仅 Overview 模式显示，Focus 模式不要圆角
        // 使用 overlay 放置标题栏，这样标题栏有独立的 hit test 区域
        contentView
            .frame(width: panelWidth, height: panelHeight)
            .cornerRadius(isFocused ? 0 : CardStyle.cornerRadius)
            // 🔑 Overview 模式统一边框样式（与 NewTabPanel、UnifiedReactNativeLayer 一致）
            .overlay {
                if !isFocused {
                    RoundedRectangle(cornerRadius: CardStyle.cornerRadius)
                        .stroke(Color.white.opacity(0.15), lineWidth: 1)
                }
            }
            .shadow(color: isFocused ? .clear : Color.black.opacity(0.3), radius: 5, x: 0, y: 2)
            // 只有卡片主体响应 onTapGesture（不包括标题栏）
            .contentShape(Rectangle())
            .onTapGesture {
                // 抑制拖动过程中的点击手势
                if !isFocused && !isBeingDragged && !isLocalDragging {
                    onSelect()
                }
            }
            // 悬浮标题栏 - 使用 overlay，让标题栏在卡片上方有独立的 hit test 区域
            .overlay(alignment: .top) {
                if !isFocused {
                    floatingTitleBarView
                        .frame(width: cardWidth, height: floatingTitleBarHeight)
                        // 将标题栏移动到卡片上方（在内容区域之外）
                        .offset(y: -(floatingTitleBarHeight + CardStyle.titleBarGap))
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
        // 整个卡片作为拖动源（Overview 模式下长按拖动）
        .onDrag {
            if isFocused {
                return NSItemProvider()
            }
            // 设置本地拖动状态，抑制点击手势
            isLocalDragging = true
            return onDragStart()
        } preview: {
            // 自定义拖动预览：整个卡片内容
            dragPreviewView
        }
        // 监听外部拖动状态变化，重置本地拖动状态
        .onChange(of: isBeingDragged) { _, newValue in
            if !newValue {
                // 外部拖动结束时，重置本地拖动状态
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) {
                    isLocalDragging = false
                }
            }
        }
    }

    // 悬浮标题栏视图（简化版，仅包含标题和关闭按钮）- 使用共享样式
    private var floatingTitleBarView: some View {
        CardStyle.floatingTitleBar(title: tab.title, onClose: onClose)
    }

    // 拖动预览视图 - 显示卡片 + 悬浮标题栏（使用共享样式）
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
            .cornerRadius(CardStyle.cornerRadius)
            .overlay(
                RoundedRectangle(cornerRadius: CardStyle.cornerRadius)
                    .stroke(Color.white.opacity(0.3), lineWidth: 1)
            )
            .shadow(color: Color.black.opacity(0.5), radius: 10, x: 0, y: 5)

            // 悬浮标题栏 - 使用共享样式
            HStack(spacing: CardStyle.titleBarItemSpacing) {
                CardStyle.closeIcon()
                CardStyle.titleText(tab.title)
                Spacer()
            }
            .padding(.horizontal, CardStyle.titleBarHorizontalPadding)
            .frame(width: cardWidth, height: floatingTitleBarHeight)
            .offset(y: -floatingTitleBarHeight - CardStyle.titleBarGap)
        }
    }

    // 卡片内容区域尺寸（画布坐标系中的尺寸）
    private var cardContentSize: CGSize {
        CGSize(width: cardWidth, height: cardHeight)
    }

    // 内容区域视图 - 多态容器
    // 根据 Tab.contentType 决定使用 ViewBridge (RN) 还是 WKWebView (Web)
    private var contentView: some View {
        // ViewBridge XPC 需要实际渲染尺寸：
        // - Focus 模式：使用 viewportSize（因为 XPC 独立进程渲染，不受 Actro 变换影响）
        // - Overview 模式：使用 cardSize
        // 注意：这里的 panelSize 是传给 ViewBridge 的渲染尺寸，不是 SwiftUI frame
        let renderSize: CGSize
        if isFocused {
            renderSize = CGSize(width: max(viewportSize.width, 1), height: max(viewportSize.height, 1))
        } else {
            renderSize = CGSize(width: max(cardWidth, 1), height: max(cardHeight, 1))
        }

        let _ = NSLog("📐 TabPanel.contentView: type=%@, isFocused=%d, viewportSize=%.0fx%.0f, cardSize=%.0fx%.0f, renderSize=%.0fx%.0f",
                      String(describing: tab.contentType), isFocused ? 1 : 0,
                      viewportSize.width, viewportSize.height,
                      cardWidth, cardHeight,
                      renderSize.width, renderSize.height)

        return ZStack(alignment: .topLeading) {
            // 底层背景 - 使用 Tab 的随机背景颜色
            // 🔑 React Native 标签在 Overview 模式使用透明背景
            //    因为 UnifiedReactNativeLayer 在 actro 外以 zIndex(50) 渲染
            //    需要通过透明背景才能看到 RN 内容
            //    边框已移到 TabPanel body 外层统一处理
            if case .reactNative = tab.contentType, !isFocused {
                // Overview 模式：透明背景，让 RN 内容显示
                Color.clear
            } else {
                Color(hslString: tab.backgroundColor)
            }

            // 多态渲染：根据 contentType 选择渲染引擎
            switch tab.contentType {
            case .reactNative(let fingerprint):
                // React Native 应用 - 使用 ViewBridge XPC
                // Media Query 风格的双层架构：
                // - Overview 模式：UnifiedReactNativeLayer 在 actro 外以 zIndex(50) 渲染
                //   内容通过透明的卡片背景显示
                // - Focus 模式：UnifiedReactNativeLayer 在 actro 外以 zIndex(100) 渲染
                //   覆盖在卡片上方
                OverviewViewBridgeContainer(
                    tab: tab,
                    fingerprint: fingerprint,
                    renderSize: renderSize,
                    isFocused: isFocused,
                    coordinator: viewBridgeCoordinator
                )
                .frame(width: renderSize.width, height: renderSize.height)

            case .web(let url):
                // Web 内容 - 使用 WKWebView
                // Focus 模式由 FocusedWebOverlay 独立渲染（actro 变换外）
                WebViewMultiProcessTabView(
                    tab: tab,
                    url: url,
                    containerSize: renderSize,
                    isFocused: isFocused
                )

            case .native(let nativeType):
                // Native SwiftUI content - rendered in main process, no XPC
                // 🔑 Focus mode: rendered by FocusedNativeOverlay (outside actro transform)
                // 🔑 Overview mode: rendered here at cardSize (will be shown as thumbnail)
                if isFocused {
                    // Focus mode: placeholder only, actual content rendered by FocusedNativeOverlay
                    Color.clear
                } else {
                    // Overview mode: render at cardSize for thumbnail
                    let nativeSize = CGSize(width: max(cardWidth, 1), height: max(cardHeight, 1))
                    switch nativeType {
                    case .startPage:
                        StartPageView(
                            bookmarks: bookmarkVM.bookmarks,
                            recentHistory: historyVM.recentItems,
                            containerSize: nativeSize,
                            isCompactMode: true,  // Always compact in Overview
                            onNavigate: { url in
                                onNavigate?(url)
                            }
                        )
                        .frame(width: nativeSize.width, height: nativeSize.height)
                    }
                }

            case .empty:
                // Empty placeholder - new tabs now use system startPage
                EmptyView()
            }
        }
        .frame(maxHeight: .infinity)
        // 注意：不要在这里添加 .onTapGesture，会与外层的 .onDrag 冲突
        // tap 手势在 TabPanel body 层统一处理
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
        .cornerRadius(CardStyle.cornerRadius)
        .overlay(
            RoundedRectangle(cornerRadius: CardStyle.cornerRadius)
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

// MARK: - Overview ViewBridge Container

/// Overview 模式下的 ViewBridge 占位容器
///
/// 🔑 新架构：不再渲染 PersistentViewBridgeView
/// ViewBridge 内容由 UnifiedReactNativeLayer 在 actro 外统一渲染
/// 这个容器只显示背景色作为占位，确保卡片有正确的视觉边界
private struct OverviewViewBridgeContainer: View {
    let tab: Tab
    let fingerprint: String
    let renderSize: CGSize
    let isFocused: Bool
    let coordinator: ViewBridgeCoordinator

    var body: some View {
        // 🔑 关键：不再渲染 PersistentViewBridgeView
        // ViewBridge 连接由 UnifiedReactNativeLayer 始终保持，这里只是占位
        Group {
            if isFocused {
                // Focus 模式：透明占位
                Color.clear
            } else {
                // Overview 模式：透明占位
                // 实际的 ViewBridge 内容在 actro 外由 UnifiedReactNativeLayer 渲染
                Color.clear
            }
        }
        .onAppear {
            allocateServiceIfNeeded()
        }
    }

    private func allocateServiceIfNeeded() {
        // 分配 service - 由 UnifiedReactNativeLayer 使用
        guard coordinator.allocatedServiceId == nil else { return }

        do {
            let serviceId = try RendererProcessPool.shared.allocateRenderer(
                for: tab.id,
                fingerprint: fingerprint
            )
            coordinator.allocatedServiceId = serviceId
            coordinator.tabId = tab.id
        } catch {
            NSLog("⚠️ OverviewViewBridgeContainer: Failed to allocate service: \(error)")
        }
    }
}

#Preview {
    WorkspaceMainView(viewModel: BrowserViewModel(), historyVM: HistoryViewModel(), bookmarkVM: BookmarkViewModel())
}
