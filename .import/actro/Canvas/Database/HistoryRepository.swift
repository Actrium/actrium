import Foundation

private let logger = AppLogger.database

/// Repository for history records using native SQLite3
final class HistoryRepository {
    private let db = DatabaseManager.shared

    // MARK: - CRUD Operations

    /// Add a new history entry
    func addVisit(id: UUID, title: String, appURL: URL?, appId: String?) {
        db.execute(
            "INSERT INTO history (id, title, app_url, app_id, visit_time) VALUES (?, ?, ?, ?, ?)",
            params: [
                id.uuidString,
                title,
                appURL?.absoluteString,
                appId,
                Int64(Date().timeIntervalSince1970)
            ]
        )
    }

    /// Get recent history items
    func getRecent(limit: Int = 10) -> [HistoryItem] {
        let rows = db.query(
            "SELECT * FROM history ORDER BY visit_time DESC LIMIT ?",
            params: [limit]
        )
        return rows.map { mapRowToHistoryItem($0) }
    }

    /// Get all history items
    func getAll() -> [HistoryItem] {
        let rows = db.query("SELECT * FROM history ORDER BY visit_time DESC")
        return rows.map { mapRowToHistoryItem($0) }
    }

    /// Search history by title
    func search(query: String) -> [HistoryItem] {
        guard !query.isEmpty else { return getAll() }

        let rows = db.query(
            "SELECT * FROM history WHERE LOWER(title) LIKE ? ORDER BY visit_time DESC",
            params: ["%\(query.lowercased())%"]
        )
        return rows.map { mapRowToHistoryItem($0) }
    }

    /// Delete a single history item
    func deleteItem(id: UUID) {
        db.execute("DELETE FROM history WHERE id = ?", params: [id.uuidString])
    }

    /// Clear all history
    func clearAll() {
        db.execute("DELETE FROM history")
        logger.notice("Cleared all history")
    }

    // MARK: - Helpers

    private func mapRowToHistoryItem(_ row: [String: Any]) -> HistoryItem {
        let idString = row["id"] as? String ?? ""
        let title = row["title"] as? String ?? ""
        let appUrlString = row["app_url"] as? String
        let appId = row["app_id"] as? String
        let visitTimeInt = row["visit_time"] as? Int64 ?? 0

        return HistoryItem(
            id: UUID(uuidString: idString) ?? UUID(),
            title: title,
            appURL: appUrlString.flatMap { URL(string: $0) },
            appId: appId,
            visitTime: Date(timeIntervalSince1970: TimeInterval(visitTimeInt))
        )
    }
}
