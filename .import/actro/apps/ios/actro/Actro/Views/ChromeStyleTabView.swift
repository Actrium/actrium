//
//  ChromeStyleTabView.swift
//  Actro
//
//  Chrome-style tab grid view for iPad
//  - Adaptive grid columns based on container width
//  - Zoom transition between overview and focused modes via .sheet
//

import SwiftUI
import Kingfisher

private enum ChromeTabNotificationContract {
    static let systemAppType = RuntimeBridgeContract.systemAppType
    static let additionalProps = "additionalProps"
}

// MARK: - Chrome Style Tab View
struct ChromeStyleTabView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel
    
    @State private var focusedTab: Tab?
    @Namespace private var animation
    
    @State private var windowSize: CGSize = .zero

    var body: some View {
        GeometryReader { geometry in
            overviewView
                .background(Color.black)
                .fullScreenCover(item: $focusedTab) { tab in
                    focusSheetContent(tab: tab)
                        .ignoresSafeArea(.all, edges: .bottom)
                        .interactiveDismissDisabled(true)
                        .asNavigationTransition(id: viewModel.currentTabId ?? tab.id, in: animation)
                }
                .onAppear {
                    syncFocusedTabWithActroMode()
                    updateWindowState(size: geometry.size)
                }
                .onChange(of: viewModel.actroMode) { oldValue, newValue in
                    handleActroModeChange(newValue)
                }
                .onChange(of: viewModel.tabs.count) { oldValue, newValue in
                    handleTabsCountChange()
                }
                .background(
                    WindowAccessor { window in
                        let screenBounds = window.screen.bounds
                        let windowBounds = window.bounds
                        
                        viewModel.multipleWindowMode =
                        abs(screenBounds.width - windowBounds.width) < 1 &&
                        abs(screenBounds.height - windowBounds.height) < 1
                        
                    }
                )
                .onChange(of: geometry.size) { _, newSize in
                    updateWindowState(size: newSize)
                }
        }
    }
    
    private func updateWindowState(size: CGSize) {
        windowSize = size
    }

    private var overviewView: some View {
        ChromeTabOverviewView(
            viewModel: viewModel,
            historyVM: historyVM,
            animation: animation,
            onSelectTab: { tabId in
                guard let tab = viewModel.tabs.first(where: { $0.id == tabId }) else { return }
                viewModel.currentTabId = tabId
                viewModel.actroMode = .focus(tabId: tabId)
                focusedTab = tab
            },
            onNewTab: {
                let newTab = viewModel.addNewTab(deferAppend: true)
                viewModel.currentTabId = newTab.id
                viewModel.actroMode = .focus(tabId: newTab.id)
                focusedTab = newTab
            },
            onCloseTab: { tabId in
                viewModel.closeTab(id: tabId)
            }
        )
        .background(Color.black)
    }

    private func focusSheetContent(tab: Tab) -> some View {
        ChromeTabFocusView(
            initialTabId: tab.id,
            viewModel: viewModel,
            historyVM: historyVM,
            onBackToOverview: {
                // 退出前先截取当前 tab 内容区域的快照
                if let currentTab = viewModel.tabs.first(where: { $0.id == viewModel.currentTabId }), !currentTab.title.isEmpty {
                    // 返回 Overview 前清除当前 tab 未读状态
                    viewModel.clearUnread(for: currentTab.id)
                    takeSnapshot(for: currentTab) {
                        viewModel.actroMode = .overview
                        focusedTab = nil
                    }
                } else {
                    viewModel.actroMode = .overview
                    focusedTab = nil
                }
            }
        )
        .onAppear {
            // fullScreenCover 弹出后，将 pending 的新标签页正式加入列表显示
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                viewModel.commitPendingNewTab()
            }
        }
    }

    private func syncFocusedTabWithActroMode() {
        if case .focus(let tabId) = viewModel.actroMode,
           let tab = viewModel.tabs.first(where: { $0.id == tabId }) {
            focusedTab = tab
        }
    }

    private func handleActroModeChange(_ newValue: ActroMode) {
        switch newValue {
        case .overview:
            focusedTab = nil
        case .focus(let tabId):
            // 仅在从 overview 进入 focus 时设置 focusedTab（弹出 sheet）
            // focus 内切换 tab 时不应更新 focusedTab，避免 sheet 收起再弹出
            if focusedTab == nil {
                // 优先从 tabs 列表查找，其次从 pendingNewTab 查找（新建 Tab 尚未加入列表时）
                let tab = viewModel.tabs.first(where: { $0.id == tabId }) ?? viewModel.pendingNewTab
                if let tab = tab, tab.id == tabId {
                    focusedTab = tab
                }
            }
        }
    }

    private func handleTabsCountChange() {
        // 关闭最后一个 tab 时，切换到 Overview
        if viewModel.tabs.isEmpty {
            viewModel.actroMode = .overview
            focusedTab = nil
        }
    }

    /// 截取当前 tab 内容区域的快照（仅内容区域，不含导航栏）
    private func takeSnapshot(for tab: Tab, completion: @escaping () -> Void) {
        DispatchQueue.main.async {
            defer { completion() }

            let window = UIApplication.shared.connectedScenes
                .filter { $0.activationState == .foregroundActive }
                .compactMap { $0 as? UIWindowScene }
                .first?.windows
                .filter { $0.isKeyWindow }.first

            guard let window = window else { return }

            let renderer = UIGraphicsImageRenderer(bounds: window.bounds)
            let fullImage = renderer.image { context in
                window.drawHierarchy(in: window.bounds, afterScreenUpdates: false)
            }

            // 仅截取内容区域：跳过顶部导航栏（44pt）
            let screenHeight = window.bounds.height
            let navBarHeight: CGFloat = 44
            let contentRect = CGRect(
                x: 0,
                y: navBarHeight,
                width: window.bounds.width,
                height: screenHeight - navBarHeight
            )

            if let croppedImage = fullImage.cropped(to: contentRect) {
                viewModel.setTabSnapshot(croppedImage, for: tab.id)
            } else {
                viewModel.setTabSnapshot(fullImage, for: tab.id)
            }
        }
    }
}

