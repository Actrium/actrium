//
//  ChromeStyleTabView.swift
//  Canvas
//
//  Chrome-style tab grid view for iPad
//  - Adaptive grid columns based on container width
//  - Zoom transition between overview and focused modes via .sheet
//

import SwiftUI
import Kingfisher

// MARK: - Chrome Style Tab View
struct ChromeStyleTabView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel

    @State private var focusedTab: Tab?
    @Namespace private var animation

    var body: some View {
        overviewView
            .fullScreenCover(item: $focusedTab) { tab in
                focusSheetContent(tab: tab)
            }
            .onAppear { syncFocusedTabWithCanvasMode() }
            .onChange(of: viewModel.canvasMode) { oldValue, newValue in
                handleCanvasModeChange(newValue)
            }
            .onChange(of: viewModel.tabs.count) { oldValue, newValue in
                handleTabsCountChange()
            }
    }

    private var overviewView: some View {
        ChromeTabOverviewView(
            tabs: viewModel.tabsSortedBySlotPosition().map { $0.tab },
            viewModel: viewModel,
            historyVM: historyVM,
            animation: animation,
            onSelectTab: { tabId in
                guard let tab = viewModel.tabs.first(where: { $0.id == tabId }) else { return }
                viewModel.currentTabId = tabId
                viewModel.canvasMode = .focus(tabId: tabId)
                focusedTab = tab
            },
            onNewTab: {
                viewModel.addNewTab(focus: false)
            },
            onCloseTab: { tabId in
                withAnimation {
                    viewModel.closeTab(id: tabId)
                }
            }
        )
        .background(Color.black)
        .environment(\.colorScheme, .dark)
    }

    private func focusSheetContent(tab: Tab) -> some View {
        ChromeTabFocusView(
            initialTabId: tab.id,
            viewModel: viewModel,
            historyVM: historyVM,
            animation: animation,
            onBackToOverview: {
                viewModel.canvasMode = .overview
                focusedTab = nil
            }
        )
        .interactiveDismissDisabled(true)
        .asNavigationTransition(id: tab.id, in: animation)
    }

    private func syncFocusedTabWithCanvasMode() {
        if case .focus(let tabId) = viewModel.canvasMode,
           let tab = viewModel.tabs.first(where: { $0.id == tabId }) {
            focusedTab = tab
        }
    }

    private func handleCanvasModeChange(_ newValue: CanvasMode) {
        switch newValue {
        case .overview:
            focusedTab = nil
        case .focus(let tabId):
            // 仅在从 overview 进入 focus 时设置 focusedTab（弹出 sheet）
            // focus 内切换 tab 时不应更新 focusedTab，避免 sheet 收起再弹出
            if focusedTab == nil,
               let tab = viewModel.tabs.first(where: { $0.id == tabId }) {
                focusedTab = tab
            }
        }
    }

    private func handleTabsCountChange() {
        if let focused = focusedTab,
           !viewModel.tabs.contains(where: { $0.id == focused.id }) {
            focusedTab = nil
        }
    }
}

// MARK: - Overview View (Tab Grid)
struct ChromeTabOverviewView: View {
    let tabs: [Tab]
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel
    var animation: Namespace.ID

    let onSelectTab: (UUID) -> Void
    let onNewTab: () -> Void
    let onCloseTab: (UUID) -> Void

    @State private var hoveringOverTabId: UUID?
    @State private var showSettings: Bool = false

