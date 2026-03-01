import AppKit
import SwiftUI

/// Content view controller - hosts SwiftUI WorkspaceView with rounded corners
final class ContentViewController: NSViewController {

    // MARK: - Properties

    private let viewModel: BrowserViewModel
    private let historyVM: HistoryViewModel
    private let bookmarkVM: BookmarkViewModel

    private var hostingView: NSHostingView<ContentAreaView>!

    /// Corner radius for content area
    private let cornerRadius: CGFloat = 16

    // MARK: - Initialization

    init(viewModel: BrowserViewModel, historyVM: HistoryViewModel, bookmarkVM: BookmarkViewModel) {
        self.viewModel = viewModel
        self.historyVM = historyVM
        self.bookmarkVM = bookmarkVM

        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    // MARK: - Lifecycle

    override func loadView() {
        // Create SwiftUI content
        let contentView = ContentAreaView(
            viewModel: viewModel,
            historyVM: historyVM,
            bookmarkVM: bookmarkVM,
            cornerRadius: cornerRadius
        )

        // Create hosting view
        hostingView = NSHostingView(rootView: contentView)
        hostingView.translatesAutoresizingMaskIntoConstraints = false

        // Container view with rounded corners
        let container = NSView()
        container.wantsLayer = true
        container.layer?.cornerRadius = cornerRadius
        container.layer?.masksToBounds = true
        container.addSubview(hostingView)

        NSLayoutConstraint.activate([
            hostingView.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            hostingView.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            hostingView.topAnchor.constraint(equalTo: container.topAnchor),
            hostingView.bottomAnchor.constraint(equalTo: container.bottomAnchor),
        ])

        self.view = container
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        // Set background color
        view.layer?.backgroundColor = NSColor(white: 0.10, alpha: 1.0).cgColor
    }
}

// MARK: - SwiftUI Content Area

/// SwiftUI view for main content area
private struct ContentAreaView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel
    @ObservedObject var bookmarkVM: BookmarkViewModel
    @ObservedObject var devTools: DevToolsManager = .shared

    let cornerRadius: CGFloat

    var body: some View {
        WorkspaceView(devTools: devTools, viewModel: viewModel) {
            WorkspaceMainView(
                viewModel: viewModel,
                historyVM: historyVM,
                bookmarkVM: bookmarkVM,
                useNativeToolbar: true
            )
        }
    }
}
