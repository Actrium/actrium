import Foundation
import ActriumCore

// MARK: - Core-backed Defaults

extension Tag {
    /// Default colors for new tags.
    static let defaultColors: [String] = defaultTagColors()

    static let systemApp = systemTag(named: "App")
    static let systemPublisher = systemTag(named: "Publisher")
    static let systemLastUsed = systemTag(named: "LastUsed")
    static let systemFrequent = systemTag(named: "Frequent")

    private static func systemTag(named name: String) -> Tag {
        if let info = defaultSystemTags().first(where: { $0.name == name }) {
            return Tag(name: info.name, color: info.color, isSystem: info.isSystem)
        }
        return Tag(name: name, isSystem: true)
    }
}
