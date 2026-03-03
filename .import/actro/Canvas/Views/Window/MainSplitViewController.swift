import AppKit
import SwiftUI
import Combine

// MARK: - Invisible Divider Split View

/// NSSplitView subclass that hides the divider line while keeping drag functionality.
private class InvisibleDividerSplitView: NSSplitView {
    override var dividerColor: NSColor { .clear }
    override func drawDivider(in rect: NSRect) {
        // Don't draw — the divider hit zone is handled by effectiveRect delegate methods.
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
    private var pendingDividerRestoreX: CGFloat?
    private var pendingDividerRestoreAttempts: Int = 0

    /// Divider hit zone extensions (points). Keeps visual divider thin,
    /// but expands the draggable area for better usability.
    private let dividerHitWidth: CGFloat = 5
    private let contentLeadingPassthroughWidth: CGFloat = 24
    private let dividerMinSidebarTolerance: CGFloat = 2
    private let dividerCollapseTrigger: CGFloat = 24
    private let dividerRestoreTolerance: CGFloat = 0.75
    private let dividerRestoreMaxAttempts: Int = 8
    private let dividerDebugNearRange: CGFloat = 80
    private let dividerDebugLogPath = "/tmp/canvas_divider_debug.log"

    private var dividerHitHalfWidth: CGFloat {
        dividerHitWidth * 0.5
    }

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
        // Swap the existing splitView's class to suppress divider drawing.
        // Safe because InvisibleDividerSplitView adds no stored properties.
        object_setClass(splitView, InvisibleDividerSplitView.self)
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
        sidebarItem.holdingPriority = .defaultHigh

        contentItem = NSSplitViewItem(viewController: contentViewController)
        contentItem.minimumThickness = 300
        contentItem.holdingPriority = .defaultLow

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

        let windowEvents: [(Notification.Name, String)] = [
            (NSWindow.willEnterFullScreenNotification, "will_enter"),
            (NSWindow.didEnterFullScreenNotification, "did_enter"),
            (NSWindow.willExitFullScreenNotification, "will_exit"),
            (NSWindow.didExitFullScreenNotification, "did_exit")
        ]
        for (event, phase) in windowEvents {
            NotificationCenter.default.publisher(for: event)
                .receive(on: RunLoop.main)
                .sink { [weak self] note in
                    self?.handleWindowFullScreenEvent(note, phase: phase)
                }
                .store(in: &cancellables)
        }
    }

    // MARK: - Split View Updates

