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
    static let background = Color(hex: "#1e1e1e")!

    /// Section title color
    static let sectionTitle = Color.white.opacity(0.9)

    /// Favorite item title color
    static let favoriteTitle = Color.white.opacity(0.7)

    /// History item title color
    static let historyTitle = Color.white.opacity(0.5)

    /// History icon background
    static let historyIconBackground = Color.white.opacity(0.1)

    /// History icon text color
    static let historyIconText = Color.white.opacity(0.6)

    /// Empty state text colors
    static let emptyTitle = Color.white.opacity(0.4)
    static let emptySubtitle = Color.white.opacity(0.3)
}

