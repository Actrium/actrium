import AppKit
import SwiftUI
import Combine

// MARK: - Invisible Divider Split View

/// Keep split behavior but remove the visual divider line.
/// Drag hit area is provided by delegate effectiveRect callbacks.
private final class InvisibleDividerSplitView: NSSplitView {
    override var dividerColor: NSColor { .clear }
    override var dividerThickness: CGFloat { 0 }

    override func drawDivider(in rect: NSRect) {
        // Intentionally no-op to keep divider visually hidden.
    }

}

/// Split view controller managing sidebar and content areas
final class MainSplitViewController: NSSplitViewController {

    // MARK: - Properties

    private let viewModel: BrowserViewModel
    private let historyVM: HistoryViewModel
    private let bookmarkVM: BookmarkViewModel
    private let edgeInset: CGFloat
    private let sidebarLayoutState: SidebarLayoutState

    private var sidebarItem: NSSplitViewItem!
    private var contentItem: NSSplitViewItem!

    private var sidebarViewController: SidebarViewController!
    private var contentViewController: ContentViewController!

    private var cancellables = Set<AnyCancellable>()
    private var dividerDebugEventMonitors: [Any] = []
    private var lastKnownDividerRects: [Int: NSRect] = [:]
    private var dividerMonitorDragActive = false
    private var dividerMonitorStartMouseX: CGFloat = 0
    private var dividerMonitorStartDividerX: CGFloat = 0
    private var dividerMonitorHasDragged = false

    /// The sidebar width set by user drag. Window resize restores this value.
    private var pinnedSidebarWidth: CGFloat = 250

    /// Divider hit zone extensions (points). Keeps visual divider thin,
    /// but expands the draggable area for better usability.
    private let dividerHitWidth: CGFloat = 5
    private let contentLeadingPassthroughWidth: CGFloat = 24
    private let dividerCollapseTrigger: CGFloat = 24
    private let dividerDebugNearRange: CGFloat = 80
    private let dividerDebugLogPath = "/tmp/canvas_divider_debug.log"
    private let dividerLogMouseMovedEvents = false

    private var dividerHitHalfWidth: CGFloat {
        dividerHitWidth * 0.5
    }

    private var lastLoggedEffectiveRectSnapshot: (drawn: NSRect, proposed: NSRect, returned: NSRect)?

    /// Sidebar width constraints
    private let minSidebarWidth: CGFloat = 180
    private let maxSidebarWidthRatio: CGFloat = 0.5

    // MARK: - Initialization

    init(
        viewModel: BrowserViewModel,
        historyVM: HistoryViewModel,
        bookmarkVM: BookmarkViewModel,
        edgeInset: CGFloat,
        sidebarLayoutState: SidebarLayoutState
    ) {
        self.viewModel = viewModel
        self.historyVM = historyVM
        self.bookmarkVM = bookmarkVM
        self.edgeInset = edgeInset
        self.sidebarLayoutState = sidebarLayoutState

        super.init(nibName: nil, bundle: nil)

        // AppKit-documented path: set custom splitView before view is loaded.
        // This avoids runtime class swapping side effects.
        self.splitView = InvisibleDividerSplitView(frame: .zero)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    // MARK: - Lifecycle

    override func viewDidLoad() {
        super.viewDidLoad()

        setupSplitView()
        setupViewControllers()
        setupNotifications()
    }

    deinit {
        for monitor in dividerDebugEventMonitors {
            NSEvent.removeMonitor(monitor)
        }
    }

    // MARK: - Setup

    private func setupSplitView() {
        splitView.isVertical = true
        splitView.dividerStyle = .thin
        splitView.delegate = self
    }

    private func setupViewControllers() {
        // Create sidebar view controller
        sidebarViewController = SidebarViewController(
            viewModel: viewModel,
            bookmarkVM: bookmarkVM,
            historyVM: historyVM
        )

        // Create content view controller
        contentViewController = ContentViewController(
            viewModel: viewModel,
            historyVM: historyVM,
            bookmarkVM: bookmarkVM
        )

        // Create split view items (plain item to avoid system source-list rounded background)
        sidebarItem = NSSplitViewItem(viewController: sidebarViewController)
        sidebarItem.canCollapse = true
        sidebarItem.minimumThickness = minSidebarWidth
        sidebarItem.maximumThickness = 400  // Updated dynamically in syncSidebarLayoutState()

        contentItem = NSSplitViewItem(viewController: contentViewController)
        contentItem.minimumThickness = 300

        // Add items
        addSplitViewItem(sidebarItem)
        addSplitViewItem(contentItem)

        // Default passthrough before first layout sync.
        contentViewController.setLeadingDividerPassthroughWidth(contentLeadingPassthroughWidth)
    }

    private func setupNotifications() {
        // Listen for sidebar toggle requests
        NotificationCenter.default.publisher(for: .toggleSidebarRequested)
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                self?.toggleSidebar(nil)
            }
            .store(in: &cancellables)
    }

