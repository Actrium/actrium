//
//  IPCMessageHandler.swift
//  Canvas
//
//  High-level IPC message handling with async/await support.
//

import Foundation
import SwiftProtobuf

// MARK: - IPC Connection (Canvas side)

/// Manages IPC connection to a Helper process
class IPCConnection {
    private let server: IPCServer
    private let tabId: String
    private var pendingRequests: [UInt32: CheckedContinuation<Canvas_Ipc_IPCMessage, Error>] = [:]
    private var nextRequestId: UInt32 = 1
    private let lock = NSLock()

    var onSurfaceUpdated: ((UInt32) -> Void)?
    var onTitleChanged: ((String) -> Void)?
    var onNavigationRequest: ((String, Bool) -> Void)?
    var onConsoleLog: ((String, String, String) -> Void)?
    var onCrash: ((String) -> Void)?
    var onDisconnected: (() -> Void)?

    init(tabId: String) {
        self.tabId = tabId
        self.server = IPCServer(socketPath: IPCSocketPath.forTab(tabId))

        server.onMessage = { [weak self] data in
            self?.handleMessage(data)
        }

        server.onDisconnected = { [weak self] in
            self?.onDisconnected?()
        }
    }

    var socketPath: String {
        IPCSocketPath.forTab(tabId)
    }

    /// Start listening for Helper connection
    func start() throws {
        try server.start()
    }

    /// Stop the connection
    func stop() {
        server.stop()

        // Cancel all pending requests
        lock.lock()
        let pending = pendingRequests
        pendingRequests.removeAll()
        lock.unlock()

        for (_, continuation) in pending {
            continuation.resume(throwing: IPCError.connectionClosed)
        }
    }

    // MARK: - Send Requests

    /// Load content in the Helper
    func loadContent(type: String, content: String, backgroundColor: String) async throws -> Bool {
        var msg = Canvas_Ipc_IPCMessage()
        msg.loadContent = Canvas_Ipc_LoadContentRequest()
        msg.loadContent.contentType = type
        msg.loadContent.content = content
        msg.loadContent.backgroundColor = backgroundColor

        let response = try await sendRequest(msg)
        guard case .loadContentResponse(let resp) = response.payload else {
            throw IPCError.unexpectedResponse
        }
        if !resp.success {
            throw IPCMessageError.loadFailed(resp.errorMessage)
        }
        return true
    }

    /// Update viewport size
    func updateViewport(width: Double, height: Double) throws {
        var msg = Canvas_Ipc_IPCMessage()
        msg.updateViewport = Canvas_Ipc_UpdateViewportRequest()
        msg.updateViewport.width = width
        msg.updateViewport.height = height

        try sendOneWay(msg)
    }

    /// Send input event
    func sendInputEvent(_ event: Canvas_Ipc_InputEvent) throws {
        var msg = Canvas_Ipc_IPCMessage()
        msg.inputEvent = event

        try sendOneWay(msg)
    }

    /// Execute JavaScript
    func executeJS(_ code: String) async throws -> String {
        var msg = Canvas_Ipc_IPCMessage()
        msg.executeJs = Canvas_Ipc_ExecuteJSRequest()
        msg.executeJs.code = code

        let response = try await sendRequest(msg)
        guard case .executeJsResponse(let resp) = response.payload else {
            throw IPCError.unexpectedResponse
        }
        if !resp.success {
            throw IPCMessageError.jsFailed(resp.errorMessage)
        }
        return resp.result
    }

    /// Get current IOSurface ID
    func getSurfaceID() async throws -> UInt32 {
        var msg = Canvas_Ipc_IPCMessage()
        msg.getSurfaceID = Canvas_Ipc_GetSurfaceIDRequest()

        let response = try await sendRequest(msg)
        guard case .surfaceIDResponse(let resp) = response.payload else {
            throw IPCError.unexpectedResponse
        }
        return resp.surfaceID
    }

    /// Get metrics
    func getMetrics() async throws -> Canvas_Ipc_MetricsResponse {
        var msg = Canvas_Ipc_IPCMessage()
        msg.getMetrics = Canvas_Ipc_GetMetricsRequest()

        let response = try await sendRequest(msg)
        guard case .metricsResponse(let resp) = response.payload else {
            throw IPCError.unexpectedResponse
        }
        return resp
    }

    /// Terminate Helper
    func terminate(reason: String) {
        var msg = Canvas_Ipc_IPCMessage()
        msg.terminate = Canvas_Ipc_TerminateRequest()
        msg.terminate.reason = reason

        try? sendOneWay(msg)
    }

