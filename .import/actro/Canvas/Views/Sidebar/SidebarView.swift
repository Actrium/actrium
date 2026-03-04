import SwiftUI
import AppKit
import Combine

struct SidebarView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var bookmarkVM: BookmarkViewModel
    @ObservedObject var historyVM: HistoryViewModel
    @ObservedObject var windowState: WindowStateManager = .shared

    var body: some View {
        VStack(spacing: 0) {
            // NSOutlineView for tabs and groups
            TabOutlineView(viewModel: viewModel, historyVM: historyVM)

            // + New Tab button (pinned at bottom of list)
            Button(action: { _ = viewModel.addNewTab() }) {
                HStack(spacing: 8) {
                    Image(systemName: "plus")
                        .font(.system(size: 13, weight: .medium))
                    Text("New Tab")
                        .font(.system(size: 13))
                    Spacer()
                }
                .foregroundStyle(.secondary)
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .padding(.horizontal, 8)
            .padding(.vertical, 4)

            Spacer(minLength: 0)
                .contentShape(Rectangle())
                .contextMenu {
                    Button(action: {
                        AppsManagerWindowController.shared.showWindow()
                    }) {
                        Label("App Manager", systemImage: "square.grid.2x2")
                    }
                }
        }
        .onAppear {
            bookmarkVM.loadAll()
        }
    }
}

#Preview {
    SidebarView(viewModel: BrowserViewModel(), bookmarkVM: BookmarkViewModel(), historyVM: HistoryViewModel())
}
