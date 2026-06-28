import Foundation

/// Repository for bookmarks using native SQLite3
final class BookmarkRepository {
    private let db = DatabaseManager.shared
    private let tagRepo = TagRepository()

    // MARK: - CRUD Operations

    /// Add a new bookmark (skips if URL is nil or already bookmarked)
    func add(_ bookmark: Bookmark) {
        guard let url = bookmark.appURL else { return }
        if isBookmarked(appURL: url) { return }

        db.execute(
            """
            INSERT INTO bookmarks (id, title, app_url, app_id, created_at, last_used_at, manufacturer, notes, favicon)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            params: [
                bookmark.id.uuidString,
                bookmark.title,
                bookmark.appURL?.absoluteString,
                bookmark.appId,
                Int64(bookmark.createdAt.timeIntervalSince1970),
                bookmark.lastUsedAt.map { Int64($0.timeIntervalSince1970) },
                bookmark.manufacturer,
                bookmark.notes,
                bookmark.favicon
            ]
        )

        // Set tags
        let tagIds = bookmark.tags.map { $0.id }
        tagRepo.setTagsForBookmark(bookmarkId: bookmark.id, tagIds: tagIds)
    }

    /// Update an existing bookmark
    func update(_ bookmark: Bookmark) {
        db.execute(
            """
            UPDATE bookmarks SET title = ?, app_url = ?, app_id = ?, last_used_at = ?,
            manufacturer = ?, notes = ?, favicon = ? WHERE id = ?
            """,
            params: [
                bookmark.title,
                bookmark.appURL?.absoluteString,
                bookmark.appId,
                bookmark.lastUsedAt.map { Int64($0.timeIntervalSince1970) },
                bookmark.manufacturer,
                bookmark.notes,
                bookmark.favicon,
                bookmark.id.uuidString
            ]
        )

        // Update tags
        let tagIds = bookmark.tags.map { $0.id }
        tagRepo.setTagsForBookmark(bookmarkId: bookmark.id, tagIds: tagIds)
    }

    /// Delete a bookmark
    func delete(id: UUID) {
        db.execute("DELETE FROM bookmarks WHERE id = ?", params: [id.uuidString])
        // Note: bookmark_tags entries are deleted via CASCADE
    }

    /// Get all bookmarks
    func getAll() -> [Bookmark] {
        let rows = db.query("SELECT * FROM bookmarks ORDER BY title ASC")
        return rows.map { row in
            let bookmarkId = UUID(uuidString: row["id"] as? String ?? "") ?? UUID()
            let tags = tagRepo.getTagsForBookmark(bookmarkId: bookmarkId)
            return mapRowToBookmark(row, tags: tags)
        }
    }

    /// Get bookmarks filtered by tag
    func getByTag(_ tag: Tag) -> [Bookmark] {
        let rows = db.query(
            """
            SELECT b.* FROM bookmarks b
            INNER JOIN bookmark_tags bt ON b.id = bt.bookmark_id
            WHERE bt.tag_id = ?
            ORDER BY b.title ASC
            """,
            params: [tag.id.uuidString]
        )
        return rows.map { row in
            let bookmarkId = UUID(uuidString: row["id"] as? String ?? "") ?? UUID()
            let tags = tagRepo.getTagsForBookmark(bookmarkId: bookmarkId)
            return mapRowToBookmark(row, tags: tags)
        }
    }

    /// Search bookmarks by title or notes
    func search(query: String) -> [Bookmark] {
        guard !query.isEmpty else { return getAll() }

        let rows = db.query(
            """
            SELECT * FROM bookmarks
            WHERE LOWER(title) LIKE ? OR LOWER(notes) LIKE ?
            ORDER BY title ASC
            """,
            params: ["%\(query.lowercased())%", "%\(query.lowercased())%"]
        )
        return rows.map { row in
            let bookmarkId = UUID(uuidString: row["id"] as? String ?? "") ?? UUID()
            let tags = tagRepo.getTagsForBookmark(bookmarkId: bookmarkId)
            return mapRowToBookmark(row, tags: tags)
        }
    }

    /// Check if an app URL is bookmarked
    func isBookmarked(appURL: URL) -> Bool {
        let count = db.queryScalar(
            "SELECT COUNT(*) FROM bookmarks WHERE app_url = ?",
            params: [appURL.absoluteString]
        ) as? Int64 ?? 0
        return count > 0
    }

    /// Get bookmark for a specific app URL
    func getBookmark(for appURL: URL) -> Bookmark? {
        let rows = db.query(
            "SELECT * FROM bookmarks WHERE app_url = ? LIMIT 1",
            params: [appURL.absoluteString]
        )

        guard let row = rows.first else { return nil }

        let bookmarkId = UUID(uuidString: row["id"] as? String ?? "") ?? UUID()
        let tags = tagRepo.getTagsForBookmark(bookmarkId: bookmarkId)
        return mapRowToBookmark(row, tags: tags)
    }

    /// Update last used time for a bookmark
    func updateLastUsed(id: UUID) {
        db.execute(
            "UPDATE bookmarks SET last_used_at = ? WHERE id = ?",
            params: [Int64(Date().timeIntervalSince1970), id.uuidString]
        )
    }

    /// Remove duplicate bookmarks (keep the oldest per app_url) and drop NULL-URL entries
    func deduplicateByURL() {
        db.execute("DELETE FROM bookmarks WHERE app_url IS NULL")
        db.execute(
            """
            DELETE FROM bookmarks WHERE id NOT IN (
                SELECT MIN(id) FROM bookmarks GROUP BY app_url
            )
            """
        )
    }

    /// Clear all bookmarks (including tag associations)
    func clearAll() {
        db.execute("DELETE FROM bookmark_tags")
        db.execute("DELETE FROM bookmarks")
        db.execute("VACUUM")
    }

    // MARK: - Helpers

    private func mapRowToBookmark(_ row: [String: Any], tags: [Tag]) -> Bookmark {
        let idString = row["id"] as? String ?? ""
        let title = row["title"] as? String ?? ""
        let appUrlString = row["app_url"] as? String
        let appId = row["app_id"] as? String
        let createdAtInt = row["created_at"] as? Int64 ?? 0
        let lastUsedAtInt = row["last_used_at"] as? Int64
        let manufacturer = row["manufacturer"] as? String
        let notes = row["notes"] as? String
        let favicon = row["favicon"] as? String

        return Bookmark(
            id: UUID(uuidString: idString) ?? UUID(),
            title: title,
            appURL: appUrlString.flatMap { URL(string: $0) },
            appId: appId,
            createdAt: Date(timeIntervalSince1970: TimeInterval(createdAtInt)),
            lastUsedAt: lastUsedAtInt.map { Date(timeIntervalSince1970: TimeInterval($0)) },
            manufacturer: manufacturer,
            notes: notes,
            favicon: favicon,
            tags: tags
        )
    }
}
