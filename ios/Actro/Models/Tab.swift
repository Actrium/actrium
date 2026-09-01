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

    /// Guest application script (deprecated; use the panel scripts instead).
    var guestScript: String?

    /// Left panel script (menu.js) for the legacy dual-engine architecture.
    var leftPanelScript: String?

    /// Right extension panel script (ext.js) for the legacy dual-engine architecture.
    var rightPanelScript: String?

    /// v2.0 unified script (unified-app.js) for the single-engine architecture.
    var unifiedScript: String?

    /// Phase 1 contracts used for host prompts and version compatibility.
    var contractName: String?
    var contractVersion: Int?

    /// Phase 1 permissions used for host prompts and future gating.
    var permissions: [String]

    /// Reload counter passed to React Native as a remount key.
    var reloadNonce: Int

    /// Whether the left panel is visible by default.
    var leftPanelDefaultVisible: Bool

    /// Whether the right panel is visible by default.
    var rightPanelDefaultVisible: Bool

    /// Background color for this tab (Hex format: "#rrggbb")
    var backgroundColor: String

    /// App ID parsed from URL (e.g. "xxx" from https://go.actro.chat/app/xxx)
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

    /// Generates a random dark hexadecimal color.
    static func generateRandomColor() -> String {
        let hue = Double.random(in: 0...360)
        let saturation = Double.random(in: 0.5...0.7)  // 50-70%
        let lightness = Double.random(in: 0.15...0.2)  // 15-20%

        // Convert HSL to RGB.
        let rgb = hslToRGB(h: hue, s: saturation, l: lightness)

        // Convert RGB to hexadecimal.
        let r = Int(rgb.r * 255)
        let g = Int(rgb.g * 255)
        let b = Int(rgb.b * 255)
        let hexColor = String(format: "#%02x%02x%02x", r, g, b)

        logger.debug("Generated random background color: \(hexColor) from HSL(\(Int(hue)), \(Int(saturation * 100))%, \(Int(lightness * 100))%)")
        return hexColor
    }

    /// HSL-to-RGB helper.
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
