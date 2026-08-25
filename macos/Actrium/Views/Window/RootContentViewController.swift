import AppKit
import ActroCore
import SwiftUI
import Combine

// MARK: - Root View (layout-cycle traffic light correction)

/// Custom root view that repositions traffic lights in its `layout()` override.
///
/// AppKit's layout cycle is top-down: the window's theme-frame lays out
/// the NSTitlebarContainerView (which positions the standard window buttons
/// at their default location) **before** laying out this content view.
/// By calling `adjustTrafficLights()` in our `layout()`, we always run
/// after the system and override its positioning — within the same frame,
/// no deferred dispatch, no visual flash.
///
/// `viewDidMoveToWindow()` installs a KVO observation on `window.title`.
/// Setting `window.title` is the trigger that causes the system to
/// re-layout the titlebar; the KVO fires synchronously in the setter,
/// marks this view for layout, and the layout system does the rest.
private class MainWindowRootView: NSView {
    weak var controller: MainWindowViewController?
    private var titleObservation: NSKeyValueObservation?
    private let chromeMaterialView = NSVisualEffectView()
    private let chromeTintView = WindowChromeTintView()

    func applyWindowChromeStyle(_ style: WindowChromeStyle) {
        wantsLayer = true
        layer?.backgroundColor = NSColor.clear.cgColor

        if chromeMaterialView.superview == nil {
            chromeMaterialView.translatesAutoresizingMaskIntoConstraints = false
            chromeMaterialView.blendingMode = .behindWindow
            chromeMaterialView.state = .active
            addSubview(chromeMaterialView)

            chromeTintView.translatesAutoresizingMaskIntoConstraints = false
            addSubview(chromeTintView)

            NSLayoutConstraint.activate([
                chromeMaterialView.leadingAnchor.constraint(equalTo: leadingAnchor),
                chromeMaterialView.trailingAnchor.constraint(equalTo: trailingAnchor),
                chromeMaterialView.topAnchor.constraint(equalTo: topAnchor),
                chromeMaterialView.bottomAnchor.constraint(equalTo: bottomAnchor),
                chromeTintView.leadingAnchor.constraint(equalTo: leadingAnchor),
                chromeTintView.trailingAnchor.constraint(equalTo: trailingAnchor),
                chromeTintView.topAnchor.constraint(equalTo: topAnchor),
                chromeTintView.bottomAnchor.constraint(equalTo: bottomAnchor),
            ])
        }
        chromeMaterialView.material = style.material
        chromeTintView.tintColor = style.tintColor
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if let window {
            titleObservation = window.observe(\.title, options: []) { [weak self] _, _ in
                self?.needsLayout = true
            }
        } else {
            titleObservation = nil
        }
    }

    override func layout() {
        super.layout()
        controller?.handleRootViewLayout()
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        controller?.applyAppearanceColors()
    }
}

/// A dedicated tint layer above the system blur. Applying tint directly to an
/// NSVisualEffectView's backing layer puts it behind AppKit's material and can
/// make a successful preference change appear to do nothing.
private final class WindowChromeTintView: NSView {
    var tintColor: NSColor = .clear {
        didSet { needsDisplay = true }
    }

    override var isOpaque: Bool { false }
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
    override var mouseDownCanMoveWindow: Bool { false }

    override func draw(_ dirtyRect: NSRect) {
        tintColor.setFill()
        dirtyRect.fill()
    }
}

/// One geometry contract shared by the SwiftUI header and the AppKit drag
/// mask. Keeping these values together prevents the draggable area from
/// drifting into the Tabs/Tabs Group container as the layout evolves.
private enum CompactToolbarLayout {
    /// Head and tail surfaces own the large outer corners directly. Keeping
    /// that rule on each logical surface avoids coupling corner treatment to
    /// the ScrollView's clipping viewport.
    static let windowCornerRadius: CGFloat = 12
    static let toolbarHeight: CGFloat = 45
    static let tabStripHeight: CGFloat = 36
    static let edgeTabCornerRadius: CGFloat = 8
    static let tabCornerRadius: CGFloat = 6
    static let horizontalPadding: CGFloat = 8
    static let itemSpacing: CGFloat = 8
    static let buttonSize: CGFloat = 30
    static let compactTrafficLightReserve: CGFloat = 78
    static let fullScreenLeadingReserve: CGFloat = 4
    static let inactiveTabMinWidth: CGFloat = 88
    static let inactiveTabCompressedMinWidth: CGFloat = 68
    static let inactiveTabMaxWidth: CGFloat = 240
    static let activeTabMinWidth: CGFloat = 300
    static let activeTabCompressedMinWidth: CGFloat = 220
    static let activeTabMaxWidth: CGFloat = 520
    static let activeTabPreferredWidth: CGFloat = 420

    static func leadingReserve(isFullScreen: Bool) -> CGFloat {
        isFullScreen ? fullScreenLeadingReserve : compactTrafficLightReserve
    }

    static func maximumTabStripWidth(in toolbarWidth: CGFloat, isFullScreen: Bool) -> CGFloat {
        max(
            0,
            toolbarWidth
                - horizontalPadding * 2
                - leadingReserve(isFullScreen: isFullScreen)
                - itemSpacing * 3
                - buttonSize * 2
        )
    }

    static func tabStripFrame(
        in bounds: NSRect,
        isFullScreen: Bool,
        contentWidth: CGFloat
    ) -> NSRect {
        let minX = tabStripMinX(in: bounds, isFullScreen: isFullScreen)
        let width = min(
            max(0, contentWidth),
            maximumTabStripWidth(in: bounds.width, isFullScreen: isFullScreen)
        )
        return centeredRect(
            minX: minX,
            maxX: minX + width,
            height: tabStripHeight,
            in: bounds
        )
    }

    static func newTabButtonFrame(in bounds: NSRect) -> NSRect {
        let maxX = bounds.maxX
            - horizontalPadding
            - buttonSize
            - itemSpacing
        return centeredRect(
            minX: maxX - buttonSize,
            maxX: maxX,
            height: buttonSize,
            in: bounds
        )
    }

    static func overviewButtonFrame(in bounds: NSRect) -> NSRect {
        let maxX = bounds.maxX - horizontalPadding
        return centeredRect(
            minX: maxX - buttonSize,
            maxX: maxX,
            height: buttonSize,
            in: bounds
        )
    }

    private static func tabStripMinX(in bounds: NSRect, isFullScreen: Bool) -> CGFloat {
        bounds.minX
            + horizontalPadding
            + leadingReserve(isFullScreen: isFullScreen)
            + itemSpacing
    }

    private static func centeredRect(
        minX: CGFloat,
        maxX: CGFloat,
        height: CGFloat,
        in bounds: NSRect
    ) -> NSRect {
        NSRect(
            x: minX,
            y: bounds.midY - height / 2,
            width: max(0, maxX - minX),
            height: height
        )
    }
}

/// Root content view controller - manages 4px edge inset and contains toolbar + split view
/// macOS renderer for the canonical `MainWindow → Tabs + Workspace` tree.
/// Windowed and full-screen presentations retain the same two child entities.
final class MainWindowViewController: NSViewController {

    // MARK: - Properties

    private let viewModel: BrowserViewModel
    private let historyVM: HistoryViewModel
    private let bookmarkVM: BookmarkViewModel
    private let toolbarInteraction: ToolbarTabInteractionCoordinator

    /// Edge inset (transparent border around content)
    private let edgeInset: CGFloat = 4

    /// Chrome-colored breathing room around the independently rounded Body.
    private let bodyInset: CGFloat = 4

    /// Corner radius for content area
    private let cornerRadius = CompactToolbarLayout.windowCornerRadius

    /// Toolbar height (further reduced by 1pt for optical centering)
    private let toolbarHeight: CGFloat = 45

    /// Window background color (TODO: make configurable via settings)
    private let windowBackgroundColor = NSColor.white

