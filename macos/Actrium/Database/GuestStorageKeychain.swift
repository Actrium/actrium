import Foundation
import Security

nonisolated enum GuestStorageKeyError: LocalizedError, Equatable {
    case missing
    case unavailable(OSStatus)
    case generationFailed(OSStatus)

    var errorDescription: String? {
        switch self {
        case .missing:
            "The saved local storage key is missing from Keychain."
        case .unavailable(errSecMissingEntitlement):
            "This build does not have the required Keychain access for local storage."
        case .unavailable(errSecInteractionNotAllowed):
            "Keychain is unavailable for local storage. Unlock Keychain and try again."
        case .unavailable:
            "Keychain is unavailable for local storage."
        case .generationFailed:
            "A local storage key could not be generated."
        }
    }
}

nonisolated protocol GuestStorageKeychain: Sendable {
    func read() throws -> Data?
    func add(_ key: Data) throws -> Bool
}

nonisolated struct SystemGuestStorageKeychain: GuestStorageKeychain {
    private func item() throws -> [String: Any] {
        guard let task = SecTaskCreateFromSelf(nil),
              let appID = SecTaskCopyValueForEntitlement(task, "com.apple.application-identifier" as CFString, nil) as? String else {
            throw GuestStorageKeyError.unavailable(errSecMissingEntitlement)
        }
        return [
            kSecClass as String: kSecClassGenericPassword,
            kSecUseDataProtectionKeychain as String: true,
            kSecAttrAccessGroup as String: appID,
            kSecAttrService as String: "io.actrium.guest-local-storage",
            kSecAttrAccount as String: "default-profile",
        ]
    }

    func read() throws -> Data? {
        var query = try item()
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        switch status {
        case errSecSuccess:
            guard let key = result as? Data else { throw GuestStorageKeyError.unavailable(errSecDecode) }
            return key
        case errSecItemNotFound:
            return nil
        default:
            throw GuestStorageKeyError.unavailable(status)
        }
    }

    func add(_ key: Data) throws -> Bool {
        var attributes = try item()
        attributes[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        attributes[kSecValueData as String] = key
        switch SecItemAdd(attributes as CFDictionary, nil) {
        case errSecSuccess: return true
        case errSecDuplicateItem: return false
        case let status: throw GuestStorageKeyError.unavailable(status)
        }
    }
}
