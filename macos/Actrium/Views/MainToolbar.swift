import AppKit
import SwiftUI
import Combine

// MARK: - Toolbar Item Identifiers

extension NSToolbarItem.Identifier {
    static let fullWidthBar = NSToolbarItem.Identifier("fullWidthBar")
}

// MARK: - Toolbar Manager (Singleton) - Using NSTitlebarAccessoryViewController

@MainActor
final class ToolbarManager: ObservableObject {
    static let shared = ToolbarManager()

    private var accessoryViewController: NSTitlebarAccessoryViewController?
    private var toolbarHostingView: NSHostingView<TitlebarToolbarView>?
    private var cancellables = Set<AnyCancellable>()

    // State
    private var canGoBack: Bool = false
    private var canGoForward: Bool = false
    private var displayTitle: String = "New Tab"
    private var isOverviewMode: Bool = false
    private var isBookmarked: Bool = false
    private var hasPendingSave: Bool = false
    private var isLoading: Bool = false

    // Callbacks
    var onGoBack: (() -> Void)?
    var onGoForward: (() -> Void)?
    var onAddNewTab: (() -> Void)?
    var onReload: (() -> Void)?
    var onToggleFocusOverview: (() -> Void)?
    var onToggleBookmark: (() -> Void)?
    var onPasswordIconTap: (() -> Void)?

    private init() {}

    func setupToolbar(for window: NSWindow, viewModel: BrowserViewModel, bookmarkVM: BookmarkViewModel) {
        // Remove Default toolbar, using custom titlebar
        window.toolbar = nil
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = false
        window.styleMask.insert(.fullSizeContentView)

        // Setup callbacks
        onGoBack = { [weak viewModel] in viewModel?.goBack() }
        onGoForward = { [weak viewModel] in viewModel?.goForward() }
        onAddNewTab = { [weak viewModel] in _ = viewModel?.addNewTab() }
        onReload = { [weak self, weak viewModel] in
            guard let viewModel = viewModel else { return }
            if self?.isLoading == true {
                viewModel.stopCurrentTab()
            } else {
                viewModel.reloadCurrentTab()
            }
        }
        onPasswordIconTap = { AutofillManager.shared.toggleSaveDialog() }

        // Create titlebar accessory view controller
        let accessoryVC = NSTitlebarAccessoryViewController()
        accessoryVC.layoutAttribute = .trailing  // Put it on the right side of the red and green light, and titlebar One.
        accessoryVC.fullScreenMinHeight = 52

        // Create SwiftUI toolbar View
        let toolbarView = TitlebarToolbarView(
            canGoBack: canGoBack,
            canGoForward: canGoForward,
            displayTitle: displayTitle,
            isOverviewMode: isOverviewMode,
            isBookmarked: isBookmarked,
            hasPendingSave: hasPendingSave,
            isLoading: isLoading,
            onBack: { [weak self] in self?.onGoBack?() },
            onForward: { [weak self] in self?.onGoForward?() },
            onAddressBarTap: { },
            onBookmarkTap: { [weak self] in self?.onToggleBookmark?() },
            onPasswordIconTap: { [weak self] in self?.onPasswordIconTap?() },
            onReloadTap: { [weak self] in self?.onReload?() },
            onNewTab: { [weak self] in self?.onAddNewTab?() },
            onOverview: { [weak self] in self?.onToggleFocusOverview?() }
        )

        let hostingView = NSHostingView(rootView: toolbarView)
        hostingView.translatesAutoresizingMaskIntoConstraints = false

        accessoryVC.view = hostingView
        self.accessoryViewController = accessoryVC
        self.toolbarHostingView = hostingView

        // Add to Window
        window.addTitlebarAccessoryViewController(accessoryVC)

        // Observe viewModel changes
        setupObservers(viewModel: viewModel, bookmarkVM: bookmarkVM)
    }

    private func setupObservers(viewModel: BrowserViewModel, bookmarkVM: BookmarkViewModel) {
        viewModel.objectWillChange
            .receive(on: RunLoop.main)
            .sink { [weak self, weak viewModel, weak bookmarkVM] _ in
                guard let viewModel = viewModel, let bookmarkVM = bookmarkVM else { return }
                self?.updateToolbarState(viewModel: viewModel, bookmarkVM: bookmarkVM)
            }
            .store(in: &cancellables)

        // Observe AutofillManager changes for key icon
        AutofillManager.shared.objectWillChange
            .receive(on: RunLoop.main)
            .sink { [weak self, weak viewModel, weak bookmarkVM] _ in
                guard let viewModel = viewModel, let bookmarkVM = bookmarkVM else { return }
                self?.updateToolbarState(viewModel: viewModel, bookmarkVM: bookmarkVM)
            }
            .store(in: &cancellables)
    }

