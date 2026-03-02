import AppKit
import SwiftUI

/// Root content view controller - manages 4px edge insets and unified content
/// Layout: Window -> 4px inset -> Rounded container -> (Toolbar + SplitView)
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
        // Root view - transparent, window-sized
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
        // Rounded container with all corners rounded
        roundedContainer = NSView()
        roundedContainer.wantsLayer = true
        roundedContainer.layer?.cornerRadius = cornerRadius
        roundedContainer.layer?.masksToBounds = true
        roundedContainer.layer?.backgroundColor = NSColor(white: 0.10, alpha: 1.0).cgColor
        roundedContainer.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(roundedContainer)
    }

    private func setupToolbar() {
        // Create custom toolbar as SwiftUI view
        let toolbarView = CustomToolbarView(
            viewModel: viewModel,
            bookmarkVM: bookmarkVM,
            onToggleSidebar: { [weak self] in
                self?.toggleSidebar()
            },
            onToggleFocusOverview: {
                NotificationCenter.default.post(name: .toggleFocusOverviewRequested, object: nil)
            }
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
            edgeInset: 0  // No extra inset needed, container handles it
        )

        addChild(splitViewController)
        splitViewController.view.translatesAutoresizingMaskIntoConstraints = false
        roundedContainer.addSubview(splitViewController.view)
    }

    private func setupConstraints() {
        NSLayoutConstraint.activate([
            // Rounded container with 4px inset on all sides
            roundedContainer.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: edgeInset),
            roundedContainer.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -edgeInset),
            roundedContainer.topAnchor.constraint(equalTo: view.topAnchor, constant: edgeInset),
            roundedContainer.bottomAnchor.constraint(equalTo: view.bottomAnchor, constant: -edgeInset),

            // Toolbar at top of container
            toolbarHostingView.leadingAnchor.constraint(equalTo: roundedContainer.leadingAnchor),
            toolbarHostingView.trailingAnchor.constraint(equalTo: roundedContainer.trailingAnchor),
            toolbarHostingView.topAnchor.constraint(equalTo: roundedContainer.topAnchor),
            toolbarHostingView.heightAnchor.constraint(equalToConstant: toolbarHeight),

            // Split view below toolbar
            splitViewController.view.leadingAnchor.constraint(equalTo: roundedContainer.leadingAnchor),
            splitViewController.view.trailingAnchor.constraint(equalTo: roundedContainer.trailingAnchor),
            splitViewController.view.topAnchor.constraint(equalTo: toolbarHostingView.bottomAnchor),
            splitViewController.view.bottomAnchor.constraint(equalTo: roundedContainer.bottomAnchor),
        ])
    }

    // MARK: - Actions

    private func toggleSidebar() {
        splitViewController.toggleSidebar(nil)
    }
}

// MARK: - Custom Toolbar View (SwiftUI)

