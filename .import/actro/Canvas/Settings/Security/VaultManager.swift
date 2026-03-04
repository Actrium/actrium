import Foundation
import CryptoKit
import CommonCrypto
import Combine

enum VaultState {
    case notSetup, locked, unlocked
}

enum VaultError: LocalizedError {
    case notSetup
    case alreadySetup
    case wrongPassword
    case locked
    case encryptionFailed
    case decryptionFailed
    case keyDerivationFailed

    var errorDescription: String? {
        switch self {
        case .notSetup: return "Vault is not set up"
        case .alreadySetup: return "Vault is already set up"
        case .wrongPassword: return "Incorrect password"
        case .locked: return "Vault is locked"
        case .encryptionFailed: return "Encryption failed"
        case .decryptionFailed: return "Decryption failed"
        case .keyDerivationFailed: return "Key derivation failed"
        }
    }
}

@MainActor
final class VaultManager: ObservableObject {
    static let shared = VaultManager()

    @Published var state: VaultState = .notSetup
    @Published var credentials: [Credential] = []

    @Published var autoLockTimeout: Int {
        didSet {
            UserDefaults.standard.set(autoLockTimeout, forKey: "vault.autoLockTimeout")
            setupAutoLockTimer()
        }
    }

    @Published var isTouchIDEnabled: Bool {
        didSet {
            UserDefaults.standard.set(isTouchIDEnabled, forKey: "vault.isTouchIDEnabled")
        }
    }

    private var derivedKey: SymmetricKey?
    private var autoLockTimer: Timer?

    private init() {
        self.autoLockTimeout = UserDefaults.standard.object(forKey: "vault.autoLockTimeout") as? Int ?? 5
        self.isTouchIDEnabled = UserDefaults.standard.bool(forKey: "vault.isTouchIDEnabled")

        // Determine initial state
        if VaultDatabase.getVaultMeta(key: "salt") != nil {
            state = .locked
        } else {
            state = .notSetup
        }
    }

    // MARK: - Setup

    func setupMasterPassword(_ password: String, enableTouchID: Bool) throws {
        guard state == .notSetup else { throw VaultError.alreadySetup }

        // Generate random salt
        var saltBytes = [UInt8](repeating: 0, count: 32)
        _ = SecRandomCopyBytes(kSecRandomDefault, saltBytes.count, &saltBytes)
        let salt = Data(saltBytes)

        let key = try deriveKey(password: password, salt: salt)
        derivedKey = key

        // Store salt
        VaultDatabase.setVaultMeta(key: "salt", value: salt.base64EncodedString())

        // Store verification token
        let verificationToken = try encrypt("vault-verification-token")
        VaultDatabase.setVaultMeta(key: "verification_hash", value: verificationToken)

        // Enable Touch ID if requested
        isTouchIDEnabled = enableTouchID
        if enableTouchID {
            try? KeychainHelper.saveMasterPassword(password)
        }

        state = .unlocked
        setupAutoLockTimer()
    }

    // MARK: - Unlock / Lock

    func unlock(password: String) throws {
        guard state == .locked else {
            if state == .notSetup { throw VaultError.notSetup }
            return
        }

        guard let saltString = VaultDatabase.getVaultMeta(key: "salt"),
              let salt = Data(base64Encoded: saltString) else {
            throw VaultError.notSetup
        }

        let key = try deriveKey(password: password, salt: salt)

        // Verify password by decrypting verification token
        guard let storedToken = VaultDatabase.getVaultMeta(key: "verification_hash") else {
            throw VaultError.notSetup
        }

        derivedKey = key
        do {
            let decrypted = try decrypt(storedToken)
            guard decrypted == "vault-verification-token" else {
                derivedKey = nil
                throw VaultError.wrongPassword
            }
        } catch {
            derivedKey = nil
            throw VaultError.wrongPassword
        }

        state = .unlocked
        try loadCredentials()
        setupAutoLockTimer()
    }

    func unlockWithBiometrics() async throws {
        let password = try await KeychainHelper.loadMasterPasswordWithBiometrics()
        try unlock(password: password)
    }

