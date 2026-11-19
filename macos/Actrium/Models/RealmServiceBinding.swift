import Actr
import ActrBindings
import ActriumCore
import ActriumIPC
import Foundation
import SwiftProtobuf

struct RealmServiceError: LocalizedError {
    let message: String
    var errorDescription: String? { message }
}

extension RealmServiceFailure {
    init(serviceError error: Error) {
        guard let error = error as? ActrBindings.ActrError else {
            self.init(error: error)
            return
        }
        let code: String
        let details: [String: String]?
        switch error {
        case .Unavailable(let msg): (code, details) = ("unavailable", ["msg": msg])
        case .ConnectionNotReady(let info):
            (code, details) = ("connection_not_ready", info.retryAfterMs.map { ["retryAfterMs": String($0)] })
        case .TimedOut: (code, details) = ("timed_out", nil)
        case .NotFound(let msg): (code, details) = ("not_found", ["msg": msg])
        case .PermissionDenied(let msg): (code, details) = ("permission_denied", ["msg": msg])
        case .InvalidArgument(let msg): (code, details) = ("invalid_argument", ["msg": msg])
        case .UnknownRoute(let msg): (code, details) = ("unknown_route", ["msg": msg])
        case .DependencyNotFound(let serviceName, let detail):
            (code, details) = ("dependency_not_found", ["serviceName": serviceName, "detail": detail])
        case .DecodeFailure(let msg): (code, details) = ("decode_failure", ["msg": msg])
        case .NotImplemented(let msg): (code, details) = ("not_implemented", ["msg": msg])
        case .Internal(let msg): (code, details) = ("internal", ["msg": msg])
        case .Config(let msg): (code, details) = ("config", ["msg": msg])
        case .DeliveryUncertain(let operationId, let cause):
            (code, details) = ("delivery_uncertain", ["operationId": operationId, "cause": cause])
        }
        self.init(code: code, message: error.localizedDescription, details: details)
    }
}

/// Receives `Ask_AttachmentUploadResult` protobufs on the per-upload result
/// stream. The FFI runtime owns the callback while the stream is registered;
/// the closure hops onto the binding actor for session bookkeeping.
private final class RealmServiceAttachmentResultCallback: ActrBindings.DataChunkCallback {
    private let onChunk: @Sendable (ActrBindings.DataChunk) -> Void

    init(onChunk: @escaping @Sendable (ActrBindings.DataChunk) -> Void) {
        self.onChunk = onChunk
    }

    func onStream(chunk: ActrBindings.DataChunk, sender: ActrBindings.ActrId) async throws {
        onChunk(chunk)
    }
}

@MainActor
final class RealmServiceBindings {
    static let shared = RealmServiceBindings()
    private var bindings: [UUID: RealmServiceBinding] = [:]

    func attach(_ binding: RealmServiceBinding, to tabId: UUID) {
        remove(tabId)
        bindings[tabId] = binding
    }

    func remove(_ tabId: UUID) {
        guard let binding = bindings.removeValue(forKey: tabId) else { return }
        Task { await binding.stop() }
    }

    func call(_ request: RealmServiceCall, tabId: UUID) async throws -> Data {
        guard let binding = bindings[tabId] else {
            throw RealmServiceError(message: "The tab has no Realm Service.")
        }
        return try await binding.call(request)
    }

    func prepareAttachmentUpload(
        _ request: RealmServiceAttachmentPrepareCall,
        tabId: UUID,
        deliver: @escaping @Sendable (RealmServiceAttachmentResultEvent) -> Void
    ) async -> RealmServiceAttachmentPrepareReply {
        guard let binding = bindings[tabId] else {
            return RealmServiceAttachmentPrepareReply(
                id: request.id,
                failure: RealmServiceFailure(
                    code: "realm_tab_unavailable",
                    message: "The tab has no Realm Service."
                )
            )
        }
        return await binding.prepareAttachmentUpload(request, deliver: deliver)
    }

