import Foundation

/// Represents a closed tab group in history (for batch restore)
struct HistoryGroup: Identifiable, Codable {
    let id: UUID
    var name: String
    var color: TabGroupColor
    var tabs: [HistoryGroupTab]
    var closedAt: Date

    init(
        id: UUID = UUID(),
        name: String,
        color: TabGroupColor,
        tabs: [HistoryGroupTab],
        closedAt: Date = Date()
    ) {
        self.id = id
        self.name = name
        self.color = color
        self.tabs = tabs
        self.closedAt = closedAt
    }

    /// Create from a TabGroup and its tabs
    static func from(group: TabGroup, tabs: [Tab]) -> HistoryGroup {
        let historyTabs = tabs.map { tab in
            HistoryGroupTab(
                title: tab.title,
                appURL: tab.url
            )
        }
        return HistoryGroup(
            name: group.name,
            color: group.color,
            tabs: historyTabs
        )
    }
}

/// Represents a tab within a history group
struct HistoryGroupTab: Identifiable, Codable {
    let id: UUID
    var title: String
    var appURL: URL?

    init(id: UUID = UUID(), title: String, appURL: URL? = nil) {
        self.id = id
        self.title = title
        self.appURL = appURL
    }
}

// MARK: - Date Grouping Helpers

extension HistoryGroup {
    /// Returns a human-readable date group label
    var dateGroupLabel: String {
        let calendar = Calendar.current
        let now = Date()

        if calendar.isDateInToday(closedAt) {
            return "Today"
        } else if calendar.isDateInYesterday(closedAt) {
            return "Yesterday"
        } else if let weekAgo = calendar.date(byAdding: .day, value: -7, to: now),
                  closedAt >= weekAgo {
            return "Last 7 Days"
        } else if let monthAgo = calendar.date(byAdding: .month, value: -1, to: now),
                  closedAt >= monthAgo {
            return "Last Month"
        } else {
            let formatter = DateFormatter()
            formatter.dateFormat = "MMMM yyyy"
            return formatter.string(from: closedAt)
        }
    }

    /// Formatted time string for display
    var formattedTime: String {
        let formatter = DateFormatter()
        if Calendar.current.isDateInToday(closedAt) {
            formatter.dateFormat = "h:mm a"
        } else {
            formatter.dateFormat = "MMM d, h:mm a"
        }
        return formatter.string(from: closedAt)
    }
}