    /// Traffic lights position (absolute, in our view's coordinates)
    private let trafficLightPosition = NSPoint(x: 15, y: 0)
    private let trafficLightSpacing: CGFloat = 9  // Gap between buttons (nudged by +1px)
    private let trafficLightTopInset: CGFloat = 15  // Distance from top (nudged up by 2pt)
    private let trafficLightButtonTypes: [NSWindow.ButtonType] = [.closeButton, .miniaturizeButton, .zoomButton]

    /// Keep original system superviews so fullscreen can restore native behavior.
    private var trafficLightOriginalSuperviews: [NSWindow.ButtonType: NSView] = [:]
    private var trafficLightConstraints: [NSWindow.ButtonType: [NSLayoutConstraint]] = [:]

    // Child views
    private var roundedContainer: NSView!
    private var toolbarHostingView: NSHostingView<AnyView>!
    private var windowDragRegion: CompactToolbarWindowDragRegion!
    private var contentViewController: ContentViewController!
    private let sidebarLayoutState = SidebarLayoutState()
    private var layoutCancellables = Set<AnyCancellable>()

    // Mask layer for content clipping
    private var contentMaskLayer: CAShapeLayer?


    // MARK: - Initialization

    init(
        viewModel: BrowserViewModel,
        historyVM: HistoryViewModel,
        bookmarkVM: BookmarkViewModel,
        toolbarInteraction: ToolbarTabInteractionCoordinator
    ) {
        self.viewModel = viewModel
        self.historyVM = historyVM
        self.bookmarkVM = bookmarkVM
        self.toolbarInteraction = toolbarInteraction

        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    // MARK: - Lifecycle

    override func loadView() {
        let rootView = MainWindowRootView()
        rootView.controller = self
        rootView.applyWindowChromeStyle(WindowChromeManager.shared.style)
        self.view = rootView
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        setupRoundedContainer()
        setupToolbar()
        setupWindowDragRegion()
        setupContent()
        setupConstraints()
        observeWindowChromeStyle()
        applyAppearanceColors()
    }

    fileprivate func applyAppearanceColors() {
        (view as? MainWindowRootView)?.applyWindowChromeStyle(WindowChromeManager.shared.style)
        toolbarHostingView?.layer?.backgroundColor = NSColor.clear.cgColor
        contentViewController?.applyAppearanceColors()
        view.needsDisplay = true
    }

    private func observeWindowChromeStyle() {
        WindowChromeManager.shared.$style
            .removeDuplicates()
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                self?.applyAppearanceColors()
            }
            .store(in: &layoutCancellables)
    }

    // MARK: - Setup

    private func setupRoundedContainer() {
        // Container that holds all content with rounded corners on all sides
        roundedContainer = NSView()
        roundedContainer.setAccessibilityIdentifier(
            MainWindowEntityName.mainWindow.accessibilityIdentifier
        )
        roundedContainer.wantsLayer = true
        roundedContainer.layer?.backgroundColor = NSColor.clear.cgColor  // Transparent - content views provide their own background
        roundedContainer.translatesAutoresizingMaskIntoConstraints = false

        // Content mask for rounded corners (clip content but not debug border)
        let maskLayer = CAShapeLayer()
        roundedContainer.layer?.mask = maskLayer
        contentMaskLayer = maskLayer

        view.addSubview(roundedContainer)
    }

    private func setupToolbar() {
        // Create SwiftUI toolbar
        let toolbarView = MainWindowTabsView(
            viewModel: viewModel,
            historyVM: historyVM,
            bookmarkVM: bookmarkVM,
            layoutState: sidebarLayoutState,
            interaction: toolbarInteraction
        )

        toolbarHostingView = InteractiveToolbarHostingView(rootView: AnyView(toolbarView))
        toolbarHostingView.setAccessibilityIdentifier(
            MainWindowEntityName.tabs.accessibilityIdentifier
        )
        toolbarHostingView.translatesAutoresizingMaskIntoConstraints = false
        toolbarHostingView.wantsLayer = true
        toolbarHostingView.layer?.backgroundColor = NSColor.clear.cgColor
        // Disable titlebar safe area — we manage our own toolbar layout;
        // the system titlebar is only meaningful in fullscreen.
        toolbarHostingView.safeAreaRegions = []
        roundedContainer.addSubview(toolbarHostingView)
    }

    private func setupWindowDragRegion() {
        windowDragRegion = CompactToolbarWindowDragRegion()
        windowDragRegion.translatesAutoresizingMaskIntoConstraints = false
        roundedContainer.addSubview(windowDragRegion)

        sidebarLayoutState.$tabStripWidth
            .removeDuplicates()
            .sink { [weak windowDragRegion] width in
                windowDragRegion?.tabStripWidth = width
            }
            .store(in: &layoutCancellables)
    }

    private func setupContent() {
        contentViewController = ContentViewController(
            viewModel: viewModel,
            historyVM: historyVM,
            bookmarkVM: bookmarkVM
        )

        addChild(contentViewController)

        let contentView = contentViewController.view
        contentView.setAccessibilityIdentifier(
            MainWindowEntityName.workspace.accessibilityIdentifier
        )
        contentView.translatesAutoresizingMaskIntoConstraints = false
        roundedContainer.addSubview(contentView)
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

            // One full-header drag mask. Its hit test excludes the complete
            // Tabs/Tabs Group container and the two toolbar buttons.
            windowDragRegion.leadingAnchor.constraint(equalTo: roundedContainer.leadingAnchor),
            windowDragRegion.trailingAnchor.constraint(equalTo: roundedContainer.trailingAnchor),
            windowDragRegion.topAnchor.constraint(equalTo: roundedContainer.topAnchor),
            windowDragRegion.heightAnchor.constraint(equalToConstant: toolbarHeight),

            // Body: an independent rounded frame nested inside the window
            // chrome. The surrounding gutter reveals the same material as the
            // Header and replaces the former full-width separator line.
            contentViewController.view.leadingAnchor.constraint(
                equalTo: roundedContainer.leadingAnchor,
                constant: bodyInset
            ),
            contentViewController.view.trailingAnchor.constraint(
                equalTo: roundedContainer.trailingAnchor,
                constant: -bodyInset
            ),
            contentViewController.view.topAnchor.constraint(
                equalTo: toolbarHostingView.bottomAnchor,
                constant: bodyInset
            ),
            contentViewController.view.bottomAnchor.constraint(
                equalTo: roundedContainer.bottomAnchor,
                constant: -bodyInset
            ),
        ])
    }

    // MARK: - Layout Updates

    /// Called from `MainWindowRootView.layout()` — guaranteed to run
    /// after the system's titlebar layout in the same layout pass.
    fileprivate func handleRootViewLayout() {
        // Update content mask to match roundedContainer bounds
        if let maskLayer = contentMaskLayer {
            let bounds = roundedContainer.bounds
            let path = CGPath(roundedRect: bounds, cornerWidth: cornerRadius, cornerHeight: cornerRadius, transform: nil)
            maskLayer.path = path
        }

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

    // MARK: - External Layout State

    func updateWindowFullScreenState(_ isFullScreen: Bool) {
        sidebarLayoutState.updateWindowFullScreen(isFullScreen)
        windowDragRegion?.isFullScreen = isFullScreen
        windowDragRegion?.isHidden = isFullScreen
    }
}

// MARK: - Interactive Toolbar Hosting View

/// SwiftUI content never falls through to implicit titlebar dragging. The
/// explicit drag mask above it decides which non-Tab header points move the
/// window.
private final class InteractiveToolbarHostingView: NSHostingView<AnyView> {
    override var mouseDownCanMoveWindow: Bool { false }
}

/// Covers the full compact header but only participates in hit testing outside
/// the Tabs/Tabs Group container and the actual new/overview button bounds.
/// Everything else—leading chrome, header padding, vertical bands and gaps
/// around controls—is a deliberate window-drag region.
private final class CompactToolbarWindowDragRegion: NSView {
    var isFullScreen = false
    var tabStripWidth: CGFloat = 300

