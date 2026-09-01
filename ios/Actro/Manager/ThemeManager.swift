import SwiftUI
import Combine

enum ThemeSchema: Int {
    case unspecified, light, dark
}

class ThemeManager: ObservableObject {
    static let shared = ThemeManager()
    
    @AppStorage("colorSchema") var colorScheme: ThemeSchema = .unspecified {
        didSet {
            applyColorSchema()
        }
    }
    
    func applyColorSchema() {
        keyWindow?.overrideUserInterfaceStyle = UIUserInterfaceStyle(rawValue: colorScheme.rawValue)!
    }
    
    var keyWindow: UIWindow? {
        guard let scene = UIApplication.shared.connectedScenes.first,
              let windowSceneDelegate = scene.delegate as? UIWindowSceneDelegate,
              let window = windowSceneDelegate.window else {
            return nil
        }
        return window
    }
    
    @Published var isDarkMode: Bool {
        didSet {
            UserDefaults.standard.set(isDarkMode, forKey: "isDarkMode")
        }
    }
    
    @Published var isSystemTheme: Bool {
        didSet {
            UserDefaults.standard.set(isSystemTheme, forKey: "isSystemTheme")
        }
    }
    
    private init() {
        // FromUserDefaultsLoad Theme Settings
        self.isDarkMode = UserDefaults.standard.bool(forKey: "isDarkMode")
        
        // Default Following System
        if !UserDefaults.standard.contains(key: "isSystemTheme") {
            self.isSystemTheme = true
            UserDefaults.standard.set(true, forKey: "isSystemTheme")
        } else {
            self.isSystemTheme = UserDefaults.standard.bool(forKey: "isSystemTheme")
        }
    }
    
    func toggleDarkMode() {
        isDarkMode.toggle()
        isSystemTheme = false
    }
    
    func useSystemTheme() {
        isSystemTheme = true
        isDarkMode = UITraitCollection.current.userInterfaceStyle == .dark
    }
    
    /// Check if the system is currently light-colored
    /// - Returns: If it's light-coloured, returntrue, dark-colour returnfalse
    func isLightMode() -> Bool {
        if isSystemTheme {
            // If following the system settings, return the current system mode
            return UITraitCollection.current.userInterfaceStyle == .light
        } else {
            // If custom settings are used, byisDarkModeJudgement
            return !isDarkMode
        }
    }
}

extension UserDefaults {
    func contains(key: String) -> Bool {
        return object(forKey: key) != nil
    }
}