    func lock() {
        derivedKey = nil
        credentials = []
        if state == .unlocked {
            state = .locked
        }
        autoLockTimer?.invalidate()
        autoLockTimer = nil
    }

    // MARK: - Change Password

    func changeMasterPassword(current: String, new newPassword: String) throws {
        // Verify current password first
        guard let saltString = VaultDatabase.getVaultMeta(key: "salt"),
              let salt = Data(base64Encoded: saltString) else {
            throw VaultError.notSetup
        }

        let currentKey = try deriveKey(password: current, salt: salt)

        guard let storedToken = VaultDatabase.getVaultMeta(key: "verification_hash") else {
            throw VaultError.notSetup
        }

        // Temporarily set key to verify
        let previousKey = derivedKey
        derivedKey = currentKey
        do {
            let decrypted = try decrypt(storedToken)
            guard decrypted == "vault-verification-token" else {
                derivedKey = previousKey
                throw VaultError.wrongPassword
            }
        } catch {
            derivedKey = previousKey
            throw VaultError.wrongPassword
        }

        // Decrypt all credentials with old key
        let oldKey = currentKey
        derivedKey = oldKey
        let decryptedCreds = try credentials.isEmpty ? loadAndReturnCredentials() : credentials

        // Generate new salt and key
        var newSaltBytes = [UInt8](repeating: 0, count: 32)
        _ = SecRandomCopyBytes(kSecRandomDefault, newSaltBytes.count, &newSaltBytes)
        let newSalt = Data(newSaltBytes)
        let newKey = try deriveKey(password: newPassword, salt: newSalt)
        derivedKey = newKey

        // Update salt
        VaultDatabase.setVaultMeta(key: "salt", value: newSalt.base64EncodedString())

        // Update verification token
        let newVerification = try encrypt("vault-verification-token")
        VaultDatabase.setVaultMeta(key: "verification_hash", value: newVerification)

        // Re-encrypt all credentials
        for cred in decryptedCreds {
            let encUsername = try encrypt(cred.username)
            let encPassword = try encrypt(cred.password)
            VaultDatabase.updateCredential(
                id: cred.id,
                domain: cred.domain,
                usernameEncrypted: encUsername,
                passwordEncrypted: encPassword,
                notes: cred.notes,
                updatedAt: Int64(cred.updatedAt.timeIntervalSince1970)
            )
        }

        // Update keychain if Touch ID enabled
        if isTouchIDEnabled {
            try? KeychainHelper.saveMasterPassword(newPassword)
        }

        credentials = decryptedCreds
    }

    // MARK: - Credential Management

    func addCredential(domain: String, username: String, password: String, notes: String?) throws {
        guard state == .unlocked, derivedKey != nil else { throw VaultError.locked }

        let id = UUID().uuidString
        let now = Date()
        let encUsername = try encrypt(username)
        let encPassword = try encrypt(password)

        VaultDatabase.insertCredential(
            id: id,
            domain: domain,
            usernameEncrypted: encUsername,
            passwordEncrypted: encPassword,
            notes: notes,
            createdAt: Int64(now.timeIntervalSince1970),
            updatedAt: Int64(now.timeIntervalSince1970)
        )

        let credential = Credential(
            id: id,
            domain: domain,
            username: username,
            password: password,
            notes: notes,
            createdAt: now,
            updatedAt: now
        )
        credentials.append(credential)
    }

    func updateCredential(_ credential: Credential) throws {
        guard state == .unlocked, derivedKey != nil else { throw VaultError.locked }

        let now = Date()
        let encUsername = try encrypt(credential.username)
        let encPassword = try encrypt(credential.password)

        VaultDatabase.updateCredential(
            id: credential.id,
            domain: credential.domain,
            usernameEncrypted: encUsername,
            passwordEncrypted: encPassword,
            notes: credential.notes,
            updatedAt: Int64(now.timeIntervalSince1970)
        )

        if let index = credentials.firstIndex(where: { $0.id == credential.id }) {
            var updated = credential
            updated.updatedAt = now
            credentials[index] = updated
        }
    }

