import ActriumIPC
import AppKit
import Darwin
import Foundation
import QuartzCore
import React

/// JSON payload for Host-mirrored IME text operations.
struct IMETextPayload: Codable {
    var text: String
    var loc: Int = 0
    var len: Int = 0
}

/// JSON payload for Host-forwarded editing actions (menu key equivalents).
struct IMECommandPayload: Codable {
    var command: String
}

/// Notification posted on the renderer's main queue after any
/// `NSWindow.makeFirstResponder(_:)` call, carrying the window.
let ActriumFirstResponderDidChangeNotification = Notification.Name(
    "ActriumFirstResponderDidChangeNotification"
)

extension NSWindow {
    /// Swizzled alongside the original `makeFirstResponder(_:)`; after the
    /// real implementation runs, surfaces report text-input focus changes to
    /// the Host so it can route IME composition there.
    @objc func actrium_makeFirstResponder(_ responder: NSResponder?) -> Bool {
        let accepted = actrium_makeFirstResponder(responder)
        NotificationCenter.default.post(
            name: ActriumFirstResponderDidChangeNotification,
            object: self
        )
        return accepted
    }

    static func actrium_installFirstResponderHook() {
        let original = class_getInstanceMethod(NSWindow.self, #selector(makeFirstResponder(_:)))
        let swizzled = class_getInstanceMethod(NSWindow.self, #selector(actrium_makeFirstResponder(_:)))
        guard let original, let swizzled else { return }
        method_exchangeImplementations(original, swizzled)
    }
}

/// Entry-point and lifecycle owner for a manually spawned Renderer Helper.
@MainActor
final class RendererProcessController {
    nonisolated static let rendererIdArgument = "--renderer-id"
    nonisolated static let fingerprintArgument = "--fingerprint"
    nonisolated static let ipcStdinArgument = "--ipc-stdin"
    nonisolated static let hostPidArgument = "--host-pid"

    nonisolated static var isRendererProcess: Bool {
        ProcessInfo.processInfo.arguments.contains(rendererIdArgument)
    }

    /// Returns `true` when the caller is running in the Renderer process. In
    /// that case the legacy distributed-notification fallback must not run,
    /// even if the Broker has already disconnected.
    @discardableResult
    nonisolated static func sendHostEvent(
        name: String,
        userInfo: [String: Any] = [:],
        tabId: String = ""
    ) -> Bool {
        guard isRendererProcess else { return false }
        guard JSONSerialization.isValidJSONObject(userInfo),
              let data = try? JSONSerialization.data(withJSONObject: userInfo),
            let json = String(data: data, encoding: .utf8)
        else {
            return true
        }
        // React Native may call from its module queue. Only immutable values
        // cross to the renderer's main-actor lifecycle and callback state.
        DispatchQueue.main.async {
            guard let controller = RendererProcessLifetime.shared.controller else { return }
            do {
                try controller.connection.sendHostEvent(name: name, userInfoJSON: json, tabId: tabId)
            } catch {
                NSLog("ActriumRenderer host event failed: %@ (%@)", name, error.localizedDescription)
            }
        }
        return true
    }

    @discardableResult
    nonisolated static func sendConsoleLog(level: String, source: String, message: String) -> Bool {
        guard isRendererProcess else { return false }
        DispatchQueue.main.async {
            guard let controller = RendererProcessLifetime.shared.controller else { return }
            do {
                try controller.connection.sendConsoleLog(
                    level: level,
                    message: message,
                    source: source
                )
            } catch {
                NSLog("ActriumRenderer console forwarding failed: %@", error.localizedDescription)
            }
        }
        return true
    }

    private struct Arguments {
        let rendererId: String
        let fingerprint: String
        let hostPid: Int32

        static func parse(_ values: [String]) -> Arguments? {
            func value(after key: String) -> String? {
                guard let index = values.firstIndex(of: key),
                      values.indices.contains(index + 1) else {
                    return nil
                }
                return values[index + 1]
            }

            guard let rendererId = value(after: rendererIdArgument),
                  let fingerprint = value(after: fingerprintArgument),
                  values.contains(ipcStdinArgument),
                  let hostPidString = value(after: hostPidArgument),
                  let hostPid = Int32(hostPidString) else {
                return nil
            }

            return Arguments(
                rendererId: rendererId,
                fingerprint: fingerprint,
                hostPid: hostPid
            )
        }
    }

    private let arguments: Arguments
    private let connection: IPCHelperConnection
    private var serviceCalls: [UUID: (RealmServiceReply) -> Void] = [:]
    private var fileImports: [String: (id: UUID, reloadKey: Int, reply: (GuestFileImportReply) -> Void)] = [:]

    static func pickFiles(
        _ call: GuestFileImportCall, tabId: String, reply: @escaping (GuestFileImportReply) -> Void
    ) {
        guard let controller = RendererProcessLifetime.shared.controller,
              controller.surfaces[tabId] != nil, controller.fileImports[tabId] == nil else {
            reply(GuestFileImportReply(id: call.id, error: "The picker tab is unavailable or already selecting files."))
            return
        }
        do {
            let data = try JSONEncoder().encode(call)
            try controller.connection.sendHostEvent(
                name: GuestFileImportCall.requestEvent, userInfoJSON: String(decoding: data, as: UTF8.self), tabId: tabId
            )
            controller.fileImports[tabId] = (call.id, call.reloadKey, reply)
            let requestId = call.id
            DispatchQueue.main.asyncAfter(deadline: .now() + .seconds(GuestFileImportCall.timeoutSeconds)) {
                [weak controller] in
                guard let controller, controller.fileImports[tabId]?.id == requestId else { return }
                controller.cancelFileImport(tabId, message: "The file selection timed out.")
            }
        } catch {
            reply(GuestFileImportReply(id: call.id, error: "Could not contact the file picker."))
        }
    }

    static func callRealmService(
        tabId: String,
        call: RealmServiceCall,
        reply: @escaping (RealmServiceReply) -> Void
    ) {
        guard let controller = RendererProcessLifetime.shared.controller,
              controller.surfaces[tabId] != nil else {
            let failure = RealmServiceFailure(code: "realm_tab_unavailable", message: "The Realm tab is unavailable.")
            reply(RealmServiceReply(id: call.id, failure: failure))
            return
        }
        do {
            let data = try JSONEncoder().encode(call)
            try controller.connection.sendHostEvent(
                name: RealmServiceCall.requestEvent,
                userInfoJSON: String(decoding: data, as: UTF8.self),
                tabId: tabId
            )
            controller.serviceCalls[call.id] = reply
            DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(call.timeoutMs)) { [weak controller] in
                let failure = RealmServiceFailure(
                    code: "realm_service_timeout", message: "The Service call timed out."
                )
                controller?.serviceCalls.removeValue(forKey: call.id)?(RealmServiceReply(id: call.id, failure: failure))
            }
        } catch {
            reply(RealmServiceReply(id: call.id, failure: RealmServiceFailure(error: error)))
        }
    }
    private let reactNativeHost = ServiceReactNativeHost.shared
    private var surfaces: [String: RendererTabSurface] = [:]

    private init(arguments: Arguments) {
        self.arguments = arguments
        connection = IPCHelperConnection(
            tabId: arguments.rendererId,
            connectedSocket: STDIN_FILENO
        )
        configureIPC()
    }

    static func runFromCommandLine() -> Never {
        guard let arguments = Arguments.parse(ProcessInfo.processInfo.arguments) else {
            fputs("ActriumRenderer: missing required Broker arguments\n", stderr)
            exit(64)
        }

        let application = NSApplication.shared
        application.setActivationPolicy(.prohibited)
        application.finishLaunching()

        let controller = RendererProcessController(arguments: arguments)
        RendererProcessLifetime.shared.controller = controller

        do {
            try controller.connection.connect()
            try controller.connection.sendReady(
                surfaceId: 0,
                width: 0,
                height: 0,
                rendererId: arguments.rendererId
            )
        } catch {
            fputs("ActriumRenderer: Broker connection failed: \(error)\n", stderr)
            exit(70)
        }

        NSLog(
            "ActriumRenderer ready: renderer=%@ fingerprint=%@ pid=%d host=%d",
            arguments.rendererId,
            arguments.fingerprint,
            ProcessInfo.processInfo.processIdentifier,
            arguments.hostPid
        )
        application.run()
        exit(0)
    }

    private func configureIPC() {
        NSWindow.actrium_installFirstResponderHook()
        NotificationCenter.default.addObserver(
            forName: ActriumFirstResponderDidChangeNotification,
            object: nil,
            queue: .main
        ) { [weak self] note in
            guard let self, let window = note.object as? NSWindow else { return }
            for (tabId, surface) in self.surfaces where surface.window === window {
                self.reportTextInputFocus(tabId: tabId, surface: surface)
            }
        }
        connection.onHostEvent = { [weak self] name, json, tabId in
            guard let self else { return }
            if name == GuestFileImportCall.responseEvent {
                receiveFileImport(json, tabId: tabId)
                return
            }
            if name == "com.actrium.rendererKeyboardFocus" {
                guard let focused = Bool(json) else { return }
                // Apply focus on the same queue as input so the next key
                // event can reach the restored editor.
                DispatchQueue.main.async {
                    self.surfaces[tabId]?.setKeyboardFocus(focused)
                }
                return
            }
            if name == RealmServiceCall.responseEvent {
                guard let response = try? JSONDecoder().decode(RealmServiceReply.self, from: Data(json.utf8)) else {
                    return
                }
                self.serviceCalls.removeValue(forKey: response.id)?(response)
                return
            }
            // IME composition runs in the Host process (TSM never engages in
            // this background, activation-prohibited process); the Host
            // mirrors marked/committed text back for the focused text input.
            DispatchQueue.main.async {
                switch name {
                case "com.actrium.imeMarkedText":
                    guard let op = try? JSONDecoder().decode(IMETextPayload.self, from: Data(json.utf8)) else { return }
                    self.surfaces[tabId]?.applyMarkedText(op.text, selectedRange: NSRange(location: op.loc, length: op.len))
                case "com.actrium.imeInsertText":
                    guard let op = try? JSONDecoder().decode(IMETextPayload.self, from: Data(json.utf8)) else { return }
                    self.surfaces[tabId]?.applyInsertText(op.text)
                case "com.actrium.textInputBlurRequest":
                    self.surfaces[tabId]?.resignTextInputFocus()
                case "com.actrium.textCommand":
                    guard let payload = try? JSONDecoder().decode(IMECommandPayload.self, from: Data(json.utf8)) else { return }
                    self.surfaces[tabId]?.performTextCommand(payload.command)
                case "com.actrium.textInputFocusReset":
                    // The Host dropped our pre-interaction focus report and
                    // its user-interaction gate just opened; force a fresh
                    // report so IME can activate on the already-focused field.
                    if let surface = self.surfaces[tabId] {
                        self.reportTextInputFocus(tabId: tabId, surface: surface, force: true)
                    }
                default:
                    break
                }
            }
        }

        connection.onLoadContent = { [weak self] request, reply in
            DispatchQueue.main.async {
                self?.loadContent(request, reply: reply)
            }
        }

        connection.onTabUpdateViewport = { [weak self] tabId, width, height, requestId, overview in
            DispatchQueue.main.async {
                self?.updateViewport(
                    tabId: tabId,
                    size: CGSize(width: width, height: height),
                    requestId: requestId,
                    isOverviewMode: overview
                )
            }
        }

        connection.onInputEvent = { [weak self] event in
            DispatchQueue.main.async {
                self?.surfaces[event.tabID]?.dispatch(event)
            }
        }

        connection.onTabGetSurfaceID = { [weak self] tabId, reply in
            DispatchQueue.main.async {
                reply(self?.surfaces[tabId]?.contextId ?? 0)
            }
        }

        connection.onDestroyTab = { [weak self] tabId in
            DispatchQueue.main.async {
                self?.destroyTab(tabId)
            }
        }

        connection.onGetAccessibilityTree = { [weak self] tabId, reply in
            DispatchQueue.main.async {
                reply(self?.surfaces[tabId]?.accessibilityTree() ?? Actrium_Ipc_AccessibilityTree())
            }
        }

        connection.onAccessibilityAction = { [weak self] request, reply in
            DispatchQueue.main.async {
                let ok = self?.surfaces[request.tabID]?.performAccessibility(
                    action: request.action,
                    nodeId: request.nodeID
                ) ?? false
                if ok {
                    self?.publishAccessibilityTree(tabId: request.tabID)
                }
                reply(ok, ok ? nil : "Accessibility action failed")
            }
        }

        connection.onGetMetrics = { reply in
            reply(RendererProcessController.metrics())
        }

        connection.onTabExecuteJS = { _, _, reply in
            reply(false, nil, "Renderer evaluateScript is not implemented")
        }

        connection.onTerminate = { [weak self] reason in
            DispatchQueue.main.async {
                self?.terminate(reason: reason)
            }
        }

        connection.onDisconnected = { [weak self] in
            DispatchQueue.main.async {
                self?.terminate(reason: "Broker disconnected")
            }
        }
    }

    private func loadContent(
        _ request: Actrium_Ipc_LoadContentRequest,
        reply: @escaping (Bool, String?) -> Void
    ) {
        let tabId = request.tabID
        guard !tabId.isEmpty else {
            reply(false, "Missing tabId")
            return
        }

        var config: [String: Any] = [:]
        if let data = request.configJson.data(using: .utf8),
           let decoded = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
            config = decoded
        }

        if let pending = fileImports[tabId], pending.reloadKey != Int(request.reloadKey) {
            cancelFileImport(tabId)
        }
        config["tabId"] = tabId
        if config["title"] == nil {
            config["title"] = request.title
        }
        if config["backgroundColor"] == nil {
            config["backgroundColor"] = request.backgroundColor
        }
        if config["unifiedScript"] == nil, request.contentType == "javascript" {
            config["unifiedScript"] = request.content
        }

        let width = (config["containerWidth"] as? NSNumber)?.doubleValue ?? 1280
        let height = (config["containerHeight"] as? NSNumber)?.doubleValue ?? 720
        let size = CGSize(width: max(width, 1), height: max(height, 1))

        let surface: RendererTabSurface
        if let existing = surfaces[tabId] {
            surface = existing
            surface.resize(to: size)
        } else {
            let entry = reactNativeHost.createRendererEntry(tabId: tabId, size: size)
            guard let created = RendererTabSurface(tabId: tabId, entry: entry, size: size) else {
                reply(false, "Failed to create remote CoreAnimation context")
                return
            }
            surfaces[tabId] = created
            surface = created
        }

        reactNativeHost.applyRendererConfig(config) { [weak self, weak surface] success, error in
            guard let self, let surface else {
                reply(false, "Renderer surface released")
                return
            }

            if success {
                surface.commit()
                self.publishAccessibilityTree(tabId: tabId)
                try? self.connection.sendSurfaceUpdated(
                    surfaceId: surface.contextId,
                    tabId: tabId
                )
                NSLog(
                    "Renderer surface ready: renderer=%@ tab=%@ context=%u",
                    self.arguments.rendererId,
                    tabId,
                    surface.contextId
                )
            }
            reply(success, error)
        }
    }