    private let cardAspectRatio: CGFloat = 3.0 / 4.0
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
                        viewModel.canvasMode = .focus(tabId: tab.id)
                    } else if let firstTab = viewModel.tabs.first {
                        viewModel.currentTabId = firstTab.id
                        viewModel.canvasMode = .focus(tabId: firstTab.id)
                    }
                },
                openSettings: { showSettings = true }
            )
            .environment(\.colorScheme, .dark)

            // 卡片网格
            cardGridView
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .sheet(isPresented: $showSettings) {
            SettingsView(systemType: .settings, viewModel: viewModel)
        }
        .onReceive(NotificationCenter.default.publisher(for: .openTabFromRN)) { notification in
            guard let userInfo = notification.userInfo,
                  let systemAppTypeStr = userInfo["systemAppType"] as? String,
                  let systemTabType = SystemTabType(rawValue: systemAppTypeStr) else { return }
            let additionalProps = userInfo["additionalProps"] as? [String: Any]
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

            ScrollView {
                LazyVGrid(
                    columns: Array(repeating: GridItem(
                        .flexible(minimum: 140),
                        spacing: interItemSpacing
                    ), count: columns),
                    spacing: lineSpacing
                ) {
                    ForEach(tabs) { tab in
                        let isHovering = hoveringOverTabId == tab.id
                        ChromeTabCardView(
                            tab: tab,
                            viewModel: viewModel,
                            onSelect: { onSelectTab(tab.id) },
                            onClose: { onCloseTab(tab.id) }
                        )
                        .aspectRatio(cardAspectRatio, contentMode: .fit)
                        .asMatchedTransitionSource(id: tab.id, in: animation)
                        .opacity(isHovering ? 0.5 : 1.0)
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
                        .onTapGesture {
                            onNewTab()
                        }
                }
                .padding(.horizontal, sideInset)
                .padding(.top, 8)
                .padding(.bottom, 16)
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

                Text((tab.title.isEmpty || tab.title == "counterapp") ? "" : tab.title)
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundColor(Color(hex: "#404040"))
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

    private let iconWidth: CGFloat = 32.0

    var body: some View {
        ZStack(alignment: .bottom) {
            // Content preview area
            Rectangle()
                .fill(Color(hslString: tab.backgroundColor).opacity(0.1))
                .overlay(alignment: .topLeading) {
                    if let snapshot = viewModel.tabSnapshots[tab.id] {
                        Image(uiImage: snapshot)
                            .resizable()
                            .aspectRatio(contentMode: .fill)
                    } else {
                        Color.white
                    }
                }
                .clipped()
                .allowsHitTesting(false)
                .contentShape(Rectangle())
                .onTapGesture {
                    onSelect()
                }

            // Bottom title bar
            HStack(alignment: .center, spacing: 12) {
                // App icon
                ZStack(alignment: .topTrailing) {
                    Circle()
                        .fill(Color(hslString: tab.backgroundColor))
                        .frame(width: iconWidth, height: iconWidth)
                        .overlay {
                            if let icon = tab.icon, let url = URL(string: icon) {
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
                Text((tab.title.isEmpty || tab.title == "counterapp") ? "" : tab.title)
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundColor(Color(hex: "#404040"))
                    .lineLimit(1)

                Spacer()

                // Bookmark button
                if !tab.title.isEmpty && tab.title != "counterapp" {
                    Button(action: {
                        NotificationCenter.default.post(
                            name: .tabBookmarkChangedFromNative,
                            object: nil,
                            userInfo: [
                                "tabId": tab.id.uuidString,
                                "isBookmarked": !tab.isBookmarked
                            ]
                        )
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
            .frame(height: 56)
            .padding(.horizontal, 14)
            .background(Color(white: 0.9))
        }
        .clipShape(RoundedRectangle(cornerRadius: 16))
        .overlay(
            RoundedRectangle(cornerRadius: 16)
                .stroke(Color.white.opacity(0.1), lineWidth: 1)
        )
        .background(Color(white: 0.15))
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
        .background(Color(white: 0.12))
    }
}

// MARK: - Focus View (Single Tab)
struct ChromeTabFocusView: View {
    let initialTabId: UUID
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel
    var animation: Namespace.ID
    let onBackToOverview: () -> Void

    @State private var hoveredTabId: UUID?
    @State private var viewWidth: CGFloat = UIScreen.main.bounds.width
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
                    }
                    .onChange(of: geometry.size.width) { _, newWidth in
                        viewWidth = newWidth
                    }
                }
            }
        }
    }
}
