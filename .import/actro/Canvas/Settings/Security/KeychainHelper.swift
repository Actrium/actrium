import Foundation
import Security
import LocalAuthentication

enum KeychainError: LocalizedError {
    case saveFailed(OSStatus)
    case loadFailed(OSStatus)
    case biometricsUnavailable
    case biometricsFailed(String)
    case dataConversionFailed

    var errorDescription: String? {
        switch self {
        case .saveFailed(let status): return "Keychain save failed (OSStatus \(status))"
        case .loadFailed(let status): return "Keychain load failed (OSStatus \(status))"
        case .biometricsUnavailable: return "Biometrics unavailable"
        case .biometricsFailed(let msg): return "Biometric authentication failed: \(msg)"
        case .dataConversionFailed: return "Data conversion failed"
        }
    }
}

struct KeychainHelper {
    private static let service = "com.canvas.vault"
    private static let account = "master-password"

    /// Save master password to standard Keychain (no biometric access control)
    static func saveMasterPassword(_ password: String) throws {
        deleteMasterPassword()

        guard let data = password.data(using: .utf8) else {
            throw KeychainError.dataConversionFailed
        }

        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        ]

        let status = SecItemAdd(query as CFDictionary, nil)
        guard status == errSecSuccess else {
            throw KeychainError.saveFailed(status)
        }
    }

    /// Authenticate with Touch ID first, then load password from Keychain
    static func loadMasterPasswordWithBiometrics() async throws -> String {
        // Step 1: Biometric authentication via LAContext
        let context = LAContext()
        var authError: NSError?
        guard context.canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: &authError) else {
            throw KeychainError.biometricsUnavailable
        }

        try await context.evaluatePolicy(
            .deviceOwnerAuthenticationWithBiometrics,
            localizedReason: "Unlock password vault"
        )

        // Step 2: Load from standard Keychain after biometric success
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true
        ]

        var result: AnyObject?
        let status = SecItemCopyMatching(query as CFDictionary, &result)

        guard status == errSecSuccess,
              let data = result as? Data,
              let password = String(data: data, encoding: .utf8) else {
            throw KeychainError.loadFailed(status)
        }
        return password
    }

    static func deleteMasterPassword() {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account
        ]
        SecItemDelete(query as CFDictionary)
    }

    static func isBiometricsAvailable() -> Bool {
        let context = LAContext()
        var error: NSError?
        return context.canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: &error)
    }
}
