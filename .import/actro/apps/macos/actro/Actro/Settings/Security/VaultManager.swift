import Foundation
import Combine
import LocalAuthentication
import ActroVault

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
    case biometricsSaveFailed(String)

    var errorDescription: String? {
        switch self {
        case .notSetup: return "Vault is not set up"
        case .alreadySetup: return "Vault is already set up"
        case .wrongPassword: return "Incorrect password"
        case .locked: return "Vault is locked"
        case .encryptionFailed: return "Encryption failed"
        case .decryptionFailed: return "Decryption failed"
        case .keyDerivationFailed: return "Key derivation failed"
        case .biometricsSaveFailed(let msg): return "Failed to save to Keychain: \(msg)"
        }
    }
}

@MainActor
final class VaultManager: ObservableObject {
    static let shared = VaultManager()

    @Published var state: VaultState = .notSetup
    @Published var items: [VaultItem] = []
    @Published var isProcessing: Bool = false

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

    var isBiometricsAvailable: Bool {
        KeychainHelper.isBiometricsAvailable()
    }

    var passwordHint: String? {
        VaultDatabase.getVaultMeta(key: "password_hint")
    }

    private var derivedKeyBase64: String?
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

    func setupMasterPassword(_ password: String, hint: String? = nil) async throws {
        guard state == .notSetup else { throw VaultError.alreadySetup }

        isProcessing = true
        defer { isProcessing = false }

        let salt = generateVaultSaltBase64()

        // Derive key on background thread (PBKDF2 is CPU-intensive)
        let key = try await Task.detached(priority: .userInitiated) {
            try Self.deriveKeyStatic(password: password, saltBase64: salt)
        }.value
        derivedKeyBase64 = key

        // Store salt
        VaultDatabase.setVaultMeta(key: "salt", value: salt)

        // Store verification token
        let verificationToken = try encrypt(vaultVerificationToken())
        VaultDatabase.setVaultMeta(key: "verification_hash", value: verificationToken)

        // Store password hint if provided
        if let hint = hint, !hint.isEmpty {
            VaultDatabase.setVaultMeta(key: "password_hint", value: hint)
        }

        state = .unlocked
        setupAutoLockTimer()
    }

    // MARK: - Unlock / Lock

    func unlock(password: String) async throws {
        guard state == .locked else {
            if state == .notSetup { throw VaultError.notSetup }
            return
        }

        isProcessing = true
        defer { isProcessing = false }

        guard let saltString = VaultDatabase.getVaultMeta(key: "salt") else {
            throw VaultError.notSetup
        }

        let key = try await Task.detached(priority: .userInitiated) {
            try Self.deriveKeyStatic(password: password, saltBase64: saltString)
        }.value

        // Verify password by decrypting verification token
        guard let storedToken = VaultDatabase.getVaultMeta(key: "verification_hash") else {
            throw VaultError.notSetup
        }

        derivedKeyBase64 = key
        do {
            let decrypted = try decrypt(storedToken)
            guard decrypted == vaultVerificationToken() else {
                derivedKeyBase64 = nil
                throw VaultError.wrongPassword
            }
        } catch {
            derivedKeyBase64 = nil
            throw VaultError.wrongPassword
        }

        state = .unlocked
        try migrateOldCredentials()
        try loadItems()
        setupAutoLockTimer()
    }

    func unlockWithBiometrics() async throws {
        let password = try await KeychainHelper.loadMasterPasswordWithBiometrics()
        try await unlock(password: password)
    }

    func lock() {
        derivedKeyBase64 = nil
        items = []
        if state == .unlocked {
            state = .locked
        }
        autoLockTimer?.invalidate()
        autoLockTimer = nil
    }

    // MARK: - Touch ID Management

    func enableTouchID(password: String) async throws {
        guard let saltString = VaultDatabase.getVaultMeta(key: "salt") else {
            throw VaultError.notSetup
        }

        let key = try await Task.detached(priority: .userInitiated) {
            try Self.deriveKeyStatic(password: password, saltBase64: saltString)
        }.value

        guard let storedToken = VaultDatabase.getVaultMeta(key: "verification_hash") else {
            throw VaultError.notSetup
        }

        let previousKey = derivedKeyBase64
        derivedKeyBase64 = key
        do {
            let decrypted = try decrypt(storedToken)
            guard decrypted == vaultVerificationToken() else {
                derivedKeyBase64 = previousKey
                throw VaultError.wrongPassword
            }
        } catch {
            derivedKeyBase64 = previousKey
            throw VaultError.wrongPassword
        }
        derivedKeyBase64 = previousKey

        try KeychainHelper.saveMasterPassword(password)
        isTouchIDEnabled = true
    }

