import Foundation
import SwiftUI

extension String {
    func markdownPlainText() -> String {
        var text = self

        // 1. Get the code blocks off. ```...```
        text = text.replacingOccurrences(
            of: "```[\\s\\S]*?```",
            with: "",
            options: .regularExpression
        )

        // 2. Remove line code `...`
        text = text.replacingOccurrences(
            of: "`([^`]*)`",
            with: "$1",
            options: .regularExpression
        )

        // 3. Get rid of the bold. **...** Or... __...__
        text = text.replacingOccurrences(
            of: "(\\*\\*|__)(.*?)\\1",
            with: "$2",
            options: .regularExpression
        )

        // 4. Get the italics off. *...* Or... _..._
        text = text.replacingOccurrences(
            of: "(\\*|_)(.*?)\\1",
            with: "$2",
            options: .regularExpression
        )

        // 5. Remove the title. #, ##, ### Wait.
        text = text.replacingOccurrences(
            of: "(?m)^#{1,6}\\s+",
            with: "",
            options: .regularExpression
        )

        // 6. Remove the quote. >
        text = text.replacingOccurrences(
            of: "^>\\s?",
            with: "",
            options: [.regularExpression]
        )

        // 7. Remove the unsequenceding symbol. - + *
        text = text.replacingOccurrences(
            of: "^[\\-\\+\\*]\\s+",
            with: "",
            options: [.regularExpression]
        )

        // 9. Remove the link. [text](url)
        text = text.replacingOccurrences(
            of: "\\[(.*?)\\]\\([^)]*\\)",
            with: "$1",
            options: .regularExpression
        )

        // 10. Get rid of the picture. ![alt](url)
        text = text.replacingOccurrences(
            of: "!\\[(.*?)\\]\\([^)]*\\)",
            with: "$1",
            options: .regularExpression
        )

        // 11. Remove Form Format
        text = text.removeTableFormat()

        // 12. Remove the extra space.
        text = text.replacingOccurrences(
            of: "\\s+",
            with: " ",
            options: .regularExpression
        )

        return text.trimmingCharacters(in: .whitespacesAndNewlines)
    }
    
    /// Remove Markdown Table content and format
    func removeTableFormat() -> String {
        var text = self
        
        // Match and delete the entire table (including headers, separated rows and data rows)
        // Tables are usually continuous by | Start or include | of the World Bank
        text = text.replacingOccurrences(
            of: "(?m)(?:^\\|.*\\|\\s*$\\n?)+",
            with: "",
            options: .regularExpression
        )
        
        // Clean up possible remaining separation lines (e.g., |---|---|---| Format)
        text = text.replacingOccurrences(
            of: "(?m)^\\s*[\\|:\\-\\s]+\\s*$\\n?",
            with: "",
            options: .regularExpression
        )
        
        return text
    }
    
    /// Spaces to filter strings
    func removingSpaces() -> String {
        return self.trimmingCharacters(in: .whitespaces)
    }
    
    var messageWith: CGFloat {
        let screenWidth = UIApplication.shared.connectedScenes
            .compactMap { ($0 as? UIWindowScene)?.screen.bounds.width }
            .first ?? 390
        let maxWidth = screenWidth - 58
        let attributes = [NSAttributedString.Key.font: UIFont.systemFont(ofSize: 19)]
        let size = self.boundingRect(
            with: CGSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude),
            options: .usesLineFragmentOrigin,
            attributes: attributes,
            context: nil
        ).size
        let tmpWidth = ceil(size.width) > maxWidth
        ? maxWidth
        : ceil(size.width)
        let receiveWidth = tmpWidth < 129 ? 129 : tmpWidth
        return receiveWidth
    }
}
