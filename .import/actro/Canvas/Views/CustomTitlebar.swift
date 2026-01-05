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

// MARK: - Toolbar Container View (Safari Liquid Glass Style)

private class ToolbarContainerView: NSView, NSTextFieldDelegate {

    weak var viewModel: BrowserViewModel?
    weak var bookmarkVM: BookmarkViewModel?

    // UI Elements
    private let navContainer: NSView
    private let navDivider: NSView
    private let backButton: NSButton
    private let forwardButton: NSButton
    private let addressBarContainer: AddressBarContainerView
    private let searchIcon: NSImageView
    private let addressField: NSTextField
    private let actionContainer: NSView
    private let shareButton: NSButton
    private let newTabButton: NSButton
    private let overviewButton: NSButton

    // Callbacks
    var onAddressBarTap: (() -> Void)?
    var onBookmarkTap: (() -> Void)?
    var onOverview: (() -> Void)?

    // Colors - Safari Liquid Glass dark theme
    private let pillBgColor = NSColor(white: 0.22, alpha: 1.0)
    private let pillBorderColor = NSColor(white: 0.35, alpha: 1.0)
    private let addressBarBgColor = NSColor(white: 0.13, alpha: 1.0)
    private let dividerColor = NSColor(white: 0.4, alpha: 1.0)
    private let iconColor = NSColor(white: 0.55, alpha: 1.0)
    private let focusBorderColor = NSColor(red: 0.2, green: 0.5, blue: 0.9, alpha: 1.0)

