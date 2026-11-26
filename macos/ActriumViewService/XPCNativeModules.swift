//
//  XPCNativeModules.swift
//  ActriumViewService
//
//  Native modules for React Native running in XPC service.
//  Uses DistributedNotificationCenter to communicate with host process.
//

import Foundation
import React
import ActriumIPC

private enum NativeHostTransport {
    static func post(name: String, userInfo: [String: Any] = [:]) {
        if RendererProcessController.sendHostEvent(name: name, userInfo: userInfo) {
            return
        }
        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name(name),
            object: nil,
            userInfo: userInfo.isEmpty ? nil : userInfo,
            deliverImmediately: true
        )
    }
}

@objc(DocumentSelection)
final class DocumentSelection: NSObject {
    @objc func setAvailable(_ tabId: String, active: Bool) {
        RendererProcessController.sendHostEvent(name: "com.actrium.documentSelection",
                                               userInfo: ["active": active], tabId: tabId)
    }

    @objc static func requiresMainQueueSetup() -> Bool { false }
}

@objc(RealmService)
final class RealmService: RCTEventEmitter {
    private static weak var instance: RealmService?
    private var hasListeners = false

    override init() {
        super.init()
        RealmService.instance = self
    }

    static func emit(_ event: RealmServiceEvent, tabId: String) {
        DispatchQueue.main.async {
            guard let instance, instance.hasListeners else { return }
            var body = [
                "tabId": tabId,
                "subscriptionId": event.subscriptionId.uuidString,
                "event": event.event,
                "payloadBase64": event.payload.base64EncodedString(),
            ]
            if let sequence = event.sequence {
                body["streamId"] = event.event
                body["sequence"] = String(sequence)
            }
            instance.sendEvent(withName: "realmServiceEvent", body: body)
        }
    }

    @objc(call:route:payloadBase64:timeoutMs:resolver:rejecter:)
    func call(
        _ tabId: String,
        route: String,
        payloadBase64: String,
        timeoutMs: NSNumber,
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        guard let payload = Data(base64Encoded: payloadBase64),
              (1...300_000).contains(timeoutMs.intValue) else {
            reject("invalid_service_call", "Invalid Service call arguments.", nil)
            return
        }
        let call = RealmServiceCall(id: UUID(), route: route, payload: payload, timeoutMs: timeoutMs.intValue)
        submit(tabId, call: call, resolve: resolve, reject: reject)
    }

    @objc(sendServiceStreamChunk:route:streamId:sequence:payloadBase64:resolver:rejecter:)
    func sendServiceStreamChunk(
        _ tabId: String, route: String, streamId: String, sequence: String, payloadBase64: String,
        resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        let maxBase64Bytes = (RealmServiceCall.maxStreamChunkBytes + 2) / 3 * 4
        guard let sequence = UInt64(sequence), sequence > 0,
              payloadBase64.utf8.count <= maxBase64Bytes,
              let payload = Data(base64Encoded: payloadBase64) else {
            reject("invalid_service_stream_chunk", "Invalid Service DataStream send arguments.", nil)
            return
        }
        let chunk = RealmServiceCall.StreamChunk(id: streamId, sequence: sequence)
        let call = RealmServiceCall(id: UUID(), route: route, payload: payload, timeoutMs: 10_000, streamChunk: chunk)
        submit(tabId, call: call, resolve: { _ in resolve(nil) }, reject: reject)
    }

    private func submit(
        _ tabId: String, call: RealmServiceCall,
        resolve: @escaping RCTPromiseResolveBlock, reject: @escaping RCTPromiseRejectBlock
    ) {
        DispatchQueue.main.async {
            RendererProcessController.callRealmService(tabId: tabId, call: call) { response in
                if let payload = response.payload {
                    resolve(["payloadBase64": payload.base64EncodedString()])
                } else {
                    let error = response.failure.map { failure in
                        var userInfo: [String: Any] = [
                            NSLocalizedDescriptionKey: failure.message,
                            "code": failure.code,
                        ]
                        userInfo["details"] = failure.details
                        return NSError(domain: failure.domain ?? "RealmService", code: 1, userInfo: userInfo)
                    }
                    reject("realm_service_call_failed", response.error ?? "The Service call failed.", error)
                }
            }
        }
    }

