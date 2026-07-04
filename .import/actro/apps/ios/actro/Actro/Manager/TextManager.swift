import Foundation
import SwiftUI
import Combine

/// 全局文案管理类
/// 使用单例模式管理应用中的所有本地化文本
class TextManager: ObservableObject {
    // 单例实例
    static let shared = TextManager()
    
    // 语言管理器引用
    private let languageManager = LanguageManager.shared
    
    private init() {}
    
    // MARK: - 通用文本获取方法
    
    /// 通过键获取本地化文本
    /// - Parameter key: 本地化文本的键
    /// - Returns: 本地化文本
    func text(_ key: TextKey) -> String {
        return languageManager.localizedString(forKey: key.rawValue)
    }
    
    // MARK: - 特定场景文案获取
    
    /// 获取应用列表页相关文案
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

/// MARK: - 文案键枚举
/// 所有文案键的枚举
enum TextKey: String {
    // 隐私协议
    case privateTitle
    case privateMessage
    case privateDisagree
    case privateAgree
        
    // 起始页
    case activity
    case preferences
    case bookmarks
    case search
    case goback
    case blackTab

    // 长按菜单
    case bookmark
    case unbookmark
    case closeTab
    case closeOtherTabs

    // 标签状态
    case serverClosed

    // 空数据页
    case emptyActiveTitle
    case emptyActiveSubtitle
}

// MARK: - 场景文案结构体

/// 隐私协议相关文案
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

/// 长按菜单相关文案
struct TabContextMenu {
    func bookmark() -> String { return TextManager.shared.text(.bookmark) }
    func unbookmark() -> String { return TextManager.shared.text(.unbookmark) }
    func closeTab() -> String { return TextManager.shared.text(.closeTab) }
    func closeOtherTabs() -> String { return TextManager.shared.text(.closeOtherTabs) }
}

/// 标签状态相关文案
struct TabStatusText {
    func serverClosed() -> String { return TextManager.shared.text(.serverClosed) }
}

/// 空数据页相关文案
struct EmptyActiveText {
    func title() -> String { return TextManager.shared.text(.emptyActiveTitle) }
    func subtitle() -> String { return TextManager.shared.text(.emptyActiveSubtitle) }
}

// MARK: - SwiftUI文本扩展
extension View {
    /// 方便在SwiftUI中使用本地化文本的方法
    func localizedText(_ key: TextKey) -> Text {
        return Text(TextManager.shared.text(key))
    }
}

// MARK: - 便捷访问
/// 提供全局访问点，方便代码中使用
let TM = TextManager.shared
