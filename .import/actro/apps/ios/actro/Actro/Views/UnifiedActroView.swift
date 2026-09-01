import SwiftUI
import Kingfisher

private let logger = AppLogger.ui
#if os(macOS)
import AppKit
typealias PlatformColor = NSColor
typealias PlatformFont = NSFont
#elseif os(iOS)
import UIKit
typealias PlatformColor = UIColor
typealias PlatformFont = UIFont
#endif
import UniformTypeIdentifiers

// MARK: - Main View

/// 大画布架构 - 统一的 Actro 视图
///
/// 核心概念：
/// - 所有 Tab 卡片都在一个大画布上，位置固定
/// - Overview 模式：缩小画布，显示所有卡片网格
/// - Focus 模式：放大画布，让目标卡片填满屏幕
/// - 只通过 scale + offset 变换实现模式切换
struct UnifiedActroView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel

    // Mode is stored in viewModel.actroMode to survive view recreation
    // (e.g., when DevTools layout changes from docked to detached)

    @State private var isACTRDropTarget: Bool = false
    @State private var showingBookmarkSheet: Bool = false

    // Toolbar 状态
    @State private var addressInputText: String = ""
    @State private var isAddressBarEditing: Bool = false
    @State private var isAddressBarFirstResponder: Bool = false
    @State private var eventMonitor: Any?

    // Search Tabs 状态
    @State private var searchTabsText: String = ""
    @FocusState private var isSearchTabsFocused: Bool

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
        GeometryReader { geometry in
            let viewportSize = geometry.size
            // DEBUG: 追踪 GeometryReader 提供的尺寸（用 print 确保可见）
            // 极窄/极矮窗口时避免 0 尺寸导致矩阵奇异
            let safeViewportSize = CGSize(
                width: max(viewportSize.width, 1),
                height: max(viewportSize.height, 1)
            )

            // 通过平台窗口计算真实的 toolbar 高度
            let measuredToolbarHeight: CGFloat = {
                #if os(macOS)
                guard let window = NSApp.keyWindow ?? NSApp.windows.first,
                      let contentView = window.contentView else {
                    return 52 // 默认值
                }
                let windowContentHeight = contentView.frame.height
                let calculated = windowContentHeight - viewportSize.height
                // 合理范围检查
                return (calculated > 30 && calculated < 100) ? calculated : 52
                #elseif os(iOS)
                // iOS 使用固定的导航栏高度
                return 44
                #endif
            }()

            // 使用测量的 toolbar 高度
            let toolbarHeight = measuredToolbarHeight
            let contentSize = safeViewportSize
            // 标题栏已移除，卡片比例 = 视口比例
            let titleBarRatio: CGFloat = 0
            let cardAspectRatio = safeViewportSize.width / safeViewportSize.height
            let gridResult = gridLayout(
                for: contentSize,
                cardAspectRatio: cardAspectRatio,
                cardCount: filteredTabs.count
            )
            let surfaceSize = gridResult.surfaceSize

            // 计算变换
            let transform: (scale: CGFloat, offset: CGPoint) = {
                switch viewModel.actroMode {
                case .overview:
                    return calculateOverviewTransform(
                        surfaceSize: surfaceSize,
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
                // 背景
                #if os(macOS)
                Color(nsColor: NSColor(white: 0.1, alpha: 0.95))
                #elseif os(iOS)
                Color.black
                #endif

                // 统一渲染 Actro（Overview 和 Focus 共用同一个视图，通过 transform 动画切换）
                // 每个 Tab 卡片包含独立的 RCTRootView，实现真正的缩放
                ActroView(
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
                    viewModel: viewModel,
                    historyVM: historyVM,
                    onSelectTab: { tabId in
                        switchToFocus(tabId: tabId)
                    },
                    onSwapPositions: { tabId1, tabId2 in
                        viewModel.swapSlotPositions(tabId1: tabId1, tabId2: tabId2)
                    },
                    onNewTab: {
                        // 新建按钮在 Overview 模式下只创建 tab，不切换到 Focus
                        viewModel.addNewTab(focus: viewModel.actroMode.isInFocusMode)
                    },
                    onCloseTab: { tabId in
                        withAnimation {
                            viewModel.closeTab(id: tabId)
                        }
                    }
                )
                .frame(width: surfaceSize.width, height: surfaceSize.height)
                // 防止 scale 为 0 导致奇异矩阵
                .scaleEffect(max(transform.scale, 0.001), anchor: .topLeading)
                .offset(x: transform.offset.x, y: transform.offset.y)
                // simultaneousGesture 不会与 UIKit（RN）手势识别器竞争，
                // 避免阻断 UITextField 等 UIResponder 链的触摸事件
                .simultaneousGesture(
                    TapGesture().onEnded {
                        if case .focus = viewModel.actroMode, isAddressBarEditing {
                            isAddressBarEditing = false
                            isAddressBarFirstResponder = false
                        }
                    }
                )

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

                            // 导入完成后自动切换到新 Tab 的 Focus 模式
                            if let newTabId = viewModel.currentTabId {
                                switchToFocus(tabId: newTabId)
                            }
                        }
                    }
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)

                // 注意：RN 内容层已移至每个 TabCardView 内部
                // 每个 Tab 都有独立的 RCTRootView，实现真正的缩放
            }
            .ignoresSafeArea() // 核心修改：允许内容堆格忽略安全区
            .toolbar {
                ToolbarItemGroup(placement: .principal) {
                    let totalWidth = geometry.size.width
                    let addressBarContainerWidth = totalWidth * 0.5

                    HStack {
                        Spacer(minLength: 0)

                        // Overview 模式：显示空状态；Focus 模式：显示地址栏
                        if case .overview = viewModel.actroMode {
                            // Overview 模式下的空占位
                            Color.clear
                                .frame(width: addressBarContainerWidth, height: 28)
                        } else {
                            AddressBarContainer(
                                titleText: .constant(viewModel.currentTab?.title ?? TM.active.blackTab()),
                                inputText: $addressInputText,
                                isEditing: $isAddressBarEditing,
                                isFirstResponder: $isAddressBarFirstResponder,
                                toggleLeftPanel: {
                                    withAnimation(.easeInOut(duration: 0.2)) {
                                        viewModel.toggleLeftPanelForCurrentTab()
                                    }
                                },
                                refreshAction: {
                                    viewModel.reloadCurrentTab()
                                }
                            )
                            .frame(width: addressBarContainerWidth)
                        }

                        Spacer(minLength: 0)
                    }
                    .frame(maxWidth: .infinity)
                }

                ToolbarItemGroup(placement: .primaryAction) {
                    Button(action: {}) {
                        Image(systemName: "square.and.arrow.up")
                    }
                    .help("分享")

                    Button(action: {
                        withAnimation(.spring(response: 0.35, dampingFraction: 1.0, blendDuration: 0)) {
                            // Toolbar 新建：在 Overview 下不自动进入 Focus
                            _ = viewModel.addNewTab(focus: viewModel.actroMode.isInFocusMode)
                        }
                    }) {
                        Image(systemName: "plus")
                    }
                    .help("新建标签页")

                    Button(action: toggleMode) {
                        Image(systemName: "square.grid.2x2")
                    }
                    .help("标签总览")
                }
            }
        }
        .ignoresSafeArea() // 确保获取全屏物理尺寸
        .onChange(of: viewModel.actroMode) { oldValue, newValue in
            // 进入 Overview 模式时自动聚焦搜索框
            if case .overview = newValue {
                // 延迟一帧，确保搜索框已渲染
                DispatchQueue.main.async {
                    isSearchTabsFocused = true
                }
            } else {
                // 离开 Overview 模式时清空搜索文本
                searchTabsText = ""
                isSearchTabsFocused = false
            }
        }
        .onAppear {
            // 启动时默认进入 Focus 模式（如果有 tab）
            if let tabId = viewModel.currentTabId {
                logger.notice("🚀 [UnifiedActroView] 启动时直接进入 Focus 模式")
                DispatchQueue.main.async {
                    withAnimation(.spring(response: 0.35, dampingFraction: 1.0, blendDuration: 0)) {
                        viewModel.actroMode = .focus(tabId: tabId)
                    }
                }
            }

            #if os(macOS)
            // 监听 ESC 键（仅 macOS）
            eventMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
                if event.keyCode == 53 { // ESC key code
                    if case .overview = viewModel.actroMode {
                        switchToFocus(tabId: viewModel.currentTabId ?? viewModel.tabs.first!.id)
                        return nil
                    }

                    if isAddressBarEditing {
                        isAddressBarEditing = false
                        isAddressBarFirstResponder = false
                        return nil
                    }
                }
                return event
            }
            #endif
        }
        .onDisappear {
            #if os(macOS)
            // 清理事件监听（仅 macOS）
            if let monitor = eventMonitor {
                NSEvent.removeMonitor(monitor)
            }
            #endif
        }
    }

    // MARK: - Mode Switching

    private func toggleMode() {
        if case .overview = viewModel.actroMode {
            // 从 Overview 切换到第一个 Tab
            if let firstTab = viewModel.tabs.first {
                switchToFocus(tabId: firstTab.id)
            }
        } else {
            // 从 Focus 切换到 Overview
            switchToOverview()
        }
    }

    private func switchToOverview() {
        guard case .focus = viewModel.actroMode else { return }

        withAnimation(.spring(response: 0.35, dampingFraction: 1.0, blendDuration: 0)) {
            viewModel.actroMode = .overview
        }
    }

    private func switchToFocus(tabId: UUID) {
        guard viewModel.actroMode != .focus(tabId: tabId) else { return }

        // 确保切换到 Focus 模式时，同时更新当前选中的 tab
        viewModel.currentTabId = tabId
        viewModel.actroMode = .focus(tabId: tabId)
    }

    // MARK: - Transform Calculations

    /// Overview 模式变换：缩放画布以适应视口，水平居中、顶端对齐
    private func calculateOverviewTransform(
        surfaceSize: CGSize,
        viewportSize: CGSize
    ) -> (scale: CGFloat, offset: CGPoint) {
        guard surfaceSize.width > 0 && surfaceSize.height > 0 else {
            return (1.0, .zero)
        }

        // 缩放：让 Actro 适应视口（不放大，只缩小）
        let scaleX = viewportSize.width / surfaceSize.width
        let scaleY = viewportSize.height / surfaceSize.height
        let scale = max(min(scaleX, scaleY, 1.0), 0.001)

        // 水平居中，顶端对齐
        let scaledWidth = surfaceSize.width * scale
        let offsetX = (viewportSize.width - scaledWidth) / 2
        let offsetY: CGFloat = 0  // 顶端对齐，不添加垂直偏移

        guard offsetX.isFinite && offsetY.isFinite && scale.isFinite else {
            return (1.0, .zero)
        }

        return (scale, CGPoint(x: offsetX, y: offsetY))
    }

    /// Focus 模式变换：放大画布，让目标卡片的内容区域填满视口
    private func calculateFocusTransform(
        for tabId: UUID,
        gridResult: (positions: [CGPoint], cardWidth: CGFloat, cardHeight: CGFloat, surfaceSize: CGSize),
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

        // 偏移：对齐左上角
        let offsetX = -cardContentRect.origin.x * scale
        let offsetY = -cardContentRect.origin.y * scale

        guard offsetX.isFinite && offsetY.isFinite && scale.isFinite else {
            return (1.0, .zero)
        }

        return (scale, CGPoint(x: offsetX, y: offsetY))
    }

    // MARK: - Grid Layout Calculations

    /// 底部服务条高度（Overview 模式下显示在卡片底部）
    private let floatingTitleBarHeight: CGFloat = 56

    /// 计算间距（卡片之间）和垂直边距
    /// - 间距 = windowWidth / (卡片数 × 7)，下限 16px
    /// - 垂直间距 = 水平间距（底部服务条在卡片内部，不需要额外空间）
    /// - 垂直边距 = 间距 × 1.25
    private func calculateSpacing(windowWidth: CGFloat, totalCards: Int) -> (hSpacing: CGFloat, vSpacing: CGFloat, verticalMargin: CGFloat) {
        let effectiveCards = max(totalCards, 4)
        let baseSpacing = max(16, windowWidth / (CGFloat(effectiveCards) * 7))
        let vSpacing = baseSpacing  // 纵向间距不需要额外高度
        let verticalMargin = baseSpacing * 1.25  // 顶部边距
        return (baseSpacing, vSpacing, verticalMargin)
    }

    /// 网格布局计算
    /// - 初始容量 4（2×2），卡片数 ≤ 4 时强制 2×2 布局
    /// - 水平边距 = spacing × 1.25（通过卡片尺寸计算保证）
    private func gridLayout(
        for size: CGSize,
        cardAspectRatio: CGFloat,
        cardCount: Int
    ) -> (positions: [CGPoint], cardWidth: CGFloat, cardHeight: CGFloat, surfaceSize: CGSize) {
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
        let surfaceSize = CGSize(
            width: size.width,
            height: verticalMargin * 2 + CGFloat(rows) * cardHeight + CGFloat(rows - 1) * vSpacing
        )

        return (positions, cardWidth, cardHeight, surfaceSize)
    }

}