    @objc(subscribeServiceEvents:events:resolver:rejecter:)
    func subscribeServiceEvents(
        _ tabId: String,
        events: [String],
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        guard !events.isEmpty, events.allSatisfy({ !$0.isEmpty }) else {
            reject("invalid_service_event", "A Service event subscription needs declared event names.", nil)
            return
        }
        let subscription = RealmServiceEventSubscription(id: UUID(), events: events)
        subscribe(tabId: tabId, subscription: subscription, resolve: resolve, reject: reject)
    }

    @objc(subscribeServiceStream:streamId:resolver:rejecter:)
    func subscribeServiceStream(
        _ tabId: String,
        streamId: String,
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        subscribe(tabId: tabId, subscription: RealmServiceEventSubscription(id: UUID(), streamId: streamId),
                  resolve: resolve, reject: reject)
    }

    private func subscribe(
        tabId: String,
        subscription: RealmServiceEventSubscription,
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        DispatchQueue.main.async {
            RendererProcessController.subscribeRealmServiceEvents(
                tabId: tabId, subscription: subscription
            ) { response in
                if let failure = response.failure {
                    reject(failure.code, failure.message, NSError(
                        domain: failure.domain ?? "RealmService", code: 1,
                        userInfo: [NSLocalizedDescriptionKey: failure.message]
                    ))
                } else {
                    resolve(subscription.id.uuidString)
                }
            }
        }
    }

    // MARK: - Attachment upload transport (internal)

    /// Prepares an attachment upload. The stream ids are generated by the
    /// Host TS layer; this method only validates and forwards them.
    /// Request keys: id, filename, type, sizeBytes, checksum,
    /// checksumAlgorithm, streamId, resultStreamId.
    @objc(prepareAttachmentUpload:request:resolver:rejecter:)
    func prepareAttachmentUpload(
        _ tabId: String,
        request: [String: Any],
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        do {
            let call = try Self.makeAttachmentPrepareCall(tabId: tabId, request: request)
            DispatchQueue.main.async {
                RendererProcessController.prepareAttachmentUpload(tabId: tabId, call: call) { response in
                    if let failure = response.failure {
                        reject(failure.code, failure.message, nil)
                    } else if response.statusCode != 0 {
                        reject(
                            "attachment_prepare_rejected",
                            response.errorMessage ?? "The attachment upload was rejected (statusCode \(response.statusCode)).",
                            nil
                        )
                    } else {
                        resolve(["id": call.attachmentId])
                    }
                }
            }
        } catch let error as AttachmentModuleValidationError {
            reject("invalid_input", error.message, nil)
        } catch {
            reject("invalid_input", error.localizedDescription, nil)
        }
    }

    /// Sends one attachment chunk. Request keys: streamId, sequence,
    /// fileData (strict padded base64) plus optional EOS metadata
    /// (either a `metadata` array of `{key, value}` entries or a flattened
    /// `eos` field).
    @objc(sendAttachmentChunk:request:resolver:rejecter:)
    func sendAttachmentChunk(
        _ tabId: String,
        request: [String: Any],
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        do {
            let call = try Self.makeAttachmentChunkCall(request: request)
            DispatchQueue.main.async {
                RendererProcessController.sendAttachmentChunk(tabId: tabId, call: call) { response in
                    if let failure = response.failure {
                        reject(failure.code, failure.message, nil)
                    } else {
                        resolve(nil)
                    }
                }
            }
        } catch let error as AttachmentModuleValidationError {
            reject("invalid_input", error.message, nil)
        } catch {
            reject("invalid_input", error.localizedDescription, nil)
        }
    }

    /// Unregisters the whole attachment session referenced by either its
    /// chunk stream or its result stream. Idempotent.
    @objc(unregisterStream:streamId:resolver:rejecter:)
    func unregisterStream(
        _ tabId: String,
        streamId: String,
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        guard !streamId.isEmpty else {
            reject("invalid_input", "The streamId must not be empty.", nil)
            return
        }
        let call = RealmServiceAttachmentUnregisterCall(streamId: streamId)
        DispatchQueue.main.async {
            RendererProcessController.unregisterAttachmentStream(tabId: tabId, call: call) { response in
                if let failure = response.failure {
                    reject(failure.code, failure.message, nil)
                } else {
                    resolve(nil)
                }
            }
        }
    }

    @objc(unsubscribeServiceEvents:subscriptionId:)
    func unsubscribeServiceEvents(_ tabId: String, subscriptionId: String) {
        guard let id = UUID(uuidString: subscriptionId) else { return }
        DispatchQueue.main.async {
            RendererProcessController.unsubscribeRealmServiceEvents(
                tabId: tabId, subscriptionId: id
            )
        }
    }

