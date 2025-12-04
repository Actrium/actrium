import SwiftUI

struct SidebarView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @State private var bookmarks: [Bookmark] = [
        Bookmark(title: "Bookmarks", url: URL(string: "https://www.apple.com")!)
    ]
    @State private var hoveredTabId: UUID?
    @State private var selection: Tab.ID?

    var body: some View {
        VStack(spacing: 0) {
            List(selection: $selection) {
                Section("\(viewModel.tabs.count) Tab\(viewModel.tabs.count == 1 ? "" : "s")") {
                    ForEach(viewModel.tabs) { tab in
                        HStack(spacing: 8) {
                            Image(systemName: "doc.text.image")
                                .foregroundColor(viewModel.currentTabId == tab.id ? .accentColor : .secondary)
                            
                            Text(tab.title)
                                .lineLimit(1)
                                .truncationMode(.tail)
                            
                            Spacer()

                            // 关闭按钮 - 悬停时显示（包括最后一个标签，关闭后退出应用）
                            if hoveredTabId == tab.id {
                                Button(action: {
                                    withAnimation {
                                        viewModel.closeTab(id: tab.id)
                                    }
                                }) {
                                    Image(systemName: "xmark.circle.fill")
                                        .foregroundColor(.secondary)
                                        .font(.caption)
                                }
                                .buttonStyle(.plain)
                                .help("关闭标签页")
                            }
                        }
                        .contentShape(Rectangle())
                        .onHover { hovering in
                            hoveredTabId = hovering ? tab.id : nil
                        }
                        .onTapGesture {
                            withAnimation {
                                viewModel.switchToTab(id: tab.id)
                            }
                        }
                        .listRowBackground(
                            viewModel.currentTabId == tab.id ?
                            Color.accentColor.opacity(0.15) : Color.clear
                        )
                    }
                }
            }
            .listStyle(.sidebar)

            // Bookmarks 区域独立于 List，避免键盘上下箭头跨到这一组
            VStack(alignment: .leading, spacing: 4) {
                Text("Bookmarks")
                    .font(.caption)
                    .foregroundColor(.secondary)
                    .padding(.horizontal, 8)

                ForEach(bookmarks) { bookmark in
                    HStack(spacing: 6) {
                        Image(systemName: "book")
                            .foregroundColor(.secondary)
                        Text(bookmark.title)
                            .lineLimit(1)
                            .truncationMode(.tail)
                    }
                    .padding(.horizontal, 8)
                    .padding(.vertical, 2)
                }

                Spacer(minLength: 8)
            }
        }
        .onChange(of: selection) { _, newValue in
            guard let id = newValue else { return }
            withAnimation {
                viewModel.switchToTab(id: id)
            }
        }
        .onChange(of: viewModel.currentTabId) { _, newValue in
            selection = newValue
        }
        .onAppear {
            selection = viewModel.currentTabId
        }
    }
}

#Preview {
    SidebarView(viewModel: BrowserViewModel())
}
