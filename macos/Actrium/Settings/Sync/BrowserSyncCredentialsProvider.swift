import Foundation
import ActriumCore
import os

struct BrowserSyncCredentialSnapshot: Equatable {
    var environment: String
    var baseUrl: String
    var accessToken: String
    var scope: String
    var deviceId: String
    var savedAtUnixSeconds: Int64
}

final class BrowserSyncCredentialsProvider: AppleBrowserStorageSyncCredentialsProvider {
    static let shared = BrowserSyncCredentialsProvider()
    static let defaultEnvironment = authEnvironmentProd()
    static let supportedEnvironments = [
        authEnvironmentProd(),
        authEnvironmentBeta(),
        authEnvironmentDev(),
    ]

    static func defaultBaseUrl(for environment: String) -> String {
        authAccountBaseUrl(environment: normalizedEnvironment(environment))
    }

    static func normalizedEnvironment(_ environment: String) -> String {
        let trimmed = environment.trimmingCharacters(in: .whitespacesAndNewlines)
        return supportedEnvironments.contains(trimmed) ? trimmed : defaultEnvironment
    }

    private struct StoredSession: Codable {
        var environment: String
        var accessToken: String
        var baseUrl: String
        var scope: String
        var savedAtUnixSeconds: Int64
    }

    private static let logger = Logger(subsystem: "io.actrium", category: "BrowserSyncCredentials")

    // Legacy plaintext UserDefaults keys. The session (including the access
    // token) now lives in the Keychain; these keys are only read once for
    // migration and then deleted.
    private let baseUrlKey = "browserSyncBaseUrl"
    private let accessTokenKey = "browserSyncAccessToken"
    private let scopeKey = "browserSyncScope"
    private let legacyStoredSessionKey = "io.actrium.browserSync.session"
    private let deviceIdKey = "io.actrium.browserSync.deviceId"
    private let legacyDeviceIdKey = "syncDeviceId"
    private let defaultScope = "default"
    private let userDefaults = UserDefaults.standard
    private let keychain = KeychainStore(service: "io.actrium.browserSync")
    private let keychainSessionAccount = "session"

    private init() {}

    func save(baseUrl: String, accessToken: String, scope: String = "default") {
        saveSession(
            environment: Self.defaultEnvironment,
            accessToken: accessToken,
            baseUrl: baseUrl,
            scope: scope
        )
    }

    func saveSession(
        environment: String,
        accessToken: String,
        baseUrl: String,
        scope: String = "default"
    ) {
        let normalizedEnvironment = Self.normalizedEnvironment(environment)
        let session = StoredSession(
            environment: normalizedEnvironment,
            accessToken: accessToken.trimmingCharacters(in: .whitespacesAndNewlines),
            baseUrl: normalizedBaseUrl(baseUrl, environment: normalizedEnvironment),
            scope: normalizedScope(scope),
            savedAtUnixSeconds: Int64(Date().timeIntervalSince1970)
        )
        persist(session)
        _ = deviceId()
    }

    func clear() {
        keychain.removeData(forAccount: keychainSessionAccount)
        removePlaintextSessionCopies()
    }

    func clear(environment: String) {
        guard let session = storedSession() else {
            clear()
            return
        }

        let requestedEnvironment = environment.trimmingCharacters(in: .whitespacesAndNewlines)
        if requestedEnvironment.isEmpty || session.environment == requestedEnvironment {
            clear()
        }
    }

    /// The durable browser-sync device identity. This provider is the sole
    /// owner of the device id (persisted under a dedicated UserDefaults key
    /// and minted on first read), so the local-folder provider settings store
    /// reuses this value instead of minting a second identity that would
    /// present the same machine as two devices to the shared cloud folder.
    func deviceIdentifier() -> String {
        deviceId()
    }

