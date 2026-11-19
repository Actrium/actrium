import ActriumIPC
import AppKit
import QuartzCore
import SwiftUI

/// SwiftUI bridge for a remotely hosted CoreAnimation layer produced by an
/// ActriumRenderer process.
struct PersistentRendererProcessView: NSViewRepresentable {
    let tabId: UUID
    let fingerprint: String
    let coordinator: ViewBridgeCoordinator
    let size: CGSize
    let allowsHitTesting: Bool
    let config: [String: Any]

    func makeNSView(context: Context) -> RendererSurfaceHostView {
        let view = RendererSurfaceHostView(frame: NSRect(origin: .zero, size: size))
        view.tabId = tabId
        view.fingerprint = fingerprint
        view.allowsInput = allowsHitTesting
        view.coordinator = coordinator
        coordinator.rendererHostView = view
        coordinator.hasConnection = true
        view.apply(size: size, isOverviewMode: coordinator.isOverviewMode)
        view.apply(config: config)
        return view
    }

    func updateNSView(_ view: RendererSurfaceHostView, context: Context) {
        view.tabId = tabId
        view.fingerprint = fingerprint
        view.coordinator = coordinator
        view.allowsInput = allowsHitTesting
        coordinator.rendererHostView = view
        coordinator.currentViewportSize = size
        view.apply(size: size, isOverviewMode: coordinator.isOverviewMode)
        view.apply(config: config)
    }

    static func dismantleNSView(
        _ view: RendererSurfaceHostView,
        coordinator: ()
    ) {
        // The coordinator owns the Renderer lifecycle. SwiftUI may dismantle the
        // representable during transitions, so do not release the Tab here.
        view.prepareForReuse()
    }
}

final class RendererSurfaceHostView: NSView {
    weak var coordinator: ViewBridgeCoordinator?
    var tabId: UUID?
    var fingerprint = ""
    var allowsInput = true {
        didSet {
            guard allowsInput != oldValue else { return }
            if allowsInput {
                scheduleKeyboardFocusRestore()
            } else if let window, window.firstResponder === self {
                window.makeFirstResponder(nil)
            }
        }
    }

    private var hostLayer: CALayer?
    private var restoresKeyboardFocus = false
    private var contextId: UInt32 = 0
    private var trackingAreaReference: NSTrackingArea?
    private var surfaceObserver: NSObjectProtocol?
    private var crashObserver: NSObjectProtocol?
    private var configureTask: Task<Void, Never>?
    private var lastConfig: [String: Any]?
    private var lastConfigData: Data?
    private var lastConfiguredFingerprint: String?
    private var lastSentSize: CGSize?
    private var recoveryAttempt = 0
    private var accessibilityObserver: NSObjectProtocol?
    private var caretObserver: NSObjectProtocol?
    private var focusObserver: NSObjectProtocol?
    /// True after real user input (mouse/key) reached this surface. Host-side
    /// IME composition only engages for user-driven focus: a programmatic
    /// autoFocus at launch must not steal first responder from Host UI.
    private var hasUserInteraction = false {
        didSet {
            guard hasUserInteraction, !oldValue, tabId != nil else { return }
            // The Renderer dedupes focus reports. A field focused
            // programmatically before this gate opened already reported
            // (and the Host dropped) state it will never repeat, so ask for
            // a forced re-report; the click/key that opened the gate can then
            // activate IME on that field.
            sendIMEPayload(name: "com.actrium.textInputFocusReset", payload: [:])
        }
    }
    /// Last remote caret rect in view coordinates; anchors the IME candidate
    /// window through `caretScreenRect()`.
    private var lastCaretRect: CGRect?
    /// Last text-input focus state reported by the Renderer. The Renderer
    /// dedupes identical reports, so a click that re-activates an already
    /// focused input produces no new message; this latch lets the IME
    /// client restore itself in that case (see `resignFirstResponder`).
    private var lastRendererTextFocus = false
    private var accessibilityElements: [RemoteAccessibilityElement] = []
    private var accessibilityById: [UInt64: RemoteAccessibilityElement] = [:]
    private var accessibilityTree = Actrium_Ipc_AccessibilityTree()

