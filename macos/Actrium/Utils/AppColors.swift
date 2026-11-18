import AppKit
import SwiftUI

enum AppColors {
    static var tabSecondaryInteractiveHoverColor: Color {
        Color(nsColor: NSColor.secondaryLabelColor.withSystemEffect(.rollover))
    }
    static var tabPrimaryInteractiveHoverColor: Color {
        Color(nsColor: NSColor.labelColor.withSystemEffect(.rollover))
    }
    static var usesDarkAppearance: Bool {
        NSApp.effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
    }

    static var contentBackground: NSColor { .windowBackgroundColor }
    static var contentBackgroundColor: Color { Color(nsColor: contentBackground) }
    static var workspaceBackground: NSColor { .textBackgroundColor }
    static var workspaceBackgroundColor: Color { Color(nsColor: workspaceBackground) }
    static var overviewCardBackground: NSColor { .controlBackgroundColor }
    static var overviewCardBackgroundColor: Color { Color(nsColor: overviewCardBackground) }
    static var separator: NSColor { .separatorColor }
    static var separatorColor: Color { Color(nsColor: separator) }

    static var bodyBorder: NSColor {
        separator.withAlphaComponent(usesDarkAppearance ? 0.58 : 0.42)
    }

    // Ordinary tab seams reveal the live header material. Opaque plates are
    // reserved for the short interval when one tab overlaps another.
    static var windowChromeDragPlateColor: Color {
        Color(nsColor: NSColor(
            calibratedWhite: usesDarkAppearance ? 0.16 : 0.94, alpha: 0.96
        ))
    }

    static var windowChromeTabPlateColor: Color {
        if usesDarkAppearance {
            return Color(nsColor: NSColor(calibratedWhite: 0.16, alpha: 1))
        }
        // #EBEBEC — opaque light plate so inactive Tabs stay solid over the
        // translucent Header material.
        return Color(nsColor: NSColor(
            srgbRed: 235 / 255, green: 235 / 255, blue: 236 / 255, alpha: 1
        ))
    }

    static var windowChromeActiveTabPlateColor: Color {
        Color(nsColor: NSColor(
            calibratedWhite: usesDarkAppearance ? 0.32 : 1, alpha: 1
        ))
    }

    static var addressEditingSurfaceColor: Color {
        usesDarkAppearance ? Color.white.opacity(0.10) : Color.white.opacity(0.56)
    }

    static var softenedAddressFocusAccentColor: Color {
        let chrome = NSColor(calibratedWhite: usesDarkAppearance ? 0.28 : 0.92, alpha: 1)
        return Color(nsColor: NSColor.controlAccentColor.blended(
            withFraction: 0.48, of: chrome
        ) ?? NSColor.controlAccentColor)
    }

    static var addressFocusInnerHighlightColor: Color {
        Color.white.opacity(usesDarkAppearance ? 0.12 : 0.48)
    }

    static var nativeTabBackgroundHex: String {
        usesDarkAppearance ? "#1e1e1e" : "#ffffff"
    }
}
