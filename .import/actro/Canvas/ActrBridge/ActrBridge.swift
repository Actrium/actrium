import Foundation
import Actr
import SwiftProtobuf
import ActrBindings
import OSLog

/// React Native bridge for Actr High Level API
/// This class provides a bridge between React Native and the Actr Swift SDK,
/// exposing the high-level API (ActrSystem, ActrNode, ActrRef) to JavaScript.
@objc(ActrBridge)
@preconcurrency
public class ActrBridge: NSObject, @unchecked Sendable {
    // MARK: - Properties
    
    private var connectionStatus: ConnectionStatus = .disconnected
    private var errorMessage: String?
    var streamMessages: [String] = []
    var textMessages: [String] = []
    
    private var actrSystem: ActrSystem?
    private var actrNode: ActrNode?
    private var actr: ActrRef?
    private var workload: LocalFileServiceWorkload<LocalFileServiceHandlerImpl>?
    
    private let queue = DispatchQueue(label: "com.actr.bridge.queue")
    
    // MARK: - Connection Status
    
    enum ConnectionStatus {
        case disconnected
        case initializing
        case connected
        case error

        var description: String {
            switch self {
            case .disconnected: return "Disconnected"
            case .initializing: return "Initializing..."
            case .connected: return "Connected"
            case .error: return "Error"
            }
        }
    }
    
    /// Prepare client-side stream handling
    /// - Parameter expectedCount: Number of messages expected from the server
    /// - Returns: PrepareStreamResponse containing the result
    func prepareClientStream(expectedCount: Int32) async throws -> DataStreamApp_PrepareStreamResponse {
        guard connectionStatus == .connected else {
            throw ActrError.StateError(msg: "Not connected to server")
        }
        
        guard let actr = actr else {
            throw ActrError.StateError(msg: "Actor reference not available")
        }
        
        var request = DataStreamApp_PrepareClientStreamRequest()
        request.streamID = "stream-\(UUID().uuidString)"
        request.expectedCount = UInt32(clamping: expectedCount)
        
        logger.info("Preparing client stream: stream_id=\(request.streamID), expected_count=\(request.expectedCount)")
        
        do {
            let response = try await actr.call(request)
            logger.info("Prepare client stream completed: ready=\(response.ready)")
            return response
        } catch let actrError as ActrError {
            logger.error("Prepare client stream error: \(actrError.localizedDescription)")
            throw actrError
        } catch {
            logger.error("Prepare client stream error: \(error.localizedDescription)")
            throw error
        }
    }
}

// MARK: - Core Actr Operations

extension ActrBridge {
    /// Initialize the Actr system and its underlying actors
    private func initialize(path: String) async throws {
        connectionStatus = .initializing
        errorMessage = nil
        logger.info("Initializing ActrBridge...")

        do {
            let result = try await Task.detached(priority: .userInitiated) { () throws
                -> (ActrSystem, ActrNode, ActrRef, LocalFileServiceWorkload<LocalFileServiceHandlerImpl>) in
                let system = try await ActrSystem.from(tomlConfig: path)
                let handler = await LocalFileServiceHandlerImpl { [weak self] message in
                    Task { @MainActor in
                        self?.streamMessages.append(message)
                    }
                }
                let workload = LocalFileServiceWorkload(handler: handler)
                let node = try system.spawn(workload: workload)
                let actr = try await node.start()
                return (system, node, actr, workload)
            }.value

            let (system, node, actr, workload) = result
            actrSystem = system
            actrNode = node
            self.actr = actr
            self.workload = workload
            
            // Wait a bit for the signaling client to establish connection
            logger.info("Waiting for signaling client to connect...")
            try await Task.sleep(nanoseconds: 500_000_000) // 0.5 seconds

            connectionStatus = .connected
            logger.info("ActrBridge initialized successfully")
        } catch let actrError as ActrError {
            errorMessage = "Actr Error: \(actrError.localizedDescription)"
            connectionStatus = .error
            logger.error("Failed to initialize: \(actrError.localizedDescription)")
            throw actrError
        } catch {
            errorMessage = "Initialization Error: \(error.localizedDescription)"
            connectionStatus = .error
            logger.error("Failed to initialize: \(error.localizedDescription)")
            throw error
        }
    }

    /// Shutdown the service and cleanup resources
    private func shutdown() async {
        logger.info("Shutting down ActrBridge...")

        if let actr {
            await actr.stop()
        }

        actr = nil
        actrNode = nil
        actrSystem = nil
        workload = nil

        connectionStatus = .disconnected
        errorMessage = nil
    }
    
