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

protocol RealmServiceBindingHandle: AnyObject, Sendable {
    var id: UUID { get }
    var attachmentUploadAvailable: Bool { get async }
    func call(_ request: RealmServiceCall) async throws -> Data
    func subscribe(events: [String], generation: UInt64) async throws
    func suspendEvents(ifGeneration generation: UInt64) async
    func prepareAttachmentUpload(
        _ request: RealmServiceAttachmentPrepareCall,
        deliver: @escaping @Sendable (RealmServiceAttachmentResultEvent) -> Void
    ) async -> RealmServiceAttachmentPrepareReply
    func sendAttachmentChunk(
        _ request: RealmServiceAttachmentChunkCall
    ) async -> RealmServiceAttachmentChunkReply
    func unregisterAttachmentStream(
        _ request: RealmServiceAttachmentUnregisterCall
    ) async -> RealmServiceAttachmentUnregisterReply
    func cancelAttachmentUploads() async
    func stop() async
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

private final class RealmServiceReplyStreamCallback: ActrBindings.DataChunkCallback {
    private let target: ActrBindings.ActrId
    private let deliver: @Sendable (ActrBindings.DataChunk) async -> Void

    init(target: ActrBindings.ActrId, deliver: @escaping @Sendable (ActrBindings.DataChunk) async -> Void) {
        self.target = target
        self.deliver = deliver
    }

    func onStream(chunk: ActrBindings.DataChunk, sender: ActrBindings.ActrId) async throws {
        guard sender == target else { return }
        await deliver(chunk)
    }
}

/// A lock-protected intake snapshot for the nonisolated Rust event callback.
/// The MainActor registry remains authoritative; queued delivery rechecks each
/// captured subscription ID so a replacement can never inherit an old event.
private final class RealmServiceEventRoutingSnapshot: @unchecked Sendable {
    private let lock = NSLock()
    private var subscriptions: [UUID: (bindingId: UUID, events: Set<String>)] = [:]

    func replace(with subscriptions: [UUID: (bindingId: UUID, events: Set<String>)]) {
        lock.lock()
        self.subscriptions = subscriptions
        lock.unlock()
    }

    func matching(bindingId: UUID, event: String) -> [UUID] {
        lock.lock()
        defer { lock.unlock() }
        return subscriptions.compactMap { subscriptionId, subscription in
            subscription.bindingId == bindingId && subscription.events.contains(event)
                ? subscriptionId : nil
        }
    }
}

@MainActor
final class RealmServiceBindings {
    static let shared = RealmServiceBindings()
    private var bindings: [UUID: any RealmServiceBindingHandle] = [:]
    private var replyStreams: [UUID: (tabId: UUID, binding: RealmServiceBinding)] = [:]
    private var bindingTabs: [UUID: UUID] = [:]
    private var subscriptions: [UUID: (bindingId: UUID, events: Set<String>)] = [:]
    private let routingSnapshot = RealmServiceEventRoutingSnapshot()
    private var pendingSubscriptions: [UUID: UUID] = [:]
    private var cancelledSubscriptions: Set<UUID> = []
    private var eventGeneration: UInt64 = 0
    private var bindingEventGenerations: [UUID: UInt64] = [:]
    var eventHandler: ((UUID, RealmServiceEvent) -> Void)?

    func binding(for tabId: UUID) -> RealmServiceBinding? {
        bindings[tabId] as? RealmServiceBinding
    }

    private func updateRoutingSnapshot() {
        routingSnapshot.replace(with: subscriptions)
    }

    func attach(_ binding: any RealmServiceBindingHandle, to tabId: UUID) {
        remove(tabId)
        bindings[tabId] = binding
        bindingTabs[binding.id] = tabId
    }

    func localFrontBinding(for tabId: UUID) -> RealmServiceBinding? {
        bindings[tabId] as? RealmServiceBinding
    }

    /// Keep established Guest subscriptions when only the Realm connection changes.
    func replaceConnection(
        _ active: any RealmServiceBindingHandle,
        with replacement: any RealmServiceBindingHandle,
        tabId: UUID,
        isCurrent: () throws -> Bool
    ) async throws -> Bool {
        func eventRoutes() -> [UUID: Set<String>] {
            subscriptions.filter { $0.value.bindingId == active.id }.mapValues { $0.events }
        }

        guard bindings[tabId] === active, !pendingSubscriptions.values.contains(tabId) else { return false }
        let routes = eventRoutes()
        var generation: UInt64?
        if !routes.isEmpty {
            eventGeneration &+= 1
            let nextGeneration = eventGeneration
            try await replacement.subscribe(events: Array(Set(routes.values.joined())), generation: nextGeneration)
            generation = nextGeneration
        }
        try Task.checkCancellation()
        guard bindings[tabId] === active, !pendingSubscriptions.values.contains(tabId),
              eventRoutes() == routes, try isCurrent() else { return false }

        attach(replacement, to: tabId)
        for (id, events) in routes {
            subscriptions[id] = (replacement.id, events)
        }
        bindingEventGenerations[replacement.id] = generation
        updateRoutingSnapshot()
        return true
    }

    func remove(_ tabId: UUID) {
        retireReplyStreams(for: tabId)
        guard let binding = bindings.removeValue(forKey: tabId) else { return }
        bindingTabs.removeValue(forKey: binding.id)
        bindingEventGenerations.removeValue(forKey: binding.id)
        subscriptions = subscriptions.filter { $0.value.bindingId != binding.id }
        updateRoutingSnapshot()
        Task { await binding.stop() }
    }

    func call(_ request: RealmServiceCall, tabId: UUID) async throws -> Data {
        guard let binding = bindings[tabId] else {
            throw RealmServiceError(message: "The tab has no Realm Service.")
        }
        return try await binding.call(request)
    }