    private func updateViewport(
        tabId: String,
        size: CGSize,
        requestId: UInt32,
        isOverviewMode: Bool
    ) {
        guard let surface = surfaces[tabId],
              size.width > 0,
              size.height > 0 else {
            return
        }

        surface.resize(to: size)
        reactNativeHost.resizeRendererEntry(
            tabId: tabId,
            size: size,
            requestId: requestId,
            isOverviewMode: isOverviewMode
        )
        surface.commit()
        try? connection.sendViewportApplied(
            tabId: tabId,
            requestId: requestId,
            width: size.width,
            height: size.height
        )
        publishAccessibilityTree(tabId: tabId)
    }

    /// Called from the Fabric surface-presenter observer after mount
    /// instructions are applied. Re-commits the matching tab surface so the
    /// new CAContext frame reaches the Host instead of waiting for a resize.
    func commitSurface(forRootTag rootTag: Int) {
        let surfaceSelector = NSSelectorFromString("surface")
        let rootTagSelector = NSSelectorFromString("rootTag")
        for surface in surfaces.values {
            guard let rootView = surface.entry.rootView else { continue }
            let rootObject = rootView as AnyObject
            guard rootObject.responds(to: surfaceSelector) else { continue }
            guard let surfaceObject = rootObject.value(forKey: "surface") as AnyObject?,
                  surfaceObject.responds(to: rootTagSelector) else { continue }
            let tagValue = surfaceObject.value(forKey: "rootTag")
            let tag = (tagValue as? NSNumber).map { Int(truncating: $0) } ?? (tagValue as? Int)
            if tag == rootTag {
                surface.commit()
            }
        }
    }

