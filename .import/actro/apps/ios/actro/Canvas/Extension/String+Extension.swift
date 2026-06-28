import Foundation
import CommonCrypto
import SwiftUI

extension String {
    func markdownPlainText() -> String {
        var text = self

        // 1. 去掉代码块 ```...```
        text = text.replacingOccurrences(
            of: "```[\\s\\S]*?```",
            with: "",
            options: .regularExpression
        )

        // 2. 去掉行内代码 `...`
        text = text.replacingOccurrences(
            of: "`([^`]*)`",
            with: "$1",
            options: .regularExpression
        )

        // 3. 去掉粗体 **...** 或者 __...__
        text = text.replacingOccurrences(
            of: "(\\*\\*|__)(.*?)\\1",
            with: "$2",
            options: .regularExpression
        )

        // 4. 去掉斜体 *...* 或者 _..._
        text = text.replacingOccurrences(
            of: "(\\*|_)(.*?)\\1",
            with: "$2",
            options: .regularExpression
        )

        // 5. 去掉标题 #、##、### 等
        text = text.replacingOccurrences(
            of: "(?m)^#{1,6}\\s+",
            with: "",
            options: .regularExpression
        )

        // 6. 去掉引用 >
        text = text.replacingOccurrences(
            of: "^>\\s?",
            with: "",
            options: [.regularExpression]
        )

        // 7. 去掉无序列表符号 - + *
        text = text.replacingOccurrences(
            of: "^[\\-\\+\\*]\\s+",
            with: "",
            options: [.regularExpression]
        )

        // 9. 去掉链接 [text](url)
        text = text.replacingOccurrences(
            of: "\\[(.*?)\\]\\([^)]*\\)",
            with: "$1",
            options: .regularExpression
        )

        // 10. 去掉图片 ![alt](url)
        text = text.replacingOccurrences(
            of: "!\\[(.*?)\\]\\([^)]*\\)",
            with: "$1",
            options: .regularExpression
        )

        // 11. 去掉表格格式
        text = text.removeTableFormat()

        // 12. 去掉多余空白
        text = text.replacingOccurrences(
            of: "\\s+",
            with: " ",
            options: .regularExpression
        )

        return text.trimmingCharacters(in: .whitespacesAndNewlines)
    }
    
    /// 去除 Markdown 表格内容和格式
    func removeTableFormat() -> String {
        var text = self
        
        // 匹配并删除整个表格（包括表头、分隔行和数据行）
        // 表格通常由连续的以 | 开始或包含 | 的行组成
        text = text.replacingOccurrences(
            of: "(?m)(?:^\\|.*\\|\\s*$\\n?)+",
            with: "",
            options: .regularExpression
        )
        
        // 清理可能剩余的分隔行（如 |---|---|---| 格式）
        text = text.replacingOccurrences(
            of: "(?m)^\\s*[\\|:\\-\\s]+\\s*$\\n?",
            with: "",
            options: .regularExpression
        )
        
        return text
    }
    
    /// 过滤字符串前后的空格
    func removingSpaces() -> String {
        return self.trimmingCharacters(in: .whitespaces)
    }
    
    /// 计算字符串的SHA256哈希值
    var sha256: String {
        guard let data = self.data(using: .utf8) else { return "" }
        
        var digest = Data(count: Int(CC_SHA256_DIGEST_LENGTH))
        _ = digest.withUnsafeMutableBytes { bytes in
            data.withUnsafeBytes { dataBytes in
                CC_SHA256(dataBytes.bindMemory(to: UInt8.self).baseAddress, CC_LONG(data.count), bytes.bindMemory(to: UInt8.self).baseAddress)
            }
        }
        
        return digest.map { String(format: "%02hhx", $0) }.joined()
    }
    
    var messageWith: CGFloat {
        let attributes = [NSAttributedString.Key.font: UIFont.systemFont(ofSize: 19)]
        let size = self.boundingRect(
            with: CGSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude),
            options: .usesLineFragmentOrigin,
            attributes: attributes,
            context: nil
        ).size
        let tmpWidth = ceil(size.width) > UIScreen.main.bounds.width - 58
        ? UIScreen.main.bounds.width - 58
        : ceil(size.width)
        let receiveWidth = tmpWidth < 129 ? 129 : tmpWidth
        return receiveWidth
    }
}