    func subscribe(_ request: RealmServiceEventSubscription, tabId: UUID) async throws {
        if let streamId = request.streamId {
            guard request.events.isEmpty else {
                throw RealmServiceError(message: "A DataStream registration must contain only a stream ID.")
            }
            try await subscribeReplyStream(streamId, subscriptionId: request.id, tabId: tabId)
            return
        }
        guard let binding = bindings[tabId] else {
            throw RealmServiceError(message: "The tab has no Realm Service.")
        }
        pendingSubscriptions[request.id] = tabId
        eventGeneration &+= 1
        let generation = eventGeneration
        bindingEventGenerations[binding.id] = generation
        // The actor may publish as soon as subscribe selects its event peer,
        // before the subscribe reply crosses the Renderer/Host IPC boundary.
        // Register first so publish can already address this subscription.
        subscriptions[request.id] = (binding.id, Set(request.events))
        updateRoutingSnapshot()
        defer {
            pendingSubscriptions.removeValue(forKey: request.id)
            cancelledSubscriptions.remove(request.id)
        }
        func rollback() async {
            subscriptions.removeValue(forKey: request.id)
            updateRoutingSnapshot()
            if !subscriptions.values.contains(where: { $0.bindingId == binding.id }) {
                // This request may be an older, superseded generation. If it
                // removes the last routing entry, tear down the latest peer
                // selected for the binding rather than leaving it enabled.
                let teardownGeneration = bindingEventGenerations[binding.id] ?? generation
                await binding.suspendEvents(ifGeneration: teardownGeneration)
            }
        }
        do {
            try await binding.subscribe(events: request.events, generation: generation)
        } catch {
            await rollback()
            throw error
        }
        guard bindings[tabId] === binding, !cancelledSubscriptions.contains(request.id) else {
            await rollback()
            throw CancellationError()
        }
    }

    func unsubscribe(_ subscriptionId: UUID, tabId: UUID) {
        if let stream = replyStreams[subscriptionId], stream.tabId == tabId {
            replyStreams.removeValue(forKey: subscriptionId)
            Task { await stream.binding.unregisterReplyStream(subscriptionId) }
            return
        }
        if pendingSubscriptions[subscriptionId] == tabId {
            cancelledSubscriptions.insert(subscriptionId)
            return
        }
        guard let existing = subscriptions[subscriptionId],
              let tab = bindingTabs[existing.bindingId], tab == tabId,
              let binding = bindings[tabId] else { return }
        subscriptions.removeValue(forKey: subscriptionId)
        updateRoutingSnapshot()
        if !subscriptions.values.contains(where: { $0.bindingId == existing.bindingId }),
           let generation = bindingEventGenerations[existing.bindingId] {
            Task { @MainActor in
                // A new subscription may have appeared before this task runs.
                guard !subscriptions.values.contains(where: {
                    $0.bindingId == existing.bindingId
                }) else { return }
                await binding.suspendEvents(ifGeneration: generation)
            }
        }
    }

    /// Retires every event subscription owned by a tab's renderer without
    /// stopping the Front. A crashed renderer cannot run its JS cleanup, and
    /// crash recovery reuses the tab ID, so both established and still-pending
    /// registrations must stop receiving events immediately.
    func retireSubscriptions(for tabId: UUID) {
        retireReplyStreams(for: tabId)
        guard let binding = bindings[tabId] else { return }
        let bindingId = binding.id
        let retired = subscriptions.filter { $0.value.bindingId == bindingId }
        for (subscriptionId, _) in retired {
            subscriptions.removeValue(forKey: subscriptionId)
        }
        updateRoutingSnapshot()
        // Keep this robust even if a pending registration has not entered the
        // early-routing table yet. The late reply still rolls back through
        // cancelledSubscriptions, but no renderer-owned pending state remains.
        let pending = pendingSubscriptions.filter { $0.value == tabId }.map(\.key)
        for subscriptionId in pending {
            cancelledSubscriptions.insert(subscriptionId)
            pendingSubscriptions.removeValue(forKey: subscriptionId)
        }
        guard !retired.isEmpty,
              !subscriptions.values.contains(where: { $0.bindingId == bindingId }),
              let generation = bindingEventGenerations[bindingId] else { return }
        Task { @MainActor in
            // A replacement renderer may already have subscribed again.
            guard !subscriptions.values.contains(where: { $0.bindingId == bindingId }) else { return }
            await binding.suspendEvents(ifGeneration: generation)
        }
    }

    private func subscribeReplyStream(_ streamId: String, subscriptionId: UUID, tabId: UUID) async throws {
        guard let binding = binding(for: tabId), replyStreams[subscriptionId] == nil else {
            throw RealmServiceError(message: "The tab has no available Realm Service DataStream binding.")
        }
        replyStreams[subscriptionId] = (tabId, binding)
        do {
            try await binding.registerReplyStream(streamId, subscriptionId: subscriptionId) { [weak self] chunk in
                await self?.publishReplyChunk(chunk, subscriptionId: subscriptionId)
            }
            guard replyStreams[subscriptionId]?.binding === binding else { throw CancellationError() }
        } catch {
            replyStreams.removeValue(forKey: subscriptionId)
            throw error
        }
    }

    private func publishReplyChunk(_ chunk: ActrBindings.DataChunk, subscriptionId: UUID) {
        guard let stream = replyStreams[subscriptionId] else { return }
        eventHandler?(stream.tabId, RealmServiceEvent(
            subscriptionId: subscriptionId, event: chunk.streamId, payload: chunk.payload, sequence: chunk.sequence
        ))
    }

    private func retireReplyStreams(for tabId: UUID) {
        for id in replyStreams.filter({ $0.value.tabId == tabId }).keys {
            unsubscribe(id, tabId: tabId)
        }
    }

