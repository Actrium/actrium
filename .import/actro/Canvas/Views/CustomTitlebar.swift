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
    private let addressBarContainer: NSView  // Rounded address bar
    private let addressLabel: NSTextField  // Display mode
    private let addressField: NSTextField  // Edit mode
    private let refreshButton: NSButton
    private let newTabButton: NSButton
    private let overviewButton: NSButton

    private var isEditing: Bool = false

    // Callbacks
    var onAddressBarTap: (() -> Void)?
    var onBookmarkTap: (() -> Void)?
    var onOverview: (() -> Void)?

    init(viewModel: BrowserViewModel, bookmarkVM: BookmarkViewModel, onOverview: @escaping () -> Void) {
        self.viewModel = viewModel
        self.bookmarkVM = bookmarkVM
        self.onOverview = onOverview

        // Create navigation container with rounded background
        navContainer = NSView()
        navContainer.wantsLayer = true
        navContainer.layer?.backgroundColor = NSColor(white: 1.0, alpha: 0.08).cgColor
        navContainer.layer?.cornerRadius = 8
        navContainer.translatesAutoresizingMaskIntoConstraints = false

        // Create nav buttons
        backButton = Self.createNavButton(symbol: "chevron.left", tooltip: "Back (⌘[)")
        forwardButton = Self.createNavButton(symbol: "chevron.right", tooltip: "Forward (⌘])")

        // Create address bar container
        addressBarContainer = NSView()
        addressBarContainer.wantsLayer = true
        addressBarContainer.layer?.backgroundColor = NSColor(white: 1.0, alpha: 0.08).cgColor
        addressBarContainer.layer?.cornerRadius = 10
        addressBarContainer.translatesAutoresizingMaskIntoConstraints = false

        // Address label (display mode) - centered text
        addressLabel = NSTextField(labelWithString: "")
        addressLabel.alignment = .center
        addressLabel.font = NSFont.systemFont(ofSize: 13)
        addressLabel.textColor = NSColor.white.withAlphaComponent(0.9)
        addressLabel.backgroundColor = .clear
        addressLabel.isBezeled = false
        addressLabel.isEditable = false
        addressLabel.translatesAutoresizingMaskIntoConstraints = false

        // Address field (edit mode) - editable
        addressField = NSTextField()
        addressField.alignment = .center
        addressField.font = NSFont.systemFont(ofSize: 13)
        addressField.textColor = NSColor.white
        addressField.backgroundColor = .clear
        addressField.isBezeled = false
        addressField.focusRingType = .none
        addressField.drawsBackground = false
        addressField.placeholderString = "Search or enter website"
        addressField.isHidden = true
        addressField.translatesAutoresizingMaskIntoConstraints = false

        // Refresh button inside address bar
        refreshButton = Self.createToolbarButton(symbol: "arrow.clockwise", tooltip: "Reload (⌘R)")

        // Action buttons
        newTabButton = Self.createToolbarButton(symbol: "plus", tooltip: "New Tab (⌘T)")
        overviewButton = Self.createToolbarButton(symbol: "square.grid.2x2", tooltip: "Tab Overview (⌘⇧\\)")

        super.init(frame: .zero)

        // Setup actions
        backButton.target = self
        backButton.action = #selector(backTapped)
        forwardButton.target = self
        forwardButton.action = #selector(forwardTapped)
        refreshButton.target = self
        refreshButton.action = #selector(refreshTapped)
        newTabButton.target = self
        newTabButton.action = #selector(newTabTapped)
        overviewButton.target = self
        overviewButton.action = #selector(overviewTapped)
        addressField.delegate = self
        addressField.target = self
        addressField.action = #selector(addressFieldSubmitted)

        // Click gesture for address bar
        let clickGesture = NSClickGestureRecognizer(target: self, action: #selector(addressBarClicked))
        addressBarContainer.addGestureRecognizer(clickGesture)

        setupView()
        updateState()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func setupView() {
        // Add nav buttons to container
        navContainer.addSubview(backButton)
        navContainer.addSubview(forwardButton)

        // Add address bar elements
        addressBarContainer.addSubview(addressLabel)
        addressBarContainer.addSubview(addressField)
        addressBarContainer.addSubview(refreshButton)

        // Add all to main view
        addSubview(navContainer)
        addSubview(addressBarContainer)
        addSubview(newTabButton)
        addSubview(overviewButton)

        // Layout
        NSLayoutConstraint.activate([
            // Nav container
            navContainer.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            navContainer.centerYAnchor.constraint(equalTo: centerYAnchor),
            navContainer.heightAnchor.constraint(equalToConstant: 28),

            // Nav buttons inside container
            backButton.leadingAnchor.constraint(equalTo: navContainer.leadingAnchor, constant: 4),
            backButton.centerYAnchor.constraint(equalTo: navContainer.centerYAnchor),
            forwardButton.leadingAnchor.constraint(equalTo: backButton.trailingAnchor, constant: 0),
            forwardButton.centerYAnchor.constraint(equalTo: navContainer.centerYAnchor),
            forwardButton.trailingAnchor.constraint(equalTo: navContainer.trailingAnchor, constant: -4),

            // Address bar - flexible width, centered
            addressBarContainer.leadingAnchor.constraint(equalTo: navContainer.trailingAnchor, constant: 12),
            addressBarContainer.trailingAnchor.constraint(equalTo: newTabButton.leadingAnchor, constant: -12),
            addressBarContainer.centerYAnchor.constraint(equalTo: centerYAnchor),
            addressBarContainer.heightAnchor.constraint(equalToConstant: 28),

            // Address label (centered in container, leave space for refresh button)
            addressLabel.leadingAnchor.constraint(equalTo: addressBarContainer.leadingAnchor, constant: 12),
            addressLabel.trailingAnchor.constraint(equalTo: refreshButton.leadingAnchor, constant: -4),
            addressLabel.centerYAnchor.constraint(equalTo: addressBarContainer.centerYAnchor),

            // Address field (same position as label)
            addressField.leadingAnchor.constraint(equalTo: addressBarContainer.leadingAnchor, constant: 12),
            addressField.trailingAnchor.constraint(equalTo: refreshButton.leadingAnchor, constant: -4),
            addressField.centerYAnchor.constraint(equalTo: addressBarContainer.centerYAnchor),

            // Refresh button at right edge of address bar
            refreshButton.trailingAnchor.constraint(equalTo: addressBarContainer.trailingAnchor, constant: -6),
            refreshButton.centerYAnchor.constraint(equalTo: addressBarContainer.centerYAnchor),

            // Action buttons - right side
            newTabButton.trailingAnchor.constraint(equalTo: overviewButton.leadingAnchor, constant: -2),
            newTabButton.centerYAnchor.constraint(equalTo: centerYAnchor),
            overviewButton.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),
            overviewButton.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }

    // MARK: - Actions

    @objc private func backTapped() {
        viewModel?.goBack()
    }

    @objc private func forwardTapped() {
        viewModel?.goForward()
    }

    @objc private func refreshTapped() {
        viewModel?.reloadCurrentTab()
    }

    @objc private func newTabTapped() {
        _ = viewModel?.addNewTab()
    }

    @objc private func overviewTapped() {
        onOverview?()
    }

    @objc private func addressBarClicked() {
        enterEditMode()
    }

    @objc private func addressFieldSubmitted() {
        let input = addressField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        exitEditMode()
        guard !input.isEmpty else { return }
        viewModel?.navigateCurrentTab(to: input)
    }

    private func enterEditMode() {
        guard !isEditing else { return }
        isEditing = true

        // Show full URL in edit field
        let fullURL = viewModel?.currentTab?.url?.absoluteString ?? ""
        addressField.stringValue = fullURL

        addressLabel.isHidden = true
        addressField.isHidden = false

        // Focus and select all
        window?.makeFirstResponder(addressField)
        addressField.selectText(nil)

        // Highlight address bar
        addressBarContainer.layer?.backgroundColor = NSColor(white: 1.0, alpha: 0.12).cgColor
    }

    private func exitEditMode() {
        guard isEditing else { return }
        isEditing = false

        addressLabel.isHidden = false
        addressField.isHidden = true

        window?.makeFirstResponder(nil)

        // Reset background
        addressBarContainer.layer?.backgroundColor = NSColor(white: 1.0, alpha: 0.08).cgColor
    }

    // MARK: - NSTextFieldDelegate

    func controlTextDidEndEditing(_ obj: Notification) {
        exitEditMode()
    }

    // MARK: - State Update

    func updateState() {
        guard let viewModel = viewModel else { return }

        backButton.isEnabled = viewModel.canGoBack
        forwardButton.isEnabled = viewModel.canGoForward

        // Update button appearance based on enabled state
        backButton.contentTintColor = viewModel.canGoBack ? .white : .white.withAlphaComponent(0.3)
        forwardButton.contentTintColor = viewModel.canGoForward ? .white : .white.withAlphaComponent(0.3)

        let title: String = {
            guard let tab = viewModel.currentTab else { return "New Tab" }
            if case .web = tab.contentType, let host = tab.url?.host {
                return host.hasPrefix("www.") ? String(host.dropFirst(4)) : host
            }
            return tab.title
        }()
        addressLabel.stringValue = title
    }

    // MARK: - Helpers

    private static func createNavButton(symbol: String, tooltip: String) -> NSButton {
        let button = NSButton()
        let config = NSImage.SymbolConfiguration(pointSize: 14, weight: .medium)
        button.image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)?
            .withSymbolConfiguration(config)
        button.bezelStyle = .inline
        button.isBordered = false
        button.imagePosition = .imageOnly
        button.toolTip = tooltip
        button.contentTintColor = .white
        button.translatesAutoresizingMaskIntoConstraints = false
        button.widthAnchor.constraint(equalToConstant: 28).isActive = true
        button.heightAnchor.constraint(equalToConstant: 24).isActive = true
        return button
    }

    private static func createToolbarButton(symbol: String, tooltip: String) -> NSButton {
        let button = NSButton()
        let config = NSImage.SymbolConfiguration(pointSize: 14, weight: .regular)
        button.image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)?
            .withSymbolConfiguration(config)
        button.bezelStyle = .inline
        button.isBordered = false
        button.imagePosition = .imageOnly
        button.toolTip = tooltip
        button.contentTintColor = .white.withAlphaComponent(0.85)
        button.translatesAutoresizingMaskIntoConstraints = false
        button.widthAnchor.constraint(equalToConstant: 28).isActive = true
        button.heightAnchor.constraint(equalToConstant: 28).isActive = true
        return button
    }
}
