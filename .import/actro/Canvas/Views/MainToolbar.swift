import AppKit
import SwiftUI
import Combine

// MARK: - Window Toolbar Configurator

/// Sets up the AppKit toolbar when the SwiftUI view appears
struct WindowToolbarConfigurator: NSViewRepresentable {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var bookmarkVM: BookmarkViewModel
    @Binding var isWebLoading: Bool
    let toggleFocusOverview: () -> Void

    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async {
            setupToolbar(for: view)
        }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        // Trigger toolbar content update via notification
        if let window = nsView.window {
            NotificationCenter.default.post(
                name: .toolbarNeedsUpdate,
                object: window
            )
        }
    }

    private func setupToolbar(for view: NSView) {
        guard let window = view.window else { return }
        if window.toolbar is MainToolbar { return }

        let toolbar = MainToolbar()
        toolbar.configure(
            viewModel: viewModel,
            bookmarkVM: bookmarkVM,
            isWebLoadingBinding: _isWebLoading,
            toggleFocusOverview: toggleFocusOverview
        )
        window.toolbar = toolbar
        window.toolbarStyle = .unified
    }
}

extension Notification.Name {
    static let toolbarNeedsUpdate = Notification.Name("toolbarNeedsUpdate")
}

// MARK: - Main Toolbar

/// AppKit toolbar with single full-width item for precise layout control
class MainToolbar: NSToolbar, NSToolbarDelegate {

    private static let fullWidthItemId = NSToolbarItem.Identifier("fullWidthContent")

    private var viewModel: BrowserViewModel?
    private var bookmarkVM: BookmarkViewModel?
    private var isWebLoadingBinding: Binding<Bool>?
    private var toggleFocusOverview: (() -> Void)?
    private var contentHostingView: NSHostingView<AnyView>?
    private var cancellables = Set<AnyCancellable>()

    override init(identifier: NSToolbar.Identifier = NSToolbar.Identifier("MainToolbar")) {
        super.init(identifier: identifier)
        self.delegate = self
        self.displayMode = .iconOnly
        self.allowsUserCustomization = false
    }

    func configure(
        viewModel: BrowserViewModel,
        bookmarkVM: BookmarkViewModel,
        isWebLoadingBinding: Binding<Bool>,
        toggleFocusOverview: @escaping () -> Void
    ) {
        self.viewModel = viewModel
        self.bookmarkVM = bookmarkVM
        self.isWebLoadingBinding = isWebLoadingBinding
        self.toggleFocusOverview = toggleFocusOverview

        // Listen for update notifications
        NotificationCenter.default.publisher(for: .toolbarNeedsUpdate)
            .receive(on: DispatchQueue.main)
            .sink { [weak self] _ in
                self?.updateContent()
            }
            .store(in: &cancellables)

        // Observe window resize
        NotificationCenter.default.publisher(for: NSWindow.didResizeNotification)
            .receive(on: DispatchQueue.main)
            .sink { [weak self] notification in
                guard let window = notification.object as? NSWindow,
                      window.toolbar === self else { return }
                self?.updateContent()
            }
            .store(in: &cancellables)
    }

    private func updateContent() {
        guard let hostingView = contentHostingView,
              let viewModel = viewModel,
              let bookmarkVM = bookmarkVM,
              let isWebLoadingBinding = isWebLoadingBinding,
              let toggleFocusOverview = toggleFocusOverview else { return }

        // Get window width for address bar sizing
        let windowWidth = hostingView.window?.frame.width ?? 800

        let content = ToolbarContentView(
            viewModel: viewModel,
            bookmarkVM: bookmarkVM,
            isWebLoading: isWebLoadingBinding,
            windowWidth: windowWidth,
            toggleFocusOverview: toggleFocusOverview
        )
        hostingView.rootView = AnyView(content)
    }

    // MARK: - NSToolbarDelegate

