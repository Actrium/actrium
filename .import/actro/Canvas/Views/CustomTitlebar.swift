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

        // Position toggle button at sidebar's right edge (5pt gap to border)
        let effectiveWidth = sidebarWidth < 50 ? 78 : sidebarWidth
        let toggleTrailing = toggleButton.trailingAnchor.constraint(equalTo: titlebarView.leadingAnchor, constant: effectiveWidth - 5)
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

        // Main toolbar - starts after sidebar (15pt gap from border)
        let leading = containerView.leadingAnchor.constraint(equalTo: titlebarView.leadingAnchor, constant: effectiveWidth + 15)
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

    /// Called from SwiftUI when sidebar width changes (including drag resize)
    func updateSidebarWidth(_ width: CGFloat) {
        guard width > minTogglePosition else { return }
        lastKnownSidebarWidth = width

        // Update toolbar position immediately when sidebar is expanded
        // Gap: toggle button at (width - 5), toolbar at (width + 15)
        if isSidebarExpanded {
            toggleTrailingConstraint?.constant = width - 5
            leadingConstraint?.constant = width + 15
            // Force layout update so SwiftUI GeometryReader picks up new size
            customToolbarView?.forceLayoutUpdate()
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
                targetTogglePosition = lastKnownSidebarWidth - 5
                targetToolbarPosition = lastKnownSidebarWidth + 15
            } else {
                targetTogglePosition = minTogglePosition
                targetToolbarPosition = minTogglePosition + 20
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
            // Expanding to last known width (5pt/15pt gap)
            targetTogglePosition = lastKnownSidebarWidth - 5
            targetToolbarPosition = lastKnownSidebarWidth + 15
        } else {
            // Collapsing to minimum position
            targetTogglePosition = minTogglePosition
            targetToolbarPosition = minTogglePosition + 20
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
        guard let viewModel = viewModel, let bookmarkVM = bookmarkVM else { return }

        let swiftUIView = LiquidGlassToolbar(
            viewModel: viewModel,
            bookmarkVM: bookmarkVM,
            onOverview: { [weak self] in self?.onOverview?() },
            onBookmarkTap: { [weak self] in self?.onBookmarkTap?() }
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

    func forceLayoutUpdate() {
        // Force the hosting view to recalculate its geometry
        hostingView?.needsLayout = true
        hostingView?.layoutSubtreeIfNeeded()
        needsLayout = true
        layoutSubtreeIfNeeded()
    }
}

// MARK: - SwiftUI Liquid Glass Toolbar

private struct LiquidGlassToolbar: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var bookmarkVM: BookmarkViewModel
    var onOverview: () -> Void
    var onBookmarkTap: () -> Void

    @State private var addressText: String = ""
    @State private var isAddressFocused: Bool = false
    @State private var showingBookmarkPopover: Bool = false
    /// Tracks the focus ring animation scale (starts larger, animates to 1.0)
    @State private var focusRingScale: CGFloat = 1.0
    /// Tracks if user has started typing (including IME composition)
    @State private var hasInputActivity: Bool = false

    // Safari 风格地址栏规则：
    // 1) 非编辑态只显示 host（去掉 scheme 和 www.）
    // 2) 点击/快捷键进入编辑态时，切换为完整 URL 并全选
    // 3) 编辑过程中不被外部的 URL/Title 变化覆盖，失焦后恢复精简展示
    private func displayText(for tab: Tab?) -> String {
        guard let tab = tab else { return "" }
        if case .web = tab.contentType, let url = tab.url {
            let host = url.host ?? url.absoluteString
            return host.hasPrefix("www.") ? String(host.dropFirst(4)) : host
        }
        return tab.title
    }

    private func editingText(for tab: Tab?) -> String {
        if let url = tab?.url {
            return url.absoluteString
        }
        return displayText(for: tab)
    }

    private func enterEditingMode() {
        addressText = editingText(for: viewModel.currentTab)
        // Start with expanded width, then animate to normal size
        focusRingScale = 1.20
        isAddressFocused = true
        withAnimation(.spring(response: 0.3, dampingFraction: 0.7)) {
            focusRingScale = 1.0
        }
    }

    private func exitEditingMode() {
        isAddressFocused = false
        focusRingScale = 1.0
        hasInputActivity = false  // Reset input activity state
        // addressText will be updated by onChange handlers when URL/Tab changes
    }

    // Breakpoints for responsive layout (Safari-style)
    // Normal: nav + 50% address bar + [share/+/tabs] capsule
    // Compact: nav + flexible address bar + [+] circle + [>>] circle
    // Narrow: flexible address bar + [>>] circle
    private let compactThreshold: CGFloat = 500
    private let narrowThreshold: CGFloat = 380

    var body: some View {
        GeometryReader { geo in
            let isNarrow = geo.size.width < narrowThreshold
            let isCompact = geo.size.width < compactThreshold && !isNarrow
            let isNormal = !isNarrow && !isCompact
            let addressBarWidth: CGFloat = max(200, geo.size.width * 0.5)

            HStack(spacing: 12) {
                // === Navigation Buttons (hidden in narrow mode) ===
                if !isNarrow {
                    navButtonsGroup
                }

                Spacer(minLength: 0)

                // === Address Bar ===
                addressBarView
                    .frame(width: isNormal ? addressBarWidth : nil)
                    .frame(maxWidth: isNormal ? nil : .infinity)

                Spacer(minLength: 0)

                // === Right side buttons ===
                if isNormal {
                    // Normal: [Share + Tabs] capsule
                    actionButtonsGroup
                } else if isCompact {
                    // Compact: [+] circle + [>>] circle
                    newTabButton
                    overflowMenuButton(showNavActions: false)
                } else {
                    // Narrow: [>>] circle only
                    overflowMenuButton(showNavActions: true)
                }
            }
            .frame(maxHeight: .infinity)
            .padding(.leading, 5)
            .padding(.trailing, 10)
            .animation(.easeInOut(duration: 0.2), value: isNarrow)
            .animation(.easeInOut(duration: 0.2), value: isCompact)
        }
        .onChange(of: viewModel.currentTab?.url) { _, _ in
            updateDisplayText()
        }
        .onChange(of: viewModel.currentTab?.title) { _, _ in
            updateDisplayText()
        }
        .onChange(of: viewModel.currentTabId) { _, _ in
            updateDisplayText()
        }
        .onChange(of: isAddressFocused) { _, focused in
            if !focused {
                // When exiting edit mode, update to display text
                updateAddressTextFromTab()
            }
        }
        .onAppear {
            updateDisplayText()
        }
        // Listen for Cmd-L shortcut (shouldAutoFocusAddressBar)
        .onChange(of: viewModel.shouldAutoFocusAddressBar) { _, shouldFocus in
            if shouldFocus {
                enterEditingMode()
                viewModel.shouldAutoFocusAddressBar = false
            }
        }
        // Listen for click-outside notification from main content area
        .onReceive(NotificationCenter.default.publisher(for: NSNotification.Name("com.canvas.dismissAddressBarFocus"))) { _ in
            exitEditingMode()
        }
    }

    private func updateAddressTextFromTab() {
        guard let tab = viewModel.currentTab else {
            addressText = ""
            return
        }
        addressText = displayText(for: tab)
    }

    private func updateDisplayText() {
        guard !isAddressFocused else { return }
        updateAddressTextFromTab()
    }
    
    private func handleBlur() {
        exitEditingMode()
    }

    private func submitAddress() {
        let input = addressText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !input.isEmpty else { return }
        viewModel.navigateCurrentTab(to: input)
        exitEditingMode()
    }

    // MARK: - Subviews

    /// Navigation buttons [< | >]
    private var navButtonsGroup: some View {
        HStack(spacing: 0) {
            Button(action: {
                NSLog("🔘 [Toolbar] Back button tapped")
                viewModel.goBack()
            }) {
                Image(systemName: "chevron.left")
                    .font(.system(size: 16, weight: .medium))
                    .frame(width: 30, height: 30)
                    .contentShape(Circle())
            }
            .buttonStyle(.plain)
            .disabled(!viewModel.canGoBack)

            Divider()
                .frame(height: 18)
                .padding(.horizontal, 1)

            Button(action: {
                NSLog("🔘 [Toolbar] Forward button tapped")
                viewModel.goForward()
            }) {
                Image(systemName: "chevron.right")
                    .font(.system(size: 16, weight: .medium))
                    .frame(width: 30, height: 30)
                    .contentShape(Circle())
            }
            .buttonStyle(.plain)
            .disabled(!viewModel.canGoForward)
        }
        .foregroundStyle(.secondary)
        .padding(.horizontal, 5)
        .padding(.vertical, 3)
        .glassEffect(.regular, in: .capsule)
    }

    /// Unified address bar for Web and RN App tabs
    /// Layout: [loading/icon | text field | bookmark | refresh]
    private var addressBarView: some View {
        let mode = addressBarMode

        return HStack(spacing: 0) {
            // Left area: ONE icon only - loading spinner, mode icon, or search icon
            if mode.showsLoading && viewModel.isCurrentTabLoading {
                // Loading: show arc spinner
                ArcSpinner()
                    .frame(width: 32, height: 32)
            } else if let icon = currentLeftIcon {
                // Has mode icon (globe for web, custom/default app icon for actr)
                Button(action: {
                    NSLog("🔘 [Toolbar] Left button tapped (mode: \(mode))")
                }) {
                    Image(systemName: icon)
                        .font(.system(size: 14, weight: .medium))
                        .frame(width: 32, height: 32)
                        .contentShape(Circle())
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
            } else {
                // Empty mode: show search icon
                Image(systemName: "magnifyingglass")
                    .font(.system(size: 14, weight: .medium))
                    .foregroundStyle(.secondary)
                    .frame(width: 32, height: 32)
            }

            // Address display/input area (no icon here - icon is in left area)
            if isAddressFocused {
                // Editing: TextField with placeholder
                ZStack(alignment: .leading) {
                    // Placeholder text (hidden when any input activity including IME)
                    if addressText.isEmpty && !hasInputActivity {
                        Text("Search or enter website name")
                            .font(.system(size: 13))
                            .foregroundStyle(.secondary)
                            .allowsHitTesting(false)
                    }

                    // Text field
                    AddressInputField(
                        text: $addressText,
                        onSubmit: submitAddress,
                        onBlur: handleBlur,
                        onInputActivity: { hasInputActivity = true }
                    )
                }
                .frame(maxWidth: .infinity, maxHeight: 22)
                .padding(.horizontal, 8)
            } else {
                // Display: Label with host/app name, centered
                Group {
                    if addressText.isEmpty {
                        // Empty state: just placeholder text (icon is in left area)
                        Text("Search or enter website name")
                            .font(.system(size: 13))
                            .foregroundStyle(.secondary)
                    } else {
                        // Has content: show address
                        Text(addressText)
                            .font(.system(size: 13))
                            .foregroundStyle(.primary)
                            .lineLimit(1)
                            .truncationMode(.tail)
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: 22)
                .padding(.horizontal, 8)
                .contentShape(Rectangle())
                .onTapGesture {
                    enterEditingMode()
                }
            }

            // Bookmark button (Chrome-style: click always adds, popover to edit/remove)
            if mode.showsBookmark {
                Button(action: {
                    NSLog("🔘 [Toolbar] Bookmark button tapped, isBookmarked=\(isBookmarked)")
                    if !isBookmarked, let tab = viewModel.currentTab {
                        let bookmark = Bookmark.from(tab: tab, manufacturer: nil)
                        bookmarkVM.addBookmark(bookmark)
                    }
                    showingBookmarkPopover = true
                }) {
                    Image(systemName: isBookmarked ? "star.fill" : "star")
                        .font(.system(size: 14, weight: .medium))
                        .frame(width: 32, height: 32)
                        .contentShape(Circle())
                }
                .buttonStyle(.plain)
                .foregroundStyle(isBookmarked ? Color.secondary : Color.secondary.opacity(0.5))
                .popover(isPresented: $showingBookmarkPopover, arrowEdge: .bottom) {
                    bookmarkPopoverContent
                }
            }

            // Refresh button
            if mode.showsRefresh {
                Button(action: {
                    NSLog("🔘 [Toolbar] Refresh button tapped")
                    viewModel.reloadCurrentTab()
                }) {
                    Image(systemName: "arrow.clockwise")
                        .font(.system(size: 14, weight: .medium))
                        .frame(width: 32, height: 32)
                        .contentShape(Circle())
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
            }
        }
        .frame(height: 30)  // Match nav buttons height
        .padding(.horizontal, 4)
        .padding(.vertical, 3)  // Match nav buttons padding
        .glassEffect(.regular, in: .capsule)
        .overlay {
            Capsule()
                .strokeBorder(Color.blue, lineWidth: 2.5)
                .scaleEffect(x: focusRingScale, y: 1.0)
                .opacity(isAddressFocused ? 1 : 0)
        }
        .animation(.easeOut(duration: 0.15), value: isAddressFocused)
    }

    // MARK: - Tab Type Helpers

    private enum AddressBarMode {
        case web       // Web content (http/https)
        case app       // React Native app (.actr)
        case empty     // Empty/new tab

        var leftIcon: String? {
            switch self {
            case .web: return "globe"  // Web: globe icon
            case .app: return "app.badge.checkmark"
            case .empty: return nil
            }
        }

        var showsLoading: Bool {
            switch self {
            case .web: return true
            case .app: return true  // RN apps can also show loading
            case .empty: return false
            }
        }

        var showsBookmark: Bool {
            switch self {
            case .web, .app: return true
            case .empty: return false
            }
        }

        var showsRefresh: Bool {
            switch self {
            case .web: return true
            case .app: return true  // RN apps can be refreshed too
            case .empty: return false
            }
        }
    }

    private var addressBarMode: AddressBarMode {
        guard let tab = viewModel.currentTab else { return .empty }
        switch tab.contentType {
        case .web: return .web
        case .reactNative: return .app
        case .empty: return .empty
        }
    }

    /// Get the actual left icon, considering tab's custom appIcon
    private var currentLeftIcon: String? {
        let mode = addressBarMode
        guard let tab = viewModel.currentTab else { return mode.leftIcon }

        // For app mode, use custom appIcon if available
        if case .app = mode, let customIcon = tab.appIcon {
            return customIcon
        }

        return mode.leftIcon
    }

    private var isBookmarked: Bool {
        guard let tab = viewModel.currentTab else { return false }
        return bookmarkVM.isBookmarked(tab: tab)
    }

    @ViewBuilder
    private var bookmarkPopoverContent: some View {
        if let tab = viewModel.currentTab {
            let existingBookmark = bookmarkVM.getBookmark(for: tab)
            BookmarkEditSheet(
                viewModel: bookmarkVM,
                tab: tab,
                existingBookmark: existingBookmark
            )
        } else {
            Text("No tab selected")
                .padding()
        }
    }

    /// Action buttons group [Share + Tabs] in capsule (normal mode)
    private var actionButtonsGroup: some View {
        HStack(spacing: 4) {
            ShareLink(item: viewModel.currentTab?.url ?? URL(string: "about:blank")!) {
                Image(systemName: "square.and.arrow.up")
                    .font(.system(size: 15, weight: .medium))
                    .frame(width: 30, height: 30)
                    .contentShape(Circle())
            }
            .buttonStyle(.plain)

            Button(action: {
                NSLog("🔘 [Toolbar] New tab button tapped (normal mode)")
                _ = viewModel.addNewTab()
            }) {
                Image(systemName: "plus")
                    .font(.system(size: 16, weight: .medium))
                    .frame(width: 30, height: 30)
                    .contentShape(Circle())
            }
            .buttonStyle(.plain)

            Button(action: {
                NSLog("🔘 [Toolbar] Overview button tapped")
                onOverview()
            }) {
                Image(systemName: "square.on.square")
                    .font(.system(size: 15, weight: .medium))
                    .frame(width: 30, height: 30)
                    .contentShape(Circle())
            }
            .buttonStyle(.plain)
        }
        .foregroundStyle(.secondary)
        .padding(.horizontal, 5)
        .padding(.vertical, 3)
        .glassEffect(.regular, in: .capsule)
    }

    /// New tab button (circular, for compact mode)
    private var newTabButton: some View {
        Button(action: {
            NSLog("🔘 [Toolbar] New tab button tapped (compact mode)")
            _ = viewModel.addNewTab()
        }) {
            Image(systemName: "plus")
                .font(.system(size: 16, weight: .medium))
                .frame(width: 36, height: 36)
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(.secondary)
        .glassEffect(.regular, in: .circle)
    }

    /// Overflow menu button (circular)
    @ViewBuilder
    private func overflowMenuButton(showNavActions: Bool) -> some View {
        Menu {
            if showNavActions {
                Button(action: { viewModel.goBack() }) {
                    Label("Go Back", systemImage: "chevron.left")
                }
                .disabled(!viewModel.canGoBack)

                Button(action: { viewModel.goForward() }) {
                    Label("Go Forward", systemImage: "chevron.right")
                }
                .disabled(!viewModel.canGoForward)

                Divider()
            }

            if let url = viewModel.currentTab?.url {
                Button(action: {
                    let picker = NSSharingServicePicker(items: [url])
                    if let window = NSApp.keyWindow,
                       let contentView = window.contentView {
                        picker.show(relativeTo: .zero, of: contentView, preferredEdge: .maxY)
                    }
                }) {
                    Label("Share...", systemImage: "square.and.arrow.up")
                }
            }

            Button(action: { _ = viewModel.addNewTab() }) {
                Label("New Tab", systemImage: "plus")
            }

            Button(action: onOverview) {
                Label("Tab Overview", systemImage: "square.on.square")
            }
        } label: {
            Image(systemName: "chevron.right.2")
                .font(.system(size: 14, weight: .medium))
                .frame(width: 36, height: 36)
                .contentShape(Circle())
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .foregroundStyle(.secondary)
        .glassEffect(.regular, in: .circle)
    }

}

// MARK: - Address Input Field (for editing state)

/// Simple text field that auto-focuses and selects all on appear
private struct AddressInputField: NSViewRepresentable {
    @Binding var text: String
    var onSubmit: () -> Void
    var onBlur: () -> Void
    var onInputActivity: (() -> Void)?  // Called when any input activity (including IME)

    func makeNSView(context: Context) -> NSTextField {
        let textField = NSTextField()
        textField.font = .systemFont(ofSize: 13)
        textField.isBordered = false
        textField.drawsBackground = false
        textField.focusRingType = .none
        textField.delegate = context.coordinator
        textField.alignment = .left
        textField.lineBreakMode = .byTruncatingTail
        textField.maximumNumberOfLines = 1
        textField.cell?.isScrollable = true
        textField.cell?.wraps = false
        textField.usesSingleLineMode = true
        textField.isAutomaticTextCompletionEnabled = false  // Disable autocomplete popup
        textField.stringValue = text
        context.coordinator.textField = textField

        // Auto-focus and select all on appear
        DispatchQueue.main.async {
            textField.window?.makeFirstResponder(textField)
            textField.currentEditor()?.selectAll(nil)
        }

        return textField
    }

    func updateNSView(_ nsView: NSTextField, context: Context) {
        // Don't update text while editing to avoid cursor issues
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }

    class Coordinator: NSObject, NSTextFieldDelegate {
        var parent: AddressInputField
        weak var textField: NSTextField?

        init(_ parent: AddressInputField) {
            self.parent = parent
        }

        func controlTextDidEndEditing(_ obj: Notification) {
            parent.onBlur()
        }

        func controlTextDidBeginEditing(_ obj: Notification) {
            // Notify when editing starts (user typed something)
            parent.onInputActivity?()
        }

        func controlTextDidChange(_ obj: Notification) {
            guard let tf = obj.object as? NSTextField else { return }
            parent.text = tf.stringValue
            // Also notify on text change (covers IME composition)
            parent.onInputActivity?()
        }

        func control(_ control: NSControl, textView: NSTextView, doCommandBy commandSelector: Selector) -> Bool {
            if commandSelector == #selector(NSResponder.insertNewline(_:)) {
                parent.onSubmit()
                textField?.window?.makeFirstResponder(nil)
                return true
            }
            if commandSelector == #selector(NSResponder.cancelOperation(_:)) {
                textField?.window?.makeFirstResponder(nil)
                return true
            }
            return false
        }
    }
}

// MARK: - Arc Spinner (Safari-style loading indicator)

/// A rotating arc spinner using TimelineView for stable animation
/// Avoids animation restart issues when view reappears rapidly
private struct ArcSpinner: View {
    var body: some View {
        TimelineView(.animation) { timeline in
            let seconds = timeline.date.timeIntervalSinceReferenceDate
            let rotation = (seconds.truncatingRemainder(dividingBy: 0.8)) / 0.8 * 360

            Circle()
                .trim(from: 0, to: 0.7)
                .stroke(Color.secondary, style: StrokeStyle(lineWidth: 2, lineCap: .round))
                .frame(width: 14, height: 14)
                .rotationEffect(.degrees(rotation))
        }
    }
}
