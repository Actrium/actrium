import AppKit
import SwiftUI
import Combine

// MARK: - Window Toolbar Setup

/// Configures NSToolbar with centeredItemIdentifier for Safari-style layout
struct ToolbarConfigurator: NSViewRepresentable {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var bookmarkVM: BookmarkViewModel
    @Binding var isWebLoading: Bool
    let toggleFocusOverview: () -> Void

    func makeNSView(context: Context) -> NSView {
        let view = NSView(frame: .zero)
        DispatchQueue.main.async {
            self.configureToolbar(for: view, context: context)
        }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        // Update toolbar when state changes
        guard let window = nsView.window,
              let coordinator = context.coordinator as? ToolbarCoordinator else { return }
        coordinator.updateToolbarItems()
    }

    func makeCoordinator() -> ToolbarCoordinator {
        ToolbarCoordinator(
            viewModel: viewModel,
            bookmarkVM: bookmarkVM,
            isWebLoading: _isWebLoading,
            toggleFocusOverview: toggleFocusOverview
        )
    }

    private func configureToolbar(for view: NSView, context: Context) {
        guard let window = view.window else { return }

        // Skip if already configured
        if window.toolbar?.identifier == ToolbarCoordinator.toolbarIdentifier { return }

        let toolbar = NSToolbar(identifier: ToolbarCoordinator.toolbarIdentifier)
        toolbar.delegate = context.coordinator
        toolbar.displayMode = .iconOnly
        toolbar.allowsUserCustomization = false

        // Key: Set centeredItemIdentifier for Safari-style centering
        toolbar.centeredItemIdentifier = ToolbarCoordinator.addressBarId

        window.toolbar = toolbar
        window.toolbarStyle = .unified

        context.coordinator.window = window
    }
}

// MARK: - Toolbar Coordinator

class ToolbarCoordinator: NSObject, NSToolbarDelegate {
    static let toolbarIdentifier = NSToolbar.Identifier("MainToolbar")
    static let navigationId = NSToolbarItem.Identifier("navigation")
    static let addressBarId = NSToolbarItem.Identifier("addressBar")
    static let actionsId = NSToolbarItem.Identifier("actions")

    weak var window: NSWindow?
    var viewModel: BrowserViewModel
    var bookmarkVM: BookmarkViewModel
    var isWebLoading: Binding<Bool>
    var toggleFocusOverview: () -> Void

    private var addressBarHostingView: NSHostingView<AnyView>?
    private var cancellables = Set<AnyCancellable>()

    init(viewModel: BrowserViewModel, bookmarkVM: BookmarkViewModel,
         isWebLoading: Binding<Bool>, toggleFocusOverview: @escaping () -> Void) {
        self.viewModel = viewModel
        self.bookmarkVM = bookmarkVM
        self.isWebLoading = isWebLoading
        self.toggleFocusOverview = toggleFocusOverview
        super.init()

        // Observe window resize
        NotificationCenter.default.publisher(for: NSWindow.didResizeNotification)
            .sink { [weak self] notification in
                guard let window = notification.object as? NSWindow,
                      window === self?.window else { return }
                self?.updateAddressBarWidth()
            }
            .store(in: &cancellables)
    }

    func updateToolbarItems() {
        updateAddressBarWidth()
    }

    private func updateAddressBarWidth() {
        guard let window = window, let hostingView = addressBarHostingView else { return }
        let width = max(window.frame.width * 0.5, 200)
        hostingView.frame.size.width = width
    }

    // MARK: - NSToolbarDelegate

    func toolbar(_ toolbar: NSToolbar, itemForItemIdentifier itemIdentifier: NSToolbarItem.Identifier,
                 willBeInsertedIntoToolbar flag: Bool) -> NSToolbarItem? {
        switch itemIdentifier {
        case Self.navigationId:
            return makeNavigationItem()
        case Self.addressBarId:
            return makeAddressBarItem()
        case Self.actionsId:
            return makeActionsItem()
        default:
            return nil
        }
    }

