import AppKit
import SwiftUI
import Combine

// MARK: - Window Toolbar Configurator

/// Sets up the AppKit toolbar when the SwiftUI view appears
struct WindowToolbarConfigurator: NSViewRepresentable {
    let viewModel: BrowserViewModel
    let bookmarkVM: BookmarkViewModel

    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        // Defer toolbar setup to next run loop to ensure window exists
        DispatchQueue.main.async {
            setupToolbar(for: view)
        }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        // Update address bar width when window resizes
        if let window = nsView.window,
           let toolbar = window.toolbar as? MainToolbar {
            toolbar.updateAddressBarWidth(windowWidth: window.frame.width)
        }
    }

    private func setupToolbar(for view: NSView) {
        guard let window = view.window else { return }

        // Check if toolbar already set up
        if window.toolbar is MainToolbar { return }

        let toolbar = MainToolbar()
        toolbar.configure(viewModel: viewModel, bookmarkVM: bookmarkVM)
        window.toolbar = toolbar
        window.toolbarStyle = .unified

        // Observe window resize to update address bar width
        NotificationCenter.default.addObserver(
            forName: NSWindow.didResizeNotification,
            object: window,
            queue: .main
        ) { notification in
            guard let window = notification.object as? NSWindow,
                  let toolbar = window.toolbar as? MainToolbar else { return }
            toolbar.updateAddressBarWidth(windowWidth: window.frame.width)
        }
    }
}

/// AppKit-based toolbar for precise layout control
/// Layout: [< >] [Spacer] [AddressBar 50%] [Spacer] [+ ⊞]
class MainToolbar: NSToolbar, NSToolbarDelegate {

    // MARK: - Item Identifiers

    private static let navigationGroupId = NSToolbarItem.Identifier("navigationGroup")
    private static let addressBarId = NSToolbarItem.Identifier("addressBar")
    private static let actionsGroupId = NSToolbarItem.Identifier("actionsGroup")
    private static let flexibleSpaceId = NSToolbarItem.Identifier.flexibleSpace

    // MARK: - State Bindings

    private var viewModel: BrowserViewModel?
    private var bookmarkVM: BookmarkViewModel?
    private var cancellables = Set<AnyCancellable>()

    // Toolbar item views (retained)
    private var addressBarHostingView: NSHostingView<AnyView>?
    private var navigationHostingView: NSHostingView<AnyView>?
    private var actionsHostingView: NSHostingView<AnyView>?

    // MARK: - Init

    override init(identifier: NSToolbar.Identifier = NSToolbar.Identifier("MainToolbar")) {
        super.init(identifier: identifier)
        self.delegate = self
        self.displayMode = .iconOnly
        self.allowsUserCustomization = false
        self.centeredItemIdentifier = Self.addressBarId
    }

    func configure(viewModel: BrowserViewModel, bookmarkVM: BookmarkViewModel) {
        self.viewModel = viewModel
        self.bookmarkVM = bookmarkVM

        // Observe changes to update toolbar items
        viewModel.$canvasMode
            .receive(on: DispatchQueue.main)
            .sink { [weak self] _ in
                self?.updateItems()
            }
            .store(in: &cancellables)

        viewModel.$currentTabId
            .receive(on: DispatchQueue.main)
            .sink { [weak self] _ in
                self?.updateItems()
            }
            .store(in: &cancellables)
    }

    private func updateItems() {
        // Force toolbar to re-validate items
        validateVisibleItems()
    }

    // MARK: - NSToolbarDelegate

    func toolbar(_ toolbar: NSToolbar, itemForItemIdentifier itemIdentifier: NSToolbarItem.Identifier, willBeInsertedIntoToolbar flag: Bool) -> NSToolbarItem? {
        switch itemIdentifier {
        case Self.navigationGroupId:
            return makeNavigationItem()
        case Self.addressBarId:
            return makeAddressBarItem()
        case Self.actionsGroupId:
            return makeActionsItem()
        default:
            return nil
        }
    }

