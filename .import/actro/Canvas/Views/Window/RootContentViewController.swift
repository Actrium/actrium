import AppKit
import SwiftUI
import Combine

/// Root content view controller - manages 4px edge inset and contains toolbar + split view
final class RootContentViewController: NSViewController {

    // MARK: - Properties

    private let viewModel: BrowserViewModel
    private let historyVM: HistoryViewModel
    private let bookmarkVM: BookmarkViewModel

    /// Edge inset (transparent border around content)
    private let edgeInset: CGFloat = 4

    /// Corner radius for content area
    private let cornerRadius: CGFloat = 12

    /// Toolbar height
    private let toolbarHeight: CGFloat = 52

    /// Window background color (TODO: make configurable via settings)
    private let windowBackgroundColor = NSColor(red: 0.1, green: 0.2, blue: 0.4, alpha: 1.0)

    /// Traffic lights position (absolute, in our view's coordinates)
    private let trafficLightPosition = NSPoint(x: 15, y: 0)
    private let trafficLightSpacing: CGFloat = 8  // Gap between buttons (system default ~6-8px)
    private let trafficLightTopInset: CGFloat = 17  // Distance from top
    private let trafficLightButtonTypes: [NSWindow.ButtonType] = [.closeButton, .miniaturizeButton, .zoomButton]

    /// Keep original system superviews so fullscreen can restore native behavior.
    private var trafficLightOriginalSuperviews: [NSWindow.ButtonType: NSView] = [:]
    private var trafficLightConstraints: [NSWindow.ButtonType: [NSLayoutConstraint]] = [:]

    // Child views
    private var roundedContainer: NSView!
    private var toolbarHostingView: NSHostingView<AnyView>!
    private var splitViewController: MainSplitViewController!

    // DEBUG: dashed border layer for roundedContainer
    private var debugBorderLayer: CAShapeLayer?

    // Mask layer for content clipping (separate from debug border)
    private var contentMaskLayer: CAShapeLayer?


    // MARK: - Initialization