    func updateToolbarState(viewModel: BrowserViewModel, bookmarkVM: BookmarkViewModel) {
        canGoBack = viewModel.canGoBack
        canGoForward = viewModel.canGoForward

        isOverviewMode = {
            if case .overview = viewModel.actriumMode { return true }
            return false
        }()

        displayTitle = {
            guard let tab = viewModel.currentTab else { return "New Tab" }
            if case .web = tab.contentType, let host = tab.url?.host {
                return host.hasPrefix("www.") ? String(host.dropFirst(4)) : host
            }
            return tab.title
        }()

        isBookmarked = viewModel.currentTab.map { bookmarkVM.isBookmarked(tab: $0) } ?? false
        hasPendingSave = AutofillManager.shared.pendingSaveAction != nil || AutofillManager.shared.hiddenSaveAction != nil
        isLoading = viewModel.isCurrentTabLoading

        // Update SwiftUI View
        let toolbarView = TitlebarToolbarView(
            canGoBack: canGoBack,
            canGoForward: canGoForward,
            displayTitle: displayTitle,
            isOverviewMode: isOverviewMode,
            isBookmarked: isBookmarked,
            hasPendingSave: hasPendingSave,
            isLoading: isLoading,
            onBack: { [weak self] in self?.onGoBack?() },
            onForward: { [weak self] in self?.onGoForward?() },
            onAddressBarTap: { },
            onBookmarkTap: { [weak self] in self?.onToggleBookmark?() },
            onPasswordIconTap: { [weak self] in self?.onPasswordIconTap?() },
            onReloadTap: { [weak self] in self?.onReload?() },
            onNewTab: { [weak self] in self?.onAddNewTab?() },
            onOverview: { [weak self] in self?.onToggleFocusOverview?() }
        )

        toolbarHostingView?.rootView = toolbarView
    }
}

// MARK: - Main Toolbar Delegate

final class MainToolbarDelegate: NSObject, NSToolbarDelegate {
    weak var viewModel: BrowserViewModel?
    weak var bookmarkVM: BookmarkViewModel?

    // Callbacks
    var onToggleFocusOverview: (() -> Void)?
    var onAddNewTab: (() -> Void)?
    var onGoBack: (() -> Void)?
    var onGoForward: (() -> Void)?
    var onReload: (() -> Void)?
    var onNavigate: ((String) -> Void)?
    var onToggleLeftPanel: (() -> Void)?
    var onToggleBookmark: (() -> Void)?

    // State bindings (updated externally)
    var canGoBack: Bool = false
    var canGoForward: Bool = false
    var isBookmarked: Bool = false
    var displayTitle: String = "New Tab"
    var currentURL: String = ""
    var isOverviewMode: Bool = false
    var viewportWidth: CGFloat = 800

    // Keep references to items for updating
    private var addressBarItem: NSToolbarItem?

    // MARK: - NSToolbarDelegate