// MARK: - Actro View

private struct ActroView: View {
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
    // 新增：用于创建 SingleTabRootView
    @ObservedObject var viewModel: BrowserViewModel
    var historyVM: HistoryViewModel?
    let onSelectTab: (UUID) -> Void
    let onSwapPositions: (UUID?, UUID?) -> Void
    let onNewTab: () -> Void
    let onCloseTab: (UUID) -> Void

    @State private var hoveringOverTabId: UUID?
    @State private var swappingTabId: UUID?  // 正在交换过程中的卡片（用于淡入动画）
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
        ZStack(alignment: .topLeading) {
            // 第一层：卡片层（可移动，带动画）
            ForEach(displayItems, id: \.item.id) { displayItem in
                let itemPosition = displayItem.position
                let position = positions[itemPosition]

                switch displayItem.item {
                case .tab(let tab):
                    let isHovering = hoveringOverTabId == tab.id
                    let isSwapping = swappingTabId == tab.id
                    let isFocusedCard: Bool = {
                        if case .focus(let id) = mode { return id == tab.id }
                        return false
                    }()
                    let isOverviewMode: Bool = {
                        if case .overview = mode { return true }
                        return false
                    }()
                    let baseCard = TabCardView(
                        tab: tab,
                        cardWidth: cardWidth,
                        cardHeight: cardHeight,
                        titleBarRatio: titleBarRatio,
                        toolbarHeight: toolbarHeight,
                        currentScale: currentScale,
                        isFocused: isFocusedCard,
                        isHovering: isHovering,
                        isSwapping: isSwapping,
                        viewModel: viewModel,
                        historyVM: historyVM,
                        viewportSize: viewportSize,
                        onSelect: { onSelectTab(tab.id) },
                        onClose: { onCloseTab(tab.id) }
                    )
                    Group {
                        // 拖拽排序仅在 Overview 模式下启用
                        // Focus 模式下所有卡片不附加拖拽手势，避免手势识别器拦截 RN 内容的触摸事件
                        if isOverviewMode {
                            baseCard
                                .draggable(tab.id.uuidString) {
                                    TabDragPreviewView(tab: tab, cardWidth: cardWidth, cardHeight: cardHeight)
                                }
                                .dropDestination(for: String.self) { items, _ in
                                    guard let sourceIdStr = items.first,
                                          let sourceId = UUID(uuidString: sourceIdStr),
                                          sourceId != tab.id else { return false }
                                    swappingTabId = tab.id
                                    withAnimation(.spring(response: 0.35, dampingFraction: 1.0, blendDuration: 0)) {
                                        onSwapPositions(sourceId, tab.id)
                                        hoveringOverTabId = nil
                                    }
                                    DispatchQueue.main.asyncAfter(deadline: .now() + 1.0) {
                                        swappingTabId = nil
                                        dropTargetSlotIndexFixed = nil
                                    }
                                    return true
                                } isTargeted: { targeted in
                                    if targeted {
                                        hoveringOverTabId = tab.id
                                        dropTargetSlotIndexFixed = tabSlotPositions[tab.id]
                                    } else if hoveringOverTabId == tab.id {
                                        hoveringOverTabId = nil
                                        dropTargetSlotIndexFixed = nil
                                    }
                                }
                        } else {
                            baseCard
                        }
                    }
                    .zIndex(isFocusedCard ? 1000 : Double(itemPosition))
                    .position(
                        x: position.x + cardWidth / 2,
                        y: position.y + cardHeight / 2
                    )

                case .newTabButton:
                    NewTabCardView(
                        cardWidth: cardWidth,
                        cardHeight: cardHeight,
                        onTap: onNewTab
                    )
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
                    isDropTarget: dropTargetSlotIndexFixed == slotIndex
                )
                .position(
                    x: positions[slotIndex].x + cardWidth / 2,
                    y: positions[slotIndex].y + cardHeight / 2
                )
                .zIndex(10000)  // 超高 zIndex，确保边框始终在最上层，不被任何卡片遮挡
                .animation(nil, value: dropTargetSlotIndexFixed)
                .allowsHitTesting(false)  // 边框不响应点击，让点击穿透到下层卡片
            }
        }
    }
}

