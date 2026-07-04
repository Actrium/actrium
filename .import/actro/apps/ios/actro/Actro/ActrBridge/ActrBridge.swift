import Foundation
import ActroCore
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
    private let contract = actrBridgeContract()
    
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
        
        var request = Actro_VoiceChunkMessage()
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
                let actorTypeInfo: ActrActorTypeInfo
                do {
                    actorTypeInfo = try parseActrActorType(raw: actorType)
                } catch {
                    throw ActrError.StateError(msg: "Invalid actorType format: \(actorType)")
                }
                let targetType = ActrType(manufacturer: actorTypeInfo.manufacturer, name: actorTypeInfo.name)
                
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
            guard let tabId = dict[contract.keyTabId] as? String,
                  let actorType = dict[contract.keyActorType] as? String,
                  let path = dict[contract.keyPath] as? String else {
                reject(contract.errorInvalidInput, "Missing tabId, actorType or path in JSON config", nil)
                return
            }
            do {
                try await self.initialize(tabId: tabId, actorType: actorType, configPath: path)
                resolve([contract.keyStatus: contract.statusConnected])
            } catch let error as ActrError {
                reject(contract.errorActr, self.formatActrError(error), error)
            } catch {
                reject(contract.errorUnknown, error.localizedDescription, error)
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
            guard let tabId = dict[contract.keyTabId] as? String else {
                reject(contract.errorInvalidInput, "Missing tabId in input", nil)
                return
            }
            await self.shutdown(tabId: tabId)
            resolve([contract.keyStatus: "disconnected"])
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
            guard let tabId = dict[contract.keyTabId] as? String,
                  let textResponseStreamID = dict[contract.keyTextResponseStreamId] as? String,
                  let instance = actorInstances[tabId] else {
                reject(contract.errorNotConnected, "ActrBridge not initialized for tabId", nil)
                return
            }
            
            do {
                logger.info("[ActrBridge] Sending text message: \(dict)")
                var request = Actro_TextMessage()
                if let base64Text = dict[contract.keyText] as? String, !base64Text.isEmpty {
                    request.text = base64Text
                }
                request.sessionID = dict[contract.keySessionId] as? String ?? ""
                request.messageID = dict[contract.keyMessageId] as? String ?? ""
                request.streamID = dict[contract.keyStreamId] as? String ?? ""
                request.attachmentIds = dict[contract.keyAttachmentIds] as? [String] ?? []
                request.textResponseStreamID = textResponseStreamID
                request.voiceResponseStreamID = dict[contract.keyVoiceResponseStreamId] as? String ?? ""
                
                if let locationDict = dict[contract.keyLocation] as? [String: Any] {
                    var location = Ask_Location()
                    location.latitude = locationDict[contract.keyLatitude] as? Double ?? 0
                    location.longitude = locationDict[contract.keyLongitude] as? Double ?? 0
                    location.address = locationDict[contract.keyAddress] as? String ?? ""
                    location.placeName = locationDict[contract.keyPlaceName] as? String ?? ""
                    request.location = location
                }
                
                if request.streamID.isEmpty {
                    reject(contract.errorInvalidInput, "Missing required field (streamId)", nil)
                    return
                }
                let response = try await instance.actr.call(request)
                resolve([
                    contract.keyQuestionId: response.questionID,
                    contract.keySessionId: response.sessionID,
                    contract.keyText: response.text,
                    contract.keyStatusCode: response.statusCode,
                    contract.keyErrorMessage: response.errorMessage
                ])
            } catch let error as ActrError {
                reject(contract.errorActr, self.formatActrError(error), error)
            } catch {
                reject(contract.errorUnknown, error.localizedDescription, error)
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
            guard let tabId = dict[contract.keyTabId] as? String,
                  let instance = actorInstances[tabId] else {
                reject(contract.errorNotConnected, "ActrBridge not initialized for tabId", nil)
                return
            }
            
            do {
                var request = Actro_StartVoiceMessage()
                request.streamID = dict[contract.keyStreamId] as? String ?? ""
                request.messageID = dict[contract.keyMessageId] as? String ?? ""
                request.sessionID = dict[contract.keySessionId] as? String ?? ""
                request.attachmentIds = dict[contract.keyAttachmentIds] as? [String] ?? []
                request.textResponseStreamID = dict[contract.keyTextResponseStreamId] as? String ?? ""
                request.voiceResponseStreamID = dict[contract.keyVoiceResponseStreamId] as? String ?? ""
                
                if let locationDict = dict[contract.keyLocation] as? [String: Any] {
                    var location = Ask_Location()
                    location.latitude = locationDict[contract.keyLatitude] as? Double ?? 0
                    location.longitude = locationDict[contract.keyLongitude] as? Double ?? 0
                    location.address = locationDict[contract.keyAddress] as? String ?? ""
                    location.placeName = locationDict[contract.keyPlaceName] as? String ?? ""
                    request.location = location
                }
                
                if request.streamID.isEmpty {
                    reject(contract.errorInvalidInput, "Missing required field (streamId)", nil)
                    return
                }
                
                let response = try await instance.actr.call(request)
                appLogger.info("startVoice response======= \(response)")
                resolve([
                    contract.keySessionId: response.sessionID,
                    contract.keyQuestionId: response.questionID,
                    contract.keyText: response.text,
                    contract.keyStatusCode: response.statusCode,
                    contract.keyErrorMessage: response.errorMessage
                ])
            } catch {
                reject(contract.errorActr, error.localizedDescription, error)
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
            guard let tabId = dict[contract.keyTabId] as? String,
                  let instance = actorInstances[tabId] else {
                reject(contract.errorNotConnected, "ActrBridge not initialized for tabId", nil)
                return
            }
            
            do {
                guard let streamID = dict[contract.keyStreamId] as? String else {
                    reject(contract.errorInvalidInput, "Missing streamId", nil)
                    return
                }
                
                var request = Actro_VoiceChunkMessage()
                request.streamID = streamID
                request.sessionID = dict[contract.keySessionId] as? String ?? ""
                request.messageID = dict[contract.keyMessageId] as? String ?? ""
                request.chunkID = dict[contract.keyChunkId] as? String ?? ""// 客户端创建
                
                // Parse metadata if present
                if let metadataArray = dict[contract.keyMetadata] as? [[String: String]] {
                    var metadataDict: [String: String] = [:]
                    for entry in metadataArray {
                        if let key = entry[contract.keyMetadataKey], let value = entry[contract.keyMetadataValue] {
                            metadataDict[key] = value
                        }
                    }
                    request.metadata = metadataDict
                }
                
                guard let audioBinary = Data(base64Encoded: audioData) else {
                    reject(contract.errorInvalidInput, "Invalid audioData base64", nil)
                    return
                }
                request.chunkData = audioBinary
                
                let response = try await instance.actr.call(request)
                resolve([
                    contract.keyStreamIdUpper: response.streamID,
                    contract.keyStatus: response.status,
                    contract.keyMessage: response.message
                ])
            } catch {
                reject(contract.errorActr, error.localizedDescription, error)
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
            guard let tabId = dict[contract.keyTabId] as? String,
                  let instance = actorInstances[tabId] else {
                reject(contract.errorNotConnected, "ActrBridge not initialized for tabId", nil)
                return
            }

            do {
                var request = Actro_PrepareAttachmentUploadMessage()
                request.id = dict[contract.keyId] as? String ?? UUID().uuidString
                request.filename = dict[contract.keyFilename] as? String ?? ""
                let typeInt = dict[contract.keyType] as? Int ?? 0
                request.type = Ask_AttachmentType(rawValue: typeInt) ?? .unknown
                request.streamID = dict[contract.keyStreamId] as? String ?? ""
                request.checksum = dict[contract.keyChecksum] as? String ?? ""
                request.checksumAlgorithm = dict[contract.keyChecksumAlgorithm] as? String ?? ""
                request.sizeBytes = dict[contract.keySizeBytes] as? UInt64 ?? 0
                request.resultStreamID = dict[contract.keyResultStreamId] as? String ?? ""

                if request.streamID.isEmpty {
                    reject(contract.errorInvalidInput, "Missing required field (streamId)", nil)
                    return
                }

                let response = try await instance.actr.call(request)
                resolve([
                    contract.keyId: response.id,
                    contract.keyStatusCode: response.statusCode,
                    contract.keyErrorMessage: response.errorMessage
                ])
            } catch {
                reject(contract.errorActr, error.localizedDescription, error)
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
            guard let tabId = dict[contract.keyTabId] as? String,
                  let instance = actorInstances[tabId] else {
                reject(contract.errorNotConnected, "ActrBridge not initialized for tabId", nil)
                return
            }

            do {
                guard let streamID = dict[contract.keyStreamId] as? String else {
                    reject(contract.errorInvalidInput, "Missing streamId", nil)
                    return
                }

                var request = Actro_AttachmentChunkMessage()
                request.streamID = streamID
                request.sessionID = dict[contract.keySessionId] as? String ?? ""
                request.attachmentID = dict[contract.keyAttachmentId] as? String ?? ""
                request.chunkID = dict[contract.keyChunkId] as? String ?? ""

                // Parse metadata if present
                if let metadataArray = dict[contract.keyMetadata] as? [[String: String]] {
                    var metadataDict: [String: String] = [:]
                    for entry in metadataArray {
                        if let key = entry[contract.keyMetadataKey], let value = entry[contract.keyMetadataValue] {
                            metadataDict[key] = value
                        }
                    }
                    request.metadata = metadataDict
                }

                guard let binaryData = Data(base64Encoded: chunkData) else {
                    reject(contract.errorInvalidInput, "Invalid chunkData base64", nil)
                    return
                }
                request.chunkData = binaryData

                let response = try await instance.actr.call(request)
                resolve([
                    contract.keyStreamIdUpper: response.streamID,
                    contract.keyStatus: response.status,
                    contract.keyMessage: response.message
                ])
            } catch {
                reject(contract.errorActr, error.localizedDescription, error)
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
                contract.keyStatus: status,
                contract.keyErrorMessage: errorMessage ?? NSNull()
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
            guard let tabId = dict[contract.keyTabId] as? String,
                  let streamId = dict[contract.keyStreamId] as? String,
                  let instance = actorInstances[tabId] else {
                reject(contract.errorInvalidInput, "Missing tabId, streamId or instance not found", nil)
                return
            }
            
            await instance.workload.handler.removeCallback(streamId: streamId)
            resolve(true)
        }
    }
}

// MARK: - Simple stream handler that logs incoming chunks.

enum StreamType {
    case text
    case voice
    case attachmentResult

    func wireValue(contract: ActrBridgeContractInfo) -> String {
        switch self {
        case .text:
            return contract.streamTypeText
        case .voice:
            return contract.streamTypeVoice
        case .attachmentResult:
            return contract.streamTypeAttachmentResult
        }
    }
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
        let contract = actrBridgeContract()
        // Convert metadata to dictionary array
        let metadataArray = chunk.metadata.map { entry in
            return [contract.keyMetadataKey: entry.key, contract.keyMetadataValue: entry.value]
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
                    contract.keyStreamIdUpper: self.streamId,
                    contract.keyType: self.streamType.wireValue(contract: contract),
                    contract.keyId: result.id,
                    contract.keyStatus: result.status.rawValue,
                    contract.keyStatusCode: result.statusCode,
                    contract.keyErrorMessage: result.errorMessage,
                    contract.keyMetadata: metadataArray,
                    contract.keySequence: chunk.sequence
                ]
                if let timestampMs = chunk.timestampMs {
                    body[contract.keyTimestampMs] = timestampMs
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
            contract.keyStreamIdUpper: self.streamId,
            contract.keyType: self.streamType.wireValue(contract: contract),
            contract.keyStatus: contract.streamStatusOk,
            contract.keyMessage: message,
            contract.keyMetadata: metadataArray,
            contract.keySequence: chunk.sequence
        ]

        if let timestampMs = chunk.timestampMs {
            body[contract.keyTimestampMs] = timestampMs
        }

        bridge.sendStreamEventToRN(body: body)
    }
}

// MARK: - Implementation of LocalFileServiceHandler

/// Implementation of AskLocalServiceHandler
/// TODO: Implement the business logic for each RPC method
public actor AskLocalServiceHandlerImpl: AskLocalServiceHandler {
    private let targetType: ActrType
    private let contract = actrBridgeContract()
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
        req: Actro_TextMessage,
        ctx: Context
    ) async throws -> Actro_TextMessageResponse {
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
            routeKey: contract.askServicePromptRouteKey,
            payloadType: .rpcReliable,
            payload: try userMessage.serializedData(),
            timeoutMs: Int64(contract.iosAskServicePromptTimeoutMs)
        )
        
        
        // 反序列化为 Ask_AssistantReply
        let reply = try Ask_AssistantReply(serializedBytes: responseData)
        
        var resp = Actro_TextMessageResponse()
        resp.questionID = reply.questionID
        resp.sessionID = reply.sessionID
        resp.text = reply.text
        resp.statusCode = reply.statusCode
        resp.errorMessage = reply.errorMessage
        
        return resp
    }
    
    /// RPC method: StartSendVoice
    public func startSendVoice(
        req: Actro_StartVoiceMessage,
        ctx: Context
    ) async throws -> Actro_StartVoiceMessageResponse {
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
            routeKey: contract.askServicePromptRouteKey,
            payloadType: .rpcReliable,
            payload: try userMessage.serializedData(),
            timeoutMs: Int64(contract.iosAskServicePromptTimeoutMs)
        )
        
        // 反序列化为 Ask_AssistantReply
        let reply = try Ask_AssistantReply(serializedBytes: responseData)
        
        
        var resp = Actro_StartVoiceMessageResponse()
        resp.questionID = reply.questionID
        resp.sessionID = reply.sessionID
        resp.text = reply.text
        resp.statusCode = reply.statusCode
        resp.errorMessage = reply.errorMessage
        
        return resp
    }
    
    /// RPC method: SendVoiceChunk
    public func sendVoiceChunk(
        req: Actro_VoiceChunkMessage,
        ctx: Context
    ) async throws -> Actro_VoiceChunkMessageResponse {
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
        
        var resp = Actro_VoiceChunkMessageResponse()
        resp.streamID = req.streamID
        resp.status = contract.streamStatusOk

        return resp
    }

    /// RPC method: PrepareAttachmentUpload
    public func prepareAttachmentUpload(
        req: Actro_PrepareAttachmentUploadMessage,
        ctx: Context
    ) async throws -> Actro_PrepareAttachmentUploadMessageResponse {
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
            routeKey: contract.askServicePrepareAttachmentUploadRouteKey,
            payloadType: .rpcReliable,
            payload: try askRequest.serializedData(),
            timeoutMs: Int64(contract.iosAskServicePrepareAttachmentUploadTimeoutMs)
        )

        let reply = try Ask_PrepareAttachmentUploadResponse(serializedBytes: responseData)

        var resp = Actro_PrepareAttachmentUploadMessageResponse()
        resp.id = reply.id
        resp.statusCode = reply.statusCode
        resp.errorMessage = reply.errorMessage

        return resp
    }

    /// RPC method: SendAttachmentChunk
    public func sendAttachmentChunk(
        req: Actro_AttachmentChunkMessage,
        ctx: Context
    ) async throws -> Actro_AttachmentChunkMessageResponse {
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

        var resp = Actro_AttachmentChunkMessageResponse()
        resp.streamID = req.streamID
        resp.status = contract.streamStatusOk

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
        self.sendEvent(withName: contract.streamEventName, body: body)
    }
    
    @objc public override static func requiresMainQueueSetup() -> Bool {
        return true
    }
    
    @objc public override func supportedEvents() -> [String] {
        return [contract.streamEventName]
    }
    
    // 添加Actro
}
