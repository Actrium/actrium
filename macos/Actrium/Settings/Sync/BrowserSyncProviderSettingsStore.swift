import Foundation
import os

// Versioned browser sync provider connection settings, the macOS sibling of
// the Windows `browser-sync-provider.json` store. The snapshot intentionally
// never contains a credential field: the local-folder provider has no
// secrets, and later cloud providers keep their tokens in provider-SDK/OS
// storage, so the file stays plain JSON.
//
// Device identity: unlike Windows (where this file owns the device id), the
// macOS device identity is owned by `BrowserSyncCredentialsProvider` under a
// durable UserDefaults key. This store persists a copy of that id for
// inspectability and cross-platform parity, but reads it back from the
// credentials provider so the reference-server transition and any file
// corruption never mint a second identity for the same machine.
struct BrowserSyncProviderSettingsSnapshot: Equatable, Codable {
    var version: Int
    var providerKind: String?
    var localFolderRootPath: String?
    var scope: String
    var deviceId: String
    var membershipJoinedAtUnixSeconds: Int64?
    var savedAtUnixSeconds: Int64

    var isConfigured: Bool {
        guard let providerKind, !providerKind.isEmpty else { return false }
        if providerKind == BrowserSyncProviderSettingsStore.localFolderProviderKind {
            return !(localFolderRootPath ?? "").isEmpty
        }
        return true
    }
}

// Owner of the durable browser-sync provider connection settings on macOS.
// The security-scoped folder bookmark (the only way a sandboxed app can reach
// a user-picked folder across launches) is the source of truth for folder
// access and is stored in UserDefaults; the JSON snapshot records the
// human-readable path alongside provider kind and the membership marker.
final class BrowserSyncProviderSettingsStore {
    static let shared = BrowserSyncProviderSettingsStore()

    static let localFolderProviderKind = "localFolder"

    private static let currentVersion = 1
    private static let defaultScope = "default"

    // The four roadmap providers of the cloud-drive architecture. Every other
    // token is contract drift and degrades to "no provider configured"
    // instead of flowing an unknown kind into the Rust connection contract.
    private static let knownProviderKinds: Set<String> = [
        localFolderProviderKind,
        "iCloudDrive",
        "googleDrive",
        "oneDrive",
    ]

    private static let logger = Logger(subsystem: "io.actrium.actro", category: "BrowserSyncProviderSettings")

    private let fileURL: URL
    private let bookmarkKey = "com.actrium.actro.browserSync.providerFolderBookmark"
    private let userDefaults = UserDefaults.standard
    private let deviceIdentifierProvider: () -> String

    private init(
        fileURL: URL = BrowserSyncProviderSettingsStore.defaultFileURL(),
        deviceIdentifierProvider: @escaping () -> String = {
            BrowserSyncCredentialsProvider.shared.deviceIdentifier()
        }
    ) {
        self.fileURL = fileURL
        self.deviceIdentifierProvider = deviceIdentifierProvider
    }

    private static func defaultFileURL() -> URL {
        let fileManager = FileManager.default
        let appSupport = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let appFolder = appSupport.appendingPathComponent("Actro", isDirectory: true)
        if !fileManager.fileExists(atPath: appFolder.path) {
            try? fileManager.createDirectory(at: appFolder, withIntermediateDirectories: true)
        }
        return appFolder.appendingPathComponent("browser-sync-provider.json", isDirectory: false)
    }

    // MARK: - Load / Save

    func load() -> BrowserSyncProviderSettingsSnapshot {
        guard FileManager.default.fileExists(atPath: fileURL.path) else {
            return normalize(nil)
        }

        do {
            let data = try Data(contentsOf: fileURL)
            let decoded = try JSONDecoder().decode(BrowserSyncProviderSettingsSnapshot.self, from: data)
            return normalize(decoded)
        } catch {
            // Corruption is not "disconnected". Device identity is owned by
            // the credentials provider, so it survives regardless, but the
            // provider connection cannot be trusted from an unreadable file;
            // report not-configured and log loudly for recovery.
            Self.logger.error("Browser sync provider settings file unreadable, treating as not configured: \(String(describing: error))")
            return normalize(nil)
        }
    }

    @discardableResult
    func save(_ snapshot: BrowserSyncProviderSettingsSnapshot) -> BrowserSyncProviderSettingsSnapshot {
        let normalized = normalize(snapshot)
        do {
            let data = try JSONEncoder.sortedPretty.encode(normalized)
            try data.write(to: fileURL, options: .atomic)
        } catch {
            Self.logger.error("Failed to persist browser sync provider settings: \(String(describing: error))")
        }
        return normalized
    }

    // MARK: - Mutations

    // Persists the picked local folder as the active provider. The
    // security-scoped bookmark is the source of truth for access; the path is
    // recorded in the JSON snapshot for display and Rust round input. The
    // membership marker resets because a new (or re-targeted) folder has not
    // been joined yet.
    @discardableResult
    func configureLocalFolder(rootPath: String, bookmarkData: Data) -> BrowserSyncProviderSettingsSnapshot {
        userDefaults.set(bookmarkData, forKey: bookmarkKey)
        let current = load()
        return save(BrowserSyncProviderSettingsSnapshot(
            version: Self.currentVersion,
            providerKind: Self.localFolderProviderKind,
            localFolderRootPath: rootPath,
            scope: current.scope,
            deviceId: current.deviceId,
            membershipJoinedAtUnixSeconds: nil,
            savedAtUnixSeconds: 0
        ))
    }

