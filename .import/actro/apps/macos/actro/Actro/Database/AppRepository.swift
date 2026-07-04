import Foundation
import ActroCore

/// Repository for Actro apps using native SQLite3
final class AppRepository {
    private let db = DatabaseManager.shared
    private let sql = coreAppRepositorySql()

    // MARK: - CRUD Operations

    /// Add a new app
    func add(_ app: InstalledActroApp) {
        guard let params = try? coreInstalledAppInsertParams(app: coreInstalledAppInfo(from: app)) else {
            return
        }

        db.execute(
            sql.insertOrReplaceApp,
            coreParams: params
        )
    }

    /// Update an existing app
    func update(_ app: InstalledActroApp) {
        guard let params = try? coreInstalledAppUpdateParams(app: coreInstalledAppInfo(from: app)) else {
            return
        }

        db.execute(
            sql.updateApp,
            coreParams: params
        )
    }

    /// Delete an app
    func delete(id: UUID) {
        db.execute(sql.deleteById, params: [id.uuidString])
    }

    /// Delete an app by bundle ID
    func delete(bundleId: String) {
        db.execute(sql.deleteByBundleId, params: [bundleId])
    }

    /// Get all apps
    func getAll() -> [InstalledActroApp] {
        let rows = db.query(sql.selectAll)
        return rows.compactMap { mapRowToApp($0) }
    }

    /// Get apps by source type
    func getBySourceType(_ sourceType: AppSourceType) -> [InstalledActroApp] {
        let rows = db.query(
            sql.selectBySourceType,
            params: [sourceType.rawValue]
        )
        return rows.compactMap { mapRowToApp($0) }
    }

    /// Get pinned apps
    func getPinned() -> [InstalledActroApp] {
        let rows = db.query(sql.selectPinned)
        return rows.compactMap { mapRowToApp($0) }
    }

    /// Get app by bundle ID
    func getByBundleId(_ bundleId: String) -> InstalledActroApp? {
        let rows = db.query(
            sql.selectByBundleId,
            params: [bundleId]
        )
        return rows.first.flatMap { mapRowToApp($0) }
    }

    /// Get app by ID
    func getById(_ id: UUID) -> InstalledActroApp? {
        let rows = db.query(
            sql.selectById,
            params: [id.uuidString]
        )
        return rows.first.flatMap { mapRowToApp($0) }
    }

    /// Check if a bundle ID is registered
    func exists(bundleId: String) -> Bool {
        let count = db.queryScalar(
            sql.countByBundleId,
            params: [bundleId]
        ) as? Int64 ?? 0
        return count > 0
    }

    /// Update last used time for an app
    func updateLastUsed(bundleId: String) {
        db.execute(
            sql.updateLastUsedByBundleId,
            params: [Int64(Date().timeIntervalSince1970), bundleId]
        )
    }

    /// Toggle pinned status
    func togglePinned(id: UUID) {
        db.execute(
            sql.togglePinnedById,
            params: [id.uuidString]
        )
    }

    /// Search apps by name or description
    func search(query: String) -> [InstalledActroApp] {
        let searchParams = coreAppSearchParams(query: query)
        guard !searchParams.isEmpty else { return getAll() }

        let rows = db.query(
            sql.searchByNameDescriptionOrBundleId,
            params: searchParams
        )
        return rows.compactMap { mapRowToApp($0) }
    }

    /// Clear all apps
    func clearAll() {
        db.execute(sql.deleteAll)
        db.execute(sql.vacuum)
    }

    // MARK: - Helpers

    private func coreInstalledAppInfo(from app: InstalledActroApp) -> CoreInstalledAppInfo {
        CoreInstalledAppInfo(
            id: app.id.uuidString,
            name: app.name,
            bundleId: app.bundleId,
            version: app.version,
            description: app.description,
            sourceType: app.sourceType.rawValue,
            sourcePath: app.sourcePath,
            icon: app.icon,
            installedAtSeconds: Int64(app.installedAt.timeIntervalSince1970),
            lastUsedAtSeconds: app.lastUsedAt.map { Int64($0.timeIntervalSince1970) },
            isPinned: app.isPinned
        )
    }

    private func mapRowToApp(_ row: [String: Any]) -> InstalledActroApp? {
        guard let app = coreInstalledAppFromRow(row: CoreInstalledAppRowInfo(
            id: row["id"] as? String,
            name: row["name"] as? String,
            bundleId: row["bundle_id"] as? String,
            version: row["version"] as? String,
            description: row["description"] as? String,
            sourceType: row["source_type"] as? String,
            sourcePath: row["source_path"] as? String,
            icon: row["icon"] as? String,
            installedAtSeconds: row["installed_at"] as? Int64,
            lastUsedAtSeconds: row["last_used_at"] as? Int64,
            isPinnedValue: row["is_pinned"] as? Int64 ?? 0
        )),
              let id = UUID(uuidString: app.id),
              let sourceType = AppSourceType(coreRawValue: app.sourceType) else {
            return nil
        }

        return InstalledActroApp(
            id: id,
            name: app.name,
            bundleId: app.bundleId,
            version: app.version,
            description: app.description,
            sourceType: sourceType,
            sourcePath: app.sourcePath,
            icon: app.icon,
            installedAt: Date(timeIntervalSince1970: TimeInterval(app.installedAtSeconds)),
            lastUsedAt: app.lastUsedAtSeconds.map { Date(timeIntervalSince1970: TimeInterval($0)) },
            isPinned: app.isPinned
        )
    }
}
