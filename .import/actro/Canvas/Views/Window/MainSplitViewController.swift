import AppKit
import SwiftUI
import Combine

/// Split view controller managing sidebar and content areas
final class MainSplitViewController: NSSplitViewController {

    // MARK: - Properties

    private let viewModel: BrowserViewModel
    private let historyVM: HistoryViewModel
    private let bookmarkVM: BookmarkViewModel
    private let edgeInset: CGFloat

    private var sidebarItem: NSSplitViewItem!
    private var contentItem: NSSplitViewItem!

    private var sidebarViewController: SidebarViewController!
    private var contentViewController: ContentViewController!

    private var cancellables = Set<AnyCancellable>()

    /// Sidebar width constraints
    private let minSidebarWidth: CGFloat = 180
    private let maxSidebarWidthRatio: CGFloat = 0.5

    // MARK: - Initialization

    init(viewModel: BrowserViewModel, historyVM: HistoryViewModel, bookmarkVM: BookmarkViewModel, edgeInset: CGFloat) {
        self.viewModel = viewModel
        self.historyVM = historyVM
        self.bookmarkVM = bookmarkVM
        self.edgeInset = edgeInset

        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    // MARK: - Lifecycle

    override func viewDidLoad() {
        super.viewDidLoad()

        setupSplitView()
        setupViewControllers()
        setupNotifications()
    }

    // MARK: - Setup

    private func setupSplitView() {
        // Configure split view
        splitView.isVertical = true
        splitView.dividerStyle = .thin
        // Note: Don't use autosaveName with delegate methods that conflict with autolayout
    }

    private func setupViewControllers() {
        // Create sidebar view controller
        sidebarViewController = SidebarViewController(
            viewModel: viewModel,
            bookmarkVM: bookmarkVM,
            historyVM: historyVM
        )

        // Create content view controller
        contentViewController = ContentViewController(
            viewModel: viewModel,
            historyVM: historyVM,
            bookmarkVM: bookmarkVM
        )

        // Create split view items
        sidebarItem = NSSplitViewItem(sidebarWithViewController: sidebarViewController)
        sidebarItem.canCollapse = true
        sidebarItem.minimumThickness = minSidebarWidth
        sidebarItem.maximumThickness = 400  // Will be updated dynamically

        contentItem = NSSplitViewItem(viewController: contentViewController)
        contentItem.minimumThickness = 300

        // Add items
        addSplitViewItem(sidebarItem)
        addSplitViewItem(contentItem)
    }

    private func setupNotifications() {
        // Listen for sidebar toggle requests
        NotificationCenter.default.publisher(for: .toggleSidebarRequested)
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                self?.toggleSidebar(nil)
            }
            .store(in: &cancellables)
    }

    // MARK: - Split View Updates

    override func viewDidLayout() {
        super.viewDidLayout()

        // Sync sidebar width with TitlebarManager after layout
        if let sidebarView = sidebarViewController?.view {
            let sidebarWidth = sidebarView.frame.width
            if sidebarWidth > 0 {
                TitlebarManager.shared.updateSidebarWidth(sidebarWidth + edgeInset)
            }
        }
    }

    // MARK: - Public Methods

    @objc override func toggleSidebar(_ sender: Any?) {
        super.toggleSidebar(sender)

        let isCollapsed = sidebarItem.isCollapsed
        TitlebarManager.shared.updateSidebarVisibility(!isCollapsed)
    }
}
