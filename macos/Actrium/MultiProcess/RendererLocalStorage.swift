import ActriumCore
import ActriumIPC
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
        let reloadKey: Int
        let methods: Set<GuestLocalStorageCall.Operation>
    }

    private var documents: [UUID: Document] = [:]
    private var database: GuestLocalStorage?

    init(database: GuestLocalStorage? = nil) {
        self.database = database
    }

    func prepare(tabId: UUID, config: [String: Any]) -> [String: Any] {
        var config = config
        let origin = config.removeValue(forKey: "localStorageOrigin") as? String
        guard let origin, let manifest = config["capabilityManifestJSON"] as? String,
              let permissions = config["permissions"] as? [String],
              let methods = Self.allowedMethods(manifest: manifest, permissions: permissions),
              !methods.isEmpty else {
            remove(tabId)
            return config
        }

        let reloadKey = config["reloadKey"] as? Int ?? 0
        let current = documents[tabId]
        if current?.origin != origin || current?.reloadKey != reloadKey || current?.methods != methods {
            documents[tabId] = Document(origin: origin, reloadKey: reloadKey, methods: methods)
        }
        config["localStorageDocumentId"] = documents[tabId]?.id.uuidString
        return config
    }

    func remove(_ tabId: UUID) {
        documents.removeValue(forKey: tabId)
    }

    func reply(to json: String, tabId: UUID) async throws -> String? {
        // A valid key and value fit even if every byte needs JSON escaping.
        guard json.utf8.count <= 512 * 1024 else { return nil }
        let call = try JSONDecoder().decode(GuestLocalStorageCall.self, from: Data(json.utf8))
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
        do {
            let value = try await execute(call, origin: document.origin)
            guard documents[tabId]?.id == document.id else {
                return GuestLocalStorageReply(id: call.id, error: "The local storage document changed.")
            }
            return GuestLocalStorageReply(id: call.id, value: value)
        } catch GuestLocalStorageError.InvalidKey(let message),
                GuestLocalStorageError.InvalidValue(let message),
                GuestLocalStorageError.QuotaExceeded(let message) {
            return GuestLocalStorageReply(id: call.id, error: message)
        } catch {
            AppLogger.database.error("Guest local storage database failure (code \((error as NSError).code)).")
            return GuestLocalStorageReply(id: call.id, error: "The local storage database operation failed.")
        }
    }

    private func execute(_ call: GuestLocalStorageCall, origin: String) async throws -> String? {
        let store = try open()
        return try await Task.detached { () throws -> String? in
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
            }
            return nil
        }.value
    }

    private func open() throws -> GuestLocalStorage {
        if let database { return database }
        let support = try FileManager.default.url(
            for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true
        )
        let folder = DatabaseManager.appDataDirectory(in: support)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let store = GuestLocalStorage(path: folder.appendingPathComponent("guest-local-storage.sqlite").path)
        database = store
        return store
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
