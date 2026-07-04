import Foundation

private let logger = AppLogger.app

// MARK: - Tab Content Type

/// Native content types rendered by SwiftUI in the main process
enum NativeContentType: Equatable {
    case startPage
}

/// Tab 内容类型：决定使用哪种渲染引擎
enum TabContentType: Equatable {
    /// React Native 应用
    /// - bundleFingerprint: 应用指纹（相同指纹 = 相同应用 = 共享进程）
    case reactNative(bundleFingerprint: String)

    /// Web 内容（使用 WKWebView）
    case web(url: URL)

    /// Native SwiftUI content (runs in main process, no XPC needed)
    case native(type: NativeContentType)

    /// SSH terminal session
    case ssh(sessionId: String)

    /// SFTP file browser session
    case sftp(sessionId: String)

    /// Remote desktop session
    case remoteDesktop(sessionId: String)

    /// 空白 Tab（无内容）
    case empty
}

/// System tab types for built-in pages (like Chrome's chrome://history)
/// Uses about: URL scheme (e.g., about:history, about:bookmarks)
enum SystemTabType: String, Codable {
    case history
    case bookmarks
    case debug
    case startPage

    var title: String {
        switch self {
        case .history: return "History"
        case .bookmarks: return "Bookmarks"
        case .debug: return "Debug"
        case .startPage: return "New Tab"
        }
    }

    var icon: String {
        switch self {
        case .history: return "clock.arrow.circlepath"
        case .bookmarks: return "bookmark"
        case .debug: return "ladybug"
        case .startPage: return "house"
        }
    }

    /// The about: URL for this system tab type
    /// e.g., about:history, about:bookmarks, about:blank
    var aboutURL: URL {
        let path: String
        switch self {
        case .history: path = "history"
        case .bookmarks: path = "bookmarks"
        case .debug: path = "debug"
        case .startPage: path = "blank"
        }
        return URL(string: "about:\(path)")!
    }

    /// Parse a URL to get the corresponding SystemTabType
    /// Returns nil if the URL is not a valid about: URL for system apps
    static func from(url: URL) -> SystemTabType? {
        guard url.scheme == "about" else { return nil }
        // about: URLs have the path in the "path" component (e.g., about:history -> path = "history")
        // Note: URL(string: "about:history")?.path returns "history"
        let path = url.path.isEmpty ? url.absoluteString.replacingOccurrences(of: "about:", with: "") : url.path
        switch path {
        case "history": return .history
        case "bookmarks": return .bookmarks
        case "debug": return .debug
        case "blank": return .startPage
        default: return nil
        }
    }
}

struct Tab: Identifiable {
    let id = UUID()
    var title: String
    var url: URL?
    var appId: String?
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

    /// Custom app icon (SF Symbol name) - can be set by RCTNApp
    var appIcon: String?

    /// SSH session ID (set when navigating to ssh:// URL)
    var sshSessionId: String?

    /// SFTP session ID (set when navigating to sftp:// URL)
    var sftpSessionId: String?

    /// Remote desktop session ID (set when navigating to ardp:// URL)
    var remoteDesktopSessionId: String?

    init(
        title: String,
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
        leftPanelDefaultVisible: Bool = true,
        rightPanelDefaultVisible: Bool = false,
        backgroundColor: String? = nil,
        appIcon: String? = nil,
        sshSessionId: String? = nil,
        sftpSessionId: String? = nil,
        remoteDesktopSessionId: String? = nil
    ) {
        self.title = title
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
        self.backgroundColor = backgroundColor ?? "#1e1e1e"
        self.appIcon = appIcon
        self.sshSessionId = sshSessionId
        self.sftpSessionId = sftpSessionId
        self.remoteDesktopSessionId = remoteDesktopSessionId
    }

    // MARK: - Content Type Detection

    /// 计算 Tab 的内容类型
    /// 优先级：Web URL > RN 应用 > Native 系统页面 > 空白
    var contentType: TabContentType {
        let result: TabContentType

        // 1. 检查是否为 SSH session
        if let url = url, url.scheme == "ssh", let sessionId = sshSessionId {
            result = .ssh(sessionId: sessionId)
        }
        // 1b. 检查是否为 SFTP session
        else if let url = url, url.scheme == "sftp", let sessionId = sftpSessionId {
            result = .sftp(sessionId: sessionId)
        }
        // 1c. Check for remote desktop session
        else if let url = url, url.scheme == "ardp", let sessionId = remoteDesktopSessionId {
            result = .remoteDesktop(sessionId: sessionId)
        }
        // 2. 检查是否为 Web URL (http/https)
        else if let url = url, url.scheme == "http" || url.scheme == "https" {
            result = .web(url: url)
        }
        // 3. 检查是否为 RN 应用（有脚本内容）
        else if let fingerprint = bundleFingerprint {
            result = .reactNative(bundleFingerprint: fingerprint)
        }
        // 4. 系统 Tab - startPage 使用原生 SwiftUI，其他系统页面仍用 RN
        else if let systemType = systemTabType {
            switch systemType {
            case .startPage:
                result = .native(type: .startPage)
            case .history, .bookmarks, .debug:
                result = .reactNative(bundleFingerprint: "system.\(systemType.rawValue)")
            }
        }
        // 5. 空白 Tab
        else {
            result = .empty
        }

        return result
    }

    /// RN 应用的 bundle 指纹
    /// 用于标识同一应用（相同指纹 = 相同应用 = 共享 XPC 进程）
    var bundleFingerprint: String? {
        // 使用 unifiedScript 的 hash 作为指纹
        if let script = unifiedScript {
            return "script.\(script.stableHash)"
        }

        // Legacy: 使用 leftPanelScript 或 guestScript
        if let script = leftPanelScript ?? guestScript {
            return "legacy.\(script.stableHash)"
        }

        // contractName 是 Host/Guest 兼容合约名，不是“应用身份”。
        // 不能用它做进程指纹，否则不同应用只要使用同一合约就会被错误地分配到同一进程，
        // 导致 Overview/Tab 内容与标题错位（严重）。
        if let name = contractName {
            return "contract.\(name)"
        }

        return nil
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
