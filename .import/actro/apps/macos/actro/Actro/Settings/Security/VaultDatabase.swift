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

    // MARK: - Vault Items CRUD

    static func insertVaultItem(
        id: String,
        title: String,
        category: String,
        fieldsEncrypted: String,
        notes: String?,
        createdAt: Int64,
        updatedAt: Int64
    ) {
        db.execute(
            """
            INSERT INTO vault_items (id, title, category, fields_encrypted, notes, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            """,
            params: [id, title, category, fieldsEncrypted, notes, createdAt, updatedAt]
        )
    }

    static func updateVaultItem(
        id: String,
        title: String,
        category: String,
        fieldsEncrypted: String,
        notes: String?,
        updatedAt: Int64
    ) {
        db.execute(
            """
            UPDATE vault_items SET title = ?, category = ?, fields_encrypted = ?, notes = ?, updated_at = ?
            WHERE id = ?
            """,
            params: [title, category, fieldsEncrypted, notes, updatedAt, id]
        )
    }

    static func deleteVaultItem(id: String) {
        db.execute("DELETE FROM vault_items WHERE id = ?", params: [id])
    }

    static func fetchAllVaultItems() -> [[String: Any]] {
        return db.query("SELECT * FROM vault_items ORDER BY updated_at DESC")
    }

    static func fetchAllVaultMeta() -> [[String: Any]] {
        return db.query("SELECT key, value FROM vault_meta")
    }

    static func deleteAllVaultData() {
        db.execute("DELETE FROM vault_meta")
        db.execute("DELETE FROM vault_items")
    }
}