// MARK: - Overview View (Tab Grid)
struct ChromeTabOverviewView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel
    var animation: Namespace.ID

    let onSelectTab: (UUID) -> Void
    let onNewTab: () -> Void
    let onCloseTab: (UUID) -> Void

    @State private var hoveringOverTabId: UUID?
    @State private var showSettings: Bool = false

    private let interItemSpacing: CGFloat = 16
    private let lineSpacing: CGFloat = 20
    private let sideInset: CGFloat = 16

    private func calculateNumberOfColumns(availableWidth: CGFloat) -> Int {
        let isLandscape = availableWidth > 600
        let maxColumns = isLandscape ? 4 : 3
        for columns in stride(from: maxColumns, through: 2, by: -1) {
            let cardWidth = (availableWidth - interItemSpacing * CGFloat(columns - 1)) / CGFloat(columns)
            if cardWidth >= 140 {
                return columns
            }
        }
        return 2
    }

    var body: some View {
        VStack(spacing: 0) {
            // 顶部导航栏
            OverviewNavigationBar(
                viewModel: viewModel,
                onBack: {
                    // 从 Overview 切换到当前选中的 Tab
                    if let currentTabId = viewModel.currentTabId,
                       let tab = viewModel.tabs.first(where: { $0.id == currentTabId }) {
                        viewModel.actroMode = .focus(tabId: tab.id)
                    } else if let firstTab = viewModel.tabs.first {
                        viewModel.currentTabId = firstTab.id
                        viewModel.actroMode = .focus(tabId: firstTab.id)
                    }
                },
                openSettings: { showSettings = true }
            )

            // 卡片网格
            cardGridView
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .sheet(isPresented: $showSettings) {
            SettingsView(systemType: .settings, viewModel: viewModel)
        }
        .onReceive(NotificationCenter.default.publisher(for: .openTabFromRN)) { notification in
            guard let userInfo = notification.userInfo,
                  let systemAppTypeStr = userInfo[ChromeTabNotificationContract.systemAppType] as? String,
                  let systemTabType = SystemTabType(rawValue: systemAppTypeStr) else { return }
            let additionalProps = userInfo[ChromeTabNotificationContract.additionalProps] as? [String: Any]
            showSettings = true
            withAnimation {
                let _ = viewModel.openSystemTab(
                    systemTabType,
                    additionalProps: additionalProps,
                    replaceAdditionalProps: additionalProps != nil
                )
            }
        }
    }

    private var cardGridView: some View {
        GeometryReader { geometry in
            let availableWidth = geometry.size.width - sideInset * 2
            let columns = calculateNumberOfColumns(availableWidth: availableWidth)
            // 卡片宽高比与设备屏幕一致，横竖屏切换时自动更新
            let cardAspectRatio = max(geometry.size.width, 1) / max(geometry.size.height, 1)

            ScrollViewReader { proxy in
                ScrollView {
                    // 顶部间距：适应自定义导航栏
                    Color.clear.frame(height: 15)

                    LazyVGrid(
                        columns: Array(repeating: GridItem(
                            .flexible(minimum: 140),
                            spacing: interItemSpacing
                        ), count: columns),
                        spacing: lineSpacing
                    ) {
                        ForEach(viewModel.tabs) { tab in
                            let isHovering = hoveringOverTabId == tab.id
                            ChromeTabCardView(
                                tab: tab,
                                viewModel: viewModel,
                                onSelect: { onSelectTab(tab.id) },
                                onClose: {
                                    withAnimation(.easeOut(duration: 0.25)) {
                                        onCloseTab(tab.id)
                                    }
                                }
                            )
                            .aspectRatio(cardAspectRatio, contentMode: .fit)
                            .asMatchedTransitionSource(id: tab.id, in: animation)
                            .opacity(isHovering ? 0.5 : 1.0)
                            .transition(.scale(scale: 0.8).combined(with: .opacity))
                            .draggable(tab.id.uuidString) {
                                ChromeTabDragPreviewView(tab: tab)
                            }
                            .dropDestination(for: String.self) { items, _ in
                                guard let sourceIdStr = items.first,
                                      let sourceId = UUID(uuidString: sourceIdStr),
                                      sourceId != tab.id else { return false }
                                withAnimation(.spring(response: 0.35, dampingFraction: 1.0)) {
                                    viewModel.swapSlotPositions(tabId1: sourceId, tabId2: tab.id)
                                    hoveringOverTabId = nil
                                    let tabIds = viewModel.tabsSortedBySlotPosition().map { $0.tab.id.uuidString }
                                    NotificationCenter.default.post(
                                        name: .tabsReorderedFromNative,
                                        object: nil,
                                        userInfo: ["tabIds": tabIds]
                                    )
                                }
                                return true
                            } isTargeted: { targeted in
                                if targeted {
                                    hoveringOverTabId = tab.id
                                } else if hoveringOverTabId == tab.id {
                                    hoveringOverTabId = nil
                                }
                            }
                        }

                        // New Tab Card
                        ChromeNewTabCardView()
                            .aspectRatio(cardAspectRatio, contentMode: .fit)
                            .id("newTabCard")
                            .onTapGesture {
                                onNewTab()
                            }
                    }
                    .padding(.horizontal, sideInset)
                    .padding(.top, 8)
                    .padding(.bottom, 16)
                }
                .onChange(of: viewModel.tabs.count) { oldCount, newCount in
                    if newCount > oldCount {
                        withAnimation {
                            proxy.scrollTo("newTabCard", anchor: .bottom)
                        }
                    }
                }
            }
        }
    }
}