    @discardableResult
    func saveMembershipJoined(_ joinedAtUnixSeconds: Int64) -> BrowserSyncProviderSettingsSnapshot {
        var snapshot = load()
        snapshot.membershipJoinedAtUnixSeconds = joinedAtUnixSeconds
        snapshot.savedAtUnixSeconds = 0
        return save(snapshot)
    }

    @discardableResult
    func clearMembershipMarker() -> BrowserSyncProviderSettingsSnapshot {
        var snapshot = load()
        snapshot.membershipJoinedAtUnixSeconds = nil
        snapshot.savedAtUnixSeconds = 0
        return save(snapshot)
    }

    // Disconnect means "stop provider sync": provider fields and the
    // membership marker are cleared and the folder bookmark is dropped, but
    // device identity (owned by the credentials provider) is untouched, so a
    // later reconnect resumes the same cloud device document.
    @discardableResult
    func disconnect() -> BrowserSyncProviderSettingsSnapshot {
        userDefaults.removeObject(forKey: bookmarkKey)
        var snapshot = load()
        snapshot.providerKind = nil
        snapshot.localFolderRootPath = nil
        snapshot.membershipJoinedAtUnixSeconds = nil
        snapshot.savedAtUnixSeconds = 0
        return save(snapshot)
    }

    // MARK: - Security-scoped access

    // Resolves the persisted security-scoped bookmark into a folder URL,
    // re-saving it when the system reports it stale. The caller must bracket
    // its file IO with start/stopAccessingSecurityScopedResource() on the
    // returned URL, because the Rust local-folder round does real IO on it.
    func resolveRootURL() -> URL? {
        guard let data = userDefaults.data(forKey: bookmarkKey) else { return nil }
        do {
            var isStale = false
            let url = try URL(
                resolvingBookmarkData: data,
                options: .withSecurityScope,
                relativeTo: nil,
                bookmarkDataIsStale: &isStale
            )
            if isStale {
                if let refreshed = try? url.bookmarkData(
                    options: .withSecurityScope,
                    includingResourceValuesForKeys: nil,
                    relativeTo: nil
                ) {
                    userDefaults.set(refreshed, forKey: bookmarkKey)
                }
            }
            return url
        } catch {
            Self.logger.error("Failed to resolve browser sync provider folder bookmark: \(String(describing: error))")
            return nil
        }
    }

    // MARK: - Normalize

    private func normalize(_ snapshot: BrowserSyncProviderSettingsSnapshot?) -> BrowserSyncProviderSettingsSnapshot {
        let providerKind = normalizeProviderKind(snapshot?.providerKind)
        let rootPath = normalizeText(snapshot?.localFolderRootPath)

        // The local-folder root must be absolute: a relative path would
        // resolve against the process working directory and sync into the
        // wrong folder, so it degrades to "not configured" instead.
        let normalizedRootPath: String?
        if providerKind == Self.localFolderProviderKind,
           !rootPath.isEmpty,
           (rootPath as NSString).isAbsolutePath {
            normalizedRootPath = rootPath
        } else {
            normalizedRootPath = nil
        }

        let configured = providerKind != nil
            && (providerKind != Self.localFolderProviderKind || normalizedRootPath != nil)

        let scope = normalizeText(snapshot?.scope)
        let deviceId = normalizeText(snapshot?.deviceId)

        // A membership marker without a configured provider is meaningless
        // and would leak into the next connection.
        let membershipJoinedAt = snapshot?.membershipJoinedAtUnixSeconds
        let normalizedMembership = (configured && (membershipJoinedAt ?? 0) > 0) ? membershipJoinedAt : nil

        let savedAt = (snapshot?.savedAtUnixSeconds ?? 0) > 0
            ? snapshot!.savedAtUnixSeconds
            : Int64(Date().timeIntervalSince1970)

        return BrowserSyncProviderSettingsSnapshot(
            version: Self.currentVersion,
            providerKind: providerKind,
            localFolderRootPath: normalizedRootPath,
            scope: scope.isEmpty ? Self.defaultScope : scope,
            // Device identity is owned by the credentials provider; adopt an
            // empty/missing persisted id from there instead of minting a new
            // one so the same machine never appears as two cloud devices.
            deviceId: deviceId.isEmpty ? deviceIdentifierProvider() : deviceId,
            membershipJoinedAtUnixSeconds: normalizedMembership,
            savedAtUnixSeconds: savedAt
        )
    }

    private func normalizeProviderKind(_ providerKind: String?) -> String? {
        let normalized = normalizeText(providerKind)
        return Self.knownProviderKinds.contains(normalized) ? normalized : nil
    }

    private func normalizeText(_ value: String?) -> String {
        value?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    }
}

private extension JSONEncoder {
    static var sortedPretty: JSONEncoder {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        return encoder
    }
}
