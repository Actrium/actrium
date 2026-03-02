import AppKit
import SwiftUI
import Combine

/// Root content view controller - manages 4px edge inset and contains toolbar + split view
final class RootContentViewController: NSViewController {

    // MARK: - Properties

    private let viewModel: BrowserViewModel
    private let historyVM: HistoryViewModel
    private let bookmarkVM: BookmarkViewModel

    /// Edge inset (transparent border around content)
    private let edgeInset: CGFloat = 4

    /// Corner radius for content area
    private let cornerRadius: CGFloat = 12

    /// Toolbar height
    private let toolbarHeight: CGFloat = 52

    // Child views
    private var roundedContainer: NSView!
    private var toolbarHostingView: NSHostingView<AnyView>!
    private var splitViewController: MainSplitViewController!

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
        // Root view with transparent background
        let rootView = NSView()
        rootView.wantsLayer = true
        rootView.layer?.backgroundColor = NSColor.clear.cgColor
        self.view = rootView
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        setupRoundedContainer()
        setupToolbar()
        setupSplitView()
        setupConstraints()
    }

    // MARK: - Setup

    private func setupRoundedContainer() {
        // Container that holds all content
        // Only bottom corners are rounded (top has 4px transparent edge inside toolbar)
        roundedContainer = NSView()
        roundedContainer.wantsLayer = true
        // Use CACornerMask to only round bottom corners
        roundedContainer.layer?.cornerRadius = cornerRadius
        roundedContainer.layer?.maskedCorners = [.layerMinXMaxYCorner, .layerMaxXMaxYCorner]  // Bottom left & right
        roundedContainer.layer?.masksToBounds = true
        roundedContainer.layer?.backgroundColor = NSColor.clear.cgColor  // Transparent - content provides background
        roundedContainer.translatesAutoresizingMaskIntoConstraints = false

        view.addSubview(roundedContainer)
    }

    private func setupToolbar() {
        // Create SwiftUI toolbar
        let toolbarView = SelfDrawnToolbarView(
            viewModel: viewModel,
            bookmarkVM: bookmarkVM
        )

        toolbarHostingView = NSHostingView(rootView: AnyView(toolbarView))
        toolbarHostingView.translatesAutoresizingMaskIntoConstraints = false
        roundedContainer.addSubview(toolbarHostingView)
    }

    private func setupSplitView() {
        // Create split view controller
        splitViewController = MainSplitViewController(
            viewModel: viewModel,
            historyVM: historyVM,
            bookmarkVM: bookmarkVM,
            edgeInset: 0  // No additional inset needed, container handles it
        )

        addChild(splitViewController)

        let splitView = splitViewController.view
        splitView.translatesAutoresizingMaskIntoConstraints = false
        roundedContainer.addSubview(splitView)
    }

    private func setupConstraints() {
        NSLayoutConstraint.activate([
            // Rounded container: 4px inset on left, right, bottom; NO top inset
            // Top 4px transparency is handled inside toolbar background
            roundedContainer.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: edgeInset),
            roundedContainer.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -edgeInset),
            roundedContainer.topAnchor.constraint(equalTo: view.topAnchor),  // No top inset
            roundedContainer.bottomAnchor.constraint(equalTo: view.bottomAnchor, constant: -edgeInset),

            // Toolbar: top of container, full width, fixed height
            toolbarHostingView.leadingAnchor.constraint(equalTo: roundedContainer.leadingAnchor),
            toolbarHostingView.trailingAnchor.constraint(equalTo: roundedContainer.trailingAnchor),
            toolbarHostingView.topAnchor.constraint(equalTo: roundedContainer.topAnchor),
            toolbarHostingView.heightAnchor.constraint(equalToConstant: toolbarHeight),

            // Split view: below toolbar, fills remaining space
            splitViewController.view.leadingAnchor.constraint(equalTo: roundedContainer.leadingAnchor),
            splitViewController.view.trailingAnchor.constraint(equalTo: roundedContainer.trailingAnchor),
            splitViewController.view.topAnchor.constraint(equalTo: toolbarHostingView.bottomAnchor),
            splitViewController.view.bottomAnchor.constraint(equalTo: roundedContainer.bottomAnchor),
        ])
    }

    // MARK: - Sidebar Width Sync

    override func viewDidLayout() {
        super.viewDidLayout()

        // Sync sidebar width for toolbar background alignment
        if let sidebarView = splitViewController?.splitViewItems.first?.viewController.view {
            let sidebarWidth = sidebarView.frame.width
            if sidebarWidth > 0 {
                // Notify toolbar of sidebar width change
                NotificationCenter.default.post(
                    name: .sidebarWidthChanged,
                    object: nil,
                    userInfo: ["width": sidebarWidth]
                )
            }
        }
    }
}

// MARK: - Notification Names

extension Notification.Name {
    static let sidebarWidthChanged = Notification.Name("sidebarWidthChanged")
}

// MARK: - Self-Drawn Toolbar View