// MARK: - Slot View (槽位视图，显示拖拽时的边框)

private struct SlotView: View {
    let cardWidth: CGFloat
    let cardHeight: CGFloat
    let isDropTarget: Bool  // 是否为目标槽位（显示实线边框）

    var body: some View {
        Rectangle()
            .fill(Color.clear)
            .frame(width: cardWidth, height: cardHeight)
            .overlay(
                Group {
                    if isDropTarget {
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

// MARK: - Address Bar Container

private struct AddressBarContainer: View {
    @Binding var titleText: String
    @Binding var inputText: String
    @Binding var isEditing: Bool
    @Binding var isFirstResponder: Bool
    var toggleLeftPanel: () -> Void
    var refreshAction: () -> Void

    private let placeholder = "Search from bookmark or enter app id"
    @State private var focusRingScale: CGFloat = 1.0
    @State private var focusRingOpacity: Double = 0.0

    var body: some View {
        Group {
            if isEditing {
                // 编辑状态：放大镜 + 输入框
                HStack(spacing: 10) {
                    Image(systemName: "magnifyingglass")
                        .foregroundStyle(Color.white.opacity(0.85))
                        .frame(width: 16, height: 16)

                    FocusableTextField(
                        text: $inputText,
                        placeholder: placeholder,
                        isFirstResponder: $isFirstResponder
                    )
                }
            } else {
                // 非编辑状态：左右图标 + 中间标题
                HStack(spacing: 10) {
                    Button(action: toggleLeftPanel) {
                        Image(systemName: "list.bullet")
                            .frame(width: 20, height: 20)
                    }
                    .buttonStyle(.borderless)
                    .help("切换左侧面板")

                    Text(titleText)
                        .foregroundColor(.white)
                        .multilineTextAlignment(.center)
                        .frame(maxWidth: .infinity)
                        .contentShape(Rectangle())
                        .onTapGesture {
                            if !isEditing {
                                isEditing = true
                                inputText = ""
                                DispatchQueue.main.async {
                                    isFirstResponder = true
                                }
                            }
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
        .onChange(of: isFirstResponder) {
            if !isFirstResponder && isEditing {
                isEditing = false
            }
        }
        .onChange(of: isEditing) {
            if isEditing {
                // 从略大的、透明的蓝圈收缩到正常大小并渐显
                focusRingScale = 1.08
                focusRingOpacity = 0.0
                withAnimation(.spring(response: 0.25, dampingFraction: 0.7, blendDuration: 0.1)) {
                    focusRingScale = 1.0
                    focusRingOpacity = 1.0
                }
            } else {
                withAnimation(.easeOut(duration: 0.15)) {
                    focusRingOpacity = 0.0
                }
            }
        }
    }

    private func endEditing() {
        isEditing = false
        isFirstResponder = false
    }
}

// MARK: - Focusable TextField

#if os(macOS)
private struct FocusableTextField: NSViewRepresentable {
    @Binding var text: String
    var placeholder: String
    @Binding var isFirstResponder: Bool

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
    }
}
#elseif os(iOS)
private struct FocusableTextField: UIViewRepresentable {
    @Binding var text: String
    var placeholder: String
    @Binding var isFirstResponder: Bool

    func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }

    func makeUIView(context: Context) -> UITextField {
        let field = UITextField()
        field.borderStyle = .none
        field.backgroundColor = .clear
        field.font = UIFont.systemFont(ofSize: UIFont.systemFontSize)
        field.placeholder = placeholder
        field.delegate = context.coordinator
        field.returnKeyType = .search
        field.autocorrectionType = .no
        field.autocapitalizationType = .none
        return field
    }

    func updateUIView(_ uiView: UITextField, context: Context) {
        if uiView.text != text {
            uiView.text = text
        }
        uiView.placeholder = placeholder

        if isFirstResponder && !uiView.isFirstResponder {
            uiView.becomeFirstResponder()
        } else if !isFirstResponder && uiView.isFirstResponder {
            uiView.resignFirstResponder()
        }
    }

    final class Coordinator: NSObject, UITextFieldDelegate {
        var parent: FocusableTextField

        init(_ parent: FocusableTextField) {
            self.parent = parent
        }

        func textField(_ textField: UITextField, shouldChangeCharactersIn range: NSRange, replacementString string: String) -> Bool {
            // 过滤掉换行符
            if string.contains("\n") {
                textField.resignFirstResponder()
                return false
            }
            return true
        }

        func textFieldDidChangeSelection(_ textField: UITextField) {
            parent.text = textField.text ?? ""
        }

        func textFieldDidEndEditing(_ textField: UITextField) {
            parent.isFirstResponder = false
        }
    }
}
#endif

// MARK: - Tab Card View

private struct TabCardView: View {
    let tab: Tab
    let cardWidth: CGFloat
    let cardHeight: CGFloat
    let titleBarRatio: CGFloat  // 精确的标题栏高度比例
    let toolbarHeight: CGFloat  // toolbar 实际高度，用作缩放基准
    let currentScale: CGFloat
    let isFocused: Bool
    let isHovering: Bool
    let isSwapping: Bool  // 正在交换过程中（用于淡入动画）
    // 新增：用于创建 SingleTabRootView
    @ObservedObject var viewModel: BrowserViewModel
    var historyVM: HistoryViewModel?
    let viewportSize: CGSize  // 视口尺寸，用于 RN 渲染
    let onSelect: () -> Void
    let onClose: () -> Void

    // 底部服务条高度（固定值，Overview 模式下显示在卡片底部）
    private let floatingTitleBarHeight: CGFloat = 56
    private let iconWidth: CGFloat = 32

    @ViewBuilder
    var body: some View {
        // 使用 ZStack 允许服务条在卡片底部
        let base = ZStack(alignment: .bottom) {
            // 卡片主体（纯内容 + 底部服务条）
            // 圆角仅 Overview 模式显示，Focus 模式不要圆角
            VStack(spacing: 0) {
                // 内容预览区
                contentView
                    .frame(width: cardWidth, height: cardHeight - floatingTitleBarHeight)

                // 底部服务条 - 仅 Overview 模式显示（Focus 模式隐藏）
                if !isFocused {
                    floatingTitleBarView
                        .frame(width: cardWidth, height: floatingTitleBarHeight)
                }
            }
            .frame(width: cardWidth, height: cardHeight)
            .cornerRadius(isFocused ? 0 : 22)
            .shadow(color: isFocused ? .clear : Color.black.opacity(0.1), radius: 8, x: 0, y: 2)
        }
        // hover 时半透明，交换过程中淡入到完全不透明
        .opacity({
            if isSwapping {
                return 1.0
            } else if isHovering && !isFocused {
                return 0.5
            } else {
                return 1.0
            }
        }())

        base
    }

    // 底部服务条视图（参考 PhoneTabListView 中的布局）
    private var floatingTitleBarView: some View {
        HStack(alignment: .center, spacing: 12) {
            // 应用图标
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
                                .cornerRadius(iconWidth/2)
                        } else {
                            Image(systemName: "plus.viewfinder")
                                .font(.system(size: 14))
                                .foregroundColor(.white.opacity(0.9))
                        }
                    }

                // 未读消息小红点
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

            // 标题
            Text(tab.title)
                .font(.system(size: 16, weight: .semibold))
                .foregroundColor(Color(hex: "#404040"))
                .lineLimit(1)

            Spacer()

            // 收藏星（标题为空时隐藏）
            if tab.title != TM.active.blackTab() {
                Button(action: {
                    // 切换收藏状态并发送通知给 RN
                    let newBookmarkStatus = !tab.isBookmarked

                    // 立即更新本地状态（UI 响应）
                    if let index = viewModel.tabs.firstIndex(where: { $0.id == tab.id }) {
                        viewModel.tabs[index].isBookmarked = newBookmarkStatus
                        viewModel.scheduleSaveTabs() // 持久化变更
                    }

                    // 发送通知给 RN 侧处理实际的收藏逻辑
                    NotificationCenter.default.post(
                        name: .tabBookmarkChangedFromNative,
                        object: nil,
                        userInfo: tab.runtimeBookmarkChangedUserInfo(isBookmarked: newBookmarkStatus)
                    )
                }) {
                    Image(systemName: "star.circle.fill")
                        .font(.system(size: 20))
                        .foregroundColor(tab.isBookmarked ? Color(hex: "#EAC15B") : Color(hex: "#9E9EA7"))
                        .frame(width: 36, height: 36)
                        .background(Circle().fill(Color.clear))
                }
            }

            // 关闭按钮
            Button(action: onClose) {
                Image(systemName: "xmark.circle.fill")
                    .font(.system(size: 20))
                    .foregroundColor(Color(hex: "#9E9EA7"))
                    .frame(width: 36, height: 36)
            }
        }
        .frame(height: floatingTitleBarHeight)
        .padding(.horizontal, 14)
        .background(Color(hex: "#E5E5EA"))
    }

    // 卡片内容区域尺寸（现在等于完整卡片尺寸，因为标题栏悬浮在外部）
    private var cardContentSize: CGSize {
        CGSize(width: cardWidth, height: cardHeight)
    }

    // RN 渲染尺寸（使用视口尺寸，宽高比与 cardContentSize 匹配）
    private var rnRenderSize: CGSize {
        CGSize(width: viewportSize.width, height: viewportSize.height)
    }

    // RN 视图需要缩放的比例
    // RN 始终按 viewportSize 渲染（保持高清），然后缩放到 cardContentSize
    // 在 Focus 模式下，外层 surface scale 会把卡片放大回 viewportSize
    // 最终效果：RN 内容以 1:1 比例显示
    private var rnScaleFactor: CGFloat {
        guard currentScale > 0 else { return 1 }
        let baseScale = cardContentSize.width / max(viewportSize.width, 1)
        return baseScale
    }

    // 内容区域视图
    @ViewBuilder
    private var contentView: some View {
        let content = ZStack {
            // 底层背景 - 使用 Tab 的随机背景颜色
            Color(hslString: tab.backgroundColor)
            
            // 每个 Tab 都有独立的 RCTRootView - 实现真正的缩放
            // 关键修复：使用 overlay 而非 frame+scaleEffect 组合
            // 原来的方式 scaleEffect 后用 frame 对齐会有问题，因为 SwiftUI 使用原始尺寸计算位置
            // 新方式：用固定尺寸的容器，overlay 放置缩放后的内容
            SingleTabRootView(
                tab: tab,
                viewModel: viewModel,
                historyVM: historyVM,
                containerSize: CGSize(
                    width: max(rnRenderSize.width, 1),
                    height: max(rnRenderSize.height, 1)
                )
            )
            .frame(
                width: max(rnRenderSize.width, 1),
                height: max(rnRenderSize.height, 1)
            )
            .scaleEffect(rnScaleFactor, anchor: .center)
        }
            .frame(maxWidth: cardContentSize.width, maxHeight: cardContentSize.height)
            .contentShape(Rectangle())
            .clipped()

        if isFocused {
            content
        } else {
            content.onTapGesture {
                onSelect()
            }
        }
    }
}

// MARK: - Tab Drag Preview View

private struct TabDragPreviewView: View {
    let tab: Tab
    let cardWidth: CGFloat
    let cardHeight: CGFloat
    private let floatingTitleBarHeight: CGFloat = 56
    private let iconWidth: CGFloat = 32

    var body: some View {
        VStack(spacing: 0) {
            ZStack {
                Color(hslString: tab.backgroundColor)
                Image(systemName: "globe")
                    .font(.system(size: 32))
                    .foregroundColor(.white.opacity(0.3))
            }
            .frame(width: cardWidth, height: cardHeight - floatingTitleBarHeight)

            HStack(alignment: .center, spacing: 12) {
                // 应用图标
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

                // 标题
                Text(tab.title)
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundColor(Color(hex: "#404040"))
                    .lineLimit(1)

                Spacer()
            }
            .frame(height: floatingTitleBarHeight)
            .padding(.horizontal, 14)
            .background(Color(hex: "#E5E5EA"))
        }
        .frame(width: cardWidth, height: cardHeight)
        .cornerRadius(22)
        .shadow(color: Color.black.opacity(0.5), radius: 10, x: 0, y: 5)
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

private struct NewTabCardView: View {
    let cardWidth: CGFloat
    let cardHeight: CGFloat
    var onTap: (() -> Void)? = nil

    var body: some View {
        ZStack {
            #if os(macOS)
            Rectangle()
                .fill(Color(nsColor: NSColor(white: 0.15, alpha: 1.0)))
            #elseif os(iOS)
            Rectangle()
                .fill(Color(uiColor: UIColor(white: 0.15, alpha: 1.0)))
            #endif

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

#Preview {
    UnifiedActroView(viewModel: BrowserViewModel(), historyVM: HistoryViewModel())
}
