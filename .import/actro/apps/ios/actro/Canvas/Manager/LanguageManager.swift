import Foundation
import SwiftUI
import Combine

class LanguageManager: ObservableObject {
	static let shared = LanguageManager()
    
    // AskAway应用支持的语言列表
    let appSupportedLanguages = ["en", "zh-Hans", "ja", "fr", "ko", "zh-Hant", "es", "ru"]
    let appDefaultLanguage = "zh-Hans"
	
	// 入驻应用支持的语言列表
    var supportedLanguages: [String] = []
	
	@Published var currentLanguage: String
	@Published var languageHeight: CGFloat = 0
	@Published var feedbackHeight: CGFloat = 0

	private init() {
        self.currentLanguage = appDefaultLanguage
        initAppLanguage()
    }
    
    private func initAppLanguage() {
        // 获取系统完整语言标识
        let preferredLanguage = Locale.preferredLanguages.first ?? appDefaultLanguage
        
        supportedLanguages = appSupportedLanguages
        
        // 匹配语言标识到支持的语言
        self.currentLanguage = getMatchedLanguage(preferredLanguage)
    }
    
    func getDefaultLanguage() {
        // 获取系统完整语言标识
        let preferredLanguage = Locale.preferredLanguages.first ?? appDefaultLanguage
                                
        // 匹配语言标识到支持的语言
        let matchedLanguage = self.getMatchedLanguage(preferredLanguage)
                
        // 如果用户已经设置了语言，优先使用用户设置的语言
        if !matchedLanguage.isEmpty {
            self.currentLanguage = matchedLanguage
        } else {
            self.currentLanguage = appDefaultLanguage
        }
    }
	
	// 将系统语言标识匹配到支持的语言
	private func getMatchedLanguage(_ preferredLanguage: String) -> String {
		let languageCode = preferredLanguage.components(separatedBy: "-").first ?? appDefaultLanguage
		
		// 特殊处理中文
		if languageCode == "zh" {
			if preferredLanguage.contains("Hans") {
				return "zh-Hans"
			} else if preferredLanguage.contains("Hant") {
				return "zh-Hant"
			} else {
				return "zh-Hans" // 默认使用简体中文
			}
		}
		
		// 直接匹配支持的语言代码
		if supportedLanguages.contains(languageCode) {
			return languageCode
		}
		
		// 尝试通过前缀匹配找到最合适的语言
		for supportedLanguage in supportedLanguages {
			if supportedLanguage.hasPrefix(languageCode) || languageCode.hasPrefix(supportedLanguage) {
				return supportedLanguage
			}
		}
		
		// 默认返回中文
        return appDefaultLanguage
	}
	
	func settingLanguage(_ code: String) {
        currentLanguage = ""
		currentLanguage = code
        UserDefaults.standard.set([currentLanguage], forKey: "")
        UserDefaults.standard.synchronize()
	}

	func localizedString(forKey key: String) -> String {
		guard let path = Bundle.main.path(forResource: currentLanguage, ofType: "lproj"),
			  let bundle = Bundle(path: path) else {
			return NSLocalizedString(key, comment: "")
		}
		return NSLocalizedString(key, bundle: bundle, comment: "")
	}
	
	func greeting(forCode code: String) -> String {
		// 如果 ConfigManager 中没有对应的配置，使用本地化文件
		let key = "welcome"
		guard let path = Bundle.main.path(forResource: code, ofType: "lproj"),
			  let bundle = Bundle(path: path) else {
			return NSLocalizedString(key, comment: "")
		}
		return NSLocalizedString(key, bundle: bundle, comment: "")
	}
	
	func backgroundColor(_ language: String) -> Color {
		switch language {
		case "en":
			return Color(hex: "#DA5E4A").opacity(0.05)
		case "zh-Hans":
			return Color(hex: "#DA5E4A").opacity(0.05)
		case "ja":
			return Color(hex: "#337B8B").opacity(0.05)
		case "fr":
			return Color(hex: "#D0687B").opacity(0.05)
		case "ko":
			return Color(hex: "#004BFF").opacity(0.05)
		case "zh-Hant":
			return Color(hex: "#DA5E4A").opacity(0.05)
		case "es":
			return Color(hex: "#D19347").opacity(0.05)
		case "ru":
			return Color(hex: "#5D509A").opacity(0.05)
		default:
			return Color(hex: "#DA5E4A")
		}
	}
	
	func fontColor(_ language: String) -> Color {
		switch language {
		case "en":
			return Color(hex: "#DA5E4A")
		case "zh-Hans":
			return Color(hex: "#DA5E4A")
		case "ja":
			return Color(hex: "#337B8B")
		case "fr":
			return Color(hex: "#D0687B")
		case "ko":
			return Color(hex: "#004BFF")
		case "zh-Hant":
			return Color(hex: "#DA5E4A")
		case "es":
			return Color(hex: "#D19347")
		case "ru":
			return Color(hex: "#5D509A")
		default:
			return Color(hex: "#FFFFFF")
		}
	}
	
	var isChinese: Bool {
		return currentLanguage.hasPrefix("zh")
	}
	
	// MARK: - ConfigManager Integration
	/// 获取指定语言的本地化字符串（支持 ConfigManager 集成）
	func localizedString(forKey key: String, language: String) -> String {
		// 如果 ConfigManager 中没有对应的配置，或者 key 不在预定义范围内，使用本地化文件
		guard let path = Bundle.main.path(forResource: language, ofType: "lproj"),
			  let bundle = Bundle(path: path) else {
			return NSLocalizedString(key, comment: "")
		}
		return NSLocalizedString(key, bundle: bundle, comment: "")
	}
}
