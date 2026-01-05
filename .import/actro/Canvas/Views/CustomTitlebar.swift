import AppKit
import SwiftUI
import Combine

// MARK: - Custom Titlebar Manager

@MainActor
final class TitlebarManager: NSObject {
    static let shared = TitlebarManager()

    private var customToolbarView: ToolbarContainerView?
    private var cancellables = Set<AnyCancellable>()
    private weak var viewModel: BrowserViewModel?
    private weak var bookmarkVM: BookmarkViewModel?

    private override init() {
        super.init()
    }

    private var leadingConstraint: NSLayoutConstraint?
    private weak var window: NSWindow?

    func setupTitlebar(for window: NSWindow, viewModel: BrowserViewModel, bookmarkVM: BookmarkViewModel) {
        self.viewModel = viewModel
        self.bookmarkVM = bookmarkVM
        self.window = window

        // Configure window for unified style
        window.styleMask.insert(.unifiedTitleAndToolbar)
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.toolbarStyle = .unified

        // Create empty toolbar (required for unified style & sidebar integration)
        let toolbar = NSToolbar(identifier: "EmptyToolbar")
        toolbar.displayMode = .iconOnly
        toolbar.showsBaselineSeparator = false
        window.toolbar = toolbar

        // Find the titlebar container view in window hierarchy
        guard let titlebarView = findTitlebarContainerView(in: window) else {
            print("⚠️ Could not find titlebar container view")
            return
        }

        // Create main toolbar view (includes sidebar toggle button)
        let containerView = ToolbarContainerView(
            viewModel: viewModel,
            bookmarkVM: bookmarkVM,
            onOverview: { [weak self] in self?.onToggleFocusOverview?() },
            onToggleSidebar: { [weak self] in self?.toggleSidebar() }
        )
        containerView.translatesAutoresizingMaskIntoConstraints = false
        self.customToolbarView = containerView

        // Add to titlebar
        titlebarView.addSubview(containerView)

        // Get initial sidebar width
        let sidebarWidth = getSidebarWidth(from: window)

        // Main toolbar - starts after sidebar
        let leading = containerView.leadingAnchor.constraint(equalTo: titlebarView.leadingAnchor, constant: sidebarWidth)
        leading.isActive = true
        self.leadingConstraint = leading

        NSLayoutConstraint.activate([
            containerView.trailingAnchor.constraint(equalTo: titlebarView.trailingAnchor),
            containerView.topAnchor.constraint(equalTo: titlebarView.topAnchor),
            containerView.bottomAnchor.constraint(equalTo: titlebarView.bottomAnchor),
        ])

        // Schedule periodic check for sidebar width (safe approach)
        scheduleSidebarWidthCheck()

        // Observe viewModel changes
        setupObservers(viewModel: viewModel, bookmarkVM: bookmarkVM)
    }

    private var sidebarCheckTimer: Timer?
    private var lastSidebarWidth: CGFloat = 0

    private func scheduleSidebarWidthCheck() {
        // Use a timer to periodically check sidebar width (avoids infinite loop)
        sidebarCheckTimer = Timer.scheduledTimer(withTimeInterval: 0.05, repeats: true) { [weak self] _ in
            guard let self = self, let window = self.window else { return }
            let newWidth = self.getSidebarWidth(from: window)

            // Only update if width actually changed and is reasonable
            if abs(newWidth - self.lastSidebarWidth) > 1 && newWidth >= 0 && newWidth < 1000 {
                self.lastSidebarWidth = newWidth

                // When sidebar is collapsed (width ~0), position toolbar near traffic lights
                let effectiveWidth = newWidth < 50 ? 78 : newWidth

                self.leadingConstraint?.constant = effectiveWidth

                // Force layout update
                self.customToolbarView?.layoutSubtreeIfNeeded()
            }
        }
    }

