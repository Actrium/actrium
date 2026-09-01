import Foundation
import ActroCore

/// Repository for bookmarks using native SQLite3
final class BookmarkRepository {
    private let db = DatabaseManager.shared
    private let tagRepo = TagRepository()
    private let sql = coreBookmarkRepositorySql()

    // MARK: - CRUD Operations

    /// Add a new bookmark (skips if URL is nil or already bookmarked)
    func add(_ bookmark: Bookmark) {
        guard let url = bookmark.appURL else { return }
        if isBookmarked(appURL: url) { return }

        db.execute(
            sql.insertBookmark,
            coreParams: coreBookmarkInsertParams(bookmark: coreBookmarkInfo(from: bookmark))
        )

        // Set tags
        let tagIds = bookmark.tags.map { $0.id }
        tagRepo.setTagsForBookmark(bookmarkId: bookmark.id, tagIds: tagIds)
    }

    /// Update an existing bookmark
    func update(_ bookmark: Bookmark) {
        db.execute(
            sql.updateBookmark,
            coreParams: coreBookmarkUpdateParams(bookmark: coreBookmarkInfo(from: bookmark))
        )

        // Update tags
        let tagIds = bookmark.tags.map { $0.id }
        tagRepo.setTagsForBookmark(bookmarkId: bookmark.id, tagIds: tagIds)
    }

    /// Delete a bookmark
    func delete(id: UUID) {
        db.execute(sql.deleteById, params: [id.uuidString])
        // Note: bookmark_tags entries are deleted via CASCADE
    }

    /// Get all bookmarks
    func getAll() -> [Bookmark] {
        let rows = db.query(sql.selectAll)
        return rows.map { row in
            let bookmarkId = UUID(uuidString: row["id"] as? String ?? "") ?? UUID()
            let tags = tagRepo.getTagsForBookmark(bookmarkId: bookmarkId)
            return mapRowToBookmark(row, tags: tags)
        }
    }

    /// Get bookmarks filtered by tag
    func getByTag(_ tag: Tag) -> [Bookmark] {
        let rows = db.query(
            sql.selectByTag,
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
        let searchParams = coreBookmarkSearchParams(query: query)
        guard !searchParams.isEmpty else { return getAll() }

        let rows = db.query(
            sql.searchByTitleOrNotes,
            params: searchParams
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
            sql.countByAppUrl,
            params: [appURL.absoluteString]
        ) as? Int64 ?? 0
        return count > 0
    }

    /// Get bookmark for a specific app URL
    func getBookmark(for appURL: URL) -> Bookmark? {
        let rows = db.query(
            sql.selectByAppUrl,
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
            sql.updateLastUsed,
            params: [Int64(Date().timeIntervalSince1970), id.uuidString]
        )
    }

    /// Remove duplicate bookmarks (keep the oldest per app_url) and drop NULL-URL entries
    func deduplicateByURL() {
        db.execute(sql.deleteNullAppUrl)
        db.execute(sql.deleteDuplicatesByAppUrl)
    }

    /// Clear all bookmarks (including tag associations)
    func clearAll() {
        db.execute(sql.deleteBookmarkTags)
        db.execute(sql.deleteAll)
        db.execute(sql.vacuum)
    }

    // MARK: - Helpers

    private func coreBookmarkInfo(from bookmark: Bookmark) -> CoreBookmarkInfo {
        CoreBookmarkInfo(
            id: bookmark.id.uuidString,
            title: bookmark.title,
            appUrl: bookmark.appURL?.absoluteString,
            appId: bookmark.appId,
            createdAtSeconds: Int64(bookmark.createdAt.timeIntervalSince1970),
            lastUsedAtSeconds: bookmark.lastUsedAt.map { Int64($0.timeIntervalSince1970) },
            manufacturer: bookmark.manufacturer,
            notes: bookmark.notes,
            favicon: bookmark.favicon
        )
    }

    private func mapRowToBookmark(_ row: [String: Any], tags: [Tag]) -> Bookmark {
        let bookmark = coreBookmarkFromRow(row: CoreBookmarkRowInfo(
            id: row["id"] as? String ?? "",
            title: row["title"] as? String ?? "",
            appUrl: row["app_url"] as? String,
            appId: row["app_id"] as? String,
            createdAtSeconds: row["created_at"] as? Int64 ?? 0,
            lastUsedAtSeconds: row["last_used_at"] as? Int64,
            manufacturer: row["manufacturer"] as? String,
            notes: row["notes"] as? String,
            favicon: row["favicon"] as? String
        ))

        return Bookmark(
            id: UUID(uuidString: bookmark.id) ?? UUID(),
            title: bookmark.title,
            appURL: bookmark.appUrl.flatMap { URL(string: $0) },
            appId: bookmark.appId,
            createdAt: Date(timeIntervalSince1970: TimeInterval(bookmark.createdAtSeconds)),
            lastUsedAt: bookmark.lastUsedAtSeconds.map { Date(timeIntervalSince1970: TimeInterval($0)) },
            manufacturer: bookmark.manufacturer,
            notes: bookmark.notes,
            favicon: bookmark.favicon,
            tags: tags
        )
    }
}