// MARK: - Tab Drag Preview View
struct ChromeTabDragPreviewView: View {
    let tab: Tab
    private let iconWidth: CGFloat = 32

    var body: some View {
        VStack(spacing: 0) {
            ZStack {
                Color(hslString: tab.backgroundColor)
                Image(systemName: "globe")
                    .font(.system(size: 32))
                    .foregroundColor(.white.opacity(0.3))
            }

            HStack(alignment: .center, spacing: 12) {
                ZStack(alignment: .topTrailing) {
                    Circle()
                        .fill(Color(hslString: tab.backgroundColor))
                        .frame(width: iconWidth, height: iconWidth)
                        .overlay {
                            Image(systemName: "plus.viewfinder")
                                .font(.system(size: 14))
                                .foregroundColor(.white.opacity(0.9))
                        }
                }
                .frame(width: iconWidth, height: iconWidth)

                Text(tab.status == .dead ? "\(tab.title)(\(TM.tabStatus.serverClosed()))" : tab.title)
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundColor(Color(hex: "#ECECED"))
                    .lineLimit(1)

                Spacer()
            }
            .frame(height: 56)
            .padding(.horizontal, 14)
            .background(Color(white: 0.9))
        }
        .frame(width: 180, height: 180)
        .clipShape(RoundedRectangle(cornerRadius: 16))
        .shadow(color: .black.opacity(0.5), radius: 10, x: 0, y: 5)
    }
}

// MARK: - Tab Card View (SwiftUI)
struct ChromeTabCardView: View {
    let tab: Tab
    @ObservedObject var viewModel: BrowserViewModel
    let onSelect: () -> Void
    let onClose: () -> Void

