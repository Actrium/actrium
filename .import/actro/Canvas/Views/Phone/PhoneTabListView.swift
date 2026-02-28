import SwiftUI
import Kingfisher

struct PhoneTabListView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel
    @ObservedObject var bookmarkVM: BookmarkViewModel
    var onSelect: (Tab) -> Void
    
    var body: some View {
        NavigationStack {
            GeometryReader { geometry in
                ZStack(alignment: .top) {
                    Color.black.ignoresSafeArea()
                    
                    PhoneNavigationBar(title: "AskAway", showLargeTitle: true, rightItems: {
                        ZStack {
                            Circle().fill(Color.accent)
                            Image(systemName: "plus")
                                .renderingMode(.template)
                                .font(.system(size: 19))
                                .frame(width: 44, height: 44)
                                .foregroundColor(.white)
                                .onTapGesture {
                                    let newTab = viewModel.addNewTab()
                                    onSelect(newTab)
                                }
                        }
                        .asGlassClearEffect(.accent)
                        .asCornerRadius(22)
                    })
                    .ignoresSafeArea(edges: .top)
                    .zIndex(10)
                    
                    ScrollView {
                        // 顶部间距：适应自定义导航栏
                        Color.clear.frame(height: 140)
                        
                        LazyVStack(spacing: 16) {
                            ForEach(viewModel.tabs) { tab in
                                PhoneTabCardView(
                                    tab: tab,
                                    viewModel: viewModel,
                                    historyVM: historyVM,
                                    bookmarkVM: bookmarkVM,
                                    screenSize: geometry.size,
                                    isSelected: viewModel.currentTabId == tab.id,
                                    onSelect: {
                                        // 立即切换 ID，确保 UI 响应（蓝色边框）
                                        withAnimation(.easeInOut(duration: 0.2)) {
                                            viewModel.currentTabId = tab.id
                                        }
                                        // 延迟一小会儿后进入全屏，让用户看到选中效果
                                        DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) {
                                            onSelect(tab)
                                        }
                                    },
                                    onClose: {
                                        withAnimation(.spring(response: 0.35, dampingFraction: 0.8)) {
                                            viewModel.closeTab(id: tab.id)
                                        }
                                    }
                                )
                            }
                        }
                        .padding(.horizontal, 16)
                    }
                    .ignoresSafeArea()
                }
            }
            .navigationBarHidden(true)
        }
    }
}

// 子组件：页签卡片，参考 ASActiveItem 布局
struct PhoneTabCardView: View {
    let tab: Tab
    @ObservedObject var viewModel: BrowserViewModel
    var historyVM: HistoryViewModel
    var bookmarkVM: BookmarkViewModel
    let screenSize: CGSize
    let isSelected: Bool
    let onSelect: () -> Void
    let onClose: () -> Void
    
    private let iconWidth = 32.0
    