    // MARK: - Split View Updates

    override func viewDidLayout() {
        super.viewDidLayout()

        // After NSSplitViewController's proportional resize, restore the pinned sidebar width.
        if !dividerMonitorDragActive,
           !sidebarItem.isCollapsed,
           let sidebarView = sidebarViewController?.view,
           abs(sidebarView.frame.width - pinnedSidebarWidth) > 1 {
            splitView.setPosition(pinnedSidebarWidth, ofDividerAt: 0)
        }

        syncSidebarLayoutState()
    }

    override func viewDidAppear() {
        super.viewDidAppear()
        // By viewDidAppear the split view subviews have their real frames.
        // Force a sync to replace any stale default values in SidebarLayoutState.
        syncSidebarLayoutState()
        setupDividerDebugMonitorsIfNeeded()
    }

    override func splitViewDidResizeSubviews(_ notification: Notification) {
        syncSidebarLayoutState()
    }

    override func splitView(
        _ splitView: NSSplitView,
        effectiveRect proposedEffectiveRect: NSRect,
        forDrawnRect drawnRect: NSRect,
        ofDividerAt dividerIndex: Int
    ) -> NSRect {
        guard dividerIndex == 0,
              splitView.isVertical,
              !sidebarItem.isCollapsed else {
            return proposedEffectiveRect
        }

        // Cache a stable divider rect from AppKit callback geometry.
        let logicalDividerWidth = max(splitView.dividerThickness, 1)
        let logicalDividerMidX = proposedEffectiveRect.midX
        let logicalDividerRect = NSRect(
            x: logicalDividerMidX - logicalDividerWidth * 0.5,
            y: splitView.bounds.minY,
            width: logicalDividerWidth,
            height: splitView.bounds.height
        )
        lastKnownDividerRects[dividerIndex] = logicalDividerRect

        let effective = NSRect(
            x: logicalDividerMidX - dividerHitHalfWidth,
            y: splitView.bounds.minY,
            width: dividerHitWidth,
            height: splitView.bounds.height
        )

        if shouldLogEffectiveRectChange(drawn: drawnRect, proposed: proposedEffectiveRect, returned: effective) {
            debugLog("effectiveRect idx=\(dividerIndex) drawn=\(formatRect(drawnRect)) proposed=\(formatRect(proposedEffectiveRect)) return=\(formatRect(effective))")
        }
        return effective
    }

    override func splitView(_ splitView: NSSplitView, additionalEffectiveRectOfDividerAt dividerIndex: Int) -> NSRect {
        guard dividerIndex == 0,
              splitView.isVertical,
              !sidebarItem.isCollapsed else {
            return .zero
        }

        return .zero
    }

    private func savePinnedSidebarWidth() {
        guard !sidebarItem.isCollapsed,
              let sidebarView = sidebarViewController?.view else { return }
        let w = sidebarView.frame.width
        if w > 1 { pinnedSidebarWidth = w }
    }