    /// Forwards the surface caret rect to the Host; nil hides the candidate
    /// window anchor. Transport reuses the generic HostEvent channel.
    /// Reports the surface's text-input focus state to the Host, unless it is
    /// unchanged since the last report. `force` bypasses the dedupe latch.
    private func reportTextInputFocus(tabId: String, surface: RendererTabSurface, force: Bool = false) {
        if force {
            surface.resetTextInputFocusDedupe()
        }
        surface.reportTextInputFocusIfChanged { focused in
            try self.connection.sendHostEvent(
                name: "com.actrium.textInputFocus",
                userInfoJSON: "{\"focused\":\(focused)}",
                tabId: tabId
            )
        }
    }

    func publishCaret(tabId: String, rect: CGRect?) {
        guard let encoded = try? JSONEncoder().encode(rect) else { return }
        try? connection.sendHostEvent(
            name: "com.actrium.rendererCaret",
            userInfoJSON: String(decoding: encoded, as: UTF8.self),
            tabId: tabId
        )
    }

    func publishAccessibilityTree(tabId: String) {
        guard let surface = surfaces[tabId] else { return }
        try? connection.sendAccessibilityTree(surface.accessibilityTree())
    }

    private func destroyTab(_ tabId: String) {
        cancelFileImport(tabId)
        surfaces.removeValue(forKey: tabId)?.close()
        reactNativeHost.destroyRendererEntry(tabId: tabId)
    }

