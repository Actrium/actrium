import ActriumCore
import ActriumIPC
import AppKit
import Foundation

/// Binds storage to the Host's current document, before sending the renderer config.
@MainActor
final class RendererLocalStorage {
    private struct Manifest: Decodable {
        struct Module: Decodable { let rpc: [String]? }
        let hostModules: [String: Module]
    }

    private struct Document {
        let id = UUID()
        let origin: String
        let principal: String
        let publisherIdentity: String?
        let reloadKey: Int
        let methods: Set<GuestLocalStorageCall.Operation>
    }

    private var documents: [UUID: Document] = [:]
    private let databaseAccess: GuestStorageDatabase
    private let migrationConsent: StorageMigrationConsent
    var pendingOperationCount: Int { databaseAccess.pendingOperationCount }

    init(database: GuestLocalStorage? = nil, pendingLimit: Int = 128, migrationConsent: StorageMigrationConsent? = nil) {
        databaseAccess = GuestStorageDatabase(database: database, pendingLimit: pendingLimit)
        self.migrationConsent = migrationConsent ?? StorageMigrationConsent()
    }

    init(access: GuestStorageDatabase, migrationConsent: StorageMigrationConsent? = nil) {
        databaseAccess = access
        self.migrationConsent = migrationConsent ?? StorageMigrationConsent()
    }

    func resolveOrigin(config: [String: Any], window: NSWindow? = nil) async throws -> [String: Any] {
        var config = config
        let prepared = (config.removeValue(forKey: "preparedLocalStorage") as? [String: String])
            .flatMap(PreparedLocalStorage.init(configuration:))
        if let prepared, prepared.matches(principal: config["localStorageOrigin"] as? String,
                                         publisherIdentity: config["localStoragePublisherIdentity"] as? String) {
            config["localStoragePrincipal"] = prepared.principal
            config["localStorageOrigin"] = prepared.origin
            return config
        }
        guard let publisherIdentity = config["localStoragePublisherIdentity"] as? String else { return config }
        guard let origin = config["localStorageOrigin"] as? String,
              let manifest = config["capabilityManifestJSON"] as? String,
              let permissions = config["permissions"] as? [String],
              let methods = Self.allowedMethods(manifest: manifest, permissions: permissions),
              !methods.isEmpty else { return config }
        var resolved = config
        resolved["localStoragePrincipal"] = origin
        resolved["localStorageOrigin"] = try await migrationConsent.resolve(
            origin: origin, publisherIdentity: publisherIdentity,
            database: databaseAccess, window: window
        )
        return resolved
    }

    func prepareForActivation(config: [String: Any], window: NSWindow?) async throws -> PreparedLocalStorage? {
        let resolved = try await resolveOrigin(config: config, window: window)
        guard let principal = resolved["localStoragePrincipal"] as? String,
              let identity = resolved["localStoragePublisherIdentity"] as? String,
              let origin = resolved["localStorageOrigin"] as? String else { return nil }
        return PreparedLocalStorage(principal: principal, publisherIdentity: identity, origin: origin)
    }

    func prepare(tabId: UUID, config: [String: Any]) -> [String: Any] {
        var config = config
        config.removeValue(forKey: "preparedLocalStorage")
        let origin = config.removeValue(forKey: "localStorageOrigin") as? String
        let principal = config.removeValue(forKey: "localStoragePrincipal") as? String ?? origin
        let publisherIdentity = config.removeValue(forKey: "localStoragePublisherIdentity") as? String
        guard let origin, let manifest = config["capabilityManifestJSON"] as? String,
              let permissions = config["permissions"] as? [String],
              let methods = Self.allowedMethods(manifest: manifest, permissions: permissions),
              !methods.isEmpty else {
            remove(tabId)
            return config
        }

        let reloadKey = config["reloadKey"] as? Int ?? 0
        let current = documents[tabId]
        if current?.origin != origin || current?.principal != principal || current?.publisherIdentity != publisherIdentity || current?.reloadKey != reloadKey || current?.methods != methods {
            documents[tabId] = Document(origin: origin, principal: principal ?? origin, publisherIdentity: publisherIdentity, reloadKey: reloadKey, methods: methods)
        }
        config["localStorageDocumentId"] = documents[tabId]?.id.uuidString
        return config
    }

    func remove(_ tabId: UUID) {
        documents.removeValue(forKey: tabId)
    }

    func reply(to json: String, tabId: UUID) async throws -> String? {
        guard json.utf8.count <= GuestLocalStorageCall.maxRequestBytes else { return nil }
        let call = try JSONDecoder().decode(GuestLocalStorageCall.self, from: Data(json.utf8))
        if call.operation != .replaceItems && json.utf8.count > GuestLocalStorageCall.maxSingleRequestBytes {
            return nil
        }
        let reply = await perform(call, tabId: tabId)
        return String(decoding: try JSONEncoder().encode(reply), as: UTF8.self)
    }

