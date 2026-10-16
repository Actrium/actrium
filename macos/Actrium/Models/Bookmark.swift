import Foundation

/// Represents a bookmarked app
struct Bookmark: Identifiable, Codable, Equatable, Hashable {
    let id: UUID
    var title: String
    var appURL: URL?            // Path to .actr file
    var appId: String?          // App identifier from ACTR package
    var createdAt: Date
    var lastUsedAt: Date?
    var publisher: String?   // App publisher/developer
    var notes: String?          // User notes
    var favicon: String?        // SF Symbol name or custom icon
    var tags: [Tag]             // Associated tags

    init(
        id: UUID = UUID(),
        title: String,
        appURL: URL? = nil,
        appId: String? = nil,
        createdAt: Date = Date(),
        lastUsedAt: Date? = nil,
        publisher: String? = nil,
        notes: String? = nil,
        favicon: String? = nil,
        tags: [Tag] = []
    ) {
        self.id = id
        self.title = title
        self.appURL = appURL
        self.appId = appId
        self.createdAt = createdAt
        self.lastUsedAt = lastUsedAt
        self.publisher = publisher
        self.notes = notes
        self.favicon = favicon
        self.tags = tags
    }

    static func == (lhs: Bookmark, rhs: Bookmark) -> Bool {
        lhs.id == rhs.id
    }

    func hash(into hasher: inout Hasher) {
        hasher.combine(id)
    }
}

// MARK: - Factory Methods

extension Bookmark {
    /// Create a bookmark from a Tab
    static func from(tab: Tab, publisher: String? = nil) -> Bookmark {
        Bookmark(
            title: tab.title.isEmpty ? "Untitled" : tab.title,
            appURL: tab.url,
            appId: tab.resolvedAppId,
            publisher: publisher,
            favicon: "star.fill" // Default icon
        )
    }
}

// MARK: - Display Helpers

extension Bookmark {
    /// SF Symbol name for display
    var displayIcon: String {
        favicon ?? "bookmark.fill"
    }

    /// Formatted creation date
    var formattedCreatedAt: String {
        let formatter = DateFormatter()
        formatter.dateStyle = .medium
        return formatter.string(from: createdAt)
    }

    /// Formatted last used date
    var formattedLastUsed: String? {
        guard let lastUsedAt = lastUsedAt else { return nil }
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .abbreviated
        return formatter.localizedString(for: lastUsedAt, relativeTo: Date())
    }

    /// Non-system tags only
    var userTags: [Tag] {
        tags.filter { !$0.isSystem }
    }
}