    override var mouseDownCanMoveWindow: Bool { false }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? {
        // AppKit supplies `point` in this view's superview coordinate space.
        // The exclusion geometry is expressed in local bounds, so comparing
        // the raw point against `bounds` makes every top-aligned Header point
        // miss whenever this view's frame origin is non-zero.
        guard !isHidden, frame.contains(point) else { return nil }
        let localPoint = convert(point, from: superview)
        let excludedFrames = [
            CompactToolbarLayout.tabStripFrame(
                in: bounds,
                isFullScreen: isFullScreen,
                contentWidth: tabStripWidth
            ),
            CompactToolbarLayout.newTabButtonFrame(
                in: bounds
            ),
            CompactToolbarLayout.overviewButtonFrame(
                in: bounds
            ),
        ]
        return excludedFrames.contains(where: { $0.contains(localPoint) }) ? nil : self
    }

    override func mouseDown(with event: NSEvent) {
        guard let window, !window.styleMask.contains(.fullScreen) else { return }

        if event.clickCount == 2 {
            performConfiguredDoubleClickAction(on: window)
            return
        }

        (window as? ActroWindow)?.performExplicitWindowDrag(with: event)
    }

    private func performConfiguredDoubleClickAction(on window: NSWindow) {
        let action = UserDefaults.standard.string(forKey: "AppleActionOnDoubleClick") ?? "Maximize"
        switch action {
        case "Minimize":
            window.miniaturize(nil)
        case "None":
            break
        default:
            window.zoom(nil)
        }
    }
}

// MARK: - Self-Drawn Toolbar View

private enum ToolbarTabSectionID: Hashable {
    case group(UUID)
    case ungrouped(afterGroupID: UUID?)
}

private struct ToolbarTabSection: Identifiable {
    let id: ToolbarTabSectionID
    let group: TabGroup?
    let tabs: [Tab]

    init(group: TabGroup?, tabs: [Tab], precedingGroupID: UUID?) {
        self.group = group
        self.tabs = tabs
        self.id = group.map { .group($0.id) }
            ?? .ungrouped(afterGroupID: precedingGroupID)
    }
}

private enum ToolbarTabStripCoordinateSpace {
    static let name = "actro.main-window.tabs-strip"
}

private struct ToolbarTabFramePreferenceKey: PreferenceKey {
    static var defaultValue: [UUID: CGRect] = [:]

    static func reduce(value: inout [UUID: CGRect], nextValue: () -> [UUID: CGRect]) {
        value.merge(nextValue(), uniquingKeysWith: { _, latest in latest })
    }
}

private struct ToolbarTabGroupFramePreferenceKey: PreferenceKey {
    static var defaultValue: [UUID: CGRect] = [:]

    static func reduce(value: inout [UUID: CGRect], nextValue: () -> [UUID: CGRect]) {
        value.merge(nextValue(), uniquingKeysWith: { _, latest in latest })
    }
}

private struct ToolbarTabGroupHeaderFramePreferenceKey: PreferenceKey {
    static var defaultValue: [UUID: CGRect] = [:]

    static func reduce(value: inout [UUID: CGRect], nextValue: () -> [UUID: CGRect]) {
        value.merge(nextValue(), uniquingKeysWith: { _, latest in latest })
    }
}

/// Owns address-bar panels whose arrows are anchored to hover-only controls.
/// The anchor controls must remain visible until AppKit confirms dismissal;
/// mouse hover alone is not a sufficient presentation state.
@MainActor
private final class AddressBarPanelPresentationCoordinator: NSObject, ObservableObject,
    NSSharingServicePickerDelegate
{
    @Published private(set) var presentedTabID: UUID?

    private var sharingServicePicker: NSSharingServicePicker?

    func isPresentingPanel(anchoredTo tabID: UUID) -> Bool {
        presentedTabID == tabID
    }

    func showSharePicker(for url: URL, anchoredTo tabID: UUID) {
        guard let window = NSApp.keyWindow,
              let contentView = window.contentView else { return }

        let windowPoint = window.convertPoint(fromScreen: NSEvent.mouseLocation)
        let contentPoint = contentView.convert(windowPoint, from: nil)
        let anchor = NSRect(origin: contentPoint, size: NSSize(width: 1, height: 1))
        let picker = NSSharingServicePicker(items: [url])

        picker.delegate = self
        sharingServicePicker = picker
        presentedTabID = tabID
        picker.show(relativeTo: anchor, of: contentView, preferredEdge: .minY)
    }

    func sharingServicePicker(
        _ sharingServicePicker: NSSharingServicePicker,
        didChoose service: NSSharingService?
    ) {
        guard self.sharingServicePicker === sharingServicePicker else { return }
        self.sharingServicePicker = nil
        presentedTabID = nil
    }
}

