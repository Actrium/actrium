import Foundation
import ActroCore

private let logger = AppLogger.database

/// Manages the app database path and schema using shared Apple SQLite glue.
final class DatabaseManager {
    static let shared = DatabaseManager()

    private var db: AppleSQLiteDatabase?

    // MARK: - Initialization

    private init() {
        setupDatabase()
    }

    private func setupDatabase() {
        let path = getDatabasePath()

        do {
            db = try AppleSQLiteDatabase(path: path) { message in
                logger.error("\(message)")
            }
        } catch {
            logger.fault("Failed to open database at: \(path), error: \(String(describing: error))")
            return
        }

        // Enable foreign keys
        execute("PRAGMA foreign_keys = ON")

        createTables()
        insertDefaultTags()
        logger.notice("Initialized at: \(path)")
    }

    private func getDatabasePath() -> String {
        let fileManager = FileManager.default
        let appSupport = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let appFolder = appSupport.appendingPathComponent("Actro", isDirectory: true)

        // Create directory if needed
        if !fileManager.fileExists(atPath: appFolder.path) {
            try? fileManager.createDirectory(at: appFolder, withIntermediateDirectories: true)
        }

        return appFolder.appendingPathComponent("actro.db").path
    }

    private func createTables() {
        for statement in coreSchemaStatements() {
            execute(statement)
        }

        // Migrate old credentials table if it exists
        migrateCredentialsTable()
    }

    private func insertDefaultTags() {
        let tagSql = coreTagRepositorySql()
        // Check if tags already exist
        guard let count = queryScalar(tagSql.countAll) as? Int64, count == 0 else {
            return
        }

        for tag in defaultTags() {
            execute(
                tagSql.insertTag,
                coreParams: coreTagInsertParams(
                    id: UUID().uuidString,
                    name: tag.name,
                    color: tag.color,
                    isSystem: tag.isSystem
                )
            )
        }

        logger.notice("Inserted default tags")
    }

    // MARK: - Query Helpers

    @discardableResult
    func execute(_ sql: String, params: [Any?] = []) -> Bool {
        db?.execute(sql, params: params) ?? false
    }

    @discardableResult
    func execute(_ sql: String, coreParams: [CoreSqlValueInfo]) -> Bool {
        db?.execute(sql, coreParams: coreParams) ?? false
    }

    func query(_ sql: String, params: [Any?] = []) -> [[String: Any]] {
        db?.query(sql, params: params) ?? []
    }

    func queryScalar(_ sql: String, params: [Any?] = []) -> Any? {
        db?.queryScalar(sql, params: params)
    }

    // MARK: - Migration

    /// Migrate old credentials table to vault_items (fields stay encrypted as-is, re-encrypted by VaultManager on unlock)
    private func migrateCredentialsTable() {
        // Check if old table exists
        let tables = query("SELECT name FROM sqlite_master WHERE type='table' AND name='credentials'")
        guard !tables.isEmpty else { return }

        // Move raw rows: domain → title, encrypted fields → fields_encrypted as a placeholder
        // The actual field re-encryption happens in VaultManager.migrateOldCredentials() after unlock
        let rows = query("SELECT * FROM credentials")
        for row in rows {
            guard let id = row["id"] as? String,
                  let domain = row["domain"] as? String,
                  let passwordEnc = row["password_encrypted"] as? String,
                  let createdAt = row["created_at"] as? Int64,
                  let updatedAt = row["updated_at"] as? Int64 else { continue }

            let usernameEnc = row["username_encrypted"] as? String ?? ""
            let notes = row["notes"] as? String

            // Store a migration marker — VaultManager will decrypt and re-encrypt properly
            let migrationPayload = "\(usernameEnc)\n\(passwordEnc)"

            execute(
                """
                INSERT OR IGNORE INTO vault_items (id, title, category, fields_encrypted, notes, created_at, updated_at)
                VALUES (?, ?, 'login', ?, ?, ?, ?)
                """,
                params: [id, domain, migrationPayload, notes, createdAt, updatedAt]
            )
        }

        // Drop old table
        execute("DROP TABLE IF EXISTS credentials")
        logger.notice("Migrated credentials table to vault_items")
    }
}
