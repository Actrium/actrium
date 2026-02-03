import Foundation
import Actr
import SwiftProtobuf
import ActrBindings
import OSLog
import React

/// React Native bridge for Actr High Level API
/// This class provides a bridge between React Native and the Actr Swift SDK,
/// exposing the high-level API (ActrSystem, ActrNode, ActrRef) to JavaScript.

@objc(ActrBridge)
@preconcurrency
public class ActrBridge: RCTEventEmitter, @unchecked Sendable {
    // MARK: - Properties
    
    private static weak var activeInstance: ActrBridge?
    
    public override init() {
        super.init()
        Self.activeInstance = self
    }

    @objc public static func shared() -> ActrBridge? {
        return activeInstance
    }
    
    private var connectionStatus: ConnectionStatus = .disconnected
    private var errorMessage: String?
    var streamMessages: [String] = []
    var textMessages: [String] = []
    
    private var actrSystem: ActrSystem?
    private var actrNode: ActrNode?
    private var actr: ActrRef?
    private var workload: AskLocalServiceWorkload<AskLocalServiceHandlerImpl>?
    
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
    /// - Returns: PrepareStreamResponse containing the result and the generated streamID
    func prepareClientStream() async throws -> (String) {
        guard connectionStatus == .connected else {
            throw ActrError.StateError(msg: "Not connected to server")
        }
        
        guard let actr = actr else {
            throw ActrError.StateError(msg: "Actor reference not available")
        }
        
        var request = DataStreamApp_VoiceChunkMessage()
        let streamID = "stream-\(UUID().uuidString)"
        request.streamID = streamID
        
        logger.info("Preparing client stream: stream_id=\(request.streamID)")
        
        do {
            let _ = try await actr.call(request)
            return (streamID)
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
                -> (ActrSystem, ActrNode, ActrRef, AskLocalServiceWorkload<AskLocalServiceHandlerImpl>) in
                let system = try await ActrSystem.from(tomlConfig: path)
                let handler = await AskLocalServiceHandlerImpl()
                let workload = AskLocalServiceWorkload(handler: handler)
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
    
    /// Send text message through ActrService
    @objc
    public func sendTextMessage(
        _ json: String,
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        Task { @MainActor in
            guard connectionStatus == .connected, let actr = self.actr else {
                reject("NOT_CONNECTED", "ActrBridge not initialized", nil)
                return
            }
            
            do {
                guard let data = json.data(using: .utf8),
                      let dict = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
                    reject("INVALID_INPUT", "Invalid JSON", nil)
                    return
                }

                var request = DataStreamApp_TextMessage()
                request.questionID = dict["text"] as? String ?? ""
                request.sessionID = dict["sessionId"] as? String ?? ""
                request.messageID = dict["messageId"] as? String ?? ""
                request.streamID = dict["streamId"] as? String ?? ""
                
                if request.questionID.isEmpty || request.sessionID.isEmpty || request.messageID.isEmpty {
                    reject("INVALID_INPUT", "Missing required fields (text, sessionId, messageId)", nil)
                    return
                }

                let response = try await actr.call(request)
                resolve([
                    "streamId": request.streamID,
                    "responseStreamId": response.streamID,
                    "status": response.status,
                    "message": response.message
                ])
            } catch let error as ActrError {
                reject("ACTR_ERROR", self.formatActrError(error), error)
            } catch {
                reject("UNKNOWN_ERROR", error.localizedDescription, error)
            }
        }
    }
    
    /// Start voice stream (was sendVoiceStream)
    /// - Parameter json: JSON string containing 'expectedCount'
    @objc
    public func startVoice(
        _ json: String,
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        Task { @MainActor in
            guard connectionStatus == .connected, let actr = self.actr else {
                reject("NOT_CONNECTED", "ActrBridge not initialized", nil)
                return
            }
            
            do {
                guard let data = json.data(using: .utf8),
                      let dict = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
                    reject("INVALID_INPUT", "Invalid JSON", nil)
                    return
                }

                var request = DataStreamApp_StartVoiceMessage()
                request.streamID = dict["streamId"] as? String ?? ""
                request.messageID = dict["messageId"] as? String ?? ""
                request.sessionID = dict["sessionId"] as? String ?? ""
                
                if request.streamID.isEmpty || request.messageID.isEmpty || request.sessionID.isEmpty {
                    reject("INVALID_INPUT", "Missing required IDs (streamId, messageId, sessionId)", nil)
                    return
                }
                
                let response = try await actr.call(request)
                appLogger.info("startVoice response======= \(response)")
                resolve([
                    "streamId": request.streamID,
                    "messageId": request.messageID,
                    "sessionId": request.sessionID,
                    "responseStreamId": response.streamID,
                    "status": response.status,
                    "message": response.message
                ])
            } catch {
                reject("ACTR_ERROR", error.localizedDescription, error)
            }
        }
    }
    