    func toolbarDefaultItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        [
            Self.navigationId,
            .flexibleSpace,
            Self.addressBarId,
            .flexibleSpace,
            Self.actionsId
        ]
    }

    func toolbarAllowedItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        toolbarDefaultItemIdentifiers(toolbar)
    }

    // MARK: - Toolbar Items

    private func makeNavigationItem() -> NSToolbarItem {
        let item = NSToolbarItem(itemIdentifier: Self.navigationId)

        let view = NavigationButtonsView(viewModel: viewModel)
        let hostingView = NSHostingView(rootView: view)
        hostingView.frame.size = hostingView.fittingSize

        item.view = hostingView
        return item
    }

    private func makeAddressBarItem() -> NSToolbarItem {
        let item = NSToolbarItem(itemIdentifier: Self.addressBarId)

        let windowWidth = window?.frame.width ?? 800
        let width = max(windowWidth * 0.5, 200)

        let view = AddressBarToolbarView(
            viewModel: viewModel,
            bookmarkVM: bookmarkVM,
            isWebLoading: isWebLoading
        )

        let hostingView = NSHostingView(rootView: AnyView(view))
        hostingView.frame.size = CGSize(width: width, height: 28)

        item.view = hostingView
        self.addressBarHostingView = hostingView

        return item
    }

    private func makeActionsItem() -> NSToolbarItem {
        let item = NSToolbarItem(itemIdentifier: Self.actionsId)

        let view = ActionButtonsView(
            viewModel: viewModel,
            toggleFocusOverview: toggleFocusOverview
        )
        let hostingView = NSHostingView(rootView: view)
        hostingView.frame.size = hostingView.fittingSize

        item.view = hostingView
        return item
    }
}

// MARK: - Navigation Buttons View

private struct NavigationButtonsView: View {
    @ObservedObject var viewModel: BrowserViewModel

    var body: some View {
        HStack(spacing: 2) {
            Button(action: { viewModel.goBack() }) {
                Image(systemName: "chevron.left")
                    .font(.system(size: 14, weight: .medium))
            }
            .buttonStyle(.borderless)
            .disabled(!viewModel.canGoBack)
            .help("后退")

            Button(action: { viewModel.goForward() }) {
                Image(systemName: "chevron.right")
                    .font(.system(size: 14, weight: .medium))
            }
            .buttonStyle(.borderless)
            .disabled(!viewModel.canGoForward)
            .help("前进")
        }
        .frame(height: 28)
    }
}

// MARK: - Action Buttons View

private struct ActionButtonsView: View {
    @ObservedObject var viewModel: BrowserViewModel
    let toggleFocusOverview: () -> Void

    var body: some View {
        HStack(spacing: 2) {
            DownloadToolbarButton()

            if case .focus = viewModel.canvasMode,
               let currentTab = viewModel.currentTab,
               currentTab.url != nil {
                ShareToolbarButton(url: currentTab.url)
                QRCodeToolbarButton(url: currentTab.url)
            }

            Button(action: {
                withAnimation(.spring(response: 0.35, dampingFraction: 1.0, blendDuration: 0)) {
                    _ = viewModel.addNewTab()
                }
            }) {
                Image(systemName: "plus")
                    .font(.system(size: 14, weight: .medium))
            }
            .buttonStyle(.borderless)
            .help("新建标签页")

            Button(action: toggleFocusOverview) {
                Image(systemName: "square.on.square")
                    .font(.system(size: 14, weight: .medium))
            }
            .buttonStyle(.borderless)
            .help("标签总览 (⌘⇧\\)")
        }
        .frame(height: 28)
    }
}

// MARK: - Address Bar Toolbar View