    init(viewModel: BrowserViewModel, bookmarkVM: BookmarkViewModel, onOverview: @escaping () -> Void) {
        self.viewModel = viewModel
        self.bookmarkVM = bookmarkVM
        self.onOverview = onOverview

        // === Navigation Container (pill with divider) ===
        navContainer = NSView()
        navContainer.wantsLayer = true
        navContainer.layer?.backgroundColor = pillBgColor.cgColor
        navContainer.layer?.cornerRadius = 16  // Full pill shape
        navContainer.layer?.borderWidth = 1
        navContainer.layer?.borderColor = pillBorderColor.cgColor
        navContainer.translatesAutoresizingMaskIntoConstraints = false

        // Divider between < and >
        navDivider = NSView()
        navDivider.wantsLayer = true
        navDivider.layer?.backgroundColor = dividerColor.cgColor
        navDivider.translatesAutoresizingMaskIntoConstraints = false

        backButton = Self.createNavButton(symbol: "chevron.left", tooltip: "Back (⌘[)")
        forwardButton = Self.createNavButton(symbol: "chevron.right", tooltip: "Forward (⌘])")

        // === Address Bar Container (large pill with focus ring) ===
        addressBarContainer = AddressBarContainerView()
        addressBarContainer.wantsLayer = true
        addressBarContainer.layer?.backgroundColor = addressBarBgColor.cgColor
        addressBarContainer.layer?.cornerRadius = 18  // Large pill
        addressBarContainer.layer?.borderWidth = 1.5
        addressBarContainer.layer?.borderColor = pillBorderColor.cgColor
        addressBarContainer.translatesAutoresizingMaskIntoConstraints = false

        // Search icon
        searchIcon = NSImageView()
        let searchConfig = NSImage.SymbolConfiguration(pointSize: 13, weight: .regular)
        searchIcon.image = NSImage(systemSymbolName: "magnifyingglass", accessibilityDescription: nil)?
            .withSymbolConfiguration(searchConfig)
        searchIcon.contentTintColor = iconColor
        searchIcon.translatesAutoresizingMaskIntoConstraints = false

        // Address field
        addressField = NSTextField()
        addressField.isEditable = true
        addressField.isSelectable = true
        addressField.isBordered = false
        addressField.drawsBackground = false
        addressField.backgroundColor = .clear
        addressField.textColor = NSColor(white: 0.75, alpha: 1.0)
        addressField.font = NSFont.systemFont(ofSize: 14, weight: .regular)
        addressField.alignment = .left
        addressField.placeholderString = "Search or enter website name"
        addressField.placeholderAttributedString = NSAttributedString(
            string: "Search or enter website name",
            attributes: [.foregroundColor: NSColor(white: 0.45, alpha: 1.0)]
        )
        addressField.focusRingType = .none
        addressField.lineBreakMode = .byTruncatingTail
        addressField.cell?.truncatesLastVisibleLine = true
        addressField.translatesAutoresizingMaskIntoConstraints = false

        // === Action Container (pill with buttons) ===
        actionContainer = NSView()
        actionContainer.wantsLayer = true
        actionContainer.layer?.backgroundColor = pillBgColor.cgColor
        actionContainer.layer?.cornerRadius = 16
        actionContainer.layer?.borderWidth = 1
        actionContainer.layer?.borderColor = pillBorderColor.cgColor
        actionContainer.translatesAutoresizingMaskIntoConstraints = false

        shareButton = Self.createActionButton(symbol: "square.and.arrow.up", tooltip: "Share")
        newTabButton = Self.createActionButton(symbol: "plus", tooltip: "New Tab (⌘T)")
        overviewButton = Self.createActionButton(symbol: "square.on.square", tooltip: "Tab Overview (⌘⇧\\)")

        super.init(frame: .zero)

        // Setup actions
        backButton.target = self
        backButton.action = #selector(backTapped)
        forwardButton.target = self
        forwardButton.action = #selector(forwardTapped)
        shareButton.target = self
        shareButton.action = #selector(shareTapped)
        newTabButton.target = self
        newTabButton.action = #selector(newTabTapped)
        overviewButton.target = self
        overviewButton.action = #selector(overviewTapped)
        addressField.delegate = self
        addressField.target = self
        addressField.action = #selector(addressFieldSubmitted)

        // Track focus for address bar border color
        addressBarContainer.onFocusChange = { [weak self] focused in
            self?.updateAddressBarBorder(focused: focused)
        }

        setupView()
        updateState()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func setupView() {
        // Nav container contents
        navContainer.addSubview(backButton)
        navContainer.addSubview(navDivider)
        navContainer.addSubview(forwardButton)

        // Address bar contents
        addressBarContainer.addSubview(searchIcon)
        addressBarContainer.addSubview(addressField)

        // Action container contents
        actionContainer.addSubview(shareButton)
        actionContainer.addSubview(newTabButton)
        actionContainer.addSubview(overviewButton)

        // Spacers
        let leftSpacer = NSView()
        let rightSpacer = NSView()
        leftSpacer.translatesAutoresizingMaskIntoConstraints = false
        rightSpacer.translatesAutoresizingMaskIntoConstraints = false

        // Add to main view
        addSubview(navContainer)
        addSubview(leftSpacer)
        addSubview(addressBarContainer)
        addSubview(rightSpacer)
        addSubview(actionContainer)

        let pillHeight: CGFloat = 32
        let addressBarHeight: CGFloat = 36

        NSLayoutConstraint.activate([
            // === Nav Container ===
            navContainer.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            navContainer.centerYAnchor.constraint(equalTo: centerYAnchor),
            navContainer.heightAnchor.constraint(equalToConstant: pillHeight),

            backButton.leadingAnchor.constraint(equalTo: navContainer.leadingAnchor, constant: 8),
            backButton.centerYAnchor.constraint(equalTo: navContainer.centerYAnchor),
            backButton.widthAnchor.constraint(equalToConstant: 24),
            backButton.heightAnchor.constraint(equalToConstant: 24),

            navDivider.leadingAnchor.constraint(equalTo: backButton.trailingAnchor, constant: 6),
            navDivider.centerYAnchor.constraint(equalTo: navContainer.centerYAnchor),
            navDivider.widthAnchor.constraint(equalToConstant: 1),
            navDivider.heightAnchor.constraint(equalToConstant: 16),

            forwardButton.leadingAnchor.constraint(equalTo: navDivider.trailingAnchor, constant: 6),
            forwardButton.trailingAnchor.constraint(equalTo: navContainer.trailingAnchor, constant: -8),
            forwardButton.centerYAnchor.constraint(equalTo: navContainer.centerYAnchor),
            forwardButton.widthAnchor.constraint(equalToConstant: 24),
            forwardButton.heightAnchor.constraint(equalToConstant: 24),

            // === Left Spacer ===
            leftSpacer.leadingAnchor.constraint(equalTo: navContainer.trailingAnchor, constant: 12),
            leftSpacer.centerYAnchor.constraint(equalTo: centerYAnchor),
            leftSpacer.heightAnchor.constraint(equalToConstant: 1),
            leftSpacer.widthAnchor.constraint(greaterThanOrEqualToConstant: 8),

            // === Address Bar ===
            addressBarContainer.leadingAnchor.constraint(equalTo: leftSpacer.trailingAnchor),
            addressBarContainer.centerYAnchor.constraint(equalTo: centerYAnchor),
            addressBarContainer.heightAnchor.constraint(equalToConstant: addressBarHeight),
            addressBarContainer.widthAnchor.constraint(greaterThanOrEqualToConstant: 280),
            addressBarContainer.widthAnchor.constraint(lessThanOrEqualToConstant: 700),

            searchIcon.leadingAnchor.constraint(equalTo: addressBarContainer.leadingAnchor, constant: 12),
            searchIcon.centerYAnchor.constraint(equalTo: addressBarContainer.centerYAnchor),
            searchIcon.widthAnchor.constraint(equalToConstant: 16),
            searchIcon.heightAnchor.constraint(equalToConstant: 16),

            addressField.leadingAnchor.constraint(equalTo: searchIcon.trailingAnchor, constant: 6),
            addressField.trailingAnchor.constraint(equalTo: addressBarContainer.trailingAnchor, constant: -12),
            addressField.centerYAnchor.constraint(equalTo: addressBarContainer.centerYAnchor),

            // === Right Spacer ===
            rightSpacer.leadingAnchor.constraint(equalTo: addressBarContainer.trailingAnchor),
            rightSpacer.centerYAnchor.constraint(equalTo: centerYAnchor),
            rightSpacer.heightAnchor.constraint(equalToConstant: 1),
            rightSpacer.widthAnchor.constraint(greaterThanOrEqualToConstant: 8),
            rightSpacer.widthAnchor.constraint(equalTo: leftSpacer.widthAnchor),

            // === Action Container ===
            actionContainer.leadingAnchor.constraint(equalTo: rightSpacer.trailingAnchor, constant: 12),
            actionContainer.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),
            actionContainer.centerYAnchor.constraint(equalTo: centerYAnchor),
            actionContainer.heightAnchor.constraint(equalToConstant: pillHeight),

            shareButton.leadingAnchor.constraint(equalTo: actionContainer.leadingAnchor, constant: 8),
            shareButton.centerYAnchor.constraint(equalTo: actionContainer.centerYAnchor),
            shareButton.widthAnchor.constraint(equalToConstant: 24),
            shareButton.heightAnchor.constraint(equalToConstant: 24),

            newTabButton.leadingAnchor.constraint(equalTo: shareButton.trailingAnchor, constant: 8),
            newTabButton.centerYAnchor.constraint(equalTo: actionContainer.centerYAnchor),
            newTabButton.widthAnchor.constraint(equalToConstant: 24),
            newTabButton.heightAnchor.constraint(equalToConstant: 24),

            overviewButton.leadingAnchor.constraint(equalTo: newTabButton.trailingAnchor, constant: 8),
            overviewButton.trailingAnchor.constraint(equalTo: actionContainer.trailingAnchor, constant: -8),
            overviewButton.centerYAnchor.constraint(equalTo: actionContainer.centerYAnchor),
            overviewButton.widthAnchor.constraint(equalToConstant: 24),
            overviewButton.heightAnchor.constraint(equalToConstant: 24),
        ])
    }