    override func supportedEvents() -> [String]! { ["realmServiceEvent"] }
    override func startObserving() { hasListeners = true }
    override func stopObserving() { hasListeners = false }

    /// Reports whether the owning tab has a Realm Service binding that permits
    /// the fixed upload transport. The Guest preflight uses this instead of
    /// assuming the browser-wide native module implies a usable tab.
    @objc(attachmentUploadAvailable:resolver:rejecter:)
    func attachmentUploadAvailable(
        _ tabId: String,
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        let call = RealmServiceAttachmentAvailabilityCall()
        DispatchQueue.main.async {
            RendererProcessController.attachmentUploadAvailable(tabId: tabId, call: call) { response in
                if let failure = response.failure {
                    reject(failure.code, failure.message, nil)
                } else {
                    resolve(response.available)
                }
            }
        }
    }

    @objc override static func requiresMainQueueSetup() -> Bool { true }
}

// MARK: - Attachment upload validation helpers

/// Renderer-local copy of the Host attachment contract boundaries. The Host
/// re-validates authoritatively; these early checks only fail fast on JS
/// bugs. Keep in sync with `RealmServiceAttachmentUploadPolicy`.
private enum AttachmentUploadValidation {
    static let endOfStreamMetadata = RealmServiceAttachmentMetadata(key: "eos", value: "true")
    static let validTypeRawValues: Set<Int32> = [1, 2, 3, 4, 99]

    /// Strict RFC 4648 (padded) base64; empty string decodes to empty data.
    static func strictBase64Data(_ text: String) -> Data? {
        if text.isEmpty { return Data() }
        var alphabet = Set<Unicode.Scalar>("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/".unicodeScalars)
        let padding: Unicode.Scalar = "="
        alphabet.insert(padding)

        let scalars = Array(text.unicodeScalars)
        guard scalars.count % 4 == 0, scalars.allSatisfy({ alphabet.contains($0) }) else {
            return nil
        }
        guard let firstPadding = scalars.firstIndex(of: padding) else {
            return Data(base64Encoded: text)
        }
        let tail = scalars[firstPadding...]
        guard tail.allSatisfy({ $0 == padding }), tail.count <= 2 else {
            return nil
        }
        return Data(base64Encoded: text)
    }
}

private enum AttachmentModuleValidationError: LocalizedError {
    case invalid(String)

    var errorDescription: String? { message }

    var message: String {
        switch self {
        case .invalid(let message): return message
        }
    }
}

extension RealmService {
    fileprivate static func makeAttachmentPrepareCall(
        tabId: String,
        request: [String: Any]
    ) throws -> RealmServiceAttachmentPrepareCall {
        guard !tabId.isEmpty else {
            throw AttachmentModuleValidationError.invalid("The tabId must not be empty.")
        }
        guard let attachmentId = request["id"] as? String,
              (1...128).contains(attachmentId.utf8.count) else {
            throw AttachmentModuleValidationError.invalid("The request id must be a string of 1...128 bytes.")
        }
        guard let filename = request["filename"] as? String,
              (1...256).contains(filename.utf8.count) else {
            throw AttachmentModuleValidationError.invalid("The filename must be a string of 1...256 bytes.")
        }
        guard let type = strictInt32(request["type"]),
              AttachmentUploadValidation.validTypeRawValues.contains(type) else {
            throw AttachmentModuleValidationError.invalid(
                "The type must be one of image(1), document(2), audio(3), video(4), other(99).")
        }
        guard let sizeBytes = strictUInt64(request["sizeBytes"]),
              sizeBytes <= UInt64(RealmServiceAttachmentIPC.maxBytes) else {
            throw AttachmentModuleValidationError.invalid(
                "The sizeBytes must be an integer between 0 and \(RealmServiceAttachmentIPC.maxBytes).")
        }
        guard let checksum = request["checksum"] as? String,
              let normalizedChecksum = normalizedMD5(checksum) else {
            throw AttachmentModuleValidationError.invalid("The checksum must be a 32 character hex MD5 digest.")
        }
        guard request["checksumAlgorithm"] as? String == "md5" else {
            throw AttachmentModuleValidationError.invalid("The checksumAlgorithm must be md5.")
        }
        guard let streamId = request["streamId"] as? String,
              (1...128).contains(streamId.utf8.count),
              let resultStreamId = request["resultStreamId"] as? String,
              (1...128).contains(resultStreamId.utf8.count) else {
            throw AttachmentModuleValidationError.invalid(
                "The streamId and resultStreamId must each be a string of 1...128 bytes.")
        }
        guard streamId != resultStreamId else {
            throw AttachmentModuleValidationError.invalid("The streamId and resultStreamId must be different.")
        }
        return RealmServiceAttachmentPrepareCall(
            attachmentId: attachmentId,
            filename: filename,
            type: type,
            streamId: streamId,
            checksum: normalizedChecksum,
            checksumAlgorithm: "md5",
            sizeBytes: sizeBytes,
            resultStreamId: resultStreamId
        )
    }