    private func cancelFileImport(_ tabId: String, message: String = "The picker document changed or closed.") {
        if let pending = fileImports.removeValue(forKey: tabId) {
            pending.reply(GuestFileImportReply(id: pending.id, error: message))
        }
    }

    private func receiveFileImport(_ json: String, tabId: String) {
        guard let response = try? JSONDecoder().decode(GuestFileImportReply.self, from: Data(json.utf8)),
              fileImports[tabId]?.id == response.id else { return }
        fileImports.removeValue(forKey: tabId)?.reply(response)
    }

    private func terminate(reason: String) {
        NSLog("ActriumRenderer terminating: %@", reason)
        let tabIds = Array(surfaces.keys)
        for tabId in tabIds {
            destroyTab(tabId)
        }
        connection.disconnect()
        NSApplication.shared.terminate(nil)
    }

    private static func metrics() -> Actrium_Ipc_MetricsResponse {
        var response = Actrium_Ipc_MetricsResponse()
        var usage = rusage()
        if getrusage(RUSAGE_SELF, &usage) == 0 {
            #if arch(arm64)
            response.memoryUsage = UInt64(max(usage.ru_maxrss, 0))
            #else
            response.memoryUsage = UInt64(max(usage.ru_maxrss, 0)) * 1024
            #endif
        }
        return response
    }
}

/// Keeps the top-level controller alive for the lifetime of NSApplication.
@MainActor
private final class RendererProcessLifetime {
    static let shared = RendererProcessLifetime()
    var controller: RendererProcessController?
}

/// Fabric mount observer: commit the matching Renderer surface after the
/// native mount instructions finish, so reload content is published without
/// waiting for a window resize.
final class RendererSurfaceMountObserver: NSObject {
    static let shared = RendererSurfaceMountObserver()

    @objc(didMountComponentsWithRootTag:)
    func didMountComponents(rootTag: Int) {
        Task { @MainActor in
            RendererProcessLifetime.shared.controller?.commitSurface(
                forRootTag: rootTag
            )
        }
    }
}

/// One Tab root and its remotely hosted CoreAnimation context.
@MainActor
private final class RendererTabSurface {
    let tabId: String
    let entry: TabViewControllerEntry
    let container: ResizableContainerView
    let window: RendererWindow
    let context: RendererCAContext
    private var generation: UInt64 = 0
    private var hasCommitted = false
    private var accessibilityObservers: [NSObjectProtocol] = []
    private var accessibilityPublishWork: DispatchWorkItem?
    private var pressedRecognizers: [UInt32: [NSGestureRecognizer]] = [:]
    private var textInputPresses: Set<UInt32> = []
    /// Character index where the current text drag started. AppKit's
    /// mouse-tracking selection cannot run here (mouse events arrive
    /// asynchronously over IPC), so drag selection is driven manually.
    private var dragSelectionAnchor: Int?
    /// The selection that created `dragSelectionAnchor`. An anchor is only
    /// reused while this still equals the current selection; keyboard-built
    /// selections leave a stale anchor whose offset may coincidentally fall
    /// inside the new range but does not describe it.
    private var dragSelectionAnchorRange: NSRange?
    /// Last text-input focus state reported to the Host. The Host mirrors
    /// IME composition for whichever surface owns the focused text input.
    private var lastReportedTextFocus = false
    private weak var focusedEditor: NSTextView?
    private var lastCaretRect: CGRect?
    /// Snapshot-based undo/redo for Host-mirrored text commits. AppKit's
    /// undoManager registrations go stale as soon as RN rewrites the text
    /// storage, so undo is implemented by restoring previous snapshots
    /// through the normal input-client path (which RN observes).
    private struct EditSnapshot {
        let string: String
        let caret: Int
    }
    /// Per-control undo history. The window field editor is shared, so a
    /// control's identity comes from the field editor's delegate. History
    /// persists while the surface lives: switching away and back keeps a
    /// control's undo stack, matching AppKit's own behavior.
    private final class UndoHistoryBox {
        var undo: [EditSnapshot] = []
        var redo: [EditSnapshot] = []
    }
    /// Weak-keyed so an unmounted guest control's history disappears with
    /// the control; a fresh control allocated at the same address must not
    /// inherit a stale stack.
    private let undoHistories = NSMapTable<AnyObject, UndoHistoryBox>.weakToStrongObjects()