private struct AddressBarToolbarView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var bookmarkVM: BookmarkViewModel
    @Binding var isWebLoading: Bool

    @State private var inputText: String = ""
    @State private var isEditing: Bool = false
    @State private var isFirstResponder: Bool = false
    @State private var showingBookmarkPopover: Bool = false
    @State private var focusRingScale: CGFloat = 1.0
    @State private var focusRingOpacity: Double = 0.0

    private let placeholder = "Enter URL (http/https) or app id"

    var body: some View {
        Group {
            if case .overview = viewModel.canvasMode {
                Color.clear.frame(height: 28)
            } else {
                addressBarContent
            }
        }
        .frame(maxWidth: .infinity, minHeight: 28)
    }

    private var addressBarContent: some View {
        let leftIconState: AddressBarLeftIconState = {
            guard let tab = viewModel.currentTab else { return .none }
            switch tab.contentType {
            case .reactNative: return .menu
            case .web: return isWebLoading ? .loading : .none
            case .empty: return .none
            }
        }()

        let currentURLString = viewModel.currentTab?.url?.absoluteString ?? ""
        let displayTitle: String = {
            guard let tab = viewModel.currentTab else { return "New Tab" }
            if case .web = tab.contentType, let host = tab.url?.host {
                return host.hasPrefix("www.") ? String(host.dropFirst(4)) : host
            }
            return tab.title
        }()
        let isBookmarked = viewModel.currentTab.map { bookmarkVM.isBookmarked(tab: $0) } ?? false

        return Group {
            if isEditing {
                HStack(spacing: 10) {
                    Image(systemName: "magnifyingglass")
                        .foregroundStyle(Color.white.opacity(0.7))
                        .frame(width: 16, height: 16)

                    FocusableTextField(
                        text: $inputText,
                        placeholder: placeholder,
                        isFirstResponder: $isFirstResponder,
                        onSubmit: { text in
                            viewModel.navigateCurrentTab(to: text)
                            isFirstResponder = false
                        }
                    )

                    bookmarkButton(isBookmarked: isBookmarked)
                    refreshButton
                }
            } else {
                HStack(spacing: 10) {
                    AddressBarLeftIcon(state: leftIconState) {
                        withAnimation(.easeInOut(duration: 0.2)) {
                            viewModel.toggleLeftPanelForCurrentTab()
                        }
                    }

                    Text(displayTitle)
                        .foregroundColor(.white)
                        .multilineTextAlignment(.center)
                        .frame(maxWidth: .infinity)
                        .contentShape(Rectangle())
                        .onTapGesture { isEditing = true }

                    bookmarkButton(isBookmarked: isBookmarked)
                    refreshButton
                }
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 4)
        .background(Capsule().fill(Color(nsColor: NSColor.controlBackgroundColor)))
        .overlay(
            Capsule()
                .stroke(Color(red: 0.18, green: 0.56, blue: 0.96).opacity(focusRingOpacity), lineWidth: 2.8)
                .scaleEffect(focusRingScale)
                .padding(.vertical, -4)
        )
        .onChange(of: isFirstResponder) { _, newValue in
            if !newValue && isEditing { isEditing = false }
        }
        .onChange(of: isEditing) { _, newValue in
            if newValue {
                inputText = currentURLString
                focusRingScale = 1.08
                focusRingOpacity = 0.0
                withAnimation(.spring(response: 0.25, dampingFraction: 0.7)) {
                    focusRingScale = 1.0
                    focusRingOpacity = 1.0
                }
                DispatchQueue.main.async { isFirstResponder = true }
            } else {
                withAnimation(.easeOut(duration: 0.15)) { focusRingOpacity = 0.0 }
            }
        }
        .onChange(of: viewModel.shouldAutoFocusAddressBar) { _, newValue in
            if newValue {
                viewModel.shouldAutoFocusAddressBar = false
                isEditing = true
            }
        }
    }

    private func bookmarkButton(isBookmarked: Bool) -> some View {
        Button(action: { showingBookmarkPopover.toggle() }) {
            Image(systemName: isBookmarked ? "star.fill" : "star")
                .foregroundColor(isBookmarked ? .yellow : .white.opacity(0.7))
        }
        .buttonStyle(.borderless)
        .popover(isPresented: $showingBookmarkPopover, arrowEdge: .bottom) {
            if let tab = viewModel.currentTab {
                let existingBookmark = bookmarkVM.getBookmark(for: tab)
                BookmarkEditSheet(
                    viewModel: bookmarkVM,
                    tab: existingBookmark == nil ? tab : nil,
                    existingBookmark: existingBookmark
                )
            }
        }
    }

    private var refreshButton: some View {
        Button(action: { viewModel.reloadCurrentTab() }) {
            Image(systemName: "arrow.clockwise")
                .foregroundColor(.white.opacity(0.7))
        }
        .buttonStyle(.borderless)
    }
}