    fileprivate static func makeAttachmentChunkCall(
        request: [String: Any]
    ) throws -> RealmServiceAttachmentChunkCall {
        guard let streamId = request["streamId"] as? String,
              (1...128).contains(streamId.utf8.count) else {
            throw AttachmentModuleValidationError.invalid("The streamId must be a string of 1...128 bytes.")
        }
        guard let sequence = strictUInt64(request["sequence"]), sequence >= 1 else {
            throw AttachmentModuleValidationError.invalid("The sequence must be an integer >= 1.")
        }
        guard let fileData = request["fileData"] as? String,
              let payload = AttachmentUploadValidation.strictBase64Data(fileData) else {
            throw AttachmentModuleValidationError.invalid("The fileData must be standard padded base64.")
        }
        guard payload.count <= RealmServiceAttachmentIPC.chunkSizeBytes else {
            throw AttachmentModuleValidationError.invalid(
                "The decoded chunk must be at most \(RealmServiceAttachmentIPC.chunkSizeBytes) bytes.")
        }
        let metadata = try attachmentChunkMetadata(request: request)
        let attachmentId = request["attachmentId"] as? String ?? ""
        guard attachmentId.isEmpty || (1...128).contains(attachmentId.utf8.count) else {
            throw AttachmentModuleValidationError.invalid("The attachmentId must be a string of 1...128 bytes.")
        }
        return RealmServiceAttachmentChunkCall(
            streamId: streamId,
            attachmentId: attachmentId,
            sequence: sequence,
            payload: payload,
            metadata: metadata
        )
    }

    /// Accepts the EOS marker either as a `metadata` array of `{key, value}`
    /// entries (matching the IPC shape) or as a flattened `eos` field. Only
    /// the single `eos=true` entry is legal, matching the Host contract.
    fileprivate static func attachmentChunkMetadata(request: [String: Any]) throws -> [RealmServiceAttachmentMetadata] {
        if let raw = request["metadata"], !(raw is NSNull) {
            guard let entries = raw as? [[String: Any]] else {
                throw AttachmentModuleValidationError.invalid("The metadata must be an array of {key, value} entries.")
            }
            guard entries.count <= 8 else {
                throw AttachmentModuleValidationError.invalid("The metadata must have at most 8 entries.")
            }
            return try entries.map { entry in
                guard let key = entry["key"] as? String, (1...128).contains(key.utf8.count),
                      let value = entry["value"] as? String, (1...256).contains(value.utf8.count) else {
                    throw AttachmentModuleValidationError.invalid(
                        "Each metadata entry needs a key of 1...128 bytes and a value of 1...256 bytes.")
                }
                return RealmServiceAttachmentMetadata(key: key, value: value)
            }
        }
        if let eos = request["eos"], !(eos is NSNull) {
            let marker: String?
            if let text = eos as? String {
                marker = text
            } else if let flag = eos as? Bool {
                marker = flag ? "true" : nil
            } else {
                marker = nil
            }
            guard marker == AttachmentUploadValidation.endOfStreamMetadata.value else {
                throw AttachmentModuleValidationError.invalid("The eos field must be the string \"true\".")
            }
            return [AttachmentUploadValidation.endOfStreamMetadata]
        }
        return []
    }

    /// NSNumber that is really a number (booleans bridging into NSNumber are
    /// rejected) and fits an Int32.
    fileprivate static func strictInt32(_ value: Any?) -> Int32? {
        guard let number = value as? NSNumber,
              CFGetTypeID(number) != CFBooleanGetTypeID(),
              number.doubleValue >= Double(Int32.min),
              number.doubleValue <= Double(Int32.max),
              number.doubleValue.truncatingRemainder(dividingBy: 1) == 0 else {
            return nil
        }
        return number.int32Value
    }