    func toolbar(_ toolbar: NSToolbar, itemForItemIdentifier itemIdentifier: NSToolbarItem.Identifier, willBeInsertedIntoToolbar flag: Bool) -> NSToolbarItem? {
        guard itemIdentifier == Self.fullWidthItemId else { return nil }

        let item = NSToolbarItem(itemIdentifier: itemIdentifier)

        guard let viewModel = viewModel,
              let bookmarkVM = bookmarkVM,
              let isWebLoadingBinding = isWebLoadingBinding,
              let toggleFocusOverview = toggleFocusOverview else {
            return item
        }

        let windowWidth: CGFloat = 800 // Initial width, will be updated
        let content = ToolbarContentView(
            viewModel: viewModel,
            bookmarkVM: bookmarkVM,
            isWebLoading: isWebLoadingBinding,
            windowWidth: windowWidth,
            toggleFocusOverview: toggleFocusOverview
        )

        let hostingView = NSHostingView(rootView: AnyView(content))
        hostingView.translatesAutoresizingMaskIntoConstraints = false

        item.view = hostingView
        self.contentHostingView = hostingView

        return item
    }

    func toolbarDefaultItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        [Self.fullWidthItemId]
    }

    func toolbarAllowedItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        [Self.fullWidthItemId]
    }
}

// MARK: - Toolbar Content View (SwiftUI)

/// Full toolbar content with precise layout: [Left] [Spacer] [AddressBar 50%] [Spacer] [Right]
private struct ToolbarContentView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var bookmarkVM: BookmarkViewModel
    @Binding var isWebLoading: Bool
    let windowWidth: CGFloat
    let toggleFocusOverview: () -> Void

    var body: some View {
        let addressBarWidth = max(windowWidth * 0.5, 200)

        HStack(spacing: 0) {
            // 左功能组（左对齐）
            leftButtonsGroup
                .frame(minWidth: 60, alignment: .leading)

            Spacer(minLength: 12)

            // 地址栏（50% 窗口宽度）
            addressBarView
                .frame(width: addressBarWidth)

            Spacer(minLength: 12)

            // 右功能组（右对齐）
            rightButtonsGroup
                .frame(minWidth: 60, alignment: .trailing)
        }
        .frame(maxWidth: .infinity)
    }

    // MARK: - Left Buttons

    private var leftButtonsGroup: some View {
        HStack(spacing: 4) {
            Button(action: { viewModel.goBack() }) {
                Image(systemName: "chevron.left")
                    .font(.system(size: 14, weight: .medium))
            }
            .buttonStyle(ToolbarButtonStyle())
            .disabled(!viewModel.canGoBack)
            .help("后退")

            Button(action: { viewModel.goForward() }) {
                Image(systemName: "chevron.right")
                    .font(.system(size: 14, weight: .medium))
            }
            .buttonStyle(ToolbarButtonStyle())
            .disabled(!viewModel.canGoForward)
            .help("前进")
        }
    }

    // MARK: - Right Buttons

    private var rightButtonsGroup: some View {
        HStack(spacing: 4) {
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
            .buttonStyle(ToolbarButtonStyle())
            .help("新建标签页")

            Button(action: toggleFocusOverview) {
                Image(systemName: "square.on.square")
                    .font(.system(size: 14, weight: .medium))
            }
            .buttonStyle(ToolbarButtonStyle())
            .help("标签总览 (⌘⇧\\)")
        }
    }

    // MARK: - Address Bar

    @ViewBuilder
    private var addressBarView: some View {
        if case .overview = viewModel.canvasMode {
            // Overview 模式：空占位
            Color.clear
                .frame(height: 28)
        } else {
            ToolbarAddressBarView(
                viewModel: viewModel,
                bookmarkVM: bookmarkVM,
                isWebLoading: $isWebLoading
            )
        }
    }
}

// MARK: - Toolbar Button Style

private struct ToolbarButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .frame(width: 28, height: 22)
            .background(
                RoundedRectangle(cornerRadius: 5)
                    .fill(configuration.isPressed ? Color.white.opacity(0.1) : Color.clear)
            )
            .contentShape(Rectangle())
    }
}

// MARK: - Address Bar View

