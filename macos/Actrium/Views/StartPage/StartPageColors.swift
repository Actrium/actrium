import SwiftUI

/// Color palette matching the React Native StartPage implementation
enum StartPageColors {
    /// Predefined color pairs for consistent icon colors
    /// Matches the RN implementation's getColorFromString function
    static let colorPairs: [(background: Color, text: Color)] = [
        (Color(hex: "#f5f5f7")!, Color(hex: "#6b6b70")!),  // Gray
        (Color(hex: "#fde8e8")!, Color(hex: "#c53030")!),  // Red
        (Color(hex: "#e8f4fd")!, Color(hex: "#2b6cb0")!),  // Blue
        (Color(hex: "#e8fdf0")!, Color(hex: "#276749")!),  // Green
        (Color(hex: "#fdf8e8")!, Color(hex: "#975a16")!),  // Yellow
        (Color(hex: "#f3e8fd")!, Color(hex: "#6b46c1")!),  // Purple
        (Color(hex: "#fde8f3")!, Color(hex: "#b83280")!),  // Pink
        (Color(hex: "#e8fdfd")!, Color(hex: "#0987a0")!),  // Cyan
    ]

    /// Generate consistent colors based on string hash
    /// Uses the same algorithm as RN implementation for consistency
    static func colors(for string: String) -> (background: Color, text: Color) {
        var hash = 0
        for char in string.unicodeScalars {
            hash = Int(char.value) &+ ((hash << 5) &- hash)
        }
        let index = abs(hash) % colorPairs.count
        return colorPairs[index]
    }

    /// Background color for the start page
    static var background: Color { AppColors.workspaceBackgroundColor }

    /// Section title color
    static var sectionTitle: Color { Color.primary.opacity(0.9) }

    /// Favorite item title color
    static var favoriteTitle: Color { Color.primary.opacity(0.75) }

    /// History item title color
    static var historyTitle: Color { Color.secondary }

    /// History icon background
    static var historyIconBackground: Color { Color.primary.opacity(0.08) }

    /// History icon text color
    static var historyIconText: Color { Color.secondary }

    /// Empty state text colors
    static var emptyTitle: Color { Color.secondary }
    static var emptySubtitle: Color { Color.secondary.opacity(0.75) }
}
