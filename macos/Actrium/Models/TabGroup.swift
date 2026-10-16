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

    var nsColor: NSColor {
        // Softer pastel colors matching the design
        switch self {
        case .gray:   return NSColor(red: 0.7, green: 0.7, blue: 0.7, alpha: 1.0)
        case .blue:   return NSColor(red: 0.55, green: 0.65, blue: 0.95, alpha: 1.0)
        case .red:    return NSColor(red: 0.95, green: 0.6, blue: 0.6, alpha: 1.0)
        case .yellow: return NSColor(red: 0.95, green: 0.85, blue: 0.5, alpha: 1.0)
        case .green:  return NSColor(red: 0.6, green: 0.85, blue: 0.6, alpha: 1.0)
        case .pink:   return NSColor(red: 0.95, green: 0.7, blue: 0.85, alpha: 1.0)
        case .purple: return NSColor(red: 0.75, green: 0.6, blue: 0.9, alpha: 1.0)
        case .cyan:   return NSColor(red: 0.5, green: 0.85, blue: 0.9, alpha: 1.0)
        case .orange: return NSColor(red: 0.95, green: 0.75, blue: 0.55, alpha: 1.0)
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
