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
            var request = DataStreamApp_TextMessage()
            request.sessionID = "text-\(UUID().uuidString)"
            let _ = try await actr.call(request)
            return request.sessionID
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
                var request = DataStreamApp_StartVoiceMessage()
                // 此处可解析 json 填充 request 字段
                
                let response = try await actr.call(request)
                resolve([
                    "streamId": response.streamID
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
                request.chunkID = dict["chunkId"] as? String ?? ""
                request.chunkData = dict["chunkData"] as? Data ?? Data()
                
                let _ = try await actr.call(request)
                resolve(["status": "sent"])
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
        let senderRepr = "\(sender.type.toStringRepr())#\(sender.serialNumber)"
        let payloadText = String(data: chunk.payload, encoding: .utf8) ?? "<non-utf8 data>"
        let message = "stream_id=\(streamId), seq=\(chunk.sequence), bytes=\(chunk.payload.count), sender=\(senderRepr), text=\"\(payloadText)\""
        await logger.info("📥 DataStream chunk: \(message)")
        onMessage?(message)
        // --- 新增: 通过事件推送到RN ---
        await ActrBridge.shared.sendStreamEventToRN(message: message)
    }
}

// MARK: - Implementation of LocalFileServiceHandler

/// Implementation of AskLocalServiceHandler
/// TODO: Implement the business logic for each RPC method
public final class AskLocalServiceHandlerImpl: AskLocalServiceHandler {
    public init() {}

    /// RPC method: SendText
    public func sendText(
        req: DataStreamApp_TextMessage,
        ctx: Context
    ) async throws -> DataStreamApp_TextMessageResponse {
        logger.info("Received SendText request")
        return DataStreamApp_TextMessageResponse()
    }
    
    /// RPC method: StartSendVoice
    public func startSendVoice(
        req: DataStreamApp_StartVoiceMessage,
        ctx: Context
    ) async throws -> DataStreamApp_StartVoiceMessageResponse {
        logger.info("[AskLocalServiceHandlerImpl] Received StartSendVoice request")

        var resp = DataStreamApp_StartVoiceMessageResponse()
        // 注册数据流并关联回调
        let _ = try await ctx.registerStream(
            streamId: resp.streamID,
            callback: StreamPrintCallback(streamId: resp.streamID, onMessage: nil)
        )
        logger.info("[AskLocalServiceHandlerImpl] DataStream registered with ID: \(resp.streamID)")
        return resp
    }

    /// RPC method: SendVoiceChunk
    public func sendVoiceChunk(
        req: DataStreamApp_VoiceChunkMessage,
        ctx: Context
    ) async throws -> DataStreamApp_VoiceChunkMessageResponse {
        logger.info("Received SendVoiceChunk request: chunk=\(req.chunkID)")
        
        return DataStreamApp_VoiceChunkMessageResponse()
    }

}

extension AskLocalServiceWorkload: Workload where T == AskLocalServiceHandlerImpl {
    public func onStart(ctx _: Context) async throws {}
    public func onStop(ctx _: Context) async throws {}

    public func dispatch(ctx: Context, envelope: RpcEnvelope) async throws -> Data {
        // Handle StartSendVoice if not handled by __dispatch
        if envelope.routeKey == "data_stream_app.AskLocalService.StartSendVoice" {
            let req = try DataStreamApp_StartVoiceMessage(serializedBytes: envelope.payload)
            let resp = try await handler.startSendVoice(req: req, ctx: ctx)
            return try resp.serializedData()
        }
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
    
    @objc public override static func requiresMainQueueSetup() -> Bool {
        return true
    }
    
    @objc public override func supportedEvents() -> [String] {
        return ["ActrStreamMessage"]
    }
}
