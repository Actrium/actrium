import Foundation

struct VaultDatabase {
    private static var db: DatabaseManager { DatabaseManager.shared }

    // MARK: - Vault Meta

    static func getVaultMeta(key: String) -> String? {
        let rows = db.query("SELECT value FROM vault_meta WHERE key = ?", params: [key])
        return rows.first?["value"] as? String
    }

    static func setVaultMeta(key: String, value: String) {
        db.execute(
            "INSERT OR REPLACE INTO vault_meta (key, value) VALUES (?, ?)",
            params: [key, value]
        )
    }

    // MARK: - Credentials CRUD

    static func insertCredential(
        id: String,
        domain: String,
        usernameEncrypted: String,
        passwordEncrypted: String,
        notes: String?,
        createdAt: Int64,
        updatedAt: Int64
    ) {
        db.execute(
            """
            INSERT INTO credentials (id, domain, username_encrypted, password_encrypted, notes, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            """,
            params: [id, domain, usernameEncrypted, passwordEncrypted, notes, createdAt, updatedAt]
        )
    }

    static func updateCredential(
        id: String,
        domain: String,
        usernameEncrypted: String,
        passwordEncrypted: String,
        notes: String?,
        updatedAt: Int64
    ) {
        db.execute(
            """
            UPDATE credentials SET domain = ?, username_encrypted = ?, password_encrypted = ?, notes = ?, updated_at = ?
            WHERE id = ?
            """,
            params: [domain, usernameEncrypted, passwordEncrypted, notes, updatedAt, id]
        )
    }

    static func deleteCredential(id: String) {
        db.execute("DELETE FROM credentials WHERE id = ?", params: [id])
    }

    static func fetchAllCredentials() -> [[String: Any]] {
        return db.query("SELECT * FROM credentials ORDER BY domain ASC")
    }

    static func fetchCredentials(domain: String) -> [[String: Any]] {
        return db.query("SELECT * FROM credentials WHERE domain = ?", params: [domain])
    }
}
