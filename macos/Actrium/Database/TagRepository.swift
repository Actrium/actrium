import Foundation
import ActriumCore

/// Repository for tags using native SQLite3
final class TagRepository {
    private let db = DatabaseManager.shared
    private let sql = coreTagRepositorySql()

    // MARK: - CRUD Operations

    /// Get all tags
    func getAllTags() -> [Tag] {
        let rows = db.query(sql.selectAll)
        return rows.map { mapRowToTag($0) }
    }

    /// Get user-created (non-system) tags only
    func getUserTags() -> [Tag] {
        let rows = db.query(sql.selectUser)
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
            sql.insertTag,
            coreParams: coreTagInsertParams(
                id: newTag.id.uuidString,
                name: name,
                color: color,
                isSystem: false
            )
        )

        return success ? newTag : nil
    }

    /// Update a tag
    func updateTag(_ tag: Tag) {
        guard !tag.isSystem else { return }

        db.execute(
            sql.updateTag,
            coreParams: coreTagUpdateParams(
                name: tag.name,
                color: tag.color,
                id: tag.id.uuidString
            )
        )
    }

    /// Delete a tag (only non-system tags)
    func deleteTag(id: UUID) {
        db.execute(
            sql.deleteUserTagById,
            params: [id.uuidString]
        )
    }

    /// Get tags for a specific bookmark
    func getTagsForBookmark(bookmarkId: UUID) -> [Tag] {
        let rows = db.query(
            sql.selectByBookmarkId,
            params: [bookmarkId.uuidString]
        )
        return rows.map { mapRowToTag($0) }
    }

    /// Set tags for a bookmark (replaces existing)
    func setTagsForBookmark(bookmarkId: UUID, tagIds: [UUID]) {
        // Remove existing associations
        db.execute(
            sql.deleteBookmarkTagsByBookmarkId,
            params: [bookmarkId.uuidString]
        )

        // Add new associations
        for tagId in tagIds {
            db.execute(
                sql.insertBookmarkTag,
                params: [bookmarkId.uuidString, tagId.uuidString]
            )
        }
    }

    /// Add a single tag to a bookmark
    func addTagToBookmark(bookmarkId: UUID, tagId: UUID) {
        db.execute(
            sql.insertOrIgnoreBookmarkTag,
            params: [bookmarkId.uuidString, tagId.uuidString]
        )
    }

    /// Remove a tag from a bookmark
    func removeTagFromBookmark(bookmarkId: UUID, tagId: UUID) {
        db.execute(
            sql.deleteBookmarkTag,
            params: [bookmarkId.uuidString, tagId.uuidString]
        )
    }

    // MARK: - Helpers

    private func mapRowToTag(_ row: [String: Any]) -> Tag {
        let tag = coreTagFromRow(row: CoreTagRowInfo(
            id: row["id"] as? String ?? "",
            name: row["name"] as? String ?? "",
            color: row["color"] as? String,
            isSystemValue: row["is_system"] as? Int64 ?? 0
        ))

        return Tag(
            id: UUID(uuidString: tag.id) ?? UUID(),
            name: tag.name,
            color: tag.color,
            isSystem: tag.isSystem
        )
    }
}