    /// Send an echo RPC request
    private func sendEchoMessage(_ message: String) async throws -> String {
        guard connectionStatus == .connected else {
            let error = ActrError.StateError(msg: "Not connected to server")
            return "Error: \(error.localizedDescription)"
        }

        guard let actr = actr else {
            let error = ActrError.StateError(msg: "Actor reference not available")
            return "Error: \(error.localizedDescription)"
        }

        do {
            var request = DataStreamApp_PrepareClientStreamRequest()
            request.streamID = "text-\(UUID().uuidString)"
            let response = try await actr.call(request)

            return response.message
        } catch let actrError as ActrError {
            throw actrError
        } catch {
            throw error
        }
    }
}

// MARK: - React Native Bridge Methods

extension ActrBridge {

// MARK: - Helper Methods
    
    private nonisolated func formatActrError(_ error: ActrError) -> String {
        return error.errorDescription ?? ""
    }
    
    private nonisolated func serializeRealm(_ realm: Realm) -> [String: Any] {
        // Realm serialization - adjust based on actual Realm structure
        return ["id": "\(realm)"]
    }

    /// Initialize ActrService
    @objc
    public func initializeService(
        _ path: String,
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        Task { @MainActor in
            do {
                try await self.initialize(path: path)
                resolve(["status": "connected"])
            } catch let error as ActrError {
                reject("ACTR_ERROR", self.formatActrError(error), error)
            } catch {
                reject("UNKNOWN_ERROR", error.localizedDescription, error)
            }
        }
    }
    
    /// Shutdown ActrService
    @objc
    public func shutdownService(
        _ resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        Task { @MainActor in
            await self.shutdown()
            resolve(["status": "disconnected"])
        }
    }
    
    /// Send echo message through ActrService
    @objc
    public func sendEcho(
        _ message: String,
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        Task { @MainActor in
            guard connectionStatus == .connected else {
                reject("NOT_CONNECTED", "ActrBridge not initialized", nil)
                return
            }
            
            do {
                let reply = try await self.sendEchoMessage(message)
                resolve(["reply": reply])
            } catch let error as ActrError {
                reject("ACTR_ERROR", self.formatActrError(error), error)
            } catch {
                reject("UNKNOWN_ERROR", error.localizedDescription, error)
            }
        }
    }
    
    /// Send voice stream to ActrService (max 60s)
    /// - expectedCount: 语音分片数量（建议根据采样率和分片时长计算）
    /// - audioData: 语音PCM数据（或分片数组）
    @objc
    public func sendVoiceStream(
        _ expectedCount: Int,
        audioData: [NSData],
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        Task { @MainActor in
            guard connectionStatus == .connected else {
                reject("NOT_CONNECTED", "ActrBridge not initialized", nil)
                return
            }
            if audioData.count > 0 && expectedCount > 0 && audioData.count <= expectedCount {
                do {
                    let response = try await self.prepareClientStream(expectedCount: Int32(expectedCount))
                    // TODO: 发送audioData分片到服务端（可扩展为逐片发送）
                    // 当前仅返回准备结果
                    resolve([
                        "ready": response.ready,
                        "message": response.message
                    ])
                } catch let error as ActrError {
                    reject("ACTR_ERROR", self.formatActrError(error), error)
                } catch {
                    reject("UNKNOWN_ERROR", error.localizedDescription, error)
                }
            } else {
                reject("INVALID_INPUT", "语音数据或分片数量无效", nil)
            }
        }
    }
    
    /// Get ActrService connection status
    @objc
    public func getServiceStatus(
        _ resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        Task { @MainActor in
            let status = connectionStatus.description
            let result: [String: Any] = [
                "status": status,
                "errorMessage": errorMessage ?? NSNull()
            ]
            resolve(result)
        }
    }
}

// MARK: - Simple stream handler that logs incoming chunks.

private actor StreamPrintCallback: Actr.DataStreamCallback {
    private let streamId: String
    private let onMessage: (@Sendable (String) -> Void)?
    
    init(streamId: String, onMessage: (@Sendable (String) -> Void)?) {
        self.streamId = streamId
        self.onMessage = onMessage
    }
    
    func onStream(chunk: Actr.DataStream, sender: ActrId) async throws {
        let senderRepr = "\(sender.type.toStringRepr())#\(sender.serialNumber)"
        let payloadText = String(data: chunk.payload, encoding: .utf8) ?? "<non-utf8 data>"
        let message = "stream_id=\(streamId), seq=\(chunk.sequence), bytes=\(chunk.payload.count), sender=\(senderRepr), text=\"\(payloadText)\""
        await logger.info("📥 DataStream chunk: \(message)")
        onMessage?(message)
        // --- 新增: 通过事件推送到RN ---
        if let bridge = ActrBridge.shared {
            bridge.sendStreamEventToRN(message: message)
        }
    }
}