/// Address bar with focus ring animation - matches original SwiftUI implementation
private struct ToolbarAddressBarView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var bookmarkVM: BookmarkViewModel
    @Binding var isWebLoading: Bool

    @State private var inputText: String = ""
    @State private var isEditing: Bool = false
    @State private var isFirstResponder: Bool = false
    @State private var showingBookmarkPopover: Bool = false

    // Focus ring animation state
    @State private var focusRingScale: CGFloat = 1.0
    @State private var focusRingOpacity: Double = 0.0

    private let placeholder = "Enter URL (http/https) or app id"

    var body: some View {
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

        Group {
            if isEditing {
                // 编辑状态
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
                // 非编辑状态
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
                        .onTapGesture {
                            isEditing = true
                        }

                    bookmarkButton(isBookmarked: isBookmarked)
                    refreshButton
                }
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 4)
        .frame(maxWidth: .infinity, minHeight: 28)
        .background(
            Capsule()
                .fill(Color(nsColor: NSColor.controlBackgroundColor))
        )
        .overlay(
            Capsule()
                .stroke(
                    Color(red: 0.18, green: 0.56, blue: 0.96)
                        .opacity(focusRingOpacity),
                    lineWidth: 2.8
                )
                .scaleEffect(focusRingScale)
                .padding(.vertical, -4)
        )
        .onChange(of: isFirstResponder) { _, newValue in
            if !newValue && isEditing {
                isEditing = false
            }
        }
        .onChange(of: isEditing) { _, newValue in
            if newValue {
                inputText = currentURLString
                focusRingScale = 1.08
                focusRingOpacity = 0.0
                withAnimation(.spring(response: 0.25, dampingFraction: 0.7, blendDuration: 0.1)) {
                    focusRingScale = 1.0
                    focusRingOpacity = 1.0
                }
                DispatchQueue.main.async {
                    isFirstResponder = true
                }
            } else {
                withAnimation(.easeOut(duration: 0.15)) {
                    focusRingOpacity = 0.0
                }
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
        .help(isBookmarked ? "Edit Bookmark" : "Add Bookmark")
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
        .help("刷新")
    }
}

// MARK: - Address Bar Left Icon State

private enum AddressBarLeftIconState {
    case none
    case loading
    case menu
}

// MARK: - Address Bar Left Icon

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
                .stroke(
                    Color.white.opacity(0.7),
                    style: StrokeStyle(lineWidth: 1.2, lineCap: .round)
                )
                .frame(width: 12, height: 12)
                .rotationEffect(.degrees(rotation))
                .onAppear {
                    withAnimation(.linear(duration: 1).repeatForever(autoreverses: false)) {
                        rotation = 360
                    }
                }
        case .menu:
            Button(action: action) {
                Image(systemName: "list.bullet")
                    .foregroundColor(.white.opacity(0.7))
            }
            .buttonStyle(.borderless)
            .help("切换左侧面板")
        }
    }
}

// MARK: - Focusable TextField

private struct FocusableTextField: NSViewRepresentable {
    @Binding var text: String
    var placeholder: String
    @Binding var isFirstResponder: Bool
    var onSubmit: ((String) -> Void)?

    func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }

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
        if nsView.stringValue != text {
            nsView.stringValue = text
        }
        nsView.placeholderString = placeholder

        if isFirstResponder,
           let window = nsView.window,
           window.firstResponder != nsView.currentEditor() {
            window.makeFirstResponder(nsView)
            DispatchQueue.main.async {
                if let editor = nsView.currentEditor() {
                    editor.selectAll(nil)
                }
            }
        }
    }

    final class Coordinator: NSObject, NSTextFieldDelegate {
        var parent: FocusableTextField

        init(_ parent: FocusableTextField) {
            self.parent = parent
        }

        func controlTextDidChange(_ obj: Notification) {
            guard let field = obj.object as? NSTextField else { return }
            let raw = field.stringValue
            let cleaned = raw
                .components(separatedBy: .newlines)
                .joined(separator: " ")
            if cleaned != raw {
                field.stringValue = cleaned
            }
            parent.text = cleaned
        }

        func controlTextDidEndEditing(_ notification: Notification) {
            parent.isFirstResponder = false
        }

        @objc func handleAction(_ sender: NSTextField) {
            let text = sender.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
            if !text.isEmpty {
                parent.onSubmit?(text)
            }
            parent.isFirstResponder = false
        }
    }
}