    private func syncSidebarLayoutState() {
        guard let sidebarView = sidebarViewController?.view else { return }
        let contentView = contentViewController?.view

        // Keep sidebar maximum width proportional to current container width.
        let dynamicMaxSidebarWidth = max(minSidebarWidth, floor(view.bounds.width * maxSidebarWidthRatio))
        if abs(sidebarItem.maximumThickness - dynamicMaxSidebarWidth) > 0.5 {
            sidebarItem.maximumThickness = dynamicMaxSidebarWidth
        }

        let sidebarWidth = max(0, sidebarView.frame.width)
        let dividerThickness = max(0, splitView.dividerThickness)

        // If the split view item reports the sidebar is open but the view
        // has zero width, the subview hasn't been laid out yet.  Keep
        // isSidebarCollapsed=false (so the toolbar doesn't flash the
        // collapsed state) but don't update geometry — we don't have
        // real values yet.
        if !sidebarItem.isCollapsed && sidebarWidth <= 0.5 { return }

        let isCollapsed = sidebarItem.isCollapsed
        contentViewController.setLeadingDividerPassthroughWidth(isCollapsed ? 0 : contentLeadingPassthroughWidth)
        let splitBoundaryX: CGFloat
        if isCollapsed {
            // Keep toolbar/background collapse synchronized with sidebar snap-to-zero.
            splitBoundaryX = 0
        } else {
            // NSSplitViewController may wrap each VC's view in a container,
            // so contentView.frame.minX is relative to the wrapper (often 0),
            // not to the split view.  Use coordinate conversion to get the
            // content's actual leading edge in the split view's space.
            let contentOriginX: CGFloat
            if let cv = contentView {
                contentOriginX = max(0, cv.convert(CGPoint.zero, to: splitView).x)
            } else {
                contentOriginX = 0
            }

            if contentOriginX > dividerThickness {
                splitBoundaryX = contentOriginX
            } else {
                // Fallback: sidebar width + divider
                splitBoundaryX = max(0, sidebarWidth + dividerThickness)
            }
        }

        sidebarLayoutState.update(
            sidebarWidth: sidebarWidth,
            splitBoundaryX: splitBoundaryX,
            isSidebarCollapsed: isCollapsed,
            windowWidth: view.bounds.width
        )

        // Keep legacy titlebar path in sync for the alternative custom titlebar implementation.
        TitlebarManager.shared.updateSidebarWidth(splitBoundaryX + edgeInset)
        TitlebarManager.shared.updateSidebarVisibility(!isCollapsed)
    }

    // MARK: - Divider Debug Logs

    private func setupDividerDebugMonitorsIfNeeded() {
        guard dividerDebugEventMonitors.isEmpty else { return }

        try? FileManager.default.removeItem(atPath: dividerDebugLogPath)
        FileManager.default.createFile(atPath: dividerDebugLogPath, contents: nil)
        view.window?.acceptsMouseMovedEvents = true
        debugLog("=== divider debug start ===")

        var eventTypes: [NSEvent.EventTypeMask] = [.leftMouseDown, .leftMouseDragged, .leftMouseUp]
        if dividerLogMouseMovedEvents {
            eventTypes.insert(.mouseMoved, at: 0)
        }
        for type in eventTypes {
            guard let monitor = NSEvent.addLocalMonitorForEvents(matching: type, handler: { [weak self] event in
                guard let self else { return event }
                self.logDividerMouseEvent(event)
                if self.interceptDividerDragEvent(event) {
                    return nil
                }
                return event
            }) else { continue }
            dividerDebugEventMonitors.append(monitor)
        }

        let globalEventTypes: [NSEvent.EventTypeMask] = [.leftMouseDragged, .leftMouseUp]
        for type in globalEventTypes {
            guard let monitor = NSEvent.addGlobalMonitorForEvents(matching: type, handler: { [weak self] event in
                self?.handleGlobalDividerDragEvent(event)
            }) else { continue }
            dividerDebugEventMonitors.append(monitor)
        }
    }