struct MainWindowTabsView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel
    @ObservedObject var bookmarkVM: BookmarkViewModel
    @ObservedObject var layoutState: SidebarLayoutState
    @ObservedObject var interaction: ToolbarTabInteractionCoordinator

    @State private var addressText: String = ""
    @State private var hasInputActivity: Bool = false
    @State private var editingGroupID: UUID?
    @State private var groupNameDraft: String = ""
    @State private var hoveredTabID: UUID?
    @State private var isAddressFieldHovered = false
    @State private var activeTabRevealTask: Task<Void, Never>?
    @StateObject private var addressPanelPresentation = AddressBarPanelPresentationCoordinator()

    private let itemSpacing = CompactToolbarLayout.itemSpacing

    var body: some View {
        GeometryReader { geo in
            let resolvedTabStripWidth = tabStripWidth(in: geo.size.width)

            HStack(spacing: 0) {
                Color.clear
                    .frame(
                        width: CompactToolbarLayout.leadingReserve(
                            isFullScreen: layoutState.isWindowFullScreen
                        ),
                        height: 1
                    )

                sidebarToolbarSection(metrics: metrics)
                    .environment(\.colorScheme, .light)
                    .frame(width: sidebarWidth, alignment: .leading)

                contentToolbarSection
                    .environment(\.colorScheme, .light)
                    .frame(width: contentWidth, alignment: .leading)
                    .offset(x: contentLeading)
            }
        }
        .onAppear { updateDisplayText() }
        .onChange(of: viewModel.currentTab?.url) { _, _ in updateDisplayText() }
        .onChange(of: viewModel.currentTab?.title) { _, _ in updateDisplayText() }
        .onChange(of: viewModel.currentTabId) { _, _ in
            interaction.activeTabChanged(to: viewModel.currentTabId)
            updateDisplayText()
        }
        .onChange(of: viewModel.shouldAutoFocusAddressBar) { _, shouldFocus in
            if shouldFocus {
                enterEditingMode()
                viewModel.shouldAutoFocusAddressBar = false
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: NSNotification.Name("com.actro.dismissAddressBarFocus"))) { _ in
            exitEditingMode()
        }
    }

    // MARK: - Tabs

    private var tabStrip: some View {
        GeometryReader { geo in
            let sections = tabSections
            let tabWidths = adaptiveTabWidths(
                for: sections,
                viewportWidth: geo.size.width
            )

            ZStack(alignment: .topLeading) {
                ScrollViewReader { proxy in
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: 2) {
                            ForEach(sections) { section in
                                if let group = section.group {
                                    groupedTabs(
                                        group,
                                        tabs: section.tabs,
                                        tabWidths: tabWidths,
                                        viewportWidth: geo.size.width
                                    )
                                } else {
                                    HStack(spacing: 2) {
                                        ForEach(section.tabs) { tab in
                                            renderedTab(
                                                tab,
                                                width: tabWidths[tab.id]
                                                    ?? preferredTabWidth(for: tab),
                                                viewportWidth: geo.size.width
                                            )
                                        }
                                    }
                                }
                            }
                        }
                        .padding(2)
                        .animation(ToolbarTabMotion.slide, value: viewModel.currentTabId)
                    }
                    .onChange(of: viewModel.currentTabId) { _, tabID in
                        guard let tabID else { return }
                        withAnimation(ToolbarTabMotion.slide) {
                            proxy.scrollTo(tabID, anchor: .center)
                        }
                    }
                    .onReceive(NotificationCenter.default.publisher(for: .ensureActiveTabVisibleRequested)) { notification in
                        guard let sourceViewModel = notification.object as? BrowserViewModel,
                              sourceViewModel === viewModel else { return }
                        scheduleActiveTabReveal(in: proxy)
                    }
                }

                movingTabForeground
                movingGroupForeground(tabWidths: tabWidths)
            }
            .onAppear { interaction.updateTabStripWidth(geo.size.width) }
            .onChange(of: geo.size.width) { _, width in
                interaction.updateTabStripWidth(width)
            }
        }
        .coordinateSpace(name: ToolbarTabStripCoordinateSpace.name)
        .onPreferenceChange(ToolbarTabFramePreferenceKey.self) {
            interaction.updateTabFrames($0)
        }
        .onPreferenceChange(ToolbarTabGroupFramePreferenceKey.self) {
            interaction.updateGroupFrames($0)
        }
        .onPreferenceChange(ToolbarTabGroupHeaderFramePreferenceKey.self) {
            interaction.updateGroupHeaderFrames($0)
        }
        .simultaneousGesture(tabReorderGesture)
        .frame(height: CompactToolbarLayout.tabStripHeight)
    }

    private func revealActiveTabIfNeeded(in proxy: ScrollViewProxy) {
        guard let tabID = viewModel.currentTabId else { return }

        withAnimation(ToolbarTabMotion.slide) {
            // An anchorless scroll is deliberately idempotent: SwiftUI moves
            // the smallest distance needed to expose a clipped item and does
            // nothing when it is already fully visible. Do not depend on the
            // preference geometry being refreshed by the preceding drag;
            // stale geometry was the reason body focus could miss the reveal.
            // A Tab wider than the viewport is the only case that needs an
            // explicit anchor because it can never be fully visible.
            if interaction.revealAlignment(for: tabID) == .center {
                proxy.scrollTo(tabID, anchor: .center)
            } else {
                proxy.scrollTo(tabID)
            }
        }
    }

    /// Scroll and drag animations report their final preference geometry on a
    /// later render pass. Check once on the next main-actor turn, then verify
    /// again after the toolbar's own slide animation has settled. A fully
    /// visible Tab remains a no-op on both passes.
    private func scheduleActiveTabReveal(in proxy: ScrollViewProxy) {
        activeTabRevealTask?.cancel()
        activeTabRevealTask = Task { @MainActor in
            await Task.yield()
            guard !Task.isCancelled else { return }
            revealActiveTabIfNeeded(in: proxy)

            try? await Task.sleep(nanoseconds: 210_000_000)
            guard !Task.isCancelled else { return }
            revealActiveTabIfNeeded(in: proxy)
        }
    }

    private var tabSections: [ToolbarTabSection] {
        var precedingGroupID: UUID?
        return viewModel.tabsOrganizedByGroups().map { section in
            let toolbarSection = ToolbarTabSection(
                group: section.group,
                tabs: section.tabs,
                precedingGroupID: precedingGroupID
            )
            if let groupID = section.group?.id {
                precedingGroupID = groupID
            }
            return toolbarSection
        }
    }

    /// Corners follow the logical order, rather than whatever happens to be
    /// touching the ScrollView viewport. A surface can own either outer edge,
    /// both outer edges, or neither.
    private func toolbarSurfaceShape(
        isLeadingEdge: Bool,
        isTrailingEdge: Bool
    ) -> UnevenRoundedRectangle {
        let leadingRadius = isLeadingEdge
            ? CompactToolbarLayout.edgeTabCornerRadius
            : CompactToolbarLayout.tabCornerRadius
        let trailingRadius = isTrailingEdge
            ? CompactToolbarLayout.edgeTabCornerRadius
            : CompactToolbarLayout.tabCornerRadius

        return UnevenRoundedRectangle(
            cornerRadii: .init(
                topLeading: leadingRadius,
                bottomLeading: leadingRadius,
                bottomTrailing: trailingRadius,
                topTrailing: trailingRadius
            ),
            style: .continuous
        )
    }

    private func tabShape(for tab: Tab) -> UnevenRoundedRectangle {
        let sections = tabSections
        let firstSection = sections.first
        let lastSection = sections.last

        // A leading group header precedes its member Tabs. An expanded last
        // group delegates the trailing outer corners to its final member Tab.
        let isLeadingEdge = firstSection?.group == nil
            && firstSection?.tabs.first?.id == tab.id
        let isTrailingEdge = lastSection?.group?.isCollapsed != true
            && lastSection?.tabs.last?.id == tab.id

        return toolbarSurfaceShape(
            isLeadingEdge: isLeadingEdge,
            isTrailingEdge: isTrailingEdge
        )
    }

    private func groupShape(for group: TabGroup) -> UnevenRoundedRectangle {
        let sections = tabSections
        return toolbarSurfaceShape(
            isLeadingEdge: sections.first?.group?.id == group.id,
            isTrailingEdge: sections.last?.group?.id == group.id
        )
    }

    private func groupHeaderShape(for group: TabGroup) -> UnevenRoundedRectangle {
        let sections = tabSections
        return toolbarSurfaceShape(
            isLeadingEdge: sections.first?.group?.id == group.id,
            isTrailingEdge: group.isCollapsed && sections.last?.group?.id == group.id
        )
    }

    private func tabStripGapWidth(for sections: [ToolbarTabSection]) -> CGFloat {
        let outerGaps = CGFloat(max(0, sections.count - 1)) * 2
        let innerGaps = sections.reduce(CGFloat.zero) { result, section in
            if let group = section.group {
                return result + (group.isCollapsed ? 0 : CGFloat(section.tabs.count) * 2)
            }
            return result + CGFloat(max(0, section.tabs.count - 1)) * 2
        }
        return outerGaps + innerGaps
    }

    private func groupLabelWidth(_ group: TabGroup) -> CGFloat {
        let title = group.name.isEmpty ? "Tab Group" : group.name
        return min(158, max(82, CGFloat(title.count) * 7 + 48))
    }

    private func tabStripWidth(in toolbarWidth: CGFloat) -> CGFloat {
        let sections = tabSections
        let visibleTabs = sections.flatMap { section in
            section.group?.isCollapsed == true ? [] : section.tabs
        }
        let tabsWidth = visibleTabs.reduce(CGFloat.zero) { result, tab in
            result + preferredTabWidth(for: tab)
        }
        let groupHeaderWidth = sections.reduce(CGFloat.zero) { result, section in
            result + (section.group.map(groupLabelWidth) ?? 0)
        }
        let contentWidth = tabsWidth
            + groupHeaderWidth
            + tabStripGapWidth(for: sections)
            + 4
        return min(
            contentWidth,
            CompactToolbarLayout.maximumTabStripWidth(
                in: toolbarWidth,
                isFullScreen: layoutState.isWindowFullScreen
            )
        )
    }

    private func preferredTabWidth(for tab: Tab) -> CGFloat {
        if viewModel.currentTabId == tab.id {
            return min(
                CompactToolbarLayout.activeTabMaxWidth,
                max(CompactToolbarLayout.activeTabMinWidth, CompactToolbarLayout.activeTabPreferredWidth)
            )
        }

        let title = tab.title.isEmpty ? "New Tab" : tab.title
        let titleWidth = ceil(
            (title as NSString).size(
                withAttributes: [.font: NSFont.systemFont(ofSize: 12.5)]
            ).width
        )
        return min(
            CompactToolbarLayout.inactiveTabMaxWidth,
            max(CompactToolbarLayout.inactiveTabMinWidth, titleWidth + 46)
        )
    }

    private func adaptiveTabWidths(
        for sections: [ToolbarTabSection],
        viewportWidth: CGFloat
    ) -> [UUID: CGFloat] {
        let visibleTabs = sections.flatMap { section in
            section.group?.isCollapsed == true ? [] : section.tabs
        }
        let groupHeaderWidth = sections.reduce(CGFloat.zero) { result, section in
            result + (section.group.map(groupLabelWidth) ?? 0)
        }
        let availableTabWidth = max(
            0,
            viewportWidth
                - groupHeaderWidth
                - tabStripGapWidth(for: sections)
                - 4
        )
        let inputs = visibleTabs.map { tab in
            ToolbarAdaptiveTabWidthInput(
                id: tab.id,
                preferredWidth: preferredTabWidth(for: tab),
                minimumWidth: viewModel.currentTabId == tab.id
                    ? CompactToolbarLayout.activeTabCompressedMinWidth
                    : CompactToolbarLayout.inactiveTabCompressedMinWidth
            )
        }
        return toolbarAdaptiveTabWidths(
            inputs: inputs,
            availableWidth: availableTabWidth
        )
    }

    @ViewBuilder
    private func renderedTab(
        _ tab: Tab,
        width: CGFloat,
        viewportWidth: CGFloat
    ) -> some View {
        let isMoving = interaction.isMovingTab(tab.id)
        let tabContainer = ZStack {
            if isMoving {
                Color.clear
            } else {
                tabLabel(tab)
                    .frame(width: width)
            }
        }
        // The slot swaps between its label and a transparent placeholder
        // instantly. Only the slot's geometry participates in reordering.
        .transaction { transaction in
            transaction.animation = nil
        }
        .frame(width: width, height: 32)
        .background {
            GeometryReader { proxy in
                Color.clear.preference(
                    key: ToolbarTabFramePreferenceKey.self,
                    value: [
                        tab.id: proxy.frame(in: .named(ToolbarTabStripCoordinateSpace.name))
                    ]
                )
            }
        }
        .contentShape(tabShape(for: tab))
        .id(tab.id)
        // Move the real Tab rather than drawing a decorative proxy above the
        // ScrollView. Its layout slot remains unchanged, preserving native
        // scrolling and scrollTo semantics while rendering, hit testing and
        // accessibility all follow the same accordion presentation.
        .visualEffect { content, geometry in
            content.offset(
                x: toolbarAccordionTabOffset(
                    tabFrame: geometry.frame(
                        in: .named(ToolbarTabStripCoordinateSpace.name)
                    ),
                    viewportWidth: viewportWidth
                )
            )
        }
        // A Tab can migrate between two nested ForEach containers when it
        // crosses a Tabs Group boundary. Never let SwiftUI turn that logical
        // move into its default remove/insert opacity transition.
        .transition(.identity)

        tabContainer
    }

    /// A single stable foreground instance follows the pointer for the whole
    /// drag. It lives above every Tabs/Tabs Group subtree, so model reordering
    /// cannot remove, insert, cross-fade, or clip the moving Tab.
    @ViewBuilder
    private var movingTabForeground: some View {
        if let tabID = interaction.movingTabID,
           let tab = viewModel.tabs.first(where: { $0.id == tabID }),
           let frame = interaction.movingTabFrame(for: tabID) {
            tabLabel(tab)
                .frame(width: frame.width, height: frame.height)
                .position(x: frame.midX, y: frame.midY)
                .allowsHitTesting(false)
                .accessibilityHidden(true)
                .zIndex(1_000)
                .id(tabID)
                .transition(.identity)
                // Pointer tracking must be direct and the foreground surface
                // must never inherit a sibling-slot animation transaction.
                .transaction { transaction in
                    transaction.animation = nil
                }
        }
    }

    /// Chrome drags a group header and all of its Tabs as one foreground block.
    /// The original subtree remains as a transparent layout placeholder until
    /// its new model position has been laid out.
    @ViewBuilder
    private func movingGroupForeground(tabWidths: [UUID: CGFloat]) -> some View {
        if let groupID = interaction.movingGroupID,
           let group = viewModel.tabGroups.first(where: { $0.id == groupID }),
           let frame = interaction.movingGroupFrame(for: groupID) {
            let tabs = viewModel.tabs.filter { $0.groupId == groupID }

            HStack(spacing: 2) {
                groupHeader(group, tabCount: tabs.count)
                    .frame(width: groupLabelWidth(group))

                if !group.isCollapsed {
                    ForEach(tabs) { tab in
                        let width = tabWidths[tab.id] ?? preferredTabWidth(for: tab)
                        tabLabel(tab)
                            .frame(width: width, height: 32)
                    }
                }
            }
            .frame(width: frame.width, height: frame.height)
            .background {
                groupSurface(for: group, isMoving: true)
            }
            .overlay {
                groupShape(for: group)
                    .strokeBorder(group.color.swiftUIColor.opacity(0.85), lineWidth: 1.5)
            }
            .position(x: frame.midX, y: frame.midY)
            .allowsHitTesting(false)
            .accessibilityHidden(true)
            .zIndex(1_000)
            .id(groupID)
            .transition(.identity)
            .transaction { transaction in
                transaction.animation = nil
            }
        }
    }

    private func interactionState(for tab: Tab) -> ToolbarTabInteractionPhase {
        interaction.phase(for: tab.id, activeTabID: viewModel.currentTabId)
    }

    private var tabReorderGesture: some Gesture {
        DragGesture(
            minimumDistance: 6,
            coordinateSpace: .named(ToolbarTabStripCoordinateSpace.name)
        )
        .onChanged { value in
            interaction.updateInPlaceDrag(
                startLocationX: value.startLocation.x,
                locationX: value.location.x,
                activeTabID: viewModel.currentTabId,
                viewModel: viewModel
            )
        }
        .onEnded { _ in
            interaction.finishDrag(activeTabID: viewModel.currentTabId)
        }
    }

    @ViewBuilder
    private func groupedTabs(
        _ group: TabGroup,
        tabs: [Tab],
        tabWidths: [UUID: CGFloat],
        viewportWidth: CGFloat
    ) -> some View {
        let isMoving = interaction.isMovingGroup(group.id)
        let content = HStack(spacing: 2) {
            groupHeader(group, tabCount: tabs.count)
                .frame(width: groupLabelWidth(group))
                .background {
                    GeometryReader { proxy in
                        Color.clear.preference(
                            key: ToolbarTabGroupHeaderFramePreferenceKey.self,
                            value: [
                                group.id: proxy.frame(
                                    in: .named(ToolbarTabStripCoordinateSpace.name)
                                )
                            ]
                        )
                    }
                }

            if !group.isCollapsed {
                ForEach(tabs) { tab in
                    renderedTab(
                        tab,
                        width: tabWidths[tab.id] ?? preferredTabWidth(for: tab),
                        viewportWidth: viewportWidth
                    )
                }
            }
        }
        .frame(height: 32)
        .background {
            groupSurface(for: group, isMoving: false)
        }
        .background {
            GeometryReader { proxy in
                Color.clear.preference(
                    key: ToolbarTabGroupFramePreferenceKey.self,
                    value: [
                        group.id: proxy.frame(in: .named(ToolbarTabStripCoordinateSpace.name))
                    ]
                )
            }
        }
        .overlay {
            groupShape(for: group)
                .strokeBorder(group.color.swiftUIColor.opacity(0.85), lineWidth: 1.5)
        }
        .opacity(isMoving ? 0 : 1)
        .transition(.identity)
        .transaction { transaction in
            transaction.animation = nil
        }

        content
    }

    /// Static groups keep the toolbar's translucent tint. While the group is
    /// moving, Chrome treats the header and every member Tab as one foreground
    /// surface, so the same tint is composited over an opaque toolbar-colored
    /// plate. Nothing underneath the dragged group can bleed through.
    private func groupSurface(for group: TabGroup, isMoving: Bool) -> some View {
        let shape = groupShape(for: group)

        return ZStack {
            // Keep both layers in one non-animated surface. This matches the
            // moving Tab plate and prevents a transient opacity interpolation
            // when the group first leaves its layout slot.
            shape.fill(isMoving ? AppColors.windowChromeDragPlateColor : Color.clear)
            shape.fill(group.color.swiftUIColor.opacity(0.08))
        }
        .transaction { transaction in
            transaction.animation = nil
        }
    }

    private func groupHeader(_ group: TabGroup, tabCount: Int) -> some View {
        let title = group.name.isEmpty ? "Tab Group" : group.name
        let containsCurrentTab = viewModel.currentTab?.groupId == group.id

        return HStack(spacing: 3) {
            Button {
                exitEditingMode()
                withAnimation(ToolbarTabMotion.slide) {
                    viewModel.toggleGroupCollapsed(groupId: group.id)
                }
            } label: {
                Image(systemName: group.isCollapsed ? "chevron.right" : "chevron.down")
                    .font(.system(size: 9, weight: .bold))
                    .frame(width: 16, height: 22)
            }
            .buttonStyle(.plain)
            .help(group.isCollapsed ? "Expand Group" : "Collapse Group")

            Button {
                exitEditingMode()
                groupNameDraft = group.name
                editingGroupID = group.id
            } label: {
                HStack(spacing: 5) {
                    Circle()
                        .fill(group.color.swiftUIColor)
                        .frame(width: 9, height: 9)

                    Text(title)
                        .font(.system(size: 12, weight: .semibold))
                        .lineLimit(1)

                    Text("\(tabCount)")
                        .font(.system(size: 10, weight: .semibold))
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .buttonStyle(.plain)
            .help("Edit Tab Group")
            .popover(isPresented: groupEditorBinding(for: group), arrowEdge: .bottom) {
                groupEditor(group)
            }
        }
        .padding(.horizontal, 5)
        .frame(height: 30)
        .background {
            groupHeaderShape(for: group)
                .fill(group.color.swiftUIColor.opacity(containsCurrentTab ? 0.32 : 0.2))
        }
        .contextMenu {
            Button("Edit Group…") {
                groupNameDraft = group.name
                editingGroupID = group.id
            }
            Button("New Tab in Group") { addNewTab(to: group) }
            Button(group.isCollapsed ? "Expand Group" : "Collapse Group") {
                viewModel.toggleGroupCollapsed(groupId: group.id)
            }
            Divider()
            Button("Ungroup") { viewModel.deleteGroup(groupId: group.id) }
            Button("Close Group", role: .destructive) {
                viewModel.closeGroup(groupId: group.id, historyVM: historyVM)
            }
        }
        .accessibilityLabel("\(title), \(tabCount) tabs")
    }

    private func groupEditorBinding(for group: TabGroup) -> Binding<Bool> {
        Binding(
            get: { editingGroupID == group.id },
            set: { isPresented in
                if !isPresented, editingGroupID == group.id {
                    commitGroupName(group)
                    editingGroupID = nil
                }
            }
        )
    }

    private func groupEditor(_ group: TabGroup) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            TextField("Group name", text: $groupNameDraft)
                .textFieldStyle(.roundedBorder)
                .frame(width: 224)
                .onSubmit {
                    commitGroupName(group)
                    editingGroupID = nil
                }

            HStack(spacing: 7) {
                ForEach(TabGroupColor.allCases, id: \.self) { color in
                    Button {
                        viewModel.updateGroup(groupId: group.id, color: color)
                    } label: {
                        Circle()
                            .fill(color.swiftUIColor)
                            .frame(width: 18, height: 18)
                            .overlay {
                                Circle()
                                    .strokeBorder(Color.primary, lineWidth: group.color == color ? 2 : 0)
                                    .padding(-3)
                            }
                    }
                    .buttonStyle(.plain)
                    .help(color.displayName)
                }
            }

            Divider()

            groupEditorButton("plus.rectangle", title: "New Tab in Group") {
                commitGroupName(group)
                addNewTab(to: group)
            }
            groupEditorButton("rectangle.badge.minus", title: "Ungroup") {
                editingGroupID = nil
                viewModel.deleteGroup(groupId: group.id)
            }
            groupEditorButton("xmark.rectangle", title: "Close Group", role: .destructive) {
                editingGroupID = nil
                viewModel.closeGroup(groupId: group.id, historyVM: historyVM)
            }
        }
        .padding(16)
    }

    private func groupEditorButton(
        _ symbol: String,
        title: String,
        role: ButtonRole? = nil,
        action: @escaping () -> Void
    ) -> some View {
        Button(role: role, action: action) {
            Label(title, systemImage: symbol)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .buttonStyle(.plain)
    }

    private func commitGroupName(_ group: TabGroup) {
        let name = groupNameDraft.trimmingCharacters(in: .whitespacesAndNewlines)
        if name != group.name {
            viewModel.updateGroup(groupId: group.id, name: name)
        }
    }

    private func addNewTab(to group: TabGroup) {
        let newTab = viewModel.addNewTab()
        viewModel.addTabToGroup(tabId: newTab.id, groupId: group.id)
        viewModel.updateGroup(groupId: group.id, isCollapsed: false)
    }

    @ViewBuilder
    private func tabLabel(_ tab: Tab) -> some View {
        switch interactionState(for: tab) {
        case .inactive:
            inactiveTab(tab)
                .transition(.identity)
        case .active:
            activeTab(tab)
                .transition(.identity)
        case .addressEditing:
            addressEditingTab(tab)
                .transition(.identity)
        case .draggingInactive:
            inactiveTab(tab)
                .transition(.identity)
        case .draggingActive:
            activeTab(tab)
                .transition(.identity)
        }
    }

    private func inactiveTab(_ tab: Tab) -> some View {
        HStack(spacing: 7) {
            tabIdentityIcon(for: tab)

            Text(tab.title.isEmpty ? "New Tab" : tab.title)
                .font(.system(size: 12.5))
                .lineLimit(1)
                .truncationMode(.tail)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.horizontal, 10)
        .frame(height: 32)
        .background {
            tabSurface(for: tab, tintOpacity: 0.035)
        }
        .overlay(alignment: .bottom) { groupIndicator(for: tab) }
        .contentShape(tabShape(for: tab))
        .onTapGesture {
            exitEditingMode()
            viewModel.switchToTab(id: tab.id)
        }
        .contextMenu { tabContextMenu(for: tab) }
        .accessibilityLabel(tab.title.isEmpty ? "New Tab" : tab.title)
    }

    private func activeTab(_ tab: Tab) -> some View {
        HStack(spacing: 3) {
            tabToolbarButton("chevron.left", help: "Back", disabled: !viewModel.canGoBack) {
                viewModel.goBack()
            }
            tabToolbarButton("chevron.right", help: "Forward", disabled: !viewModel.canGoForward) {
                viewModel.goForward()
            }

            Rectangle()
                .fill(Color.primary.opacity(AppColors.usesDarkAppearance ? 0.24 : 0.16))
                .frame(width: 1, height: 18)
                // The chevron has more optical whitespace than the favicon.
                // Shift the divider within the same 5pt allocation so both
                // visible glyphs, rather than their frames, feel equidistant.
                .padding(.leading, 0)
                .padding(.trailing, 4)
                .allowsHitTesting(false)

            activeAddressRegion(for: tab)

            HoverIconButton(
                symbol: "xmark",
                help: "Close Tab"
            ) {
                viewModel.closeTab(id: tab.id)
            }
        }
        .padding(.horizontal, 6)
        .frame(height: 32)
        .background {
            tabSurface(for: tab, tintOpacity: 0.11)
        }
        .overlay {
            tabShape(for: tab)
                .strokeBorder(
                    Color.primary.opacity(AppColors.usesDarkAppearance ? 0.16 : 0.10),
                    lineWidth: 0.75
                )
        }
        .overlay(alignment: .bottom) { groupIndicator(for: tab) }
        .contentShape(tabShape(for: tab))
        .onHover { isInside in
            hoveredTabID = isInside ? tab.id : nil
        }
        .contextMenu { tabContextMenu(for: tab) }
        .accessibilityLabel(tab.title.isEmpty ? "New Tab" : tab.title)
        .accessibilityAddTraits(.isSelected)
    }

    /// Static Tabs keep their translucent tint. During an in-place drag, the
    /// same tint is composited over an opaque toolbar-colored surface so the
    /// moving Tab behaves like Chrome's foreground Tab and occludes siblings.
    private func tabSurface(for tab: Tab, tintOpacity: Double) -> some View {
        let shape = tabShape(for: tab)
        let isMoving = interaction.isMovingTab(tab.id)

        return ZStack {
            // Keep the opaque drag plate mounted at all times. Inserting or
            // removing this layer inside a reorder animation would briefly
            // cross-fade the foreground Tab.
            shape.fill(isMoving ? AppColors.windowChromeDragPlateColor : Color.clear)
            shape.fill(Color.primary.opacity(tintOpacity))
        }
        // Reordering still animates the Tab's geometry, but never its surface.
        .transaction { transaction in
            transaction.animation = nil
        }
    }

    private func addressEditingTab(_ tab: Tab) -> some View {
        HStack(spacing: 7) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(.secondary)
                .frame(width: 15)

            ZStack(alignment: .leading) {
                if addressText.isEmpty && !hasInputActivity {
                    Text("Search or enter website name")
                        .font(.system(size: 13))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.tail)
                        .allowsHitTesting(false)
                }

                SelfDrawnAddressInputField(
                    text: $addressText,
                    onSubmit: submitAddress,
                    onBlur: exitEditingMode,
                    onInputActivity: { hasInputActivity = true }
                )
            }
            .frame(maxWidth: .infinity)
        }
        .padding(.horizontal, 10)
        .frame(height: 32)
        .background {
            tabShape(for: tab)
                .fill(Color.primary.opacity(0.13))
        }
        .overlay {
            tabShape(for: tab)
                .strokeBorder(Color.accentColor.opacity(0.85), lineWidth: 2)
        }
        .overlay(alignment: .bottom) { groupIndicator(for: tab) }
        .contentShape(tabShape(for: tab))
        .accessibilityLabel("Address for \(tab.title.isEmpty ? "New Tab" : tab.title)")
        .accessibilityAddTraits(.isSelected)
    }

    @ViewBuilder
    private func groupIndicator(for tab: Tab) -> some View {
        if let groupID = tab.groupId,
           let group = viewModel.tabGroups.first(where: { $0.id == groupID }) {
            Capsule()
                .fill(Color(nsColor: group.color.nsColor))
                .frame(height: 2)
                .padding(.horizontal, 10)
        }
    }

    @ViewBuilder
    private func tabContextMenu(for tab: Tab) -> some View {
        if tab.groupId != nil {
            Button("Remove from Group") {
                viewModel.removeTabFromGroup(tabId: tab.id)
            }
        }

        Button("Add to New Group") {
            let group = viewModel.createGroup(name: "", color: .blue)
            viewModel.addTabToGroup(tabId: tab.id, groupId: group.id)
            DispatchQueue.main.async {
                groupNameDraft = ""
                editingGroupID = group.id
            }
        }

        if !viewModel.tabGroups.filter({ $0.id != tab.groupId }).isEmpty {
            Menu(tab.groupId == nil ? "Add to Group" : "Move to Group") {
                ForEach(viewModel.tabGroups.filter { $0.id != tab.groupId }) { group in
                    Button(group.name.isEmpty ? "Tab Group" : group.name) {
                        viewModel.addTabToGroup(tabId: tab.id, groupId: group.id)
                        viewModel.updateGroup(groupId: group.id, isCollapsed: false)
                    }
                }
            }
        }

        Divider()
        Button("Close Tab") { viewModel.closeTab(id: tab.id) }
        Button("Close Other Tabs") {
            let otherIDs = viewModel.tabs.map(\.id).filter { $0 != tab.id }
            viewModel.switchToTab(id: tab.id)
            otherIDs.forEach { viewModel.closeTab(id: $0) }
        }
    }

    private func tabFallbackIcon(for tab: Tab) -> String {
        if let appIcon = tab.appIcon { return appIcon }
        if let systemType = tab.systemTabType { return systemType.icon }
        if case .web = tab.contentType { return "globe" }
        return "doc.text"
    }

    private func tabIdentityIcon(for tab: Tab) -> some View {
        TabIdentityIcon(
            tab: tab,
            isLoading: viewModel.isTabLoading(tab.id),
            fallbackSymbol: tabFallbackIcon(for: tab)
        )
    }

    // MARK: - Address Bar

    private var activeAddressField: some View {
        Group {
            if addressText.isEmpty {
                Text("Search or enter website name")
                    .font(.system(size: 12.5))
                    .foregroundStyle(
                        isAddressFieldHovered
                            ? AppColors.tabSecondaryInteractiveHoverColor
                            : Color.secondary
                    )
                    .lineLimit(1)
                    .truncationMode(.tail)
            } else {
                Text(addressText)
                    .font(.system(size: 12.5))
                    .foregroundStyle(
                        isAddressFieldHovered
                            ? AppColors.tabPrimaryInteractiveHoverColor
                            : Color.primary
                    )
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
        .onHover { isInside in
            isAddressFieldHovered = isInside
        }
        .onTapGesture { enterEditingMode() }
    }

    private func activeAddressRegion(for tab: Tab) -> some View {
        HStack(spacing: 7) {
            tabIdentityIcon(for: tab)

            activeAddressField
                .frame(minWidth: 44, maxWidth: .infinity)
        }
        .frame(maxWidth: .infinity)
        .overlay(alignment: .trailing) {
            if showsAddressActions(for: tab) {
                HStack(spacing: 0) {
                    tabToolbarButton("arrow.clockwise", help: "Reload") {
                        viewModel.reloadCurrentTab()
                    }
                    tabToolbarButton(
                        isBookmarked ? "star.fill" : "star",
                        help: isBookmarked ? "Remove Bookmark" : "Add Bookmark"
                    ) {
                        toggleBookmark()
                    }
                    activeShareButton(for: tab)
                }
                .padding(.leading, 6)
            }
        }
        .contentShape(Rectangle())
    }

    private func showsAddressActions(for tab: Tab) -> Bool {
        hoveredTabID == tab.id
            || addressPanelPresentation.isPresentingPanel(anchoredTo: tab.id)
    }

    // MARK: - Address Bar Helpers

    private func displayText(for tab: Tab?) -> String {
        guard let tab = tab else { return "" }
        if let systemType = tab.systemTabType {
            if systemType == .startPage { return "" }
            return (tab.url ?? systemType.systemURL).absoluteString
        }
        if case .web = tab.contentType, let url = tab.url {
            let host = url.host ?? url.absoluteString
            return host.hasPrefix("www.") ? String(host.dropFirst(4)) : host
        }
        return tab.title
    }

    private func editingText(for tab: Tab?) -> String {
        guard let tab = tab else { return "" }
        if let systemType = tab.systemTabType {
            if systemType == .startPage { return "" }
            return (tab.url ?? systemType.systemURL).absoluteString
        }
        if let url = tab.url {
            return url.absoluteString
        }
        return displayText(for: tab)
    }

    private func enterEditingMode() {
        guard let currentTabID = viewModel.currentTabId,
              interaction.beginAddressEditing(
                  tabID: currentTabID,
                  activeTabID: viewModel.currentTabId
              ) else { return }
        isAddressFieldHovered = false
        addressText = editingText(for: viewModel.currentTab)
    }

    private func exitEditingMode() {
        interaction.endAddressEditing(activeTabID: viewModel.currentTabId)
        hasInputActivity = false
        updateDisplayText()
    }

    private func submitAddress() {
        let input = addressText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !input.isEmpty else { return }
        viewModel.navigateCurrentTab(to: input)
        exitEditingMode()
    }

    private func updateDisplayText() {
        guard !interaction.isAddressEditing else { return }
        addressText = displayText(for: viewModel.currentTab)
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

    private func toolbarBackground(metrics: ToolbarLayoutMetrics) -> some View {
        // White toolbar background with a top-left rounded corner.
        UnevenRoundedRectangle(
            topLeadingRadius: 12,
            bottomLeadingRadius: 0,
            bottomTrailingRadius: 0,
            topTrailingRadius: 0  // roundedContainer mask handles right corner
        )
        .fill(Color.white)
        .padding(.leading, metrics.backgroundLeadingX)
        .allowsHitTesting(false)
    }

    // MARK: - Actions

    private var isBookmarked: Bool {
        guard let url = viewModel.currentTab?.url else { return false }
        return bookmarkVM.bookmarks.contains(where: { $0.appURL == url })
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

    // MARK: - Buttons

    private var newTabButton: some View {
        toolbarButton("plus", help: "New Tab") {
            exitEditingMode()
            _ = viewModel.addNewTab()
        }
    }

    private var overviewButton: some View {
        toolbarButton("square.on.square", help: "Tab Overview") {
            exitEditingMode()
            toggleOverview()
        }
    }

    private func activeShareButton(for tab: Tab) -> some View {
        Group {
            if let url = viewModel.currentTab?.url {
                HoverIconButton(
                    symbol: "square.and.arrow.up",
                    help: "Share"
                ) {
                    addressPanelPresentation.showSharePicker(
                        for: url,
                        anchoredTo: tab.id
                    )
                }
            }
        }
    }

    private func tabToolbarButton(
        _ symbol: String,
        help: String,
        disabled: Bool = false,
        action: @escaping () -> Void
    ) -> some View {
        HoverIconButton(
            symbol: symbol,
            help: help,
            disabled: disabled,
            action: action
        )
    }

    private func toolbarButton(
        _ symbol: String,
        help: String,
        disabled: Bool = false,
        action: @escaping () -> Void
    ) -> some View {
        HoverIconButton(
            symbol: symbol,
            help: help,
            metrics: .header,
            disabled: disabled,
            action: action
        )
    }
}

private enum HoverIconButtonMetrics {
    case tabEmbedded
    case header

    var glyphSize: CGFloat {
        switch self {
        case .tabEmbedded: return 12
        case .header: return 16
        }
    }

    var controlSize: CGFloat {
        switch self {
        case .tabEmbedded: return 24
        case .header: return CompactToolbarLayout.buttonSize
        }
    }
}

/// Shared line-only hover treatment for Header and Tab actions. Metrics are
/// selected by semantic role so adjacent controls cannot drift in size.
private struct HoverIconButton: View {
    let symbol: String
    let help: String
    var metrics: HoverIconButtonMetrics = .tabEmbedded
    var disabled = false
    let action: () -> Void

    @State private var isHovered = false

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: metrics.glyphSize, weight: .medium))
                .frame(width: metrics.controlSize, height: metrics.controlSize)
                .foregroundStyle(
                    isHovered && !disabled
                        ? AppColors.tabSecondaryInteractiveHoverColor
                        : Color.secondary
                )
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .disabled(disabled)
        .opacity(disabled ? 0.35 : 1)
        .onHover { isInside in
            isHovered = isInside && !disabled
        }
        .help(help)
        .accessibilityLabel(help)
    }
}