    @EnvironmentObject private var theme: ThemeManager

    private let iconWidth: CGFloat = 32.0

    private func toggleBookmark() {
        let newBookmarkStatus = !tab.isBookmarked
        if let index = viewModel.tabs.firstIndex(where: { $0.id == tab.id }) {
            viewModel.tabs[index].isBookmarked = newBookmarkStatus
            viewModel.scheduleSaveTabs()
        }
        NotificationCenter.default.post(
            name: .tabBookmarkChangedFromNative,
            object: nil,
            userInfo: tab.runtimeBookmarkChangedUserInfo(isBookmarked: newBookmarkStatus)
        )
    }

    private func closeOtherTabs() {
        withAnimation(.spring(response: 0.35, dampingFraction: 0.8)) {
            viewModel.closeOtherTabs(keepTabId: tab.id)
        }
    }

    var body: some View {
        ZStack(alignment: .bottom) {
            // Content preview area
            HStack {
                if let snapshot = viewModel.tabSnapshots[tab.id] {
                    GeometryReader { geo in
                        Image(uiImage: snapshot)
                            .resizable()
                            .aspectRatio(contentMode: .fill)
                            .frame(width: geo.size.width, height: geo.size.height)
                    }
                } else {
                    Color.white
                }
            }
            .padding(.bottom, 56)
            .clipped()
            .allowsHitTesting(false)
            .contentShape(Rectangle())
            .onTapGesture {
                onSelect()
            }
            .layoutPriority(1)

            // Bottom title bar
            HStack(alignment: .center, spacing: 12) {
                // App icon
                ZStack(alignment: .topTrailing) {
                    Circle()
                        .fill(Color(hslString: tab.backgroundColor))
                        .frame(width: iconWidth, height: iconWidth)
                        .overlay {
                            if tab.status == .dead {
                                Image("service_stop")
                                    .font(.system(size: iconWidth))
                            } else if let icon = tab.icon, let url = URL(string: icon) {
                                KFImage(url)
                                    .resizable()
                                    .scaledToFit()
                                    .frame(width: iconWidth, height: iconWidth)
                                    .cornerRadius(iconWidth / 2)
                            } else {
                                Image(systemName: "plus.viewfinder")
                                    .font(.system(size: 14))
                                    .foregroundColor(.white.opacity(0.9))
                            }
                        }

                    // Unread indicator
                    if tab.hasUnreadMessage {
                        Circle()
                            .fill(Color.red)
                            .frame(width: 10, height: 10)
                            .overlay(
                                Circle()
                                    .stroke(Color(.systemBackground), lineWidth: 2)
                            )
                            .offset(x: 2, y: -2)
                    }
                }
                .frame(width: iconWidth, height: iconWidth)

                // Title
                Text(tab.status == .dead ? "\(tab.title)(\(TM.tabStatus.serverClosed()))" : tab.title)
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundColor(theme.isDarkMode ? Color(hex: "#ECECED") : Color(hex: "#404040"))
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .layoutPriority(1)

                Spacer(minLength: 0)

                // Bookmark button
                if tab.title != TM.active.blackTab() {
                    Button(action: {
                        toggleBookmark()
                    }) {
                        Image(systemName: "star.circle.fill")
                            .font(.system(size: 20))
                            .foregroundColor(tab.isBookmarked ? Color(hex: "#EAC15B") : Color(hex: "#9E9EA7"))
                            .frame(width: 36, height: 36)
                    }
                }

                // Close button
                Button(action: onClose) {
                    Image(systemName: "xmark.circle.fill")
                        .font(.system(size: 20))
                        .foregroundColor(Color(hex: "#9E9EA7"))
                        .frame(width: 36, height: 36)
                }
            }
            .frame(maxWidth: .infinity)
            .frame(height: 56)
            .padding(.horizontal, 14)
            .background(Color(hex: theme.isDarkMode ? "#2C2C2C" : "#E5E5EA"))
        }
        .clipShape(RoundedRectangle(cornerRadius: 16))
        .background(Color.black)
        .onAppear {
            // 内存警告回收快照后，卡片出现时按需从磁盘加载
            viewModel.loadSnapshotIfNeeded(for: tab.id)
        }
        .contextMenu {
            if tab.title != TM.active.blackTab() {
                Button {
                    toggleBookmark()
                } label: {
                    Label(tab.isBookmarked ? TM.tabContextMenu.unbookmark() : TM.tabContextMenu.bookmark(),
                          systemImage: tab.isBookmarked ? "star.slash" : "star")
                }
            }

            Button(role: .destructive) {
                onClose()
            } label: {
                Label(TM.tabContextMenu.closeTab(), systemImage: "xmark")
            }

            if viewModel.tabs.count > 1 {
                Divider()
                Button(role: .destructive) {
                    closeOtherTabs()
                } label: {
                    Label(TM.tabContextMenu.closeOtherTabs(), systemImage: "xmark")
                }
            }
        }
    }
}

