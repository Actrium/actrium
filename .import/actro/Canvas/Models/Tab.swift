import AppKit

struct Tab: Identifiable {
    let id = UUID()
    var title: String
    var url: URL?
    var htmlContent: String?

    /// Guest 应用脚本 (已废弃，使用 leftPanelScript 和 rightPanelScript 代替)
    var guestScript: String?

    /// 左侧面板脚本 (menu.js)
    var leftPanelScript: String?

    /// 右侧扩展面板脚本 (ext.js) - 按需加载
    var rightPanelScript: String?

    /// 右侧面板默认是否可见
    var rightPanelDefaultVisible: Bool

    var snapshot: NSImage?
    /// Background color for this tab (Hex format: "#rrggbb")
    var backgroundColor: String

    init(
        title: String,
        url: URL? = nil,
        htmlContent: String? = nil,
        guestScript: String? = nil,
        leftPanelScript: String? = nil,
        rightPanelScript: String? = nil,
        rightPanelDefaultVisible: Bool = false,
        snapshot: NSImage? = nil,
        backgroundColor: String? = nil
    ) {
        self.title = title
        self.url = url
        self.htmlContent = htmlContent
        self.guestScript = guestScript
        self.leftPanelScript = leftPanelScript
        self.rightPanelScript = rightPanelScript
        self.rightPanelDefaultVisible = rightPanelDefaultVisible
        self.snapshot = snapshot
        self.backgroundColor = backgroundColor ?? Self.generateRandomColor()
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

        print("🎨 生成随机背景颜色: \(hexColor) (from HSL(\(Int(hue)), \(Int(saturation * 100))%, \(Int(lightness * 100))%))")
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
