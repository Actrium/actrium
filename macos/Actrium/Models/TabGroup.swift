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
        Color(nsColor: nsColor)
    }

    /// The group's tint. Every surface (editor swatch, dot, header fill,
    /// border, sidebar indicator) derives from this single value, so the
    /// group reads the same color everywhere.
    var nsColor: NSColor {
        let base: NSColor
        switch self {
        case .gray:   base = .systemGray
        case .blue:   base = .systemBlue
        case .red:    base = .systemRed
        case .yellow: base = .systemYellow
        case .green:  base = .systemGreen
        case .pink:   base = .systemPink
        case .purple: base = .systemPurple
        case .cyan:   base = .systemCyan
        case .orange: base = .systemOrange
        }
        return base.pastelTint
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

private extension NSColor {
    /// Same hue, lower saturation and higher brightness — a soft pastel tint
    /// of the system color that suits a toolbar surface.
    ///
    /// Saturated system colors already sit at (or near) full brightness, so
    /// the brightness lift is a share of the headroom left rather than a flat
    /// addition; that keeps every case pastel without clipping any to white.
    var pastelTint: NSColor {
        guard let srgb = usingColorSpace(.sRGB) else { return self }

        var hue: CGFloat = 0
        var saturation: CGFloat = 0
        var brightness: CGFloat = 0
        var alpha: CGFloat = 0
        srgb.getHue(&hue, saturation: &saturation, brightness: &brightness, alpha: &alpha)

        return NSColor(
            hue: hue,
            saturation: saturation * 0.45,
            brightness: brightness + (1 - brightness) * 0.45,
            alpha: alpha
        )
    }
}