    func toolbarDefaultItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        [.fullWidthBar]
    }

    func toolbarAllowedItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        [.fullWidthBar]
    }

    func toolbar(_ toolbar: NSToolbar, itemForItemIdentifier itemIdentifier: NSToolbarItem.Identifier, willBeInsertedIntoToolbar flag: Bool) -> NSToolbarItem? {
        switch itemIdentifier {
        case .fullWidthBar:
            return makeFullWidthBarItem()
        default:
            return nil
        }
    }

    // MARK: - Create Toolbar Items

    private func makeFullWidthBarItem() -> NSToolbarItem {
        let item = NSToolbarItem(itemIdentifier: .fullWidthBar)

        // Create a full width with all elements SwiftUI View
        let fullBarView = FullWidthToolbarView(
            canGoBack: canGoBack,
            canGoForward: canGoForward,
            displayTitle: displayTitle,
            isOverviewMode: isOverviewMode,
            isBookmarked: isBookmarked,
            onBack: { [weak self] in self?.onGoBack?() },
            onForward: { [weak self] in self?.onGoForward?() },
            onAddressBarTap: { [weak self] in
                // TODO: Enter edit mode
            },
            onBookmarkTap: { [weak self] in self?.onToggleBookmark?() },
            onReloadTap: { [weak self] in self?.onReload?() },
            onNewTab: { [weak self] in self?.onAddNewTab?() },
            onOverview: { [weak self] in self?.onToggleFocusOverview?() }
        )

        // Create a container view to ensure that the entire width is filled
        let hostingView = NSHostingView(rootView: fullBarView)
        hostingView.translatesAutoresizingMaskIntoConstraints = false

        // Packed in one NSView in order to set constraints
        let containerView = NSView()
        containerView.translatesAutoresizingMaskIntoConstraints = false
        containerView.addSubview(hostingView)

        NSLayoutConstraint.activate([
            hostingView.leadingAnchor.constraint(equalTo: containerView.leadingAnchor),
            hostingView.trailingAnchor.constraint(equalTo: containerView.trailingAnchor),
            hostingView.topAnchor.constraint(equalTo: containerView.topAnchor),
            hostingView.bottomAnchor.constraint(equalTo: containerView.bottomAnchor),
        ])

        item.view = containerView

        // Allow stretch to maximum width - That's the key!
        item.minSize = NSSize(width: 400, height: 38)
        item.maxSize = NSSize(width: 10000, height: 38)
        item.visibilityPriority = .user

        fullWidthBarItem = item
        return item
    }

    private var fullWidthBarItem: NSToolbarItem?

    // MARK: - Actions

    @objc private func goBackAction() {
        onGoBack?()
    }

    @objc private func goForwardAction() {
        onGoForward?()
    }

    @objc private func addNewTabAction() {
        onAddNewTab?()
    }

    @objc private func toggleOverviewAction() {
        onToggleFocusOverview?()
    }

    // MARK: - Update State

    func updateNavigationState(canGoBack: Bool, canGoForward: Bool) {
        self.canGoBack = canGoBack
        self.canGoForward = canGoForward
        // Navigation item It'll be next time. updateAddressBar Time Rebuild
    }

    func updateAddressBar(title: String, url: String, isBookmarked: Bool, isOverviewMode: Bool, width: CGFloat) {
        self.displayTitle = title
        self.currentURL = url
        self.isBookmarked = isBookmarked
        self.isOverviewMode = isOverviewMode
        self.viewportWidth = width

        // Reconstruction fullWidthBar View
        if let item = fullWidthBarItem {
            let fullBarView = FullWidthToolbarView(
                canGoBack: canGoBack,
                canGoForward: canGoForward,
                displayTitle: title,
                isOverviewMode: isOverviewMode,
                isBookmarked: isBookmarked,
                onBack: { [weak self] in self?.onGoBack?() },
                onForward: { [weak self] in self?.onGoForward?() },
                onAddressBarTap: { [weak self] in
                    // TODO: Enter edit mode
                },
                onBookmarkTap: { [weak self] in self?.onToggleBookmark?() },
                onReloadTap: { [weak self] in self?.onReload?() },
                onNewTab: { [weak self] in self?.onAddNewTab?() },
                onOverview: { [weak self] in self?.onToggleFocusOverview?() }
            )

            let hostingView = NSHostingView(rootView: fullBarView)
            item.view = hostingView
        }
    }
}

// MARK: - Titlebar Toolbar View (for NSTitlebarAccessoryViewController)

struct TitlebarToolbarView: View {
    let canGoBack: Bool
    let canGoForward: Bool
    let displayTitle: String
    let isOverviewMode: Bool
    let isBookmarked: Bool
    let hasPendingSave: Bool
    let isLoading: Bool
    let onBack: () -> Void
    let onForward: () -> Void
    let onAddressBarTap: () -> Void
    let onBookmarkTap: () -> Void
    let onPasswordIconTap: () -> Void
    let onReloadTap: () -> Void
    let onNewTab: () -> Void
    let onOverview: () -> Void

    var body: some View {
        HStack(spacing: 0) {
            // Left: Navigation button
            HStack(spacing: 0) {
                Button(action: onBack) {
                    Image(systemName: "chevron.left")
                        .font(.system(size: 13, weight: .medium))
                        .frame(width: 26, height: 22)
                }
                .disabled(!canGoBack)

                Divider()
                    .frame(height: 14)

                Button(action: onForward) {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 13, weight: .medium))
                        .frame(width: 26, height: 22)
                }
                .disabled(!canGoForward)
            }
            .buttonStyle(.borderless)
            .foregroundColor(.white.opacity(0.8))
            .background(Color.white.opacity(0.1))
            .clipShape(RoundedRectangle(cornerRadius: 6))

