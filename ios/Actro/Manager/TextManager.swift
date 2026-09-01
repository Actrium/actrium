import Foundation
import SwiftUI
import Combine

/// Global Text Management
/// Manage all localized text in application using a single case
class TextManager: ObservableObject {
    // Single instance
    static let shared = TextManager()
    
    // Language Manager Reference
    private let languageManager = LanguageManager.shared
    
    private init() {}
    
    // MARK: - Universal Text Acquisition Method
    
    /// Get Localized Text by Key
    /// - Parameter key: Keys for Localized Text
    /// - Returns: Localized Text
    func text(_ key: TextKey) -> String {
        return languageManager.localizedString(forKey: key.rawValue)
    }
    
    // MARK: - Specific scenes obtained
    
    /// Get the relevant text file for the application list page
    var policy: PrivatePolicy {
        return PrivatePolicy()
    }
    
    var active: StartActive {
        return StartActive()
    }

    var tabContextMenu: TabContextMenu {
        return TabContextMenu()
    }

    var tabStatus: TabStatusText {
        return TabStatusText()
    }

    var emptyActive: EmptyActiveText {
        return EmptyActiveText()
    }
}

/// MARK: - File key count
/// An enumeration of all the file keys
enum TextKey: String {
    // Privacy protocol
    case privateTitle
    case privateMessage
    case privateDisagree
    case privateAgree
        
    // Start Page
    case activity
    case preferences
    case bookmarks
    case search
    case goback
    case blackTab

    // Long Press Menu
    case bookmark
    case unbookmark
    case closeTab
    case closeOtherTabs

    // Tab Status
    case serverClosed

    // Empty Data Page
    case emptyActiveTitle
    case emptyActiveSubtitle
}

// MARK: - scenes case structure

/// Privacy protocol-related documentation
struct PrivatePolicy {
    func privateTitle() -> String { return TextManager.shared.text(.privateTitle) }
    func privateMessage() -> String { return TextManager.shared.text(.privateMessage) }
    func privateDisagree() -> String { return TextManager.shared.text(.privateDisagree) }
    func privateAgree() -> String { return TextManager.shared.text(.privateAgree) }
}

struct StartActive {
    func activity() -> String { return TextManager.shared.text(.activity) }
    func preferences() -> String { return TextManager.shared.text(.preferences) }
    func bookmarks() -> String { return TextManager.shared.text(.bookmarks) }
    func search() -> String { return TextManager.shared.text(.search) }
    func goback() -> String { return TextManager.shared.text(.goback) }
    func blackTab() -> String { return TextManager.shared.text(.blackTab) }
}

/// Long-Campaign Text
struct TabContextMenu {
    func bookmark() -> String { return TextManager.shared.text(.bookmark) }
    func unbookmark() -> String { return TextManager.shared.text(.unbookmark) }
    func closeTab() -> String { return TextManager.shared.text(.closeTab) }
    func closeOtherTabs() -> String { return TextManager.shared.text(.closeOtherTabs) }
}

/// Tag Status Relevant Document
struct TabStatusText {
    func serverClosed() -> String { return TextManager.shared.text(.serverClosed) }
}

/// Empty Data Page Relevant Texts
struct EmptyActiveText {
    func title() -> String { return TextManager.shared.text(.emptyActiveTitle) }
    func subtitle() -> String { return TextManager.shared.text(.emptyActiveSubtitle) }
}

// MARK: - SwiftUIText Extension
extension View {
    /// It's convenient.SwiftUIUse localized text method
    func localizedText(_ key: TextKey) -> Text {
        return Text(TextManager.shared.text(key))
    }
}

// MARK: - Easy access
/// Provide global access points to facilitate the use of codes
let TM = TextManager.shared