    @objc private func toggleSidebar() {
        // Toggle NavigationSplitView sidebar
        NSApp.keyWindow?.firstResponder?.tryToPerform(#selector(NSSplitViewController.toggleSidebar(_:)), with: nil)
    }

    private func getSidebarWidth(from window: NSWindow) -> CGFloat {
        // Try to find NSSplitView and get sidebar width
        if let splitView = findSplitView(in: window.contentView) {
            if splitView.subviews.count > 0 {
                let width = splitView.subviews[0].frame.width
                // Sanity check - sidebar shouldn't be wider than half the window
                if width > 0 && width < window.frame.width / 2 {
                    return width
                }
            }
        }
        // Default sidebar width
        return 250
    }

    private func findSplitView(in view: NSView?) -> NSSplitView? {
        guard let view = view else { return nil }
        if let splitView = view as? NSSplitView {
            return splitView
        }
        for subview in view.subviews {
            if let found = findSplitView(in: subview) {
                return found
            }
        }
        return nil
    }

    private func findTitlebarContainerView(in window: NSWindow) -> NSView? {
        // The titlebar container is typically window.contentView.superview's first subview
        // that contains the traffic lights and toolbar
        guard let themeFrame = window.contentView?.superview else { return nil }

        // Look for NSTitlebarContainerView
        for subview in themeFrame.subviews {
            let className = String(describing: type(of: subview))
            if className.contains("TitlebarContainerView") || className.contains("Titlebar") {
                // Find the actual titlebar view inside
                for innerView in subview.subviews {
                    let innerClassName = String(describing: type(of: innerView))
                    if innerClassName.contains("TitlebarView") {
                        return innerView
                    }
                }
                return subview
            }
        }

        // Fallback: use themeFrame itself
        return themeFrame
    }

    private func setupObservers(viewModel: BrowserViewModel, bookmarkVM: BookmarkViewModel) {
        viewModel.objectWillChange
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                self?.customToolbarView?.updateState()
            }
            .store(in: &cancellables)
    }

    var onToggleFocusOverview: (() -> Void)?

    var onToggleBookmark: (() -> Void)? {
        get { customToolbarView?.onBookmarkTap }
        set { customToolbarView?.onBookmarkTap = newValue }
    }

    var onAddressBarTap: (() -> Void)? {
        get { customToolbarView?.onAddressBarTap }
        set { customToolbarView?.onAddressBarTap = newValue }
    }
}

// MARK: - Toolbar Container View

private class ToolbarContainerView: NSView, NSSearchFieldDelegate {

    weak var viewModel: BrowserViewModel?
    weak var bookmarkVM: BookmarkViewModel?

    // UI Elements
    let sidebarToggleButton: NSButton
    let backButton: NSButton
    let forwardButton: NSButton
    let searchField: NSSearchField
    let newTabButton: NSButton
    let overviewButton: NSButton

    // Callbacks
    var onAddressBarTap: (() -> Void)?
    var onBookmarkTap: (() -> Void)?
    var onOverview: (() -> Void)?
    var onToggleSidebar: (() -> Void)?