/// One visual identity for a Tab across active and inactive Header states.
/// Web Tabs show their favicon; while loading, Chrome-style progress rotates
/// around the favicon instead of occupying the Tab's bottom border.
private struct TabIdentityIcon: View {
    let tab: Tab
    let isLoading: Bool
    let fallbackSymbol: String

    @State private var loadedFavicon: NSImage?

    private var webURL: URL? {
        guard case .web(let url) = tab.contentType else { return nil }
        return url
    }

    private var favicon: NSImage? {
        if let loadedFavicon { return loadedFavicon }
        guard let webURL else { return nil }
        return FaviconCache.shared.getSync(for: webURL)
    }

    var body: some View {
        ZStack {
            if let favicon {
                Image(nsImage: favicon)
                    .resizable()
                    .interpolation(.high)
                    .frame(
                        width: isLoading ? 10 : 14,
                        height: isLoading ? 10 : 14
                    )
            } else {
                Image(systemName: fallbackSymbol)
                    .font(.system(size: isLoading ? 9 : 12, weight: .medium))
                    .foregroundStyle(.secondary)
            }

            if isLoading {
                ChromeTabLoadingRing()
            }
        }
        .frame(width: 16, height: 16)
        .task(id: webURL?.host) {
            guard let webURL else {
                loadedFavicon = nil
                return
            }
            loadedFavicon = await FaviconCache.shared.image(for: webURL)
        }
        .accessibilityHidden(true)
    }
}

