import SwiftUI

struct PhoneTabListView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel
    @ObservedObject var bookmarkVM: BookmarkViewModel
    var onSelect: (Tab) -> Void
    
    // 两列网格布局，参考 Safari
    private let columns = [
        GridItem(.flexible(), spacing: 16),
        GridItem(.flexible(), spacing: 16)
    ]
    
    var body: some View {
        NavigationStack {
            GeometryReader { geometry in
                ScrollView {
                    LazyVGrid(columns: columns, spacing: 20) {
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
                    .padding(16)
                }
            }
            .navigationTitle("标签页")
            .navigationBarTitleDisplayMode(.inline)
            .background(Color(.systemBackground)) // Safari 风格通常是深色或纯色背景
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(action: {
                        let newTab = viewModel.addNewTab()
                        onSelect(newTab)
                    }) {
                        Image(systemName: "plus")
                            .font(.system(size: 18, weight: .semibold))
                    }
                }
                
                // 左侧完成按钮（类似 Safari）
                ToolbarItem(placement: .navigationBarLeading) {
                    Button("完成") {
                        if let currentTab = viewModel.tabs.first(where: { $0.id == viewModel.currentTabId }) {
                            onSelect(currentTab)
                        } else if let firstTab = viewModel.tabs.first {
                            onSelect(firstTab)
                        }
                    }
                }
            }
        }
    }
}

// 子组件：页签卡片，参考 Safari 手机端风格
struct PhoneTabCardView: View {
    let tab: Tab
    @ObservedObject var viewModel: BrowserViewModel
    var historyVM: HistoryViewModel
    var bookmarkVM: BookmarkViewModel
    let screenSize: CGSize
    let isSelected: Bool
    let onSelect: () -> Void
    let onClose: () -> Void
    
    // 固定的宽高比，约 3:4
    private let cardAspectRatio: CGFloat = 0.8
    
    var body: some View {
        VStack(spacing: 0) {
            // 顶部标题栏：白色背景，包含标题和图标，以及关闭按钮
            HStack(spacing: 6) {
                // 小图标
                Circle()
                    .fill(Color(hslString: tab.backgroundColor))
                    .frame(width: 10, height: 10)
                
                Text(tab.title.isEmpty ? "新标签页" : tab.title)
                    .font(.system(size: 12, weight: .medium))
                    .lineLimit(1)
                    .foregroundColor(.primary)
                
                Spacer()
                
                Button(action: onClose) {
                    Image(systemName: "xmark")
                        .font(.system(size: 10, weight: .bold))
                        .foregroundColor(.primary.opacity(0.6))
                        .padding(5)
                        .background(Circle().fill(Color(.systemFill)))
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .background(Color(.secondarySystemGroupedBackground))
            
            // 主体内容区：优先显示快照，没有快照则渲染实时视图或背景
            // 使用矩形占位符并应用比例，确保容器大小稳定
            Rectangle()
                .fill(Color(hslString: tab.backgroundColor).opacity(0.05))
                .aspectRatio(cardAspectRatio, contentMode: .fit)
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
                                Image(systemName: "safari")
                                    .font(.system(size: 32))
                                    .foregroundColor(.gray.opacity(0.3))
                                if let host = tab.url?.host() {
                                    Text(host)
                                        .font(.system(size: 10))
                                        .foregroundColor(.secondary)
                                }
                            }
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                        }
                    }
                }
                .clipped()
                .allowsHitTesting(false)
        }
        .background(Color(.systemBackground))
        .clipShape(RoundedRectangle(cornerRadius: 16))
        .overlay(
            RoundedRectangle(cornerRadius: 16)
                .stroke(isSelected ? Color.blue : Color.primary.opacity(0.1), lineWidth: isSelected ? 3 : 0.5)
        )
        .scaleEffect(isSelected ? 0.98 : 1.0) // 选中时轻微缩小，增加点击感
        .animation(.spring(response: 0.2, dampingFraction: 0.7), value: isSelected)
        .shadow(color: Color.black.opacity(isSelected ? 0.25 : 0.1), radius: isSelected ? 12 : 4, x: 0, y: isSelected ? 4 : 2)
        .contentShape(Rectangle())
        .onTapGesture {
            onSelect()
        }
    }
}

struct PhoneBrowserView: View {
    let tab: Tab
    @ObservedObject var viewModel: BrowserViewModel
    var historyVM: HistoryViewModel
    var bookmarkVM: BookmarkViewModel
    
    @Environment(\.dismiss) var dismiss
    
    var body: some View {
        GeometryReader { geometry in
            VStack(spacing: 0) {
                // Top Bar
                HStack {
                    Button(action: {
                        // 在关闭前尝试对当前内容进行快照
                        takeSnapshotAndDismiss()
                    }) {
                        Image(systemName: "square.grid.2x2")
                            .font(.system(size: 18))
                    }
                    .padding(.leading)
                    
                    Spacer()
                    
                    Text(tab.title)
                        .font(.system(size: 16, weight: .semibold))
                        .lineLimit(1)
                    
                    Spacer()
                    
                    Button(action: {
                        viewModel.toggleLeftPanelForCurrentTab()
                    }) {
                        Image(systemName: "sidebar.left")
                            .font(.system(size: 18))
                    }
                    .padding(.trailing)
                }
                .frame(height: 44)
                .background(Color(.systemBackground))
                
                Divider()
                
                // Content
                GeometryReader { contentGeo in
                    SingleTabRootView(
                        tab: tab,
                        viewModel: viewModel,
                        historyVM: historyVM,
                        bookmarkVM: bookmarkVM,
                        containerSize: contentGeo.size
                    )
                    .id(tab.id)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .ignoresSafeArea(.keyboard)
        .onDisappear {
            // 确保在离开时也保存一份快照（例如通过其他方式关闭）
            // 注意：此时视图可能已经不在层级中，快照可能失效，主要依赖主动触发的快照
        }
    }
    
    private func takeSnapshotAndDismiss() {
        // 尝试捕获当前窗口的快照并裁剪掉导航栏，只保留内容区
        DispatchQueue.main.async {
            let window = UIApplication.shared.connectedScenes
                .filter { $0.activationState == .foregroundActive }
                .compactMap { $0 as? UIWindowScene }
                .first?.windows
                .filter { $0.isKeyWindow }.first
            
            if let window = window {
                let renderer = UIGraphicsImageRenderer(bounds: window.bounds)
                let fullImage = renderer.image { context in
                    window.drawHierarchy(in: window.bounds, afterScreenUpdates: true)
                }
                
                // 计算裁剪区域：
                // 1. 避开顶部的状态栏 (safeAreaInsets.top)
                // 2. 避开我们自定义的导航栏 (44) 和 分割线 (1)
                let topBarHeight: CGFloat = 44 + 1
                let topOffset = window.safeAreaInsets.top + topBarHeight
                
                // 裁剪区域：从内容区开始，直到底部安全区域之上
                let cropRect = CGRect(
                    x: 0,
                    y: topOffset,
                    width: window.bounds.width,
                    height: window.bounds.height - topOffset - window.safeAreaInsets.bottom
                )
                
                if let croppedImage = fullImage.cropped(to: cropRect) {
                    viewModel.tabSnapshots[tab.id] = croppedImage
                } else {
                    // 如果裁剪失败，保留全屏图作为 fallback
                    viewModel.tabSnapshots[tab.id] = fullImage
                }
            }
            dismiss()
        }
    }
}