    init(viewModel: BrowserViewModel, bookmarkVM: BookmarkViewModel, onOverview: @escaping () -> Void, onToggleSidebar: @escaping () -> Void) {
        self.viewModel = viewModel
        self.bookmarkVM = bookmarkVM
        self.onOverview = onOverview
        self.onToggleSidebar = onToggleSidebar

        // Create buttons
        sidebarToggleButton = Self.createButton(symbol: "sidebar.left", tooltip: "Toggle Sidebar (⌃⌘S)")
        backButton = Self.createButton(symbol: "chevron.left", tooltip: "Back")
        forwardButton = Self.createButton(symbol: "chevron.right", tooltip: "Forward")
        newTabButton = Self.createButton(symbol: "plus", tooltip: "New Tab")
        overviewButton = Self.createButton(symbol: "square.grid.2x2", tooltip: "Tab Overview (⌘⇧\\)")

        // Create search field
        searchField = NSSearchField()
        searchField.placeholderString = "Search or enter website"
        searchField.sendsSearchStringImmediately = false
        searchField.sendsWholeSearchString = true
        searchField.translatesAutoresizingMaskIntoConstraints = false

        super.init(frame: .zero)

        // Setup actions
        sidebarToggleButton.target = self
        sidebarToggleButton.action = #selector(sidebarToggleTapped)
        backButton.target = self
        backButton.action = #selector(backTapped)
        forwardButton.target = self
        forwardButton.action = #selector(forwardTapped)
        newTabButton.target = self
        newTabButton.action = #selector(newTabTapped)
        overviewButton.target = self
        overviewButton.action = #selector(overviewTapped)
        searchField.delegate = self

        setupView()
        updateState()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func setupView() {
        // Navigation stack (includes sidebar toggle)
        let navStack = NSStackView(views: [sidebarToggleButton, backButton, forwardButton])
        navStack.orientation = .horizontal
        navStack.spacing = 2
        navStack.translatesAutoresizingMaskIntoConstraints = false

        // Action stack
        let actionStack = NSStackView(views: [newTabButton, overviewButton])
        actionStack.orientation = .horizontal
        actionStack.spacing = 2
        actionStack.translatesAutoresizingMaskIntoConstraints = false

        // Spacers for equal distribution
        let leftSpacer = NSView()
        let rightSpacer = NSView()
        leftSpacer.translatesAutoresizingMaskIntoConstraints = false
        rightSpacer.translatesAutoresizingMaskIntoConstraints = false

        // Add subviews
        addSubview(navStack)
        addSubview(leftSpacer)
        addSubview(searchField)
        addSubview(rightSpacer)
        addSubview(actionStack)

        // Layout: [ToggleButton+NavStack][LeftSpacer][SearchField][RightSpacer][ActionStack]
        NSLayoutConstraint.activate([
            // Nav stack - left (now includes toggle button)
            navStack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            navStack.centerYAnchor.constraint(equalTo: centerYAnchor),

            // Left spacer
            leftSpacer.leadingAnchor.constraint(equalTo: navStack.trailingAnchor, constant: 8),
            leftSpacer.centerYAnchor.constraint(equalTo: centerYAnchor),
            leftSpacer.heightAnchor.constraint(equalToConstant: 1),
            leftSpacer.widthAnchor.constraint(greaterThanOrEqualToConstant: 20),

            // Search field - center
            searchField.leadingAnchor.constraint(equalTo: leftSpacer.trailingAnchor),
            searchField.centerYAnchor.constraint(equalTo: centerYAnchor),
            searchField.widthAnchor.constraint(greaterThanOrEqualToConstant: 200),
            searchField.widthAnchor.constraint(lessThanOrEqualToConstant: 500),

            // Right spacer - EQUAL to left spacer
            rightSpacer.leadingAnchor.constraint(equalTo: searchField.trailingAnchor),
            rightSpacer.centerYAnchor.constraint(equalTo: centerYAnchor),
            rightSpacer.heightAnchor.constraint(equalToConstant: 1),
            rightSpacer.widthAnchor.constraint(greaterThanOrEqualToConstant: 20),
            rightSpacer.widthAnchor.constraint(equalTo: leftSpacer.widthAnchor),

            // Action stack - right
            actionStack.leadingAnchor.constraint(equalTo: rightSpacer.trailingAnchor, constant: 8),
            actionStack.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),
            actionStack.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }

    // MARK: - Actions

    @objc private func sidebarToggleTapped() {
        onToggleSidebar?()
    }

    @objc private func backTapped() {
        viewModel?.goBack()
    }

    @objc private func forwardTapped() {
        viewModel?.goForward()
    }

    @objc private func newTabTapped() {
        _ = viewModel?.addNewTab()
    }

    @objc private func overviewTapped() {
        onOverview?()
    }

    // MARK: - State Update

    func updateState() {
        guard let viewModel = viewModel else { return }

        backButton.isEnabled = viewModel.canGoBack
        forwardButton.isEnabled = viewModel.canGoForward

        let title: String = {
            guard let tab = viewModel.currentTab else { return "New Tab" }
            if case .web = tab.contentType, let host = tab.url?.host {
                return host.hasPrefix("www.") ? String(host.dropFirst(4)) : host
            }
            return tab.title
        }()
        searchField.stringValue = title

        // Hide address bar in overview mode
        let isOverview: Bool = {
            if case .overview = viewModel.canvasMode { return true }
            return false
        }()
        searchField.isHidden = isOverview
    }

    // MARK: - NSSearchFieldDelegate

    func searchFieldDidStartSearching(_ sender: NSSearchField) {
        onAddressBarTap?()
    }

    // MARK: - Helpers

    private static func createButton(symbol: String, tooltip: String) -> NSButton {
        let button = NSButton()
        let config = NSImage.SymbolConfiguration(pointSize: 13, weight: .medium)
        button.image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)?
            .withSymbolConfiguration(config)
        button.bezelStyle = .accessoryBar
        button.isBordered = false
        button.imagePosition = .imageOnly
        button.toolTip = tooltip
        button.translatesAutoresizingMaskIntoConstraints = false
        button.widthAnchor.constraint(equalToConstant: 28).isActive = true
        button.heightAnchor.constraint(equalToConstant: 28).isActive = true
        return button
    }
}