    private func currentControl() -> AnyObject? {
        guard let editor = window.firstResponder as? NSTextView else { return nil }
        if editor.isFieldEditor {
            return editor.delegate.map { $0 as AnyObject }
        }
        return editor
    }

    private func withHistory(_ body: (UndoHistoryBox) -> Void) {
        guard let control = currentControl() else { return }
        let box = undoHistories.object(forKey: control) ?? {
            let created = UndoHistoryBox()
            undoHistories.setObject(created, forKey: control)
            return created
        }()
        body(box)
        if box.undo.count > 100 { box.undo.removeFirst(box.undo.count - 100) }
    }


    var contextId: UInt32 {
        context.contextId
    }

    init?(tabId: String, entry: TabViewControllerEntry, size: CGSize) {
        guard let container = entry.containerView,
              let context = RendererCAContext() else {
            return nil
        }

        self.tabId = tabId
        self.entry = entry
        self.container = container
        self.context = context

        window = RendererWindow(
            contentRect: NSRect(origin: .zero, size: size),
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        window.isReleasedWhenClosed = false
        window.backgroundColor = .clear
        window.isOpaque = false
        window.acceptsMouseMovedEvents = true
        window.contentView = container
        window.setFrameOrigin(NSPoint(x: -100_000, y: -100_000))
        window.orderOut(nil)

        container.frame = NSRect(origin: .zero, size: size)
        container.wantsLayer = true
        commit()
        observeAccessibility()
    }

    func accessibilityTree() -> Actrium_Ipc_AccessibilityTree {
        generation += 1
        return RendererAccessibility.snapshot(
            tabId: tabId,
            root: container,
            generation: generation
        )
    }

    func performAccessibility(action: String, nodeId: UInt64) -> Bool {
        RendererAccessibility.perform(action: action, nodeId: nodeId, in: container)
    }

    func resize(to size: CGSize) {
        guard size.width > 0, size.height > 0 else { return }
        window.setContentSize(size)
        container.applyHostSize(size)
        container.layer?.frame = CGRect(origin: .zero, size: size)
    }

    func commit() {
        // The first commit happens on an empty view tree, so lay it out once.
        // Later commits rely on AppKit/RN marking the container dirty
        // (addSubview, surface stage change, resize); Fabric sets frames
        // directly, so a subtree-only dirty state is intentionally not
        // remeasured here.
        if !hasCommitted || container.needsLayout {
            container.needsLayout = true
            container.layoutSubtreeIfNeeded()
            hasCommitted = true
        }
        context.attachWindowLayerTree(for: container)
        CATransaction.flush()
    }

    func setKeyboardFocus(_ focused: Bool) {
        guard window.hasHostKeyboardFocus != focused else { return }
        window.setHostKeyboardFocus(focused)
        commit()
    }

    func close() {
        undoHistories.removeAllObjects()
        accessibilityPublishWork?.cancel()
        accessibilityPublishWork = nil
        for observer in accessibilityObservers {
            NotificationCenter.default.removeObserver(observer)
        }
        accessibilityObservers.removeAll()
        context.layer = nil
        window.contentView = nil
        window.close()
    }

    private func observeAccessibility() {
        let names: [Notification.Name] = [
            Notification.Name(NSAccessibility.Notification.focusedUIElementChanged.rawValue),
            Notification.Name(NSAccessibility.Notification.layoutChanged.rawValue),
            Notification.Name(NSAccessibility.Notification.valueChanged.rawValue),
            Notification.Name(NSAccessibility.Notification.titleChanged.rawValue),
            Notification.Name(NSAccessibility.Notification.selectedChildrenChanged.rawValue)
        ]
        for name in names {
            let observer = NotificationCenter.default.addObserver(
                forName: name,
                object: nil,
                queue: .main
            ) { [weak self] notification in
                // NotificationCenter delivers this observer on OperationQueue.main.
                MainActor.assumeIsolated {
                    guard let self else { return }
                    if let view = notification.object as? NSView, view.window !== self.window {
                        return
                    }
                    self.scheduleAccessibilityPublish()
                }
            }
            accessibilityObservers.append(observer)
        }
    }

    private func scheduleAccessibilityPublish() {
        accessibilityPublishWork?.cancel()
        let work = DispatchWorkItem { [weak self] in
            self?.pushAccessibilityTree()
        }
        accessibilityPublishWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.08, execute: work)
    }

    private func pushAccessibilityTree() {
        guard let controller = RendererProcessLifetime.shared.controller else { return }
        controller.publishAccessibilityTree(tabId: tabId)
    }