    func sendAttachmentChunk(
        _ request: RealmServiceAttachmentChunkCall,
        tabId: UUID
    ) async -> RealmServiceAttachmentChunkReply {
        guard let binding = bindings[tabId] else {
            return RealmServiceAttachmentChunkReply(
                id: request.id,
                failure: RealmServiceFailure(
                    code: "realm_tab_unavailable",
                    message: "The tab has no Realm Service."
                )
            )
        }
        return await binding.sendAttachmentChunk(request)
    }

    func unregisterAttachmentStream(
        _ request: RealmServiceAttachmentUnregisterCall,
        tabId: UUID
    ) async -> RealmServiceAttachmentUnregisterReply {
        guard let binding = bindings[tabId] else {
            return RealmServiceAttachmentUnregisterReply(
                id: request.id,
                failure: RealmServiceFailure(
                    code: "realm_tab_unavailable",
                    message: "The tab has no Realm Service."
                )
            )
        }
        return await binding.unregisterAttachmentStream(request)
    }

    /// Cancels every attachment session of one tab without touching the
    /// binding itself. Used when the tab's renderer is replaced while the
    /// Front keeps running.
    /// True when the tab actually owns a Realm Service binding that still
    /// permits the fixed upload transport. Used by the renderer preflight so
    /// a Guest without a Realm Service (for example a locally bundled
    /// package) fails before its engine starts instead of on first upload.
    func attachmentUploadAvailable(_ tabId: UUID) async -> Bool {
        guard let binding = bindings[tabId] else { return false }
        return await binding.attachmentUploadAvailable
    }

    func cancelAttachmentUploads(_ tabId: UUID) {
        guard let binding = bindings[tabId] else { return }
        Task { await binding.cancelAttachmentUploads() }
    }
}

