import Foundation

/// Repository for AskAway apps using native SQLite3
final class AppRepository {
    private let db = DatabaseManager.shared

    // MARK: - CRUD Operations

    /// Add a new app
    func add(_ app: AskAwayApp) {
        db.execute(
            """
            INSERT OR REPLACE INTO apps (id, name, bundle_id, version, description, source_type, source_path, icon, installed_at, last_used_at, is_pinned)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            params: [
                app.id.uuidString,
                app.name,
                app.bundleId,
                app.version,
                app.description,
                app.sourceType.rawValue,
                app.sourcePath,
                app.icon,
                Int64(app.installedAt.timeIntervalSince1970),
                app.lastUsedAt.map { Int64($0.timeIntervalSince1970) },
                app.isPinned ? 1 : 0
            ]
        )
    }

    /// Update an existing app
    func update(_ app: AskAwayApp) {
        db.execute(
            """
            UPDATE apps SET name = ?, version = ?, description = ?, source_path = ?, icon = ?, last_used_at = ?, is_pinned = ?
            WHERE id = ?
            """,
            params: [
                app.name,
                app.version,
                app.description,
                app.sourcePath,
                app.icon,
                app.lastUsedAt.map { Int64($0.timeIntervalSince1970) },
                app.isPinned ? 1 : 0,
                app.id.uuidString
            ]
        )
    }

    /// Delete an app
    func delete(id: UUID) {
        db.execute("DELETE FROM apps WHERE id = ?", params: [id.uuidString])
    }

    /// Delete an app by bundle ID
    func delete(bundleId: String) {
        db.execute("DELETE FROM apps WHERE bundle_id = ?", params: [bundleId])
    }

    /// Get all apps
    func getAll() -> [AskAwayApp] {
        let rows = db.query("SELECT * FROM apps ORDER BY name ASC")
        return rows.compactMap { mapRowToApp($0) }
    }

    /// Get apps by source type
    func getBySourceType(_ sourceType: AppSourceType) -> [AskAwayApp] {
        let rows = db.query(
            "SELECT * FROM apps WHERE source_type = ? ORDER BY name ASC",
            params: [sourceType.rawValue]
        )
        return rows.compactMap { mapRowToApp($0) }
    }

    /// Get pinned apps
    func getPinned() -> [AskAwayApp] {
        let rows = db.query("SELECT * FROM apps WHERE is_pinned = 1 ORDER BY name ASC")
        return rows.compactMap { mapRowToApp($0) }
    }

    /// Get app by bundle ID
    func getByBundleId(_ bundleId: String) -> AskAwayApp? {
        let rows = db.query(
            "SELECT * FROM apps WHERE bundle_id = ? LIMIT 1",
            params: [bundleId]
        )
        return rows.first.flatMap { mapRowToApp($0) }
    }

    /// Get app by ID
    func getById(_ id: UUID) -> AskAwayApp? {
        let rows = db.query(
            "SELECT * FROM apps WHERE id = ? LIMIT 1",
            params: [id.uuidString]
        )
        return rows.first.flatMap { mapRowToApp($0) }
    }

    /// Check if a bundle ID is registered
    func exists(bundleId: String) -> Bool {
        let count = db.queryScalar(
            "SELECT COUNT(*) FROM apps WHERE bundle_id = ?",
            params: [bundleId]
        ) as? Int64 ?? 0
        return count > 0
    }

    /// Update last used time for an app
    func updateLastUsed(bundleId: String) {
        db.execute(
            "UPDATE apps SET last_used_at = ? WHERE bundle_id = ?",
            params: [Int64(Date().timeIntervalSince1970), bundleId]
        )
    }

    /// Toggle pinned status
    func togglePinned(id: UUID) {
        db.execute(
            "UPDATE apps SET is_pinned = NOT is_pinned WHERE id = ?",
            params: [id.uuidString]
        )
    }

    /// Search apps by name or description
    func search(query: String) -> [AskAwayApp] {
        guard !query.isEmpty else { return getAll() }

        let rows = db.query(
            """
            SELECT * FROM apps
            WHERE LOWER(name) LIKE ? OR LOWER(description) LIKE ? OR LOWER(bundle_id) LIKE ?
            ORDER BY name ASC
            """,
            params: ["%\(query.lowercased())%", "%\(query.lowercased())%", "%\(query.lowercased())%"]
        )
        return rows.compactMap { mapRowToApp($0) }
    }

    /// Clear all apps
    func clearAll() {
        db.execute("DELETE FROM apps")
        db.execute("VACUUM")
    }

    // MARK: - Helpers

    private func mapRowToApp(_ row: [String: Any]) -> AskAwayApp? {
        guard let idString = row["id"] as? String,
              let id = UUID(uuidString: idString),
              let name = row["name"] as? String,
              let bundleId = row["bundle_id"] as? String,
              let sourceTypeString = row["source_type"] as? String,
              let sourceType = AppSourceType(rawValue: sourceTypeString),
              let sourcePath = row["source_path"] as? String,
              let installedAtInt = row["installed_at"] as? Int64 else {
            return nil
        }

        let version = row["version"] as? String
        let description = row["description"] as? String
        let icon = row["icon"] as? String
        let lastUsedAtInt = row["last_used_at"] as? Int64
        let isPinned = (row["is_pinned"] as? Int64 ?? 0) != 0

        return AskAwayApp(
            id: id,
            name: name,
            bundleId: bundleId,
            version: version,
            description: description,
            sourceType: sourceType,
            sourcePath: sourcePath,
            icon: icon,
            installedAt: Date(timeIntervalSince1970: TimeInterval(installedAtInt)),
            lastUsedAt: lastUsedAtInt.map { Date(timeIntervalSince1970: TimeInterval($0)) },
            isPinned: isPinned
        )
    }
}
