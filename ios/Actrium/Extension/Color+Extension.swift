import SwiftUI

extension Color {
	init(hex: String, opacity: Double = 1.0) {
		let hex = hex.trimmingCharacters(in: CharacterSet.alphanumerics.inverted)
		var int: UInt64 = 0
		Scanner(string: hex).scanHexInt64(&int)
		
		let r, g, b, a: UInt64
		switch hex.count {
		case 3: // RGB (12-bit)
			(r, g, b, a) = (
				(int >> 8) * 17,
				(int >> 4 & 0xF) * 17,
				(int & 0xF) * 17,
				255
			)
		case 6: // RGB (24-bit)
			(r, g, b, a) = (
				int >> 16,
				int >> 8 & 0xFF,
				int & 0xFF,
				255
			)
		case 8: // RGBA (32-bit)
			(r, g, b, a) = (
				int >> 24 & 0xFF,
				int >> 16 & 0xFF,
				int >> 8 & 0xFF,
				int & 0xFF
			)
		default:
			(r, g, b, a) = (1, 1, 1, 255) // fallback: white
		}
		
		self.init(
			.sRGB,
			red: Double(r) / 255,
			green: Double(g) / 255,
			blue: Double(b) / 255,
			opacity: Double(a) / 255 * opacity
		)
	}
	
	// Theme Colour - AutoAdapt to Light/Dark Mode
	static func themed(light: String, dark: String, opacity: Double = 1.0) -> Color {
		let themeManager = ThemeManager.shared

		if themeManager.isSystemTheme {
			// Follow System Settings
			let dynamicUIColor = UIColor { traitCollection in
				let hex = (traitCollection.userInterfaceStyle == .dark) ? dark : light
				return UIColor(Color(hex: hex, opacity: opacity))
			}
			return Color(dynamicUIColor.cgColor)
		} else {
			// Use application settings
			return themeManager.isDarkMode
				? Color(hex: dark, opacity: opacity)
				: Color(hex: light, opacity: opacity)
		}
	}

	// Predefined Theme Colour
	static var backgroundPrimary: Color {
		return themed(light: "#FFFFFF", dark: "#121212")
	}
	
	static var backgroundSecondary: Color {
		return themed(light: "#F6F6F6", dark: "#1E1E1E")
	}
	
	static var backgroundInput: Color {
		return themed(light: "#FFFFFF", dark: "#333333")
	}
	
	static var backgroundTemperature: Color {
		return themed(light: "#F3ECE4", dark: "#282828")
	}
	
	static var backgroundLocation: Color {
		return themed(light: "#FEFCFC", dark: "#131313")
	}
	
	static var backgroundMapAlert: Color {
		return themed(light: "#F6F6F6", dark: "#353535")
	}
	
	static var backgroundFeedback: Color {
		return themed(light: "#FFFFFF", dark: "#2C2C2C")
	}
    
    static var backgroundReport: Color {
        return themed(light: "#FFFFFF", dark: "#B5B5B6")
    }
	
	static var backgroundNavigation: Color {
		return themed(light: "#FFFFFF", dark: "#000000")
	}
	
	static var backgroundCompany: Color {
		return themed(light: "#0057FF16", dark: "#1C69FF16")
	}
    
    static var backgroundImage: Color {
        return themed(light: "#FFFFFF", dark: "#111111")
    }
    
    static var backgroundAbout: Color {
        return themed(light: "#F7F7F7", dark: "#111111")
    }
    
    static var backgroundAboutItem: Color {
        return themed(light: "#FFFFFF", dark: "#282828")
    }
	
	static var textPrimary: Color {
		return themed(light: "#252525", dark: "#D5D5D5")
	}
	
	static var textSecondary: Color {
		return themed(light: "#565656", dark: "#D5D5D5")
	}
	
	static var textDeleteChatTip: Color {
		return themed(light: "#565656", dark: "#6C6C6C")
	}
	
	static var textInput: Color {
		return themed(light: "#A0A0A0", dark: "#7C7C7C")
	}
	
