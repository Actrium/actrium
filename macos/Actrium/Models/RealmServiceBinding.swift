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

    /// Keep established Guest subscriptions when only the Realm connection changes.
    func replaceConnection(
        _ active: RealmServiceBinding,
        with replacement: RealmServiceBinding,
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

    func select(
        generation: UInt64,
        reference: any RealmServiceEventReference,
        serviceType: ActrBindings.ActrType
    ) async throws {
        self.generation = max(self.generation, generation)
        guard self.generation == generation else { throw CancellationError() }
        // The v1 domain model requires exactly one Service of the signed type;
        // taking the first of multiple route candidates is not authentication.
        let target = try await reference.discoverService(serviceType: serviceType)
        guard self.generation == generation else { throw CancellationError() }
        try reference.setServiceEventPeer(peer: target)
        self.reference = reference
        peer = target
    }

    /// A generation snapshot for `call` only when a subscription is active.
    /// A call that started before the first subscription must not select a peer.
    func activeGeneration() -> UInt64? {
        reference == nil ? nil : generation
    }

    /// Strictly rediscover the Service for an RPC and, when this generation's
    /// subscription is still active, move its event peer to the same ActorId.
    /// Discovery commits are sequenced within the generation: a slow response
    /// for a retired Service cannot undo a newer RPC's committed selection.
    func resolve(
        ifGeneration generation: UInt64?,
        reference: any RealmServiceEventReference,
        serviceType: ActrBindings.ActrType
    ) async throws -> ActrBindings.ActrId {
        discoverySequence &+= 1
        let sequence = discoverySequence
        let target = try await reference.discoverService(serviceType: serviceType)
        if let generation {
            refresh(
                ifGeneration: generation, reference: reference, target: target,
                sequence: sequence
            )
        }
        return target
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
    private let trustedFrontKey: Data
    private let configuration: String
    private let serviceType: ActrBindings.ActrType
    /// Business routes; internal for the attachment transport wiring tests.
    let routes: Set<String>
    private let events: Set<String>
    private let eventRelay: RealmServiceEventRelay
    private let eventPeers = RealmServiceEventPeerController()
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
        let connection = try Connection(profile: profile, verifiedProfileBody: verifiedProfileBody)
        let capabilities = try JSONDecoder().decode(Capabilities.self, from: Data(capabilitiesJSON.utf8))
        let services = ["service", "host:service"].compactMap { capabilities.hostModules[$0] }
        let declaredRoutes = Set(services.flatMap { $0.rpc ?? [] })
        try self.init(
            connection: connection,
            front: front,
            serviceType: serviceType,
            trustedFrontKey: trustedFrontKey,
            routes: RealmServiceAttachmentUploadPolicy.businessRoutes(declaredRoutes),
            events: Set(services.flatMap { $0.events ?? [] }),
            attachmentUploadAllowed: RealmServiceAttachmentUploadPolicy.isPermitted(
                permissions: permissions, declaredRoutes: declaredRoutes
            )
        )
    }

    private init(
        connection: Connection,
        front: Data,
        serviceType: ActrBindings.ActrType,
        trustedFrontKey: Data,
        routes: Set<String>,
        events: Set<String>,
        attachmentUploadAllowed: Bool
    ) throws {
        self.connection = connection
        let bindingId = UUID()
        self.id = bindingId
        self.eventRelay = RealmServiceEventRelay(bindingId: bindingId)
        self.front = front
        self.serviceType = serviceType
        self.trustedFrontKey = trustedFrontKey
        self.routes = routes
        self.events = events
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
            connection: connection, front: front, serviceType: serviceType, trustedFrontKey: trustedFrontKey,
            routes: routes, events: events, attachmentUploadAllowed: attachmentUploadAllowed
        )
    }

    /// Prepare a replacement binding without sending a business RPC.
    func prepareForSwitch() async throws {
        try Task.checkCancellation()
        let reference = try await connect()
        _ = try await reference.discoverService(serviceType: serviceType)
        guard !stopped else { throw CancellationError() }
        try Task.checkCancellation()
    }

    func call(_ request: RealmServiceCall) async throws -> Data {
        guard !stopped, routes.contains(request.route),
              (1...300_000).contains(request.timeoutMs), request.payload.count <= 1_048_576 else {
            throw RealmServiceError(message: "The Service call is outside this tab's declared capabilities.")
        }
        let eventGeneration = await eventPeers.activeGeneration()
        let reference = try await connect()
        guard !stopped else { throw CancellationError() }
        // A Realm tab targets the unique Service of the selected type. A
        // restarted Service may register with a different ActorId while the
        // Front stays running; ambiguous discovery fails instead of selecting
        // an arbitrary event peer.
        let target = try await eventPeers.resolve(
            ifGeneration: eventGeneration, reference: reference, serviceType: serviceType
        )
        guard !stopped else { throw CancellationError() }
        return try await reference.callPeer(
            target: target,
            routeKey: request.route,
            requestPayload: request.payload,
            timeoutMs: Int64(request.timeoutMs)
        )
    }

    func subscribe(events requestedEvents: [String], generation: UInt64) async throws {
        let requested = Set(requestedEvents)
        guard !stopped, !requested.isEmpty, requested.isSubset(of: events) else {
            throw RealmServiceError(message: "The Service event subscription is outside this tab's declared capabilities.")
        }
        let reference = try await connect()
        guard !stopped else { throw CancellationError() }
        try await eventPeers.select(
            generation: generation, reference: reference, serviceType: serviceType
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
                let target = try await reference.discoverService(serviceType: serviceType)
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
        await clearEventPeer()
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