    func toolbarDefaultItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        [
            Self.navigationGroupId,
            Self.flexibleSpaceId,
            Self.addressBarId,
            Self.flexibleSpaceId,
            Self.actionsGroupId
        ]
    }

    func toolbarAllowedItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        toolbarDefaultItemIdentifiers(toolbar)
    }

    // MARK: - Navigation Item

    private func makeNavigationItem() -> NSToolbarItem {
        let item = NSToolbarItem(itemIdentifier: Self.navigationGroupId)

        let view = NavigationButtonsView(viewModel: viewModel)
        let hostingView = NSHostingView(rootView: AnyView(view))
        hostingView.setContentHuggingPriority(.defaultHigh, for: .horizontal)

        item.view = hostingView
        self.navigationHostingView = hostingView

        return item
    }

    // MARK: - Address Bar Item

    private func makeAddressBarItem() -> NSToolbarItem {
        let item = NSToolbarItem(itemIdentifier: Self.addressBarId)

        guard let viewModel = viewModel, let bookmarkVM = bookmarkVM else {
            return item
        }

        let view = ToolbarAddressBar(viewModel: viewModel, bookmarkVM: bookmarkVM)
        let hostingView = NSHostingView(rootView: AnyView(view))

        // Set up size constraints for 50% width
        if let window = NSApp.mainWindow {
            let width = window.frame.width * 0.5
            hostingView.frame.size = CGSize(width: max(width, 200), height: 28)
        } else {
            hostingView.frame.size = CGSize(width: 400, height: 28)
        }

        // Use constraints instead of deprecated minSize/maxSize
        hostingView.translatesAutoresizingMaskIntoConstraints = false
        hostingView.widthAnchor.constraint(greaterThanOrEqualToConstant: 200).isActive = true
        hostingView.heightAnchor.constraint(equalToConstant: 28).isActive = true

        item.view = hostingView
        self.addressBarHostingView = hostingView

        return item
    }

    // MARK: - Actions Item

    private func makeActionsItem() -> NSToolbarItem {
        let item = NSToolbarItem(itemIdentifier: Self.actionsGroupId)

        guard let viewModel = viewModel else {
            return item
        }

        let view = ActionButtonsView(viewModel: viewModel)
        let hostingView = NSHostingView(rootView: AnyView(view))
        hostingView.setContentHuggingPriority(.defaultHigh, for: .horizontal)

        item.view = hostingView
        self.actionsHostingView = hostingView

        return item
    }

    // MARK: - Update Address Bar Width

    func updateAddressBarWidth(windowWidth: CGFloat) {
        guard let hostingView = addressBarHostingView else { return }
        let width = max(windowWidth * 0.5, 200)
        hostingView.frame.size.width = width
        hostingView.needsLayout = true
    }
}

// MARK: - Navigation Buttons View

private struct NavigationButtonsView: View {
    weak var viewModel: BrowserViewModel?

    var body: some View {
        HStack(spacing: 8) {
            Button(action: { viewModel?.goBack() }) {
                Image(systemName: "chevron.left")
            }
            .disabled(!(viewModel?.canGoBack ?? false))
            .help("后退")

            Button(action: { viewModel?.goForward() }) {
                Image(systemName: "chevron.right")
            }
            .disabled(!(viewModel?.canGoForward ?? false))
            .help("前进")
        }
        .buttonStyle(.borderless)
    }
}

// MARK: - Action Buttons View

private struct ActionButtonsView: View {
    @ObservedObject var viewModel: BrowserViewModel

    var body: some View {
        HStack(spacing: 8) {
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
            }
            .help("新建标签页")

            Button(action: {
                viewModel.toggleOverview()
            }) {
                Image(systemName: "square.on.square")
            }
            .help("标签总览 (⌘⇧\\)")
        }
        .buttonStyle(.borderless)
    }
}

// MARK: - Toolbar Address Bar

private struct ToolbarAddressBar: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var bookmarkVM: BookmarkViewModel

    @State private var inputText: String = ""
    @State private var isEditing: Bool = false
    @State private var isFirstResponder: Bool = false
    @State private var showingBookmarkPopover: Bool = false
    @State private var isWebLoading: Bool = false

    var body: some View {
        Group {
            if case .overview = viewModel.canvasMode {
                // Overview mode: empty placeholder
                Color.clear
                    .frame(maxWidth: .infinity, minHeight: 28)
            } else {
                addressBarContent
            }
        }
        .frame(maxWidth: .infinity, minHeight: 28)
        .background(
            Capsule()
                .fill(Color(nsColor: NSColor.controlBackgroundColor))
        )
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

        return HStack(spacing: 8) {
            // Left icon
            AddressBarLeftIconView(state: leftIconState) {
                withAnimation(.easeInOut(duration: 0.2)) {
                    viewModel.toggleLeftPanelForCurrentTab()
                }
            }

            // Title / Input
            if isEditing {
                TextField("Enter URL or app id", text: $inputText)
                    .textFieldStyle(.plain)
                    .onSubmit {
                        viewModel.navigateCurrentTab(to: inputText)
                        isEditing = false
                    }
            } else {
                Text(displayTitle)
                    .lineLimit(1)
                    .frame(maxWidth: .infinity)
                    .contentShape(Rectangle())
                    .onTapGesture {
                        inputText = currentURLString
                        isEditing = true
                    }
            }

            // Bookmark button
            Button(action: { showingBookmarkPopover.toggle() }) {
                Image(systemName: isBookmarked ? "star.fill" : "star")
                    .foregroundColor(isBookmarked ? .yellow : .secondary)
            }
            .buttonStyle(.borderless)
            .popover(isPresented: $showingBookmarkPopover) {
                if let tab = viewModel.currentTab {
                    let existingBookmark = bookmarkVM.getBookmark(for: tab)
                    BookmarkEditSheet(
                        viewModel: bookmarkVM,
                        tab: existingBookmark == nil ? tab : nil,
                        existingBookmark: existingBookmark
                    )
                }
            }

            // Refresh button
            Button(action: { viewModel.reloadCurrentTab() }) {
                Image(systemName: "arrow.clockwise")
            }
            .buttonStyle(.borderless)
        }
        .padding(.horizontal, 12)
    }
}

// MARK: - Address Bar Left Icon

private enum AddressBarLeftIconState {
    case none, loading, menu
}

private struct AddressBarLeftIconView: View {
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
                .stroke(Color.secondary, style: StrokeStyle(lineWidth: 1.2, lineCap: .round))
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
                    .foregroundColor(.secondary)
            }
            .buttonStyle(.borderless)
        }
    }
}
