import SwiftUI

/// 浏览器风格的标签页导航栏
struct TabNavigationBar: View {
    @ObservedObject var viewModel: BrowserViewModel
    @Binding var showAllTabs: Bool
    
    @State private var hoveredTabId: UUID?
    @State private var draggedTabId: UUID?
    @State private var showSettings: Bool = false
    
    private let tabHeight: CGFloat = 44
    private let maxTabWidth: CGFloat = 240
    private let minTabWidth: CGFloat = 120
    
    private var isInOverviewMode: Bool {
        if case .overview = viewModel.canvasMode {
            return true
        }
        return false
    }
    
    var body: some View {
        ZStack {
            if isInOverviewMode {
                // Overview 模式的导航栏
                OverviewNavigationBar(
                    viewModel: viewModel,
                    onBack: {toggleOverviewMode()},
                    height: tabHeight, openSettings: { showSettings = true }
                )
            } else {
                // Focus 模式的导航栏（标签页列表）
                FocusModeNavigationBar(
                    viewModel: viewModel,
                    hoveredTabId: $hoveredTabId,
                    tabHeight: tabHeight,
                    calculateTabWidth: calculateTabWidth,
                    toggleOverviewMode: toggleOverviewMode,
                    openSettings: { showSettings = true }
                )
            }
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
    
    private func calculateTabWidth() -> CGFloat {
        let availableWidth = UIScreen.main.bounds.width - 44 - 44 - tabHeight // 减去左右按钮和新建按钮的宽度
        let tabCount = CGFloat(viewModel.tabs.count)
        let calculatedWidth = availableWidth / tabCount
        return min(max(calculatedWidth, minTabWidth), maxTabWidth)
    }
    
    private func toggleOverviewMode() {
        if case .overview = viewModel.canvasMode {
            // 从 Overview 切换到当前选中的 Tab
            if let currentTabId = viewModel.currentTabId {
                withAnimation(.spring(response: 0.35, dampingFraction: 1.0, blendDuration: 0)) {
                    viewModel.canvasMode = .focus(tabId: currentTabId)
                }
            } else if let firstTab = viewModel.tabs.first {
                withAnimation(.spring(response: 0.35, dampingFraction: 1.0, blendDuration: 0)) {
                    viewModel.canvasMode = .focus(tabId: firstTab.id)
                    viewModel.currentTabId = firstTab.id
                }
            }
            viewModel.currentSystemTabType = nil
        } else {
            // 从 Focus 切换到 Overview
            viewModel.canvasMode = .overview
        }
    }
}

/// Overview 模式的导航栏（中间改为“活跃”，图标在上文字在下，选中使用 accentColor，未选中使用 gray）
private struct OverviewNavigationBar: View {
    @ObservedObject var viewModel: BrowserViewModel
    let onBack: () -> Void
    let height: CGFloat
    let openSettings: () -> Void
    @State private var selectedTab: OverviewTab = .active

    enum OverviewTab {
        case active, bookmarks
    }

    var body: some View {
        HStack(spacing: 0) {
            // 左侧：返回按钮（文字与图标为白色）
            Button(action: onBack) {
                HStack(spacing: 6) {
                    Image(systemName: "chevron.left")
                        .font(.system(size: 20, weight: .regular))
                    Text("返回")
                        .font(.system(size: 20, weight: .regular))
                }
                .foregroundColor(.white)
                .padding(.horizontal, 18)
            }
            .buttonStyle(.plain)

            Spacer()

            // 中间：活跃 / 书签 切换（SwitchButton 风格）
            // 图标在上、文字在下；选中使用 accentColor，未选中使用 gray；高亮背景在选中项下平滑移动
            GeometryReader { geo in
                let totalWidth = min(geo.size.width, 144)
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
                            VStack(spacing: 4) {
                                Image(systemName: "rectangle.grid.1x2.fill")
                                    .font(.system(size: 14))
                                Text("活跃")
                                    .font(.system(size: 8))
                            }
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
                            VStack(spacing: 4) {
                                Image(systemName: "book.fill")
                                    .font(.system(size: 14))
                                Text("书签")
                                    .font(.system(size: 8))
                            }
                            .frame(width: segmentWidth, height: segmentHeight)
                            .foregroundColor(selectedTab == .bookmarks ? Color.accentColor : Color.gray)
                        }
                        .buttonStyle(.plain)
                    }
                    .frame(width: totalWidth, height: height)
                }
                .frame(width: totalWidth, height: height)
            }
            .frame(width: 144, height: height)

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
        .frame(height: height)
        .background(Color(white: 0.15))
    }
}

