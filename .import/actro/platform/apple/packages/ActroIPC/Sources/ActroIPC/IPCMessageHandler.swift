//
//  IPCMessageHandler.swift
//  Actro
//
//  High-level IPC message handling with async/await support.
//

import Foundation
import SwiftProtobuf

// MARK: - IPC Connection (Actro side)

/// Manages IPC connection to a Helper process
public class IPCConnection {
    private let server: IPCServer
    private let tabId: String
    private var pendingRequests: [UInt32: CheckedContinuation<Actro_Ipc_IPCMessage, Error>] = [:]
    private var nextRequestId: UInt32 = 1
    private let lock = NSLock()

    public var onSurfaceUpdated: ((UInt32) -> Void)?
    public var onTitleChanged: ((String) -> Void)?
    public var onNavigationRequest: ((String, Bool) -> Void)?
    public var onConsoleLog: ((String, String, String) -> Void)?
    public var onCrash: ((String) -> Void)?
    public var onDisconnected: (() -> Void)?

    public init(tabId: String) {
        self.tabId = tabId
        self.server = IPCServer(socketPath: IPCSocketPath.forTab(tabId))

        server.onMessage = { [weak self] data in
            self?.handleMessage(data)
        }

        server.onDisconnected = { [weak self] in
            self?.onDisconnected?()
        }
    }

    public var socketPath: String {
        IPCSocketPath.forTab(tabId)
    }

    /// Start listening for Helper connection
    public func start() throws {
        try server.start()
    }