// MARK: - Implementation of LocalFileServiceHandler

public actor LocalFileServiceHandlerImpl: LocalFileServiceHandler {
    private var streamCallbacks: [String: Actr.DataStreamCallback] = [:]
    private let onMessage: (@Sendable (String) -> Void)?
    
    public init(onMessage: (@Sendable (String) -> Void)? = nil) {
        self.onMessage = onMessage
    }
    
    /// RPC method: PrepareClientStream
    public func prepareClientStream(
        req: DataStreamApp_PrepareClientStreamRequest,
        ctx: Actr.Context
    ) async throws -> DataStreamApp_PrepareStreamResponse {
        await logger.info("📡 Received PrepareClientStream request: stream_id=\(req.streamID), expected_count=\(req.expectedCount)")
        
        do {
            try await ensureStreamHandler(streamId: req.streamID, ctx: ctx)
            let registerResponse = try await registerRemoteStream(
                streamId: req.streamID,
                messageCount: Int32(clamping: req.expectedCount),
                ctx: ctx
            )
            
            var response = await DataStreamApp_PrepareStreamResponse()
            response.ready = registerResponse.success
            response.message = registerResponse.message
            return response
        } catch {
            await logger.error("❌ PrepareClientStream failed: \(error.localizedDescription)")
            var response = await DataStreamApp_PrepareStreamResponse()
            response.ready = false
            response.message = "Prepare client stream failed: \(error.localizedDescription)"
            return response
        }
    }
    
    private func ensureStreamHandler(streamId: String, ctx: Actr.Context) async throws {
        if streamCallbacks[streamId] != nil {
            return
        }
        
        let callback = StreamPrintCallback(streamId: streamId, onMessage: onMessage)
        streamCallbacks[streamId] = callback
        try await ctx.registerStream(streamId: streamId, callback: callback)
    }
    
    private func registerRemoteStream(
        streamId: String,
        messageCount: Int32,
        ctx: Actr.Context
    ) async throws -> StreamServer_RegisterStreamResponse {
        let targetType = try ActrType.fromStringRepr("acme+StreamEchoServer")
        let targetId = try await ctx.discover(targetType: targetType)
        
        var request = await StreamServer_RegisterStreamRequest()
        request.streamID = streamId
        request.messageCount = messageCount
        
        let payload = try request.serializedData()
        let responseData = try await ctx.call(
            target: targetId,
            routeKey: "stream_server.StreamServer.RegisterStream",
            payload: payload
        )
        return try StreamServer_RegisterStreamResponse(serializedBytes: responseData)
    }
}

// MARK: - LocalFileServiceWorkload Extension

extension LocalFileServiceWorkload: Workload where T == LocalFileServiceHandlerImpl {
    public func onStart(ctx _: Actr.Context) async throws {}
    public func onStop(ctx _: Actr.Context) async throws {}

    public func dispatch(ctx: Actr.Context, envelope: Actr.RpcEnvelope) async throws -> Data {
        return try await __dispatch(ctx: ctx, envelope: envelope)
    }
}

// Global logger for the handlers to use
private let logger = Logger(subsystem: "io.actr.App", category: "ActrBridge")

// MARK: - React Native Bridge Support

/// Type alias for React Native promise callbacks
public typealias RCTPromiseResolveBlock = (Any?) -> Void
public typealias RCTPromiseRejectBlock = (String?, String?, Error?) -> Void

// --- ActrBridge 单例与事件推送 ---
extension ActrBridge {
    @objc static let shared = ActrBridge()
    
    @objc func sendStreamEventToRN(message: String) {
        DispatchQueue.main.async {
            if let bridge = self.value(forKey: "bridge") as? RCTBridge {
                let eventEmitter = bridge.module(forName: "RCTDeviceEventEmitter") as? RCTEventEmitter
                eventEmitter?.sendEvent(withName: "ActrStreamMessage", body: message)
            }
        }
    }
    
    @objc public static func requiresMainQueueSetup() -> Bool {
        return true
    }
    
    @objc public func supportedEvents() -> [String] {
        return ["ActrStreamMessage"]
    }
}