    private func interceptDividerDragEvent(_ event: NSEvent) -> Bool {
        guard let window = view.window,
              event.window === window else { return false }

        let globalPointInSplit = splitPointFromGlobalMouse(in: window)
        let eventPointInSplit = splitView.convert(event.locationInWindow, from: nil)
        let screenPointInSplit = splitPointFromScreenMouse(in: window)
        switch event.type {
        case .leftMouseDown:
            if dividerMonitorDragActive {
                debugLog(
                    "monitor=down_ignored reason=already_active " +
                    "eventSplit=\(formatPoint(eventPointInSplit)) splitGlobal=\(formatPoint(globalPointInSplit))"
                )
                return true
            }
            let downSource: String
            let armPoint: CGPoint
            if shouldArmMonitorDrag(at: eventPointInSplit) {
                armPoint = eventPointInSplit
                downSource = "event"
            } else if shouldArmMonitorDrag(at: screenPointInSplit) {
                // NSRemoteView occasionally reports incorrect event.locationInWindow on first down.
                // Fallback to current screen mouse location to avoid missing the first drag attempt.
                armPoint = screenPointInSplit
                downSource = "screen"
            } else {
                return false
            }
            dividerMonitorDragActive = true
            dividerMonitorStartMouseX = armPoint.x
            dividerMonitorStartDividerX = currentDividerRect(in: splitView, dividerIndex: 0).midX
            dividerMonitorHasDragged = false
            debugLog(
                "monitor=monitor_down_local action=arm source=\(downSource) " +
                "armSplit=\(formatPoint(armPoint)) eventSplit=\(formatPoint(eventPointInSplit)) " +
                "splitScreen=\(formatPoint(screenPointInSplit)) splitGlobal=\(formatPoint(globalPointInSplit)) " +
                "startMouseX=\(String(format: "%.1f", dividerMonitorStartMouseX)) " +
                "startDividerX=\(String(format: "%.1f", dividerMonitorStartDividerX))"
            )
            return true
        case .leftMouseDragged:
            guard dividerMonitorDragActive else { return false }
            handleMonitorDividerDrag(at: eventPointInSplit, phase: "monitor_dragged_local")
            return true
        case .leftMouseUp:
            guard dividerMonitorDragActive else { return false }
            if (NSEvent.pressedMouseButtons & 1) != 0 {
                debugLog("monitor=monitor_up_local action=ignore_pressed split=\(formatPoint(eventPointInSplit))")
                return true
            }
            if dividerMonitorHasDragged {
                handleMonitorDividerDrag(at: eventPointInSplit, phase: "monitor_up_local")
            } else {
                debugLog("monitor=monitor_up_local action=no_drag split=\(formatPoint(eventPointInSplit))")
            }
            dividerMonitorDragActive = false
            savePinnedSidebarWidth()
            return true
        default:
            return false
        }
    }

    private func handleGlobalDividerDragEvent(_ event: NSEvent) {
        DispatchQueue.main.async { [weak self] in
            guard let self,
                  self.dividerMonitorDragActive,
                  let window = self.view.window else { return }

            let pointInSplit = self.splitPointFromGlobalMouse(in: window)
            switch event.type {
            case .leftMouseDragged:
                self.handleMonitorDividerDrag(at: pointInSplit, phase: "monitor_dragged_global")
            case .leftMouseUp:
                if self.dividerMonitorHasDragged {
                    self.handleMonitorDividerDrag(at: pointInSplit, phase: "monitor_up_global")
                } else {
                    self.debugLog("monitor=monitor_up_global action=no_drag split=\(self.formatPoint(pointInSplit))")
                }
                self.dividerMonitorDragActive = false
                self.savePinnedSidebarWidth()
            default:
                break
            }
        }
    }

    private func splitPointFromGlobalMouse(in window: NSWindow) -> CGPoint {
        // More reliable than NSEvent.mouseLocation when NSRemoteView is involved.
        let pointInWindow = window.mouseLocationOutsideOfEventStream
        return splitView.convert(pointInWindow, from: nil)
    }

    private func splitPointFromScreenMouse(in window: NSWindow) -> CGPoint {
        let pointInWindow = window.convertPoint(fromScreen: NSEvent.mouseLocation)
        return splitView.convert(pointInWindow, from: nil)
    }

    private func shouldArmMonitorDrag(at pointInSplit: CGPoint) -> Bool {
        guard splitView.isVertical,
              !sidebarItem.isCollapsed else { return false }

        let dividerRect = currentDividerRect(in: splitView, dividerIndex: 0)
        let triggerRect = NSRect(
            x: dividerRect.midX - dividerHitHalfWidth,
            y: splitView.bounds.minY,
            width: dividerHitWidth,
            height: splitView.bounds.height
        )
        return triggerRect.contains(pointInSplit)
    }