private struct ChromeTabLoadingRing: View {
    @State private var rotation = 0.0

    var body: some View {
        ZStack {
            Circle()
                .stroke(Color.secondary.opacity(0.22), lineWidth: 1.5)

            Circle()
                .trim(from: 0.08, to: 0.72)
                .stroke(
                    Color.accentColor,
                    style: StrokeStyle(lineWidth: 1.5, lineCap: .round)
                )
                .rotationEffect(.degrees(rotation))
        }
        .frame(width: 16, height: 16)
        .onAppear {
            withAnimation(.linear(duration: 0.72).repeatForever(autoreverses: false)) {
                rotation = 360
            }
        }
        .allowsHitTesting(false)
    }
}

// MARK: - Address Input Field (NSViewRepresentable)

private struct SelfDrawnAddressInputField: NSViewRepresentable {
    @Binding var text: String
    var onSubmit: () -> Void
    var onBlur: () -> Void
    var onInputActivity: (() -> Void)?

    func makeNSView(context: Context) -> NSTextField {
        let textField = NSTextField()
        textField.font = .systemFont(ofSize: 13)
        textField.textColor = .black
        textField.appearance = NSAppearance(named: .aqua)
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
        textField.isAutomaticTextCompletionEnabled = false
        textField.stringValue = text
        context.coordinator.textField = textField

        // SwiftUI mounts the NSView into the window during layout, which can
        // happen after this runloop tick — a single async focus attempt loses
        // the race (window == nil → makeFirstResponder silently skipped) and
        // the field never gains focus. Retry until attached.
        SelfDrawnAddressInputField.focusWhenAttached(
            textField: textField,
            coordinator: context.coordinator,
            attempt: 0
        )

        return textField
    }

