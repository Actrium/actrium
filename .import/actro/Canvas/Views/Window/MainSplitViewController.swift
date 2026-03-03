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
    private let sidebarLayoutState: SidebarLayoutState

    private var sidebarItem: NSSplitViewItem!
    private var contentItem: NSSplitViewItem!

    private var sidebarViewController: SidebarViewController!
    private var contentViewController: ContentViewController!

    private var cancellables = Set<AnyCancellable>()

    /// Sidebar width constraints
    private let minSidebarWidth: CGFloat = 180
    private let maxSidebarWidthRatio: CGFloat = 0.5

    // MARK: - Initialization

    init(
        viewModel: BrowserViewModel,
        historyVM: HistoryViewModel,
        bookmarkVM: BookmarkViewModel,
        edgeInset: CGFloat,
        sidebarLayoutState: SidebarLayoutState
    ) {
        self.viewModel = viewModel
        self.historyVM = historyVM
        self.bookmarkVM = bookmarkVM
        self.edgeInset = edgeInset
        self.sidebarLayoutState = sidebarLayoutState

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
        splitView.delegate = self
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
        sidebarItem.maximumThickness = 400  // Updated dynamically in syncSidebarLayoutState()

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
        syncSidebarLayoutState()
    }

    override func splitViewDidResizeSubviews(_ notification: Notification) {
        syncSidebarLayoutState()
    }

    private func syncSidebarLayoutState() {
        guard let sidebarView = sidebarViewController?.view else { return }
        let contentView = contentViewController?.view

        // Keep sidebar maximum width proportional to current container width.
        let dynamicMaxSidebarWidth = max(minSidebarWidth, floor(view.bounds.width * maxSidebarWidthRatio))
        if abs(sidebarItem.maximumThickness - dynamicMaxSidebarWidth) > 0.5 {
            sidebarItem.maximumThickness = dynamicMaxSidebarWidth
        }

        let sidebarWidth = max(0, sidebarView.frame.width)
        let dividerThickness = max(0, splitView.dividerThickness)

        // If the split view item reports the sidebar is open but the view
        // has zero width, the subview hasn't been laid out yet.  Skip this
        // sync so the toolbar doesn't flash the collapsed state on launch.
        if !sidebarItem.isCollapsed && sidebarWidth <= 0.5 { return }

        let isCollapsed = sidebarItem.isCollapsed
        let splitBoundaryX: CGFloat
        if isCollapsed {
            // Keep toolbar/background collapse synchronized with sidebar snap-to-zero.
            splitBoundaryX = 0
        } else {
            let splitBoundaryXFromSidebar = max(0, sidebarWidth + dividerThickness)
            let splitBoundaryXFromContent = max(0, contentView?.frame.minX ?? 0)
            splitBoundaryX = splitBoundaryXFromContent > dividerThickness
                ? splitBoundaryXFromContent
                : splitBoundaryXFromSidebar
        }

        sidebarLayoutState.update(
            sidebarWidth: sidebarWidth,
            splitBoundaryX: splitBoundaryX,
            isSidebarCollapsed: isCollapsed,
            windowWidth: view.bounds.width
        )

        // Keep legacy titlebar path in sync for the alternative custom titlebar implementation.
        TitlebarManager.shared.updateSidebarWidth(splitBoundaryX + edgeInset)
        TitlebarManager.shared.updateSidebarVisibility(!isCollapsed)
    }

    // MARK: - Public Methods

    @objc override func toggleSidebar(_ sender: Any?) {
        // Set isCollapsed directly instead of calling super.toggleSidebar,
        // which internally uses animator() and always animates.
        sidebarItem.isCollapsed.toggle()
        syncSidebarLayoutState()
    }
}
