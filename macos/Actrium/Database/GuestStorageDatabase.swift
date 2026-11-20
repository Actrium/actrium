import ActriumCore
import Foundation
import Security

enum GuestStorageDatabaseError: Error { case busy, expiredBeforeStart }

/// Serializes blocking Core calls and shares one profile database with Publisher acceptance.
@MainActor
final class GuestStorageDatabase {
    static let shared = GuestStorageDatabase()
    private let connection: GuestStorageConnection
    private let queue = DispatchQueue(label: "com.actrium.guest-storage", qos: .userInitiated)
    private let pendingLimit: Int
    private var publisherTrustBusy = false
    private var publisherTrustWaiters: [CheckedContinuation<Void, Never>] = []
    private(set) var pendingOperationCount = 0
    var pendingPublisherTrustCount: Int { publisherTrustWaiters.count + (publisherTrustBusy ? 1 : 0) }

    init(
        database: GuestLocalStorage? = nil, pendingLimit: Int = 128,
        databaseURL: URL? = nil, keychain: any GuestStorageKeychain = SystemGuestStorageKeychain()
    ) {
        connection = GuestStorageConnection(database: database, databaseURL: databaseURL, keychain: keychain)
        self.pendingLimit = pendingLimit
    }

    func run<T: Sendable>(deadline: UInt64? = nil, _ operation: @escaping @Sendable (GuestLocalStorage) throws -> T) async throws -> T {
        if let deadline, DispatchTime.now().uptimeNanoseconds >= deadline { throw GuestStorageDatabaseError.expiredBeforeStart }
        guard pendingOperationCount < pendingLimit else { throw GuestStorageDatabaseError.busy }
        pendingOperationCount += 1
        defer { pendingOperationCount -= 1 }
        let connection = connection
        return try await withCheckedThrowingContinuation { continuation in
            queue.async {
                continuation.resume(with: Result {
                    if let deadline, DispatchTime.now().uptimeNanoseconds >= deadline { throw GuestStorageDatabaseError.expiredBeforeStart }
                    let store = try connection.open()
                    if let deadline, DispatchTime.now().uptimeNanoseconds >= deadline { throw GuestStorageDatabaseError.expiredBeforeStart }
                    return try operation(store)
                })
            }
        }
    }

    func acceptPublisherProfile(body: String, domain: String, previousPins: String? = nil, previousProfile: String? = nil) async throws -> PublisherProfileAcceptance {
        try await withPublisherTrust {
            try await run {
                try $0.acceptPublisherProfile(body: body, domain: domain, previousPinsJson: previousPins, previousProfileBody: previousProfile)
            }
        }
    }

    /// Serialize Publisher updates with the final validation-and-activation operation.
    func withPublisherTrust<T>(_ operation: () async throws -> T) async throws -> T {
        if publisherTrustBusy {
            guard publisherTrustWaiters.count < pendingLimit else { throw GuestStorageDatabaseError.busy }
            await withCheckedContinuation { publisherTrustWaiters.append($0) }
        } else {
            publisherTrustBusy = true
        }
        defer {
            if publisherTrustWaiters.isEmpty { publisherTrustBusy = false }
            else { publisherTrustWaiters.removeFirst().resume() }
        }
        try Task.checkCancellation()
        return try await operation()
    }

    func acceptedPublisherPins(domain: String) async throws -> String? {
        try await run { try $0.acceptedPublisherPins(domain: domain) }
    }

    func acceptedPublishers(domains: [String]) async throws -> [PublisherProfileInfo] {
        try await run { try $0.acceptedPublishers(domains: domains) }
    }

    func resolveSignedOrigin(_ origin: String, publisherIdentity: String? = nil, migrationApproval: String? = nil) async throws -> String {
        try await run { try $0.resolveSignedOrigin(origin: origin, publisherIdentity: publisherIdentity, migrationApproval: migrationApproval) }
    }

    func migrationToken(_ origin: String, publisherIdentity: String) async throws -> String? {
        try await run { try $0.storageMigrationToken(origin: origin, publisherIdentity: publisherIdentity) }
    }

    func keepSeparate(_ origin: String, publisherIdentity: String) async throws -> String {
        try await run { try $0.keepStorageSeparate(origin: origin, publisherIdentity: publisherIdentity) }
    }
}

/// Accessed only on the owning GuestStorageDatabase's serial queue.
private nonisolated final class GuestStorageConnection: @unchecked Sendable {
    private var database: GuestLocalStorage?
    private let databaseURL: URL?
    private let keychain: any GuestStorageKeychain

    init(database: GuestLocalStorage?, databaseURL: URL?, keychain: any GuestStorageKeychain) {
        self.database = database
        self.databaseURL = databaseURL
        self.keychain = keychain
    }

    func open() throws -> GuestLocalStorage {
        if let database { return database }
        let url = try databaseURL ?? defaultDatabaseURL()
        var key = try loadKey(for: url)
        defer { key.resetBytes(in: 0..<key.count) }
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        let store = try GuestLocalStorage.newEncrypted(path: url.path, key: key)
        database = store
        return store
    }

    private func defaultDatabaseURL() throws -> URL {
        let support = try FileManager.default.url(
            for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true
        )
        return DatabaseManager.appDataDirectory(in: support).appendingPathComponent("guest-local-storage.sqlite")
    }

    private func loadKey(for url: URL) throws -> Data {
        if let key = try keychain.read() { return key }
        let databaseExists: Bool
        do {
            _ = try FileManager.default.attributesOfItem(atPath: url.path)
            databaseExists = true
        } catch let error as CocoaError where [.fileNoSuchFile, .fileReadNoSuchFile].contains(error.code) {
            databaseExists = false
        }
        if !databaseExists {
            var key = Data(count: 32)
            let status = key.withUnsafeMutableBytes { buffer in
                SecRandomCopyBytes(kSecRandomDefault, buffer.count, buffer.baseAddress!)
            }
            guard status == errSecSuccess else { throw GuestStorageKeyError.generationFailed(status) }
            if try keychain.add(key) { return key }
        }
        // Another launch may have persisted the key after the initial lookup.
        guard let existing = try keychain.read() else { throw GuestStorageKeyError.missing }
        return existing
    }
}
