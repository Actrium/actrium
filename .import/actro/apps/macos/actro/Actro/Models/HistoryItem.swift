import Foundation

/// Represents a history entry for app usage
struct HistoryItem: Identifiable, Codable, Equatable {
    let id: UUID
    var title: String
    var appURL: URL?
    var appId: String?
    var visitTime: Date

    init(id: UUID = UUID(), title: String, appURL: URL? = nil, appId: String? = nil, visitTime: Date = Date()) {
        self.id = id
        self.title = title
        self.appURL = appURL
        self.appId = appId
        self.visitTime = visitTime
    }
}

// MARK: - Date Grouping Helpers

extension HistoryItem {
    /// Returns a human-readable date group label
    var dateGroupLabel: String {
        let calendar = Calendar.current
        let now = Date()

        if calendar.isDateInToday(visitTime) {
            return "Today"
        } else if calendar.isDateInYesterday(visitTime) {
            return "Yesterday"
        } else if let weekAgo = calendar.date(byAdding: .day, value: -7, to: now),
                  visitTime >= weekAgo {
            return "Last 7 Days"
        } else if let monthAgo = calendar.date(byAdding: .month, value: -1, to: now),
                  visitTime >= monthAgo {
            return "Last Month"
        } else {
            let formatter = DateFormatter()
            formatter.dateFormat = "MMMM yyyy"
            return formatter.string(from: visitTime)
        }
    }

    /// Formatted time string for display
    var formattedTime: String {
        let formatter = DateFormatter()
        if Calendar.current.isDateInToday(visitTime) {
            formatter.dateFormat = "h:mm a"
        } else {
            formatter.dateFormat = "MMM d, h:mm a"
        }
        return formatter.string(from: visitTime)
    }
}

// MARK: - Grouping

extension Array where Element == HistoryItem {
    /// Groups history items by date
    func groupedByDate() -> [(key: String, items: [HistoryItem])] {
        let grouped = Dictionary(grouping: self) { $0.dateGroupLabel }

        // Define sort order for groups
        let order = ["Today", "Yesterday", "Last 7 Days", "Last Month"]

        return grouped
            .map { (key: $0.key, items: $0.value.sorted { $0.visitTime > $1.visitTime }) }
            .sorted { first, second in
                let firstIndex = order.firstIndex(of: first.key) ?? Int.max
                let secondIndex = order.firstIndex(of: second.key) ?? Int.max

                if firstIndex != Int.max || secondIndex != Int.max {
                    return firstIndex < secondIndex
                }
                // For month-year groups, sort by most recent
                return first.items.first?.visitTime ?? .distantPast > second.items.first?.visitTime ?? .distantPast
            }
    }
}
