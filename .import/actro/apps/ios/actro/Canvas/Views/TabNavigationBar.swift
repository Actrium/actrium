import SwiftUI

/// Overview 模式的导航栏（中间改为"活跃"，图标在上文字在下，选中使用 accentColor，未选中使用 gray）
struct OverviewNavigationBar: View {
    @ObservedObject var viewModel: BrowserViewModel
    let onBack: () -> Void
    let openSettings: () -> Void
    @State private var selectedTab: OverviewTab = .active

    private let height: CGFloat = 44

    enum OverviewTab {
        case active, bookmarks
    }

    var body: some View {
        ZStack(alignment: .center) {
            HStack(spacing: 0) {
                // 左侧：返回按钮（文字与图标为白色）
                Button(action: onBack) {
                    HStack(spacing: 6) {
                        Image(systemName: "chevron.left")
                            .font(.system(size: 20, weight: .regular))
                        Text(TM.active.goback())
                            .font(.system(size: 20, weight: .regular))
                    }
                    .foregroundColor(.white)
                    .padding(.horizontal, 18)
                }
                .buttonStyle(.plain)

                Spacer()

                // 右侧：仅显示设置图标（白色）
                Button(action: {
                    openSettings()
                }) {
                    Image(systemName: "gearshape.fill")
                        .font(.system(size: 20, weight: .regular))
                        .foregroundColor(.white)
                        .padding(.horizontal, 21.5)
                        .frame(height: height)
                }
                .buttonStyle(.plain)
            }
            .padding(.leading, viewModel.multipleWindowMode ? 0 : 65)

            // 中间：活跃 / 书签 切换（SwitchButton 风格）
            SegmentedToggle(
                selectedTab: $selectedTab,
                option1: TM.active.activity(),
                option2: TM.active.bookmarks(),
                onOption1: {
                    viewModel.currentSystemTabType = nil
                },
                onOption2: {
                    viewModel.reloadSystemTab(.bookmarks)
                    viewModel.openSystemTab(.bookmarks, additionalProps: nil, replaceAdditionalProps: true)
                    viewModel.currentSystemTabType = .bookmarks
                }
            )
        }
        .frame(height: height)
        .background(Color.black)
        .onChange(of: viewModel.canvasMode) { newMode in
            if case .overview = newMode {
                selectedTab = viewModel.currentSystemTabType == .bookmarks ? .bookmarks : .active
            }
        }
    }
}

/// Focus 模式的导航栏
struct FocusModeNavigationBar: View {
    @ObservedObject var viewModel: BrowserViewModel
    @Binding var hoveredTabId: UUID?
    let tabHeight: CGFloat
    let calculateTabWidth: () -> CGFloat
    let availableWidth: CGFloat
    let onBackToOverview: () -> Void

    /// 导航栏背景色
    private let barColor: Color = Color(hex: "#E0E0E0")

    /// 记录上一次标签数量，用于判断是否为新增标签
    @State private var previousTabCount: Int = 0

    // 计算是否需要滚动
    private var needsScrolling: Bool {
        let tabAreaWidth = availableWidth - 44 - 44 - tabHeight // 减去左侧按钮、右侧按钮、新建按钮
        let tabWidth = calculateTabWidth()
        let totalTabsWidth = CGFloat(viewModel.tabs.count) * (tabWidth + 2) // 每个 tab 加上间距
        return totalTabsWidth > tabAreaWidth
    }

    /// 选中 tab 在 tabs 列表中的索引
    private var selectedTabIndex: Int? {
        guard let currentId = viewModel.currentTabId else { return nil }
        return viewModel.tabs.firstIndex(where: { $0.id == currentId })
    }

    /// 选中 tab 左边缘相对于导航栏起始位置的 x 坐标
    private var selectedTabLeftX: CGFloat? {
        guard let index = selectedTabIndex else { return nil }
        let tabWidth = calculateTabWidth()
        // index * (tabWidth + 2) + 1(padding) + 19(leading)
        return CGFloat(index) * (tabWidth + 2) + 1 + 19
    }

