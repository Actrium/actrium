import Foundation

private let logger = AppLogger.app

/// Tab status
enum TabStatus: String, Codable {
    case alive
    case dead
    case unknown
}

/// System tab types for built-in pages (like Chrome's chrome://history)
enum SystemTabType: String, Codable {
    case history
    case bookmarks
    case login
    case settings
    case globalSearch

    var title: String {
        switch self {
        case .history: return "History"
        case .bookmarks: return "Bookmarks"
        case .login: return "Login"
        case .settings: return "Settings"
        case .globalSearch: return "Search"
        }
    }

    var icon: String {
        switch self {
        case .history: return "clock.arrow.circlepath"
        case .bookmarks: return "bookmark"
        case .login: return "lock"
        case .settings: return "gearshape"
        case .globalSearch: return "magnifyingglass"
        }
    }
}

struct Tab: Identifiable {
    let id: UUID
    var title: String
    var icon: String?  // Tab icon (SF Symbol name or URL string)
    var url: URL?
    var htmlContent: String?

    /// System tab type (nil for regular tabs, set for built-in pages like History/Bookmarks)
    var systemTabType: SystemTabType?

    /// Tab group ID (nil if not in a group)
    var groupId: UUID?

    /// Guest 应用脚本 (已废弃，使用 leftPanelScript 和 rightPanelScript 代替)
    var guestScript: String?

    /// 左侧面板脚本 (menu.js) - Legacy 双 Engine 架构
    var leftPanelScript: String?

    /// 右侧扩展面板脚本 (ext.js) - Legacy 双 Engine 架构
    var rightPanelScript: String?

    /// v2.0: 统一脚本 (unified-app.js) - Single Engine 架构
    var unifiedScript: String?

    /// contracts（Phase 1）：用于宿主提示与版本兼容
    var contractName: String?
    var contractVersion: Int?

    /// permissions（Phase 1）：用于宿主提示（后续可用于 gating）
    var permissions: [String]

    /// 刷新计数：每次 reload 增加，传递给 RN 侧作为 remount key
    var reloadNonce: Int

    /// 左侧面板默认是否可见
    var leftPanelDefaultVisible: Bool

    /// 右侧面板默认是否可见
    var rightPanelDefaultVisible: Bool

    /// Background color for this tab (Hex format: "#rrggbb")
    var backgroundColor: String

    /// App ID parsed from URL (e.g. "xxx" from https://go.askaway.chat/app/xxx)
    var appId: String?

    /// Additional properties from RN (excluding tabId)
    var additionalProps: [String: Any]?

    /// Has unread message indicator (red dot on tab card)
    var hasUnreadMessage: Bool

    /// Tab bookmark status (managed by RN side)
    var isBookmarked: Bool

    /// Tab status: alive or dead
    var status: TabStatus

    init(
        id: UUID = UUID(),
        title: String,
        icon: String? = nil,
        url: URL? = nil,
        appId: String? = nil,
        htmlContent: String? = nil,
        systemTabType: SystemTabType? = nil,
        groupId: UUID? = nil,
        guestScript: String? = nil,
        leftPanelScript: String? = nil,
        rightPanelScript: String? = nil,
        unifiedScript: String? = nil,
        contractName: String? = nil,
        contractVersion: Int? = nil,
        permissions: [String] = [],
        reloadNonce: Int = 0,
        leftPanelDefaultVisible: Bool = false,
        rightPanelDefaultVisible: Bool = false,
        backgroundColor: String? = nil,
        additionalProps: [String: Any]? = nil,
        hasUnreadMessage: Bool = false,
        isBookmarked: Bool = false,
        status: TabStatus = .unknown
    ) {
        self.id = id
        self.title = title
        self.icon = icon
        self.url = url
        self.appId = appId
        self.htmlContent = htmlContent
        self.systemTabType = systemTabType
        self.groupId = groupId
        self.guestScript = guestScript
        self.leftPanelScript = leftPanelScript
        self.rightPanelScript = rightPanelScript
        self.unifiedScript = unifiedScript
        self.contractName = contractName
        self.contractVersion = contractVersion
        self.permissions = permissions
        self.reloadNonce = reloadNonce
        self.leftPanelDefaultVisible = leftPanelDefaultVisible
        self.rightPanelDefaultVisible = rightPanelDefaultVisible
        self.backgroundColor = backgroundColor ?? Self.generateRandomColor()
        self.additionalProps = additionalProps
        self.hasUnreadMessage = hasUnreadMessage
        self.isBookmarked = isBookmarked
        self.status = status
    }

    /// 生成随机的十六进制颜色字符串（深色调）
    static func generateRandomColor() -> String {
        let hue = Double.random(in: 0...360)
        let saturation = Double.random(in: 0.5...0.7)  // 50-70%
        let lightness = Double.random(in: 0.15...0.2)  // 15-20%

        // HSL 转 RGB
        let rgb = hslToRGB(h: hue, s: saturation, l: lightness)

        // RGB 转十六进制
        let r = Int(rgb.r * 255)
        let g = Int(rgb.g * 255)
        let b = Int(rgb.b * 255)
        let hexColor = String(format: "#%02x%02x%02x", r, g, b)

        logger.debug("Generated random background color: \(hexColor) from HSL(\(Int(hue)), \(Int(saturation * 100))%, \(Int(lightness * 100))%)")
        return hexColor
    }

    /// HSL 转 RGB 辅助函数
    private static func hslToRGB(h: Double, s: Double, l: Double) -> (r: Double, g: Double, b: Double) {
        let c = (1 - abs(2 * l - 1)) * s
        let x = c * (1 - abs((h / 60).truncatingRemainder(dividingBy: 2) - 1))
        let m = l - c / 2

        var r: Double, g: Double, b: Double

        switch h {
        case 0..<60:
            (r, g, b) = (c, x, 0)
        case 60..<120:
            (r, g, b) = (x, c, 0)
        case 120..<180:
            (r, g, b) = (0, c, x)
        case 180..<240:
            (r, g, b) = (0, x, c)
        case 240..<300:
            (r, g, b) = (x, 0, c)
        default:
            (r, g, b) = (c, 0, x)
        }

        return (r + m, g + m, b + m)
    }
}

// MARK: - Persistence

/// Persisted tab data for saving to disk (only essential fields)
struct PersistedTab: Identifiable, Codable {
    let id: UUID
    var title: String
    var icon: String?
    var appUrl: URL?
    var appId: String?
    var hasUnreadMessage: Bool
    var isBookmarked: Bool
    var status: TabStatus

    init(id: UUID, title: String, icon: String?, appUrl: URL?, appId: String? = nil, hasUnreadMessage: Bool = false, isBookmarked: Bool = false, status: TabStatus = .unknown) {
        self.id = id
        self.title = title
        self.icon = icon
        self.appUrl = appUrl
        self.appId = appId
        self.hasUnreadMessage = hasUnreadMessage
        self.isBookmarked = isBookmarked
        self.status = status
    }

    init(from tab: Tab) {
        self.id = tab.id
        self.title = tab.title
        self.icon = tab.icon
        self.appUrl = tab.url
        self.appId = tab.appId
        self.hasUnreadMessage = tab.hasUnreadMessage
        self.isBookmarked = tab.isBookmarked
        self.status = tab.status
    }
}
