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

// MARK: - Toolbar Container View (Safari Style)

private class ToolbarContainerView: NSView, NSTextFieldDelegate {

    weak var viewModel: BrowserViewModel?
    weak var bookmarkVM: BookmarkViewModel?

    // UI Elements
    private let navContainer: NSView  // Rounded background for nav buttons
    private let backButton: NSButton
    private let forwardButton: NSButton
    private let addressBarContainer: NSView  // Rounded pill for address bar
    private let addressField: NSTextField
    private let refreshButton: NSButton
    private let newTabButton: NSButton
    private let overviewButton: NSButton

    // Callbacks
    var onAddressBarTap: (() -> Void)?
    var onBookmarkTap: (() -> Void)?
    var onOverview: (() -> Void)?

    // Colors - Safari dark theme
    private let containerBgColor = NSColor(white: 0.18, alpha: 1.0)
    private let containerBorderColor = NSColor(white: 0.28, alpha: 1.0)
    private let addressBarBgColor = NSColor(white: 0.12, alpha: 1.0)

    init(viewModel: BrowserViewModel, bookmarkVM: BookmarkViewModel, onOverview: @escaping () -> Void) {
        self.viewModel = viewModel
        self.bookmarkVM = bookmarkVM
        self.onOverview = onOverview

        // Create navigation container with rounded background
        navContainer = NSView()
        navContainer.wantsLayer = true
        navContainer.layer?.backgroundColor = containerBgColor.cgColor
        navContainer.layer?.cornerRadius = 8
        navContainer.layer?.borderWidth = 0.5
        navContainer.layer?.borderColor = containerBorderColor.cgColor
        navContainer.translatesAutoresizingMaskIntoConstraints = false

        // Create nav buttons (larger, Safari style)
        backButton = Self.createNavButton(symbol: "chevron.left", tooltip: "Back (⌘[)")
        forwardButton = Self.createNavButton(symbol: "chevron.right", tooltip: "Forward (⌘])")

        // Create address bar container (pill shape)
        addressBarContainer = NSView()
        addressBarContainer.wantsLayer = true
        addressBarContainer.layer?.backgroundColor = addressBarBgColor.cgColor
        addressBarContainer.layer?.cornerRadius = 10
        addressBarContainer.layer?.borderWidth = 0.5
        addressBarContainer.layer?.borderColor = containerBorderColor.cgColor
        addressBarContainer.translatesAutoresizingMaskIntoConstraints = false

        // Create address field (plain text field, centered)
        addressField = NSTextField()
        addressField.isEditable = true
        addressField.isSelectable = true
        addressField.isBordered = false
        addressField.drawsBackground = false
        addressField.backgroundColor = .clear
        addressField.textColor = NSColor(white: 0.85, alpha: 1.0)
        addressField.font = NSFont.systemFont(ofSize: 13, weight: .regular)
        addressField.alignment = .center
        addressField.placeholderString = "Search or enter website"
        addressField.focusRingType = .none
        addressField.lineBreakMode = .byTruncatingTail
        addressField.cell?.truncatesLastVisibleLine = true
        addressField.translatesAutoresizingMaskIntoConstraints = false

        // Refresh button inside address bar
        refreshButton = Self.createInlineButton(symbol: "arrow.clockwise", tooltip: "Reload (⌘R)")

        // Create action buttons
        newTabButton = Self.createActionButton(symbol: "plus", tooltip: "New Tab (⌘T)")
        overviewButton = Self.createActionButton(symbol: "square.grid.2x2", tooltip: "Tab Overview (⌘⇧\\)")

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
        refreshButton.target = self
        refreshButton.action = #selector(refreshTapped)
        addressField.delegate = self
        addressField.target = self
        addressField.action = #selector(addressFieldSubmitted)

        setupView()
        updateState()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func setupView() {
        // Add nav buttons to nav container
        navContainer.addSubview(backButton)
        navContainer.addSubview(forwardButton)

        // Add address field and refresh to address bar container
        addressBarContainer.addSubview(addressField)
        addressBarContainer.addSubview(refreshButton)

        // Spacers for centering address bar
        let leftSpacer = NSView()
        let rightSpacer = NSView()
        leftSpacer.translatesAutoresizingMaskIntoConstraints = false
        rightSpacer.translatesAutoresizingMaskIntoConstraints = false

        // Add all to main view
        addSubview(navContainer)
        addSubview(leftSpacer)
        addSubview(addressBarContainer)
        addSubview(rightSpacer)
        addSubview(newTabButton)
        addSubview(overviewButton)

        let navHeight: CGFloat = 28
        let addressBarHeight: CGFloat = 28

        NSLayoutConstraint.activate([
            // Nav container
            navContainer.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 12),
            navContainer.centerYAnchor.constraint(equalTo: centerYAnchor),
            navContainer.heightAnchor.constraint(equalToConstant: navHeight),

            // Back button inside nav container
            backButton.leadingAnchor.constraint(equalTo: navContainer.leadingAnchor, constant: 4),
            backButton.centerYAnchor.constraint(equalTo: navContainer.centerYAnchor),
            backButton.widthAnchor.constraint(equalToConstant: 28),
            backButton.heightAnchor.constraint(equalToConstant: 24),

            // Forward button inside nav container
            forwardButton.leadingAnchor.constraint(equalTo: backButton.trailingAnchor, constant: 0),
            forwardButton.trailingAnchor.constraint(equalTo: navContainer.trailingAnchor, constant: -4),
            forwardButton.centerYAnchor.constraint(equalTo: navContainer.centerYAnchor),
            forwardButton.widthAnchor.constraint(equalToConstant: 28),
            forwardButton.heightAnchor.constraint(equalToConstant: 24),

            // Left spacer
            leftSpacer.leadingAnchor.constraint(equalTo: navContainer.trailingAnchor, constant: 12),
            leftSpacer.centerYAnchor.constraint(equalTo: centerYAnchor),
            leftSpacer.heightAnchor.constraint(equalToConstant: 1),
            leftSpacer.widthAnchor.constraint(greaterThanOrEqualToConstant: 8),

            // Address bar container (centered, flexible width)
            addressBarContainer.leadingAnchor.constraint(equalTo: leftSpacer.trailingAnchor),
            addressBarContainer.centerYAnchor.constraint(equalTo: centerYAnchor),
            addressBarContainer.heightAnchor.constraint(equalToConstant: addressBarHeight),
            addressBarContainer.widthAnchor.constraint(greaterThanOrEqualToConstant: 200),
            addressBarContainer.widthAnchor.constraint(lessThanOrEqualToConstant: 600),

            // Address field inside container
            addressField.leadingAnchor.constraint(equalTo: addressBarContainer.leadingAnchor, constant: 12),
            addressField.trailingAnchor.constraint(equalTo: refreshButton.leadingAnchor, constant: -4),
            addressField.centerYAnchor.constraint(equalTo: addressBarContainer.centerYAnchor),

            // Refresh button inside address bar (right side)
            refreshButton.trailingAnchor.constraint(equalTo: addressBarContainer.trailingAnchor, constant: -8),
            refreshButton.centerYAnchor.constraint(equalTo: addressBarContainer.centerYAnchor),
            refreshButton.widthAnchor.constraint(equalToConstant: 20),
            refreshButton.heightAnchor.constraint(equalToConstant: 20),

            // Right spacer (equal to left)
            rightSpacer.leadingAnchor.constraint(equalTo: addressBarContainer.trailingAnchor),
            rightSpacer.centerYAnchor.constraint(equalTo: centerYAnchor),
            rightSpacer.heightAnchor.constraint(equalToConstant: 1),
            rightSpacer.widthAnchor.constraint(greaterThanOrEqualToConstant: 8),
            rightSpacer.widthAnchor.constraint(equalTo: leftSpacer.widthAnchor),

            // Action buttons (right side)
            newTabButton.leadingAnchor.constraint(equalTo: rightSpacer.trailingAnchor, constant: 8),
            newTabButton.centerYAnchor.constraint(equalTo: centerYAnchor),
            newTabButton.widthAnchor.constraint(equalToConstant: 28),
            newTabButton.heightAnchor.constraint(equalToConstant: 28),

            overviewButton.leadingAnchor.constraint(equalTo: newTabButton.trailingAnchor, constant: 4),
            overviewButton.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -12),
            overviewButton.centerYAnchor.constraint(equalTo: centerYAnchor),
            overviewButton.widthAnchor.constraint(equalToConstant: 28),
            overviewButton.heightAnchor.constraint(equalToConstant: 28),
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

    @objc private func refreshTapped() {
        viewModel?.reloadCurrentTab()
    }

    @objc private func addressFieldSubmitted() {
        let input = addressField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !input.isEmpty else { return }
        viewModel?.navigateCurrentTab(to: input)
        // Resign first responder
        addressField.window?.makeFirstResponder(nil)
        // Restore display title
        updateState()
    }

    // MARK: - NSTextFieldDelegate

    func controlTextDidBeginEditing(_ obj: Notification) {
        // When editing starts, show full URL instead of domain
        if let tab = viewModel?.currentTab, let url = tab.url {
            addressField.stringValue = url.absoluteString
            // Select all text for easy replacement
            if let editor = addressField.currentEditor() {
                editor.selectAll(nil)
            }
        }
    }

    func controlTextDidEndEditing(_ obj: Notification) {
        // Restore display title when editing ends
        updateState()
    }

    // MARK: - State Update

    func updateState() {
        guard let viewModel = viewModel else { return }

        backButton.isEnabled = viewModel.canGoBack
        forwardButton.isEnabled = viewModel.canGoForward

        // Update button appearance based on enabled state
        backButton.contentTintColor = viewModel.canGoBack ? NSColor(white: 0.9, alpha: 1.0) : NSColor(white: 0.4, alpha: 1.0)
        forwardButton.contentTintColor = viewModel.canGoForward ? NSColor(white: 0.9, alpha: 1.0) : NSColor(white: 0.4, alpha: 1.0)

        // Show domain or title (not full URL) when not editing
        if addressField.currentEditor() == nil {
            let displayText: String = {
                guard let tab = viewModel.currentTab else { return "New Tab" }
                if case .web = tab.contentType, let host = tab.url?.host {
                    return host.hasPrefix("www.") ? String(host.dropFirst(4)) : host
                }
                return tab.title
            }()
            addressField.stringValue = displayText
        }
    }

    // MARK: - Button Factories

    /// Navigation button (inside rounded container)
    private static func createNavButton(symbol: String, tooltip: String) -> NSButton {
        let button = NSButton()
        let config = NSImage.SymbolConfiguration(pointSize: 14, weight: .medium)
        button.image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)?
            .withSymbolConfiguration(config)
        button.bezelStyle = .inline
        button.isBordered = false
        button.imagePosition = .imageOnly
        button.toolTip = tooltip
        button.contentTintColor = NSColor(white: 0.9, alpha: 1.0)
        button.translatesAutoresizingMaskIntoConstraints = false
        return button
    }

    /// Inline button (inside address bar)
    private static func createInlineButton(symbol: String, tooltip: String) -> NSButton {
        let button = NSButton()
        let config = NSImage.SymbolConfiguration(pointSize: 11, weight: .medium)
        button.image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)?
            .withSymbolConfiguration(config)
        button.bezelStyle = .inline
        button.isBordered = false
        button.imagePosition = .imageOnly
        button.toolTip = tooltip
        button.contentTintColor = NSColor(white: 0.5, alpha: 1.0)
        button.translatesAutoresizingMaskIntoConstraints = false
        return button
    }

    /// Action button (standalone)
    private static func createActionButton(symbol: String, tooltip: String) -> NSButton {
        let button = NSButton()
        let config = NSImage.SymbolConfiguration(pointSize: 14, weight: .medium)
        button.image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)?
            .withSymbolConfiguration(config)
        button.bezelStyle = .inline
        button.isBordered = false
        button.imagePosition = .imageOnly
        button.toolTip = tooltip
        button.contentTintColor = NSColor(white: 0.7, alpha: 1.0)
        button.translatesAutoresizingMaskIntoConstraints = false
        return button
    }
}