    init(viewModel: BrowserViewModel, historyVM: HistoryViewModel, bookmarkVM: BookmarkViewModel) {
        self.viewModel = viewModel
        self.historyVM = historyVM
        self.bookmarkVM = bookmarkVM

        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    // MARK: - Lifecycle

    override func loadView() {
        // Root view - background visible through 4px edge padding
        let rootView = NSView()
        rootView.wantsLayer = true
        rootView.layer?.backgroundColor = windowBackgroundColor.cgColor
        self.view = rootView
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        setupRoundedContainer()
        setupToolbar()
        setupSplitView()
        setupConstraints()
    }

    // MARK: - Setup

    private func setupRoundedContainer() {
        // Container that holds all content with rounded corners on all sides
        roundedContainer = NSView()
        roundedContainer.wantsLayer = true
        roundedContainer.layer?.backgroundColor = NSColor.clear.cgColor  // Transparent - content views provide their own background
        roundedContainer.translatesAutoresizingMaskIntoConstraints = false

        // Content mask for rounded corners (clip content but not debug border)
        let maskLayer = CAShapeLayer()
        roundedContainer.layer?.mask = maskLayer
        contentMaskLayer = maskLayer

        // DEBUG: Add dashed border using CAShapeLayer (added to parent so it's not clipped)
        let borderLayer = CAShapeLayer()
        borderLayer.strokeColor = NSColor.red.cgColor
        borderLayer.fillColor = nil
        borderLayer.lineWidth = 1
        borderLayer.lineDashPattern = [4, 4]  // 4px dash, 4px gap
        debugBorderLayer = borderLayer

        view.addSubview(roundedContainer)

        // Add debug border to parent view (will be positioned over roundedContainer)
        view.layer?.addSublayer(borderLayer)
    }

    private func setupToolbar() {
        // Create SwiftUI toolbar
        let toolbarView = SelfDrawnToolbarView(
            viewModel: viewModel,
            bookmarkVM: bookmarkVM
        )

        toolbarHostingView = NSHostingView(rootView: AnyView(toolbarView))
        toolbarHostingView.translatesAutoresizingMaskIntoConstraints = false
        toolbarHostingView.wantsLayer = true
        toolbarHostingView.layer?.borderColor = NSColor.systemGreen.cgColor  // DEBUG
        toolbarHostingView.layer?.borderWidth = 1  // DEBUG
        roundedContainer.addSubview(toolbarHostingView)
    }

    private func setupSplitView() {
        // Create split view controller
        splitViewController = MainSplitViewController(
            viewModel: viewModel,
            historyVM: historyVM,
            bookmarkVM: bookmarkVM,
            edgeInset: 0  // No additional inset needed, container handles it
        )

        addChild(splitViewController)

        let splitView = splitViewController.view
        splitView.translatesAutoresizingMaskIntoConstraints = false
        splitView.wantsLayer = true
        splitView.layer?.borderColor = NSColor.systemYellow.cgColor  // DEBUG
        splitView.layer?.borderWidth = 1  // DEBUG
        roundedContainer.addSubview(splitView)
    }

    private func setupConstraints() {
        NSLayoutConstraint.activate([
            // Rounded container: 4px inset on all sides
            roundedContainer.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: edgeInset),
            roundedContainer.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -edgeInset),
            roundedContainer.topAnchor.constraint(equalTo: view.topAnchor, constant: edgeInset),
            roundedContainer.bottomAnchor.constraint(equalTo: view.bottomAnchor, constant: -edgeInset),

            // Toolbar: top of container, full width, fixed height
            toolbarHostingView.leadingAnchor.constraint(equalTo: roundedContainer.leadingAnchor),
            toolbarHostingView.trailingAnchor.constraint(equalTo: roundedContainer.trailingAnchor),
            toolbarHostingView.topAnchor.constraint(equalTo: roundedContainer.topAnchor),
            toolbarHostingView.heightAnchor.constraint(equalToConstant: toolbarHeight),

            // Split view: below toolbar, fills remaining space
            splitViewController.view.leadingAnchor.constraint(equalTo: roundedContainer.leadingAnchor),
            splitViewController.view.trailingAnchor.constraint(equalTo: roundedContainer.trailingAnchor),
            splitViewController.view.topAnchor.constraint(equalTo: toolbarHostingView.bottomAnchor),
            splitViewController.view.bottomAnchor.constraint(equalTo: roundedContainer.bottomAnchor),
        ])
    }

    // MARK: - Layout Updates

    override func viewDidLayout() {
        super.viewDidLayout()

        // Update content mask to match roundedContainer bounds
        if let maskLayer = contentMaskLayer {
            let bounds = roundedContainer.bounds
            let path = CGPath(roundedRect: bounds, cornerWidth: cornerRadius, cornerHeight: cornerRadius, transform: nil)
            maskLayer.path = path
        }

        // DEBUG: Update dashed border path to match roundedContainer frame (in parent coordinates)
        if let borderLayer = debugBorderLayer {
            let frame = roundedContainer.frame
            let path = CGPath(roundedRect: frame, cornerWidth: cornerRadius, cornerHeight: cornerRadius, transform: nil)
            borderLayer.path = path
        }

        // Sync sidebar width for toolbar background alignment
        if let sidebarView = splitViewController?.splitViewItems.first?.viewController.view {
            let sidebarWidth = sidebarView.frame.width
            if sidebarWidth > 0 {
                // Notify toolbar of sidebar width change
                NotificationCenter.default.post(
                    name: .sidebarWidthChanged,
                    object: nil,
                    userInfo: ["width": sidebarWidth]
                )
            }
        }

        // Adjust traffic lights position
        adjustTrafficLights()
    }

    // MARK: - Traffic Lights

    private func adjustTrafficLights() {
        guard let window = view.window else { return }

        if window.styleMask.contains(.fullScreen) {
            // Fullscreen traffic lights are handled by MainWindowController.
            return
        }

        var xOffset: CGFloat = 0

        for buttonType in trafficLightButtonTypes {
            guard let button = window.standardWindowButton(buttonType) else { continue }

            if trafficLightOriginalSuperviews[buttonType] == nil, let original = button.superview {
                trafficLightOriginalSuperviews[buttonType] = original
            }

            let leadingConstant = trafficLightPosition.x + xOffset

            if button.superview != roundedContainer {
                button.removeFromSuperview()
                button.translatesAutoresizingMaskIntoConstraints = false
                roundedContainer.addSubview(button)

                let constraints = [
                    button.leadingAnchor.constraint(equalTo: roundedContainer.leadingAnchor, constant: leadingConstant),
                    button.topAnchor.constraint(equalTo: roundedContainer.topAnchor, constant: trafficLightTopInset)
                ]
                trafficLightConstraints[buttonType] = constraints
                NSLayoutConstraint.activate(constraints)
            } else if let constraints = trafficLightConstraints[buttonType], constraints.count == 2 {
                constraints[0].constant = leadingConstant
                constraints[1].constant = trafficLightTopInset
            }

            // Next button position: current position + button width + gap
            let buttonWidth = max(button.frame.width, button.fittingSize.width)
            xOffset += buttonWidth + trafficLightSpacing
        }
    }

    private func restoreTrafficLightsToSystemContainer(_ window: NSWindow) {
        for buttonType in trafficLightButtonTypes {
            guard let button = window.standardWindowButton(buttonType) else { continue }
            guard let originalSuperview = trafficLightOriginalSuperviews[buttonType] else { continue }

            if let constraints = trafficLightConstraints[buttonType] {
                NSLayoutConstraint.deactivate(constraints)
                trafficLightConstraints[buttonType] = nil
            }

            if button.superview != originalSuperview {
                button.removeFromSuperview()
                button.translatesAutoresizingMaskIntoConstraints = true
                originalSuperview.addSubview(button)
            }
        }
    }
}

