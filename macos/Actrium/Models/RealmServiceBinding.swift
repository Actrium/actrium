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
    func call(_ request: RealmServiceCall) async throws -> Data
    func subscribe(events: [String], generation: UInt64) async throws
    func suspendEvents(ifGeneration generation: UInt64) async
    func stop() async
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

    // Keep in step with the packaged Service startup in actr run.
    private static let actriumAttachEnabled = false

    struct RuntimeConnection: Sendable {
        let reference: ActrRefWrapper
        let session: ActrPeerSessionWrapper?
        let target: ActrBindings.ActrId

        func call(route: String, capability: String, payload: Data, timeoutMs: UInt64) async throws -> Data {
            if let session {
                return try await session.call(
                    route: route, capability: capability, payload: payload, timeoutMs: timeoutMs
                )
            }
            return try await reference.callPeer(
                target: target, routeKey: route, requestPayload: payload, timeoutMs: Int64(timeoutMs)
            )
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
    private let metadata: ActriumSessionMetadata
    private let frontType: String
    private let trustedFrontKey: Data
    private let configuration: String
    private let serviceType: ActrBindings.ActrType
    /// Business routes declared by the verified Front manifest.
    let routes: Set<String>
    private let routeCapabilities: [String: String]
    private let events: Set<String>
    private let eventRelay: RealmServiceEventRelay
    private let eventPeers = RealmServiceEventPeerController()
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
        capabilitiesJSON: String
    ) throws {
        let connection = try Connection(profile: profile, verifiedProfileBody: verifiedProfileBody)
        let capabilities = try JSONDecoder().decode(Capabilities.self, from: Data(capabilitiesJSON.utf8))
        let services = ["service", "host:service"].compactMap { capabilities.hostModules[$0] }
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
            routes: declaredRoutes,
            events: Set(services.flatMap { $0.events ?? [] })
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
        events: Set<String>
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
            serviceType: serviceType, trustedFrontKey: trustedFrontKey, routes: routes, events: events
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
              (1...300_000).contains(request.timeoutMs), request.payload.count <= 1_048_576 else {
            throw RealmServiceError(message: "The Service call is outside this tab's declared capabilities.")
        }
        let eventGeneration = await eventPeers.activeGeneration()
        let connection = try await readyConnection(
            resolveTargetForCall: true,
            eventGeneration: eventGeneration
        )
        guard !stopped else { throw CancellationError() }
        return try await connection.call(
            route: request.route,
            capability: routeCapabilities[request.route] ?? "service",
            payload: request.payload,
            timeoutMs: UInt64(request.timeoutMs)
        )
    }

    func prepareLocalFront() async throws {
        // Resolve the selected Service before installing the renderer.
        _ = try await connect()
    }

    /// Release the old lease only after its replacement has been verified.
    /// The node and Actor identity remain alive so a failed replacement can
    /// restore an authenticated session without restarting the old renderer.
    func suspendForLocalFrontReplacement() async throws {
        guard localFrontHandoff == nil else { return }
        let connection = try await readyConnection()
        localFrontHandoff = connection
        try await connection.session?.detach()
    }

    /// Re-Attach the old Front after a failed replacement. Calls remain blocked
    /// while this is in flight, and a later retry can use the retained node.
    func restoreAfterFailedLocalFrontReplacement() async throws {
        guard !stopped, let handoff = localFrontHandoff else { return }
        guard let session = handoff.session else {
            localFrontHandoff = nil
            return
        }
        try? await session.detach()
        let replacement = try await openConnection(using: handoff.reference)
        do {
            try Task.checkCancellation()
            guard !stopped else { throw CancellationError() }
            startup = (UUID(), Task { replacement })
            localFrontHandoff = nil
        } catch {
            try? await replacement.session?.detach()
            throw error
        }
    }

    private func findService(using reference: ActrRefWrapper) async throws -> ActrBindings.ActrId {
        // Every Front, downloaded or local, must discover the unique Service
        // selected by the verified Realm Profile rather than an arbitrary peer.
        try await reference.discoverService(serviceType: serviceType)
    }

    func subscribe(events requestedEvents: [String], generation: UInt64) async throws {
        let requested = Set(requestedEvents)
        guard !stopped, !requested.isEmpty, requested.isSubset(of: events) else {
            throw RealmServiceError(message: "The Service event subscription is outside this tab's declared capabilities.")
        }
        let connection = try await readyConnection(resolveTargetForCall: !Self.actriumAttachEnabled)
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
        guard let session = connection.session else { return connection }
        do {
            try await session.resume()
            try Task.checkCancellation()
            guard !stopped else { throw CancellationError() }
            return connection
        } catch {
            guard Self.requiresReattach(error) else { throw error }
            try? await session.detach()
            let replacement = try await openConnection(using: connection.reference)
            do {
                try Task.checkCancellation()
                guard !stopped else { throw CancellationError() }
                return replacement
            } catch {
                try? await replacement.session?.detach()
                throw error
            }
        }
    }

    private func retarget(
        _ connection: RuntimeConnection,
        to target: ActrBindings.ActrId
    ) async throws -> RuntimeConnection {
        try? await connection.session?.detach()
        guard !stopped else { throw CancellationError() }
        let replacement = try await openConnection(using: connection.reference, target: target)
        do {
            try Task.checkCancellation()
            guard !stopped else { throw CancellationError() }
            startup = (UUID(), Task { replacement })
            // A replacement supersedes a shared recovery that resumed the old
            // peer. Do not let that recovery's owner reinstall its stale result.
            recovery = nil
            return replacement
        } catch {
            try? await replacement.session?.detach()
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
            let configured = if Self.actriumAttachEnabled {
                try node.withActriumService(
                    config: serviceConfiguration, policies: ActriumServicePolicies.allowAll()
                )
            } else {
                node
            }
            let reference = try await configured.start()
            do {
                let connection = try await openConnection(using: reference)
                do {
                    try Task.checkCancellation()
                } catch {
                    try? await connection.session?.detach()
                    throw error
                }
                return connection
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

    private func openConnection(
        using reference: ActrRefWrapper, target: ActrBindings.ActrId
    ) async throws -> RuntimeConnection {
        let session: ActrPeerSessionWrapper?
        if Self.actriumAttachEnabled {
            session = try await reference.attachActrium(service: target, params: attachParams)
        } else {
            try await reference.preparePeerRpc(target: target, timeoutMs: 5_000)
            session = nil
        }
        return RuntimeConnection(reference: reference, session: session, target: target)
    }

    /// Allow the selected Service time to become discoverable after startup.
    private func openConnection(using reference: ActrRefWrapper) async throws -> RuntimeConnection {
        let deadline = Date().addingTimeInterval(30)
        var lastError: Error?
        while Date() < deadline {
            try Task.checkCancellation()
            guard !stopped else { throw CancellationError() }
            do {
                let target = try await findService(using: reference)
                return try await openConnection(using: reference, target: target)
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

        if let connection {
            try? await connection.session?.detach()
            connection.reference.shutdown()
            await connection.reference.waitForShutdown()
        }
        self.startup = nil
        self.localFrontHandoff = nil
        try? FileManager.default.removeItem(at: directory)
    }
}