    var body: some View {
        ZStack(alignment: .trailing) {
            tabListArea
            trailingButtonsArea
        }
        .frame(height: tabHeight)
    }

    /// 安全获取 tabWidth
    private var calculateTabWidthSafe: CGFloat? {
        let width = calculateTabWidth()
        return width > 0 ? width : nil
    }

    // 左侧按钮 + 中间标签页列表
    private var tabListArea: some View {
        HStack(spacing: 0) {
            overviewButton

            // 中间：标签页列表
            if needsScrolling {
                scrollingTabs
            } else {
                staticTabsWithAddButton
            }
        }
        .padding(.leading, viewModel.multipleWindowMode ? 0 : 65)
        .background(barColor)
    }

    private var overviewButton: some View {
        Button(action: onBackToOverview) {
            Image(systemName: "rectangle.grid.2x2.fill")
                .font(.system(size: 16))
                .foregroundColor(.black)
                .frame(width: 44, height: tabHeight)
        }
        .buttonStyle(.plain)
        .background(barColor)
    }

    private var scrollingTabs: some View {
        ScrollViewReader { proxy in
            ScrollView(.horizontal, showsIndicators: false) {
                ZStack(alignment: .topLeading) {
                    HStack(spacing: 0) {
                        Color.clear.frame(width: 19)
                        tabsContent
                        Color.clear.frame(width: 120)
                            .id("trailingSpacer")
                    }
                    // 选中 tab 底部的两个反圆角遮罩（盖在 tab 上层）
                    if let leftX = selectedTabLeftX, let tabWidth = calculateTabWidthSafe, let currentTabId = viewModel.currentTabId {
                        let cornerSize: CGFloat = 19
                        let cornerColor: Color = Color(hex: "#F2F2F6")
                        // 左下反圆角：右边缘对齐选中 tab 左边缘，底部对齐导航栏底部
                        BottomLeftInverseCorner(radius: cornerSize, color: cornerColor)
                            .frame(width: cornerSize, height: cornerSize)
                            .offset(x: leftX - cornerSize, y: tabHeight - cornerSize + 2)
                            .id("leftCorner_\(currentTabId)")
                            .transition(.asymmetric(
                                insertion: .move(edge: .bottom).combined(with: .opacity),
                                removal: .identity
                            ))

                        // 右下反圆角：左边缘对齐选中 tab 右边缘，底部对齐导航栏底部
                        BottomRightInverseCorner(radius: cornerSize, color: cornerColor)
                            .frame(width: cornerSize, height: cornerSize)
                            .offset(x: leftX + tabWidth, y: tabHeight - cornerSize + 2)
                            .id("rightCorner_\(currentTabId)")
                            .transition(.asymmetric(
                                insertion: .move(edge: .bottom).combined(with: .opacity),
                                removal: .identity
                            ))
                    }
                }
            }
            .frame(maxWidth: .infinity)
            .background(barColor)
            .onChange(of: viewModel.currentTabId) { newTabId in
                let isTabAdded = viewModel.tabs.count > previousTabCount
                previousTabCount = viewModel.tabs.count
                guard newTabId != nil, isTabAdded else { return }
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.35) {
                    withAnimation {
                        proxy.scrollTo("trailingSpacer", anchor: .trailing)
                    }
                }
            }
            .onAppear {
                previousTabCount = viewModel.tabs.count
            }
        }
    }

    private var staticTabsWithAddButton: some View {
        ZStack(alignment: .topLeading) {
            HStack(spacing: 0) {
                Color.clear.frame(width: 19)
                tabsContent
                Spacer()
            }

            // 选中 tab 底部的两个反圆角遮罩（盖在 tab 上层）
            if let leftX = selectedTabLeftX, let tabWidth = calculateTabWidthSafe, let currentTabId = viewModel.currentTabId {
                let cornerSize: CGFloat = 19
                let cornerColor: Color = Color(hex: "#F2F2F6")
                // 左下反圆角
                BottomLeftInverseCorner(radius: cornerSize, color: cornerColor)
                    .frame(width: cornerSize, height: cornerSize)
                    .offset(x: leftX - cornerSize, y: tabHeight - cornerSize + 2)
                    .id("leftCorner_\(currentTabId)")
                    .transition(.asymmetric(
                        insertion: .move(edge: .bottom).combined(with: .opacity),
                        removal: .identity
                    ))

                // 右下反圆角
                BottomRightInverseCorner(radius: cornerSize, color: cornerColor)
                    .frame(width: cornerSize, height: cornerSize)
                    .offset(x: leftX + tabWidth, y: tabHeight - cornerSize + 2)
                    .id("rightCorner_\(currentTabId)")
                    .transition(.asymmetric(
                        insertion: .move(edge: .bottom).combined(with: .opacity),
                        removal: .identity
                    ))
            }
        }
        .frame(maxWidth: .infinity)
    }

    private var addTabButton: some View {
        Button(action: {
            withAnimation(.easeOut(duration: 0.3)) {
                let newTab = viewModel.addNewTab()
                viewModel.switchToTab(id: newTab.id)
            }
        }) {
            Image(systemName: "plus")
                .font(.system(size: 14, weight: .medium))
                .foregroundColor(.black)
                .frame(width: tabHeight, height: tabHeight)
        }
        .buttonStyle(.plain)
        .background(barColor)
    }

    private var trailingButtonsArea: some View {
        HStack(spacing: 0) {
            if needsScrolling {
                scrollFadeGradient
                scrollingAddTabButton
            }
            leftPanelToggleButton
        }
    }

    private var scrollFadeGradient: some View {
        LinearGradient(
            gradient: Gradient(colors: [
                Color.clear,
                barColor
            ]),
            startPoint: .leading,
            endPoint: .trailing
        )
        .frame(width: 50)
        .allowsHitTesting(false)
    }

    private var scrollingAddTabButton: some View {
        Button(action: {
            withAnimation(.easeOut(duration: 0.3)) {
                let newTab = viewModel.addNewTab(focus: false)
                viewModel.switchToTab(id: newTab.id)
            }
        }) {
            Image(systemName: "plus")
                .font(.system(size: 14, weight: .medium))
                .foregroundColor(.black)
                .frame(width: tabHeight, height: tabHeight)
        }
        .buttonStyle(.plain)
        .background(barColor)
    }

    private var leftPanelToggleButton: some View {
        Button(action: {
            viewModel.toggleLeftPanelForCurrentTab()
        }) {
            Image(systemName: "line.3.horizontal")
                .font(.system(size: 16))
                .foregroundColor(.black)
                .frame(width: 44, height: tabHeight)
        }
        .buttonStyle(.plain)
        .background(barColor)
    }

    // 标签页内容
    @ViewBuilder
    private var tabsContent: some View {
        ForEach(Array(viewModel.tabs.enumerated()), id: \.element.id) { index, tab in
            TabItemView(
                tab: viewModel.tabs[index],
                isActive: viewModel.tabs[index].id == viewModel.currentTabId,
                isHovered: viewModel.tabs[index].id == hoveredTabId,
                width: calculateTabWidth(),
                tabHeight: tabHeight,
                tabsCount: viewModel.tabs.count,
                onSelect: {
                    viewModel.switchToTab(id: viewModel.tabs[index].id)
                },
                onClose: {
                    viewModel.closeTab(id: viewModel.tabs[index].id)
                },
                onToggleBookmark: {
                    let newBookmarkStatus = !viewModel.tabs[index].isBookmarked
                    viewModel.tabs[index].isBookmarked = newBookmarkStatus
                    viewModel.scheduleSaveTabs()
                    NotificationCenter.default.post(
                        name: .tabBookmarkChangedFromNative,
                        object: nil,
                        userInfo: [
                            "tabId": viewModel.tabs[index].id.uuidString,
                            "isBookmarked": newBookmarkStatus
                        ]
                    )
                },
                onCloseOtherTabs: {
                    viewModel.closeOtherTabs(keepTabId: viewModel.tabs[index].id)
                }
            )
            .onHover { hovering in
                hoveredTabId = hovering ? viewModel.tabs[index].id : nil
            }
            .transition(.move(edge: .bottom).combined(with: .opacity))
            
            if index == viewModel.tabs.count - 1, !needsScrolling {
                addTabButton
            }
        }
    }
}

