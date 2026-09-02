import Foundation
import ActriumCore

private let logger = AppLogger.database

/// Repository for history records using native SQLite3
final class HistoryRepository {
    private let db = DatabaseManager.shared
    private let sql = coreHistoryRepositorySql()

    // MARK: - CRUD Operations

    /// Add a new history entry
    func addVisit(id: UUID, title: String, appURL: URL?, appId: String?) {
        db.execute(
            sql.insertVisit,
            coreParams: coreHistoryInsertVisitParams(
                id: id.uuidString,
                title: title,
                appUrl: appURL?.absoluteString,
                appId: appId,
                visitTimeSeconds: Int64(Date().timeIntervalSince1970)
            )
        )
    }

    /// Get recent history items
    func getRecent(limit: Int = 10) -> [HistoryItem] {
        let rows = db.query(
            sql.selectRecent,
            params: [limit]
        )
        return rows.map { mapRowToHistoryItem($0) }
    }

    /// Get all history items
    func getAll() -> [HistoryItem] {
        let rows = db.query(sql.selectAll)
        return rows.map { mapRowToHistoryItem($0) }
    }

    /// Search history by title
    func search(query: String) -> [HistoryItem] {
        guard let searchParam = coreHistorySearchTitleParam(query: query) else { return getAll() }

        let rows = db.query(
            sql.searchByTitle,
            params: [searchParam]
        )
        return rows.map { mapRowToHistoryItem($0) }
    }

    /// Delete a single history item
    func deleteItem(id: UUID) {
        db.execute(sql.deleteById, params: [id.uuidString])
    }

    /// Clear all history
    func clearAll() {
        db.execute(sql.deleteAll)
        logger.notice("Cleared all history")
    }

    // MARK: - Helpers

    private func mapRowToHistoryItem(_ row: [String: Any]) -> HistoryItem {
        let item = coreHistoryItemFromRow(row: CoreHistoryItemRowInfo(
            id: row["id"] as? String ?? "",
            title: row["title"] as? String ?? "",
            appUrl: row["app_url"] as? String,
            appId: row["app_id"] as? String,
            visitTimeSeconds: row["visit_time"] as? Int64 ?? 0
        ))

        return HistoryItem(
            id: UUID(uuidString: item.id) ?? UUID(),
            title: item.title,
            appURL: item.appUrl.flatMap { URL(string: $0) },
            appId: item.appId,
            visitTime: Date(timeIntervalSince1970: TimeInterval(item.visitTimeSeconds))
        )
    }
}