    /// Send voice chunk message
    /// - Parameters:
    ///   - json: JSON string containing 'sessionId', 'messageId', 'chunkId'
    ///   - audioData: Base64 encoded audio chunk data
    @objc
    public func sendVoiceChunk(
        _ json: String,
        audioData: String,
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        Task { @MainActor in
            guard connectionStatus == .connected, let actr = self.actr else {
                reject("NOT_CONNECTED", "ActrBridge not initialized", nil)
                return
            }
            
            do {
                guard let data = json.data(using: .utf8),
                      let dict = try JSONSerialization.jsonObject(with: data) as? [String: Any],
                      let streamID = dict["streamId"] as? String else {
                    reject("INVALID_INPUT", "Invalid JSON or streamId", nil)
                    return
                }
                
                var request = DataStreamApp_VoiceChunkMessage()
                request.streamID = streamID
                request.sessionID = dict["sessionId"] as? String ?? ""
                request.messageID = dict["messageId"] as? String ?? ""
                request.chunkID = dict["chunkId"] as? String ?? ""// 客户端创建
                request.chunkData = dict["chunkData"] as? Data ?? Data()
                
                let response = try await actr.call(request)
                resolve([
                    "streamID": response.streamID,
                    "status": response.status,
                    "message": response.message
                ])
            } catch {
                reject("ACTR_ERROR", error.localizedDescription, error)
            }
        }
    }
    
    /// End voice stream (UnregisterDataStream)
    /// - Parameter json: JSON string containing 'streamId'
    @objc
    public func endVoice(
        _ json: String,
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        Task { @MainActor in
            guard connectionStatus == .connected, let actr = self.actr else {
                reject("NOT_CONNECTED", "ActrBridge not initialized", nil)
                return
            }
            
            do {
                guard let data = json.data(using: .utf8),
                      let dict = try JSONSerialization.jsonObject(with: data) as? [String: Any],
                      let streamID = dict["streamId"] as? String else {
                    reject("INVALID_INPUT", "Invalid JSON or missing streamId", nil)
                    return
                }
                
                var request = StreamServer_UnregisterRequest()
                request.streamID = streamID
                
                let response = try await actr.call(request)
                resolve([
                    "success": response.success,
                    "message": response.message
                ])
            } catch {
                reject("ACTR_ERROR", error.localizedDescription, error)
            }
        }
    }