    func dispatch(_ input: Actrium_Ipc_InputEvent) {
        let modifiers = modifierFlags(from: input.modifiers)
        // IPC coordinates are top-left based, while AppKit window events use
        // bottom-left based window coordinates.
        let location = container.convert(NSPoint(x: input.x, y: input.y), to: nil)
        let timestamp = ProcessInfo.processInfo.systemUptime

        switch input.type {
        case .mouseDown, .mouseUp, .mouseMove:
            let type: NSEvent.EventType
            if input.type == .mouseMove {
                type = .mouseMoved
            } else if input.button == 1 {
                type = input.type == .mouseDown ? .rightMouseDown : .rightMouseUp
            } else if input.button == 2 {
                type = input.type == .mouseDown ? .otherMouseDown : .otherMouseUp
            } else {
                type = input.type == .mouseDown ? .leftMouseDown : .leftMouseUp
            }

            guard let event = NSEvent.mouseEvent(
                with: type,
                location: location,
                modifierFlags: modifiers,
                timestamp: timestamp,
                windowNumber: window.windowNumber,
                context: nil,
                eventNumber: 0,
                clickCount: input.type == .mouseMove ? 0 : 1,
                pressure: input.type == .mouseDown ? 1 : 0
            ) else {
                return
            }

            let hitView = hitTest(at: location)
            // Editable RN controls keep AppKit's native focus path; routing
            // their clicks through the JS responder revokes first responder.
            if input.type == .mouseDown {
                let ancestors = sequence(first: hitView, next: { $0?.superview })
                    .prefix(while: { $0 != nil }).compactMap { $0 }
                let textInput = ancestors.first { $0 is NSTextView || $0 is NSTextField }
                if input.button == 0, let textInput {
                    textInputPresses.insert(input.button)
                    focusTextInput(textInput, at: location, modifiers: modifiers)
                    // AppKit adds the field editor to the offscreen view
                    // hierarchy on focus. Republish after that update.
                    DispatchQueue.main.async { [weak self] in
                        guard let self, self.window.contentView === self.container else { return }
                        self.commit()
                    }
                    return
                }
                window.makeFirstResponder(hitView ?? container)
                let recognizers = ancestors.flatMap(\.gestureRecognizers)
                forwardMouse(event, to: recognizers)
                pressedRecognizers[input.button] = recognizers
            } else if input.type == .mouseMove, input.button == 0,
                      textInputPresses.contains(0),
                      let anchor = dragSelectionAnchor,
                      let editor = window.firstResponder as? NSTextView {
                // Drive drag selection manually (see dragSelectionAnchor).
                let point = editor.convert(location, from: nil)
                let index = editor.characterIndexForInsertion(at: point)
                if index != NSNotFound {
                    let range = NSRange(
                        location: min(anchor, index),
                        length: abs(index - anchor)
                    )
                    if editor.selectedRange() != range {
                        editor.setSelectedRange(range)
                        editor.scrollRangeToVisible(range)
                        publishCaret()
                        commit()
                    }
                }
                return
            } else if input.type == .mouseUp {
                if textInputPresses.remove(input.button) != nil {
                    return
                }
                if let recognizers = pressedRecognizers.removeValue(forKey: input.button) {
                    forwardMouse(event, to: recognizers)
                }
            }
            window.sendEvent(event)

        case .scroll:
            guard let hitView = hitTest(at: location) else { return }
            let delta = NSPoint(x: input.scrollDeltaX, y: input.scrollDeltaY)
            if ActriumScrollRendererContent(hitView, delta) {
                commit()
            }

        case .keyDown, .keyUp:
            guard window.hasHostKeyboardFocus,
                  let event = NSEvent.keyEvent(
                with: input.type == .keyDown ? .keyDown : .keyUp,
                location: location,
                modifierFlags: modifiers,
                timestamp: timestamp,
                windowNumber: window.windowNumber,
                context: nil,
                characters: input.characters,
                charactersIgnoringModifiers: input.characters,
                isARepeat: false,
                keyCode: UInt16(input.keyCode)
            ) else {
                return
            }
            // Text composition happens in the Host process (TSM never engages
            // in this background, activation-prohibited process); dispatched
            // keys here are only non-text commands (arrows, enter, ...).
            // Deleting, Return and Tab can mutate the text storage, so
            // capture undo snapshots around them the same way mirrored
            // inserts do.
            let isDelete = input.type == .keyDown
                && (input.keyCode == 51 || input.keyCode == 117
                    || input.keyCode == 36 || input.keyCode == 48)
            if isDelete { beginUndoableEdit() }
            if let textView = window.firstResponder as? NSTextView {
                if input.type == .keyDown { textView.keyDown(with: event) }
                else { textView.keyUp(with: event) }
            } else {
                window.sendEvent(event)
            }
            if isDelete { endUndoableEdit() }
            if input.type == .keyDown {
                // Fabric publishes content changes after mounting. A caret
                // movement alone does not need to flush the remote surface.
                publishCaret()
            }

        case .mouseEntered, .mouseExited, .UNRECOGNIZED(_):
            break
        }
    }

    private func hitTest(at location: NSPoint) -> NSView? {
        let point = container.superview?.convert(location, from: nil) ?? location
        return container.hitTest(point)
    }