    private static let layerHostClass: AnyClass? = NSClassFromString("CALayerHost")

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setup()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        setup()
    }

    deinit {
        configureTask?.cancel()
        if let surfaceObserver {
            NotificationCenter.default.removeObserver(surfaceObserver)
        }
        if let crashObserver {
            NotificationCenter.default.removeObserver(crashObserver)
        }
        if let accessibilityObserver {
            NotificationCenter.default.removeObserver(accessibilityObserver)
        }
        if let caretObserver {
            NotificationCenter.default.removeObserver(caretObserver)
        }
        if let focusObserver {
            NotificationCenter.default.removeObserver(focusObserver)
        }
    }

    private func setup() {
        wantsLayer = true
        layer?.backgroundColor = AppColors.workspaceBackground.cgColor
        layer?.masksToBounds = true
        caretObserver = NotificationCenter.default.addObserver(
            forName: .rendererCaretUpdated, object: nil, queue: .main
        ) { [weak self] notification in
            guard let self, let tabId = self.tabId, notification.userInfo?["tabId"] as? UUID == tabId else { return }
            guard let rect = notification.userInfo?["rect"] as? CGRect else {
                self.lastCaretRect = nil
                return
            }
            self.lastCaretRect = rect
        }

        focusObserver = NotificationCenter.default.addObserver(
            forName: .rendererTextInputFocusChanged,
            object: nil,
            queue: .main
        ) { [weak self] notification in
            guard let self,
                  let tabId = self.tabId,
                  notification.userInfo?["tabId"] as? UUID == tabId,
                  let focused = notification.userInfo?["focused"] as? Bool
            else { return }
            if focused {
                self.lastRendererTextFocus = true
                // Re-activate after focus churn: SwiftUI may reclaim first
                // responder once when the click event settles, which resigns
                // the IME client and blurs the remote input; a later
                // focus=true must be able to take it back. The user-interaction
                // gate only excludes the pre-interaction startup autoFocus.
                if self.hasUserInteraction {
                    self.activateIMEComposition()
                }
            } else {
                self.lastRendererTextFocus = false
                if self.window?.firstResponder === self.imeCompositionView {
                    self.window?.makeFirstResponder(self)
                }
            }
        }

        surfaceObserver = NotificationCenter.default.addObserver(
            forName: .rendererSurfaceUpdated,
            object: nil,
            queue: .main
        ) { [weak self] notification in
            guard let self,
                  let tabId = notification.userInfo?["tabId"] as? UUID,
                  tabId == self.tabId,
                  let contextId = notification.userInfo?["contextId"] as? UInt32 else {
                return
            }
            self.installContext(contextId)
        }

        accessibilityObserver = NotificationCenter.default.addObserver(
            forName: .rendererAccessibilityUpdated,
            object: nil,
            queue: .main
        ) { [weak self] notification in
            guard let self,
                  let tabId = notification.userInfo?["tabId"] as? UUID,
                  tabId == self.tabId,
                  let tree = notification.userInfo?["tree"] as? Actrium_Ipc_AccessibilityTree else {
                return
            }
            self.applyAccessibilityTree(tree)
        }

        crashObserver = NotificationCenter.default.addObserver(
            forName: .rendererProcessCrashed,
            object: nil,
            queue: .main
        ) { [weak self] notification in
            guard let self,
                  let tabId = notification.userInfo?["tabId"] as? UUID,
                  tabId == self.tabId else {
                return
            }
            self.showPlaceholder()
            self.coordinator?.hasConnection = false
            self.coordinator?.xpcServiceReady = false
            self.coordinator?.allocatedServiceId = nil
            // The replacement Renderer starts with no focused text input and
            // dedupes its (false) initial state, so it will never correct a
            // stale latch; a leftover true would let the next surface click
            // pull first responder back to the IME client and swallow keys.
            self.lastRendererTextFocus = false
            self.recoverRendererAfterCrash()
        }
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        layer?.backgroundColor = AppColors.workspaceBackground.cgColor
    }

    func apply(config: [String: Any]) {
        guard let tabId,
              !fingerprint.isEmpty,
              JSONSerialization.isValidJSONObject(config),
              let data = try? JSONSerialization.data(
                  withJSONObject: config,
                  options: [.sortedKeys]
              ) else {
            return
        }

        let requestedFingerprint = fingerprint
        guard data != lastConfigData
                || requestedFingerprint != lastConfiguredFingerprint else {
            return
        }

        lastConfig = config
        lastConfigData = data
        lastConfiguredFingerprint = requestedFingerprint
        configureTask?.cancel()
        configureTask = Task { @MainActor [weak self] in
            guard let self else { return }
            do {
                // A Tab can navigate from a built-in page to an ordinary app
                // without SwiftUI recreating this NSView. Always reconcile the
                // allocation before loading so an identity change cannot retain
                // the previous process.
                let previousRendererId = self.coordinator?.allocatedServiceId
                let rendererId = try RendererProcessPool.shared.allocateRenderer(
                    for: tabId,
                    fingerprint: requestedFingerprint
                )
                guard !Task.isCancelled,
                      self.tabId == tabId,
                      self.fingerprint == requestedFingerprint else {
                    return
                }

                if previousRendererId != nil, previousRendererId != rendererId {
                    self.showPlaceholder()
                    self.coordinator?.xpcServiceReady = false
                    self.coordinator?.hasConnection = false
                }
                self.coordinator?.allocatedServiceId = rendererId

                let contextId = try await RendererProcessPool.shared.prepareTab(
                    tabId: tabId,
                    config: config
                )
                guard !Task.isCancelled,
                      self.tabId == tabId,
                      self.fingerprint == requestedFingerprint else {
                    return
                }
                self.installContext(contextId)
                RendererProcessPool.shared.requestAccessibilityTree(for: tabId)
                self.sendViewport(size: self.bounds.size)
                self.recoveryAttempt = 0
                self.coordinator?.xpcServiceReady = true
                self.coordinator?.hasConnection = true
                // Do not call onConnectionEstablished here. This runs after
                // every successful config apply, and its persistent closure
                // captures the tab from the first SwiftUI render, so it would
                // resend stale unifiedScript/reloadKey over the new config.
            } catch is CancellationError {
                return
            } catch {
                NSLog(
                    "Renderer configuration failed for %@: %@",
                    tabId.uuidString,
                    error.localizedDescription
                )
                if self.fingerprint == requestedFingerprint {
                    self.lastConfigData = nil
                    self.lastConfiguredFingerprint = nil
                }
                self.coordinator?.xpcServiceReady = false
            }
        }
    }

    func apply(size: CGSize, isOverviewMode: Bool) {
        guard size.width > 0, size.height > 0 else { return }
        setFrameSize(size)
        hostLayer?.frame = bounds
        guard size != lastSentSize, tabId != nil else { return }
        lastSentSize = size
        sendViewport(size: size)
    }

    override func prepareForReuse() {
        allowsInput = false
        super.prepareForReuse()
        configureTask?.cancel()
        configureTask = nil
        // Drop per-tab interaction/IME state so the recycled surface cannot
        // leak it into the next tab (the gate exists to stop programmatic
        // autoFocus from stealing first responder).
        hasUserInteraction = false
        lastCaretRect = nil
        lastRendererTextFocus = false
        if window?.firstResponder === imeCompositionView {
            window?.makeFirstResponder(nil)
        }
        imeCompositionView?.removeFromSuperview()
        imeCompositionView = nil
    }

    private func sendViewport(size: CGSize) {
        guard let tabId, size.width > 0, size.height > 0 else { return }
        RendererProcessPool.shared.updateViewport(
            tabId: tabId,
            size: size,
            isOverviewMode: coordinator?.isOverviewMode ?? false
        )
    }

    func captureSnapshot() -> NSImage? {
        guard bounds.width > 0, bounds.height > 0,
              let bitmap = bitmapImageRepForCachingDisplay(in: bounds) else {
            return nil
        }
        cacheDisplay(in: bounds, to: bitmap)
        let image = NSImage(size: bounds.size)
        image.addRepresentation(bitmap)
        return image
    }

    private func installContext(_ newContextId: UInt32) {
        guard newContextId != 0, newContextId != contextId else { return }
        guard let layerHostClass = Self.layerHostClass,
              let newLayer = layerHostClass.alloc() as? CALayer else {
            NSLog("CALayerHost is unavailable")
            return
        }

        hostLayer?.removeFromSuperlayer()
        _ = newLayer.perform(NSSelectorFromString("init"))
        newLayer.setValue(NSNumber(value: newContextId), forKey: "contextId")
        newLayer.anchorPoint = .zero
        newLayer.frame = bounds
        newLayer.contentsScale = window?.backingScaleFactor
            ?? NSScreen.main?.backingScaleFactor
            ?? 2
        newLayer.masksToBounds = true
        newLayer.actions = [
            "bounds": NSNull(),
            "position": NSNull(),
            "contents": NSNull(),
            "sublayers": NSNull(),
            "transform": NSNull()
        ]
        layer?.addSublayer(newLayer)
        hostLayer = newLayer
        contextId = newContextId
        // The host may have gained focus before the new surface became ready.
        syncKeyboardFocus()
        if let tabId {
            NSLog(
                "Renderer surface installed: tab=%@ context=%u",
                tabId.uuidString,
                newContextId
            )
        }
    }

    private func showPlaceholder() {
        contextId = 0
        hostLayer?.removeFromSuperlayer()
        hostLayer = nil
    }

    private func recoverRendererAfterCrash() {
        guard recoveryAttempt < 3,
              let tabId,
              !fingerprint.isEmpty,
              let config = lastConfig else {
            return
        }

        recoveryAttempt += 1
        let attempt = recoveryAttempt
        configureTask?.cancel()
        configureTask = Task { @MainActor [weak self] in
            guard let self else { return }
            if attempt > 1 {
                try? await Task.sleep(for: .milliseconds(200 * attempt))
            }
            guard !Task.isCancelled, self.tabId == tabId else { return }

            do {
                let rendererId = try RendererProcessPool.shared.allocateRenderer(
                    for: tabId,
                    fingerprint: self.fingerprint
                )
                self.coordinator?.allocatedServiceId = rendererId
                self.lastConfigData = nil
                self.apply(config: config)
                NSLog(
                    "Renderer recovery scheduled: tab=%@ attempt=%d renderer=%@",
                    tabId.uuidString,
                    attempt,
                    rendererId
                )
            } catch {
                NSLog(
                    "Renderer recovery failed: tab=%@ attempt=%d error=%@",
                    tabId.uuidString,
                    attempt,
                    error.localizedDescription
                )
                self.recoverRendererAfterCrash()
            }
        }
    }

    override func layout() {
        super.layout()
        hostLayer?.frame = bounds
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let trackingAreaReference {
            removeTrackingArea(trackingAreaReference)
        }
        let area = NSTrackingArea(
            rect: bounds,
            options: [
                .mouseEnteredAndExited,
                .mouseMoved,
                .activeInKeyWindow,
                .inVisibleRect
            ],
            owner: self,
            userInfo: nil
        )
        addTrackingArea(area)
        trackingAreaReference = area
    }

    override var acceptsFirstResponder: Bool { allowsInput }

    override func becomeFirstResponder() -> Bool {
        guard allowsInput, super.becomeFirstResponder() else { return false }
        restoresKeyboardFocus = true
        sendKeyboardFocus(window?.isKeyWindow == true)
        return true
    }

    override func resignFirstResponder() -> Bool {
        guard super.resignFirstResponder() else { return false }
        // The next first responder is only known once AppKit finishes the
        // handoff. Resigning in favour of the internal IMECompositionView is
        // an internal transfer and must NOT drop keyboard focus; resync on
        // the next main-queue turn so syncKeyboardFocus can inspect the
        // actual successor (IME view keeps focus, address bar drops it).
        DispatchQueue.main.async { [weak self] in
            self?.syncKeyboardFocus()
        }
        return true
    }

    override func viewWillMove(toWindow newWindow: NSWindow?) {
        if let window, window !== newWindow {
            if window.firstResponder === self { sendKeyboardFocus(false) }
            let center = NotificationCenter.default
            center.removeObserver(self, name: NSWindow.didBecomeKeyNotification, object: window)
            center.removeObserver(self, name: NSWindow.didResignKeyNotification, object: window)
        }
        super.viewWillMove(toWindow: newWindow)
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        guard let window else { return }
        let center = NotificationCenter.default
        for name in [NSWindow.didBecomeKeyNotification, NSWindow.didResignKeyNotification] {
            center.addObserver(self, selector: #selector(windowKeyStatusChanged(_:)), name: name, object: window)
        }
        syncKeyboardFocus()
        scheduleKeyboardFocusRestore()
    }

    @objc private func windowKeyStatusChanged(_ notification: Notification) {
        guard window?.firstResponder === self else { return }
        syncKeyboardFocus()
    }

    private func syncKeyboardFocus() {
        let responder = window?.firstResponder
        let ownsInput = responder === self || (responder as? IMECompositionView)?.surfaceView === self
        sendKeyboardFocus(allowsInput && window?.isKeyWindow == true && ownsInput)
    }

    private func sendKeyboardFocus(_ focused: Bool) {
        guard let tabId else { return }
        RendererProcessPool.shared.setKeyboardFocus(focused, for: tabId)
    }

    private func scheduleKeyboardFocusRestore() {
        guard restoresKeyboardFocus else { return }
        // SwiftUI can update the incoming Tab before it disables the outgoing
        // Tab. Wait for both updates and recheck eligibility before claiming focus.
        DispatchQueue.main.async { [weak self] in
            self?.restoreKeyboardFocus()
        }
    }

    private func restoreKeyboardFocus() {
        guard allowsInput, let window, !isHiddenOrHasHiddenAncestor else { return }
        let responder = window.firstResponder
        // An ancestor can be the default responder after AppKit removes the
        // old surface. Leave explicit responders such as the address editor alone.
        let ancestorHasFocus = (responder as? NSView).map { isDescendant(of: $0) } ?? false
        guard responder == nil || responder === window || ancestorHasFocus else { return }
        window.makeFirstResponder(self)
    }

    override var isFlipped: Bool { true }

    override func isAccessibilityElement() -> Bool { true }

    override func accessibilityRole() -> NSAccessibility.Role { .group }

    override func accessibilityLabel() -> String? {
        let title = coordinator?.tabConfig?.title ?? ""
        return title.isEmpty ? "Application content" : title
    }

    override func accessibilityChildren() -> [Any]? {
        accessibilityElements
    }

    override func accessibilityHitTest(_ point: NSPoint) -> Any? {
        guard let window else { return self }
        let inWindow = window.convertFromScreen(NSRect(origin: point, size: .zero)).origin
        let local = convert(inWindow, from: nil)
        if let nodeId = RemoteAccessibilitySnapshot.hitTest(accessibilityTree, point: local),
           let element = accessibilityById[nodeId] {
            return element
        }
        return self
    }

    override var accessibilityFocusedUIElement: Any? {
        if accessibilityTree.focusedID != 0 {
            return accessibilityById[accessibilityTree.focusedID]
        }
        return self
    }

    func performRemoteAccessibilityAction(_ action: String, nodeId: UInt64) -> Bool {
        guard let tabId else { return false }
        return RendererProcessPool.shared.performAccessibilityAction(
            tabId: tabId,
            nodeId: nodeId,
            action: action
        )
    }

    private func applyAccessibilityTree(_ tree: Actrium_Ipc_AccessibilityTree) {
        accessibilityTree = tree
        var byId: [UInt64: RemoteAccessibilityElement] = [:]
        for node in tree.nodes {
            let element = RemoteAccessibilityElement(node: node, host: self)
            byId[node.nodeID] = element
        }
        for node in tree.nodes {
            guard let element = byId[node.nodeID] else { continue }
            element.setChildren(node.childIds.compactMap { byId[$0] })
        }
        let roots = tree.nodes.filter { $0.parentID == 0 }
        let hostChildren: [RemoteAccessibilityElement]
        if roots.count == 1, let root = roots.first, !root.childIds.isEmpty {
            hostChildren = root.childIds.compactMap { byId[$0] }
        } else {
            hostChildren = roots.compactMap { byId[$0.nodeID] }
        }
        for element in hostChildren {
            element.setAccessibilityParent(self)
        }
        accessibilityElements = hostChildren
        accessibilityById = byId
        NSAccessibility.post(element: self, notification: .layoutChanged)
        if tree.focusedID != 0, let focused = byId[tree.focusedID] {
            NSAccessibility.post(element: focused, notification: .focusedUIElementChanged)
        }
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        guard allowsInput, super.hitTest(point) != nil else { return nil }
        return self
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
        allowsInput
    }

    override func mouseDown(with event: NSEvent) {
        hasUserInteraction = true
        let focused = window?.makeFirstResponder(self) == true
        #if DEBUG
        if let tabId {
            let point = convert(event.locationInWindow, from: nil)
            NSLog("Renderer mouse down: tab=%@ x=%.0f y=%.0f focused=%d",
                  tabId.uuidString, point.x, point.y, focused ? 1 : 0)
        }
        #endif
        forwardMouse(event, type: .mouseDown)
    }

    override func mouseUp(with event: NSEvent) {
        forwardMouse(event, type: .mouseUp)
    }

    override func rightMouseDown(with event: NSEvent) {
        forwardMouse(event, type: .mouseDown, button: 1)
    }

    override func rightMouseUp(with event: NSEvent) {
        forwardMouse(event, type: .mouseUp, button: 1)
    }

    override func otherMouseDown(with event: NSEvent) {
        forwardMouse(event, type: .mouseDown, button: 2)
    }

    override func otherMouseUp(with event: NSEvent) {
        forwardMouse(event, type: .mouseUp, button: 2)
    }

    override func mouseMoved(with event: NSEvent) {
        forwardMouse(event, type: .mouseMove)
    }

    override func mouseDragged(with event: NSEvent) {
        forwardMouse(event, type: .mouseMove)
    }

    override func rightMouseDragged(with event: NSEvent) {
        forwardMouse(event, type: .mouseMove, button: 1)
    }

    override func otherMouseDragged(with event: NSEvent) {
        forwardMouse(event, type: .mouseMove, button: 2)
    }

    override func mouseEntered(with event: NSEvent) {
        forwardMouse(event, type: .mouseEntered)
    }

    override func mouseExited(with event: NSEvent) {
        forwardMouse(event, type: .mouseExited)
    }

    override func scrollWheel(with event: NSEvent) {
        guard let tabId else { return }
        let point = convert(event.locationInWindow, from: nil)
        var input = Actrium_Ipc_InputEvent()
        input.type = .scroll
        input.x = point.x
        input.y = point.y
        input.scrollDeltaX = event.scrollingDeltaX
        input.scrollDeltaY = event.scrollingDeltaY
        input.modifiers = encodedModifiers(event.modifierFlags)
        RendererProcessPool.shared.sendInput(input, to: tabId)
    }

    override func keyDown(with event: NSEvent) {
        hasUserInteraction = true
        forwardKey(event, type: .keyDown)
    }

    override func keyUp(with event: NSEvent) {
        forwardKey(event, type: .keyUp)
    }

    private func forwardMouse(
        _ event: NSEvent,
        type: Actrium_Ipc_InputEvent.EventType,
        button: UInt32 = 0
    ) {
        guard allowsInput, let tabId else { return }
        let point = convert(event.locationInWindow, from: nil)
        var input = Actrium_Ipc_InputEvent()
        input.type = type
        input.x = point.x
        input.y = point.y
        input.button = button
        input.modifiers = encodedModifiers(event.modifierFlags)
        RendererProcessPool.shared.sendInput(input, to: tabId)
    }

    /// fileprivate: IMECompositionView forwards its own key events here once it
    /// owns first responder.
    fileprivate func forwardKey(
        _ event: NSEvent,
        type: Actrium_Ipc_InputEvent.EventType
    ) {
        guard allowsInput, let window, window.isKeyWindow, let tabId,
              window.firstResponder === self
                  || window.firstResponder === imeCompositionView else {
            return
        }
        var input = Actrium_Ipc_InputEvent()
        input.type = type
        input.keyCode = UInt32(event.keyCode)
        input.characters = event.characters ?? ""
        input.modifiers = encodedModifiers(event.modifierFlags)
        RendererProcessPool.shared.sendInput(input, to: tabId)
    }

    // MARK: - Host-side IME composition
    //
    // TSM only engages in a regular, active application, so keystrokes for a
    // focused Realm text input are composed HERE: this view's invisible
    // NSTextInputClient sibling becomes first responder, the system IME runs
    // against it, and marked/committed text is mirrored to the Renderer.

    /// The invisible text client that hosts IME composition for this surface.
    /// Created lazily on first activation; optional so first-responder checks
    /// do not force its instantiation.
    private var imeCompositionView: IMECompositionView?

    private func makeIMECompositionView() -> IMECompositionView {
        let view = IMECompositionView()
        view.surfaceView = self
        // Non-zero frame: AppKit treats zero-frame views as non-participating
        // in some first-responder validation paths.
        view.frame = NSRect(x: 0, y: 0, width: 1, height: 1)
        view.alphaValue = 0
        window?.contentView?.addSubview(view)
        imeCompositionView = view
        return view
    }

    /// Makes the IME client first responder so real key events compose here.
    func activateIMEComposition() {
        guard window?.isKeyWindow == true else { return }
        let view = imeCompositionView ?? makeIMECompositionView()
        if view.superview == nil {
            window?.contentView?.addSubview(view)
        }
        window?.makeFirstResponder(view)
    }

    /// Screen rect of the remote caret; anchors the IME candidate window.
    func caretScreenRect() -> CGRect? {
        guard let rect = lastCaretRect, let window else { return nil }
        return window.convertToScreen(convert(rect, to: nil))
    }

    private func sendIMEPayload(name: String, payload: [String: Any]) {
        guard let tabId,
              JSONSerialization.isValidJSONObject(payload),
              let data = try? JSONSerialization.data(withJSONObject: payload),
              let json = String(data: data, encoding: .utf8)
        else { return }
        try? RendererProcessPool.shared.sendHostEvent(name: name, userInfoJSON: json, to: tabId)
    }

    func sendMarkedText(_ text: String, selectedRange: NSRange) {
        sendIMEPayload(
            name: "com.actrium.imeMarkedText",
            payload: ["text": text, "loc": selectedRange.location, "len": selectedRange.length]
        )
    }

    func sendInsertText(_ text: String) {
        // loc/len are required by the decoder (Swift's synthesized Codable
        // ignores property default values), so always send them.
        sendIMEPayload(name: "com.actrium.imeInsertText", payload: ["text": text, "loc": 0, "len": 0])
    }

    /// Forwards a non-text key (arrow, enter, backspace, ...) to the Renderer.
    /// `modifiers` restores flags that interpretKeyEvents discarded when it
    /// reduced the key event to a selector (e.g. Shift for selection-extending
    /// movement or Shift-Return); without them the Renderer rebuilds a plain
    /// caret move / unmodified Return.
    func sendCommandKey(keyCode: UInt32, modifiers: NSEvent.ModifierFlags = []) {
        guard let tabId else { return }
        // interpretKeyEvents on the render side derives editing commands from
        // the event's characters; an empty string produces no command.
        let characters: [UInt32: String] = [
            36: "\r",
            48: "\t",
            51: "\u{7F}",
            53: "\u{1B}",
            115: String(Unicode.Scalar(NSHomeFunctionKey)!),
            116: String(Unicode.Scalar(NSPageUpFunctionKey)!),
            117: String(Unicode.Scalar(NSDeleteFunctionKey)!),
            119: String(Unicode.Scalar(NSEndFunctionKey)!),
            121: String(Unicode.Scalar(NSPageDownFunctionKey)!),
            123: String(Unicode.Scalar(NSLeftArrowFunctionKey)!),
            124: String(Unicode.Scalar(NSRightArrowFunctionKey)!),
            125: String(Unicode.Scalar(NSDownArrowFunctionKey)!),
            126: String(Unicode.Scalar(NSUpArrowFunctionKey)!),
        ]
        var down = Actrium_Ipc_InputEvent()
        down.type = .keyDown
        down.keyCode = keyCode
        down.modifiers = encodedModifiers(modifiers)
        if let chars = characters[keyCode] {
            down.characters = chars
        }
        RendererProcessPool.shared.sendInput(down, to: tabId)
        var up = down
        up.type = .keyUp
        RendererProcessPool.shared.sendInput(up, to: tabId)
    }

    /// Forwards a standard editing action (copy/paste/select-all/undo/redo)
    /// to the Renderer's focused text input. The Host's menu bar captures the
    /// corresponding command-key equivalents before any key event reaches
    /// this view, so these actions arrive as selector dispatches on this
    /// (empty) text view instead of as key events; executing them against
    /// the local storage would do nothing.
    func sendTextCommand(_ command: String) {
        sendIMEPayload(name: "com.actrium.textCommand", payload: ["command": command])
    }

    /// The IME client lost first responder (user focused another Host
    /// control); ask the Renderer to blur its text input as well.
    func imeCompositionDidResign() {
        sendIMEPayload(name: "com.actrium.textInputBlurRequest", payload: [:])
    }

    /// A mouse click handed first responder from the IME client to this
    /// surface while the Renderer may still be focused on its text input.
    /// The Renderer dedupes identical focus reports, so it will not ask for
    /// the IME client back; restore it directly, otherwise TSM goes silent
    /// and IME stops until the window re-keys. If the click actually blurs
    /// the remote input, the pending focus=false report hands first
    /// responder back here through the focus observer.
    func imeCompositionDidHandoffToSurface() {
        guard hasUserInteraction, lastRendererTextFocus else { return }
        window?.makeFirstResponder(imeCompositionView)
    }

    private func encodedModifiers(_ flags: NSEvent.ModifierFlags) -> UInt32 {
        var value: UInt32 = 0
        if flags.contains(.shift) { value |= 1 }
        if flags.contains(.control) { value |= 2 }
        if flags.contains(.option) { value |= 4 }
        if flags.contains(.command) { value |= 8 }
        if flags.contains(.capsLock) { value |= 16 }
        return value
    }
}

