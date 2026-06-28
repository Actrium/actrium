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
public class ActrBridge: RCTEventEmitter {
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
    
    private struct ActorInstance {
        let system: ActrSystem
        let node: ActrNode
        let actr: ActrRef
        let workload: AskLocalServiceWorkload<AskLocalServiceHandlerImpl>
        let targetType: ActrType
    }
    
    private var actorInstances: [String: ActorInstance] = [:]
    
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
    /// - Parameter tabId: The tab ID to use
    /// - Returns: The generated streamID
    func prepareClientStream(tabId: String) async throws -> (String) {
        guard let instance = actorInstances[tabId] else {
            throw ActrError.StateError(msg: "ActrBridge not initialized for \(tabId)")
        }
        
        var request = DataStreamApp_VoiceChunkMessage()
        let streamID = "stream-\(UUID().uuidString)"
        request.streamID = streamID
        
        logger.info("[ActrBridge] Preparing client stream: stream_id=\(request.streamID)")
        
        do {
            let _ = try await instance.actr.call(request)
            return (streamID)
        } catch let actrError as ActrError {
            logger.error("[ActrBridge] Prepare client stream error: \(actrError.localizedDescription)")
            throw actrError
        } catch {
            logger.error("[ActrBridge] Prepare client stream error: \(error.localizedDescription)")
            throw error
        }
    }
}

// MARK: - Core Actr Operations

extension ActrBridge {
    /// Initialize the Actr system and its underlying actors
    private func initialize(tabId: String, actorType: String, configPath: String) async throws {
        connectionStatus = .initializing
        errorMessage = nil
        logger.info("[ActrBridge] Initializing ActrBridge for \(actorType) (tabId: \(tabId))...")
        
        do {
            let result = try await Task.detached(priority: .userInitiated) { () throws
                -> (ActrSystem, ActrNode, ActrRef, AskLocalServiceWorkload<AskLocalServiceHandlerImpl>, ActrType) in
                // 解析 actorType
                let parts = actorType.split(separator: "+")
                guard parts.count == 2 else {
                    throw ActrError.StateError(msg: "Invalid actorType format: \(actorType)")
                }
                let manufacturer = String(parts[0])
                let name = String(parts[1])
                let targetType = ActrType(manufacturer: manufacturer, name: name)
                
                let system = try await ActrSystem.from(tomlConfig: configPath)
                let handler = AskLocalServiceHandlerImpl(targetType)
                let workload = AskLocalServiceWorkload(handler: handler)
                let node = try system.spawn(workload: workload)
                let actr = try await node.start()
                
                let discovered = try await actr.discover(targetType: targetType, count: 1)
                guard let _ = discovered.first else {
                    throw ActrError.StateError(msg: "No actor found for target type: \(actorType)")
                }
                
                return (system, node, actr, workload, targetType)
            }.value
            
            let (system, node, actr, workload, targetType) = result
            let instance = ActorInstance(system: system, node: node, actr: actr, workload: workload, targetType: targetType)
            actorInstances[tabId] = instance
            
            // Wait a bit for the signaling client to establish connection
            logger.info("[ActrBridge] Waiting for signaling client to connect...")
            try await Task.sleep(nanoseconds: 500_000_000) // 0.5 seconds
            
            connectionStatus = .connected
            logger.info("[ActrBridge] ActrBridge initialized successfully for \(actorType) (tabId: \(tabId))")
        } catch let actrError as ActrError {
            errorMessage = "Actr Error: \(actrError.localizedDescription)"
            connectionStatus = .error
            logger.error("[ActrBridge] Failed to initialize \(actorType): \(actrError.localizedDescription)")
            throw actrError
        } catch {
            errorMessage = "Initialization Error: \(error.localizedDescription)"
            connectionStatus = .error
            logger.error("[ActrBridge] Failed to initialize \(actorType): \(error.localizedDescription)")
            throw error
        }
    }
    