    /// NSNumber that is really a non-negative integral number.
    fileprivate static func strictUInt64(_ value: Any?) -> UInt64? {
        guard let number = value as? NSNumber,
              CFGetTypeID(number) != CFBooleanGetTypeID(),
              number.doubleValue >= 0,
              number.doubleValue <= Double(UInt64.max),
              number.doubleValue.truncatingRemainder(dividingBy: 1) == 0 else {
            return nil
        }
        return number.uint64Value
    }

    /// Lowercases and verifies a 32 character hex MD5 digest.
    fileprivate static func normalizedMD5(_ text: String) -> String? {
        let normalized = text.lowercased()
        guard normalized.utf8.count == 32,
              normalized.utf8.allSatisfy({ byte in
                  (48...57).contains(byte) || (97...102).contains(byte)
              }) else {
            return nil
        }
        return normalized
    }
}

// MARK: - Capability Permission Module

/// Requests a user decision from the trusted Native Front Host. The renderer
/// never persists authorization state itself.
@objc(CapabilityPermission)
class CapabilityPermission: NSObject {
    @objc
    func requestGrant(_ tabId: String, capability: String) {
        NativeHostTransport.post(
            name: "com.actrium.capabilityPermissionRequest",
            userInfo: ["tabId": tabId, "capability": capability]
        )
    }

    @objc
    static func requiresMainQueueSetup() -> Bool {
        return false
    }

    @objc
    static func moduleName() -> String! {
        return "CapabilityPermission"
    }
}

// MARK: - Tab Title Module

/// Runtime tab title update reported by a Guest, analogous to
/// `document.title`. The shell owns display and persistence; the renderer
/// forwards the title with its tab id and originating content generation.
@objc(TabTitle)
class TabTitle: NSObject {
    @objc
    func setTitle(_ title: String, tabId: String, reloadKey: NSNumber) {
        RendererProcessController.sendTitleChanged(
            title,
            tabId: tabId,
            reloadKey: reloadKey.intValue
        )
    }

    @objc
    static func requiresMainQueueSetup() -> Bool {
        return false
    }

    @objc
    static func moduleName() -> String! {
        return "TabTitle"
    }
}

// MARK: - StartPage Module

/// Native module for Start Page actions from React Native (XPC Service version)
@objc(StartPageModule)
class StartPageModule: NSObject {

    @objc
    func openURL(_ urlString: String?) {
        guard let urlString = urlString else {
            NSLog("[StartPageModule] URL is nil")
            return
        }

        NSLog("[StartPageModule] Opening URL: \(urlString)")

        NativeHostTransport.post(
            name: "com.actro.openURLFromStartPage",
            userInfo: ["url": urlString]
        )
    }

    @objc
    static func requiresMainQueueSetup() -> Bool {
        return false
    }

    @objc
    static func moduleName() -> String! {
        return "StartPageModule"
    }
}

// MARK: - History Module

/// Native module for history actions from React Native (XPC Service version)
@objc(HistoryModule)
class HistoryModule: NSObject {

    @objc
    func openHistoryItem(_ tabId: String, _ itemId: String) {
        RendererProcessController.sendHostEvent(
            name: "com.actro.openHistoryItem",
            userInfo: ["itemId": itemId],
            tabId: tabId
        )
    }

    @objc
    func deleteHistoryItem(_ tabId: String, _ itemId: String) {
        RendererProcessController.sendHostEvent(
            name: "com.actro.deleteHistoryItem",
            userInfo: ["itemId": itemId],
            tabId: tabId
        )
    }

    @objc
    func restoreGroup(_ tabId: String, _ groupId: String) {
        RendererProcessController.sendHostEvent(
            name: "com.actro.restoreHistoryGroup",
            userInfo: ["groupId": groupId],
            tabId: tabId
        )
    }

    @objc
    func clearHistory(_ tabId: String) {
        RendererProcessController.sendHostEvent(name: "com.actro.clearHistory", tabId: tabId)
    }

    @objc
    func setHistoryPage(_ tabId: String, _ page: NSNumber) {
        RendererProcessController.sendHostEvent(
            name: "com.actrium.setHistoryPage",
            userInfo: ["page": page],
            tabId: tabId
        )
    }

    @objc
    static func requiresMainQueueSetup() -> Bool {
        return false
    }

