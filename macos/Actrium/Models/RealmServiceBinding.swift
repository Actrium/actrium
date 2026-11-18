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

    /// The running Front node plus its verified Attach session. Both live for
    /// as long as the tab; the session heartbeat runs inside the peer runtime.
    private struct Connection: Sendable {
        let reference: ActrRefWrapper
        let session: ActrPeerSessionWrapper
    }

    private let directory = FileManager.default.temporaryDirectory
        .appendingPathComponent("actrium-front-" + UUID().uuidString, isDirectory: true)
    private let profile: RealmProfileInfo
    private let front: Data
    private let metadata: ActriumSessionMetadata
    private let trustedFrontKey: Data
    private let configuration: String
    private let serviceType: ActrBindings.ActrType
    private let routes: Set<String>
    private let routeCapabilities: [String: String]
    private var startup: (id: UUID, task: Task<Connection, Error>)?
    private var stopped = false

    init(
        profile: RealmProfileInfo,
        verifiedProfileBody: Data,
        front: Data,
        metadata: ActriumSessionMetadata,
        trustedFrontKey: Data
    ) throws {
        let document = try JSONDecoder().decode(ProfileDocument.self, from: verifiedProfileBody)
        let provider = document.provider
        guard let signaling = provider.signaling.sorted(by: { $0.priority < $1.priority }).first,
              URL(string: signaling.url)?.scheme?.lowercased() == "wss" else {
            throw RealmServiceError(message: "The verified Realm profile has invalid signaling information.")
        }
        self.profile = profile
        self.front = front
        self.metadata = metadata
        self.trustedFrontKey = trustedFrontKey
        self.serviceType = ActrBindings.ActrType(
            publisher: metadata.program.publisher,
            name: metadata.program.name,
            version: metadata.program.version
        )
        self.routes = Set(metadata.routes.map(\.route))
        self.routeCapabilities = metadata.routes.reduce(into: [String: String]()) { capabilities, route in
            capabilities[route.route] = route.capabilities.first
        }

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
        let connection = try await connect()
        guard !stopped else { throw CancellationError() }
        return try await connection.session.call(
            route: request.route,
            capability: routeCapabilities[request.route] ?? "service",
            payload: request.payload,
            timeoutMs: UInt64(request.timeoutMs)
        )
    }

    private func connect() async throws -> Connection {
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
                configPath: configURL.path,
                frontArchive: front,
                serviceType: serviceType,
                membership: membership,
                actorKeyNamespace: nil
            )
            // Installing the session registry enables both inbound Attach and
            // outbound Attach over the verified Actor channel.
            let configured = try node.withActriumService(
                config: serviceConfiguration, policies: ActriumServicePolicies.allowAll()
            )
            let reference = try await configured.start()
            do {
                let session = try await attach(reference)
                do {
                    try Task.checkCancellation()
                } catch {
                    try? await session.detach()
                    throw error
                }
                return Connection(reference: reference, session: session)
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
    private func attach(_ reference: ActrRefWrapper) async throws -> ActrPeerSessionWrapper {
        let deadline = Date().addingTimeInterval(30)
        var lastError: Error?
        while Date() < deadline {
            try Task.checkCancellation()
            guard !stopped else { throw CancellationError() }
            do {
                let targets = try await reference.discover(targetType: serviceType, count: 1)
                if let target = targets.first {
                    return try await reference.attachActrium(service: target, params: attachParams)
                }
                lastError = RealmServiceError(message: "The Realm's selected Service is not online.")
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
            allowedFrontActorTypes: [profile.packageType],
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
        if let startup {
            startup.task.cancel()
            if let connection = try? await startup.task.value {
                try? await connection.session.detach()
                connection.reference.shutdown()
                await connection.reference.waitForShutdown()
            }
            self.startup = nil
        }
        try? FileManager.default.removeItem(at: directory)
    }
}
