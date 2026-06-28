import Foundation

/// Source type for AskAway apps
enum AppSourceType: String, Codable {
    /// Downloaded from network (stored in app sandbox)
    case downloaded
    /// Local development directory (external path)
    case localDev
}

/// Represents an installed AskAway app
/// Used for askw:// URL scheme navigation
struct AskAwayApp: Identifiable, Codable, Equatable, Hashable {
    let id: UUID
    var name: String
    var bundleId: String           // Unique app identifier (e.g., "com.example.myapp")
    var version: String?
    var description: String?
    var sourceType: AppSourceType
    var sourcePath: String         // Path to .actr file or dev directory
    var icon: String?              // SF Symbol name or custom icon
    var installedAt: Date
    var lastUsedAt: Date?
    var isPinned: Bool             // Pinned apps show in toolbar/dock

    init(
        id: UUID = UUID(),
        name: String,
        bundleId: String,
        version: String? = nil,
        description: String? = nil,
        sourceType: AppSourceType,
        sourcePath: String,
        icon: String? = nil,
        installedAt: Date = Date(),
        lastUsedAt: Date? = nil,
        isPinned: Bool = false
    ) {
        self.id = id
        self.name = name
        self.bundleId = bundleId
        self.version = version
        self.description = description
        self.sourceType = sourceType
        self.sourcePath = sourcePath
        self.icon = icon
        self.installedAt = installedAt
        self.lastUsedAt = lastUsedAt
        self.isPinned = isPinned
    }

    static func == (lhs: AskAwayApp, rhs: AskAwayApp) -> Bool {
        lhs.id == rhs.id
    }

    func hash(into hasher: inout Hasher) {
        hasher.combine(id)
    }

    /// The askw:// URL for this app
    var askwURL: URL {
        URL(string: "askw://\(bundleId)")!
    }

    /// The source URL (file path) for loading this app
    var sourceURL: URL {
        URL(fileURLWithPath: sourcePath)
    }
}

// MARK: - Display Helpers

extension AskAwayApp {
    /// SF Symbol name for display
    var displayIcon: String {
        icon ?? "app.fill"
    }

    /// Formatted installation date
    var formattedInstalledAt: String {
        let formatter = DateFormatter()
        formatter.dateStyle = .medium
        return formatter.string(from: installedAt)
    }

    /// Formatted last used date
    var formattedLastUsed: String? {
        guard let lastUsedAt = lastUsedAt else { return nil }
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .abbreviated
        return formatter.localizedString(for: lastUsedAt, relativeTo: Date())
    }

    /// Human-readable source type
    var sourceTypeDisplay: String {
        switch sourceType {
        case .downloaded: return "Downloaded"
        case .localDev: return "Local Development"
        }
    }
}
