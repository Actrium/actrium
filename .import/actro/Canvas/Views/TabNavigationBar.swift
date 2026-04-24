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

            // 中间：活跃 / 书签 切换（SwitchButton 风格）
            GeometryReader { geo in
                let totalWidth = min(geo.size.width, 164)
                let segmentHeight: CGFloat = height
                let segmentCount: CGFloat = 2
                let segmentWidth = totalWidth / segmentCount
                ZStack(alignment: .center) {
                    // 背景大胶囊
                    Capsule()
                        .fill(Color(hex: "#3C3C4399"))
                        .frame(width: totalWidth, height: segmentHeight)

                    // 可移动的选中高亮胶囊
                    Capsule()
                        .fill(Color(hex: "#121216"))
                        .frame(width: segmentWidth - 8, height: segmentHeight - 8)
                        .offset(x: selectedTab == .active ? -segmentWidth/2 : segmentWidth/2)
                        .animation(.spring(response: 0.28, dampingFraction: 0.9), value: selectedTab)

                    // 按钮堆栈
                    HStack(spacing: 0) {
                        Button(action: {
                            selectedTab = .active
                            viewModel.currentSystemTabType = nil
                        }) {
                            Text(TM.active.activity())
                                .font(.system(size: 16, weight: .medium))
                                .frame(width: segmentWidth, height: segmentHeight)
                                .foregroundColor(selectedTab == .active ? Color.accentColor : Color.gray)
                        }
                        .buttonStyle(.plain)

                        Button(action: {
                            selectedTab = .bookmarks
                            // 切换到书签页（系统 Tab）
                            viewModel.openSystemTab(.bookmarks, additionalProps: nil, replaceAdditionalProps: true)
                            viewModel.currentSystemTabType = .bookmarks
                        }) {
                            Text(TM.active.bookmarks())
                                .font(.system(size: 16, weight: .medium))
                                .frame(width: segmentWidth, height: segmentHeight)
                                .foregroundColor(selectedTab == .bookmarks ? Color.accentColor : Color.gray)
                        }
                        .buttonStyle(.plain)
                    }
                    .frame(width: totalWidth, height: height)
                }
                .frame(width: totalWidth, height: height)
            }
            .frame(width: 164, height: height)
        }
        .frame(height: height)
        .background(Color.black)
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

    // 计算是否需要滚动
    private var needsScrolling: Bool {
        let tabAreaWidth = availableWidth - 44 - 44 - tabHeight // 减去左侧按钮、右侧按钮、新建按钮
        let tabWidth = calculateTabWidth()
        let totalTabsWidth = CGFloat(viewModel.tabs.count) * (tabWidth + 2) // 每个 tab 加上间距
        return totalTabsWidth > tabAreaWidth
    }

    var body: some View {
        ZStack(alignment: .trailing) {
            tabListArea
            trailingButtonsArea
        }
        .frame(height: tabHeight)
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
        .background(Color(hex: "#D1D1D6"))
    }

    private var overviewButton: some View {
        Button(action: onBackToOverview) {
            Image(systemName: "rectangle.grid.2x2.fill")
                .font(.system(size: 16))
                .foregroundColor(.primary)
                .frame(width: 44, height: tabHeight)
        }
        .buttonStyle(.plain)
        .background(Color(hex: "#D1D1D6"))
    }

    private var scrollingTabs: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 0) {
                tabsContent
                Color.clear.frame(width: 100)
            }
        }
        .frame(maxWidth: .infinity)
        .background(Color(hex: "#D1D1D6"))
    }

    private var staticTabsWithAddButton: some View {
        HStack(spacing: 0) {
            tabsContent
            addTabButton
            Spacer()
        }
        .frame(maxWidth: .infinity)
    }

    private var addTabButton: some View {
        Button(action: {
            let newTab = viewModel.addNewTab()
            viewModel.switchToTab(id: newTab.id)
        }) {
            Image(systemName: "plus")
                .font(.system(size: 14, weight: .medium))
                .foregroundColor(.primary)
                .frame(width: tabHeight, height: tabHeight)
        }
        .buttonStyle(.plain)
        .background(Color(hex: "#D1D1D6"))
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
                Color(hex: "#D1D1D6")
            ]),
            startPoint: .leading,
            endPoint: .trailing
        )
        .frame(width: 50)
        .allowsHitTesting(false)
    }

    private var scrollingAddTabButton: some View {
        Button(action: {
            let newTab = viewModel.addNewTab(focus: false)
            viewModel.switchToTab(id: newTab.id)
        }) {
            Image(systemName: "plus")
                .font(.system(size: 14, weight: .medium))
                .foregroundColor(.primary)
                .frame(width: tabHeight, height: tabHeight)
        }
        .buttonStyle(.plain)
        .background(Color(hex: "#D1D1D6"))
    }

    private var leftPanelToggleButton: some View {
        Button(action: {
            viewModel.toggleLeftPanelForCurrentTab()
        }) {
            Image(systemName: "line.3.horizontal")
                .font(.system(size: 16))
                .foregroundColor(.primary)
                .frame(width: 44, height: tabHeight)
        }
        .buttonStyle(.plain)
        .background(Color(hex: "#D1D1D6"))
    }

    // 标签页内容
    @ViewBuilder
    private var tabsContent: some View {
        ForEach(viewModel.tabs) { tab in
            TabItemView(
                tab: tab,
                isActive: tab.id == viewModel.currentTabId,
                isHovered: tab.id == hoveredTabId,
                width: calculateTabWidth(),
                tabHeight: tabHeight,
                onSelect: {
                    viewModel.switchToTab(id: tab.id)
                },
                onClose: {
                    viewModel.closeTab(id: tab.id)
                }
            )
            .onHover { hovering in
                hoveredTabId = hovering ? tab.id : nil
            }
            .transition(.move(edge: .leading).combined(with: .opacity))
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
    let onSelect: () -> Void
    let onClose: () -> Void

    var body: some View {
        tabContent
            .padding(.horizontal, 12)
            .frame(width: width, height: tabHeight)
            .background(tabBackground)
            .contentShape(Rectangle())
            .onTapGesture {
                onSelect()
            }
            .padding(.horizontal, 1)
            .padding(.bottom, 0)
    }

    private var tabContent: some View {
        HStack(spacing: 8) {
            Text(tab.title.isEmpty ? TM.active.blackTab() : tab.title)
                .font(.system(size: 13))
                .foregroundColor(isActive ? .primary : .secondary)
                .lineLimit(1)
                .truncationMode(.tail)
                .frame(maxWidth: .infinity, alignment: .leading)

            Button(action: onClose) {
                Image(systemName: "xmark")
                    .font(.system(size: 12))
                    .foregroundColor(isActive ? .primary : .secondary)
            }
            .buttonStyle(.plain)
        }
    }

    private var tabBackground: some View {
        UnevenRoundedRectangle(
            topLeadingRadius: 14,
            bottomLeadingRadius: 0,
            bottomTrailingRadius: 0,
            topTrailingRadius: 14
        )
        .fill(isActive ? Color.white : Color(hex: "#E6E6E6"))
    }
}