    override func viewDidLayout() {
        super.viewDidLayout()
        syncSidebarLayoutState()
        if pendingDividerRestoreX != nil {
            applyPendingDividerRestoreIfNeeded(reason: "view_did_layout")
        }
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
        if pendingDividerRestoreX != nil {
            applyPendingDividerRestoreIfNeeded(reason: "split_resized")
        }
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

        debugLog("effectiveRect idx=\(dividerIndex) drawn=\(formatRect(drawnRect)) proposed=\(formatRect(proposedEffectiveRect)) return=\(formatRect(effective))")
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

    // MARK: - FullScreen Transition

    private func handleWindowFullScreenEvent(_ notification: Notification, phase: String) {
        guard let currentWindow = view.window,
              let eventWindow = notification.object as? NSWindow,
              eventWindow === currentWindow else { return }

        let sidebarW = sidebarViewController?.view.frame.width ?? -1
        let dividerX = currentDividerRect(in: splitView, dividerIndex: 0).midX
        debugLog(
            "fullscreen=\(phase) event collapsed=\(sidebarItem.isCollapsed ? 1 : 0) " +
            "sidebarW=\(String(format: "%.1f", sidebarW)) dividerX=\(String(format: "%.1f", dividerX)) " +
            "windowW=\(String(format: "%.1f", view.bounds.width)) split=\(formatRect(splitView.bounds))"
        )

        switch phase {
        case "will_enter", "will_exit":
            captureDividerRestoreTarget(reason: phase)
        case "did_enter", "did_exit":
            applyPendingDividerRestoreIfNeeded(reason: phase)
            DispatchQueue.main.async { [weak self] in
                self?.applyPendingDividerRestoreIfNeeded(reason: "\(phase)_next")
            }
        default:
            break
        }
    }

    private func captureDividerRestoreTarget(reason: String) {
        guard !sidebarItem.isCollapsed else {
            pendingDividerRestoreX = nil
            pendingDividerRestoreAttempts = 0
            debugLog("fullscreen=\(reason) capture skip collapsed=1")
            return
        }

        let dividerX = currentDividerRect(in: splitView, dividerIndex: 0).midX
        guard dividerX.isFinite else {
            pendingDividerRestoreX = nil
            pendingDividerRestoreAttempts = 0
            debugLog("fullscreen=\(reason) capture skip invalid_divider")
            return
        }

        pendingDividerRestoreX = dividerX
        pendingDividerRestoreAttempts = 0
        let sidebarW = sidebarViewController?.view.frame.width ?? -1
        debugLog(
            "fullscreen=\(reason) capture dividerX=\(String(format: "%.1f", dividerX)) " +
            "sidebarW=\(String(format: "%.1f", sidebarW))"
        )
    }

    private func applyPendingDividerRestoreIfNeeded(reason: String) {
        guard let target = pendingDividerRestoreX else { return }
        guard !sidebarItem.isCollapsed else {
            debugLog("fullscreen=\(reason) restore skip collapsed=1")
            pendingDividerRestoreX = nil
            pendingDividerRestoreAttempts = 0
            return
        }

        let minPos = splitView.minPossiblePositionOfDivider(at: 0)
        let maxPos = splitView.maxPossiblePositionOfDivider(at: 0)
        let clamped = min(max(target, minPos), maxPos)
        let current = currentDividerRect(in: splitView, dividerIndex: 0).midX

        if abs(current - clamped) <= dividerRestoreTolerance {
            debugLog(
                "fullscreen=\(reason) restore done current=\(String(format: "%.1f", current)) " +
                "target=\(String(format: "%.1f", clamped)) attempts=\(pendingDividerRestoreAttempts)"
            )
            pendingDividerRestoreX = nil
            pendingDividerRestoreAttempts = 0
            return
        }

        NSAnimationContext.beginGrouping()
        NSAnimationContext.current.duration = 0
        NSAnimationContext.current.allowsImplicitAnimation = false
        splitView.setPosition(clamped, ofDividerAt: 0)
        NSAnimationContext.endGrouping()
        syncSidebarLayoutState()

        pendingDividerRestoreAttempts += 1
        debugLog(
            "fullscreen=\(reason) restore set current=\(String(format: "%.1f", current)) " +
            "target=\(String(format: "%.1f", clamped)) attempt=\(pendingDividerRestoreAttempts) " +
            "min=\(String(format: "%.1f", minPos)) max=\(String(format: "%.1f", maxPos))"
        )

        guard pendingDividerRestoreAttempts < dividerRestoreMaxAttempts else {
            debugLog("fullscreen=\(reason) restore giveup")
            pendingDividerRestoreX = nil
            pendingDividerRestoreAttempts = 0
            return
        }
    }

    // MARK: - Divider Debug Logs

    private func setupDividerDebugMonitorsIfNeeded() {
        guard dividerDebugEventMonitors.isEmpty else { return }

        try? FileManager.default.removeItem(atPath: dividerDebugLogPath)
        FileManager.default.createFile(atPath: dividerDebugLogPath, contents: nil)
        view.window?.acceptsMouseMovedEvents = true
        debugLog("=== divider debug start ===")

        let eventTypes: [NSEvent.EventTypeMask] = [.mouseMoved, .leftMouseDown, .leftMouseDragged, .leftMouseUp]
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

        let eventPointInSplit = splitView.convert(event.locationInWindow, from: nil)
        switch event.type {
        case .leftMouseDown:
            let downPoint = resolveDownPointInSplit(window: window, eventPoint: eventPointInSplit)
            if dividerMonitorDragActive {
                debugLog("monitor=down_ignored reason=already_active split=\(formatPoint(downPoint.point))")
                return true
            }
            guard shouldArmMonitorDrag(at: downPoint.point) else {
                debugLog(
                    "monitor=down_reject chosen=\(formatPoint(downPoint.point)) source=\(downPoint.source) " +
                    "outside=\(formatPoint(downPoint.outside)) screen=\(formatPoint(downPoint.screen)) event=\(formatPoint(eventPointInSplit)) " +
                    "divider=\(formatRect(currentDividerRect(in: splitView, dividerIndex: 0)))"
                )
                return false
            }
            dividerMonitorDragActive = true
            dividerMonitorStartMouseX = downPoint.point.x
            if sidebarItem.isCollapsed {
                dividerMonitorStartDividerX = max(splitView.minPossiblePositionOfDivider(at: 0), splitView.bounds.minX)
            } else {
                dividerMonitorStartDividerX = currentDividerRect(in: splitView, dividerIndex: 0).midX
            }
            dividerMonitorHasDragged = false
            debugLog(
                "monitor=monitor_down_local action=arm split=\(formatPoint(downPoint.point)) source=\(downPoint.source) " +
                "outside=\(formatPoint(downPoint.outside)) screen=\(formatPoint(downPoint.screen)) event=\(formatPoint(eventPointInSplit)) " +
                "collapsed=\(sidebarItem.isCollapsed ? 1 : 0) " +
                "startMouseX=\(String(format: "%.1f", dividerMonitorStartMouseX)) " +
                "startDividerX=\(String(format: "%.1f", dividerMonitorStartDividerX))"
            )
            return true
        case .leftMouseDragged:
            guard dividerMonitorDragActive else { return false }
            let globalPointInSplit = splitPointFromGlobalMouse(in: window)
            handleMonitorDividerDrag(at: globalPointInSplit, phase: "monitor_dragged_local")
            return true
        case .leftMouseUp:
            guard dividerMonitorDragActive else { return false }
            let globalPointInSplit = splitPointFromGlobalMouse(in: window)
            if (NSEvent.pressedMouseButtons & 1) != 0 {
                debugLog("monitor=monitor_up_local action=ignore_pressed split=\(formatPoint(globalPointInSplit))")
                return true
            }
            if dividerMonitorHasDragged {
                handleMonitorDividerDrag(at: globalPointInSplit, phase: "monitor_up_local")
            } else {
                debugLog("monitor=monitor_up_local action=no_drag split=\(formatPoint(globalPointInSplit))")
            }
            dividerMonitorDragActive = false
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

    private func resolveDownPointInSplit(window: NSWindow, eventPoint: CGPoint) -> (point: CGPoint, source: String, outside: CGPoint, screen: CGPoint) {
        let outsidePoint = splitPointFromGlobalMouse(in: window)
        let screenPoint = splitPointFromScreenMouse(in: window)
        let dividerX = currentDividerRect(in: splitView, dividerIndex: 0).midX

        let candidates: [(CGPoint, String)] = [
            (outsidePoint, "outside"),
            (screenPoint, "screen"),
            (eventPoint, "event")
        ]
        let chosen = candidates.min { lhs, rhs in
            abs(lhs.0.x - dividerX) < abs(rhs.0.x - dividerX)
        } ?? (outsidePoint, "outside")
        return (chosen.0, chosen.1, outsidePoint, screenPoint)
    }

    private func shouldArmMonitorDrag(at pointInSplit: CGPoint) -> Bool {
        guard splitView.isVertical else { return false }

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
        splitView.layoutSubtreeIfNeeded()
        syncSidebarLayoutState()
        let appliedX = currentDividerRect(in: splitView, dividerIndex: 0).midX
        if abs(appliedX - clamped) > 1 {
            debugLog(
                "monitor=\(phase) action=applied_mismatch " +
                "clamped=\(String(format: "%.1f", clamped)) applied=\(String(format: "%.1f", appliedX)) " +
                "arranged=\(describeViews(splitView.arrangedSubviews)) " +
                "subviews=\(describeViews(splitView.subviews))"
            )
        }
        debugLog(
            "monitor=\(phase) action=setPosition split=\(formatPoint(pointInSplit)) delta=\(String(format: "%.1f", delta)) proposed=\(String(format: "%.1f", proposed)) " +
            "clamped=\(String(format: "%.1f", clamped)) applied=\(String(format: "%.1f", appliedX)) " +
            "min=\(String(format: "%.1f", minPos)) max=\(String(format: "%.1f", maxPos))"
        )
    }

    private func logDividerMouseEvent(_ event: NSEvent) {
        guard let window = view.window,
              event.window === window else { return }

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
        if sidebarItem?.isCollapsed == true {
            let logicalWidth = max(splitView.dividerThickness, 1)
            let x = max(splitView.minPossiblePositionOfDivider(at: dividerIndex), splitView.bounds.minX)
            return NSRect(
                x: x,
                y: splitView.bounds.minY,
                width: logicalWidth,
                height: splitView.bounds.height
            )
        }

        // Prefer live geometry from split subviews; AppKit may not refresh
        // effectiveRect callbacks at drag frequency, so cache can go stale.
        if let live = liveDividerRect(in: splitView, dividerIndex: dividerIndex) {
            lastKnownDividerRects[dividerIndex] = live
            return live
        }

        if let cached = lastKnownDividerRects[dividerIndex], cached.width > 0, cached.height > 0 {
            return cached
        }

        return .zero
    }

    private func liveDividerRect(in splitView: NSSplitView, dividerIndex: Int) -> NSRect? {
        guard splitView.isVertical else {
            return nil
        }

        // arrangedSubviews only contains pane views (excludes divider decoration views),
        // so it is the most reliable source for live divider geometry.
        let panes: [NSView]
        if splitView.arrangedSubviews.count >= 2 {
            panes = splitView.arrangedSubviews
        } else {
            panes = splitView.subviews
        }
        guard panes.count >= 2 else { return nil }

        // Order by x to obtain the visual left pane for divider 0.
        let ordered = panes.sorted { $0.frame.minX < $1.frame.minX }
        guard ordered.count > dividerIndex else { return nil }
        let leftSubview = ordered[dividerIndex]
        let x = leftSubview.frame.maxX
        return NSRect(
            x: x,
            y: splitView.bounds.minY,
            width: max(splitView.dividerThickness, 1),
            height: splitView.bounds.height
        )
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

    private func describeViews(_ views: [NSView]) -> String {
        guard !views.isEmpty else { return "[]" }
        let maxCount = min(views.count, 4)
        let desc = views.prefix(maxCount).enumerated().map { idx, v in
            "#\(idx):\(typeName(v))\(formatRect(v.frame))"
        }
        let suffix = views.count > maxCount ? ",..." : ""
        return "[\(desc.joined(separator: ","))\(suffix)]"
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