/// Invisible NSTextInputClient that hosts system IME composition for a
/// remotely rendered surface. TSM only engages in a regular, active
/// application, so while a Realm text input holds focus in the Renderer,
/// this sibling view takes over first responder in the Host window: real key
/// events compose against the Host's input context (candidate window and
/// all), and the resulting marked/committed text is mirrored to the
/// Renderer, which applies it to the native RN text control there.
///
/// A plain NSView conforms to NSTextInputClient only through a category, and
/// AppKit does not establish a real IME session for it — keystrokes vanish.
/// Subclassing NSTextView (a first-class text client) makes TSM engage; all
/// mutations are intercepted and forwarded instead of applied locally.
final class IMECompositionView: NSTextView {
    weak var surfaceView: RendererSurfaceHostView?

    private var forwardingMarkedText: String = ""
    private var forwardingMarkedSelection: NSRange = NSRange(location: 0, length: 0)
    private var interpretingModifiers: NSEvent.ModifierFlags = []

    init() {
        super.init(frame: NSRect(x: 0, y: 0, width: 1, height: 1), textContainer: nil)
        isEditable = true
        isSelectable = true
        string = ""
        backgroundColor = .clear
        drawsBackground = false
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) is not supported")
    }

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect, textContainer: nil)
        isEditable = true
        isSelectable = true
        string = ""
        backgroundColor = .clear
        drawsBackground = false
    }

    override var acceptsFirstResponder: Bool { true }
    override var canBecomeKeyView: Bool { false }
    override func becomeFirstResponder() -> Bool { true }

    // MARK: Key delivery

    /// Command-key equivalents (copy/paste/select-all/undo/redo, ...) belong
    /// to the focused text input inside the Renderer. They are pure key
    /// equivalents: AppKit does not route them through interpretKeyEvents,
    /// so forwarding raw events leaves the Renderer unable to map them.
    /// Resolve the command here and forward it as a named action instead.
    /// Returning true stops the local NSTextView from consuming them against
    /// its empty storage. Unmapped command keys return false so they fall
    /// through to keyDown and are forwarded as raw events like any other
    /// modified key.
    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        // performKeyEquivalent reaches every view in the window hierarchy,
        // not just the first responder: an IME view left in a recycled or
        // inactive surface must not consume command keys meant elsewhere.
        guard window?.firstResponder === self,
              surfaceView?.allowsInput == true,
              event.modifierFlags.contains(.command),
              let command = Self.commandEquivalent(
                characters: event.characters,
                shift: event.modifierFlags.contains(.shift)
              ) else { return false }
        surfaceView?.sendTextCommand(command)
        return true
    }

    /// Maps a command-key event to a named editing action, or nil when it is
    /// not one of the equivalents this client forwards.
    static func commandEquivalent(characters: String?, shift: Bool) -> String? {
        switch (characters?.lowercased(), shift) {
        case ("a", false): return "selectAll"
        case ("c", false): return "copy"
        case ("x", false): return "cut"
        case ("v", false): return "paste"
        case ("z", false): return "undo"
        case ("z", true): return "redo"
        default: return nil
        }
    }

    override func keyDown(with event: NSEvent) {
        // Named command equivalents (copy/paste/select-all/undo/redo) travel
        // as actions. Modified FUNCTION keys (Shift/Cmd+arrows, Home/End, ...)
        // are forwarded raw so the Renderer reproduces the exact editing
        // command — the same path that handled them before Host-side IME
        // composition existed. Character keys (Shift+A, Caps Lock text,
        // Option+Delete, Shift+Return, ...) must stay on the
        // interpretKeyEvents path so the Host IME composes them.
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        if flags.contains(.command),
           let command = Self.commandEquivalent(
                characters: event.characters,
                shift: flags.contains(.shift)
           ) {
            surfaceView?.sendTextCommand(command)
            return
        }
        // Modified function keys forward raw only OUTSIDE composition. While
        // marked text is active the Input Method owns these keys (Backspace
        // erases pinyin, Cmd+Backspace cancels the composition, arrows move
        // in the candidate window): routing them to the Renderer edits the
        // remote marked text without TSM knowing, leaving the candidate
        // window stranded after the pinyin is gone.
        if !flags.isEmpty, Self.isFunctionKey(event), forwardingMarkedText.isEmpty {
            surfaceView?.forwardKey(event, type: .keyDown)
            surfaceView?.forwardKey(event, type: .keyUp)
            return
        }
        let previousModifiers = interpretingModifiers
        interpretingModifiers = flags
        defer { interpretingModifiers = previousModifiers }
        interpretKeyEvents([event])
    }

    /// True when the event's characters identify a navigation/editing
    /// function key (arrows, Home/End, PageUp/Down, F1-F12, and the
    /// backspace/delete character) rather than a text character.
    static func isFunctionKey(_ event: NSEvent) -> Bool {
        guard let value = event.characters?.unicodeScalars.first?.value else { return false }
        // 0x7F is the backspace/delete character (NSDeleteCharacter); it is
        // not in the NSFunctionKey range but is an editing key (e.g.
        // Option+Delete removes the previous word when forwarded raw).
        if value == UInt32(NSDeleteCharacter) { return true }
        return (Int(NSUpArrowFunctionKey)...Int(NSMenuFunctionKey)).contains(Int(value))
    }

    /// interpretKeyEvents turns unhandled keys into editing commands; map the
    /// common ones to raw key codes and forward them to the Renderer so RN
    /// keeps working (enter to send, arrows to move, backspace, ...).
    override func doCommand(by aSelector: Selector) {
        let keyCodes: [Selector: (keyCode: UInt32, modifiers: NSEvent.ModifierFlags)] = [
            #selector(NSResponder.deleteBackward(_:)): (51, []),
            #selector(NSResponder.deleteForward(_:)): (117, []),
            #selector(NSResponder.moveLeft(_:)): (123, []),
            #selector(NSResponder.moveRight(_:)): (124, []),
            #selector(NSResponder.moveUp(_:)): (126, []),
            #selector(NSResponder.moveDown(_:)): (125, []),
            #selector(NSResponder.moveLeftAndModifySelection(_:)): (123, .shift),
            #selector(NSResponder.moveRightAndModifySelection(_:)): (124, .shift),
            #selector(NSResponder.moveUpAndModifySelection(_:)): (126, .shift),
            #selector(NSResponder.moveDownAndModifySelection(_:)): (125, .shift),
            #selector(NSResponder.moveToBeginningOfLine(_:)): (115, []),
            #selector(NSResponder.moveToEndOfLine(_:)): (119, []),
            #selector(NSResponder.moveToBeginningOfDocument(_:)): (115, []),
            #selector(NSResponder.moveToEndOfDocument(_:)): (119, []),
            #selector(NSResponder.pageUp(_:)): (116, []),
            #selector(NSResponder.pageDown(_:)): (121, []),
            #selector(NSResponder.insertNewline(_:)): (36, []),
            #selector(NSResponder.insertTab(_:)): (48, []),
            #selector(NSResponder.insertBacktab(_:)): (48, []),
            #selector(NSResponder.cancelOperation(_:)): (53, []),
        ]
        if let command = keyCodes[aSelector] {
            // With no active composition, Esc means nothing here; forwarding
            // it blurs the remote input and flashes the surface. While
            // composing, TSM consumes Esc itself (it never reaches this
            // command), so this only guards the no-candidate case.
            if aSelector == #selector(NSResponder.cancelOperation(_:)),
               forwardingMarkedText.isEmpty {
                return
            }
            // The renderer distinguishes Return from Shift-Return using the
            // original modifiers, which the table cannot express.
            let modifiers = command.keyCode == 36
                ? interpretingModifiers
                : command.modifiers
            surfaceView?.sendCommandKey(keyCode: command.keyCode, modifiers: modifiers)
            return
        }
        super.doCommand(by: aSelector)
    }

    // MARK: Editing actions (menu key equivalents)

    // The Host menu bar owns Cmd+C/V/X/A/Z/Shift-Z key equivalents and
    // dispatches these selectors to the first responder (this view). Route
    // them to the real text input inside the Renderer.
    override func copy(_ sender: Any?) { surfaceView?.sendTextCommand("copy") }
    override func cut(_ sender: Any?) { surfaceView?.sendTextCommand("cut") }
    override func paste(_ sender: Any?) { surfaceView?.sendTextCommand("paste") }
    override func selectAll(_ sender: Any?) { surfaceView?.sendTextCommand("selectAll") }
    // `undo:`/`redo:` are not visible to Swift as overridable NSTextView API;
    // plain @objc methods with the same selectors still win at runtime.
    @objc func undo(_ sender: Any?) { surfaceView?.sendTextCommand("undo") }
    @objc func redo(_ sender: Any?) { surfaceView?.sendTextCommand("redo") }

    override func resignFirstResponder() -> Bool {
        // Defer the handoff until the successor is known. A click that
        // lands on our own surface view resigns this IME view in favour of
        // the surface itself. Two cases:
        //  - The remote text input stays focused (e.g. Shift+click after a
        //    drag selection): the Renderer sees no focus change and never
        //    re-reports, so nobody would give this IME view its first
        //    responder status back — restore it here, otherwise TSM goes
        //    silent and Chinese input stops until the window re-keys.
        //    Sending blurRequest instead would restore IME the same way,
        //    but only by blurring the Renderer first, which destroys the
        //    drag-built selection (NSTextField's becomeFirstResponder
        //    selectText runs on refocus).
        //  - The click blurs the remote input: the Renderer reports
        //    focus=false and the observer moves first responder to the
        //    surface; nothing to do here.
        // A handoff to a foreign control (address bar, another tab) needs
        // an explicit remote blur.
        guard super.resignFirstResponder() else { return false }
        DispatchQueue.main.async { [weak self] in
            guard let self, let successor = self.window?.firstResponder else { return }
            if successor === self {
                // No handoff: the focus observer already restored this view
                // between resign and this check. Treating it as a foreign
                // handoff would blur the input the user just focused.
                return
            }
            if successor === self.surfaceView {
                self.surfaceView?.imeCompositionDidHandoffToSurface()
            } else {
                self.forwardingMarkedText = ""
                self.surfaceView?.imeCompositionDidResign()
            }
        }
        return true
    }

    // MARK: NSTextInputClient (forwarding overrides)
    //
    // Every mutation is intercepted and mirrored to the Renderer; the local
    // text storage stays empty so the view remains invisible.

    override func insertText(_ string: Any, replacementRange: NSRange) {
        let text = (string as? NSAttributedString)?.string ?? (string as? String) ?? ""
        forwardingMarkedText = ""
        surfaceView?.sendInsertText(text)
    }

    override func setMarkedText(
        _ string: Any,
        selectedRange selRange: NSRange,
        replacementRange: NSRange
    ) {
        let text = (string as? NSAttributedString)?.string ?? (string as? String) ?? ""
        forwardingMarkedText = text
        forwardingMarkedSelection = selRange
        surfaceView?.sendMarkedText(text, selectedRange: selRange)
    }

    override func unmarkText() {
        // Cancel composition remotely too; otherwise the Renderer keeps
        // showing the previously mirrored marked range.
        surfaceView?.sendMarkedText("", selectedRange: NSRange(location: 0, length: 0))
        forwardingMarkedText = ""
        forwardingMarkedSelection = NSRange(location: 0, length: 0)
    }

    override func selectedRange() -> NSRange {
        forwardingMarkedSelection
    }

    override func markedRange() -> NSRange {
        forwardingMarkedText.isEmpty
            ? NSRange(location: NSNotFound, length: 0)
            : NSRange(location: 0, length: (forwardingMarkedText as NSString).length)
    }

    override func hasMarkedText() -> Bool {
        !forwardingMarkedText.isEmpty
    }

    override func attributedSubstring(
        forProposedRange range: NSRange,
        actualRange: NSRangePointer?
    ) -> NSAttributedString? {
        guard !forwardingMarkedText.isEmpty, range.location != NSNotFound,
              let subrange = Range(
                NSRange(location: range.location, length: min(range.length, (forwardingMarkedText as NSString).length - range.location)),
                in: forwardingMarkedText
              )
        else {
            return nil
        }
        return NSAttributedString(string: String(forwardingMarkedText[subrange]))
    }

    override func validAttributesForMarkedText() -> [NSAttributedString.Key] {
        []
    }

    /// Anchors the candidate window at the remote caret's on-screen rect.
    override func firstRect(
        forCharacterRange range: NSRange,
        actualRange: NSRangePointer?
    ) -> NSRect {
        surfaceView?.caretScreenRect() ?? super.firstRect(forCharacterRange: range, actualRange: actualRange)
    }

    override func characterIndex(for point: NSPoint) -> Int {
        NSNotFound
    }
}
