import Foundation
import SwiftUI

/// Represents a tag for organizing bookmarks
struct Tag: Identifiable, Codable, Hashable, Equatable {
    let id: UUID
    var name: String
    var color: String?      // Hex color like "#FF5733"
    var isSystem: Bool      // System tags cannot be deleted

    init(id: UUID = UUID(), name: String, color: String? = nil, isSystem: Bool = false) {
        self.id = id
        self.name = name
        self.color = color
        self.isSystem = isSystem
    }
}

// MARK: - Color Helpers

extension Tag {
    /// SwiftUI Color from hex string
    var swiftUIColor: Color {
        guard let hex = color else {
            return .accentColor
        }
        return Color(hex: hex)
    }

    /// Default colors for new tags
    static let defaultColors: [String] = [
        "#FF3B30", // Red
        "#FF9500", // Orange
        "#FFCC00", // Yellow
        "#34C759", // Green
        "#00C7BE", // Teal
        "#007AFF", // Blue
        "#5856D6", // Purple
        "#AF52DE", // Magenta
        "#FF2D55", // Pink
        "#8E8E93"  // Gray
    ]
}

// MARK: - System Tags

extension Tag {
    static let systemApp = Tag(name: "App", color: "#007AFF", isSystem: true)
    static let systemManufacturer = Tag(name: "Manufacturer", color: "#5856D6", isSystem: true)
    static let systemLastUsed = Tag(name: "LastUsed", color: "#FF9500", isSystem: true)
    static let systemFrequent = Tag(name: "Frequent", color: "#FF2D55", isSystem: true)
}

// MARK: - Color Extension

extension Color {
    init(hex: String) {
        var hexSanitized = hex.trimmingCharacters(in: .whitespacesAndNewlines)
        hexSanitized = hexSanitized.replacingOccurrences(of: "#", with: "")

        var rgb: UInt64 = 0
        guard Scanner(string: hexSanitized).scanHexInt64(&rgb) else {
            // 解析失败，返回默认灰色
            self.init(white: 0.5)
            return
        }

        let length = hexSanitized.count
        if length == 6 {
            self.init(
                red: Double((rgb & 0xFF0000) >> 16) / 255.0,
                green: Double((rgb & 0x00FF00) >> 8) / 255.0,
                blue: Double(rgb & 0x0000FF) / 255.0
            )
        } else if length == 8 {
            self.init(
                red: Double((rgb & 0xFF000000) >> 24) / 255.0,
                green: Double((rgb & 0x00FF0000) >> 16) / 255.0,
                blue: Double((rgb & 0x0000FF00) >> 8) / 255.0,
                opacity: Double(rgb & 0x000000FF) / 255.0
            )
        } else {
            // 不支持的格式，返回默认灰色
            self.init(white: 0.5)
        }
    }
}