    func disableTouchID() {
        KeychainHelper.deleteMasterPassword()
        isTouchIDEnabled = false
    }

    // MARK: - Change Password

    func changeMasterPassword(current: String, new newPassword: String, hint: String? = nil) async throws {
        guard state == .unlocked else { throw VaultError.locked }

        isProcessing = true
        defer { isProcessing = false }

        // Verify current password
        guard let saltString = VaultDatabase.getVaultMeta(key: "salt") else {
            throw VaultError.notSetup
        }

        let currentKey = try await Task.detached(priority: .userInitiated) {
            try Self.deriveKeyStatic(password: current, saltBase64: saltString)
        }.value

        guard let storedToken = VaultDatabase.getVaultMeta(key: "verification_hash") else {
            throw VaultError.notSetup
        }

        let previousKey = derivedKeyBase64
        derivedKeyBase64 = currentKey
        do {
            let decrypted = try decrypt(storedToken)
            guard decrypted == vaultVerificationToken() else {
                derivedKeyBase64 = previousKey
                throw VaultError.wrongPassword
            }
        } catch {
            derivedKeyBase64 = previousKey
            throw VaultError.wrongPassword
        }

        // Decrypt all items with old key
        let decryptedItems = try items.isEmpty ? loadAndReturnItems() : items

        let newSalt = generateVaultSaltBase64()

        let newKey = try await Task.detached(priority: .userInitiated) {
            try Self.deriveKeyStatic(password: newPassword, saltBase64: newSalt)
        }.value
        derivedKeyBase64 = newKey

        // Update salt
        VaultDatabase.setVaultMeta(key: "salt", value: newSalt)

        // Update verification token
        let newVerification = try encrypt(vaultVerificationToken())
        VaultDatabase.setVaultMeta(key: "verification_hash", value: newVerification)

        // Re-encrypt all items
        for item in decryptedItems {
            let encFields = try encryptFields(item.fields)
            VaultDatabase.updateVaultItem(
                id: item.id,
                title: item.title,
                category: item.category.rawValue,
                fieldsEncrypted: encFields,
                notes: item.notes,
                updatedAt: Int64(item.updatedAt.timeIntervalSince1970)
            )
        }

        // Update password hint
        if let hint = hint, !hint.isEmpty {
            VaultDatabase.setVaultMeta(key: "password_hint", value: hint)
        } else {
            VaultDatabase.setVaultMeta(key: "password_hint", value: "")
        }

        // Update keychain if Touch ID enabled
        if isTouchIDEnabled {
            try? KeychainHelper.saveMasterPassword(newPassword)
        }

        items = decryptedItems
    }

    // MARK: - Reset Vault

    func resetVault() {
        autoLockTimer?.invalidate()
        autoLockTimer = nil
        derivedKeyBase64 = nil
        items = []
        KeychainHelper.deleteMasterPassword()
        isTouchIDEnabled = false
        VaultDatabase.deleteAllVaultData()
        state = .notSetup
    }

    // MARK: - Password Hint

    func setPasswordHint(_ hint: String) {
        if hint.isEmpty {
            VaultDatabase.setVaultMeta(key: "password_hint", value: "")
        } else {
            VaultDatabase.setVaultMeta(key: "password_hint", value: hint)
        }
    }

    // MARK: - Item Management

    func addItem(_ item: VaultItem) throws {
        guard state == .unlocked, derivedKeyBase64 != nil else { throw VaultError.locked }

        let now = Date()
        let encFields = try encryptFields(item.fields)

        VaultDatabase.insertVaultItem(
            id: item.id,
            title: item.title,
            category: item.category.rawValue,
            fieldsEncrypted: encFields,
            notes: item.notes,
            createdAt: Int64(now.timeIntervalSince1970),
            updatedAt: Int64(now.timeIntervalSince1970)
        )

        var saved = item
        saved.createdAt = now
        saved.updatedAt = now
        items.append(saved)
        setupAutoLockTimer()
    }

    func updateItem(_ item: VaultItem) throws {
        guard state == .unlocked, derivedKeyBase64 != nil else { throw VaultError.locked }

        let now = Date()
        let encFields = try encryptFields(item.fields)

        VaultDatabase.updateVaultItem(
            id: item.id,
            title: item.title,
            category: item.category.rawValue,
            fieldsEncrypted: encFields,
            notes: item.notes,
            updatedAt: Int64(now.timeIntervalSince1970)
        )

        if let index = items.firstIndex(where: { $0.id == item.id }) {
            var updated = item
            updated.updatedAt = now
            items[index] = updated
        }
        setupAutoLockTimer()
    }

    func deleteItem(id: String) throws {
        guard state == .unlocked else { throw VaultError.locked }

        VaultDatabase.deleteVaultItem(id: id)
        items.removeAll { $0.id == id }
        setupAutoLockTimer()
    }

