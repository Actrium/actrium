import Foundation
import Security
import LocalAuthentication

enum KeychainError: LocalizedError {
    case saveFailed(OSStatus)
    case loadFailed(OSStatus)
    case biometricsUnavailable
    case dataConversionFailed
    case accessControlCreationFailed

    var errorDescription: String? {
        switch self {
        case .saveFailed(let status): return "Keychain save failed (OSStatus \(status))"
        case .loadFailed(let status): return "Keychain load failed (OSStatus \(status))"
        case .biometricsUnavailable: return "Biometrics unavailable"
        case .dataConversionFailed: return "Data conversion failed"
        case .accessControlCreationFailed: return "Access control creation failed"
        }
    }
}

struct KeychainHelper {
    private static let service = "com.canvas.vault"
    private static let account = "master-password"

    static func saveMasterPassword(_ password: String) throws {
        // Delete any existing item first
        deleteMasterPassword()

        guard let data = password.data(using: .utf8) else {
            throw KeychainError.dataConversionFailed
        }

        var error: Unmanaged<CFError>?
        guard let accessControl = SecAccessControlCreateWithFlags(
            nil,
            kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly,
            .biometryCurrentSet,
            &error
        ) else {
            throw KeychainError.accessControlCreationFailed
        }

        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecValueData as String: data,
            kSecAttrAccessControl as String: accessControl,
            kSecUseDataProtectionKeychain as String: true
        ]

        let status = SecItemAdd(query as CFDictionary, nil)
        guard status == errSecSuccess else {
            throw KeychainError.saveFailed(status)
        }
    }

    static func loadMasterPasswordWithBiometrics() async throws -> String {
        let context = LAContext()
        context.localizedReason = "Unlock password vault"

        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecUseAuthenticationContext as String: context,
            kSecUseDataProtectionKeychain as String: true
        ]

        return try await withCheckedThrowingContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async {
                var result: AnyObject?
                let status = SecItemCopyMatching(query as CFDictionary, &result)

                guard status == errSecSuccess,
                      let data = result as? Data,
                      let password = String(data: data, encoding: .utf8) else {
                    continuation.resume(throwing: KeychainError.loadFailed(status))
                    return
                }
                continuation.resume(returning: password)
            }
        }
    }

    static func deleteMasterPassword() {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecUseDataProtectionKeychain as String: true
        ]
        SecItemDelete(query as CFDictionary)
    }

    static func isBiometricsAvailable() -> Bool {
        let context = LAContext()
        var error: NSError?
        return context.canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: &error)
    }
}