    private func handleMonitorDividerDrag(at pointInSplit: CGPoint, phase: String) {
        let minPos = splitView.minPossiblePositionOfDivider(at: 0)
        let maxPos = splitView.maxPossiblePositionOfDivider(at: 0)
        let delta = pointInSplit.x - dividerMonitorStartMouseX
        if abs(delta) > 0.5 {
            dividerMonitorHasDragged = true
        }
        let proposed = dividerMonitorStartDividerX + delta

        if !sidebarItem.isCollapsed, proposed <= (minPos - dividerCollapseTrigger) {
            NSAnimationContext.beginGrouping()
            NSAnimationContext.current.duration = 0
            NSAnimationContext.current.allowsImplicitAnimation = false
            sidebarItem.isCollapsed = true
            NSAnimationContext.endGrouping()
            syncSidebarLayoutState()
            debugLog("monitor=\(phase) action=collapse split=\(formatPoint(pointInSplit)) min=\(String(format: "%.1f", minPos))")
            return
        }

        if sidebarItem.isCollapsed, proposed > (minPos + dividerCollapseTrigger) {
            NSAnimationContext.beginGrouping()
            NSAnimationContext.current.duration = 0
            NSAnimationContext.current.allowsImplicitAnimation = false
            sidebarItem.isCollapsed = false
            NSAnimationContext.endGrouping()
        }

        let clamped = min(max(proposed, minPos), maxPos)
        splitView.setPosition(clamped, ofDividerAt: 0)
        syncSidebarLayoutState()
        debugLog(
            "monitor=\(phase) action=setPosition split=\(formatPoint(pointInSplit)) delta=\(String(format: "%.1f", delta)) proposed=\(String(format: "%.1f", proposed)) " +
            "clamped=\(String(format: "%.1f", clamped)) min=\(String(format: "%.1f", minPos)) max=\(String(format: "%.1f", maxPos))"
        )
    }

    private func logDividerMouseEvent(_ event: NSEvent) {
        guard let window = view.window,
              event.window === window else { return }

        if event.type == .mouseMoved, !dividerLogMouseMovedEvents {
            return
        }

        let pointInSplit = splitView.convert(event.locationInWindow, from: nil)
        let pointInSplitGlobal = splitPointFromGlobalMouse(in: window)
        let dividerRect = currentDividerRect(in: splitView, dividerIndex: 0)
        let dividerX = dividerRect.midX
        let deltaX = pointInSplit.x - dividerX
        let isNearDivider = abs(deltaX) <= dividerDebugNearRange

        if event.type == .mouseMoved, !isNearDivider { return }
        if !splitView.bounds.insetBy(dx: -dividerDebugNearRange, dy: 0).contains(pointInSplit) { return }

        let splitHit = splitView.hitTest(pointInSplit)
        let contentView = contentViewController?.view
        let pointInContent = contentView?.convert(event.locationInWindow, from: nil) ?? .zero
        let contentHit = contentView?.hitTest(pointInContent)

        let rootHit: NSView? = {
            guard let root = window.contentView else { return nil }
            let p = root.convert(event.locationInWindow, from: nil)
            return root.hitTest(p)
        }()

        let sidebarWidth = sidebarViewController?.view.frame.width ?? 0
        let fullScreen = window.styleMask.contains(.fullScreen)
        let tabDesc: String = {
            guard let tab = viewModel.currentTab else { return "nil" }
            let systemType = tab.systemTabType?.rawValue ?? "none"
            let url = tab.url?.absoluteString ?? "nil"
            return "title=\(tab.title),system=\(systemType),url=\(url)"
        }()

        let eventName: String = {
            switch event.type {
            case .mouseMoved: return "moved"
            case .leftMouseDown: return "down"
            case .leftMouseDragged: return "dragged"
            case .leftMouseUp: return "up"
            default: return "other"
            }
        }()

        debugLog(
            "mouse=\(eventName) win=\(formatPoint(event.locationInWindow)) split=\(formatPoint(pointInSplit)) splitGlobal=\(formatPoint(pointInSplitGlobal)) " +
            "dividerRect=\(formatRect(dividerRect)) deltaX=\(String(format: "%.1f", deltaX)) near=\(isNearDivider ? 1 : 0) " +
            "collapsed=\(sidebarItem.isCollapsed ? 1 : 0) sidebarW=\(String(format: "%.1f", sidebarWidth)) " +
            "fullScreen=\(fullScreen ? 1 : 0) splitHit=\(typeName(splitHit)) contentHit=\(typeName(contentHit)) rootHit=\(typeName(rootHit)) " +
            "tab={\(tabDesc)}"
        )
    }