    // MARK: - Private

    private func sendRequest(_ message: Canvas_Ipc_IPCMessage) async throws -> Canvas_Ipc_IPCMessage {
        var msg = message

        lock.lock()
        let requestId = nextRequestId
        nextRequestId += 1
        lock.unlock()

        msg.id = requestId

        return try await withCheckedThrowingContinuation { continuation in
            lock.lock()
            pendingRequests[requestId] = continuation
            lock.unlock()

            do {
                let data = try msg.serializedData()
                try server.send(data)
            } catch {
                lock.lock()
                pendingRequests.removeValue(forKey: requestId)
                lock.unlock()
                continuation.resume(throwing: error)
            }

            // Timeout after 10 seconds
            DispatchQueue.global().asyncAfter(deadline: .now() + 10) { [weak self] in
                self?.lock.lock()
                if let cont = self?.pendingRequests.removeValue(forKey: requestId) {
                    self?.lock.unlock()
                    cont.resume(throwing: IPCError.timeout)
                } else {
                    self?.lock.unlock()
                }
            }
        }
    }

    private func sendOneWay(_ message: Canvas_Ipc_IPCMessage) throws {
        let data = try message.serializedData()
        try server.send(data)
    }

    private func handleMessage(_ data: Data) {
        do {
            let msg = try Canvas_Ipc_IPCMessage(serializedData: data)

            // Check if it's a response to a pending request
            if msg.id != 0 {
                lock.lock()
                if let continuation = pendingRequests.removeValue(forKey: msg.id) {
                    lock.unlock()
                    continuation.resume(returning: msg)
                    return
                }
                lock.unlock()
            }

            // Handle push messages from Helper
            switch msg.payload {
            case .ready(let ready):
                NSLog("IPCConnection: Helper ready, surfaceId=\(ready.surfaceID)")
                onSurfaceUpdated?(ready.surfaceID)

            case .surfaceUpdated(let update):
                onSurfaceUpdated?(update.surfaceID)

            case .titleChanged(let title):
                onTitleChanged?(title.title)

            case .navigationRequest(let nav):
                onNavigationRequest?(nav.url, nav.newTab)

            case .consoleLog(let log):
                onConsoleLog?(log.level, log.message, log.source)

            case .crashReport(let crash):
                onCrash?(crash.reason)

            default:
                NSLog("IPCConnection: Unhandled message: \(msg)")
            }
        } catch {
            NSLog("IPCConnection: Failed to parse message: \(error)")
        }
    }
}

// MARK: - IPC Helper Connection (Helper side)

/// Manages IPC connection from Helper to Canvas
class IPCHelperConnection {
    private let client: IPCClient
    private let tabId: String

    var onLoadContent: ((Canvas_Ipc_LoadContentRequest, @escaping (Bool, String?) -> Void) -> Void)?
    var onUpdateViewport: ((Double, Double) -> Void)?
    var onInputEvent: ((Canvas_Ipc_InputEvent) -> Void)?
    var onExecuteJS: ((String, @escaping (Bool, String?, String?) -> Void) -> Void)?
    var onGetSurfaceID: ((@escaping (UInt32) -> Void) -> Void)?
    var onGetMetrics: ((@escaping (Canvas_Ipc_MetricsResponse) -> Void) -> Void)?
    var onTerminate: ((String) -> Void)?
    var onDisconnected: (() -> Void)?

    init(tabId: String, socketPath: String) {
        self.tabId = tabId
        self.client = IPCClient(socketPath: socketPath)

        client.onMessage = { [weak self] data in
            self?.handleMessage(data)
        }

        client.onDisconnected = { [weak self] in
            self?.onDisconnected?()
        }
    }

    /// Connect to Canvas
    func connect() throws {
        try client.connect()
    }

    /// Disconnect
    func disconnect() {
        client.disconnect()
    }

    // MARK: - Send Messages to Canvas

    /// Send ready notification
    func sendReady(surfaceId: UInt32, width: UInt32, height: UInt32) throws {
        var msg = Canvas_Ipc_IPCMessage()
        msg.ready = Canvas_Ipc_ReadyResponse()
        msg.ready.surfaceID = surfaceId
        msg.ready.width = width
        msg.ready.height = height

        try send(msg)
    }

    /// Send surface updated notification
    func sendSurfaceUpdated(surfaceId: UInt32) throws {
        var msg = Canvas_Ipc_IPCMessage()
        msg.surfaceUpdated = Canvas_Ipc_SurfaceUpdated()
        msg.surfaceUpdated.surfaceID = surfaceId

        try send(msg)
    }

