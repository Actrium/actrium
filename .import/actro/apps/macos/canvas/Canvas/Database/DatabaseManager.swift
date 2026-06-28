import Foundation
import SQLite3

private let logger = AppLogger.database

/// Manages SQLite database connection and schema using native SQLite3 API
final class DatabaseManager {
    static let shared = DatabaseManager()

    private var db: OpaquePointer?

    // MARK: - Initialization

    private init() {
        setupDatabase()
    }

    deinit {
        if let db = db {
            sqlite3_close(db)
        }
    }

    private func setupDatabase() {
        let path = getDatabasePath()

        if sqlite3_open(path, &db) != SQLITE_OK {
            logger.fault("Failed to open database at: \(path)")
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
        let appFolder = appSupport.appendingPathComponent("Canvas", isDirectory: true)

        // Create directory if needed
        if !fileManager.fileExists(atPath: appFolder.path) {
            try? fileManager.createDirectory(at: appFolder, withIntermediateDirectories: true)
        }

        return appFolder.appendingPathComponent("canvas.db").path
    }

    private func createTables() {
        // History table
        execute("""
            CREATE TABLE IF NOT EXISTS history (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                app_url TEXT,
                app_id TEXT,
                visit_time INTEGER NOT NULL
            )
        """)
        execute("CREATE INDEX IF NOT EXISTS idx_history_time ON history(visit_time DESC)")

        // Bookmarks table
        execute("""
            CREATE TABLE IF NOT EXISTS bookmarks (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                app_url TEXT,
                app_id TEXT,
                created_at INTEGER NOT NULL,
                last_used_at INTEGER,
                manufacturer TEXT,
                notes TEXT,
                favicon TEXT
            )
        """)

        // Tags table
        execute("""
            CREATE TABLE IF NOT EXISTS tags (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                color TEXT,
                is_system INTEGER DEFAULT 0
            )
        """)

        // Bookmark-Tags junction table
        execute("""
            CREATE TABLE IF NOT EXISTS bookmark_tags (
                bookmark_id TEXT NOT NULL,
                tag_id TEXT NOT NULL,
                PRIMARY KEY (bookmark_id, tag_id),
                FOREIGN KEY (bookmark_id) REFERENCES bookmarks(id) ON DELETE CASCADE,
                FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
            )
        """)
        execute("CREATE INDEX IF NOT EXISTS idx_bookmark_tags_tag ON bookmark_tags(tag_id)")

        // Apps table - stores AskAway apps (downloaded packages and local dev packages)
        execute("""
            CREATE TABLE IF NOT EXISTS apps (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                bundle_id TEXT NOT NULL UNIQUE,
                version TEXT,
                description TEXT,
                source_type TEXT NOT NULL,
                source_path TEXT NOT NULL,
                icon TEXT,
                installed_at INTEGER NOT NULL,
                last_used_at INTEGER,
                is_pinned INTEGER DEFAULT 0
            )
        """)
        execute("CREATE INDEX IF NOT EXISTS idx_apps_bundle_id ON apps(bundle_id)")
        execute("CREATE INDEX IF NOT EXISTS idx_apps_source_type ON apps(source_type)")

        // Vault metadata
        execute("""
            CREATE TABLE IF NOT EXISTS vault_meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )
        """)

        // Vault items
        execute("""
            CREATE TABLE IF NOT EXISTS vault_items (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                category TEXT NOT NULL DEFAULT 'login',
                fields_encrypted TEXT NOT NULL,
                notes TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )
        """)
        execute("CREATE INDEX IF NOT EXISTS idx_vault_items_category ON vault_items(category)")

        // Migrate old credentials table if it exists
        migrateCredentialsTable()
    }

    private func insertDefaultTags() {
        // Check if tags already exist
        guard let count = queryScalar("SELECT COUNT(*) FROM tags") as? Int64, count == 0 else {
            return
        }

        // System tags
        let systemTags: [(String, String, Bool)] = [
            ("App", "#007AFF", true),
            ("Manufacturer", "#5856D6", true),
            ("LastUsed", "#FF9500", true),
            ("Frequent", "#FF2D55", true)
        ]

        // Default user tags
        let userTags: [(String, String, Bool)] = [
            ("Work", "#34C759", false),
            ("Personal", "#AF52DE", false),
            ("Development", "#00C7BE", false),
            ("Entertainment", "#FF3B30", false)
        ]

        for (name, color, isSystem) in systemTags + userTags {
            execute(
                "INSERT INTO tags (id, name, color, is_system) VALUES (?, ?, ?, ?)",
                params: [UUID().uuidString, name, color, isSystem ? 1 : 0]
            )
        }

        logger.notice("Inserted default tags")
    }

    // MARK: - Query Helpers

    @discardableResult
    func execute(_ sql: String, params: [Any?] = []) -> Bool {
        guard let db = db else { return false }

        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK else {
            logger.error("Prepare failed: \(String(cString: sqlite3_errmsg(db)))")
            return false
        }

        defer { sqlite3_finalize(statement) }

        // Bind parameters
        for (index, param) in params.enumerated() {
            let idx = Int32(index + 1)
            if param == nil {
                sqlite3_bind_null(statement, idx)
            } else if let value = param as? String {
                sqlite3_bind_text(statement, idx, value, -1, unsafeBitCast(-1, to: sqlite3_destructor_type.self))
            } else if let value = param as? Int64 {
                sqlite3_bind_int64(statement, idx, value)
            } else if let value = param as? Int {
                sqlite3_bind_int64(statement, idx, Int64(value))
            } else if let value = param as? Double {
                sqlite3_bind_double(statement, idx, value)
            }
        }

        let result = sqlite3_step(statement)
        if result != SQLITE_DONE && result != SQLITE_ROW {
            logger.error("Execute failed: \(String(cString: sqlite3_errmsg(db)))")
            return false
        }

        return true
    }

    func query(_ sql: String, params: [Any?] = []) -> [[String: Any]] {
        guard let db = db else { return [] }

        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK else {
            logger.error("Prepare failed: \(String(cString: sqlite3_errmsg(db)))")
            return []
        }

        defer { sqlite3_finalize(statement) }

        // Bind parameters
        for (index, param) in params.enumerated() {
            let idx = Int32(index + 1)
            if param == nil {
                sqlite3_bind_null(statement, idx)
            } else if let value = param as? String {
                sqlite3_bind_text(statement, idx, value, -1, unsafeBitCast(-1, to: sqlite3_destructor_type.self))
            } else if let value = param as? Int64 {
                sqlite3_bind_int64(statement, idx, value)
            } else if let value = param as? Int {
                sqlite3_bind_int64(statement, idx, Int64(value))
            } else if let value = param as? Double {
                sqlite3_bind_double(statement, idx, value)
            }
        }

        var results: [[String: Any]] = []
        let columnCount = sqlite3_column_count(statement)

        while sqlite3_step(statement) == SQLITE_ROW {
            var row: [String: Any] = [:]
            for i in 0..<columnCount {
                let columnName = String(cString: sqlite3_column_name(statement, i))
                let type = sqlite3_column_type(statement, i)

                switch type {
                case SQLITE_INTEGER:
                    row[columnName] = sqlite3_column_int64(statement, i)
                case SQLITE_FLOAT:
                    row[columnName] = sqlite3_column_double(statement, i)
                case SQLITE_TEXT:
                    if let text = sqlite3_column_text(statement, i) {
                        row[columnName] = String(cString: text)
                    }
                case SQLITE_NULL:
                    row[columnName] = NSNull()
                default:
                    break
                }
            }
            results.append(row)
        }

        return results
    }

    func queryScalar(_ sql: String, params: [Any?] = []) -> Any? {
        let results = query(sql, params: params)
        return results.first?.values.first
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