            Spacer()

            // Middle: Address bar (%) 50% width)
            GeometryReader { geometry in
                let addressBarWidth = geometry.size.width

                if !isOverviewMode {
                    HStack(spacing: 8) {
                        Text(displayTitle)
                            .foregroundColor(.white)
                            .lineLimit(1)
                            .frame(maxWidth: .infinity)

                        if hasPendingSave {
                            Button(action: onPasswordIconTap) {
                                Image(systemName: "key.fill")
                                    .font(.system(size: 12))
                                    .foregroundColor(.yellow)
                            }
                            .buttonStyle(.borderless)
                        }

                        Button(action: onBookmarkTap) {
                            Image(systemName: isBookmarked ? "star.fill" : "star")
                                .font(.system(size: 12))
                                .foregroundColor(isBookmarked ? .yellow : .white.opacity(0.6))
                        }
                        .buttonStyle(.borderless)

                        Button(action: onReloadTap) {
                            Image(systemName: isLoading ? "xmark" : "arrow.clockwise")
                                .font(.system(size: 12))
                                .foregroundColor(.white.opacity(0.6))
                        }
                        .buttonStyle(.borderless)
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 4)
                    .frame(width: addressBarWidth, height: 26)
                    .clipShape(RoundedRectangle(cornerRadius: 8))
                    .onTapGesture { onAddressBarTap() }
                }
            }
            .frame(maxWidth: .infinity)

            Spacer()

            // Right: Operation button
            HStack(spacing: 6) {
                Button(action: onNewTab) {
                    Image(systemName: "plus")
                        .font(.system(size: 13, weight: .medium))
                        .frame(width: 26, height: 22)
                }
                .help("New Tab")

                Button(action: onOverview) {
                    Image(systemName: "square.on.square")
                        .font(.system(size: 13, weight: .medium))
                        .frame(width: 26, height: 22)
                }
                .help("Tab Overview (⌘⇧\\)")
            }
            .buttonStyle(.borderless)
            .foregroundColor(.white.opacity(0.8))
        }
        .padding(.horizontal, 8)
        .frame(maxWidth: .infinity)
        .frame(height: 52)
    }
}

// MARK: - Full Width Toolbar View (Legacy)

struct FullWidthToolbarView: View {
    let canGoBack: Bool
    let canGoForward: Bool
    let displayTitle: String
    let isOverviewMode: Bool
    let isBookmarked: Bool
    var hasPendingSave: Bool = false
    let onBack: () -> Void
    let onForward: () -> Void
    let onAddressBarTap: () -> Void
    let onBookmarkTap: () -> Void
    var onPasswordIconTap: (() -> Void)? = nil
    let onReloadTap: () -> Void
    let onNewTab: () -> Void
    let onOverview: () -> Void