    /// Stop the connection
    public func stop() {
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
    public func loadContent(
        type: String,
        content: String,
        backgroundColor: String,
        tabId: String = "",
        title: String = "",
        leftPanelVisible: Bool = true,
        rightPanelVisible: Bool = false,
        reloadKey: Int = 0,
        contractName: String? = nil,
        contractVersion: Int? = nil,
        permissions: [String]? = nil
    ) async throws -> Bool {
        var msg = Actro_Ipc_IPCMessage()
        msg.loadContent = Actro_Ipc_LoadContentRequest()
        msg.loadContent.contentType = type
        msg.loadContent.content = content
        msg.loadContent.backgroundColor = backgroundColor
        msg.loadContent.tabID = tabId
        msg.loadContent.title = title
        msg.loadContent.leftPanelVisible = leftPanelVisible
        msg.loadContent.rightPanelVisible = rightPanelVisible
        msg.loadContent.reloadKey = Int32(reloadKey)

        if let name = contractName {
            msg.loadContent.contractName = name
        }
        if let version = contractVersion {
            msg.loadContent.contractVersion = Int32(version)
        }
        if let perms = permissions {
            msg.loadContent.permissions = perms
        }

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
    public func updateViewport(width: Double, height: Double) throws {
        var msg = Actro_Ipc_IPCMessage()
        msg.updateViewport = Actro_Ipc_UpdateViewportRequest()
        msg.updateViewport.width = width
        msg.updateViewport.height = height

        try sendOneWay(msg)
    }

    /// Send input event
    public func sendInputEvent(_ event: Actro_Ipc_InputEvent) throws {
        var msg = Actro_Ipc_IPCMessage()
        msg.inputEvent = event

        try sendOneWay(msg)
    }

    /// Execute JavaScript
    public func executeJS(_ code: String) async throws -> String {
        var msg = Actro_Ipc_IPCMessage()
        msg.executeJs = Actro_Ipc_ExecuteJSRequest()
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
    public func getSurfaceID() async throws -> UInt32 {
        var msg = Actro_Ipc_IPCMessage()
        msg.getSurfaceID = Actro_Ipc_GetSurfaceIDRequest()

        let response = try await sendRequest(msg)
        guard case .surfaceIDResponse(let resp) = response.payload else {
            throw IPCError.unexpectedResponse
        }
        return resp.surfaceID
    }

    /// Get metrics
    public func getMetrics() async throws -> Actro_Ipc_MetricsResponse {
        var msg = Actro_Ipc_IPCMessage()
        msg.getMetrics = Actro_Ipc_GetMetricsRequest()

        let response = try await sendRequest(msg)
        guard case .metricsResponse(let resp) = response.payload else {
            throw IPCError.unexpectedResponse
        }
        return resp
    }

    /// Terminate Helper
    public func terminate(reason: String) {
        var msg = Actro_Ipc_IPCMessage()
        msg.terminate = Actro_Ipc_TerminateRequest()
        msg.terminate.reason = reason

        try? sendOneWay(msg)
    }

    // MARK: - Private

    private func sendRequest(_ message: Actro_Ipc_IPCMessage) async throws -> Actro_Ipc_IPCMessage {
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

    private func sendOneWay(_ message: Actro_Ipc_IPCMessage) throws {
        let data = try message.serializedData()
        try server.send(data)
    }

    private func handleMessage(_ data: Data) {
        do {
            let msg = try Actro_Ipc_IPCMessage(serializedData: data)

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

/// Manages IPC connection from Helper to Actro
public class IPCHelperConnection {
    private let client: IPCClient
    private let tabId: String

    public var onLoadContent: ((Actro_Ipc_LoadContentRequest, @escaping (Bool, String?) -> Void) -> Void)?
    public var onUpdateViewport: ((Double, Double) -> Void)?
    public var onInputEvent: ((Actro_Ipc_InputEvent) -> Void)?
    public var onExecuteJS: ((String, @escaping (Bool, String?, String?) -> Void) -> Void)?
    public var onGetSurfaceID: ((@escaping (UInt32) -> Void) -> Void)?
    public var onGetMetrics: ((@escaping (Actro_Ipc_MetricsResponse) -> Void) -> Void)?
    public var onTerminate: ((String) -> Void)?
    public var onDisconnected: (() -> Void)?

    public init(tabId: String, socketPath: String) {
        self.tabId = tabId
        self.client = IPCClient(socketPath: socketPath)

        client.onMessage = { [weak self] data in
            self?.handleMessage(data)
        }

        client.onDisconnected = { [weak self] in
            self?.onDisconnected?()
        }
    }

    /// Connect to Actro
    public func connect() throws {
        try client.connect()
    }

    /// Disconnect
    public func disconnect() {
        client.disconnect()
    }

    // MARK: - Send Messages to Actro

    /// Send ready notification
    public func sendReady(surfaceId: UInt32, width: UInt32, height: UInt32) throws {
        var msg = Actro_Ipc_IPCMessage()
        msg.ready = Actro_Ipc_ReadyResponse()
        msg.ready.surfaceID = surfaceId
        msg.ready.width = width
        msg.ready.height = height

        try send(msg)
    }

    /// Send surface updated notification
    public func sendSurfaceUpdated(surfaceId: UInt32) throws {
        var msg = Actro_Ipc_IPCMessage()
        msg.surfaceUpdated = Actro_Ipc_SurfaceUpdated()
        msg.surfaceUpdated.surfaceID = surfaceId

        try send(msg)
    }

    /// Send title changed notification
    public func sendTitleChanged(_ title: String) throws {
        var msg = Actro_Ipc_IPCMessage()
        msg.titleChanged = Actro_Ipc_TitleChanged()
        msg.titleChanged.title = title

        try send(msg)
    }

    /// Send navigation request
    public func sendNavigationRequest(url: String, newTab: Bool) throws {
        var msg = Actro_Ipc_IPCMessage()
        msg.navigationRequest = Actro_Ipc_NavigationRequest()
        msg.navigationRequest.url = url
        msg.navigationRequest.newTab = newTab

        try send(msg)
    }

    /// Send console log
    public func sendConsoleLog(level: String, message: String, source: String) throws {
        var msg = Actro_Ipc_IPCMessage()
        msg.consoleLog = Actro_Ipc_ConsoleLog()
        msg.consoleLog.level = level
        msg.consoleLog.message = message
        msg.consoleLog.source = source

        try send(msg)
    }

    /// Send crash report
    public func sendCrash(reason: String) throws {
        var msg = Actro_Ipc_IPCMessage()
        msg.crashReport = Actro_Ipc_CrashReport()
        msg.crashReport.reason = reason

        try send(msg)
    }

    // MARK: - Private

    private func send(_ message: Actro_Ipc_IPCMessage) throws {
        let data = try message.serializedData()
        try client.send(data)
    }

    private func sendResponse(requestId: UInt32, _ message: Actro_Ipc_IPCMessage) throws {
        var msg = message
        msg.id = requestId
        try send(msg)
    }

    private func handleMessage(_ data: Data) {
        do {
            let msg = try Actro_Ipc_IPCMessage(serializedData: data)
            let requestId = msg.id

            switch msg.payload {
            case .loadContent(let req):
                onLoadContent?(req) { [weak self] success, error in
                    var resp = Actro_Ipc_IPCMessage()
                    resp.loadContentResponse = Actro_Ipc_LoadContentResponse()
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
                    var resp = Actro_Ipc_IPCMessage()
                    resp.executeJsResponse = Actro_Ipc_ExecuteJSResponse()
                    resp.executeJsResponse.requestID = requestId
                    resp.executeJsResponse.success = success
                    resp.executeJsResponse.result = result ?? ""
                    resp.executeJsResponse.errorMessage = error ?? ""
                    try? self?.sendResponse(requestId: requestId, resp)
                }

            case .getSurfaceID:
                onGetSurfaceID? { [weak self] surfaceId in
                    var resp = Actro_Ipc_IPCMessage()
                    resp.surfaceIDResponse = Actro_Ipc_SurfaceIDResponse()
                    resp.surfaceIDResponse.requestID = requestId
                    resp.surfaceIDResponse.surfaceID = surfaceId
                    try? self?.sendResponse(requestId: requestId, resp)
                }

            case .getMetrics:
                onGetMetrics? { [weak self] metrics in
                    var resp = Actro_Ipc_IPCMessage()
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

public enum IPCMessageError: Error, LocalizedError {
    case loadFailed(String)
    case jsFailed(String)
    case unexpectedResponse

    public var errorDescription: String? {
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