actor RealmServiceBinding {
    private struct Provider: Decodable {
        struct Signaling: Decodable {
            let url: String
            let priority: Int
        }

        struct Ice: Decodable {
            let urls: [String]
        }

        let signaling: [Signaling]
        let ice: [Ice]
    }

    private struct ProfileDocument: Decodable {
        let provider: Provider
    }

    private struct Capabilities: Decodable {
        struct Service: Decodable { let rpc: [String]? }
        let hostModules: [String: Service]
    }

    private let directory = FileManager.default.temporaryDirectory
        .appendingPathComponent("actrium-front-" + UUID().uuidString, isDirectory: true)
    private let profile: RealmProfileInfo
    private let front: Data
    private let configuration: String
    private let serviceType: ActrBindings.ActrType
    /// Business routes; internal for the attachment transport wiring tests.
    let routes: Set<String>
    /// `true` only when the manifest grants `service.attachmentUpload` AND the
    /// capability manifest declares the reserved `uploadAttachment` transport.
    /// Internal for the attachment transport wiring tests.
    let attachmentUploadAllowed: Bool
    private let sessions = RealmServiceAttachmentSessionStore()
    /// Keyed by the chunk stream id, which is unique per upload attempt. The
    /// caller-supplied attachment id may be reused by a retry, so it cannot
    /// identify a delivery on its own.
    private var attachmentDeliveries: [String: @Sendable (RealmServiceAttachmentResultEvent) -> Void] = [:]
    private var startup: (id: UUID, task: Task<ActrRefWrapper, Error>)?
    private var stopped = false

    init(
        profile: RealmProfileInfo,
        verifiedProfileBody: Data,
        front: Data,
        serviceType: ActrBindings.ActrType,
        trustedFrontKey: Data,
        capabilitiesJSON: String,
        permissions: [String]
    ) throws {
        let document = try JSONDecoder().decode(ProfileDocument.self, from: verifiedProfileBody)
        let provider = document.provider
        guard let signaling = provider.signaling.sorted(by: { $0.priority < $1.priority }).first,
              URL(string: signaling.url)?.scheme?.lowercased() == "wss" else {
            throw RealmServiceError(message: "The verified Realm profile has invalid signaling information.")
        }
        let capabilities = try JSONDecoder().decode(Capabilities.self, from: Data(capabilitiesJSON.utf8))
        let declaredRoutes = Set(capabilities.hostModules["service"]?.rpc ?? capabilities.hostModules["host:service"]?.rpc ?? [])
        self.attachmentUploadAllowed = RealmServiceAttachmentUploadPolicy.isPermitted(
            permissions: permissions,
            declaredRoutes: declaredRoutes
        )
        // The attachment transport is verified separately and must never be
        // reachable through the generic RealmService.call business routes.
        self.routes = RealmServiceAttachmentUploadPolicy.businessRoutes(declaredRoutes)
        self.serviceType = serviceType
        self.profile = profile
        self.front = front

        let encoder = JSONEncoder()
        encoder.outputFormatting = [.withoutEscapingSlashes]
        func literal<T: Encodable>(_ value: T) throws -> String {
            try String(decoding: encoder.encode(value), as: UTF8.self)
        }
        let iceURLs = provider.ice.flatMap(\.urls)
        self.configuration = try """
        edition = 1
        [signaling]
        url = \(literal(signaling.url))
        [issuer_endpoint]
        url = \(literal(profile.issuerUrl))
        [deployment]
        realm = \(literal(profile.realmName))
        [hyper]
        data_dir = \(literal(directory.path))
        [discovery]
        visible = false
        [webrtc]
        stun_urls = \(literal(iceURLs.filter { $0.hasPrefix("stun:") || $0.hasPrefix("stuns:") }))
        turn_urls = \(literal(iceURLs.filter { $0.hasPrefix("turn:") || $0.hasPrefix("turns:") }))
        [[acl.rules]]
        permission = "allow"
        type = \(literal(profile.serviceType))
        realm = "self"
        [[trust]]
        kind = "static"
        pubkey_b64 = \(literal(trustedFrontKey.base64EncodedString()))
        """
    }

    func call(_ request: RealmServiceCall) async throws -> Data {
        guard !stopped, routes.contains(request.route),
              (1...300_000).contains(request.timeoutMs), request.payload.count <= 1_048_576 else {
            throw RealmServiceError(message: "The Service call is outside this tab's declared capabilities.")
        }
        let reference = try await connect()
        guard !stopped else { throw CancellationError() }
        // A Realm tab targets the selected Service type. A restarted Service
        // may register with a different ActorId while the Front stays running.
        guard let target = try await reference.discover(targetType: serviceType, count: 1).first else {
            throw RealmServiceError(message: "The Realm's selected Service is not online.")
        }
        guard !stopped else { throw CancellationError() }
        return try await reference.callPeer(
            target: target,
            routeKey: request.route,
            requestPayload: request.payload,
            timeoutMs: Int64(request.timeoutMs)
        )
    }

    /// Availability as seen by this binding: the manifest must opt in and the
    /// binding must still be running. Actor-isolated so a race with `stop()`
    /// cannot report a binding that is already tearing down.
    var attachmentUploadAvailable: Bool {
        attachmentUploadAllowed && !stopped
    }

    /// Prepares one attachment upload: validates capability and fields,
    /// discovers the Service target once and fixes it for the whole session,
    /// registers the result stream, then issues the fixed
    /// `ask.AskService.PrepareAttachmentUpload` reliable RPC with a 20 second
    /// budget. Any failure unregisters the result stream again so no state
    /// leaks; JS never waits on a timeout for these paths.
    func prepareAttachmentUpload(
        _ call: RealmServiceAttachmentPrepareCall,
        deliver: @escaping @Sendable (RealmServiceAttachmentResultEvent) -> Void
    ) async -> RealmServiceAttachmentPrepareReply {
        do {
            guard attachmentUploadAllowed else { throw RealmServiceAttachmentUploadError.notPermitted }
            guard !stopped else { throw RealmServiceAttachmentUploadError.cancelled }
            let checksum = try RealmServiceAttachmentUploadPolicy.validatePrepare(call)
            // Register before the first suspension: connect() and discover()
            // may take seconds, and a renderer cancellation during that window
            // must find this upload instead of letting it start afterwards.
            try await sessions.registerPreparing(call, checksum: checksum)
            do {
                let reference = try await connect()
                guard !stopped,
                      await sessions.isCurrent(attachmentId: call.attachmentId, streamId: call.streamId) else {
                    throw RealmServiceAttachmentUploadError.cancelled
                }
                guard let target = try await reference.discover(targetType: serviceType, count: 1).first else {
                    throw RealmServiceAttachmentUploadError.unavailable(
                        "The Realm's selected Service is not online.")
                }
                guard !stopped,
                      await sessions.isCurrent(attachmentId: call.attachmentId, streamId: call.streamId) else {
                    throw RealmServiceAttachmentUploadError.cancelled
                }
                // The result delivery must exist before registration: the
                // Service may answer the prepare RPC and publish its terminal
                // result while this actor is still suspended on the RPC.
                attachmentDeliveries[call.streamId] = deliver
                let callback = RealmServiceAttachmentResultCallback { [weak self] chunk in
                    let payload = chunk.payload
                    Task { await self?.receiveAttachmentResult(streamId: chunk.streamId, payload: payload) }
                }
                do {
                    try await reference.registerStream(streamId: call.resultStreamId, callback: callback)
                } catch {
                    await cleanupAttachmentSession(attachmentId: call.attachmentId, streamId: call.streamId)
                    throw error
                }
                // Cancellation may have run while registration was suspended:
                // cancelAll() scanned the store before this stream existed, so
                // unregister it explicitly instead of leaking it.
                guard await sessions.isCurrent(attachmentId: call.attachmentId, streamId: call.streamId) else {
                    attachmentDeliveries.removeValue(forKey: call.streamId)
                    try? await reference.unregisterStream(streamId: call.resultStreamId)
                    throw RealmServiceAttachmentUploadError.cancelled
                }
                var request = Ask_PrepareAttachmentUploadRequest()
                request.id = call.attachmentId
                request.filename = call.filename
                request.type = Ask_AttachmentType(rawValue: Int(call.type)) ?? .other
                request.streamID = call.streamId
                request.checksum = checksum
                request.checksumAlgorithm = call.checksumAlgorithm
                request.sizeBytes = call.sizeBytes
                request.resultStreamID = call.resultStreamId
                let response = try await reference.callPeer(
                    target: target,
                    routeKey: RealmServiceAttachmentIPC.prepareRoute,
                    requestPayload: request.serializedData(),
                    timeoutMs: Int64(RealmServiceAttachmentIPC.prepareTimeoutMs)
                )
                // A late reply belongs to this attempt only. If the caller
                // unregistered and started a retry with the same attachment
                // id, the session now carries a different chunk stream and
                // this reply must not mutate it.
                guard !stopped,
                      await sessions.isCurrent(attachmentId: call.attachmentId, streamId: call.streamId) else {
                    throw RealmServiceAttachmentUploadError.cancelled
                }
                let reply = try Ask_PrepareAttachmentUploadResponse(serializedBytes: response)
                // A non-zero status is a Service rejection. Do not expose the
                // chunk stream and do not leave a live result registration.
                guard reply.statusCode == 0 else {
                    throw RealmServiceAttachmentUploadError.rejected(
                        reply.statusCode,
                        reply.errorMessage.isEmpty ? nil : reply.errorMessage)
                }
                try await sessions.markUploading(
                    attachmentId: call.attachmentId,
                    streamId: call.streamId,
                    target: target
                )
                return RealmServiceAttachmentPrepareReply(
                    id: call.id,
                    attachmentId: call.attachmentId,
                    statusCode: reply.statusCode
                )
            } catch {
                await cleanupAttachmentSession(
                    attachmentId: call.attachmentId,
                    streamId: call.streamId
                )
                throw error
            }
        } catch let error as RealmServiceAttachmentUploadError {
            return RealmServiceAttachmentPrepareReply(id: call.id, failure: error.failure)
        } catch {
            return RealmServiceAttachmentPrepareReply(
                id: call.id, failure: RealmServiceFailure(serviceError: error))
        }
    }

    /// Sends one validated chunk to the session's fixed target on the
    /// prepare-registered chunk stream. Sequence continuity, the 48 KiB
    /// payload cap and the declared-size accounting are enforced by the
    /// session store before any byte hits the transport.
    func sendAttachmentChunk(_ call: RealmServiceAttachmentChunkCall) async -> RealmServiceAttachmentChunkReply {
        do {
            guard attachmentUploadAllowed else { throw RealmServiceAttachmentUploadError.notPermitted }
            guard !stopped else { throw RealmServiceAttachmentUploadError.cancelled }
            let reference = try await connect()
            guard !stopped else { throw RealmServiceAttachmentUploadError.cancelled }
            let planned = try await sessions.planChunk(
                streamId: call.streamId,
                sequence: call.sequence,
                payloadBytes: call.payload.count,
                metadata: call.metadata
            )
            let chunk = ActrBindings.DataChunk(
                streamId: planned.session.chunkStreamId,
                sequence: planned.plan.sequence,
                payload: call.payload,
                metadata: call.metadata.map { ActrBindings.MetadataEntry(key: $0.key, value: $0.value) },
                timestampMs: Int64(Date().timeIntervalSince1970 * 1000)
            )
            do {
                try await reference.sendDataChunk(
                    target: planned.plan.target,
                    chunk: chunk,
                    payloadType: .streamReliable
                )
            } catch {
                // The sequence window already moved; the session cannot
                // recover from a failed send. Scope the cleanup to this
                // attempt so a retry reusing the attachment id survives.
                await cleanupAttachmentSession(
                    attachmentId: planned.session.attachmentId,
                    streamId: planned.session.chunkStreamId
                )
                throw error
            }
            return RealmServiceAttachmentChunkReply(id: call.id)
        } catch let error as RealmServiceAttachmentUploadError {
            return RealmServiceAttachmentChunkReply(id: call.id, failure: error.failure)
        } catch {
            return RealmServiceAttachmentChunkReply(
                id: call.id, failure: RealmServiceFailure(serviceError: error))
        }
    }

    /// Idempotent session teardown referenced by either the chunk stream or
    /// the result stream; the whole session (both streams) is cleaned up.
    func unregisterAttachmentStream(_ call: RealmServiceAttachmentUnregisterCall) async -> RealmServiceAttachmentUnregisterReply {
        if let session = await sessions.unregister(streamId: call.streamId) {
            attachmentDeliveries.removeValue(forKey: session.chunkStreamId)
            if let reference = try? await startup?.task.value {
                try? await reference.unregisterStream(streamId: session.resultStreamId)
                try? await reference.unregisterStream(streamId: session.chunkStreamId)
            }
        }
        return RealmServiceAttachmentUnregisterReply(id: call.id)
    }

    /// Drops every upload session of this binding without delivering
    /// results. Used when the tab's renderer is replaced: the JS side that
    /// owned these sessions is gone.
    func cancelAttachmentUploads() async {
        let removed = await sessions.cancelAll()
        guard !removed.isEmpty else { return }
        for session in removed {
            attachmentDeliveries.removeValue(forKey: session.chunkStreamId)
        }
        guard let reference = try? await startup?.task.value else { return }
        for session in removed {
            try? await reference.unregisterStream(streamId: session.resultStreamId)
            try? await reference.unregisterStream(streamId: session.chunkStreamId)
        }
    }

    /// Decodes one result-stream chunk. Anything that is not a decodable
    /// terminal `Ask_AttachmentUploadResult` becomes an explicit FAILED
    /// event so JS settles instead of waiting for its own timeout.
    private func receiveAttachmentResult(streamId: String, payload: Data) async {
        guard let session = await sessions.session(forResultStreamId: streamId) else { return }
        let failedStatus = Int32(Ask_AttachmentUploadStatus.failed.rawValue)
        let event: RealmServiceAttachmentResultEvent
        if let result = try? Ask_AttachmentUploadResult(serializedBytes: payload) {
            switch result.status {
            case .completed:
                if result.id.isEmpty {
                    event = RealmServiceAttachmentResultEvent(
                        streamId: streamId,
                        attachmentId: session.attachmentId,
                        status: failedStatus,
                        statusCode: -1,
                        errorMessage: "The upload completed without an attachment id."
                    )
                } else {
                    event = RealmServiceAttachmentResultEvent(
                        streamId: streamId,
                        attachmentId: result.id,
                        status: Int32(Ask_AttachmentUploadStatus.completed.rawValue),
                        statusCode: result.statusCode
                    )
                }
            case .failed:
                event = RealmServiceAttachmentResultEvent(
                    streamId: streamId,
                    attachmentId: session.attachmentId,
                    status: failedStatus,
                    statusCode: result.statusCode,
                    errorMessage: result.errorMessage.isEmpty
                        ? "The attachment upload failed."
                        : result.errorMessage
                )
            default:
                event = RealmServiceAttachmentResultEvent(
                    streamId: streamId,
                    attachmentId: session.attachmentId,
                    status: failedStatus,
                    statusCode: result.statusCode,
                    errorMessage: "The upload result had a non-terminal status (\(result.status.rawValue))."
                )
            }
        } else {
            event = RealmServiceAttachmentResultEvent(
                streamId: streamId,
                attachmentId: session.attachmentId,
                status: failedStatus,
                statusCode: -1,
                errorMessage: "The upload result could not be decoded."
            )
        }
        // Claim the delivery before awaiting cleanup. Only the first result
        // on this stream may settle the upload; a duplicate chunk that races
        // through actor reentrancy would otherwise deliver twice.
        let deliver = attachmentDeliveries.removeValue(forKey: session.chunkStreamId)
        await cleanupAttachmentSession(
            attachmentId: session.attachmentId,
            streamId: session.chunkStreamId
        )
        deliver?(event)
    }

    /// Terminal cleanup: drop the session and best-effort unregister both
    /// streams. Safe to call repeatedly. The chunk stream scopes the removal
    /// to one upload attempt, so a late failure from an abandoned attempt
    /// cannot delete a retry that reuses the attachment id.
    private func cleanupAttachmentSession(attachmentId: String, streamId: String) async {
        guard let session = await sessions.cancel(attachmentId: attachmentId, streamId: streamId) else {
            return
        }
        attachmentDeliveries.removeValue(forKey: session.chunkStreamId)
        guard let reference = try? await startup?.task.value else { return }
        try? await reference.unregisterStream(streamId: session.resultStreamId)
        try? await reference.unregisterStream(streamId: session.chunkStreamId)
    }

    /// Internal for the shutdown-race tests: a stopped binding must reject
    /// before it creates or reuses a startup task.
    func connect() async throws -> ActrRefWrapper {
        // A caller may have been suspended on another actor (for example while
        // registering an upload session) when `stop()` ran. Re-check on this
        // actor before starting or reusing a startup task so teardown can
        // never resurrect a Realm node.
        guard !stopped else { throw CancellationError() }
        if let startup { return try await startup.task.value }
        let id = UUID()
        let task = Task {
            let membership = try RealmIdentityStore().membership(
                realm: profile.realmName, providerId: profile.providerId, issuer: profile.issuerUrl
            )
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
            let configURL = directory.appendingPathComponent("actr.toml")
            try configuration.write(to: configURL, atomically: true, encoding: .utf8)
            let node = try await ActrBindings.ActrNode.newFromFrontWithMembership(
                configPath: configURL.path, frontArchive: front, serviceType: serviceType, membership: membership
            )
            let reference = try await node.start()
            do {
                try Task.checkCancellation()
                return reference
            } catch {
                reference.shutdown()
                await reference.waitForShutdown()
                throw error
            }
        }
        startup = (id, task)
        do {
            return try await task.value
        } catch {
            if startup?.id == id {
                try? FileManager.default.removeItem(at: directory)
                startup = nil
            }
            throw error
        }
    }

    func stop() async {
        stopped = true
        if let startup {
            startup.task.cancel()
            let reference = try? await startup.task.value
            // Unregister every attachment stream before the runtime goes
            // down so a restarting Service never sees ghost streams. Local
            // bookkeeping is dropped even when startup never produced a
            // usable runtime.
            let removed = await sessions.cancelAll()
            for session in removed {
                attachmentDeliveries.removeValue(forKey: session.chunkStreamId)
                if let reference {
                    try? await reference.unregisterStream(streamId: session.resultStreamId)
                    try? await reference.unregisterStream(streamId: session.chunkStreamId)
                }
            }
            if let reference {
                reference.shutdown()
                await reference.waitForShutdown()
            }
            self.startup = nil
        } else {
            // No runtime to unregister from; drop local bookkeeping only.
            let removed = await sessions.cancelAll()
            for session in removed {
                attachmentDeliveries.removeValue(forKey: session.chunkStreamId)
            }
        }
        try? FileManager.default.removeItem(at: directory)
    }
}
