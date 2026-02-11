import SwiftUI

struct PhoneTabListView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel
    @ObservedObject var bookmarkVM: BookmarkViewModel
    var onSelect: (Tab) -> Void
    
    var body: some View {
        NavigationStack {
            List {
                Section {
                    ForEach(viewModel.tabs) { tab in
                        Button(action: {
                            viewModel.currentTabId = tab.id
                            onSelect(tab)
                        }) {
                            HStack(spacing: 12) {
                                // Tab Icon/Color
                                Circle()
                                    .fill(Color(hslString: tab.backgroundColor))
                                    .frame(width: 8, height: 8)
                                
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(tab.title.isEmpty ? "New Tab" : tab.title)
                                        .font(.system(size: 16, weight: .medium))
                                        .foregroundColor(.primary)
                                    
                                    if let url = tab.url {
                                        Text(url.host() ?? url.absoluteString)
                                            .font(.system(size: 12))
                                            .foregroundColor(.secondary)
                                            .lineLimit(1)
                                    }
                                }
                                
                                Spacer()
                                
                                if viewModel.currentTabId == tab.id {
                                    Image(systemName: "checkmark.circle.fill")
                                        .foregroundColor(.blue)
                                }
                            }
                            .padding(.vertical, 4)
                        }
                        .swipeActions(edge: .trailing) {
                            Button(role: .destructive) {
                                viewModel.closeTab(id: tab.id)
                            } label: {
                                Label("关闭", systemImage: "xmark")
                            }
                        }
                    }
                } header: {
                    Text("当前页签")
                }
                
                Section {
                    Button(action: {
                        let newTab = viewModel.addNewTab()
                        onSelect(newTab)
                    }) {
                        Label("新建页签", systemImage: "plus")
                            .fontWeight(.medium)
                    }
                }
            }
            .navigationTitle("页签")
            .listStyle(.insetGrouped)
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
                    Button(action: { dismiss() }) {
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
    }
}
