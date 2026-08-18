import ActroIPC
import AppKit
import Darwin
import Foundation
import QuartzCore

/// Entry-point and lifecycle owner for a manually spawned Renderer Helper.
final class RendererProcessController {
    static let rendererIdArgument = "--renderer-id"
    static let fingerprintArgument = "--fingerprint"
    static let ipcStdinArgument = "--ipc-stdin"
    static let hostPidArgument = "--host-pid"

    static var isRendererProcess: Bool {
        ProcessInfo.processInfo.arguments.contains(rendererIdArgument)
    }

    /// Returns `true` when the caller is running in the Renderer process. In
    /// that case the legacy distributed-notification fallback must not run,
    /// even if the Broker has already disconnected.
    @discardableResult
    static func sendHostEvent(name: String, userInfo: [String: Any] = [:]) -> Bool {
        guard isRendererProcess else { return false }
        guard JSONSerialization.isValidJSONObject(userInfo),
              let data = try? JSONSerialization.data(withJSONObject: userInfo),
              let json = String(data: data, encoding: .utf8),
              let controller = RendererProcessLifetime.shared.controller else {
            return true
        }
        do {
            try controller.connection.sendHostEvent(name: name, userInfoJSON: json)
        } catch {
            NSLog("ActroRenderer host event failed: %@ (%@)", name, error.localizedDescription)
        }
        return true
    }

    @discardableResult
    static func sendConsoleLog(level: String, source: String, message: String) -> Bool {
        guard isRendererProcess else { return false }
        guard let controller = RendererProcessLifetime.shared.controller else {
            return true
        }
        do {
            try controller.connection.sendConsoleLog(
                level: level,
                message: message,
                source: source
            )
        } catch {
            NSLog("ActroRenderer console forwarding failed: %@", error.localizedDescription)
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
            fputs("ActroRenderer: missing required Broker arguments\n", stderr)
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
            fputs("ActroRenderer: Broker connection failed: \(error)\n", stderr)
            exit(70)
        }

        NSLog(
            "ActroRenderer ready: renderer=%@ fingerprint=%@ pid=%d host=%d",
            arguments.rendererId,
            arguments.fingerprint,
            ProcessInfo.processInfo.processIdentifier,
            arguments.hostPid
        )
        application.run()
        exit(0)
    }

    private func configureIPC() {
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
        _ request: Actro_Ipc_LoadContentRequest,
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
    }

    private func destroyTab(_ tabId: String) {
        surfaces.removeValue(forKey: tabId)?.close()
        reactNativeHost.destroyRendererEntry(tabId: tabId)
    }

    private func terminate(reason: String) {
        NSLog("ActroRenderer terminating: %@", reason)
        let tabIds = Array(surfaces.keys)
        for tabId in tabIds {
            destroyTab(tabId)
        }
        connection.disconnect()
        NSApplication.shared.terminate(nil)
    }

    private static func metrics() -> Actro_Ipc_MetricsResponse {
        var response = Actro_Ipc_MetricsResponse()
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
private final class RendererProcessLifetime {
    static let shared = RendererProcessLifetime()
    var controller: RendererProcessController?
}

/// One Tab root and its remotely hosted CoreAnimation context.
private final class RendererTabSurface {
    let tabId: String
    let entry: TabViewControllerEntry
    let container: ResizableContainerView
    let window: NSWindow
    let context: RendererCAContext

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

        window = NSWindow(
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
        context.layer = container.layer
        commit()
    }

    func resize(to size: CGSize) {
        guard size.width > 0, size.height > 0 else { return }
        window.setContentSize(size)
        container.applyHostSize(size)
        container.layer?.frame = CGRect(origin: .zero, size: size)
    }

    func commit() {
        context.layer = container.layer
        container.needsLayout = true
        container.layoutSubtreeIfNeeded()
        CATransaction.flush()
    }

    func close() {
        context.layer = nil
        window.contentView = nil
        window.close()
    }

    func dispatch(_ input: Actro_Ipc_InputEvent) {
        let modifiers = modifierFlags(from: input.modifiers)
        let location = NSPoint(x: input.x, y: input.y)
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

            if input.type == .mouseDown {
                let hitView = window.contentView?.hitTest(location)
                window.makeFirstResponder(hitView ?? container)
            }
            window.sendEvent(event)

        case .scroll:
            guard let cgEvent = CGEvent(
                scrollWheelEvent2Source: nil,
                units: .pixel,
                wheelCount: 2,
                wheel1: Int32(input.scrollDeltaY.rounded()),
                wheel2: Int32(input.scrollDeltaX.rounded()),
                wheel3: 0
            ) else {
                return
            }
            cgEvent.location = location
            cgEvent.flags = cgEventFlags(from: input.modifiers)
            if let event = NSEvent(cgEvent: cgEvent) {
                window.sendEvent(event)
            }

        case .keyDown, .keyUp:
            guard let event = NSEvent.keyEvent(
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
            window.sendEvent(event)

        case .mouseEntered, .mouseExited, .UNRECOGNIZED(_):
            break
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

    private func cgEventFlags(from encoded: UInt32) -> CGEventFlags {
        var result: CGEventFlags = []
        if encoded & 1 != 0 { result.insert(.maskShift) }
        if encoded & 2 != 0 { result.insert(.maskControl) }
        if encoded & 4 != 0 { result.insert(.maskAlternate) }
        if encoded & 8 != 0 { result.insert(.maskCommand) }
        if encoded & 16 != 0 { result.insert(.maskAlphaShift) }
        return result
    }
}

/// Dynamic wrapper for the private CAContext used by Chromium/WebKit style
/// cross-process layer hosting.
private final class RendererCAContext {
    private let object: NSObject

    var contextId: UInt32 {
        if let number = object.value(forKey: "contextId") as? NSNumber {
            return number.uint32Value
        }
        return object.value(forKey: "contextId") as? UInt32 ?? 0
    }

    var layer: CALayer? {
        get {
            object.value(forKey: "layer") as? CALayer
        }
        set {
            object.setValue(newValue, forKey: "layer")
        }
    }

    init?() {
        guard let contextClass = NSClassFromString("CAContext") else {
            return nil
        }

        let options: [String: Any] = [
            "colorSpace": CGColorSpaceCreateDeviceRGB(),
            "displayId": CGMainDisplayID()
        ]

        let connectionSelector = NSSelectorFromString("contextWithCGSConnection:options:")
        if contextClass.responds(to: connectionSelector),
           let symbol = dlsym(dlopen(nil, RTLD_LAZY), "CGSMainConnectionID") {
            typealias ConnectionFunction = @convention(c) () -> UInt32
            let connection = unsafeBitCast(symbol, to: ConnectionFunction.self)()
            let implementation = contextClass.method(for: connectionSelector)
            typealias Factory = @convention(c) (
                AnyClass,
                Selector,
                UInt32,
                [String: Any]
            ) -> NSObject?
            if let created = unsafeBitCast(implementation, to: Factory.self)(
                contextClass,
                connectionSelector,
                connection,
                options
            ) {
                object = created
                return
            }
        }

        let remoteSelector = NSSelectorFromString("remoteContextWithOptions:")
        if contextClass.responds(to: remoteSelector) {
            let implementation = contextClass.method(for: remoteSelector)
            typealias Factory = @convention(c) (
                AnyClass,
                Selector,
                [String: Any]
            ) -> NSObject?
            if let created = unsafeBitCast(implementation, to: Factory.self)(
                contextClass,
                remoteSelector,
                options
            ) {
                object = created
                return
            }
        }

        return nil
    }
}