    func deleteCredential(id: String) throws {
        guard state == .unlocked else { throw VaultError.locked }

        VaultDatabase.deleteCredential(id: id)
        credentials.removeAll { $0.id == id }
    }

    // MARK: - Crypto

    private func deriveKey(password: String, salt: Data) throws -> SymmetricKey {
        guard let passwordData = password.data(using: .utf8) else {
            throw VaultError.keyDerivationFailed
        }

        var derivedBytes = [UInt8](repeating: 0, count: 32)
        let status = salt.withUnsafeBytes { saltBuffer in
            passwordData.withUnsafeBytes { passwordBuffer in
                CCKeyDerivationPBKDF(
                    CCPBKDFAlgorithm(kCCPBKDF2),
                    passwordBuffer.baseAddress?.assumingMemoryBound(to: Int8.self),
                    passwordData.count,
                    saltBuffer.baseAddress?.assumingMemoryBound(to: UInt8.self),
                    salt.count,
                    CCPseudoRandomAlgorithm(kCCPRFHmacAlgSHA256),
                    100_000,
                    &derivedBytes,
                    32
                )
            }
        }

        guard status == kCCSuccess else {
            throw VaultError.keyDerivationFailed
        }

        return SymmetricKey(data: Data(derivedBytes))
    }

    private func encrypt(_ plaintext: String) throws -> String {
        guard let key = derivedKey else { throw VaultError.locked }
        guard let data = plaintext.data(using: .utf8) else { throw VaultError.encryptionFailed }

        do {
            let sealedBox = try AES.GCM.seal(data, using: key)
            guard let combined = sealedBox.combined else { throw VaultError.encryptionFailed }
            return combined.base64EncodedString()
        } catch {
            throw VaultError.encryptionFailed
        }
    }

    private func decrypt(_ ciphertext: String) throws -> String {
        guard let key = derivedKey else { throw VaultError.locked }
        guard let data = Data(base64Encoded: ciphertext) else { throw VaultError.decryptionFailed }

        do {
            let sealedBox = try AES.GCM.SealedBox(combined: data)
            let decryptedData = try AES.GCM.open(sealedBox, using: key)
            guard let string = String(data: decryptedData, encoding: .utf8) else {
                throw VaultError.decryptionFailed
            }
            return string
        } catch {
            throw VaultError.decryptionFailed
        }
    }

    private func loadCredentials() throws {
        credentials = try loadAndReturnCredentials()
    }

    private func loadAndReturnCredentials() throws -> [Credential] {
        let rows = VaultDatabase.fetchAllCredentials()
        return try rows.compactMap { row -> Credential? in
            guard let id = row["id"] as? String,
                  let domain = row["domain"] as? String,
                  let encPassword = row["password_encrypted"] as? String,
                  let createdAt = row["created_at"] as? Int64,
                  let updatedAt = row["updated_at"] as? Int64 else {
                return nil
            }

            let encUsername = row["username_encrypted"] as? String ?? ""
            let username = encUsername.isEmpty ? "" : try decrypt(encUsername)
            let password = try decrypt(encPassword)
            let notes = row["notes"] as? String

            return Credential(
                id: id,
                domain: domain,
                username: username,
                password: password,
                notes: notes,
                createdAt: Date(timeIntervalSince1970: TimeInterval(createdAt)),
                updatedAt: Date(timeIntervalSince1970: TimeInterval(updatedAt))
            )
        }
    }

    // MARK: - Auto Lock

    private func setupAutoLockTimer() {
        autoLockTimer?.invalidate()
        autoLockTimer = nil

        guard state == .unlocked, autoLockTimeout > 0 else { return }

        let timeout = TimeInterval(autoLockTimeout * 60)
        autoLockTimer = Timer.scheduledTimer(withTimeInterval: timeout, repeats: false) { [weak self] _ in
            Task { @MainActor [weak self] in
                self?.lock()
            }
        }
    }
}