    /// Compares the current first responder against the last reported
    /// text-input focus state and reports changes through `report`.
    func reportTextInputFocusIfChanged(_ report: (Bool) throws -> Void) {
        let responder = window.firstResponder
        let focused = (responder as? NSTextView)?.isEditable == true
            || (responder as? NSTextField)?.isEditable == true
        guard focused != lastReportedTextFocus else { return }
        lastReportedTextFocus = focused
        try? report(focused)
        if focused { publishCaret() }
    }

    /// Flips the dedupe latch so the next `reportTextInputFocusIfChanged`
    /// reports even when the focus state itself did not change.
    func resetTextInputFocusDedupe() {
        let responder = window.firstResponder
        let focused = (responder as? NSTextView)?.isEditable == true
            || (responder as? NSTextField)?.isEditable == true
        lastReportedTextFocus = !focused
    }

    /// Applies Host-mirrored IME marked text to the focused text input.
    func applyMarkedText(_ text: String, selectedRange: NSRange) {
        guard let client = window.firstResponder as? NSTextInputClient else { return }
        client.setMarkedText(
            text,
            selectedRange: selectedRange,
            replacementRange: NSRange(location: NSNotFound, length: 0)
        )
        publishCaret()
        commit()
    }

    /// Applies Host-mirrored committed IME text to the focused text input.
    func applyInsertText(_ text: String) {
        guard let client = window.firstResponder as? NSTextInputClient else { return }
        beginUndoableEdit()
        client.insertText(text, replacementRange: NSRange(location: NSNotFound, length: 0))
        endUndoableEdit()
        publishCaret()
        commit()
    }

    /// The editor's string without any marked (in-progress composition)
    /// text plus the caret, both in unmarked-string coordinates. Marked text
    /// lives in the text storage but must not leak into undo snapshots.
    private func currentEditSnapshot() -> EditSnapshot? {
        guard let editor = window.firstResponder as? NSTextView else { return nil }
        let marked = editor.markedRange()
        let string = (marked.location != NSNotFound && marked.length > 0)
            ? (editor.string as NSString).replacingCharacters(in: marked, with: "")
            : editor.string
        let caret = min(editor.selectedRange().location, (string as NSString).length)
        return EditSnapshot(string: string, caret: caret)
    }

    private func beginUndoableEdit() {
        guard let snapshot = currentEditSnapshot() else { return }
        withHistory { box in
            if box.undo.last?.string != snapshot.string { box.undo.append(snapshot) }
            box.redo.removeAll()
        }
    }

    private func endUndoableEdit() {
        guard let snapshot = currentEditSnapshot() else { return }
        withHistory { box in
            if box.undo.last?.string != snapshot.string { box.undo.append(snapshot) }
        }
    }

    private func restoreSnapshot(_ snapshot: String, caret: Int) {
        guard let editor = window.firstResponder as? NSTextView, editor.isEditable else { return }
        let current = editor.string
        guard current != snapshot else {
            editor.setSelectedRange(NSRange(location: caret, length: 0))
            publishCaret()
            return
        }
        let full = NSRange(location: 0, length: (current as NSString).length)
        editor.setSelectedRange(full)
        editor.insertText(snapshot, replacementRange: full)
        editor.setSelectedRange(NSRange(location: caret, length: 0))
        publishCaret()
        commit()
    }

    private func undoTextEdit() {
        guard window.firstResponder is NSTextView else { return }
        withHistory { box in
            guard box.undo.count > 1 else { return }
            let current = box.undo.removeLast()
            box.redo.append(current)
            let target = box.undo[box.undo.count - 1]
            // Undoing an insertion puts the caret back at the insertion
            // point, not at the end of the restored text. All offsets are
            // UTF-16 units to match NSTextView selection semantics.
            let delta = (current.string as NSString).length - (target.string as NSString).length
            let caret = max(0, min(current.caret - delta, (target.string as NSString).length))
            restoreSnapshot(target.string, caret: caret)
        }
    }

    private func redoTextEdit() {
        guard window.firstResponder is NSTextView else { return }
        withHistory { box in
            guard let snapshot = box.redo.popLast() else { return }
            if let current = currentEditSnapshot() {
                box.undo.append(current)
            }
            restoreSnapshot(snapshot.string, caret: snapshot.caret)
        }
    }

    /// Applies Host-forwarded editing actions (menu key equivalents) to the
    /// focused text input. copy/cut/paste/selectAll run through AppKit's own
    /// implementations on the real text storage; undo/redo restore snapshots
    /// captured at each Host-mirrored text commit (AppKit's undoManager
    /// registrations go stale once RN rewrites the text storage).
    func performTextCommand(_ command: String) {
        if command == "undo" { undoTextEdit(); return }
        if command == "redo" { redoTextEdit(); return }
        let selectors: [String: Selector] = [
            "copy": #selector(NSText.copy(_:)),
            "cut": #selector(NSText.cut(_:)),
            "paste": #selector(NSText.paste(_:)),
            "selectAll": #selector(NSText.selectAll(_:)),
        ]
        guard let selector = selectors[command] else { return }
        let target: NSResponder? = (window.firstResponder as? NSResponder) ?? window
        let mutates = command == "cut" || command == "paste"
        if mutates { beginUndoableEdit() }
        ActriumSendEditingAction(selector, target ?? NSApp)
        if mutates { endUndoableEdit() }
        publishCaret()
        commit()
    }


    /// The Host asked this text input to give up focus (its own IME client
    /// resigned first responder, e.g. the user focused the address bar).
    func resignTextInputFocus() {
        if window.firstResponder is NSTextView || window.firstResponder is NSTextField {
            window.makeFirstResponder(container)
        }
        focusedEditor = nil
        lastCaretRect = nil
        RendererProcessLifetime.shared.controller?.publishCaret(tabId: tabId, rect: nil)
    }

