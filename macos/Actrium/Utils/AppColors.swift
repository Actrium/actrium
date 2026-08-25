import AppKit
import SwiftUI

/// Centralized app color constants
enum AppColors {
    static var usesDarkAppearance: Bool {
        NSApp.effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
    }

    /// Main toolbar and window content surface.
    static var contentBackground: NSColor { .windowBackgroundColor }
    static var contentBackgroundColor: Color { Color(nsColor: contentBackground) }

    /// Browser workspace behind focused pages and Overview cards.
    static var workspaceBackground: NSColor { .textBackgroundColor }
    static var workspaceBackgroundColor: Color { Color(nsColor: workspaceBackground) }

    /// Default card/control surface used by native Overview content.
    static var overviewCardBackground: NSColor { .controlBackgroundColor }
    static var overviewCardBackgroundColor: Color { Color(nsColor: overviewCardBackground) }

    static var separator: NSColor { .separatorColor }
    static var separatorColor: Color { Color(nsColor: separator) }

    /// Subtle semantic rollover colors for the compact Tab. Keeping the base
    /// label color and applying AppKit's rollover effect avoids the large jump
    /// caused by replacing content with translucent white.
    static var tabSecondaryInteractiveHoverColor: Color {
        Color(nsColor: NSColor.secondaryLabelColor.withSystemEffect(.rollover))
    }

    static var tabPrimaryInteractiveHoverColor: Color {
        Color(nsColor: NSColor.labelColor.withSystemEffect(.rollover))
    }

    /// Subtle complete outline around the nested Body container. It replaces
    /// the old full-width Header/Body separator.
    static var bodyBorder: NSColor {
        separator.withAlphaComponent(usesDarkAppearance ? 0.58 : 0.42)
    }

    /// Opaque backing used only while a Tab/Tab Group is being reordered.
    /// The normal Header remains transparent so it reveals the exact same
    /// material as the outer window frame.
    static var windowChromeDragPlate: NSColor {
        usesDarkAppearance
            ? NSColor(calibratedWhite: 0.16, alpha: 0.96)
            : NSColor(calibratedWhite: 0.94, alpha: 0.96)
    }
    static var windowChromeDragPlateColor: Color {
        Color(nsColor: windowChromeDragPlate)
    }

    /// Fully opaque base shared by every Tab. Accordion depth is expressed by
    /// a short directional head mask, never by making the whole Tab material
    /// translucent.
    static var windowChromeTabPlate: NSColor {
        usesDarkAppearance
            ? NSColor(calibratedWhite: 0.16, alpha: 1)
            : NSColor(calibratedWhite: 0.94, alpha: 1)
    }
    static var windowChromeTabPlateColor: Color {
        Color(nsColor: windowChromeTabPlate)
    }

    /// Safari-like address editing surface. The focused field is primarily a
    /// neutral capsule; accent color is reserved for a restrained rim/glow.
    static var addressEditingSurfaceColor: Color {
        usesDarkAppearance
            ? Color.white.opacity(0.10)
            : Color.white.opacity(0.56)
    }

    /// Blend the user's accent with the window chrome so the focus treatment
    /// remains semantic without presenting as a saturated blue marker.
    static var softenedAddressFocusAccent: NSColor {
        let chrome = usesDarkAppearance
            ? NSColor(calibratedWhite: 0.28, alpha: 1)
            : NSColor(calibratedWhite: 0.92, alpha: 1)
        return NSColor.controlAccentColor.blended(withFraction: 0.48, of: chrome)
            ?? NSColor.controlAccentColor
    }

    static var softenedAddressFocusAccentColor: Color {
        Color(nsColor: softenedAddressFocusAccent)
    }

    static var addressFocusInnerHighlightColor: Color {
        Color.white.opacity(usesDarkAppearance ? 0.12 : 0.48)
    }

    /// Serialized fallback used by native/system tabs before their semantic
    /// SwiftUI surface is mounted. Web tabs keep their page-provided color.
    static var nativeTabBackgroundHex: String {
        usesDarkAppearance ? "#1e1e1e" : "#ffffff"
    }
}
