import Foundation
import Security
import os

/// Minimal generic-password Keychain wrapper for small secrets (for example
/// sync session credentials) that must not be persisted in UserDefaults,
/// where values end up as plaintext plist entries on disk.
///
/// Items are stored as `kSecClassGenericPassword` with
/// `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`: readable by background
/// sync once the device has been unlocked after boot, and never migrated to
/// another device via backup or restore.
public struct KeychainStore {
    private static let logger = Logger(subsystem: "io.actrium.actro", category: "KeychainStore")

    public let service: String

    public init(service: String) {
        self.service = service
    }

    /// Returns the stored data for `account`, or nil when the item is missing
    /// or unreadable. Read failures other than "not found" are logged as
    /// errors because callers will treat them as "no credentials".
    public func data(forAccount account: String) -> Data? {
        var query = baseQuery(account: account)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne

        var result: AnyObject?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        switch status {
        case errSecSuccess:
            return result as? Data
        case errSecItemNotFound:
            return nil
        default:
            Self.logger.error(
                "Keychain read failed (service: \(service, privacy: .public), account: \(account, privacy: .public), OSStatus: \(status))"
            )
            return nil
        }
    }

    /// Stores `data` for `account`, replacing any existing item.
    /// Returns false when the item could not be persisted; callers should
    /// treat that as "credentials were not saved" rather than crash.
    @discardableResult
    public func setData(_ data: Data, forAccount account: String) -> Bool {
        // Delete-then-add keeps the flow simple and refreshes item attributes
        // (an update would keep whatever accessibility the old item had).
        SecItemDelete(baseQuery(account: account) as CFDictionary)

        var attributes = baseQuery(account: account)
        attributes[kSecValueData as String] = data
        attributes[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly

        let status = SecItemAdd(attributes as CFDictionary, nil)
        guard status == errSecSuccess else {
            Self.logger.error(
                "Keychain write failed (service: \(service, privacy: .public), account: \(account, privacy: .public), OSStatus: \(status))"
            )
            return false
        }
        return true
    }

    /// Removes the item for `account`. A missing item is not an error.
    public func removeData(forAccount account: String) {
        let status = SecItemDelete(baseQuery(account: account) as CFDictionary)
        if status != errSecSuccess && status != errSecItemNotFound {
            Self.logger.warning(
                "Keychain delete failed (service: \(service, privacy: .public), account: \(account, privacy: .public), OSStatus: \(status))"
            )
        }
    }

    private func baseQuery(account: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
    }
}
