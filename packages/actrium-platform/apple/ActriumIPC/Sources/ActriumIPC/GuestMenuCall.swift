import Foundation

public struct GuestMenuCall: Codable, Sendable {
    public static let requestEvent = "com.actrium.guestMenu.request"
    public static let responseEvent = "com.actrium.guestMenu.response"
    public static let maxRequestBytes = 96 * 1024
    public static let maxEncodedRequestBytes = maxRequestBytes + 256

    /// Points relative to the top-left of the renderer tab, where the Guest is laid out.
    public struct Anchor: Codable, Sendable {
        public let x: Double
        public let y: Double
        public let width: Double
        public let height: Double

        var isValid: Bool {
            [x, y, width, height].allSatisfy { $0.isFinite && abs($0) <= 100_000 }
                && width > 0 && height > 0
        }
    }

    public struct Item: Codable, Sendable {
        public let value: String
        public let label: String
        public let disabled: Bool?

        var isValid: Bool {
            (1...128).contains(value.utf8.count)
                && (1...160).contains(label.utf8.count)
                && label.rangeOfCharacter(from: .controlCharacters) == nil
        }
    }

    public struct Content: Codable, Sendable {
        public enum Component: String, Codable, Sendable {
            case select = "Select"
            case menu = "Menu"
        }

        public let component: Component
        public let anchor: Anchor
        public let items: [Item]
        public let selectedValue: String?
    }

    public let id: UUID
    public let reloadKey: Int
    public let content: Content

    public init(reloadKey: Int, content: Content) {
        self.id = UUID()
        self.reloadKey = reloadKey
        self.content = content
    }

    public var hasValidArguments: Bool {
        let items = content.items
        return reloadKey >= 0 && content.anchor.isValid && (1...300).contains(items.count)
            && items.allSatisfy(\.isValid) && Set(items.map(\.value)).count == items.count
            && (content.selectedValue.map { value in items.contains { $0.value == value } } ?? true)
    }
}

public struct GuestMenuReply: Codable, Sendable {
    public let id: UUID
    public let value: String?
    public let error: String?

    public init(id: UUID, value: String? = nil, error: String? = nil) {
        self.id = id
        self.value = value
        self.error = error
    }
}