    private func focusTextInput(_ view: NSView, at location: NSPoint, modifiers: NSEvent.ModifierFlags) {
        // AppKit's native mouse tracking waits synchronously for mouse-up.
        // Renderer mouse-up arrives on the main queue over IPC.
        let focused = window.makeFirstResponder(view)
        let editor = (view as? NSTextField)?.currentEditor() as? NSTextView ?? view as? NSTextView
        focusedEditor = focused ? editor : nil
        guard focused, let editor else { return }
        let point = editor.convert(location, from: nil)
        let index = editor.characterIndexForInsertion(at: point)
        guard index != NSNotFound, index <= (editor.string as NSString).length else { return }
        if modifiers.contains(.shift) {
            // Shift+click/drag extends the selection; the mouse-down and
            // mouse-up paths consume the event, so AppKit's own extension
            // never runs. The base is the existing selection's anchor when a
            // selection is live, and the caret otherwise — a stored anchor
            // is only trusted while it still belongs to the current
            // selection (the caret may have moved since, via keyboard or
            // undo). The resolved base is stored back so the whole gesture
            // (mouse-down, drag, and repeated Shift+clicks) uses one anchor.
            let current = editor.selectedRange()
            let base: Int
            if current.length > 0,
               let anchor = dragSelectionAnchor,
               dragSelectionAnchorRange == current {
                // The stored anchor still describes the current selection.
                base = anchor
            } else if index >= current.upperBound {
                // Untracked selection, click on the right: extend from the
                // opposite (left) edge so the existing range stays selected.
                base = current.location
            } else if index <= current.location {
                // Click on the left: extend from the right edge.
                base = current.upperBound
            } else {
                // Click inside: keep the farther edge so neither side is
                // dropped arbitrarily.
                base = (index - current.location) > (current.upperBound - index)
                    ? current.location
                    : current.upperBound
            }
            dragSelectionAnchor = base
            let range = NSRange(location: min(base, index), length: abs(index - base))
            dragSelectionAnchorRange = range
            editor.setSelectedRange(range)
            publishCaret()
            commit()
        } else {
            dragSelectionAnchor = index
            dragSelectionAnchorRange = NSRange(location: index, length: 0)
            editor.setSelectedRange(NSRange(location: index, length: 0))
            publishCaret()
        }
    }

    /// Publishes the caret rect to the Host so native IME UI (candidate
    /// window) can anchor at the remotely rendered insertion point.
    private func publishCaret() {
        let rect = caretRect()
        guard rect != lastCaretRect else { return }
        lastCaretRect = rect
        RendererProcessLifetime.shared.controller?.publishCaret(tabId: tabId, rect: rect)
    }

    private func caretRect() -> CGRect? {
        // Prefer the stored editor but fall back to the current first
        // responder: autoFocus / Tab / JS .focus() never pass through
        // `focusTextInput`, leaving `focusedEditor` nil even though a text
        // view owns first responder.
        let editor = focusedEditor ?? (window.firstResponder as? NSTextView)
        guard let editor, window.firstResponder === editor, editor.isEditable,
              editor.selectedRange().length == 0 else {
            return nil
        }
        let screenRect = editor.firstRect(forCharacterRange: editor.selectedRange(), actualRange: nil)
        let localRect: NSRect
        if editor.string.isEmpty {
            // AppKit can return an empty character rectangle for an empty field.
            let origin = editor.textContainerOrigin
            let inset = editor.textContainer?.lineFragmentPadding ?? 0
            let lineHeight = editor.font.map {
                editor.layoutManager?.defaultLineHeight(for: $0) ?? $0.pointSize
            } ?? NSFont.systemFontSize
            let height = min(editor.bounds.height, lineHeight)
            let rect = NSRect(x: origin.x + inset, y: editor.bounds.midY - height / 2,
                              width: 2, height: height)
            localRect = container.convert(rect, from: editor)
        } else {
            guard screenRect.height > 0 else {
                return nil
            }
            localRect = container.convert(window.convertFromScreen(screenRect), from: nil)
        }
        return CGRect(x: localRect.minX, y: localRect.minY, width: 2, height: localRect.height)
    }

    private func forwardMouse(_ event: NSEvent, to recognizers: [NSGestureRecognizer]) {
        // Synthetic AppKit events do not drive recognizers in this offscreen
        // window. Release only the recognizers that received the press.
        for recognizer in recognizers {
            switch event.type {
            case .leftMouseDown: recognizer.mouseDown(with: event)
            case .rightMouseDown: recognizer.rightMouseDown(with: event)
            case .otherMouseDown: recognizer.otherMouseDown(with: event)
            case .leftMouseUp: recognizer.mouseUp(with: event)
            case .rightMouseUp: recognizer.rightMouseUp(with: event)
            case .otherMouseUp: recognizer.otherMouseUp(with: event)
            default: break
            }
        }
    }

    private func modifierFlags(from encoded: UInt32) -> NSEvent.ModifierFlags {
        var result: NSEvent.ModifierFlags = []
        if encoded & 1 != 0 { result.insert(.shift) }
        if encoded & 2 != 0 { result.insert(.control) }
        if encoded & 4 != 0 { result.insert(.option) }
        if encoded & 8 != 0 { result.insert(.command) }
        if encoded & 16 != 0 { result.insert(.capsLock) }
        return result
    }
}