// MARK: - Notification Names

extension Notification.Name {
    static let sidebarWidthChanged = Notification.Name("sidebarWidthChanged")
}

// MARK: - Self-Drawn Toolbar View

struct SelfDrawnToolbarView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var bookmarkVM: BookmarkViewModel

    @State private var sidebarWidth: CGFloat = 250
    @State private var isSidebarCollapsed: Bool = false
    @State private var addressText: String = ""
    @State private var isAddressFocused: Bool = false
    @State private var showingBookmarkPopover: Bool = false

    // Layout constants
    private let iconButtonSize: CGFloat = 30
    private let iconPointSize: CGFloat = 16

    var body: some View {
        // Toolbar content pinned to the top edge of the hosting container
        HStack(spacing: 0) {
            // Sidebar area (traffic lights + sidebar toggle + new tab)
            sidebarToolbarSection
                .frame(width: isSidebarCollapsed ? 78 : sidebarWidth)

            // Content area (navigation + address bar + actions)
            contentToolbarSection
        }
        .padding(.top, 8)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(alignment: .topLeading) {
            toolbarBackground
        }
        .overlay {
            // DEBUG: border for SwiftUI toolbar content bounds
            Rectangle()
                .stroke(Color.blue.opacity(0.8), lineWidth: 1)
        }
        .ignoresSafeArea(.container, edges: .top)
        .onReceive(NotificationCenter.default.publisher(for: .sidebarWidthChanged)) { notification in
            if let width = notification.userInfo?["width"] as? CGFloat {
                withAnimation(.easeInOut(duration: 0.15)) {
                    sidebarWidth = width
                }
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: .toggleSidebarRequested)) { _ in
            withAnimation(.easeInOut(duration: 0.25)) {
                isSidebarCollapsed.toggle()
            }
        }
    }

    // MARK: - Sidebar Section

    private var sidebarToolbarSection: some View {
        HStack(spacing: 8) {
            // Space for traffic lights (~70px from left edge)
            Spacer()
                .frame(width: 70)

            // New tab button
            Button(action: { _ = viewModel.addNewTab() }) {
                Image(systemName: "plus")
                    .font(.system(size: iconPointSize, weight: .medium))
                    .frame(width: iconButtonSize, height: iconButtonSize)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)

            // Overview toggle button
            Button(action: { toggleOverview() }) {
                Image(systemName: "square.on.square")
                    .font(.system(size: iconPointSize, weight: .medium))
                    .frame(width: iconButtonSize, height: iconButtonSize)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)

            Spacer()
        }
    }

    // MARK: - Content Section

    private var contentToolbarSection: some View {
        HStack(spacing: 12) {
            // Sidebar toggle
            Button(action: { toggleSidebar() }) {
                Image(systemName: "sidebar.left")
                    .font(.system(size: iconPointSize, weight: .medium))
                    .frame(width: iconButtonSize, height: iconButtonSize)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)

            // Navigation buttons
            HStack(spacing: 4) {
                Button(action: { viewModel.goBack() }) {
                    Image(systemName: "chevron.left")
                        .font(.system(size: 14, weight: .semibold))
                        .frame(width: iconButtonSize, height: iconButtonSize)
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .disabled(!viewModel.canGoBack)

                Button(action: { viewModel.goForward() }) {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 14, weight: .semibold))
                        .frame(width: iconButtonSize, height: iconButtonSize)
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .disabled(!viewModel.canGoForward)
            }

            // Address bar
            addressBar
                .frame(maxWidth: 400)

            Spacer()

            // Right side actions
            rightActions
        }
        .padding(.horizontal, 12)
    }

    // MARK: - Address Bar

    private var addressBar: some View {
        HStack(spacing: 8) {
            // Page icon
            Image(systemName: "line.3.horizontal")
                .font(.system(size: 12))
                .foregroundStyle(.tertiary)

            // Address text
            Text(currentDisplayURL)
                .font(.system(size: 13))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)

            Spacer()
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .background(Color(white: 0.15))
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    private var currentDisplayURL: String {
        if let tab = viewModel.currentTab {
            if tab.systemTabType != nil {
                return "about:blank"
            }
            return tab.url?.host ?? tab.url?.absoluteString ?? "about:blank"
        }
        return "about:blank"
    }

    // MARK: - Right Actions

    private var rightActions: some View {
        HStack(spacing: 8) {
            // Bookmark button
            Button(action: { toggleBookmark() }) {
                Image(systemName: isBookmarked ? "star.fill" : "star")
                    .font(.system(size: 14, weight: .medium))
                    .frame(width: iconButtonSize, height: iconButtonSize)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            .opacity(isBookmarked ? 1 : 0.5)

            // Refresh button
            Button(action: { viewModel.reloadCurrentTab() }) {
                Image(systemName: "arrow.clockwise")
                    .font(.system(size: 14, weight: .medium))
                    .frame(width: iconButtonSize, height: iconButtonSize)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)

            // More button
            Button(action: {}) {
                Image(systemName: "ellipsis")
                    .font(.system(size: 14, weight: .medium))
                    .frame(width: iconButtonSize, height: iconButtonSize)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
        }
    }

    // MARK: - Background

    private var toolbarBackground: some View {
        // Content background (dark with top-left rounded corner only)
        UnevenRoundedRectangle(
            topLeadingRadius: 12,
            bottomLeadingRadius: 0,
            bottomTrailingRadius: 0,
            topTrailingRadius: 0  // roundedContainer mask handles right corner
        )
        .fill(Color(white: 0.10))
        .padding(.leading, isSidebarCollapsed ? 0 : sidebarWidth)
    }

    // MARK: - Actions

    private var isBookmarked: Bool {
        guard let url = viewModel.currentTab?.url else { return false }
        return bookmarkVM.bookmarks.contains(where: { $0.appURL == url })
    }

    private func toggleSidebar() {
        NotificationCenter.default.post(name: .toggleSidebarRequested, object: nil)
    }

    private func toggleOverview() {
        NotificationCenter.default.post(name: .toggleFocusOverviewRequested, object: nil)
    }

    private func toggleBookmark() {
        guard let tab = viewModel.currentTab else { return }

        if isBookmarked {
            if let url = tab.url,
               let bookmark = bookmarkVM.bookmarks.first(where: { $0.appURL == url }) {
                bookmarkVM.removeBookmark(id: bookmark.id)
            }
        } else {
            bookmarkVM.addBookmark(from: tab)
        }
    }
}
