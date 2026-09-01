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
