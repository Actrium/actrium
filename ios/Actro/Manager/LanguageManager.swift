import Foundation
import SwiftUI
import Combine

class LanguageManager: ObservableObject {
	static let shared = LanguageManager()
    
    // ActroList of languages to apply support
    let appSupportedLanguages = ["en", "zh-Hans", "ja", "fr", "ko", "zh-Hant", "es", "ru"]
    let appDefaultLanguage = "zh-Hans"
	
	// List of languages supported for in-scanning
    var supportedLanguages: [String] = []
	
	@Published var currentLanguage: String
	@Published var languageHeight: CGFloat = 0
	@Published var feedbackHeight: CGFloat = 0

	private init() {
        self.currentLanguage = appDefaultLanguage
        initAppLanguage()
    }
    
    private func initAppLanguage() {
        // Get System Full Language Identification
        let preferredLanguage = Locale.preferredLanguages.first ?? appDefaultLanguage
        
        supportedLanguages = appSupportedLanguages
        
        // Matching Language to Supported Language
        self.currentLanguage = getMatchedLanguage(preferredLanguage)
    }
    
    func getDefaultLanguage() {
        // Get System Full Language Identification
        let preferredLanguage = Locale.preferredLanguages.first ?? appDefaultLanguage
                                
        // Matching Language to Supported Language
        let matchedLanguage = self.getMatchedLanguage(preferredLanguage)
                
        // If the user has set the language, give preference to the language set by the user
        if !matchedLanguage.isEmpty {
            self.currentLanguage = matchedLanguage
        } else {
            self.currentLanguage = appDefaultLanguage
        }
    }
	
	// Match system language identifiers to supported language
	private func getMatchedLanguage(_ preferredLanguage: String) -> String {
		let languageCode = preferredLanguage.components(separatedBy: "-").first ?? appDefaultLanguage
		
		// Special handling of Chinese
		if languageCode == "zh" {
			if preferredLanguage.contains("Hans") {
				return "zh-Hans"
			} else if preferredLanguage.contains("Hant") {
				return "zh-Hant"
			} else {
				return "zh-Hans" // Default use of Chinese
			}
		}
		
		// Language code that directly matches the support
		if supportedLanguages.contains(languageCode) {
			return languageCode
		}
		
		// Try to find the most appropriate language by matching the prefix
		for supportedLanguage in supportedLanguages {
			if supportedLanguage.hasPrefix(languageCode) || languageCode.hasPrefix(supportedLanguage) {
				return supportedLanguage
			}
		}
		
		// Default returns Chinese
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
		// If ConfigManager No matching configuration, use localised files
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
	/// Get a localised string for the specified language (support) ConfigManager Integrated)
	func localizedString(forKey key: String, language: String) -> String {
		// If ConfigManager There is no matching configuration, or key Use localised files not within predefined scope
		guard let path = Bundle.main.path(forResource: language, ofType: "lproj"),
			  let bundle = Bundle(path: path) else {
			return NSLocalizedString(key, comment: "")
		}
		return NSLocalizedString(key, bundle: bundle, comment: "")
	}
}
