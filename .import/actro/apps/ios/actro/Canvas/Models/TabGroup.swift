import SwiftUI

/// Helper struct for organizing tabs with their group
struct TabGroupWithTabs: Identifiable {
    let id: UUID
    let group: TabGroup?
    let tabs: [Tab]

    init(group: TabGroup?, tabs: [Tab]) {
        self.id = group?.id ?? UUID()
        self.group = group
        self.tabs = tabs
    }
}

/// Represents a group of tabs (similar to Chrome Tab Groups)
struct TabGroup: Identifiable, Codable {
    let id: UUID
    var name: String
    var color: TabGroupColor
    var isCollapsed: Bool
    var createdAt: Date

    init(
        id: UUID = UUID(),
        name: String = "",
        color: TabGroupColor = .gray,
        isCollapsed: Bool = false,
        createdAt: Date = Date()
    ) {
        self.id = id
        self.name = name
        self.color = color
        self.isCollapsed = isCollapsed
        self.createdAt = createdAt
    }
}

/// Available colors for tab groups (9 colors)
enum TabGroupColor: String, Codable, CaseIterable {
    case gray
    case blue
    case red
    case yellow
    case green
    case pink
    case purple
    case cyan
    case orange

    var swiftUIColor: Color {
        switch self {
        case .gray:   return Color.gray
        case .blue:   return Color.blue
        case .red:    return Color.red
        case .yellow: return Color.yellow
        case .green:  return Color.green
        case .pink:   return Color.pink
        case .purple: return Color.purple
        case .cyan:   return Color.cyan
        case .orange: return Color.orange
        }
    }

    var displayName: String {
        switch self {
        case .gray:   return "Gray"
        case .blue:   return "Blue"
        case .red:    return "Red"
        case .yellow: return "Yellow"
        case .green:  return "Green"
        case .pink:   return "Pink"
        case .purple: return "Purple"
        case .cyan:   return "Cyan"
        case .orange: return "Orange"
        }
    }
}
