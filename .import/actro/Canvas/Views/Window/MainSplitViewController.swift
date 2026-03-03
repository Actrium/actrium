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

    override func viewDidAppear() {
        super.viewDidAppear()
        // By viewDidAppear the split view subviews have their real frames.
        // Force a sync to replace any stale default values in SidebarLayoutState.
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
        // has zero width, the subview hasn't been laid out yet.  Keep
        // isSidebarCollapsed=false (so the toolbar doesn't flash the
        // collapsed state) but don't update geometry — we don't have
        // real values yet.
        if !sidebarItem.isCollapsed && sidebarWidth <= 0.5 { return }

        let isCollapsed = sidebarItem.isCollapsed
        let splitBoundaryX: CGFloat
        if isCollapsed {
            // Keep toolbar/background collapse synchronized with sidebar snap-to-zero.
            splitBoundaryX = 0
        } else {
            // NSSplitViewController may wrap each VC's view in a container,
            // so contentView.frame.minX is relative to the wrapper (often 0),
            // not to the split view.  Use coordinate conversion to get the
            // content's actual leading edge in the split view's space.
            let contentOriginX: CGFloat
            if let cv = contentView {
                contentOriginX = max(0, cv.convert(CGPoint.zero, to: splitView).x)
            } else {
                contentOriginX = 0
            }

            if contentOriginX > dividerThickness {
                splitBoundaryX = contentOriginX
            } else {
                // Fallback: sidebar width + divider
                splitBoundaryX = max(0, sidebarWidth + dividerThickness)
            }
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
        // Bypass super.toggleSidebar which always uses animator().
        // Wrap in explicit no-animation context to suppress any
        // implicit Core Animation the split view might trigger.
        NSAnimationContext.beginGrouping()
        NSAnimationContext.current.duration = 0
        NSAnimationContext.current.allowsImplicitAnimation = false
        sidebarItem.isCollapsed.toggle()
        NSAnimationContext.endGrouping()
        syncSidebarLayoutState()
    }
}
