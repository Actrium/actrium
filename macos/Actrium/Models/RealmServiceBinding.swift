import Actr
import ActrBindings
import ActriumCore
import ActriumIPC
import Foundation

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
    func subscribe(events: [String]) async throws
    func suspendEvents() async
    func stop() async
}

@MainActor
final class RealmServiceBindings {
    static let shared = RealmServiceBindings()
    private var bindings: [UUID: any RealmServiceBindingHandle] = [:]
    private var bindingTabs: [UUID: UUID] = [:]
    private var subscriptions: [UUID: (bindingId: UUID, events: Set<String>)] = [:]
    private var pendingSubscriptions: [UUID: UUID] = [:]
    private var cancelledSubscriptions: Set<UUID> = []
    var eventHandler: ((UUID, RealmServiceEvent) -> Void)?

    func attach(_ binding: any RealmServiceBindingHandle, to tabId: UUID) {
        remove(tabId)
        bindings[tabId] = binding
        bindingTabs[binding.id] = tabId
    }

    func remove(_ tabId: UUID) {
        guard let binding = bindings.removeValue(forKey: tabId) else { return }
        bindingTabs.removeValue(forKey: binding.id)
        subscriptions = subscriptions.filter { $0.value.bindingId != binding.id }
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
        // The actor may publish as soon as subscribe selects its event peer,
        // before the subscribe reply crosses the Renderer/Host IPC boundary.
        // Register first so publish can already address this subscription.
        subscriptions[request.id] = (binding.id, Set(request.events))
        defer {
            pendingSubscriptions.removeValue(forKey: request.id)
            cancelledSubscriptions.remove(request.id)
        }
        func rollback() async {
            subscriptions.removeValue(forKey: request.id)
            if !subscriptions.values.contains(where: { $0.bindingId == binding.id }) {
                await binding.suspendEvents()
            }
        }
        do {
            try await binding.subscribe(events: request.events)
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
        guard let removed = subscriptions.removeValue(forKey: subscriptionId),
              let tab = bindingTabs[removed.bindingId], tab == tabId,
              let binding = bindings[tabId] else { return }
        if !subscriptions.values.contains(where: { $0.bindingId == removed.bindingId }) {
            Task { await binding.suspendEvents() }
        }
    }

    nonisolated func publish(bindingId: UUID, event: String, payload: Data) {
        Task { @MainActor in
            guard let tabId = bindingTabs[bindingId] else { return }
            for (subscriptionId, subscription) in subscriptions
            where subscription.bindingId == bindingId && subscription.events.contains(event) {
                eventHandler?(tabId, RealmServiceEvent(
                    subscriptionId: subscriptionId, event: event, payload: payload
                ))
            }
        }
    }
}

private final class RealmServiceEventRelay: ActrBindings.ServiceEventListener, @unchecked Sendable {
    private let bindingId: UUID

    init(bindingId: UUID) {
        self.bindingId = bindingId
    }

    func onEvent(event: ActrBindings.ServiceEventBridge) async {
        RealmServiceBindings.shared.publish(
            bindingId: bindingId, event: event.event, payload: event.payload
        )
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
    private let profile: RealmProfileInfo
    private let front: Data
    private let configuration: String
    private let serviceType: ActrBindings.ActrType
    private let routes: Set<String>
    private let events: Set<String>
    private let eventRelay: RealmServiceEventRelay
    private var eventReference: ActrRefWrapper?
    private var startup: (id: UUID, task: Task<ActrRefWrapper, Error>)?
    private var stopped = false

    init(
        profile: RealmProfileInfo,
        verifiedProfileBody: Data,
        front: Data,
        serviceType: ActrBindings.ActrType,
        trustedFrontKey: Data,
        capabilitiesJSON: String
    ) throws {
        let document = try JSONDecoder().decode(ProfileDocument.self, from: verifiedProfileBody)
        let provider = document.provider
        guard let signaling = provider.signaling.sorted(by: { $0.priority < $1.priority }).first,
              URL(string: signaling.url)?.scheme?.lowercased() == "wss" else {
            throw RealmServiceError(message: "The verified Realm profile has invalid signaling information.")
        }
        let capabilities = try JSONDecoder().decode(Capabilities.self, from: Data(capabilitiesJSON.utf8))
        let services = ["service", "host:service"].compactMap { capabilities.hostModules[$0] }
        self.routes = Set(services.flatMap { $0.rpc ?? [] })
        self.events = Set(services.flatMap { $0.events ?? [] })
        let bindingId = UUID()
        self.id = bindingId
        self.eventRelay = RealmServiceEventRelay(bindingId: bindingId)
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

    func subscribe(events requestedEvents: [String]) async throws {
        let requested = Set(requestedEvents)
        guard !stopped, !requested.isEmpty, requested.isSubset(of: events) else {
            throw RealmServiceError(message: "The Service event subscription is outside this tab's declared capabilities.")
        }
        let reference = try await connect()
        guard !stopped else { throw CancellationError() }
        guard let target = try await reference.discover(targetType: serviceType, count: 1).first else {
            throw RealmServiceError(message: "The Realm's selected Service is not online.")
        }
        guard !stopped else { throw CancellationError() }
        try reference.setServiceEventPeer(peer: target)
        eventReference = reference
    }

    func suspendEvents() {
        try? eventReference?.setServiceEventPeer(peer: nil)
        eventReference = nil
    }

    private func connect() async throws -> ActrRefWrapper {
        if let startup { return try await startup.task.value }
        let id = UUID()
        let task = Task {
            let membership = try RealmIdentityStore().membership(
                realm: profile.realmName, providerId: profile.providerId, issuer: profile.issuerUrl
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
        suspendEvents()
        if let startup {
            startup.task.cancel()
            if let reference = try? await startup.task.value {
                reference.shutdown()
                await reference.waitForShutdown()
            }
            self.startup = nil
        }
        try? FileManager.default.removeItem(at: directory)
    }
}