/// Focus 模式的导航栏
private struct FocusModeNavigationBar: View {
    @ObservedObject var viewModel: BrowserViewModel
    @Binding var hoveredTabId: UUID?
    let tabHeight: CGFloat
    let calculateTabWidth: () -> CGFloat
    let toggleOverviewMode: () -> Void
    let openSettings: () -> Void
    
    // 计算是否需要滚动
    private var needsScrolling: Bool {
        let availableWidth = UIScreen.main.bounds.width - 44 - 44 - tabHeight // 减去左侧按钮、右侧按钮、新建按钮
        let tabWidth = calculateTabWidth()
        let totalTabsWidth = CGFloat(viewModel.tabs.count) * (tabWidth + 2) // 每个 tab 加上间距
        return totalTabsWidth > availableWidth
    }
    
    var body: some View {
        HStack(spacing: 0) {
            // 左侧：显示所有标签页按钮
            Button(action: toggleOverviewMode) {
                Image(systemName: "rectangle.grid.2x2.fill")
                    .font(.system(size: 16))
                    .foregroundColor(.primary)
                    .frame(width: 44, height: tabHeight)
            }
            .buttonStyle(.plain)
            .background(Color(hex: "#D1D1D6"))
            
            // 中间：标签页列表
            if needsScrolling {
                // 需要滚动时：ScrollView，添加按钮固定在外部
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 0) {
                        tabsContent
                    }
                }
                .frame(maxWidth: .infinity)
                .background(Color(hex: "#D1D1D6"))
                
                // 新建标签按钮（固定在右侧）
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
            } else {
                // 不需要滚动时：添加按钮紧跟在最后一个页签后
                HStack(spacing: 0) {
                    tabsContent
                    
                    // 新建标签按钮（紧挨最后一个页签）
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
                    
                    Spacer()
                }
                .frame(maxWidth: .infinity)
                .background(Color(hex: "#D1D1D6"))
            }
            
            // 右侧：设置按钮
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
        .frame(height: tabHeight)
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
private struct TabItemView: View {
    let tab: Tab
    let isActive: Bool
    let isHovered: Bool
    let width: CGFloat
    let tabHeight: CGFloat
    let onSelect: () -> Void
    let onClose: () -> Void
        
    var body: some View {
        HStack(spacing: 8) {
            // 标签标题
            Text(tab.title.isEmpty ? "新页签" : tab.title)
                .font(.system(size: 13))
                .foregroundColor(isActive ? .primary : .secondary)
                .lineLimit(1)
                .truncationMode(.tail)
                .frame(maxWidth: .infinity, alignment: .leading)
            
            // 关闭按钮
            Button(action: onClose) {
                Image(systemName: "xmark")
                    .font(.system(size: 12))
                    .foregroundColor(isActive ? .primary : .secondary)
            }
            .buttonStyle(.plain)
        }
        .padding(.horizontal, 12)
        .frame(width: width, height: tabHeight) // 高度与 tabHeight 一致，填满导航栏
        .background(
            // 使用 UnevenRoundedRectangle 实现顶部圆角 14pt，底部无圆角
            UnevenRoundedRectangle(
                topLeadingRadius: 14,
                bottomLeadingRadius: 0,
                bottomTrailingRadius: 0,
                topTrailingRadius: 14
            )
            .fill(isActive ? Color.white : Color(hex: "#E6E6E6"))
        )
        .contentShape(Rectangle())
        .onTapGesture {
            onSelect()
        }
        .padding(.horizontal, 1) // 左右间距 2pt (1pt + 1pt)
        .padding(.bottom, 0) // 底部无间距，紧贴导航栏底部
    }
}

#Preview {
    let vm = BrowserViewModel()
    return TabNavigationBar(viewModel: vm, showAllTabs: .constant(false))
        .frame(height: 44)
}