struct SelfDrawnToolbarView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var bookmarkVM: BookmarkViewModel

    @State private var sidebarWidth: CGFloat = 250
    @State private var isSidebarCollapsed: Bool = false
    @State private var addressText: String = ""
    @State private var isAddressFocused: Bool = false
    @State private var showingBookmarkPopover: Bool = false

    // Layout constants
    private let iconButtonSize: CGFloat = 30
    private let iconPointSize: CGFloat = 16

    var body: some View {
        // Toolbar content area - icons aligned near top to match traffic lights
        HStack(spacing: 0) {
            // Sidebar area (traffic lights + sidebar toggle + new tab)
            sidebarToolbarSection
                .frame(width: isSidebarCollapsed ? 78 : sidebarWidth)

            // Content area (navigation + address bar + actions)
            contentToolbarSection
        }
        .padding(.top, 4)  // Position icons to align with traffic lights
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(toolbarBackground)
        .onReceive(NotificationCenter.default.publisher(for: .sidebarWidthChanged)) { notification in
            if let width = notification.userInfo?["width"] as? CGFloat {
                withAnimation(.easeInOut(duration: 0.15)) {
                    sidebarWidth = width
                }
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: .toggleSidebarRequested)) { _ in
            withAnimation(.easeInOut(duration: 0.25)) {
                isSidebarCollapsed.toggle()
            }
        }
    }

    // MARK: - Sidebar Section

    private var sidebarToolbarSection: some View {
        HStack(spacing: 8) {
            // Space for traffic lights (~70px from left edge)
            Spacer()
                .frame(width: 70)

            // New tab button
            Button(action: { _ = viewModel.addNewTab() }) {
                Image(systemName: "plus")
                    .font(.system(size: iconPointSize, weight: .medium))
                    .frame(width: iconButtonSize, height: iconButtonSize)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)

            // Overview toggle button
            Button(action: { toggleOverview() }) {
                Image(systemName: "square.on.square")
                    .font(.system(size: iconPointSize, weight: .medium))
                    .frame(width: iconButtonSize, height: iconButtonSize)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)

            Spacer()
        }
    }

    // MARK: - Content Section

    private var contentToolbarSection: some View {
        HStack(spacing: 12) {
            // Sidebar toggle
            Button(action: { toggleSidebar() }) {
                Image(systemName: "sidebar.left")
                    .font(.system(size: iconPointSize, weight: .medium))
                    .frame(width: iconButtonSize, height: iconButtonSize)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)

            // Navigation buttons
            HStack(spacing: 4) {
                Button(action: { viewModel.goBack() }) {
                    Image(systemName: "chevron.left")
                        .font(.system(size: 14, weight: .semibold))
                        .frame(width: 28, height: 28)
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .disabled(!viewModel.canGoBack)

                Button(action: { viewModel.goForward() }) {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 14, weight: .semibold))
                        .frame(width: 28, height: 28)
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .disabled(!viewModel.canGoForward)
            }

            // Address bar
            addressBar
                .frame(maxWidth: 400)

            Spacer()

            // Right side actions
            rightActions
        }
        .padding(.horizontal, 12)
    }

    // MARK: - Address Bar

    private var addressBar: some View {
        HStack(spacing: 8) {
            // Page icon
            Image(systemName: "line.3.horizontal")
                .font(.system(size: 12))
                .foregroundStyle(.tertiary)

            // Address text
            Text(currentDisplayURL)
                .font(.system(size: 13))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)

            Spacer()
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .background(Color(white: 0.15))
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    private var currentDisplayURL: String {
        if let tab = viewModel.currentTab {
            if tab.systemTabType != nil {
                return "about:blank"
            }
            return tab.url?.host ?? tab.url?.absoluteString ?? "about:blank"
        }
        return "about:blank"
    }

    // MARK: - Right Actions

    private var rightActions: some View {
        HStack(spacing: 8) {
            // Bookmark button
            Button(action: { toggleBookmark() }) {
                Image(systemName: isBookmarked ? "star.fill" : "star")
                    .font(.system(size: 14, weight: .medium))
                    .frame(width: 32, height: 32)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            .opacity(isBookmarked ? 1 : 0.5)

            // Refresh button
            Button(action: { viewModel.reloadCurrentTab() }) {
                Image(systemName: "arrow.clockwise")
                    .font(.system(size: 14, weight: .medium))
                    .frame(width: 32, height: 32)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)

            // More button
            Button(action: {}) {
                Image(systemName: "ellipsis")
                    .font(.system(size: 14, weight: .medium))
                    .frame(width: 32, height: 32)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
        }
    }

    // MARK: - Background

    private var toolbarBackground: some View {
        GeometryReader { geo in
            VStack(spacing: 0) {
                // Top 4px transparent (window edge effect) - background only
                Color.clear.frame(height: 4)

                // Content background starts 4px from top
                HStack(spacing: 0) {
                    // Sidebar background (transparent)
                    Color.clear
                        .frame(width: isSidebarCollapsed ? 0 : sidebarWidth)

                    // Content background (dark with top rounded corners)
                    UnevenRoundedRectangle(
                        topLeadingRadius: 12,
                        bottomLeadingRadius: 0,
                        bottomTrailingRadius: 0,
                        topTrailingRadius: 12
                    )
                    .fill(Color(white: 0.10))
                }
            }
        }
    }

    // MARK: - Actions

    private var isBookmarked: Bool {
        guard let url = viewModel.currentTab?.url else { return false }
        return bookmarkVM.bookmarks.contains(where: { $0.appURL == url })
    }

    private func toggleSidebar() {
        NotificationCenter.default.post(name: .toggleSidebarRequested, object: nil)
    }

    private func toggleOverview() {
        NotificationCenter.default.post(name: .toggleFocusOverviewRequested, object: nil)
    }

    private func toggleBookmark() {
        guard let tab = viewModel.currentTab else { return }

        if isBookmarked {
            if let url = tab.url,
               let bookmark = bookmarkVM.bookmarks.first(where: { $0.appURL == url }) {
                bookmarkVM.removeBookmark(id: bookmark.id)
            }
        } else {
            bookmarkVM.addBookmark(from: tab)
        }
    }
}
