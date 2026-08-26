import AppKit
import Combine

enum AppAppearanceMode: String, CaseIterable, Identifiable, Codable {
    case system
    case light
    case dark

    var id: Self { self }

    var title: String {
        switch self {
        case .system: return "跟随系统"
        case .light: return "Light"
        case .dark: return "Dark"
        }
    }

    fileprivate var appKitAppearance: NSAppearance? {
        switch self {
        case .system: return nil
        case .light: return NSAppearance(named: .aqua)
        case .dark: return NSAppearance(named: .darkAqua)
        }
    }
}

/// Single source of truth for the appearance of every Actro-owned window.
@MainActor
final class AppearanceManager {
    static let shared = AppearanceManager()
    static let storageKey = "actro.appearance.mode"

    private(set) var mode: AppAppearanceMode

    private init(defaults: UserDefaults = .standard) {
        if let rawValue = defaults.string(forKey: Self.storageKey),
           let storedMode = AppAppearanceMode(rawValue: rawValue) {
            mode = storedMode
        } else {
            mode = .system
        }
    }

    /// Commits a Picker choice after SwiftUI finishes its current control
    /// update. The manager deliberately isn't ObservableObject: the Picker owns
    /// its transient selection state, while this object owns persistence and
    /// the application-wide AppKit appearance transaction.
    func select(_ selectedMode: AppAppearanceMode) {
        guard selectedMode != mode else { return }

        DispatchQueue.main.async { [weak self] in
            guard let self, selectedMode != self.mode else { return }
            self.mode = selectedMode
            UserDefaults.standard.set(selectedMode.rawValue, forKey: Self.storageKey)
            self.apply()
        }
    }

    func apply() {
        NSApplication.shared.appearance = mode.appKitAppearance
    }
}

/// Preset materials shared by the outer window frame and compact Header.
/// The body is intentionally opaque, so this glass surface remains visible
/// only in the outer gutter and Header region.
enum WindowChromeStyle: String, CaseIterable, Identifiable, Codable {
    case clearGlass
    case graphiteGlass
    case oceanGlass
    case auroraGlass
    case roseGlass

    var id: Self { self }

    var title: String {
        switch self {
        case .clearGlass: return "Clear Glass"
        case .graphiteGlass: return "Graphite"
        case .oceanGlass: return "Ocean"
        case .auroraGlass: return "Aurora"
        case .roseGlass: return "Rose"
        }
    }

    var subtitle: String {
        switch self {
        case .clearGlass: return "Neutral translucent glass"
        case .graphiteGlass: return "Smoky monochrome glass"
        case .oceanGlass: return "Cool blue glass"
        case .auroraGlass: return "Soft violet glass"
        case .roseGlass: return "Warm rose glass"
        }
    }

    /// CSS preview color mirrors the translucent tint applied above the
    /// native glass material instead of presenting an opaque paint swatch.
    var previewCSSColor: String {
        switch self {
        case .clearGlass: return "rgba(174, 180, 188, 0.34)"
        case .graphiteGlass: return "rgba(98, 105, 116, 0.42)"
        case .oceanGlass: return "rgba(76, 141, 219, 0.40)"
        case .auroraGlass: return "rgba(138, 115, 214, 0.40)"
        case .roseGlass: return "rgba(216, 120, 145, 0.40)"
        }
    }

    var material: NSVisualEffectView.Material {
        switch self {
        case .clearGlass: return .headerView
        case .graphiteGlass: return .underWindowBackground
        case .oceanGlass, .auroraGlass, .roseGlass: return .hudWindow
        }
    }

    var tintColor: NSColor {
        let isDark = AppColors.usesDarkAppearance
        switch self {
        case .clearGlass:
            return NSColor(
                calibratedWhite: isDark ? 0.36 : 0.92,
                alpha: isDark ? 0.18 : 0.18
            )
        case .graphiteGlass:
            return NSColor(
                calibratedRed: 0.24,
                green: 0.27,
                blue: 0.32,
                alpha: isDark ? 0.36 : 0.24
            )
        case .oceanGlass:
            return NSColor(
                calibratedRed: 0.18,
                green: 0.48,
                blue: 0.82,
                alpha: isDark ? 0.34 : 0.26
            )
        case .auroraGlass:
            return NSColor(
                calibratedRed: 0.46,
                green: 0.34,
                blue: 0.82,
                alpha: isDark ? 0.34 : 0.24
            )
        case .roseGlass:
            return NSColor(
                calibratedRed: 0.82,
                green: 0.34,
                blue: 0.48,
                alpha: isDark ? 0.32 : 0.24
            )
        }
    }

    /// Opaque approximation used only while one Tab is physically layered
    /// above another. Ordinary Tab seams expose the real Header material;
    /// a layered boundary must instead occlude the lower Tab surface.
    var layeredTabSeparationColor: NSColor {
        let baseWhite: CGFloat = AppColors.usesDarkAppearance ? 0.16 : 0.94
        let base = NSColor(calibratedWhite: baseWhite, alpha: 1)
        guard let tint = tintColor.usingColorSpace(.deviceRGB),
              let resolvedBase = base.usingColorSpace(.deviceRGB) else {
            return base
        }

        let alpha = tint.alphaComponent
        let inverseAlpha = 1 - alpha
        return NSColor(
            calibratedRed: resolvedBase.redComponent * inverseAlpha
                + tint.redComponent * alpha,
            green: resolvedBase.greenComponent * inverseAlpha
                + tint.greenComponent * alpha,
            blue: resolvedBase.blueComponent * inverseAlpha
                + tint.blueComponent * alpha,
            alpha: 1
        )
    }

}

/// Single source of truth for the configurable outer frame/Header material.
@MainActor
final class WindowChromeManager: ObservableObject {
    static let shared = WindowChromeManager()
    static let storageKey = "actro.appearance.windowChrome"

    @Published private(set) var style: WindowChromeStyle

    private init(defaults: UserDefaults = .standard) {
        if let rawValue = defaults.string(forKey: Self.storageKey),
           let storedStyle = WindowChromeStyle(rawValue: rawValue) {
            style = storedStyle
        } else {
            style = .clearGlass
        }
    }

    func select(_ selectedStyle: WindowChromeStyle) {
        guard selectedStyle != style else { return }
        style = selectedStyle
        UserDefaults.standard.set(selectedStyle.rawValue, forKey: Self.storageKey)
    }
}