    /// Upload attachment
    /// - Parameters:
    ///   - json: JSON string containing 'id', 'filename', 'type'
    ///   - fileData: Base64 encoded file data
    @objc
    public func uploadAttachment(
        _ json: String,
        fileData: String,
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        Task { @MainActor in
            guard connectionStatus == .connected, let actr = self.actr else {
                reject("NOT_CONNECTED", "ActrBridge not initialized", nil)
                return
            }

            do {
                guard let data = json.data(using: .utf8),
                      let dict = try JSONSerialization.jsonObject(with: data) as? [String: Any],
                      let binaryData = Data(base64Encoded: fileData) else {
                    reject("INVALID_INPUT", "Invalid JSON or fileData", nil)
                    return
                }

                var request = StreamServer_AttachRequest()
                request.id = dict["id"] as? String ?? UUID().uuidString
                request.filename = dict["filename"] as? String ?? "file"
                
                let typeInt = dict["type"] as? Int ?? 0
                request.type = StreamServer_AttachmentType(rawValue: typeInt) ?? .unknown
                request.data = binaryData

                let response = try await actr.call(request)
                resolve([
                    "id": response.id,
                    "statusCode": response.statusCode,
                    "errorMessage": response.errorMessage
                ])
            } catch {
                reject("ACTR_ERROR", error.localizedDescription, error)
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
        let payloadText = String(data: chunk.payload, encoding: .utf8) ?? ""
        
        // 推送到 RN
        guard let bridge = await ActrBridge.shared() else {
            let errorMsg = "Failed to get ActrBridge instance to send event"
            throw ActrError.StateError(msg: errorMsg)
        }
        
        let body: [String: Any] = [
            "streamID": self.streamId,
            "status": 0,
            "message": payloadText
        ]
        
        await bridge.sendStreamEventToRN(body: body)
    }
}

// MARK: - Implementation of LocalFileServiceHandler

/// Implementation of AskLocalServiceHandler
/// TODO: Implement the business logic for each RPC method
public final class AskLocalServiceHandlerImpl: AskLocalServiceHandler {
    private let remoteServiceTypeRepr = "acme+AskService"
    private var callbacks: [String: StreamPrintCallback] = [:]
    
    public init() {}

    private func discoverTarget(ctx: Context) async throws -> ActrId {
        let targetType = try ActrType.fromStringRepr(remoteServiceTypeRepr)
        return try await ctx.discover(targetType: targetType)
    }

    /// RPC method: SendText
    public func sendText(
        req: DataStreamApp_TextMessage,
        ctx: Context
    ) async throws -> DataStreamApp_TextMessageResponse {
        logger.info("[AskLocalServiceHandlerImpl] Received SendText request")

        var userMessage = StreamServer_UsrPromptRequest()
        userMessage.text = req.questionID
        userMessage.sessionID = req.sessionID
        userMessage.questionID = req.messageID
        
        let targetId = try await discoverTarget(ctx: ctx)
        let responseData = try await ctx.call(
            target: targetId,
            routeKey: StreamServer_UsrPromptRequest.routeKey,
            payload: try userMessage.serializedData()
        )
        
        // 反序列化为 StreamServer_AssistantReply
        let reply = try StreamServer_AssistantReply(serializedBytes: responseData)
        
        // 创建并保存回调实例，确保生命周期
        let callback = StreamPrintCallback(streamId: reply.streamID, onMessage: nil)
        self.callbacks[reply.streamID] = callback
        
        // 注册回调以便处理后续的流式响应
        let _ = try await ctx.registerStream(
            streamId: reply.streamID,
            callback: callback
        )
        
        logger.info("[AskLocalServiceHandlerImpl] Text response stream registered: \(reply.streamID)")
        
        var resp = DataStreamApp_TextMessageResponse()
        resp.streamID = reply.streamID
        resp.status = reply.statusCode
        resp.message = reply.errorMessage
        
        return resp
    }
    
    /// RPC method: StartSendVoice
    public func startSendVoice(
        req: DataStreamApp_StartVoiceMessage,
        ctx: Context
    ) async throws -> DataStreamApp_StartVoiceMessageResponse {
        logger.info("[AskLocalServiceHandlerImpl] Received StartSendVoice request")

        var userMessage = StreamServer_UsrPromptRequest()
        userMessage.voiceStreamID = req.streamID
        userMessage.sessionID = req.sessionID
        userMessage.questionID = req.messageID
        
        let targetId = try await discoverTarget(ctx: ctx)
        let responseData = try await ctx.call(
            target: targetId,
            routeKey: StreamServer_UsrPromptRequest.routeKey,
            payload: try userMessage.serializedData()
        )
        
        // 反序列化为 StreamServer_AssistantReply
        let reply = try StreamServer_AssistantReply(serializedBytes: responseData)
        
        // 创建并保存回调实例，确保生命周期
        let callback = StreamPrintCallback(streamId: reply.streamID, onMessage: nil)
        self.callbacks[reply.streamID] = callback
        
        // 使用 StreamServer_AssistantReply 的 streamID 注册回调
        let _ = try await ctx.registerStream(
            streamId: reply.streamID,
            callback: callback
        )
        
        logger.info("[AskLocalServiceHandlerImpl] DataStream registered with ID: \(reply.streamID)")
        
        var resp = DataStreamApp_StartVoiceMessageResponse()
        resp.streamID = reply.streamID
        resp.status = reply.statusCode
        resp.message = reply.errorMessage
        
        return resp
    }

    /// RPC method: SendVoiceChunk
    public func sendVoiceChunk(
        req: DataStreamApp_VoiceChunkMessage,
        ctx: Context
    ) async throws -> DataStreamApp_VoiceChunkMessageResponse {
        logger.info("[AskLocalServiceHandlerImpl] Received sendVoiceChunk: streamId=\(req.streamID), chunkId=\(req.chunkID)")
        
        let targetId = try await discoverTarget(ctx: ctx)
        
        let seq = UInt64(req.chunkID) ?? 0
        
        let chunk = DataStream(
            streamId: req.streamID,
            sequence: seq,
            payload: req.chunkData,
            metadata: [],
            timestampMs: Int64(Date().timeIntervalSince1970 * 1000)
        )
        
        try await ctx.sendDataStream(target: targetId, chunk: chunk)
        
        var resp = DataStreamApp_VoiceChunkMessageResponse()
        resp.streamID = req.streamID
        resp.status = 0
        
        return resp
    }

}

extension AskLocalServiceWorkload: Workload where T == AskLocalServiceHandlerImpl {
    public func onStart(ctx _: Context) async throws {}
    public func onStop(ctx _: Context) async throws {}

    public func dispatch(ctx: Context, envelope: RpcEnvelope) async throws -> Data {
        return try await __dispatch(ctx: ctx, envelope: envelope)
    }
}

// Global logger for the handlers to use
private let logger = Logger(subsystem: "io.actr.App", category: "ActrBridge")

// MARK: - React Native Bridge Support

/// Type alias for React Native promise callbacks
public typealias RCTPromiseResolveBlock = (Any?) -> Void
public typealias RCTPromiseRejectBlock = (String?, String?, Error?) -> Void

// --- ActrBridge 事件推送 ---
extension ActrBridge {
    @objc func sendStreamEventToRN(body: Any) {
        self.sendEvent(withName: "ActrStreamMessage", body: body)
    }
    
    @objc public override static func requiresMainQueueSetup() -> Bool {
        return true
    }
    
    @objc public override func supportedEvents() -> [String] {
        return ["ActrStreamMessage"]
    }
}