/// 单个标签页项视图
struct TabItemView: View {
    let tab: Tab
    let isActive: Bool
    let isHovered: Bool
    let width: CGFloat
    let tabHeight: CGFloat
    let tabsCount: Int
    let onSelect: () -> Void
    let onClose: () -> Void
    let onToggleBookmark: () -> Void
    let onCloseOtherTabs: () -> Void

    var body: some View {
        tabContent
            .padding(.horizontal, 12)
            .frame(width: width, height: tabHeight + (isActive ? 2 : 0))
            .background(
                UnevenRoundedRectangle(
                    topLeadingRadius: 14,
                    bottomLeadingRadius: 0,
                    bottomTrailingRadius: 0,
                    topTrailingRadius: 14
                )
                .fill(isActive ? Color(hex: "#F2F2F6") : Color(hex: "#EAEAEA"))
            )
            .contentShape(Rectangle())
            .onTapGesture {
                onSelect()
            }
            .contextMenu {
                if tab.title != TM.active.blackTab() {
                    Button {
                        onToggleBookmark()
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

                if tabsCount > 1 {
                    Divider()
                    Button(role: .destructive) {
                        onCloseOtherTabs()
                    } label: {
                        Label(TM.tabContextMenu.closeOtherTabs(), systemImage: "xmark")
                    }
                }
            }
            .padding(.horizontal, 1)
            .padding(.bottom, 0)
    }

    private var tabContent: some View {
        HStack(spacing: 8) {
            Text(tab.title.isEmpty ? TM.active.blackTab() : (tab.status == .dead ? "\(tab.title)(\(TM.tabStatus.serverClosed()))" : tab.title))
                .font(.system(size: 13))
                .foregroundColor(isActive ? .black : Color(hex: "#9E9EA7"))
                .lineLimit(1)
                .truncationMode(.tail)
                .frame(maxWidth: .infinity, alignment: .leading)

            Button(action: onClose) {
                Image(systemName: "xmark")
                    .font(.system(size: 12))
                    .foregroundColor(isActive ? .black : Color(hex: "#9E9EA7"))
            }
            .buttonStyle(.plain)
        }
    }
}

/// 左下反圆角遮罩：填充导航栏背景色，遮住选中 tab 左下角
/// 路径对应 SVG: M0 19 C9.46 16.85 16.85 9.46 19 0 V19 H0
struct BottomLeftInverseCorner: View {
    let radius: CGFloat
    let color: Color

    var body: some View {
        Canvas { context, size in
            let w = size.width
            let h = size.height
            let path = Path { p in
                p.move(to: CGPoint(x: 0, y: h))
                p.addCurve(
                    to: CGPoint(x: w, y: 0),
                    control1: CGPoint(x: w * 0.498, y: h * 0.887),
                    control2: CGPoint(x: w * 0.887, y: h * 0.498)
                )
                p.addLine(to: CGPoint(x: w, y: h))
                p.closeSubpath()
            }
            context.fill(path, with: .color(color))
        }
    }
}

/// 右下反圆角遮罩：填充导航栏背景色，遮住选中 tab 右下角
/// 路径对应 SVG: M19 19 C9.54 16.85 2.15 9.46 0 0 V19 H19
struct BottomRightInverseCorner: View {
    let radius: CGFloat
    let color: Color

    var body: some View {
        Canvas { context, size in
            let w = size.width
            let h = size.height
            let path = Path { p in
                p.move(to: CGPoint(x: w, y: h))
                p.addCurve(
                    to: CGPoint(x: 0, y: 0),
                    control1: CGPoint(x: w * 0.502, y: h * 0.887),
                    control2: CGPoint(x: w * 0.113, y: h * 0.498)
                )
                p.addLine(to: CGPoint(x: 0, y: h))
                p.closeSubpath()
            }
            context.fill(path, with: .color(color))
        }
    }
}

// MARK: - 自适应宽度的分段切换控件

/// 用于收集子视图宽度的 PreferenceKey
private struct SegmentWidthPreferenceKey: PreferenceKey {
    static var defaultValue: [CGFloat] = []
    static func reduce(value: inout [CGFloat], nextValue: () -> [CGFloat]) {
        value.append(contentsOf: nextValue())
    }
}

/// 自适应文字宽度的两段式切换控件
/// 文字宽度自适应，不会折行，整体居中显示
struct SegmentedToggle: View {
    @Binding var selectedTab: OverviewNavigationBar.OverviewTab
    let option1: String
    let option2: String
    let onOption1: () -> Void
    let onOption2: () -> Void

    @State private var segmentWidths: [CGFloat] = [0, 0]
    private let height: CGFloat = 44
    private let horizontalPadding: CGFloat = 20

    private var segmentWidth: CGFloat {
        max(segmentWidths[0], segmentWidths[1])
    }

    private var totalWidth: CGFloat {
        segmentWidth * 2
    }

    var body: some View {
        ZStack(alignment: .center) {
            // 背景大胶囊
            Capsule()
                .fill(Color(hex: "#3C3C4399"))
                .frame(width: totalWidth, height: height)

            // 可移动的选中高亮胶囊
            Capsule()
                .fill(Color(hex: "#121216"))
                .frame(width: segmentWidth - 8, height: height - 8)
                .offset(x: selectedTab == .active ? -segmentWidth/2 : segmentWidth/2)
                .animation(.spring(response: 0.28, dampingFraction: 0.9), value: selectedTab)

            // 按钮堆栈
            HStack(spacing: 0) {
                Button(action: {
                    selectedTab = .active
                    onOption1()
                }) {
                    Text(option1)
                        .font(.system(size: 16, weight: .medium))
                        .lineLimit(1)
                        .fixedSize()
                        .padding(.horizontal, horizontalPadding)
                        .frame(height: height)
                        .foregroundColor(selectedTab == .active ? Color.accentColor : Color.gray)
                }
                .buttonStyle(.plain)
                .background(
                    GeometryReader { geo in
                        Color.clear.preference(
                            key: SegmentWidthPreferenceKey.self,
                            value: [geo.size.width]
                        )
                    }
                )

                Button(action: {
                    selectedTab = .bookmarks
                    onOption2()
                }) {
                    Text(option2)
                        .font(.system(size: 16, weight: .medium))
                        .lineLimit(1)
                        .fixedSize()
                        .padding(.horizontal, horizontalPadding)
                        .frame(height: height)
                        .foregroundColor(selectedTab == .bookmarks ? Color.accentColor : Color.gray)
                }
                .buttonStyle(.plain)
                .background(
                    GeometryReader { geo in
                        Color.clear.preference(
                            key: SegmentWidthPreferenceKey.self,
                            value: [geo.size.width]
                        )
                    }
                )
            }
            .frame(width: totalWidth, height: height)
        }
        .frame(width: totalWidth, height: height)
        .onPreferenceChange(SegmentWidthPreferenceKey.self) { widths in
            if widths.count >= 2 {
                segmentWidths = [widths[0], widths[1]]
            }
        }
    }
}