    /// Shutdown the service and cleanup resources
    private func shutdown(tabId: String?) async {
        if let tabId = tabId {
            logger.info("[ActrBridge] Shutting down ActrBridge for tabId: \(tabId)...")
            if let instance = actorInstances.removeValue(forKey: tabId) {
                await instance.actr.stop()
            }
        } else {
            logger.info("[ActrBridge] Shutting down all ActrBridge instances...")
            for (tid, instance) in actorInstances {
                logger.info("[ActrBridge] Shutting down instance for tabId: \(tid)...")
                await instance.actr.stop()
            }
            actorInstances.removeAll()
            connectionStatus = .disconnected
        }
        
        errorMessage = nil
    }
}

// MARK: - React Native Bridge Methods

extension ActrBridge {
    
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
        _ dict: [String: Any],
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        Task { @MainActor in
            guard let tabId = dict["tabId"] as? String,
                  let actorType = dict["actorType"] as? String,
                  let path = dict["path"] as? String else {
                reject("INVALID_INPUT", "Missing tabId, actorType or path in JSON config", nil)
                return
            }
            do {
                try await self.initialize(tabId: tabId, actorType: actorType, configPath: path)
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
        _ dict: [String: Any],
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        Task { @MainActor in
            guard let tabId = dict["tabId"] as? String else {
                reject("INVALID_INPUT", "Missing tabId in input", nil)
                return
            }
            await self.shutdown(tabId: tabId)
            resolve(["status": "disconnected"])
        }
    }
    
    /// Send text message through ActrService
    @objc
    public func sendTextMessage(
        _ dict: [String: Any],
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        Task { @MainActor in
            guard let tabId = dict["tabId"] as? String,
                  let textResponseStreamID = dict["textResponseStreamId"] as? String,
                  let instance = actorInstances[tabId] else {
                reject("NOT_CONNECTED", "ActrBridge not initialized for tabId", nil)
                return
            }
            
            do {
                logger.info("[ActrBridge] Sending text message: \(dict)")
                var request = DataStreamApp_TextMessage()
                if let base64Text = dict["text"] as? String, !base64Text.isEmpty {
                    request.text = base64Text
                }
                request.sessionID = dict["sessionId"] as? String ?? ""
                request.messageID = dict["messageId"] as? String ?? ""
                request.streamID = dict["streamId"] as? String ?? ""
                request.attachmentIds = dict["attachmentIds"] as? [String] ?? []
                request.textResponseStreamID = textResponseStreamID
                request.voiceResponseStreamID = dict["voiceResponseStreamId"] as? String ?? ""
                
                if let locationDict = dict["location"] as? [String: Any] {
                    var location = Ask_Location()
                    location.latitude = locationDict["latitude"] as? Double ?? 0
                    location.longitude = locationDict["longitude"] as? Double ?? 0
                    location.address = locationDict["address"] as? String ?? ""
                    location.placeName = locationDict["placeName"] as? String ?? ""
                    request.location = location
                }
                
                if request.streamID.isEmpty {
                    reject("INVALID_INPUT", "Missing required field (streamId)", nil)
                    return
                }
                let response = try await instance.actr.call(request)
                resolve([
                    "questionId": response.questionID,
                    "sessionId": response.sessionID,
                    "text": response.text,
                    "statusCode": response.statusCode,
                    "errorMessage": response.errorMessage
                ])
            } catch let error as ActrError {
                reject("ACTR_ERROR", self.formatActrError(error), error)
            } catch {
                reject("UNKNOWN_ERROR", error.localizedDescription, error)
            }
        }
    }
    
    /// Start voice stream (was sendVoiceStream)
    /// - Parameter dict: Dictionary containing IDs
    @objc
    public func startVoice(
        _ dict: [String: Any],
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        Task { @MainActor in
            guard let tabId = dict["tabId"] as? String,
                  let instance = actorInstances[tabId] else {
                reject("NOT_CONNECTED", "ActrBridge not initialized for tabId", nil)
                return
            }
            
            do {
                var request = DataStreamApp_StartVoiceMessage()
                request.streamID = dict["streamId"] as? String ?? ""
                request.messageID = dict["messageId"] as? String ?? ""
                request.sessionID = dict["sessionId"] as? String ?? ""
                request.attachmentIds = dict["attachmentIds"] as? [String] ?? []
                request.textResponseStreamID = dict["textResponseStreamId"] as? String ?? ""
                request.voiceResponseStreamID = dict["voiceResponseStreamId"] as? String ?? ""
                
                if let locationDict = dict["location"] as? [String: Any] {
                    var location = Ask_Location()
                    location.latitude = locationDict["latitude"] as? Double ?? 0
                    location.longitude = locationDict["longitude"] as? Double ?? 0
                    location.address = locationDict["address"] as? String ?? ""
                    location.placeName = locationDict["placeName"] as? String ?? ""
                    request.location = location
                }
                
                if request.streamID.isEmpty {
                    reject("INVALID_INPUT", "Missing required field (streamId)", nil)
                    return
                }
                
                let response = try await instance.actr.call(request)
                appLogger.info("startVoice response======= \(response)")
                resolve([
                    "sessionId": response.sessionID,
                    "questionId": response.questionID,
                    "text": response.text,
                    "statusCode": response.statusCode,
                    "errorMessage": response.errorMessage
                ])
            } catch {
                reject("ACTR_ERROR", error.localizedDescription, error)
            }
        }
    }
    
    /// Send voice chunk message
    /// - Parameters:
    ///   - dict: Dictionary containing 'sessionId', 'messageId', 'chunkId'
    ///   - audioData: Base64 encoded audio chunk data
    @objc
    public func sendVoiceChunk(
        _ dict: [String: Any],
        audioData: String,
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        Task { @MainActor in
            guard let tabId = dict["tabId"] as? String,
                  let instance = actorInstances[tabId] else {
                reject("NOT_CONNECTED", "ActrBridge not initialized for tabId", nil)
                return
            }
            
            do {
                guard let streamID = dict["streamId"] as? String else {
                    reject("INVALID_INPUT", "Missing streamId", nil)
                    return
                }
                
                var request = DataStreamApp_VoiceChunkMessage()
                request.streamID = streamID
                request.sessionID = dict["sessionId"] as? String ?? ""
                request.messageID = dict["messageId"] as? String ?? ""
                request.chunkID = dict["chunkId"] as? String ?? ""// 客户端创建
                
                // Parse metadata if present
                if let metadataArray = dict["metadata"] as? [[String: String]] {
                    var metadataDict: [String: String] = [:]
                    for entry in metadataArray {
                        if let key = entry["key"], let value = entry["value"] {
                            metadataDict[key] = value
                        }
                    }
                    request.metadata = metadataDict
                }
                
                guard let audioBinary = Data(base64Encoded: audioData) else {
                    reject("INVALID_INPUT", "Invalid audioData base64", nil)
                    return
                }
                request.chunkData = audioBinary
                
                let response = try await instance.actr.call(request)
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
    
    /// Prepare attachment upload
    /// - Parameter dict: Dictionary containing tabId, id, filename, type, streamId, checksum, checksumAlgorithm, sizeBytes, resultStreamId
    @objc
    public func prepareAttachmentUpload(
        _ dict: [String: Any],
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        Task { @MainActor in
            guard let tabId = dict["tabId"] as? String,
                  let instance = actorInstances[tabId] else {
                reject("NOT_CONNECTED", "ActrBridge not initialized for tabId", nil)
                return
            }

            do {
                var request = DataStreamApp_PrepareAttachmentUploadMessage()
                request.id = dict["id"] as? String ?? UUID().uuidString
                request.filename = dict["filename"] as? String ?? ""
                let typeInt = dict["type"] as? Int ?? 0
                request.type = Ask_AttachmentType(rawValue: typeInt) ?? .unknown
                request.streamID = dict["streamId"] as? String ?? ""
                request.checksum = dict["checksum"] as? String ?? ""
                request.checksumAlgorithm = dict["checksumAlgorithm"] as? String ?? ""
                request.sizeBytes = dict["sizeBytes"] as? UInt64 ?? 0
                request.resultStreamID = dict["resultStreamId"] as? String ?? ""

                if request.streamID.isEmpty {
                    reject("INVALID_INPUT", "Missing required field (streamId)", nil)
                    return
                }

                let response = try await instance.actr.call(request)
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

    /// Send attachment chunk
    /// - Parameters:
    ///   - dict: Dictionary containing tabId, sessionId, attachmentId, chunkId, streamId, metadata
    ///   - chunkData: Base64 encoded chunk data
    @objc
    public func sendAttachmentChunk(
        _ dict: [String: Any],
        chunkData: String,
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        Task { @MainActor in
            guard let tabId = dict["tabId"] as? String,
                  let instance = actorInstances[tabId] else {
                reject("NOT_CONNECTED", "ActrBridge not initialized for tabId", nil)
                return
            }

            do {
                guard let streamID = dict["streamId"] as? String else {
                    reject("INVALID_INPUT", "Missing streamId", nil)
                    return
                }

                var request = DataStreamApp_AttachmentChunkMessage()
                request.streamID = streamID
                request.sessionID = dict["sessionId"] as? String ?? ""
                request.attachmentID = dict["attachmentId"] as? String ?? ""
                request.chunkID = dict["chunkId"] as? String ?? ""

                // Parse metadata if present
                if let metadataArray = dict["metadata"] as? [[String: String]] {
                    var metadataDict: [String: String] = [:]
                    for entry in metadataArray {
                        if let key = entry["key"], let value = entry["value"] {
                            metadataDict[key] = value
                        }
                    }
                    request.metadata = metadataDict
                }

                guard let binaryData = Data(base64Encoded: chunkData) else {
                    reject("INVALID_INPUT", "Invalid chunkData base64", nil)
                    return
                }
                request.chunkData = binaryData

                let response = try await instance.actr.call(request)
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
    
    /// Unregister a stream callback
    @objc
    public func unregisterStream(
        _ dict: [String: Any],
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        Task { @MainActor in
            guard let tabId = dict["tabId"] as? String,
                  let streamId = dict["streamId"] as? String,
                  let instance = actorInstances[tabId] else {
                reject("INVALID_INPUT", "Missing tabId, streamId or instance not found", nil)
                return
            }
            
            await instance.workload.handler.removeCallback(streamId: streamId)
            resolve(true)
        }
    }
}

// MARK: - Simple stream handler that logs incoming chunks.

enum StreamType: String {
    case text
    case voice
    case attachmentResult
}

private actor StreamPrintCallback: Actr.DataStreamCallback {
    private let streamId: String
    private let streamType: StreamType
    private let onMessage: (@Sendable (String) -> Void)?
    
    init(streamId: String, type: StreamType, onMessage: (@Sendable (String) -> Void)?) {
        self.streamId = streamId
        self.streamType = type
        self.onMessage = onMessage
    }
    
    func onStream(chunk: Actr.DataStream, sender: ActrId) async throws {
        // Convert metadata to dictionary array
        let metadataArray = chunk.metadata.map { entry in
            return ["key": entry.key, "value": entry.value]
        }

        // 推送到 RN
        guard let bridge = ActrBridge.shared() else {
            let errorMsg = "[StreamPrintCallback] Failed to get ActrBridge instance to send event"
            throw ActrError.StateError(msg: errorMsg)
        }

        if streamType == .attachmentResult {
            // Deserialize as Ask_AttachmentUploadResult
            if let result = try? Ask_AttachmentUploadResult(serializedBytes: chunk.payload) {
                var body: [String: Any] = [
                    "streamID": self.streamId,
                    "type": self.streamType.rawValue,
                    "id": result.id,
                    "status": result.status.rawValue,
                    "statusCode": result.statusCode,
                    "errorMessage": result.errorMessage,
                    "metadata": metadataArray,
                    "sequence": chunk.sequence
                ]
                if let timestampMs = chunk.timestampMs {
                    body["timestampMs"] = timestampMs
                }
                bridge.sendStreamEventToRN(body: body)
                return
            }
        }

        let message: String
        if streamType == .voice {
            message = chunk.payload.base64EncodedString()
        } else {
            message = String(data: chunk.payload, encoding: .utf8) ?? ""
        }

        var body: [String: Any] = [
            "streamID": self.streamId,
            "type": self.streamType.rawValue,
            "status": 0,
            "message": message,
            "metadata": metadataArray,
            "sequence": chunk.sequence
        ]

        if let timestampMs = chunk.timestampMs {
            body["timestampMs"] = timestampMs
        }

        bridge.sendStreamEventToRN(body: body)
    }
}

// MARK: - Implementation of LocalFileServiceHandler

/// Implementation of AskLocalServiceHandler
/// TODO: Implement the business logic for each RPC method
public final class AskLocalServiceHandlerImpl: AskLocalServiceHandler {
    private let targetType: ActrType
    fileprivate var serverActId: ActrId? = nil
    
    fileprivate struct StreamInfo {
        let callback: StreamPrintCallback
        let ctx: Context
    }
    
    fileprivate var activeStreams: [String: StreamInfo] = [:]
    
    public init(_ targetType: ActrType) {
        self.targetType = targetType
    }
    
    public func removeCallback(streamId: String) async {
        if let info = self.activeStreams.removeValue(forKey: streamId) {
            try? await info.ctx.unregisterStream(streamId: streamId)
            logger.info("[AskLocalServiceHandlerImpl] Stream unregistered from context: \(streamId)")
        }
        logger.info("[AskLocalServiceHandlerImpl] Stream callback info removed: \(streamId)")
    }
    
    /// RPC method: SendText
    public func sendText(
        req: DataStreamApp_TextMessage,
        ctx: Context
    ) async throws -> DataStreamApp_TextMessageResponse {
        logger.info("[AskLocalServiceHandlerImpl] Received SendText request")
        
        var userMessage = Ask_UsrPromptRequest()
        userMessage.text = req.text
        userMessage.sessionID = req.sessionID
        userMessage.questionID = req.messageID
        userMessage.attachmentIds = req.attachmentIds
        userMessage.textResponseStreamID = req.textResponseStreamID
        userMessage.voiceResponseStreamID = req.voiceResponseStreamID
        if req.hasLocation {
            userMessage.location = req.location
        }

        logger.info("[AskLocalServiceHandlerImpl] User message prepared: \(userMessage.debugDescription)")
        
        serverActId = try await ctx.discover(targetType: targetType)
        guard let targetId = serverActId else {
            throw ActrError.StateError(msg: "Server ActId not initialized")
        }
        
        // 注册文本响应流
        if !req.textResponseStreamID.isEmpty {
            let callback = StreamPrintCallback(streamId: req.textResponseStreamID, type: .text, onMessage: nil)
            self.activeStreams[req.textResponseStreamID] = StreamInfo(callback: callback, ctx: ctx)
            let _ = try await ctx.registerStream(
                streamId: req.textResponseStreamID,
                callback: callback
            )
            logger.info("[AskLocalServiceHandlerImpl] Text response stream registered: \(req.textResponseStreamID)")
        }
        
        // 注册语音响应流
        if !req.voiceResponseStreamID.isEmpty {
            let callback = StreamPrintCallback(streamId: req.voiceResponseStreamID, type: .voice, onMessage: nil)
            self.activeStreams[req.voiceResponseStreamID] = StreamInfo(callback: callback, ctx: ctx)
            let _ = try await ctx.registerStream(
                streamId: req.voiceResponseStreamID,
                callback: callback
            )
            logger.info("[AskLocalServiceHandlerImpl] Voice response stream registered: \(req.voiceResponseStreamID)")
        }
        
        let responseData = try await ctx.callRaw(
            target: targetId,
            routeKey: Ask_UsrPromptRequest.routeKey,
            payloadType: .rpcReliable,
            payload: try userMessage.serializedData(),
            timeoutMs: 20000
        )
        
        
        // 反序列化为 Ask_AssistantReply
        let reply = try Ask_AssistantReply(serializedBytes: responseData)
        
        var resp = DataStreamApp_TextMessageResponse()
        resp.questionID = reply.questionID
        resp.sessionID = reply.sessionID
        resp.text = reply.text
        resp.statusCode = reply.statusCode
        resp.errorMessage = reply.errorMessage
        
        return resp
    }
    
    /// RPC method: StartSendVoice
    public func startSendVoice(
        req: DataStreamApp_StartVoiceMessage,
        ctx: Context
    ) async throws -> DataStreamApp_StartVoiceMessageResponse {
        logger.info("[AskLocalServiceHandlerImpl] Received StartSendVoice request")
        
        var userMessage = Ask_UsrPromptRequest()
        userMessage.voiceStreamID = req.streamID
        userMessage.sessionID = req.sessionID
        userMessage.questionID = req.messageID
        userMessage.attachmentIds = req.attachmentIds
        userMessage.textResponseStreamID = req.textResponseStreamID
        userMessage.voiceResponseStreamID = req.voiceResponseStreamID
        if req.hasLocation {
            userMessage.location = req.location
        }
        
        serverActId = try await ctx.discover(targetType: targetType)
        guard let targetId = serverActId else {
            throw ActrError.StateError(msg: "Server ActId not initialized")
        }

        // 注册文本响应流
        if !req.textResponseStreamID.isEmpty {
            let callback = StreamPrintCallback(streamId: req.textResponseStreamID, type: .text, onMessage: nil)
            self.activeStreams[req.textResponseStreamID] = StreamInfo(callback: callback, ctx: ctx)
            let _ = try await ctx.registerStream(
                streamId: req.textResponseStreamID,
                callback: callback
            )
            logger.info("[AskLocalServiceHandlerImpl] Text response stream registered: \(req.textResponseStreamID)")
        }
        
        // 注册语音响应流
        if !req.voiceResponseStreamID.isEmpty {
            let callback = StreamPrintCallback(streamId: req.voiceResponseStreamID, type: .voice, onMessage: nil)
            self.activeStreams[req.voiceResponseStreamID] = StreamInfo(callback: callback, ctx: ctx)
            let _ = try await ctx.registerStream(
                streamId: req.voiceResponseStreamID,
                callback: callback
            )
            logger.info("[AskLocalServiceHandlerImpl] Voice response stream registered: \(req.voiceResponseStreamID)")
        }
        
        let responseData = try await ctx.callRaw(
            target: targetId,
            routeKey: Ask_UsrPromptRequest.routeKey,
            payloadType: .rpcReliable,
            payload: try userMessage.serializedData(),
            timeoutMs: 20000
        )
        
        // 反序列化为 Ask_AssistantReply
        let reply = try Ask_AssistantReply(serializedBytes: responseData)
        
        
        var resp = DataStreamApp_StartVoiceMessageResponse()
        resp.questionID = reply.questionID
        resp.sessionID = reply.sessionID
        resp.text = reply.text
        resp.statusCode = reply.statusCode
        resp.errorMessage = reply.errorMessage
        
        return resp
    }
    
    /// RPC method: SendVoiceChunk
    public func sendVoiceChunk(
        req: DataStreamApp_VoiceChunkMessage,
        ctx: Context
    ) async throws -> DataStreamApp_VoiceChunkMessageResponse {
        logger.info("[AskLocalServiceHandlerImpl] Received sendVoiceChunk: streamId=\(req.streamID), chunkId=\(req.chunkID)")
        
        guard let targetId = self.serverActId else {
            throw ActrError.StateError(msg: "Server ActId not initialized")
        }
        
        let seq = UInt64(req.chunkID) ?? 0
        
        // Convert metadata dictionary to MetadataEntry array
        let metadataEntries = req.metadata.map { MetadataEntry(key: $0.key, value: $0.value) }
        
        let chunk = DataStream(
            streamId: req.streamID,
            sequence: seq,
            payload: req.chunkData,
            metadata: metadataEntries,
            timestampMs: Int64(Date().timeIntervalSince1970 * 1000)
        )
        
        try await ctx.sendDataStream(target: targetId, chunk: chunk)
        
        var resp = DataStreamApp_VoiceChunkMessageResponse()
        resp.streamID = req.streamID
        resp.status = 0

        return resp
    }

    /// RPC method: PrepareAttachmentUpload
    public func prepareAttachmentUpload(
        req: DataStreamApp_PrepareAttachmentUploadMessage,
        ctx: Context
    ) async throws -> DataStreamApp_PrepareAttachmentUploadMessageResponse {
        logger.info("[AskLocalServiceHandlerImpl] Received PrepareAttachmentUpload request: id=\(req.id)")

        var askRequest = Ask_PrepareAttachmentUploadRequest()
        askRequest.id = req.id
        askRequest.filename = req.filename
        askRequest.type = req.type
        askRequest.streamID = req.streamID
        askRequest.checksum = req.checksum
        askRequest.checksumAlgorithm = req.checksumAlgorithm
        askRequest.sizeBytes = req.sizeBytes
        askRequest.resultStreamID = req.resultStreamID

        serverActId = try await ctx.discover(targetType: targetType)
        guard let targetId = serverActId else {
            throw ActrError.StateError(msg: "Server ActId not initialized")
        }

        // Register result stream for receiving AttachmentUploadResult
        if !req.resultStreamID.isEmpty {
            let callback = StreamPrintCallback(streamId: req.resultStreamID, type: .attachmentResult, onMessage: nil)
            self.activeStreams[req.resultStreamID] = StreamInfo(callback: callback, ctx: ctx)
            let _ = try await ctx.registerStream(
                streamId: req.resultStreamID,
                callback: callback
            )
            logger.info("[AskLocalServiceHandlerImpl] Attachment result stream registered: \(req.resultStreamID)")
        }

        let responseData = try await ctx.callRaw(
            target: targetId,
            routeKey: Ask_PrepareAttachmentUploadRequest.routeKey,
            payloadType: .rpcReliable,
            payload: try askRequest.serializedData(),
            timeoutMs: 20000
        )

        let reply = try Ask_PrepareAttachmentUploadResponse(serializedBytes: responseData)

        var resp = DataStreamApp_PrepareAttachmentUploadMessageResponse()
        resp.id = reply.id
        resp.statusCode = reply.statusCode
        resp.errorMessage = reply.errorMessage

        return resp
    }

    /// RPC method: SendAttachmentChunk
    public func sendAttachmentChunk(
        req: DataStreamApp_AttachmentChunkMessage,
        ctx: Context
    ) async throws -> DataStreamApp_AttachmentChunkMessageResponse {
        logger.info("[AskLocalServiceHandlerImpl] Received sendAttachmentChunk: streamId=\(req.streamID), attachmentId=\(req.attachmentID), chunkId=\(req.chunkID)")

        guard let targetId = self.serverActId else {
            throw ActrError.StateError(msg: "Server ActId not initialized")
        }

        let seq = UInt64(req.chunkID) ?? 0

        let metadataEntries = req.metadata.map { MetadataEntry(key: $0.key, value: $0.value) }

        let chunk = DataStream(
            streamId: req.streamID,
            sequence: seq,
            payload: req.chunkData,
            metadata: metadataEntries,
            timestampMs: Int64(Date().timeIntervalSince1970 * 1000)
        )

        try await ctx.sendDataStream(target: targetId, chunk: chunk)

        var resp = DataStreamApp_AttachmentChunkMessageResponse()
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
    
    // 添加AskAway
}
