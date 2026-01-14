import SwiftUI

/// 浏览器风格的标签页导航栏
struct TabNavigationBar: View {
    @ObservedObject var viewModel: BrowserViewModel
    @Binding var showAllTabs: Bool
    
    @State private var hoveredTabId: UUID?
    @State private var draggedTabId: UUID?
    
    private let tabHeight: CGFloat = 36
    private let maxTabWidth: CGFloat = 240
    private let minTabWidth: CGFloat = 120
    
    private var isInOverviewMode: Bool {
        if case .overview = viewModel.canvasMode {
            return true
        }
        return false
    }
    
    var body: some View {
        if isInOverviewMode {
            // Overview 模式的导航栏
            OverviewNavigationBar(onBack: {
                toggleOverviewMode()
            })
        } else {
            // Focus 模式的导航栏（标签页列表）
            FocusModeNavigationBar(
                viewModel: viewModel,
                hoveredTabId: $hoveredTabId,
                tabHeight: tabHeight,
                calculateTabWidth: calculateTabWidth,
                toggleOverviewMode: toggleOverviewMode
            )
        }
    }
    
    private func calculateTabWidth() -> CGFloat {
        let availableWidth = UIScreen.main.bounds.width - 44 - 44 - 36 // 减去左右按钮和新建按钮的宽度
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
        } else {
            // 从 Focus 切换到 Overview
            withAnimation(.spring(response: 0.35, dampingFraction: 1.0, blendDuration: 0)) {
                viewModel.canvasMode = .overview
            }
        }
    }
}

/// Overview 模式的导航栏
private struct OverviewNavigationBar: View {
    let onBack: () -> Void
    @State private var selectedTab: OverviewTab = .allTabs
    
    enum OverviewTab {
        case allTabs, bookmarks
    }
    
    var body: some View {
        HStack(spacing: 0) {
            // 左侧：返回按钮
            Button(action: onBack) {
                HStack(spacing: 6) {
                    Image(systemName: "chevron.left")
                        .font(.system(size: 14, weight: .semibold))
                    Text("返回")
                        .font(.system(size: 15))
                }
                .foregroundColor(.white)
                .padding(.horizontal, 16)
            }
            .buttonStyle(.plain)
            
            Spacer()
            
            // 中间：所有页签/书签切换
            HStack(spacing: 0) {
                Button(action: {
                    selectedTab = .allTabs
                }) {
                    HStack(spacing: 6) {
                        Image(systemName: "square.grid.2x2")
                            .font(.system(size: 14))
                        Text("所有页签")
                            .font(.system(size: 14))
                    }
                    .foregroundColor(.white)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 6)
                    .background(
                        selectedTab == .allTabs ? Color.white.opacity(0.2) : Color.clear
                    )
                    .cornerRadius(8)
                }
                .buttonStyle(.plain)
                
                Button(action: {
                    selectedTab = .bookmarks
                    // TODO: 实现书签页切换
                }) {
                    HStack(spacing: 6) {
                        Image(systemName: "bookmark")
                            .font(.system(size: 14))
                        Text("书签")
                            .font(.system(size: 14))
                    }
                    .foregroundColor(.white)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 6)
                    .background(
                        selectedTab == .bookmarks ? Color.white.opacity(0.2) : Color.clear
                    )
                    .cornerRadius(8)
                }
                .buttonStyle(.plain)
            }
            .padding(4)
            .background(Color(white: 0.2).opacity(0.5))
            .cornerRadius(10)
            
            Spacer()
            
            // 右侧：设置按钮
            Button(action: {
                // TODO: 打开设置
            }) {
                HStack(spacing: 6) {
                    Image(systemName: "gearshape")
                        .font(.system(size: 14))
                    Text("设置")
                        .font(.system(size: 15))
                }
                .foregroundColor(.white)
                .padding(.horizontal, 16)
            }
            .buttonStyle(.plain)
        }
        .frame(height: 36)
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
    
    var body: some View {
        HStack(spacing: 0) {
            // 左侧：显示所有标签页按钮
            Button(action: toggleOverviewMode) {
                Image(systemName: "house")
                    .font(.system(size: 16))
                    .foregroundColor(.primary)
                    .frame(width: 44, height: tabHeight)
            }
            .buttonStyle(.plain)
            .background(Color(white: 0.15))
            
            // 中间：标签页列表
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 0) {
                    ForEach(viewModel.tabs) { tab in
                        TabItemView(
                            tab: tab,
                            isActive: tab.id == viewModel.currentTabId,
                            isHovered: tab.id == hoveredTabId,
                            width: calculateTabWidth(),
                            onSelect: {
                                viewModel.switchToTab(id: tab.id)
                            },
                            onClose: {
                                if viewModel.tabs.count > 1 {
                                    viewModel.closeTab(id: tab.id)
                                }
                            }
                        )
                        .onHover { hovering in
                            hoveredTabId = hovering ? tab.id : nil
                        }
                    }
                    
                    // 新建标签按钮
                    Button(action: {
                        let _ = viewModel.addNewTab()
                    }) {
                        Image(systemName: "plus")
                            .font(.system(size: 14, weight: .medium))
                            .foregroundColor(.secondary)
                            .frame(width: 36, height: tabHeight)
                    }
                    .buttonStyle(.plain)
                    .background(Color(white: 0.15))
                }
            }
            .frame(maxWidth: .infinity)
            .background(Color(white: 0.15))
            
            // 右侧：设置按钮
            Button(action: {
                // TODO: 打开设置
            }) {
                Image(systemName: "gearshape")
                    .font(.system(size: 16))
                    .foregroundColor(.primary)
                    .frame(width: 44, height: tabHeight)
            }
            .buttonStyle(.plain)
            .background(Color(white: 0.15))
        }
        .frame(height: tabHeight)
        .background(Color(white: 0.15))
    }
}

/// 单个标签页项视图
private struct TabItemView: View {
    let tab: Tab
    let isActive: Bool
    let isHovered: Bool
    let width: CGFloat
    let onSelect: () -> Void
    let onClose: () -> Void
    
    @State private var isCloseHovered = false
    
    var body: some View {
        HStack(spacing: 8) {
            // 标签标题
            Text(tab.title)
                .font(.system(size: 13))
                .foregroundColor(isActive ? .primary : .secondary)
                .lineLimit(1)
                .truncationMode(.tail)
                .frame(maxWidth: .infinity, alignment: .leading)
            
            // 关闭按钮
            Button(action: onClose) {
                Image(systemName: isCloseHovered ? "xmark.circle.fill" : "xmark")
                    .font(.system(size: 12))
                    .foregroundColor(isCloseHovered ? .white : .secondary)
            }
            .buttonStyle(.plain)
            .opacity(isActive || isHovered ? 1 : 0)
            .onHover { hovering in
                isCloseHovered = hovering
            }
        }
        .padding(.horizontal, 12)
        .frame(width: width, height: 32)
        .background(
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .fill(isActive ? Color(white: 0.25) : Color(white: 0.18))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .stroke(isActive ? Color.accentColor.opacity(0.5) : Color.clear, lineWidth: 1)
        )
        .contentShape(Rectangle())
        .onTapGesture {
            onSelect()
        }
        .padding(.horizontal, 4)
        .padding(.vertical, 2)
    }
}

#Preview {
    let vm = BrowserViewModel()
    return TabNavigationBar(viewModel: vm, showAllTabs: .constant(false))
        .frame(height: 36)
}
