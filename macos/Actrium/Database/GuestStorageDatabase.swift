import ActriumCore
import Foundation

enum GuestStorageDatabaseError: Error { case busy, expiredBeforeStart }

/// Serializes blocking Core calls and shares one profile database with Publisher acceptance.
@MainActor
final class GuestStorageDatabase {
    static let shared = GuestStorageDatabase()
    private var database: GuestLocalStorage?
    private let queue = DispatchQueue(label: "com.actrium.guest-storage", qos: .userInitiated)
    private let pendingLimit: Int
    private var publisherTrustBusy = false
    private var publisherTrustWaiters: [CheckedContinuation<Void, Never>] = []
    private(set) var pendingOperationCount = 0
    var pendingPublisherTrustCount: Int { publisherTrustWaiters.count + (publisherTrustBusy ? 1 : 0) }

    init(database: GuestLocalStorage? = nil, pendingLimit: Int = 128) {
        self.database = database
        self.pendingLimit = pendingLimit
    }

    func run<T: Sendable>(deadline: UInt64? = nil, _ operation: @escaping @Sendable (GuestLocalStorage) throws -> T) async throws -> T {
        if let deadline, DispatchTime.now().uptimeNanoseconds >= deadline { throw GuestStorageDatabaseError.expiredBeforeStart }
        guard pendingOperationCount < pendingLimit else { throw GuestStorageDatabaseError.busy }
        pendingOperationCount += 1
        defer { pendingOperationCount -= 1 }
        let store = try open()
        return try await withCheckedThrowingContinuation { continuation in
            queue.async {
                continuation.resume(with: Result {
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
}