    var body: some View {
        ZStack(alignment: .bottom) {
            // 内容预览区
            Rectangle()
                .fill(Color(hslString: tab.backgroundColor).opacity(0.1))
                .overlay(alignment: .topLeading) {
                    Group {
                        if let snapshot = viewModel.tabSnapshots[tab.id] {
                            Image(uiImage: snapshot)
                                .resizable()
                                .aspectRatio(contentMode: .fill)
                        } else if isSelected {
                            // 如果是当前选中的，但没快照，显示 Live View
                            GeometryReader { cardContentGeo in
                                let cardWidth = cardContentGeo.size.width
                                let scale = cardWidth / max(screenSize.width, 0.1)
                                
                                SingleTabRootView(
                                    tab: tab,
                                    viewModel: viewModel,
                                    historyVM: historyVM,
                                    bookmarkVM: bookmarkVM,
                                    containerSize: screenSize
                                )
                                .frame(width: screenSize.width, height: screenSize.height)
                                .scaleEffect(scale, anchor: .topLeading)
                            }
                        } else {
                            // 默认占位图标
                            VStack(spacing: 8) {
                                Circle()
                                    .fill(Color(hslString: tab.backgroundColor))
                                    .frame(width: 60, height: 60)
                                    .overlay {
                                        Image(systemName: "safari")
                                            .font(.system(size: 24))
                                            .foregroundColor(.white.opacity(0.8))
                                    }
                                if let host = tab.url?.host() {
                                    Text(host)
                                        .font(.system(size: 12))
                                        .foregroundColor(.secondary)
                                }
                            }
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                        }
                    }
                }
                .clipped()
                .allowsHitTesting(false)
                .contentShape(Rectangle())
                .onTapGesture {
                    onSelect()
                }
            
            // 底部服务条
            HStack(alignment: .center, spacing: 12) {
                // 应用图标
                ZStack(alignment: .topTrailing) {
                    Circle()
                        .fill(Color(hslString: tab.backgroundColor))
                        .frame(width: iconWidth, height: iconWidth)
                        .overlay {
                            Image(systemName: "safari")
                                .font(.system(size: 14))
                                .foregroundColor(.white.opacity(0.9))
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
                VStack(alignment: .leading, spacing: 2) {
                    Text(tab.title.isEmpty ? "新标签页" : tab.title)
                        .font(.system(size: 16, weight: .semibold))
                        .foregroundColor(.primary)
                        .lineLimit(1)
                    
                    if let host = tab.url?.host() {
                        Text(host)
                            .font(.system(size: 13))
                            .foregroundColor(.secondary)
                            .lineLimit(1)
                    }
                }
                
                Spacer()
                
                // 收藏星
                Button(action: {
                    // 切换收藏状态并发送通知给 RN
                    let newBookmarkStatus = !tab.isBookmarked
                    
                    // 立即更新本地状态（UI 响应）
                    if let index = viewModel.tabs.firstIndex(where: { $0.id == tab.id }) {
                        viewModel.tabs[index].isBookmarked = newBookmarkStatus
                    }
                    
                    // 发送通知给 RN 侧处理实际的收藏逻辑
                    NotificationCenter.default.post(
                        name: .tabBookmarkChangedFromNative,
                        object: nil,
                        userInfo: [
                            "tabId": tab.id.uuidString,
                            "isBookmarked": newBookmarkStatus
                        ]
                    )
                }) {
                    Image(systemName: "star.circle.fill")
                        .font(.system(size: 20))
                        .foregroundColor(tab.isBookmarked ? Color(hex: "#EAC15B") : Color(hex: "#404040"))
                        .frame(width: 36, height: 36)
                        .background(Circle().fill(Color.clear))
                }
                
                // 关闭按钮
                Button(action: onClose) {
                    Image(systemName: "xmark.circle.fill")
                        .font(.system(size: 20))
                        .foregroundColor(.primary)
                        .frame(width: 36, height: 36)
                }
            }
            .frame(height: 56)
            .padding(.horizontal, 14)
            .background(Color(.secondarySystemGroupedBackground).opacity(0.95))
        }
        .frame(height: (UIScreen.main.bounds.width - 32) * 0.54)
        .background(Color(.systemBackground))
        .clipShape(RoundedRectangle(cornerRadius: 22))
        .overlay(
            RoundedRectangle(cornerRadius: 22)
                .stroke(isSelected ? Color.blue : Color.clear, lineWidth: isSelected ? 3 : 0)
        )
        .shadow(color: Color.black.opacity(0.1), radius: 8, x: 0, y: 2)
    }
}

struct PhoneBrowserView: View {
    let tab: Tab
    @ObservedObject var viewModel: BrowserViewModel
    var historyVM: HistoryViewModel
    var bookmarkVM: BookmarkViewModel
    
    
    var body: some View {
        GeometryReader { geometry in
            SingleTabRootView(
                tab: tab,
                viewModel: viewModel,
                historyVM: historyVM,
                bookmarkVM: bookmarkVM,
                containerSize: geometry.size
            )
            .id(tab.id)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .ignoresSafeArea(.container)
        .onAppear {
            // Tab 进入全屏视图
            NotificationCenter.default.post(
                name: .tabEnteredFullscreenFromNative,
                object: nil,
                userInfo: [
                    "tabId": tab.id.uuidString,
                    "title": tab.title
                ]
            )
        }
    }
}
