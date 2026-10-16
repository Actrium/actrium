import Foundation
import ActriumCore
import os

final class BrowserSyncCredentialsProvider: AppleBrowserStorageSyncCredentialsProvider {
    static let shared = BrowserSyncCredentialsProvider()

    private struct StoredSession: Codable {
        var environment: String
        var accessToken: String
        var baseUrl: String
        var savedAtUnixSeconds: Int64
    }

    private static let logger = Logger(subsystem: "io.actrium", category: "BrowserSyncCredentials")

    private let deviceIdKey = "com.actrium.actro.browserSync.deviceId"
    // Legacy plaintext UserDefaults key. The session (including the access
    // token) now lives in the Keychain; this key is only read once for
    // migration and then deleted.
    private let legacyStoredSessionKey = "com.actrium.actro.browserSync.session"
    private let defaultScope = "default"
    private let userDefaults = UserDefaults.standard
    private let keychain = KeychainStore(service: "com.actrium.actro.browserSync")
    private let keychainSessionAccount = "session"

    private init() {}

    func save(environment: String, accessToken: String, baseUrl: String) {
        let session = StoredSession(
            environment: environment.trimmingCharacters(in: .whitespacesAndNewlines),
            accessToken: accessToken.trimmingCharacters(in: .whitespacesAndNewlines),
            baseUrl: baseUrl.trimmingCharacters(in: .whitespacesAndNewlines),
            savedAtUnixSeconds: Int64(Date().timeIntervalSince1970)
        )
        persist(session)
        _ = deviceId()
    }

    func clear(environment: String) {
        guard let session = storedSession() else {
            removeStoredSession()
            return
        }
        let requestedEnvironment = environment.trimmingCharacters(in: .whitespacesAndNewlines)
        if requestedEnvironment.isEmpty || session.environment == requestedEnvironment {
            removeStoredSession()
        }
    }

    /// Browser sync is decoupled from the Actrium account: credentials come
    /// only from the sync session saved via `saveSyncSession` (never from the
    /// account login token). With no stored session this returns nil and the
    /// sync scheduler stays in `waitingForCredentials`.
    func browserStorageSyncCredentials() async -> AppleBrowserStorageSyncCredentials? {
        guard let session = storedSession(),
              !session.accessToken.isEmpty,
              !session.baseUrl.isEmpty
        else {
            return nil
        }

        return AppleBrowserStorageSyncCredentials(
            baseUrl: session.baseUrl,
            accessToken: session.accessToken,
            scope: defaultScope,
            deviceId: deviceId()
        )
    }

    /// Writes the session to the Keychain. Any leftover plaintext copy in
    /// UserDefaults is removed only after a successful write, so a transient
    /// Keychain failure never destroys the only remaining copy of the
    /// credentials.
    @discardableResult
    private func persist(_ session: StoredSession) -> Bool {
        guard let data = try? JSONEncoder().encode(session) else {
            Self.logger.error("Failed to encode browser sync session; credentials were not saved")
            return false
        }
        guard keychain.setData(data, forAccount: keychainSessionAccount) else {
            return false
        }
        userDefaults.removeObject(forKey: legacyStoredSessionKey)
        return true
    }

    private func removeStoredSession() {
        keychain.removeData(forAccount: keychainSessionAccount)
        userDefaults.removeObject(forKey: legacyStoredSessionKey)
    }

    private func storedSession() -> StoredSession? {
        if let data = keychain.data(forAccount: keychainSessionAccount),
           let session = try? JSONDecoder().decode(StoredSession.self, from: data) {
            return normalized(session)
        }

        // One-time migration: earlier builds persisted the session (including
        // the access token) in UserDefaults, i.e. as a plaintext plist on
        // disk. Move any leftover copy into the Keychain and delete the
        // plaintext key.
        guard let data = userDefaults.data(forKey: legacyStoredSessionKey),
              let legacy = try? JSONDecoder().decode(StoredSession.self, from: data)
        else {
            return nil
        }
        let session = normalized(legacy)
        if persist(session) {
            Self.logger.notice("Migrated browser sync session from UserDefaults to Keychain")
        } else {
            Self.logger.error(
                "Keychain migration of browser sync session failed; plaintext copy kept in UserDefaults so migration can retry on next read"
            )
        }
        return session
    }

    private func normalized(_ session: StoredSession) -> StoredSession {
        StoredSession(
            environment: session.environment.trimmingCharacters(in: .whitespacesAndNewlines),
            accessToken: session.accessToken.trimmingCharacters(in: .whitespacesAndNewlines),
            baseUrl: session.baseUrl.trimmingCharacters(in: .whitespacesAndNewlines),
            savedAtUnixSeconds: session.savedAtUnixSeconds
        )
    }

    private func deviceId() -> String {
        if let existing = userDefaults.string(forKey: deviceIdKey),
           !existing.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return existing
        }

        let next = UUID().uuidString
        userDefaults.set(next, forKey: deviceIdKey)
        return next
    }
}