    @objc
    static func moduleName() -> String! {
        return "HistoryModule"
    }
}

// MARK: - Bookmarks Module

/// Native module for bookmark actions from React Native (XPC Service version)
@objc(BookmarksModule)
class BookmarksModule: NSObject {

    @objc
    func openBookmark(_ tabId: String, _ bookmarkId: String) {
        RendererProcessController.sendHostEvent(
            name: "com.actro.openBookmark",
            userInfo: ["bookmarkId": bookmarkId],
            tabId: tabId
        )
    }

    @objc
    func deleteBookmark(_ tabId: String, _ bookmarkId: String) {
        RendererProcessController.sendHostEvent(
            name: "com.actro.deleteBookmark",
            userInfo: ["bookmarkId": bookmarkId],
            tabId: tabId
        )
    }

    @objc
    static func requiresMainQueueSetup() -> Bool {
        return false
    }

    @objc
    static func moduleName() -> String! {
        return "BookmarksModule"
    }
}

// MARK: - Native Logger Module

/// Native logger module for React Native (XPC Service version)
/// Logs to stderr and optionally sends to host for DevTools Console
@objc(NativeLogger)
class NativeLogger: NSObject {

    // Remote log server for development
    private static let logServerURL = "http://localhost:9999/log"

    /// Write to shared IPC debug log file for cross-process log correlation
    private static func writeToIPCLog(_ message: String) {
        if RendererProcessController.isRendererProcess {
            return
        }
        let timestamp: String = {
            let now = Date()
            let formatter = DateFormatter()
            formatter.dateFormat = "HH:mm:ss.SSS"
            return formatter.string(from: now)
        }()
        let line = "[\(timestamp)] [JS] \(message)\n"

        // Write to shared IPC log file
        let logPath = "/tmp/actrium_ipc_debug.log"
        if let handle = FileHandle(forWritingAtPath: logPath) {
            handle.seekToEndOfFile()
            if let data = line.data(using: .utf8) {
                handle.write(data)
            }
            handle.closeFile()
        } else {
            FileManager.default.createFile(atPath: logPath, contents: line.data(using: .utf8))
        }
    }

    private static func sendToHost(level: String, source: String, message: String) {
        if RendererProcessController.sendConsoleLog(
            level: level,
            source: source,
            message: message
        ) {
            return
        }
        // Send log to host process for DevTools Console
        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name("com.actro.xpcLog"),
            object: nil,
            userInfo: [
                "level": level,
                "source": source,
                "message": message
            ],
            deliverImmediately: true
        )
    }

    private static func sendToRemote(level: String, message: String) {
        // Disabled: Remote log server causes "Connection refused" noise when not running
        // The DevTools Console receives logs via DistributedNotificationCenter (sendToHost)
        // which is more reliable and doesn't require a separate server process
    }

    @objc
    func log(_ message: String) {
        NSLog("[JS] %@", message)
        NativeLogger.writeToIPCLog(message)
        NativeLogger.sendToHost(level: "info", source: "JS", message: message)
        NativeLogger.sendToRemote(level: "log", message: message)
    }

    @objc
    func warn(_ message: String) {
        NSLog("[JS WARN] %@", message)
        NativeLogger.sendToHost(level: "warn", source: "JS", message: message)
        NativeLogger.sendToRemote(level: "warn", message: message)
    }

    @objc
    func error(_ message: String) {
        NSLog("[JS ERROR] %@", message)
        NativeLogger.sendToHost(level: "error", source: "JS", message: message)
        NativeLogger.sendToRemote(level: "error", message: message)
    }

    @objc
    static func nativeLog(_ message: String) {
        NSLog("[Native] %@", message)
        sendToHost(level: "info", source: "Native", message: message)
        sendToRemote(level: "log", message: "[Native] \(message)")
    }

    @objc
    static func nativeWarn(_ message: String) {
        NSLog("[Native WARN] %@", message)
        sendToHost(level: "warn", source: "Native", message: message)
        sendToRemote(level: "warn", message: "[Native] \(message)")
    }

    @objc
    static func nativeError(_ message: String) {
        NSLog("[Native ERROR] %@", message)
        sendToHost(level: "error", source: "Native", message: message)
        sendToRemote(level: "error", message: "[Native] \(message)")
    }

    @objc
    static func requiresMainQueueSetup() -> Bool {
        return false
    }

    @objc
    static func moduleName() -> String! {
        return "NativeLogger"
    }
}