    nonisolated func publish(bindingId: UUID, event: String, payload: Data) {
        // Capture ownership before hopping to the MainActor. Enumerating only
        // inside the task would let a replacement subscription inherit an
        // event that arrived while its predecessor was still registered.
        let eligible = routingSnapshot.matching(bindingId: bindingId, event: event)
        Task { @MainActor in
            guard let tabId = bindingTabs[bindingId] else { return }
            for subscriptionId in eligible {
                guard let subscription = subscriptions[subscriptionId],
                      subscription.bindingId == bindingId,
                      subscription.events.contains(event) else { continue }
                eventHandler?(tabId, RealmServiceEvent(
                    subscriptionId: subscriptionId, event: event, payload: payload
                ))
            }
        }
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

/// The Rust callback is synchronous so `publish` snapshots eligible
/// subscriptions before any executor hop can observe a replacement.
final class RealmServiceEventRelay: ActrBindings.ServiceEventListener, @unchecked Sendable {
    private let bindingId: UUID
    private let registry: RealmServiceBindings

    init(bindingId: UUID, registry: RealmServiceBindings = .shared) {
        self.bindingId = bindingId
        self.registry = registry
    }

    func onEvent(event: ActrBindings.ServiceEventBridge) {
        registry.publish(
            bindingId: bindingId, event: event.event, payload: event.payload
        )
    }
}

/// The discovery and peer-selection surface used by Service event routing.
/// Internal so tests can drive out-of-order discoveries without a live Realm.
protocol RealmServiceEventReference: AnyObject, Sendable {
    func discoverService(serviceType: ActrBindings.ActrType) async throws -> ActrBindings.ActrId
    func setServiceEventPeer(peer: ActrBindings.ActrId?) throws
}

extension ActrRefWrapper: RealmServiceEventReference {}

/// Owns the Front's authenticated event peer. Discovery suspends, so the
/// generation is checked again when its result is committed: an older
/// subscription or a retired renderer must never overwrite a newer selection.
actor RealmServiceEventPeerController {
    private var generation: UInt64 = 0
    private var reference: (any RealmServiceEventReference)?
    private(set) var peer: ActrBindings.ActrId?
    private var discoverySequence: UInt64 = 0
    private var committedDiscoverySequence: UInt64 = 0

    struct Discovery: Sendable {
        let target: ActrBindings.ActrId
        fileprivate let generation: UInt64?
        fileprivate let sequence: UInt64
    }

    func select(
        generation: UInt64,
        reference: any RealmServiceEventReference,
        target: ActrBindings.ActrId
    ) throws {
        self.generation = max(self.generation, generation)
        guard self.generation == generation else { throw CancellationError() }
        // The caller has already established this exact RuntimeConnection.
        // Never rediscover and publish a different peer before its Attach succeeds.
        try reference.setServiceEventPeer(peer: target)
        self.reference = reference
        peer = target
        committedDiscoverySequence = discoverySequence
    }

    /// A generation snapshot for `call` only when a subscription is active.
    /// A call that started before the first subscription must not select a peer.
    func activeGeneration() -> UInt64? {
        reference == nil ? nil : generation
    }

    /// Strictly rediscover the Service for an RPC. The returned discovery is only
    /// a candidate: event routing changes after its replacement Attach succeeds.
    func resolve(
        ifGeneration generation: UInt64?,
        reference: any RealmServiceEventReference,
        serviceType: ActrBindings.ActrType
    ) async throws -> Discovery {
        discoverySequence &+= 1
        let sequence = discoverySequence
        let target = try await reference.discoverService(serviceType: serviceType)
        return Discovery(target: target, generation: generation, sequence: sequence)
    }

    /// Publish a successfully attached RuntimeConnection without letting a slow,
    /// older discovery overwrite a newer selection or revive a cleared peer.
    func commit(
        _ discovery: Discovery,
        reference: any RealmServiceEventReference,
        target: ActrBindings.ActrId
    ) {
        guard let generation = discovery.generation else { return }
        refresh(
            ifGeneration: generation, reference: reference, target: target,
            sequence: discovery.sequence
        )
    }

    /// Follow Service rediscovery without reviving a cleared subscription,
    /// overwriting a newer generation, or committing an older discovery result.
    /// Event-routing failure must not fail RPC.
    private func refresh(
        ifGeneration generation: UInt64,
        reference: any RealmServiceEventReference,
        target: ActrBindings.ActrId,
        sequence: UInt64
    ) {
        guard self.generation == generation, self.reference != nil,
              sequence >= committedDiscoverySequence else { return }
        do {
            try reference.setServiceEventPeer(peer: target)
            self.reference = reference
            peer = target
            committedDiscoverySequence = sequence
        } catch {
            return
        }
    }

    func suspend(ifGeneration generation: UInt64) {
        // The manager may already have issued a newer generation whose
        // discovery never reached this actor. That newer empty-set teardown
        // must still clear the current peer; only an older teardown is stale.
        guard generation >= self.generation else { return }
        clear()
    }

    func clear() {
        // Invalidate an in-flight discovery as well as the committed peer.
        generation &+= 1
        try? reference?.setServiceEventPeer(peer: nil)
        reference = nil
        peer = nil
    }
}

actor RealmServiceBinding: RealmServiceBindingHandle {
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

    struct Connection: Equatable, Sendable {
        let realm: String
        let providerId: String
        let issuerURL: String
        let signalingURL: String
        let stunURLs: [String]
        let turnURLs: [String]

        init(profile: RealmProfileInfo, verifiedProfileBody: Data) throws {
            let provider = try JSONDecoder().decode(ProfileDocument.self, from: verifiedProfileBody).provider
            guard let signaling = provider.signaling.sorted(by: { $0.priority < $1.priority }).first,
                  URL(string: signaling.url)?.scheme?.lowercased() == "wss" else {
                throw RealmServiceError(message: "The verified Realm profile has invalid signaling information.")
            }
            let iceURLs = provider.ice.flatMap(\.urls)
            realm = profile.realmName
            providerId = profile.providerId
            issuerURL = profile.issuerUrl
            signalingURL = signaling.url
            stunURLs = iceURLs.filter { $0.hasPrefix("stun:") || $0.hasPrefix("stuns:") }
            turnURLs = iceURLs.filter { $0.hasPrefix("turn:") || $0.hasPrefix("turns:") }
        }
    }

    /// The running Front node plus its verified Attach session. Both live for
    /// as long as the tab; the session heartbeat runs inside the peer runtime.
    struct RuntimeConnection: Sendable {
        let reference: ActrRefWrapper
        let session: ActrPeerSessionWrapper
        let target: ActrBindings.ActrId
    }

    private struct Capabilities: Decodable {
        struct Service: Decodable {
            let rpc: [String]?
            let events: [String]?
        }
        let hostModules: [String: Service]
    }

    let id: UUID
    private let directory = FileManager.default.temporaryDirectory
        .appendingPathComponent("actrium-front-" + UUID().uuidString, isDirectory: true)
    nonisolated let connection: Connection
    private let front: Data
    private let metadata: ActriumSessionMetadata
    private let frontType: String
    private let trustedFrontKey: Data
    private let configuration: String
    private let serviceType: ActrBindings.ActrType
    /// Business routes; internal for the attachment transport wiring tests.
    let routes: Set<String>
    private let routeCapabilities: [String: String]
    private let events: Set<String>
    private let replyStreamsAllowed: Bool
    private let streamSendAllowed: Bool
    private struct ReplyStream {
        let streamId: String
        var reference: ActrRefWrapper?
        var cancelled = false
    }
    private var replyStreams: [UUID: ReplyStream] = [:]
    private let eventRelay: RealmServiceEventRelay
    private let eventPeers = RealmServiceEventPeerController()
    /// `true` only when the manifest grants `service.attachmentUpload` AND the
    /// capability manifest declares the reserved `uploadAttachment` transport.
    /// Internal for the attachment transport wiring tests.
    let attachmentUploadAllowed: Bool
    private let sessions = RealmServiceAttachmentSessionStore()
    /// Keyed by the unique registration attempt. Public stream IDs and the
    /// caller-supplied attachment ID may both be reused by a later upload.
    private var attachmentDeliveries: [UUID: @Sendable (RealmServiceAttachmentResultEvent) -> Void] = [:]
    private var startup: (id: UUID, task: Task<RuntimeConnection, Error>)?
    private var recovery: (id: UUID, task: Task<RuntimeConnection, Error>, discoversTarget: Bool)?
    /// The detached old session retained while a verified Local Front
    /// replacement attaches. Ordinary calls stay blocked until commit/rollback.
    private var localFrontHandoff: RuntimeConnection?
    private var stopped = false

    init(
        profile: RealmProfileInfo,
        verifiedProfileBody: Data,
        front: Data,
        metadata: ActriumSessionMetadata,
        trustedFrontKey: Data,
        capabilitiesJSON: String,
        permissions: [String]
    ) throws {
        let connection = try Connection(profile: profile, verifiedProfileBody: verifiedProfileBody)
        let capabilities = try JSONDecoder().decode(Capabilities.self, from: Data(capabilitiesJSON.utf8))
        let services = ["service", "host:service"].compactMap { capabilities.hostModules[$0] }
        let stream = capabilities.hostModules["service-stream"] ?? capabilities.hostModules["host:service-stream"]
        // Verified session metadata owns Attach routes and capabilities. The
        // signed capability manifest additionally declares Service events.
        let declaredRoutes = Set(metadata.routes.map(\.route))
        try self.init(
            connection: connection,
            front: front,
            metadata: metadata,
            frontType: profile.packageType,
            serviceType: ActrBindings.ActrType(
                publisher: metadata.program.publisher,
                name: metadata.program.name,
                version: metadata.program.version
            ),
            trustedFrontKey: trustedFrontKey,
            routes: RealmServiceAttachmentUploadPolicy.businessRoutes(declaredRoutes),
            events: Set(services.flatMap { $0.events ?? [] }),
            replyStreamsAllowed: permissions.contains("host:service-stream.register")
                && stream?.rpc?.contains("register") == true,
            streamSendAllowed: permissions.contains("host:service-stream.send")
                && stream?.rpc?.contains("send") == true,
            attachmentUploadAllowed: RealmServiceAttachmentUploadPolicy.isPermitted(
                permissions: permissions, declaredRoutes: declaredRoutes
            )
        )
    }

    private init(
        connection: Connection,
        front: Data,
        metadata: ActriumSessionMetadata,
        frontType: String,
        serviceType: ActrBindings.ActrType,
        trustedFrontKey: Data,
        routes: Set<String>,
        events: Set<String>,
        replyStreamsAllowed: Bool,
        streamSendAllowed: Bool,
        attachmentUploadAllowed: Bool
    ) throws {
        self.connection = connection
        let bindingId = UUID()
        self.id = bindingId
        self.eventRelay = RealmServiceEventRelay(bindingId: bindingId)
        self.front = front
        self.metadata = metadata
        self.frontType = frontType
        self.serviceType = serviceType
        self.trustedFrontKey = trustedFrontKey
        self.routes = routes
        self.routeCapabilities = metadata.routes.reduce(into: [String: String]()) { capabilities, route in
            capabilities[route.route] = route.capabilities.first
        }
        self.events = events
        self.replyStreamsAllowed = replyStreamsAllowed
        self.streamSendAllowed = streamSendAllowed
        self.attachmentUploadAllowed = attachmentUploadAllowed

        let encoder = JSONEncoder()
        encoder.outputFormatting = [.withoutEscapingSlashes]
        func literal<T: Encodable>(_ value: T) throws -> String {
            try String(decoding: encoder.encode(value), as: UTF8.self)
        }
        self.configuration = try """
        edition = 1
        [signaling]
        url = \(literal(connection.signalingURL))
        [issuer_endpoint]
        url = \(literal(connection.issuerURL))
        [deployment]
        realm = \(literal(connection.realm))
        [hyper]
        data_dir = \(literal(directory.path))
        [discovery]
        visible = false
        [webrtc]
        stun_urls = \(literal(connection.stunURLs))
        turn_urls = \(literal(connection.turnURLs))
        [[acl.rules]]
        permission = "allow"
        type = \(literal("\(serviceType.publisher)/\(serviceType.name)@\(serviceType.version)"))
        realm = "self"
        [[trust]]
        kind = "static"
        pubkey_b64 = \(literal(trustedFrontKey.base64EncodedString()))
        """
    }

    func replacingConnection(_ connection: Connection) throws -> RealmServiceBinding {
        guard !stopped else { throw CancellationError() }
        return try RealmServiceBinding(
            connection: connection, front: front, metadata: metadata, frontType: frontType,
            serviceType: serviceType, trustedFrontKey: trustedFrontKey, routes: routes, events: events,
            replyStreamsAllowed: replyStreamsAllowed, streamSendAllowed: streamSendAllowed,
            attachmentUploadAllowed: attachmentUploadAllowed
        )
    }

    /// Prepare a replacement binding without sending a business RPC.
    func prepareForSwitch() async throws {
        try Task.checkCancellation()
        _ = try await connect()
        guard !stopped else { throw CancellationError() }
        try Task.checkCancellation()
    }

    func call(_ request: RealmServiceCall) async throws -> Data {
        guard !stopped, routes.contains(request.route),
              (1...300_000).contains(request.timeoutMs) else {
            throw RealmServiceError(message: "The Service call is outside this tab's declared capabilities.")
        }
        if let chunk = request.streamChunk {
            try await sendStreamChunk(chunk, request: request)
            return Data()
        }
        guard request.payload.count <= 1_048_576 else {
            throw RealmServiceError(message: "The Service call payload exceeds the limit.")
        }
        let eventGeneration = await eventPeers.activeGeneration()
        let connection = try await readyConnection(
            resolveTargetForCall: true,
            eventGeneration: eventGeneration
        )
        guard !stopped else { throw CancellationError() }
        return try await connection.session.call(
            route: request.route,
            capability: routeCapabilities[request.route] ?? "service",
            payload: request.payload,
            timeoutMs: UInt64(request.timeoutMs)
        )
    }

    private func sendStreamChunk(_ chunk: RealmServiceCall.StreamChunk, request: RealmServiceCall) async throws {
        guard streamSendAllowed, chunk.sequence > 0, !request.payload.isEmpty,
              request.payload.count <= RealmServiceCall.maxStreamChunkBytes,
              chunk.id.range(of: #"\A[A-Za-z0-9_-]{1,128}\z"#, options: .regularExpression) != nil else {
            throw RealmServiceError(message: "The Service DataStream send is outside this tab's declared capabilities.")
        }
        let connection = try await readyConnection()
        guard !stopped else { throw CancellationError() }
        try await connection.session.sendDataChunk(
            route: request.route,
            capability: routeCapabilities[request.route] ?? "service",
            chunk: ActrBindings.DataChunk(
                streamId: chunk.id, sequence: chunk.sequence, payload: request.payload, metadata: [],
                timestampMs: Int64(Date().timeIntervalSince1970 * 1000)
            ),
            payloadType: .streamReliable,
            timeoutMs: UInt64(request.timeoutMs)
        )
    }

    func prepareLocalFront() async throws {
        // connect() discovers the selected local Service and establishes its
        // authenticated Attach session before the renderer is installed.
        _ = try await connect()
    }

    /// Release the old lease only after its replacement has been verified.
    /// The node and Actor identity remain alive so a failed replacement can
    /// restore an authenticated session without restarting the old renderer.
    func suspendForLocalFrontReplacement() async throws {
        guard localFrontHandoff == nil else { return }
        let connection = try await readyConnection()
        localFrontHandoff = connection
        try await connection.session.detach()
    }

    /// Re-Attach the old Front after a failed replacement. Calls remain blocked
    /// while this is in flight, and a later retry can use the retained node.
    func restoreAfterFailedLocalFrontReplacement() async throws {
        guard !stopped, let handoff = localFrontHandoff else { return }
        try? await handoff.session.detach()
        let attached = try await attach(handoff.reference)
        do {
            try Task.checkCancellation()
            guard !stopped else { throw CancellationError() }
            let replacement = RuntimeConnection(
                reference: handoff.reference,
                session: attached.session,
                target: attached.target
            )
            startup = (UUID(), Task { replacement })
            localFrontHandoff = nil
        } catch {
            try? await attached.session.detach()
            throw error
        }
    }

    private func findService(using reference: ActrRefWrapper) async throws -> ActrBindings.ActrId {
        // Every Front, downloaded or local, must attach to the unique Service
        // selected by the verified Realm Profile rather than an arbitrary peer.
        try await reference.discoverService(serviceType: serviceType)
    }

    func subscribe(events requestedEvents: [String], generation: UInt64) async throws {
        let requested = Set(requestedEvents)
        guard !stopped, !requested.isEmpty, requested.isSubset(of: events) else {
            throw RealmServiceError(message: "The Service event subscription is outside this tab's declared capabilities.")
        }
        let connection = try await readyConnection()
        guard !stopped else { throw CancellationError() }
        try await eventPeers.select(
            generation: generation,
            reference: connection.reference,
            target: connection.target
        )
        guard !stopped else {
            await eventPeers.suspend(ifGeneration: generation)
            throw CancellationError()
        }
    }

    func suspendEvents(ifGeneration generation: UInt64) async {
        await eventPeers.suspend(ifGeneration: generation)
    }

    private func clearEventPeer() async {
        await eventPeers.clear()
    }

    func registerReplyStream(
        _ streamId: String,
        subscriptionId: UUID,
        deliver: @escaping @Sendable (ActrBindings.DataChunk) async -> Void
    ) async throws {
        guard replyStreamsAllowed, !stopped,
              streamId.range(of: #"\A[A-Za-z0-9_-]{1,128}\z"#, options: .regularExpression) != nil,
              !replyStreams.values.contains(where: { $0.streamId == streamId }) else {
            throw RealmServiceError(message: "The Service DataStream cannot be registered.")
        }
        replyStreams[subscriptionId] = ReplyStream(streamId: streamId)
        do {
            let connection = try await readyConnection(resolveTargetForCall: true)
            guard replyStreams[subscriptionId]?.cancelled == false else { throw CancellationError() }
            let callback = RealmServiceReplyStreamCallback(target: connection.target, deliver: deliver)
            try await connection.reference.registerServiceStream(
                streamId: streamId, service: connection.target, callback: callback
            )
            replyStreams[subscriptionId]?.reference = connection.reference
            if stopped || replyStreams[subscriptionId]?.cancelled == true {
                try await connection.reference.unregisterStream(streamId: streamId)
                throw CancellationError()
            }
        } catch {
            replyStreams.removeValue(forKey: subscriptionId)
            throw error
        }
    }

    func unregisterReplyStream(_ subscriptionId: UUID) async {
        guard let stream = replyStreams[subscriptionId], !stream.cancelled else { return }
        replyStreams[subscriptionId]?.cancelled = true
        guard let reference = stream.reference else { return }
        defer { replyStreams.removeValue(forKey: subscriptionId) }
        do {
            try await reference.unregisterStream(streamId: stream.streamId)
        } catch {
            NSLog("Could not unregister Service DataStream: %@", error.localizedDescription)
        }
    }

    /// Availability as seen by this binding: the manifest must opt in and the
    /// binding must still be running. Actor-isolated so a race with `stop()`
    /// cannot report a binding that is already tearing down.
    var attachmentUploadAvailable: Bool {
        attachmentUploadAllowed && !stopped
    }

    /// Prepares one attachment upload: validates capability and fields, binds
    /// the prepare RPC and every later chunk to the same Attach session,
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
            // Register before the first suspension: connect(), recovery, and
            // stream registration may take seconds, and a renderer
            // cancellation during that window must find this upload instead
            // of letting it start afterwards.
            let attemptId = try await sessions.registerPreparing(call, checksum: checksum)
            do {
                let connection = try await readyConnection()
                let reference = connection.reference
                guard !stopped,
                      await sessions.isCurrent(attachmentId: call.attachmentId, streamId: call.streamId) else {
                    throw RealmServiceAttachmentUploadError.cancelled
                }
                // Prepare and every chunk use the target fixed by this Attach;
                // a second Discover could select a different Service instance.
                // The result delivery must exist before registration: the
                // Service may answer the prepare RPC and publish its terminal
                // result while this actor is still suspended on the RPC.
                attachmentDeliveries[attemptId] = deliver
                let callback = RealmServiceAttachmentResultCallback { [weak self] chunk in
                    let payload = chunk.payload
                    Task {
                        await self?.receiveAttachmentResult(
                            streamId: chunk.streamId,
                            attemptId: attemptId,
                            payload: payload
                        )
                    }
                }
                try await reference.registerStream(streamId: call.resultStreamId, callback: callback)
                // Cancellation may have run while registration was suspended.
                // The prepare-owning catch below performs the final unregister
                // only after this late registration has finished.
                guard await sessions.isCurrent(attachmentId: call.attachmentId, streamId: call.streamId) else {
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
                let response = try await connection.session.call(
                    route: RealmServiceAttachmentIPC.prepareRoute,
                    capability: routeCapabilities[RealmServiceAttachmentUploadPolicy.transportRoute]
                        ?? "service",
                    payload: request.serializedData(),
                    timeoutMs: UInt64(RealmServiceAttachmentIPC.prepareTimeoutMs)
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
                    target: connection.target
                )
                return RealmServiceAttachmentPrepareReply(
                    id: call.id,
                    attachmentId: call.attachmentId,
                    statusCode: reply.statusCode
                )
            } catch {
                await cleanupPreparingAttachmentSession(
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
            let connection = try await readyConnection()
            guard !stopped else { throw RealmServiceAttachmentUploadError.cancelled }
            let planned = try await sessions.planChunk(
                streamId: call.streamId,
                sequence: call.sequence,
                payloadBytes: call.payload.count,
                metadata: call.metadata
            )
            // A re-Attach may select a different Service instance. Its upload
            // state does not exist there, so never continue a prepared stream
            // against a new target; the caller must restart the upload.
            guard planned.session.target == connection.target else {
                await cleanupAttachmentSession(
                    attachmentId: planned.session.attachmentId,
                    streamId: planned.session.chunkStreamId,
                    attemptId: planned.session.attemptId
                )
                throw RealmServiceAttachmentUploadError.unavailable(
                    "The Service instance changed after reconnecting; restart the attachment upload.")
            }
            let chunk = ActrBindings.DataChunk(
                streamId: planned.session.chunkStreamId,
                sequence: planned.plan.sequence,
                payload: call.payload,
                metadata: call.metadata.map { ActrBindings.MetadataEntry(key: $0.key, value: $0.value) },
                timestampMs: Int64(Date().timeIntervalSince1970 * 1000)
            )
            do {
                try await connection.session.sendDataChunk(
                    route: RealmServiceAttachmentUploadPolicy.transportRoute,
                    capability: routeCapabilities[RealmServiceAttachmentUploadPolicy.transportRoute]
                        ?? "service",
                    chunk: chunk,
                    payloadType: .streamReliable,
                    timeoutMs: UInt64(RealmServiceAttachmentIPC.chunkTimeoutMs)
                )
            } catch {
                // The sequence window already moved; the session cannot
                // recover from a failed send. Scope the cleanup to this
                // attempt so a retry reusing the attachment id survives.
                await cleanupAttachmentSession(
                    attachmentId: planned.session.attachmentId,
                    streamId: planned.session.chunkStreamId,
                    attemptId: planned.session.attemptId
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
            await teardownAttachmentSession(session)
        }
        return RealmServiceAttachmentUnregisterReply(id: call.id)
    }

    /// Drops every upload session of this binding without delivering
    /// results. Used when the tab's renderer is replaced: the JS side that
    /// owned these sessions is gone.
    func cancelAttachmentUploads() async {
        let removed = await sessions.cancelAll()
        for session in removed {
            await teardownAttachmentSession(session)
        }
    }

    /// Decodes one result-stream chunk. Anything that is not a decodable
    /// terminal `Ask_AttachmentUploadResult` becomes an explicit FAILED
    /// event so JS settles instead of waiting for its own timeout.
    private func receiveAttachmentResult(streamId: String, attemptId: UUID, payload: Data) async {
        guard let session = await sessions.claimResult(streamId: streamId, attemptId: attemptId) else {
            return
        }
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
        // Delivery is attempt-scoped and claimed before cleanup. Only the
        // first result for this registration may settle the upload.
        let deliver = attachmentDeliveries.removeValue(forKey: session.attemptId)
        await teardownAttachmentSession(session)
        deliver?(event)
    }

    /// Starts terminal cleanup for a live upload. A preparing session stays
    /// reserved until its owning prepare call has finished any late stream
    /// registration and performs the final transport cleanup.
    private func cleanupAttachmentSession(
        attachmentId: String,
        streamId: String,
        attemptId: UUID
    ) async {
        guard let session = await sessions.cancel(
            attachmentId: attachmentId,
            streamId: streamId,
            attemptId: attemptId
        ) else {
            return
        }
        await teardownAttachmentSession(session)
    }

    /// Finishes the prepare call's own cleanup, including when cancellation
    /// already claimed the session while registration or the RPC was in flight.
    private func cleanupPreparingAttachmentSession(attachmentId: String, streamId: String) async {
        guard let session = await sessions.preparationCleanup(
            attachmentId: attachmentId,
            streamId: streamId
        ) else {
            return
        }
        await teardownAttachmentSession(session, includePreparing: true)
    }

    private func teardownAttachmentSession(
        _ session: RealmServiceAttachmentSession,
        includePreparing: Bool = false
    ) async {
        attachmentDeliveries.removeValue(forKey: session.attemptId)
        guard includePreparing || session.state != .preparing else { return }
        await unregisterAttachmentStreams(session)
    }

    /// Unregisters transport state and only then releases the session's public
    /// IDs. Until that point a replacement cannot register under the same IDs.
    private func unregisterAttachmentStreams(_ session: RealmServiceAttachmentSession) async {
        if let reference = await startedReference {
            try? await reference.unregisterStream(streamId: session.resultStreamId)
            try? await reference.unregisterStream(streamId: session.chunkStreamId)
        }
        await sessions.completeTeardown(session)
    }

    /// The runtime reference of an already started connection, if any.
    /// Attachment teardown must never start a new node just to unregister.
    private var startedReference: ActrRefWrapper? {
        get async {
            guard let startup else { return nil }
            return try? await startup.task.value.reference
        }
    }

    /// Return a connection whose session is valid on the current verified
    /// channel. Actor reentrancy is serialized through one shared recovery
    /// task, so concurrent requests cannot Resume or re-Attach independently.
    private func readyConnection(
        resolveTargetForCall: Bool = false,
        eventGeneration: UInt64? = nil
    ) async throws -> RuntimeConnection {
        guard !stopped else { throw CancellationError() }
        guard localFrontHandoff == nil else {
            throw RealmServiceError(message: "The Local Front is refreshing its Service session.")
        }
        if let recovery {
            if resolveTargetForCall && !recovery.discoversTarget {
                let id = recovery.id
                let recovered = try await Self.recoverCallAfterJoining(
                    recovery: recovery.task,
                    recover: { joined in
                        try await self.recoverJoinedCall(
                            joined: joined,
                            replacingRecovery: id,
                            eventGeneration: eventGeneration
                        )
                    },
                    restart: {
                        // A resume-only subscriber or upload may have joined the
                        // offline old peer first. Its failure must not prevent
                        // this RPC from discovering and attaching a replacement.
                        if self.recovery?.id == id {
                            self.recovery = nil
                        }
                        return try await self.readyConnection(
                            resolveTargetForCall: true,
                            eventGeneration: eventGeneration
                        )
                    }
                )
                guard !stopped else { throw CancellationError() }
                return recovered
            }
            // Target-aware recovery already includes discovery and any fresh
            // Attach. Every RPC joins that same transition instead of racing
            // an exclusive lease with an independent retarget.
            let recovered = try await recovery.task.value
            guard !stopped else { throw CancellationError() }
            return recovered
        }
        let id = UUID()
        // Reserve the shared slot before connect() can suspend this actor.
        // Every reentrant caller then awaits this same Resume/re-Attach.
        let task = Task<RuntimeConnection, Error> {
            let connection = try await self.connect()
            if resolveTargetForCall {
                // Discover before touching the old session. If A is offline and
                // B is now the unique Service, resuming A first would surface a
                // transport error and hide the available replacement.
                let discovery = try await self.eventPeers.resolve(
                    ifGeneration: eventGeneration,
                    reference: connection.reference,
                    serviceType: self.serviceType
                )
                return try await Self.recoverDiscoveredConnection(
                    currentTarget: connection.target,
                    discoveredTarget: discovery.target,
                    retarget: { try await self.retarget(connection, to: discovery.target) },
                    resume: { try await self.resumeOrReattach(connection) },
                    commit: { recovered in
                        await self.eventPeers.commit(
                            discovery, reference: recovered.reference, target: recovered.target
                        )
                    }
                )
            }
            return try await self.resumeOrReattach(connection)
        }
        recovery = (id: id, task: task, discoversTarget: resolveTargetForCall)
        return try await awaitRecovery(id: id, task: task)
    }

    /// Upgrade one completed resume-only recovery to a shared target-aware
    /// transition. Publishing the new task before any suspension ensures that
    /// concurrent RPC waiters cannot independently Attach the replacement.
    private func recoverJoinedCall(
        joined: RuntimeConnection,
        replacingRecovery previousID: UUID,
        eventGeneration: UInt64?
    ) async throws -> RuntimeConnection {
        guard !stopped else { throw CancellationError() }
        guard localFrontHandoff == nil else {
            throw RealmServiceError(message: "The Local Front is refreshing its Service session.")
        }
        if let recovery, recovery.id != previousID {
            return try await readyConnection(
                resolveTargetForCall: true,
                eventGeneration: eventGeneration
            )
        }
        // If the original owner has not published yet, the joined task may have
        // replaced an expired session with a fresh Attach to the same Actor.
        // Carry that result through the handoff instead of reading stale startup.
        let ownsRecoverySlot = recovery?.id == previousID
        let id = UUID()
        Self.publishJoinedRecovery(joined: joined, ownsRecoverySlot: ownsRecoverySlot) { connection in
            self.startup = (id, Task { connection })
        }
        let task = Task<RuntimeConnection, Error> {
            // Once the owner has published, use the latest connection. Another
            // RPC may already have finished the replacement before this waiter.
            let connection = try await Self.joinedRecoveryConnection(
                joined: joined,
                ownsRecoverySlot: ownsRecoverySlot,
                connect: { try await self.connect() }
            )
            let discovery = try await self.eventPeers.resolve(
                ifGeneration: eventGeneration,
                reference: connection.reference,
                serviceType: self.serviceType
            )
            return try await Self.recoverDiscoveredConnection(
                currentTarget: connection.target,
                discoveredTarget: discovery.target,
                retarget: { try await self.retarget(connection, to: discovery.target) },
                resume: { connection },
                commit: { recovered in
                    await self.eventPeers.commit(
                        discovery, reference: recovered.reference, target: recovered.target
                    )
                }
            )
        }
        recovery = (id: id, task: task, discoversTarget: true)
        return try await awaitRecovery(id: id, task: task)
    }

    private func awaitRecovery(
        id: UUID,
        task: Task<RuntimeConnection, Error>
    ) async throws -> RuntimeConnection {
        do {
            let recovered = try await task.value
            guard !stopped else { throw CancellationError() }
            if recovery?.id == id {
                startup = (id, Task { recovered })
                recovery = nil
            }
            return recovered
        } catch {
            task.cancel()
            if recovery?.id == id {
                recovery = nil
            }
            throw error
        }
    }

    /// Internal for shared-recovery tests. The recovery closure must enter the
    /// binding's shared target-aware transition; waiters must never Discover
    /// and retarget independently after the same Resume completes.
    static func recoverCallAfterJoining<RecoveredConnection: Sendable>(
        recovery: Task<RecoveredConnection, Error>,
        recover: (RecoveredConnection) async throws -> RecoveredConnection,
        restart: () async throws -> RecoveredConnection
    ) async throws -> RecoveredConnection {
        let joined: RecoveredConnection
        do {
            joined = try await recovery.value
        } catch {
            return try await restart()
        }
        return try await recover(joined)
    }

    /// Publish a same-Actor re-Attach before the new recovery can suspend on
    /// Discovery. The old owner cannot overwrite it because the slot ID changes
    /// in the same actor turn.
    static func publishJoinedRecovery<RecoveredConnection>(
        joined: RecoveredConnection,
        ownsRecoverySlot: Bool,
        publish: (RecoveredConnection) -> Void
    ) {
        if ownsRecoverySlot {
            publish(joined)
        }
    }

    /// Internal for the same-Actor re-Attach handoff tests. While the waiter
    /// owns the old recovery slot, its joined result is newer than startup.
    static func joinedRecoveryConnection<RecoveredConnection>(
        joined: RecoveredConnection,
        ownsRecoverySlot: Bool,
        connect: () async throws -> RecoveredConnection
    ) async throws -> RecoveredConnection {
        if ownsRecoverySlot {
            return joined
        }
        return try await connect()
    }

    /// Internal for recovery-ordering tests. A changed discovery target must
    /// attach the replacement without first resuming the unreachable old peer.
    /// The event-routing commit runs only after that recovery has succeeded.
    static func recoverDiscoveredConnection<RecoveredConnection>(
        currentTarget: ActrBindings.ActrId,
        discoveredTarget: ActrBindings.ActrId,
        retarget: () async throws -> RecoveredConnection,
        resume: () async throws -> RecoveredConnection,
        commit: (RecoveredConnection) async -> Void = { _ in }
    ) async throws -> RecoveredConnection {
        let recovered: RecoveredConnection
        if discoveredTarget != currentTarget {
            recovered = try await retarget()
        } else {
            recovered = try await resume()
        }
        await commit(recovered)
        return recovered
    }

    static func requiresReattach(_ error: Error) -> Bool {
        let message = String(describing: error)
        return ["SessionExpired", "SessionEpochMismatch", "ChannelExpired"]
            .contains(where: { message.contains($0) })
    }

    private func resumeOrReattach(_ connection: RuntimeConnection) async throws -> RuntimeConnection {
        do {
            try await connection.session.resume()
            try Task.checkCancellation()
            guard !stopped else { throw CancellationError() }
            return connection
        } catch {
            guard Self.requiresReattach(error) else { throw error }
            try? await connection.session.detach()
            let attached = try await attach(connection.reference)
            let replacement = RuntimeConnection(
                reference: connection.reference,
                session: attached.session,
                target: attached.target
            )
            do {
                try Task.checkCancellation()
                guard !stopped else { throw CancellationError() }
                return replacement
            } catch {
                try? await replacement.session.detach()
                throw error
            }
        }
    }

    private func retarget(
        _ connection: RuntimeConnection,
        to target: ActrBindings.ActrId
    ) async throws -> RuntimeConnection {
        try? await connection.session.detach()
        guard !stopped else { throw CancellationError() }
        let session = try await connection.reference.attachActrium(
            service: target,
            params: attachParams
        )
        let replacement = RuntimeConnection(
            reference: connection.reference,
            session: session,
            target: target
        )
        do {
            try Task.checkCancellation()
            guard !stopped else { throw CancellationError() }
            startup = (UUID(), Task { replacement })
            // A replacement supersedes a shared recovery that resumed the old
            // peer. Do not let that recovery's owner reinstall its stale result.
            recovery = nil
            return replacement
        } catch {
            try? await replacement.session.detach()
            throw error
        }
    }

    /// Internal for the shutdown-race tests: a stopped binding must reject
    /// before it creates or reuses a startup task.
    func connect() async throws -> RuntimeConnection {
        // A caller may have been suspended on another actor (for example while
        // registering an upload session) when `stop()` ran. Re-check on this
        // actor before starting or reusing a startup task so teardown can
        // never resurrect a Realm node.
        guard !stopped else { throw CancellationError() }
        if let startup { return try await startup.task.value }
        let id = UUID()
        let task = Task {
            let membership = try RealmIdentityStore().membership(
                realm: connection.realm, providerId: connection.providerId, issuer: connection.issuerURL
            )
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
            let configURL = directory.appendingPathComponent("actr.toml")
            try configuration.write(to: configURL, atomically: true, encoding: .utf8)
            let node = try await ActrBindings.ActrNode.newFromFrontWithMembershipAndEvents(
                configPath: configURL.path,
                frontArchive: front,
                serviceType: serviceType,
                membership: membership,
                events: Array(events).sorted(),
                listener: eventRelay
            )
            // Installing the session registry enables both inbound Attach and
            // outbound Attach over the verified Actor channel.
            let configured = try node.withActriumService(
                config: serviceConfiguration, policies: ActriumServicePolicies.allowAll()
            )
            let reference = try await configured.start()
            do {
                let attached = try await attach(reference)
                do {
                    try Task.checkCancellation()
                } catch {
                    try? await attached.session.detach()
                    throw error
                }
                return RuntimeConnection(
                    reference: reference, session: attached.session, target: attached.target
                )
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

    /// Attach may race the peer handshake that creates the verified channel.
    /// Retry until the Service is discoverable and Attach succeeds, bounded so
    /// a permanently unreachable Service still surfaces an error.
    private func attach(
        _ reference: ActrRefWrapper
    ) async throws -> (target: ActrBindings.ActrId, session: ActrPeerSessionWrapper) {
        let deadline = Date().addingTimeInterval(30)
        var lastError: Error?
        while Date() < deadline {
            try Task.checkCancellation()
            guard !stopped else { throw CancellationError() }
            do {
                let target = try await findService(using: reference)
                let session = try await reference.attachActrium(service: target, params: attachParams)
                return (target, session)
            } catch {
                lastError = error
            }
            try await Task.sleep(nanoseconds: 500_000_000)
        }
        if let lastError { throw lastError }
        throw RealmServiceError(message: "The Realm's selected Service is not online.")
    }

    /// The Front node cannot host a useful Service, but the P2P session protocol
    /// is installed through the same Service config. It must be internally
    /// consistent, so reuse the verified Front declarations.
    private var serviceConfiguration: ActriumServiceConfig {
        ActriumServiceConfig(
            serviceActorId: "",
            allowedFrontActorTypes: [frontType],
            program: metadata.program,
            packageDigest: metadata.packageDigest,
            guestDigest: metadata.guestDigest,
            protocolFingerprint: metadata.protocolFingerprint,
            supportedRuntimes: [metadata.runtime],
            programCapabilities: metadata.programCapabilities,
            routes: metadata.routes,
            attachmentPolicy: .exclusive,
            leaseDurationSecs: 300,
            heartbeatIntervalSecs: 30,
            gracePeriodSecs: 60
        )
    }

    private var attachParams: ActriumAttachParams {
        ActriumAttachParams(
            program: metadata.program,
            packageDigest: metadata.packageDigest,
            guestDigest: metadata.guestDigest,
            protocolFingerprint: metadata.protocolFingerprint,
            runtime: metadata.runtime,
            requestedCapabilities: metadata.programCapabilities
        )
    }

    func stop() async {
        stopped = true
        await clearEventPeer()
        let startup = self.startup
        let recovery = self.recovery
        self.recovery = nil
        startup?.task.cancel()
        recovery?.task.cancel()

        var base: RuntimeConnection?
        if let startup {
            base = try? await startup.task.value
        }
        var recovered: RuntimeConnection?
        if let recovery {
            recovered = try? await recovery.task.value
        }
        // A completed recovery owns the newest session; its reference is the
        // same runtime as the original startup connection.
        let connection = recovered ?? base ?? localFrontHandoff

        // Unregister every attachment stream before the runtime goes down so
        // a restarting Service never sees ghost streams. Local bookkeeping is
        // dropped even when startup never produced a usable runtime.
        let removed = await sessions.cancelAll()
        for session in removed {
            attachmentDeliveries.removeValue(forKey: session.attemptId)
            if let reference = connection?.reference {
                try? await reference.unregisterStream(streamId: session.resultStreamId)
                try? await reference.unregisterStream(streamId: session.chunkStreamId)
            }
            await sessions.completeTeardown(session)
        }
        if let connection {
            try? await connection.session.detach()
            connection.reference.shutdown()
            await connection.reference.waitForShutdown()
        }
        self.startup = nil
        self.localFrontHandoff = nil
        try? FileManager.default.removeItem(at: directory)
    }
}