    private func currentDividerRect(in splitView: NSSplitView, dividerIndex: Int) -> NSRect {
        if let cached = lastKnownDividerRects[dividerIndex], cached.width > 0, cached.height > 0 {
            return cached
        }

        guard splitView.isVertical,
              splitView.subviews.count >= 2 else {
            return .zero
        }

        // NSSplitView subviews order is not guaranteed to be visual left-to-right.
        // Sort by x to get the visual left pane for divider 0 fallback.
        let ordered = splitView.subviews.sorted { $0.frame.minX < $1.frame.minX }
        guard ordered.count > dividerIndex else { return .zero }
        let leftSubview = ordered[dividerIndex]
        let x = leftSubview.frame.maxX
        return NSRect(
            x: x,
            y: splitView.bounds.minY,
            width: max(splitView.dividerThickness, 1),
            height: splitView.bounds.height
        )
    }

    private func shouldLogEffectiveRectChange(drawn: NSRect, proposed: NSRect, returned: NSRect) -> Bool {
        guard let last = lastLoggedEffectiveRectSnapshot else {
            lastLoggedEffectiveRectSnapshot = (drawn, proposed, returned)
            return true
        }

        let unchanged = rectApproximatelyEqual(last.drawn, drawn)
            && rectApproximatelyEqual(last.proposed, proposed)
            && rectApproximatelyEqual(last.returned, returned)
        if unchanged {
            return false
        }

        lastLoggedEffectiveRectSnapshot = (drawn, proposed, returned)
        return true
    }

    private func rectApproximatelyEqual(_ lhs: NSRect, _ rhs: NSRect, tolerance: CGFloat = 0.25) -> Bool {
        abs(lhs.origin.x - rhs.origin.x) <= tolerance
            && abs(lhs.origin.y - rhs.origin.y) <= tolerance
            && abs(lhs.size.width - rhs.size.width) <= tolerance
            && abs(lhs.size.height - rhs.size.height) <= tolerance
    }

    private func debugLog(_ message: String) {
        let now = Date()
        let ms = Int((now.timeIntervalSince1970 * 1000).truncatingRemainder(dividingBy: 1000))
        let formatter = DateFormatter()
        formatter.dateFormat = "HH:mm:ss"
        let ts = "\(formatter.string(from: now)).\(String(format: "%03d", ms))"
        let line = "[\(ts)] [DividerDebug] \(message)\n"

        if !FileManager.default.fileExists(atPath: dividerDebugLogPath) {
            FileManager.default.createFile(atPath: dividerDebugLogPath, contents: nil)
        }

        guard let handle = FileHandle(forWritingAtPath: dividerDebugLogPath),
              let data = line.data(using: .utf8) else { return }
        handle.seekToEndOfFile()
        handle.write(data)
        handle.closeFile()
    }

    private func formatPoint(_ point: CGPoint) -> String {
        "(\(String(format: "%.1f", point.x)),\(String(format: "%.1f", point.y)))"
    }

    private func formatRect(_ rect: CGRect) -> String {
        "[x=\(String(format: "%.1f", rect.origin.x)),y=\(String(format: "%.1f", rect.origin.y)),w=\(String(format: "%.1f", rect.size.width)),h=\(String(format: "%.1f", rect.size.height))]"
    }

    private func typeName(_ view: NSView?) -> String {
        guard let view else { return "nil" }
        return String(describing: type(of: view))
    }

    // MARK: - Public Methods

    @objc override func toggleSidebar(_ sender: Any?) {
        // Bypass super.toggleSidebar which always uses animator().
        // Wrap in explicit no-animation context to suppress any
        // implicit Core Animation the split view might trigger.
        NSAnimationContext.beginGrouping()
        NSAnimationContext.current.duration = 0
        NSAnimationContext.current.allowsImplicitAnimation = false
        sidebarItem.isCollapsed.toggle()
        NSAnimationContext.endGrouping()
        syncSidebarLayoutState()
    }
}
