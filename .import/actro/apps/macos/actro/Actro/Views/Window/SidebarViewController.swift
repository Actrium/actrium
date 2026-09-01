import AppKit
import SwiftUI

/// Sidebar view controller - hosts SwiftUI SidebarView
final class SidebarViewController: NSViewController {

    // MARK: - Properties

    private let viewModel: BrowserViewModel
    private let bookmarkVM: BookmarkViewModel
    private let historyVM: HistoryViewModel

    private var hostingView: NSHostingView<SidebarContentView>!

    // MARK: - Initialization

    init(viewModel: BrowserViewModel, bookmarkVM: BookmarkViewModel, historyVM: HistoryViewModel) {
        self.viewModel = viewModel
        self.bookmarkVM = bookmarkVM
        self.historyVM = historyVM

        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    // MARK: - Lifecycle

    override func loadView() {
        // Create SwiftUI content
        let sidebarContent = SidebarContentView(
            viewModel: viewModel,
            bookmarkVM: bookmarkVM,
            historyVM: historyVM
        )

        // Create hosting view
        hostingView = NSHostingView(rootView: sidebarContent)
        hostingView.translatesAutoresizingMaskIntoConstraints = false

        // Container view
        let container = NSView()
        container.wantsLayer = true
        container.addSubview(hostingView)

        NSLayoutConstraint.activate([
            hostingView.leadingAnchor.constraint(equalTo: container.leadingAnchor, constant: 1),
            hostingView.trailingAnchor.constraint(equalTo: container.trailingAnchor, constant: -6),
            hostingView.topAnchor.constraint(equalTo: container.topAnchor, constant: 1),
            hostingView.bottomAnchor.constraint(equalTo: container.bottomAnchor, constant: -1),
        ])

        self.view = container
    }
}

// MARK: - SwiftUI Sidebar Content

/// SwiftUI view for sidebar content (wraps existing SidebarView)
private struct SidebarContentView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var bookmarkVM: BookmarkViewModel
    @ObservedObject var historyVM: HistoryViewModel

    var body: some View {
        SidebarView(viewModel: viewModel, bookmarkVM: bookmarkVM, historyVM: historyVM)
    }
}
