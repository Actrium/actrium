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
