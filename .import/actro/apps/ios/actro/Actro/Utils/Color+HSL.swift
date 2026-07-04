import SwiftUI

extension Color {
    /// 从 HSL 字符串创建 Color
    /// 格式: "hsl(180, 50%, 25%)"
    init(hslString: String) {
        // 解析 HSL 字符串
        let pattern = #"hsl\((\d+),\s*(\d+)%,\s*(\d+)%\)"#
        guard let regex = try? NSRegularExpression(pattern: pattern),
              let match = regex.firstMatch(in: hslString, range: NSRange(hslString.startIndex..., in: hslString)),
              match.numberOfRanges == 4 else {
            // 解析失败，返回默认灰色
            self = Color(white: 0.15)
            return
        }

        // 提取 H, S, L 值
        let hRange = Range(match.range(at: 1), in: hslString)!
        let sRange = Range(match.range(at: 2), in: hslString)!
        let lRange = Range(match.range(at: 3), in: hslString)!

        let h = Double(hslString[hRange]) ?? 0
        let s = (Double(hslString[sRange]) ?? 0) / 100.0
        let l = (Double(hslString[lRange]) ?? 0) / 100.0

        // 转换 HSL 到 RGB
        let (r, g, b) = hslToRgb(h: h / 360.0, s: s, l: l)

        self = Color(red: r, green: g, blue: b)
    }
}

/// 将 HSL 转换为 RGB
/// - Parameters:
///   - h: 色调 (0-1)
///   - s: 饱和度 (0-1)
///   - l: 亮度 (0-1)
/// - Returns: RGB 值 (0-1, 0-1, 0-1)
private func hslToRgb(h: Double, s: Double, l: Double) -> (Double, Double, Double) {
    if s == 0 {
        return (l, l, l) // 灰度
    }
    
    let q = l < 0.5 ? l * (1 + s) : l + s - l * s
    let p = 2 * l - q
    
    func hueToRgb(_ p: Double, _ q: Double, _ t: Double) -> Double {
        var t = t
        if t < 0 { t += 1 }
        if t > 1 { t -= 1 }
        if t < 1/6 { return p + (q - p) * 6 * t }
        if t < 1/2 { return q }
        if t < 2/3 { return p + (q - p) * (2/3 - t) * 6 }
        return p
    }
    
    let r = hueToRgb(p, q, h + 1/3)
    let g = hueToRgb(p, q, h)
    let b = hueToRgb(p, q, h - 1/3)
    
    return (r, g, b)
}
