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
        let config = NSImage.SymbolConfiguration(pointSize: 16, weight: .medium)
        toggleButton.image = NSImage(systemSymbolName: "sidebar.left", accessibilityDescription: nil)?
            .withSymbolConfiguration(config)
        toggleButton.bezelStyle = .accessoryBar
        toggleButton.isBordered = false
        toggleButton.imagePosition = .imageOnly
        toggleButton.contentTintColor = NSColor(white: 0.55, alpha: 1.0)
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
            toggleButton.widthAnchor.constraint(equalToConstant: 30),
            toggleButton.heightAnchor.constraint(equalToConstant: 30),
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

// MARK: - Toolbar Container View (SwiftUI with Liquid Glass)

private class ToolbarContainerView: NSView {

    weak var viewModel: BrowserViewModel?
    weak var bookmarkVM: BookmarkViewModel?
    private var hostingView: NSHostingView<AnyView>?

    // Callbacks
    var onAddressBarTap: (() -> Void)?
    var onBookmarkTap: (() -> Void)?
    var onOverview: (() -> Void)?

    init(viewModel: BrowserViewModel, bookmarkVM: BookmarkViewModel, onOverview: @escaping () -> Void) {
        self.viewModel = viewModel
        self.bookmarkVM = bookmarkVM
        self.onOverview = onOverview

        super.init(frame: .zero)

        setupSwiftUIView()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func setupSwiftUIView() {
        guard let viewModel = viewModel else { return }

        let swiftUIView = LiquidGlassToolbar(
            viewModel: viewModel,
            onOverview: { [weak self] in self?.onOverview?() }
        )

        let hosting = NSHostingView(rootView: AnyView(swiftUIView))
        hosting.translatesAutoresizingMaskIntoConstraints = false
        // Transparent background for Liquid Glass effect
        hosting.wantsLayer = true
        hosting.layer?.backgroundColor = .clear
        addSubview(hosting)

        NSLayoutConstraint.activate([
            hosting.leadingAnchor.constraint(equalTo: leadingAnchor),
            hosting.trailingAnchor.constraint(equalTo: trailingAnchor),
            hosting.topAnchor.constraint(equalTo: topAnchor),
            hosting.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])

        self.hostingView = hosting
    }

    func updateState() {
        // SwiftUI handles updates via @ObservedObject
    }
}

// MARK: - SwiftUI Liquid Glass Toolbar

private struct LiquidGlassToolbar: View {
    @ObservedObject var viewModel: BrowserViewModel
    var onOverview: () -> Void

    @State private var addressText: String = ""
    @State private var isEditing: Bool = false
    @FocusState private var isAddressFocused: Bool

    var body: some View {
        GeometryReader { geo in
            HStack(spacing: 15) {
                // === Navigation Buttons (with Glass) ===
                HStack(spacing: 0) {
                    Button(action: { viewModel.goBack() }) {
                        Image(systemName: "chevron.left")
                            .font(.system(size: 16, weight: .medium))
                            .frame(width: 30, height: 30)
                    }
                    .buttonStyle(.plain)
                    .disabled(!viewModel.canGoBack)

                    Divider()
                        .frame(height: 18)
                        .padding(.horizontal, 1)

                    Button(action: { viewModel.goForward() }) {
                        Image(systemName: "chevron.right")
                            .font(.system(size: 16, weight: .medium))
                            .frame(width: 30, height: 30)
                    }
                    .buttonStyle(.plain)
                    .disabled(!viewModel.canGoForward)
                }
                .foregroundStyle(.secondary)
                .padding(.horizontal, 5)
                .padding(.vertical, 3)
                .glassEffect(.regular, in: .capsule)

                Spacer(minLength: 0)

                // === Address Bar (50% of container width) ===
                HStack(spacing: 8) {
                    Image(systemName: "magnifyingglass")
                        .font(.system(size: 14))
                        .foregroundStyle(.tertiary)

                    TextField("Search or enter website name", text: $addressText)
                        .textFieldStyle(.plain)
                        .font(.system(size: 14))
                        .focused($isAddressFocused)
                        .onSubmit {
                            submitAddress()
                        }
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 9)
                .frame(width: max(200, geo.size.width * 0.5))
                .glassEffect(.regular, in: .capsule)
                .overlay {
                    // Blue focus ring with shrink animation
                    Capsule()
                        .strokeBorder(Color.blue, lineWidth: isAddressFocused ? 2.5 : 0)
                        .scaleEffect(isAddressFocused ? 1.0 : 1.08)
                        .opacity(isAddressFocused ? 1 : 0)
                        .animation(.easeOut(duration: 0.25), value: isAddressFocused)
                }

                Spacer(minLength: 0)

                // === Action Buttons (with Glass) ===
                HStack(spacing: 4) {
                    ShareLink(item: viewModel.currentTab?.url ?? URL(string: "about:blank")!) {
                        Image(systemName: "square.and.arrow.up")
                            .font(.system(size: 15, weight: .medium))
                            .frame(width: 30, height: 30)
                    }
                    .buttonStyle(.plain)

                    Button(action: { _ = viewModel.addNewTab() }) {
                        Image(systemName: "plus")
                            .font(.system(size: 16, weight: .medium))
                            .frame(width: 30, height: 30)
                    }
                    .buttonStyle(.plain)

                    Button(action: onOverview) {
                        Image(systemName: "square.on.square")
                            .font(.system(size: 15, weight: .medium))
                            .frame(width: 30, height: 30)
                    }
                    .buttonStyle(.plain)
                }
                .foregroundStyle(.secondary)
                .padding(.horizontal, 5)
                .padding(.vertical, 3)
                .glassEffect(.regular, in: .capsule)
            }
            .frame(maxHeight: .infinity)
            .padding(.leading, 5)
            .padding(.trailing, 10)
        }
        // Tap outside to dismiss focus
        .contentShape(Rectangle())
        .onTapGesture {
            isAddressFocused = false
        }
        .onChange(of: viewModel.currentTab?.url) { _, _ in
            updateDisplayText()
        }
        .onChange(of: viewModel.currentTab?.title) { _, _ in
            updateDisplayText()
        }
        .onChange(of: isAddressFocused) { _, focused in
            if focused {
                // Show full URL when editing
                if let url = viewModel.currentTab?.url {
                    addressText = url.absoluteString
                }
                // Auto-select all text after focus
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) {
                    NSApp.sendAction(#selector(NSText.selectAll(_:)), to: nil, from: nil)
                }
            } else {
                updateDisplayText()
            }
        }
        .onAppear {
            updateDisplayText()
        }
    }

    private func updateDisplayText() {
        guard !isAddressFocused else { return }
        guard let tab = viewModel.currentTab else {
            addressText = ""
            return
        }
        if case .web = tab.contentType, let host = tab.url?.host {
            addressText = host.hasPrefix("www.") ? String(host.dropFirst(4)) : host
        } else if !tab.title.isEmpty {
            addressText = tab.title
        } else {
            addressText = ""
        }
    }

    private func submitAddress() {
        let input = addressText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !input.isEmpty else { return }
        viewModel.navigateCurrentTab(to: input)
        isAddressFocused = false
    }
}