    // MARK: - Fields Encryption

    private func encryptFields(_ fields: [VaultField]) throws -> String {
        guard let key = derivedKeyBase64 else { throw VaultError.locked }

        do {
            return try encryptVaultFields(
                keyBase64: key,
                fields: fields.map { field in
                    VaultFieldInfo(
                        id: field.id.uuidString,
                        label: field.label,
                        value: field.value,
                        isSensitive: field.isSensitive
                    )
                }
            )
        } catch {
            throw VaultError.encryptionFailed
        }
    }

    private func decryptFields(_ ciphertext: String) throws -> [VaultField] {
        guard let key = derivedKeyBase64 else { throw VaultError.locked }

        do {
            return try decryptVaultFields(keyBase64: key, ciphertextBase64: ciphertext).map { field in
                guard let id = UUID(uuidString: field.id) else {
                    throw VaultError.decryptionFailed
                }

                return VaultField(
                    id: id,
                    label: field.label,
                    value: field.value,
                    isSensitive: field.isSensitive
                )
            }
        } catch {
            throw VaultError.decryptionFailed
        }
    }

    // MARK: - Migration from old credentials

    /// Migrate rows that were copied from the old credentials table by DatabaseManager.
    /// Those rows have fields_encrypted = "usernameEnc\npasswordEnc" (raw encrypted values from old schema).
    /// We decrypt them with the current key, rebuild as VaultField array, and re-encrypt.
    private func migrateOldCredentials() throws {
        let rows = VaultDatabase.fetchAllVaultItems()
        for row in rows {
            guard let id = row["id"] as? String,
                  let fieldsEnc = row["fields_encrypted"] as? String else { continue }

            // Migration marker: old rows contain a newline separator between username_encrypted and password_encrypted
            // Normal vault_items have base64 AES-GCM data which never contains newlines
            let parts = fieldsEnc.split(separator: "\n", maxSplits: 1)
            guard parts.count == 2 else { continue }

            let usernameEnc = String(parts[0])
            let passwordEnc = String(parts[1])

            let username = usernameEnc.isEmpty ? "" : (try? decrypt(usernameEnc)) ?? ""
            let password = (try? decrypt(passwordEnc)) ?? ""

            let title = row["title"] as? String ?? ""
            let notes = row["notes"] as? String
            let updatedAt = row["updated_at"] as? Int64 ?? Int64(Date().timeIntervalSince1970)

            let fields: [VaultField] = [
                VaultField(label: "URL", value: title, isSensitive: false),
                VaultField(label: "Username", value: username, isSensitive: false),
                VaultField(label: "Password", value: password, isSensitive: true),
            ]

            let newEncFields = try encryptFields(fields)
            VaultDatabase.updateVaultItem(
                id: id,
                title: title,
                category: VaultItemCategory.login.rawValue,
                fieldsEncrypted: newEncFields,
                notes: notes,
                updatedAt: updatedAt
            )
        }
    }

    // MARK: - Crypto

    private nonisolated static func deriveKeyStatic(password: String, saltBase64: String) throws -> String {
        do {
            return try deriveVaultKeyBase64(password: password, saltBase64: saltBase64)
        } catch {
            throw VaultError.keyDerivationFailed
        }
    }

    private func encrypt(_ plaintext: String) throws -> String {
        guard let key = derivedKeyBase64 else { throw VaultError.locked }

        do {
            return try encryptVaultString(keyBase64: key, plaintext: plaintext)
        } catch {
            throw VaultError.encryptionFailed
        }
    }

    private func decrypt(_ ciphertext: String) throws -> String {
        guard let key = derivedKeyBase64 else { throw VaultError.locked }

        do {
            return try decryptVaultString(keyBase64: key, ciphertextBase64: ciphertext)
        } catch {
            throw VaultError.decryptionFailed
        }
    }

    private func loadItems() throws {
        items = try loadAndReturnItems()
    }

    private func loadAndReturnItems() throws -> [VaultItem] {
        let rows = VaultDatabase.fetchAllVaultItems()
        return try rows.compactMap { row -> VaultItem? in
            guard let id = row["id"] as? String,
                  let title = row["title"] as? String,
                  let categoryRaw = row["category"] as? String,
                  let fieldsEnc = row["fields_encrypted"] as? String,
                  let createdAt = row["created_at"] as? Int64,
                  let updatedAt = row["updated_at"] as? Int64 else {
                return nil
            }

            let category = VaultItemCategory(rawValue: categoryRaw) ?? .login
            let fields = try decryptFields(fieldsEnc)
            let notes = row["notes"] as? String

            return VaultItem(
                id: id,
                title: title,
                category: category,
                fields: fields,
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