    private func perform(_ call: GuestLocalStorageCall, tabId: UUID) async -> GuestLocalStorageReply {
        guard call.hasValidArguments else {
            return GuestLocalStorageReply(id: call.id, error: "Invalid local storage arguments.")
        }
        guard let document = documents[tabId], document.id == call.documentId,
              document.methods.contains(call.operation) else {
            return GuestLocalStorageReply(id: call.id, error: "Local storage access was denied.")
        }
        let now = DispatchTime.now()
        let deadline = min(call.deadlineUptimeNanoseconds, (now + .seconds(GuestLocalStorageCall.timeoutSeconds)).uptimeNanoseconds)
        guard now.uptimeNanoseconds < deadline else { return .expiredBeforeStart(id: call.id) }
        do {
            // Accepted database operations may finish after revocation.
            // Reject subsequent calls and prevent replies from reaching a replaced document.
            let value = try await execute(call, origin: document.origin, principal: document.principal, publisherIdentity: document.publisherIdentity, deadline: deadline)
            guard documents[tabId]?.id == document.id else {
                return GuestLocalStorageReply(id: call.id, error: "The local storage document changed.", errorCode: call.operation == .getItem ? nil : .outcomeUnknown)
            }
            return GuestLocalStorageReply(id: call.id, value: value)
        } catch GuestStorageDatabaseError.expiredBeforeStart {
            return .expiredBeforeStart(id: call.id)
        } catch GuestStorageDatabaseError.busy {
            return GuestLocalStorageReply(id: call.id, error: "Too many pending local storage operations.")
        } catch GuestLocalStorageError.InvalidPublisher, GuestLocalStorageError.IdentityChanged {
            return GuestLocalStorageReply(id: call.id, error: "Local storage access was denied.")
        } catch GuestLocalStorageError.QuotaExceeded(let message) {
            return GuestLocalStorageReply(id: call.id, error: message, errorCode: .quotaExceeded)
        } catch GuestLocalStorageError.Conflict(let message) {
            return GuestLocalStorageReply(id: call.id, error: message, errorCode: .conflict)
        } catch GuestLocalStorageError.InvalidKey(let message),
                GuestLocalStorageError.InvalidValue(let message) {
            return GuestLocalStorageReply(id: call.id, error: message)
        } catch {
            AppLogger.database.error("Guest local storage database failure (code \((error as NSError).code)).")
            return GuestLocalStorageReply(id: call.id, error: "The local storage database operation failed.")
        }
    }

    private func execute(_ call: GuestLocalStorageCall, origin: String, principal: String, publisherIdentity: String?, deadline: UInt64) async throws -> String? {
        try await databaseAccess.run(deadline: deadline) { store in
            let current = try store.resolveSignedOrigin(origin: principal, publisherIdentity: publisherIdentity, migrationApproval: nil)
            guard current == origin else {
                throw GuestLocalStorageError.InvalidPublisher(message: "The storage identity changed.")
            }
            guard DispatchTime.now().uptimeNanoseconds < deadline else { throw GuestStorageDatabaseError.expiredBeforeStart }
            return try Self.executeSynchronously(call, origin: origin, store: store)
        }
    }

    private nonisolated static func executeSynchronously(
        _ call: GuestLocalStorageCall, origin: String, store: GuestLocalStorage
    ) throws -> String? {
        switch call.operation {
        case .getItem:
            return try store.getItem(origin: origin, key: call.key)
        case .setItem:
            guard let value = call.value else {
                throw GuestLocalStorageError.InvalidValue(message: "Invalid local storage arguments.")
            }
            try store.setItem(origin: origin, key: call.key, value: value)
        case .removeItem:
            try store.removeItem(origin: origin, key: call.key)
        case .replaceItems:
            guard let replacement = call.replacement else {
                throw GuestLocalStorageError.InvalidValue(message: "Invalid local storage replacement.")
            }
            try store.replaceItems(
                origin: origin, key: call.key, expected: replacement.expected,
                remove: replacement.remove, set: replacement.set.map { GuestLocalStorageItem(key: $0.key, value: $0.value) }
            )
        }
        return nil
    }

    private static func allowedMethods(
        manifest: String, permissions: [String]
    ) -> Set<GuestLocalStorageCall.Operation>? {
        let module = "host:local-storage"
        let decoded = try? JSONDecoder().decode(Manifest.self, from: Data(manifest.utf8))
        guard let methods = decoded?.hostModules["local-storage"]?.rpc else { return nil }
        return Set(methods.compactMap { method in
            guard permissions.contains(module) || permissions.contains("\(module).\(method)") else { return nil }
            return GuestLocalStorageCall.Operation(rawValue: method)
        })
    }
}