    var body: some View {
        GeometryReader { geometry in
            HStack(spacing: 0) {
                // Left: Navigation button
                HStack(spacing: 0) {
                    Button(action: onBack) {
                        Image(systemName: "chevron.left")
                            .font(.system(size: 14, weight: .medium))
                            .frame(width: 28, height: 24)
                    }
                    .disabled(!canGoBack)

                    Divider()
                        .frame(height: 16)

                    Button(action: onForward) {
                        Image(systemName: "chevron.right")
                            .font(.system(size: 14, weight: .medium))
                            .frame(width: 28, height: 24)
                    }
                    .disabled(!canGoForward)
                }
                .buttonStyle(.borderless)
                .foregroundColor(.white.opacity(0.8))
                .background(Color.white.opacity(0.1))
                .clipShape(RoundedRectangle(cornerRadius: 6))

                Spacer()

                // Middle: Address bar (%) 50% width)
                if !isOverviewMode {
                    HStack(spacing: 8) {
                        Text(displayTitle)
                            .foregroundColor(.white)
                            .lineLimit(1)
                            .frame(maxWidth: .infinity)

                        if hasPendingSave {
                            Button { onPasswordIconTap?() } label: {
                                Image(systemName: "key.fill")
                                    .font(.system(size: 13))
                                    .foregroundColor(.yellow)
                            }
                            .buttonStyle(.borderless)
                        }

                        Button(action: onBookmarkTap) {
                            Image(systemName: isBookmarked ? "star.fill" : "star")
                                .font(.system(size: 13))
                                .foregroundColor(isBookmarked ? .yellow : .white.opacity(0.6))
                        }
                        .buttonStyle(.borderless)

                        Button(action: onReloadTap) {
                            Image(systemName: "arrow.clockwise")
                                .font(.system(size: 13))
                                .foregroundColor(.white.opacity(0.6))
                        }
                        .buttonStyle(.borderless)
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 5)
                    .frame(width: geometry.size.width * 0.5, height: 28)
                    .clipShape(RoundedRectangle(cornerRadius: 8))
                    .onTapGesture {
                        onAddressBarTap()
                    }
                } else {
                    // Overview Show spaces in mode
                    Color.clear
                        .frame(width: geometry.size.width * 0.5, height: 28)
                }

                Spacer()

                // Right: Operation button
                HStack(spacing: 8) {
                    Button(action: onNewTab) {
                        Image(systemName: "plus")
                            .font(.system(size: 14, weight: .medium))
                            .frame(width: 28, height: 24)
                    }
                    .help("New Tab")

                    Button(action: onOverview) {
                        Image(systemName: "square.on.square")
                            .font(.system(size: 14, weight: .medium))
                            .frame(width: 28, height: 24)
                    }
                    .help("Tab Overview (⌘⇧\\)")
                }
                .buttonStyle(.borderless)
                .foregroundColor(.white.opacity(0.8))
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .frame(height: 28)
    }
}

// MARK: - Navigation Buttons View (Safari style) - Legacy

struct NavigationButtonsView: View {
    let onBack: () -> Void
    let onForward: () -> Void
    let canGoBack: Bool
    let canGoForward: Bool

    var body: some View {
        HStack(spacing: 0) {
            Button(action: onBack) {
                Image(systemName: "chevron.left")
                    .font(.system(size: 14, weight: .medium))
                    .frame(width: 28, height: 24)
            }
            .disabled(!canGoBack)

            Divider()
                .frame(height: 16)

            Button(action: onForward) {
                Image(systemName: "chevron.right")
                    .font(.system(size: 14, weight: .medium))
                    .frame(width: 28, height: 24)
            }
            .disabled(!canGoForward)
        }
        .buttonStyle(.borderless)
        .foregroundColor(.white.opacity(0.8))
        .background(Color.white.opacity(0.1))
        .clipShape(RoundedRectangle(cornerRadius: 6))
    }
}

// MARK: - Action Buttons View (Safari style)

struct ActionButtonsView: View {
    let onNewTab: () -> Void
    let onOverview: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            Button(action: onNewTab) {
                Image(systemName: "plus")
                    .font(.system(size: 14, weight: .medium))
                    .frame(width: 28, height: 24)
            }
            .help("New Tab")

            Button(action: onOverview) {
                Image(systemName: "square.on.square")
                    .font(.system(size: 14, weight: .medium))
                    .frame(width: 28, height: 24)
            }
            .help("Tab Overview (⌘⇧\\)")
        }
        .buttonStyle(.borderless)
        .foregroundColor(.white.opacity(0.8))
    }
}

// MARK: - Address Bar SwiftUI View (for NSToolbar)

struct AddressBarToolbarView: View {
    let displayTitle: String
    let isOverviewMode: Bool
    let isBookmarked: Bool
    let onTap: () -> Void
    let onBookmarkTap: () -> Void
    let onReloadTap: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            if !isOverviewMode {
                Text(displayTitle)
                    .foregroundColor(.white)
                    .lineLimit(1)
                    .frame(maxWidth: .infinity)

                Button(action: onBookmarkTap) {
                    Image(systemName: isBookmarked ? "star.fill" : "star")
                        .font(.system(size: 13))
                        .foregroundColor(isBookmarked ? .yellow : .white.opacity(0.6))
                }
                .buttonStyle(.borderless)

                Button(action: onReloadTap) {
                    Image(systemName: "arrow.clockwise")
                        .font(.system(size: 13))
                        .foregroundColor(.white.opacity(0.6))
                }
                .buttonStyle(.borderless)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 5)
        .frame(maxWidth: .infinity, minHeight: 28)
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .onTapGesture {
            if !isOverviewMode {
                onTap()
            }
        }
    }
}

// MARK: - Toolbar Setup Helper

func setupMainToolbar(for window: NSWindow, delegate: MainToolbarDelegate) {
    let toolbar = NSToolbar(identifier: "MainToolbar")
    toolbar.delegate = delegate
    toolbar.displayMode = .iconOnly
    toolbar.allowsUserCustomization = false
    toolbar.autosavesConfiguration = false

    window.toolbar = toolbar
    window.toolbarStyle = .unified
}
