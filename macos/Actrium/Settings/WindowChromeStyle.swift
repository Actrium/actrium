import AppKit
import Combine

enum AppAppearanceMode: String, CaseIterable, Identifiable {
    case system, light, dark

    var id: Self { self }
    var title: String {
        switch self {
        case .system: "Follow System"
        case .light: "Light"
        case .dark: "Dark"
        }
    }
    var appKitAppearance: NSAppearance? {
        switch self {
        case .system: nil
        case .light: NSAppearance(named: .aqua)
        case .dark: NSAppearance(named: .darkAqua)
        }
    }
}

@MainActor
final class AppearanceManager {
    static let shared = AppearanceManager()
    private static let storageKey = "actrium.appearance.mode"

    private(set) var mode: AppAppearanceMode

    private init(defaults: UserDefaults = .standard) {
        mode = defaults.string(forKey: Self.storageKey)
            .flatMap(AppAppearanceMode.init(rawValue:)) ?? .system
    }

    func select(_ selectedMode: AppAppearanceMode) {
        guard selectedMode != mode else { return }
        DispatchQueue.main.async { [weak self] in
            guard let self, selectedMode != self.mode else { return }
            self.mode = selectedMode
            UserDefaults.standard.set(selectedMode.rawValue, forKey: Self.storageKey)
            self.apply()
        }
    }

    func apply() { NSApplication.shared.appearance = mode.appKitAppearance }
}

/// The frame and compact header share one live AppKit material.
enum WindowChromeStyle: String, CaseIterable, Identifiable {
    case clearGlass, graphiteGlass, oceanGlass, auroraGlass, roseGlass

    var id: Self { self }

    var title: String {
        switch self {
        case .clearGlass: "Clear Glass"
        case .graphiteGlass: "Graphite"
        case .oceanGlass: "Ocean"
        case .auroraGlass: "Aurora"
        case .roseGlass: "Rose"
        }
    }

    var material: NSVisualEffectView.Material {
        switch self {
        case .clearGlass: .headerView
        case .graphiteGlass: .underWindowBackground
        case .oceanGlass, .auroraGlass, .roseGlass: .hudWindow
        }
    }

    var tintColor: NSColor {
        let isDark = AppColors.usesDarkAppearance
        return switch self {
        case .clearGlass:
            NSColor(calibratedWhite: isDark ? 0.36 : 0.92, alpha: 1)
        case .graphiteGlass:
            NSColor(calibratedRed: 0.24, green: 0.27, blue: 0.32,
                    alpha: isDark ? 0.36 : 0.24)
        case .oceanGlass:
            NSColor(calibratedRed: 0.18, green: 0.48, blue: 0.82,
                    alpha: isDark ? 0.34 : 0.26)
        case .auroraGlass:
            NSColor(calibratedRed: 0.46, green: 0.34, blue: 0.82,
                    alpha: isDark ? 0.34 : 0.24)
        case .roseGlass:
            NSColor(calibratedRed: 0.82, green: 0.34, blue: 0.48,
                    alpha: isDark ? 0.32 : 0.24)
        }
    }

    /// A layered tab needs an opaque estimate of the live tinted material.
    var layeredTabSeparationColor: NSColor {
        let base = NSColor(calibratedWhite: AppColors.usesDarkAppearance ? 0.16 : 0.94, alpha: 1)
        guard let tint = tintColor.usingColorSpace(.deviceRGB),
              let resolvedBase = base.usingColorSpace(.deviceRGB) else { return base }
        let alpha = tint.alphaComponent
        return NSColor(
            calibratedRed: resolvedBase.redComponent * (1 - alpha) + tint.redComponent * alpha,
            green: resolvedBase.greenComponent * (1 - alpha) + tint.greenComponent * alpha,
            blue: resolvedBase.blueComponent * (1 - alpha) + tint.blueComponent * alpha,
            alpha: 1
        )
    }
}

@MainActor
final class WindowChromeManager: ObservableObject {
    static let shared = WindowChromeManager()
    private static let storageKey = "actrium.appearance.windowChrome"

    @Published private(set) var style: WindowChromeStyle

    private init(defaults: UserDefaults = .standard) {
        style = defaults.string(forKey: Self.storageKey)
            .flatMap(WindowChromeStyle.init(rawValue:)) ?? .clearGlass
    }

    func select(_ selectedStyle: WindowChromeStyle) {
        guard style != selectedStyle else { return }
        style = selectedStyle
        UserDefaults.standard.set(selectedStyle.rawValue, forKey: Self.storageKey)
    }
}