// MARK: - Address Bar Left Icon

private enum AddressBarLeftIconState { case none, loading, menu }

private struct AddressBarLeftIcon: View {
    let state: AddressBarLeftIconState
    let action: () -> Void
    @State private var rotation: Double = 0

    var body: some View {
        switch state {
        case .none:
            Color.clear.frame(width: 16, height: 16)
        case .loading:
            Circle()
                .trim(from: 0, to: 0.7)
                .stroke(Color.white.opacity(0.7), style: StrokeStyle(lineWidth: 1.2, lineCap: .round))
                .frame(width: 12, height: 12)
                .rotationEffect(.degrees(rotation))
                .onAppear {
                    withAnimation(.linear(duration: 1).repeatForever(autoreverses: false)) {
                        rotation = 360
                    }
                }
        case .menu:
            Button(action: action) {
                Image(systemName: "list.bullet").foregroundColor(.white.opacity(0.7))
            }
            .buttonStyle(.borderless)
        }
    }
}

// MARK: - Focusable TextField

private struct FocusableTextField: NSViewRepresentable {
    @Binding var text: String
    var placeholder: String
    @Binding var isFirstResponder: Bool
    var onSubmit: ((String) -> Void)?

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeNSView(context: Context) -> NSTextField {
        let field = NSTextField()
        field.isBordered = false
        field.isBezeled = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.font = NSFont.systemFont(ofSize: NSFont.systemFontSize)
        field.placeholderString = placeholder
        field.delegate = context.coordinator
        field.usesSingleLineMode = true
        field.maximumNumberOfLines = 1
        field.cell?.wraps = false
        field.cell?.isScrollable = true
        field.target = context.coordinator
        field.action = #selector(Coordinator.handleAction(_:))
        return field
    }

    func updateNSView(_ nsView: NSTextField, context: Context) {
        if nsView.stringValue != text { nsView.stringValue = text }
        nsView.placeholderString = placeholder
        if isFirstResponder, let window = nsView.window, window.firstResponder != nsView.currentEditor() {
            window.makeFirstResponder(nsView)
            DispatchQueue.main.async {
                nsView.currentEditor()?.selectAll(nil)
            }
        }
    }

    final class Coordinator: NSObject, NSTextFieldDelegate {
        var parent: FocusableTextField
        init(_ parent: FocusableTextField) { self.parent = parent }

        func controlTextDidChange(_ obj: Notification) {
            guard let field = obj.object as? NSTextField else { return }
            let raw = field.stringValue
            let cleaned = raw.components(separatedBy: .newlines).joined(separator: " ")
            if cleaned != raw { field.stringValue = cleaned }
            parent.text = cleaned
        }

        func controlTextDidEndEditing(_ notification: Notification) {
            parent.isFirstResponder = false
        }

        @objc func handleAction(_ sender: NSTextField) {
            let text = sender.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
            if !text.isEmpty { parent.onSubmit?(text) }
            parent.isFirstResponder = false
        }
    }
}