private struct CustomToolbarView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var bookmarkVM: BookmarkViewModel
    var onToggleSidebar: () -> Void
    var onToggleFocusOverview: () -> Void

    @State private var addressText: String = ""
    @State private var isAddressFocused: Bool = false
    @State private var showingBookmarkPopover: Bool = false

    // Layout constants
    private let iconButtonSize: CGFloat = 30
    private let iconPointSize: CGFloat = 16

    var body: some View {
        HStack(spacing: 12) {
            // Left section: traffic lights space + buttons
            HStack(spacing: 12) {
                // Space for traffic lights (~78px)
                Color.clear.frame(width: 70)

                // New tab button
                Button(action: { _ = viewModel.addNewTab() }) {
                    Image(systemName: "plus")
                        .font(.system(size: iconPointSize, weight: .medium))
                        .frame(width: iconButtonSize, height: iconButtonSize)
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)

                // Overview button
                Button(action: onToggleFocusOverview) {
                    Image(systemName: "square.on.square")
                        .font(.system(size: iconPointSize, weight: .medium))
                        .frame(width: iconButtonSize, height: iconButtonSize)
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
            }

            // Sidebar toggle
            Button(action: onToggleSidebar) {
                Image(systemName: "sidebar.left")
                    .font(.system(size: iconPointSize, weight: .medium))
                    .frame(width: iconButtonSize, height: iconButtonSize)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)

            // Navigation buttons
            HStack(spacing: 0) {
                Button(action: { viewModel.goBack() }) {
                    Image(systemName: "chevron.left")
                        .font(.system(size: 16, weight: .medium))
                        .frame(width: iconButtonSize, height: iconButtonSize)
                }
                .buttonStyle(.plain)
                .disabled(!viewModel.canGoBack)

                Divider().frame(height: 18)

                Button(action: { viewModel.goForward() }) {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 16, weight: .medium))
                        .frame(width: iconButtonSize, height: iconButtonSize)
                }
                .buttonStyle(.plain)
                .disabled(!viewModel.canGoForward)
            }
            .foregroundStyle(.secondary)

            Spacer()

            // Address bar
            addressBar
                .frame(maxWidth: 400)

            Spacer()

            // Share button
            if let url = viewModel.currentTab?.url {
                ShareLink(item: url) {
                    Image(systemName: "square.and.arrow.up")
                        .font(.system(size: 15, weight: .medium))
                        .frame(width: iconButtonSize, height: iconButtonSize)
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
            }

            // More button
            Menu {
                Button("New Tab") { _ = viewModel.addNewTab() }
                Button("Reload") { viewModel.reloadCurrentTab() }
            } label: {
                Image(systemName: "ellipsis")
                    .font(.system(size: 14, weight: .medium))
                    .frame(width: iconButtonSize, height: iconButtonSize)
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .foregroundStyle(.secondary)
        }
        .padding(.horizontal, 12)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .onChange(of: viewModel.currentTab?.url) { _, _ in updateDisplayText() }
        .onChange(of: viewModel.currentTabId) { _, _ in updateDisplayText() }
        .onAppear { updateDisplayText() }
    }

    // MARK: - Address Bar

    private var addressBar: some View {
        HStack(spacing: 0) {
            // Left icon
            if viewModel.isCurrentTabLoading {
                ProgressView()
                    .scaleEffect(0.6)
                    .frame(width: 32, height: 32)
            } else {
                Image(systemName: addressBarIcon)
                    .font(.system(size: 14, weight: .medium))
                    .foregroundStyle(.secondary.opacity(0.5))
                    .frame(width: 32, height: 32)
            }

            // Address text
            Text(addressText.isEmpty ? "Search or enter website" : addressText)
                .font(.system(size: 13))
                .foregroundStyle(addressText.isEmpty ? .secondary : .primary)
                .lineLimit(1)
                .truncationMode(.tail)
                .frame(maxWidth: .infinity, alignment: .leading)

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
            .foregroundStyle(.secondary.opacity(0.5))
        }
        .frame(height: 30)
        .padding(.horizontal, 4)
        .background(Color.white.opacity(0.08))
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    private var addressBarIcon: String {
        guard let tab = viewModel.currentTab else { return "magnifyingglass" }
        switch tab.contentType {
        case .web: return "globe"
        case .reactNative, .native: return "list.bullet"
        case .empty: return "magnifyingglass"
        }
    }

    private var isBookmarked: Bool {
        guard let tab = viewModel.currentTab else { return false }
        return bookmarkVM.isBookmarked(tab: tab)
    }

    private func toggleBookmark() {
        guard let tab = viewModel.currentTab else { return }
        if isBookmarked {
            if let bookmark = bookmarkVM.getBookmark(for: tab) {
                bookmarkVM.removeBookmark(id: bookmark.id)
            }
        } else {
            let bookmark = Bookmark.from(tab: tab, manufacturer: nil)
            bookmarkVM.addBookmark(bookmark)
        }
    }

    private func updateDisplayText() {
        guard let tab = viewModel.currentTab else {
            addressText = ""
            return
        }
        if let systemType = tab.systemTabType {
            addressText = systemType.aboutURL.absoluteString
        } else if let url = tab.url {
            let host = url.host ?? url.absoluteString
            addressText = host.hasPrefix("www.") ? String(host.dropFirst(4)) : host
        } else {
            addressText = tab.title
        }
    }
}