	static var textFeedback: Color {
		return themed(light: "#2C1717", dark: "#D5D5D5")
	}
	
	static var textSubmit: Color {
		return themed(light: "#FFFFFF", dark: "#D5D5D5")
	}
	
	static var textMapDetail: Color {
		return themed(light: "#B18247", dark: "#D5D5D5")
	}
	
	static var textMapSelect: Color {
		return themed(light: "#9D9D9D", dark: "#6C6C6C")
	}
    
    static var textPrivate: Color {
        return themed(light: "#9D9D9D", dark: "#6C6C6C")
    }
    
    static var textPrivateBtn: Color {
        return themed(light: "#000000", dark: "#D5D5D5")
    }
    
    static var textQRScan: Color {
        return themed(light: "#252525", dark: "#FFFFFF")
    }
    
    static var textLoginSheetBtn: Color {
        return themed(light: "#597EF7", dark: "#D5D5D5")
    }
    
    static var textLoginBtn: Color {
        return themed(light: "#32325D", dark: "#D5D5D5")
    }
    
    static var textTabsBtn: Color {
        return themed(light: "#404040", dark: "#ECECED")
    }
    
    static var textReference: Color {
        return themed(light: "#9E9E9E", dark: "#898989")
    }
    
    static var lablesTertiary: Color {
        return themed(light: "#3C3C43", dark: "#EBEBF5", opacity: 0.3)
    }
	
	static var feedBackBorder: Color {
		return themed(light: "#D5D5D5", dark: "#FFFFFF26")
	}
	
	static var mapDetailBorder: Color {
		return themed(light: "#00000026", dark: "#FFFFFF26")
	}
	
	static var feedTextFiled: Color {
		return themed(light: "#F6F6F6", dark: "#232323")
	}
	
	static var bubbleSender: Color {
		return themed(light: "#E6E9F6", dark: "#2A2A2E")
	}
    
    static var bubbleReceiver: Color {
        return themed(light: "#F9F9FB", dark: "#2C2C2E")
    }
	
	static var bubbleLocation: Color {
		return themed(light: "#FEFCFC", dark: "#131313")
	}
    
    static var bubbleDivider: Color {
        return themed(light: "#EAEAEA", dark: "#313131")
    }
    
    static var bubbleAction: Color {
        return themed(light: "#EAEAEA", dark: "#414141")
    }
	
	static var divider: Color {
		return themed(light: "#DFDFDF", dark: "#3D3D3D")
	}
    
    static var tabDivider: Color {
        return themed(light: "#F2F2F6", dark: "#3A3A3C", opacity: 0.95)
    }
	
	static var menuDivider: Color {
		return themed(light: "#00000014", dark: "#FFFFFF14")
	}
	
	static var menuBoldDivider: Color {
		return themed(light: "#EFEFEF", dark: "#373737")
	}
	
	static var cardBackground: Color {
		return themed(light: "#FBF9F7", dark: "#2C2C2E")
	}
	
	static var carDivder: Color {
		return themed(light: "#FFFFFF", dark: "#000000")
	}
	
	static var locationClose: Color {
		return themed(light: "#000000", dark: "#FFFFFF")
	}
	
	static var editClose: Color {
		return themed(light: "#969696", dark: "#6B6B6B")
	}
    
    static var channelsBackground: Color {
        return themed(light: "#FFFFFF", dark: "#191919")
    }
    
    static var activeBanner: Color {
        return themed(light: "#E5E5EA", dark: "#1C1C1E")
    }
    
    static var scrollDownShadow: Color {
        return themed(light: "#13131366", dark: "#1313134d")
    }
    
    static var inputShadow: Color {
        return themed(light: "#2C38614D", dark: "#2C38614D")
    }
    
    static var aboutRightArrow: Color {
        return themed(light: "#D5D5D5", dark: "#666666")
    }
	
	static var imagePrimary: Color? {
		let themeManager = ThemeManager.shared

		if themeManager.isSystemTheme {
			return UITraitCollection.current.userInterfaceStyle == .dark ? .white : nil
		} else {
			return themeManager.isDarkMode ? .white : nil
		}
	}
}
