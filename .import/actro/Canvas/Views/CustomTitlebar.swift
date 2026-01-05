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
    private var sidebarToggleButton: NSButton?
    private var toggleTrailingConstraint: NSLayoutConstraint?
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

        // Use default sidebar width initially (will be updated from SwiftUI)
        let sidebarWidth: CGFloat = 250

        // Create sidebar toggle button (separate, positioned in sidebar area)
        let toggleButton = NSButton()
        let config = NSImage.SymbolConfiguration(pointSize: 13, weight: .medium)
        toggleButton.image = NSImage(systemSymbolName: "sidebar.left", accessibilityDescription: nil)?
            .withSymbolConfiguration(config)
        toggleButton.bezelStyle = .accessoryBar
        toggleButton.isBordered = false
        toggleButton.imagePosition = .imageOnly
        toggleButton.toolTip = "Toggle Sidebar (⌃⌘S)"
        toggleButton.translatesAutoresizingMaskIntoConstraints = false
        toggleButton.target = self
        toggleButton.action = #selector(toggleSidebar)
        self.sidebarToggleButton = toggleButton

        titlebarView.addSubview(toggleButton)

        // Position toggle button at sidebar's right edge
        let effectiveWidth = sidebarWidth < 50 ? 78 : sidebarWidth
        let toggleTrailing = toggleButton.trailingAnchor.constraint(equalTo: titlebarView.leadingAnchor, constant: effectiveWidth - 8)
        toggleTrailing.isActive = true
        self.toggleTrailingConstraint = toggleTrailing

        NSLayoutConstraint.activate([
            toggleButton.centerYAnchor.constraint(equalTo: titlebarView.centerYAnchor),
            toggleButton.widthAnchor.constraint(equalToConstant: 28),
            toggleButton.heightAnchor.constraint(equalToConstant: 28),
        ])

        // Create main toolbar view (navigation, address bar, actions)
        let containerView = ToolbarContainerView(
            viewModel: viewModel,
            bookmarkVM: bookmarkVM,
            onOverview: { [weak self] in self?.onToggleFocusOverview?() }
        )
        containerView.translatesAutoresizingMaskIntoConstraints = false
        self.customToolbarView = containerView

        // Add to titlebar
        titlebarView.addSubview(containerView)

        // Main toolbar - starts after sidebar (leave space for toggle button)
        let leading = containerView.leadingAnchor.constraint(equalTo: titlebarView.leadingAnchor, constant: effectiveWidth)
        leading.isActive = true
        self.leadingConstraint = leading

        NSLayoutConstraint.activate([
            containerView.trailingAnchor.constraint(equalTo: titlebarView.trailingAnchor),
            containerView.topAnchor.constraint(equalTo: titlebarView.topAnchor),
            containerView.bottomAnchor.constraint(equalTo: titlebarView.bottomAnchor),
        ])

        // Observe viewModel changes
        setupObservers(viewModel: viewModel, bookmarkVM: bookmarkVM)
    }

    // Minimum position for toggle button (after traffic lights)
    // Left edge at 110px, button is 28px wide, so right edge at 138px
    private let minTogglePosition: CGFloat = 138

    /// Called from SwiftUI when sidebar width changes
    func updateSidebarWidth(_ width: CGFloat) {
        // Just save the width for future use, don't move the button
        // (button animation is handled by toggleSidebar)
        if width > minTogglePosition {
            lastKnownSidebarWidth = width
        }
    }

    private var lastKnownSidebarWidth: CGFloat = 250

    /// Called from SwiftUI when sidebar visibility changes
    func updateSidebarVisibility(_ visible: Bool) {
        // Handle sidebar toggled by other means (keyboard shortcut, menu, etc.)
        if isSidebarExpanded != visible {
            isSidebarExpanded = visible

            let targetTogglePosition: CGFloat
            let targetToolbarPosition: CGFloat

            if visible {
                targetTogglePosition = lastKnownSidebarWidth - 8
                targetToolbarPosition = lastKnownSidebarWidth
            } else {
                targetTogglePosition = minTogglePosition
                targetToolbarPosition = minTogglePosition + 8
            }

            // Animate to sync with sidebar
            NSAnimationContext.runAnimationGroup { context in
                context.duration = 0.25
                context.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
                toggleTrailingConstraint?.animator().constant = targetTogglePosition
                leadingConstraint?.animator().constant = targetToolbarPosition
            }
        }
    }

    private var isSidebarExpanded: Bool = true

    @objc private func toggleSidebar() {
        // Toggle state and start animation IMMEDIATELY (same time as sidebar)
        isSidebarExpanded.toggle()

        let targetTogglePosition: CGFloat
        let targetToolbarPosition: CGFloat

        if isSidebarExpanded {
            // Expanding to last known width
            targetTogglePosition = lastKnownSidebarWidth - 8
            targetToolbarPosition = lastKnownSidebarWidth
        } else {
            // Collapsing to minimum position
            targetTogglePosition = minTogglePosition
            targetToolbarPosition = minTogglePosition + 8
        }

        // Start animation immediately
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.25
            context.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
            toggleTrailingConstraint?.animator().constant = targetTogglePosition
            leadingConstraint?.animator().constant = targetToolbarPosition
        }

        // Trigger sidebar toggle at the same time
        NSApp.keyWindow?.firstResponder?.tryToPerform(#selector(NSSplitViewController.toggleSidebar(_:)), with: nil)
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
    let backButton: NSButton
    let forwardButton: NSButton
    let searchField: NSSearchField
    let newTabButton: NSButton
    let overviewButton: NSButton

    // Callbacks
    var onAddressBarTap: (() -> Void)?
    var onBookmarkTap: (() -> Void)?
    var onOverview: (() -> Void)?

    init(viewModel: BrowserViewModel, bookmarkVM: BookmarkViewModel, onOverview: @escaping () -> Void) {
        self.viewModel = viewModel
        self.bookmarkVM = bookmarkVM
        self.onOverview = onOverview

        // Create buttons
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
        backButton.target = self
        backButton.action = #selector(backTapped)
        forwardButton.target = self
        forwardButton.action = #selector(forwardTapped)
        newTabButton.target = self
        newTabButton.action = #selector(newTabTapped)
        overviewButton.target = self
        overviewButton.action = #selector(overviewTapped)
        searchField.delegate = self
        searchField.target = self
        searchField.action = #selector(searchFieldSubmitted)

        setupView()
        updateState()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func setupView() {
        // Navigation stack
        let navStack = NSStackView(views: [backButton, forwardButton])
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

        // Layout: [NavStack][LeftSpacer][SearchField][RightSpacer][ActionStack]
        NSLayoutConstraint.activate([
            // Nav stack - left
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

    @objc private func searchFieldSubmitted() {
        let input = searchField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !input.isEmpty else { return }
        viewModel?.navigateCurrentTab(to: input)
        // Resign first responder to dismiss keyboard
        searchField.window?.makeFirstResponder(nil)
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