    private func updateAddressBarBorder(focused: Bool) {
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.15
            if focused {
                addressBarContainer.layer?.borderColor = focusBorderColor.cgColor
                addressBarContainer.layer?.borderWidth = 2.5
            } else {
                addressBarContainer.layer?.borderColor = pillBorderColor.cgColor
                addressBarContainer.layer?.borderWidth = 1.5
            }
        }
    }

    // MARK: - Actions

    @objc private func backTapped() {
        viewModel?.goBack()
    }

    @objc private func forwardTapped() {
        viewModel?.goForward()
    }

    @objc private func shareTapped() {
        guard let url = viewModel?.currentTab?.url,
              let view = shareButton.superview else { return }
        let picker = NSSharingServicePicker(items: [url])
        picker.show(relativeTo: shareButton.bounds, of: shareButton, preferredEdge: .minY)
    }

    @objc private func newTabTapped() {
        _ = viewModel?.addNewTab()
    }

    @objc private func overviewTapped() {
        onOverview?()
    }

    @objc private func addressFieldSubmitted() {
        let input = addressField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !input.isEmpty else { return }
        viewModel?.navigateCurrentTab(to: input)
        addressField.window?.makeFirstResponder(nil)
        updateState()
    }

    // MARK: - NSTextFieldDelegate

    func controlTextDidBeginEditing(_ obj: Notification) {
        addressBarContainer.onFocusChange?(true)
        if let tab = viewModel?.currentTab, let url = tab.url {
            addressField.stringValue = url.absoluteString
            if let editor = addressField.currentEditor() {
                editor.selectAll(nil)
            }
        }
    }

    func controlTextDidEndEditing(_ obj: Notification) {
        addressBarContainer.onFocusChange?(false)
        updateState()
    }

    // MARK: - State Update

    func updateState() {
        guard let viewModel = viewModel else { return }

        backButton.isEnabled = viewModel.canGoBack
        forwardButton.isEnabled = viewModel.canGoForward

        backButton.contentTintColor = viewModel.canGoBack ? iconColor : NSColor(white: 0.3, alpha: 1.0)
        forwardButton.contentTintColor = viewModel.canGoForward ? iconColor : NSColor(white: 0.3, alpha: 1.0)

        if addressField.currentEditor() == nil {
            let displayText: String = {
                guard let tab = viewModel.currentTab else { return "" }
                if case .web = tab.contentType, let host = tab.url?.host {
                    return host.hasPrefix("www.") ? String(host.dropFirst(4)) : host
                }
                if tab.title.isEmpty { return "" }
                return tab.title
            }()
            addressField.stringValue = displayText
        }
    }

    // MARK: - Button Factories

    private static func createNavButton(symbol: String, tooltip: String) -> NSButton {
        let button = NSButton()
        let config = NSImage.SymbolConfiguration(pointSize: 15, weight: .medium)
        button.image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)?
            .withSymbolConfiguration(config)
        button.bezelStyle = .inline
        button.isBordered = false
        button.imagePosition = .imageOnly
        button.toolTip = tooltip
        button.contentTintColor = NSColor(white: 0.55, alpha: 1.0)
        button.translatesAutoresizingMaskIntoConstraints = false
        return button
    }

    private static func createActionButton(symbol: String, tooltip: String) -> NSButton {
        let button = NSButton()
        let config = NSImage.SymbolConfiguration(pointSize: 14, weight: .medium)
        button.image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)?
            .withSymbolConfiguration(config)
        button.bezelStyle = .inline
        button.isBordered = false
        button.imagePosition = .imageOnly
        button.toolTip = tooltip
        button.contentTintColor = NSColor(white: 0.55, alpha: 1.0)
        button.translatesAutoresizingMaskIntoConstraints = false
        return button
    }
}

// MARK: - Address Bar Container (tracks focus)

private class AddressBarContainerView: NSView {
    var onFocusChange: ((Bool) -> Void)?

    override var acceptsFirstResponder: Bool { true }

    override func becomeFirstResponder() -> Bool {
        onFocusChange?(true)
        return super.becomeFirstResponder()
    }

    override func resignFirstResponder() -> Bool {
        onFocusChange?(false)
        return super.resignFirstResponder()
    }
}