    private static func focusWhenAttached(
        textField: NSTextField,
        coordinator: Coordinator,
        attempt: Int
    ) {
        DispatchQueue.main.async {
            guard let window = textField.window else {
                if attempt < 30 {  // ~0.5s of runloop ticks, well within mount time
                    SelfDrawnAddressInputField.focusWhenAttached(
                        textField: textField,
                        coordinator: coordinator,
                        attempt: attempt + 1
                    )
                }
                return
            }
            let _ = window.makeFirstResponder(textField)
            textField.currentEditor()?.selectAll(nil)
            coordinator.startObservingFirstResponder()
        }
    }

    func updateNSView(_ nsView: NSTextField, context: Context) {}

    func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }

    class Coordinator: NSObject, NSTextFieldDelegate {
        var parent: SelfDrawnAddressInputField
        weak var textField: NSTextField?
        private var firstResponderObservation: NSKeyValueObservation?

        init(_ parent: SelfDrawnAddressInputField) {
            self.parent = parent
            super.init()
        }

        deinit {
            stopObservingFirstResponder()
        }

        func startObservingFirstResponder() {
            guard let window = textField?.window else { return }
            firstResponderObservation = window.observe(\.firstResponder, options: [.new]) { [weak self] _, _ in
                self?.checkFirstResponder()
            }
        }

        func stopObservingFirstResponder() {
            firstResponderObservation?.invalidate()
            firstResponderObservation = nil
        }

        private func checkFirstResponder() {
            guard let textField = textField, let window = textField.window else { return }

            let firstResponder = window.firstResponder
            let isOurTextField = (firstResponder === textField)
            let isFieldEditor = (firstResponder as? NSTextView)?.delegate === textField

            if !isOurTextField && !isFieldEditor {
                DispatchQueue.main.async {
                    self.parent.onBlur()
                }
            }
        }

        func controlTextDidEndEditing(_ obj: Notification) {
            stopObservingFirstResponder()
            parent.onBlur()
        }

        func controlTextDidBeginEditing(_ obj: Notification) {
            startObservingFirstResponder()
            parent.onInputActivity?()
        }

        func controlTextDidChange(_ obj: Notification) {
            guard let tf = obj.object as? NSTextField else { return }
            parent.text = tf.stringValue
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
