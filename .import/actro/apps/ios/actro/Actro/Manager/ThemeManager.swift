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
        // 从UserDefaults加载主题设置
        self.isDarkMode = UserDefaults.standard.bool(forKey: "isDarkMode")
        
        // 默认跟随系统
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
    
    /// 检查系统当前是否是浅色模式
    /// - Returns: 如果是浅色模式返回true，深色模式返回false
    func isLightMode() -> Bool {
        if isSystemTheme {
            // 如果跟随系统设置，则返回系统当前的模式
            return UITraitCollection.current.userInterfaceStyle == .light
        } else {
            // 如果使用自定义设置，则根据isDarkMode判断
            return !isDarkMode
        }
    }
}

extension UserDefaults {
    func contains(key: String) -> Bool {
        return object(forKey: key) != nil
    }
}
