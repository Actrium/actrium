import Foundation

/// Repository for tags using native SQLite3
final class TagRepository {
    private let db = DatabaseManager.shared

    // MARK: - CRUD Operations

    /// Get all tags
    func getAllTags() -> [Tag] {
        let rows = db.query("SELECT * FROM tags ORDER BY is_system DESC, name ASC")
        return rows.map { mapRowToTag($0) }
    }

    /// Get user-created (non-system) tags only
    func getUserTags() -> [Tag] {
        let rows = db.query("SELECT * FROM tags WHERE is_system = 0 ORDER BY name ASC")
        return rows.map { mapRowToTag($0) }
    }

    /// Create a new tag
    func createTag(name: String, color: String? = nil) -> Tag? {
        let newTag = Tag(
            id: UUID(),
            name: name,
            color: color,
            isSystem: false
        )

        let success = db.execute(
            "INSERT INTO tags (id, name, color, is_system) VALUES (?, ?, ?, ?)",
            params: [newTag.id.uuidString, name, color, 0]
        )

        return success ? newTag : nil
    }

    /// Update a tag
    func updateTag(_ tag: Tag) {
        guard !tag.isSystem else { return }

        db.execute(
            "UPDATE tags SET name = ?, color = ? WHERE id = ?",
            params: [tag.name, tag.color, tag.id.uuidString]
        )
    }

    /// Delete a tag (only non-system tags)
    func deleteTag(id: UUID) {
        db.execute(
            "DELETE FROM tags WHERE id = ? AND is_system = 0",
            params: [id.uuidString]
        )
    }

    /// Get tags for a specific bookmark
    func getTagsForBookmark(bookmarkId: UUID) -> [Tag] {
        let rows = db.query(
            """
            SELECT t.* FROM tags t
            INNER JOIN bookmark_tags bt ON t.id = bt.tag_id
            WHERE bt.bookmark_id = ?
            """,
            params: [bookmarkId.uuidString]
        )
        return rows.map { mapRowToTag($0) }
    }

    /// Set tags for a bookmark (replaces existing)
    func setTagsForBookmark(bookmarkId: UUID, tagIds: [UUID]) {
        // Remove existing associations
        db.execute(
            "DELETE FROM bookmark_tags WHERE bookmark_id = ?",
            params: [bookmarkId.uuidString]
        )

        // Add new associations
        for tagId in tagIds {
            db.execute(
                "INSERT INTO bookmark_tags (bookmark_id, tag_id) VALUES (?, ?)",
                params: [bookmarkId.uuidString, tagId.uuidString]
            )
        }
    }

    /// Add a single tag to a bookmark
    func addTagToBookmark(bookmarkId: UUID, tagId: UUID) {
        db.execute(
            "INSERT OR IGNORE INTO bookmark_tags (bookmark_id, tag_id) VALUES (?, ?)",
            params: [bookmarkId.uuidString, tagId.uuidString]
        )
    }

    /// Remove a tag from a bookmark
    func removeTagFromBookmark(bookmarkId: UUID, tagId: UUID) {
        db.execute(
            "DELETE FROM bookmark_tags WHERE bookmark_id = ? AND tag_id = ?",
            params: [bookmarkId.uuidString, tagId.uuidString]
        )
    }

    // MARK: - Helpers

    private func mapRowToTag(_ row: [String: Any]) -> Tag {
        let idString = row["id"] as? String ?? ""
        let name = row["name"] as? String ?? ""
        let color = row["color"] as? String
        let isSystemInt = row["is_system"] as? Int64 ?? 0

        return Tag(
            id: UUID(uuidString: idString) ?? UUID(),
            name: name,
            color: color,
            isSystem: isSystemInt != 0
        )
    }
}