    /// Send title changed notification
    func sendTitleChanged(_ title: String) throws {
        var msg = Canvas_Ipc_IPCMessage()
        msg.titleChanged = Canvas_Ipc_TitleChanged()
        msg.titleChanged.title = title

        try send(msg)
    }

    /// Send navigation request
    func sendNavigationRequest(url: String, newTab: Bool) throws {
        var msg = Canvas_Ipc_IPCMessage()
        msg.navigationRequest = Canvas_Ipc_NavigationRequest()
        msg.navigationRequest.url = url
        msg.navigationRequest.newTab = newTab

        try send(msg)
    }

    /// Send console log
    func sendConsoleLog(level: String, message: String, source: String) throws {
        var msg = Canvas_Ipc_IPCMessage()
        msg.consoleLog = Canvas_Ipc_ConsoleLog()
        msg.consoleLog.level = level
        msg.consoleLog.message = message
        msg.consoleLog.source = source

        try send(msg)
    }

    /// Send crash report
    func sendCrash(reason: String) throws {
        var msg = Canvas_Ipc_IPCMessage()
        msg.crashReport = Canvas_Ipc_CrashReport()
        msg.crashReport.reason = reason

        try send(msg)
    }

    // MARK: - Private

    private func send(_ message: Canvas_Ipc_IPCMessage) throws {
        let data = try message.serializedData()
        try client.send(data)
    }

    private func sendResponse(requestId: UInt32, _ message: Canvas_Ipc_IPCMessage) throws {
        var msg = message
        msg.id = requestId
        try send(msg)
    }

    private func handleMessage(_ data: Data) {
        do {
            let msg = try Canvas_Ipc_IPCMessage(serializedData: data)
            let requestId = msg.id

            switch msg.payload {
            case .loadContent(let req):
                onLoadContent?(req) { [weak self] success, error in
                    var resp = Canvas_Ipc_IPCMessage()
                    resp.loadContentResponse = Canvas_Ipc_LoadContentResponse()
                    resp.loadContentResponse.requestID = requestId
                    resp.loadContentResponse.success = success
                    resp.loadContentResponse.errorMessage = error ?? ""
                    try? self?.sendResponse(requestId: requestId, resp)
                }

            case .updateViewport(let req):
                onUpdateViewport?(req.width, req.height)

            case .inputEvent(let event):
                onInputEvent?(event)

            case .executeJs(let req):
                onExecuteJS?(req.code) { [weak self] success, result, error in
                    var resp = Canvas_Ipc_IPCMessage()
                    resp.executeJsResponse = Canvas_Ipc_ExecuteJSResponse()
                    resp.executeJsResponse.requestID = requestId
                    resp.executeJsResponse.success = success
                    resp.executeJsResponse.result = result ?? ""
                    resp.executeJsResponse.errorMessage = error ?? ""
                    try? self?.sendResponse(requestId: requestId, resp)
                }

            case .getSurfaceID:
                onGetSurfaceID? { [weak self] surfaceId in
                    var resp = Canvas_Ipc_IPCMessage()
                    resp.surfaceIDResponse = Canvas_Ipc_SurfaceIDResponse()
                    resp.surfaceIDResponse.requestID = requestId
                    resp.surfaceIDResponse.surfaceID = surfaceId
                    try? self?.sendResponse(requestId: requestId, resp)
                }

            case .getMetrics:
                onGetMetrics? { [weak self] metrics in
                    var resp = Canvas_Ipc_IPCMessage()
                    resp.metricsResponse = metrics
                    resp.metricsResponse.requestID = requestId
                    try? self?.sendResponse(requestId: requestId, resp)
                }

            case .terminate(let req):
                onTerminate?(req.reason)

            default:
                NSLog("IPCHelperConnection: Unhandled message: \(msg)")
            }
        } catch {
            NSLog("IPCHelperConnection: Failed to parse message: \(error)")
        }
    }
}

// MARK: - Additional Errors

enum IPCMessageError: Error, LocalizedError {
    case loadFailed(String)
    case jsFailed(String)
    case unexpectedResponse

    var errorDescription: String? {
        switch self {
        case .loadFailed(let msg): return "Load failed: \(msg)"
        case .jsFailed(let msg): return "JS execution failed: \(msg)"
        case .unexpectedResponse: return "Unexpected response"
        }
    }
}

extension IPCError {
    static var timeout: IPCError { .connectionClosed }
    static var unexpectedResponse: IPCError { .invalidFrameLength }
}