    func snapshot() -> BrowserSyncCredentialSnapshot? {
        guard let session = storedSession() else { return nil }
        return BrowserSyncCredentialSnapshot(
            environment: session.environment,
            baseUrl: session.baseUrl,
            accessToken: session.accessToken,
            scope: session.scope,
            deviceId: deviceId(),
            savedAtUnixSeconds: session.savedAtUnixSeconds
        )
    }

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
            scope: session.scope,
            deviceId: deviceId()
        )
    }

    /// Writes the session to the Keychain. Any leftover plaintext copy in
    /// UserDefaults is removed only after a successful write, so a transient
    /// Keychain failure (for example a locked keychain) never destroys the
    /// only remaining copy of the credentials.
    @discardableResult
    private func persist(_ session: StoredSession) -> Bool {
        guard let data = try? JSONEncoder().encode(session) else {
            Self.logger.error("Failed to encode browser sync session; credentials were not saved")
            return false
        }
        guard keychain.setData(data, forAccount: keychainSessionAccount) else {
            return false
        }
        removePlaintextSessionCopies()
        return true
    }

    private func removePlaintextSessionCopies() {
        userDefaults.removeObject(forKey: legacyStoredSessionKey)
        userDefaults.removeObject(forKey: baseUrlKey)
        userDefaults.removeObject(forKey: accessTokenKey)
        userDefaults.removeObject(forKey: scopeKey)
    }

    private func storedSession() -> StoredSession? {
        if let data = keychain.data(forAccount: keychainSessionAccount),
           let session = try? JSONDecoder().decode(StoredSession.self, from: data) {
            return normalizedSession(session)
        }
        return migrateLegacySessionIfNeeded()
    }

    /// One-time migration: earlier builds persisted the session (including
    /// the access token) in UserDefaults, i.e. as a plaintext plist on disk.
    /// Move any leftover copy into the Keychain and delete the plaintext keys.
    private func migrateLegacySessionIfNeeded() -> StoredSession? {
        guard let legacy = legacyUserDefaultsSession ?? legacyBareKeySession else {
            return nil
        }

        let session = normalizedSession(legacy)
        if persist(session) {
            Self.logger.notice("Migrated browser sync session from UserDefaults to Keychain")
        } else {
            Self.logger.error(
                "Keychain migration of browser sync session failed; plaintext copy kept in UserDefaults so migration can retry on next read"
            )
        }
        return session
    }

    /// Session JSON stored under the pre-Keychain UserDefaults key.
    private var legacyUserDefaultsSession: StoredSession? {
        guard let data = userDefaults.data(forKey: legacyStoredSessionKey),
              let session = try? JSONDecoder().decode(StoredSession.self, from: data)
        else {
            return nil
        }
        return session
    }

    /// Oldest storage format: separate bare UserDefaults keys.
    private var legacyBareKeySession: StoredSession? {
        let token = userDefaults.string(forKey: accessTokenKey)?
            .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let baseUrl = userDefaults.string(forKey: baseUrlKey)?
            .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let scope = userDefaults.string(forKey: scopeKey)?
            .trimmingCharacters(in: .whitespacesAndNewlines) ?? defaultScope

        guard !token.isEmpty, !baseUrl.isEmpty else {
            return nil
        }

        return StoredSession(
            environment: Self.defaultEnvironment,
            accessToken: token,
            baseUrl: baseUrl,
            scope: normalizedScope(scope),
            savedAtUnixSeconds: Int64(Date().timeIntervalSince1970)
        )
    }

    private func normalizedSession(_ session: StoredSession) -> StoredSession {
        let environment = Self.normalizedEnvironment(session.environment)
        return StoredSession(
            environment: environment,
            accessToken: session.accessToken.trimmingCharacters(in: .whitespacesAndNewlines),
            baseUrl: normalizedBaseUrl(session.baseUrl, environment: environment),
            scope: normalizedScope(session.scope),
            savedAtUnixSeconds: session.savedAtUnixSeconds > 0
                ? session.savedAtUnixSeconds
                : Int64(Date().timeIntervalSince1970)
        )
    }

    private func normalizedBaseUrl(_ baseUrl: String, environment: String) -> String {
        let trimmed = baseUrl.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? Self.defaultBaseUrl(for: environment) : trimmed
    }

    private func normalizedScope(_ scope: String) -> String {
        let trimmed = scope.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? defaultScope : trimmed
    }

    private func deviceId() -> String {
        if let existing = userDefaults.string(forKey: deviceIdKey),
           !existing.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return existing
        }

        if let legacy = userDefaults.string(forKey: legacyDeviceIdKey),
           !legacy.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            userDefaults.set(legacy, forKey: deviceIdKey)
            userDefaults.removeObject(forKey: legacyDeviceIdKey)
            return legacy
        }

        let next = UUID().uuidString
        userDefaults.set(next, forKey: deviceIdKey)
        return next
    }
}