// MARK: - New Tab Card (SwiftUI)
struct ChromeNewTabCardView: View {
    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: "plus")
                .font(.system(size: 40))
                .foregroundColor(.white.opacity(0.5))

            Text("New Tab")
                .font(.system(size: 15, weight: .medium))
                .foregroundColor(.white.opacity(0.5))
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .clipShape(RoundedRectangle(cornerRadius: 16))
        .overlay(
            RoundedRectangle(cornerRadius: 16)
                .stroke(Color.white.opacity(0.15), lineWidth: 1)
        )
        .background(Color.black)
    }
}

// MARK: - Focus View (Single Tab)
struct ChromeTabFocusView: View {
    let initialTabId: UUID
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel
    let onBackToOverview: () -> Void

    @State private var hoveredTabId: UUID?
    @State private var viewWidth: CGFloat = 390
    private let tabHeight: CGFloat = 44
    private let maxTabWidth: CGFloat = 240
    private let minTabWidth: CGFloat = 120

    /// 当前显示的 tab（跟随 viewModel.currentTabId 变化，实现 sheet 内切换 tab）
    private var currentTab: Tab? {
        if let tabId = viewModel.currentTabId {
            return viewModel.tabs.first(where: { $0.id == tabId })
        }
        return viewModel.tabs.first(where: { $0.id == initialTabId })
    }

    private func calculateTabWidth() -> CGFloat {
        let availableWidth = viewWidth - 44 - 44 - tabHeight
        let tabCount = CGFloat(viewModel.tabs.count)
        let calculatedWidth = availableWidth / max(tabCount, 1)
        return min(max(calculatedWidth, minTabWidth), maxTabWidth)
    }

    var body: some View {
        VStack(spacing: 0) {
            // 顶部导航栏
            FocusModeNavigationBar(
                viewModel: viewModel,
                hoveredTabId: $hoveredTabId,
                tabHeight: tabHeight,
                calculateTabWidth: calculateTabWidth,
                availableWidth: viewWidth,
                onBackToOverview: onBackToOverview
            )

            // 内容区域
            GeometryReader { geometry in
                if let tab = currentTab {
                    SingleTabRootView(
                        tab: tab,
                        viewModel: viewModel,
                        historyVM: historyVM,
                        containerSize: CGSize(
                            width: max(geometry.size.width, 1),
                            height: max(geometry.size.height, 1)
                        )
                    )
                    .frame(width: geometry.size.width, height: geometry.size.height)
                    .id(tab.id)
                    .onAppear {
                        viewWidth = geometry.size.width
                        // Tab 进入全屏视图，通知 RN 侧清除未读状态
                        NotificationCenter.default.post(
                            name: .tabEnteredFullscreenFromNative,
                            object: nil,
                            userInfo: tab.runtimeFullscreenEnteredUserInfo
                        )
                    }
                    .onDisappear {
                        // Tab 退出全屏视图
                        NotificationCenter.default.post(
                            name: .tabExitedFullscreenFromNative,
                            object: nil,
                            userInfo: tab.runtimeFullscreenExitedUserInfo
                        )
                    }
                    .onChange(of: geometry.size.width) { _, newWidth in
                        viewWidth = newWidth
                    }
                    .onChange(of: tab.id) { oldTabId, newTabId in
                        // Focus 模式下切换 tab 时，先退出旧 tab，再进入新 tab
                        NotificationCenter.default.post(
                            name: .tabExitedFullscreenFromNative,
                            object: nil,
                            userInfo: Tab.runtimeFullscreenUserInfo(tabId: oldTabId.uuidString)
                        )
                        NotificationCenter.default.post(
                            name: .tabEnteredFullscreenFromNative,
                            object: nil,
                            userInfo: Tab.runtimeFullscreenUserInfo(
                                tabId: newTabId.uuidString,
                                title: tab.title
                            )
                        )
                    }
                }
            }
        }
    }
}
