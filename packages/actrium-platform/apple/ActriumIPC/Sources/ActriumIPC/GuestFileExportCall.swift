import Foundation

public struct GuestFileExportCall: Codable, Sendable {
    public static let capability = "host:file-export.save"
    public static let requestEvent = "com.actrium.fileExport.request"
    public static let responseEvent = "com.actrium.fileExport.response"
    public static let timeoutSeconds = 300
    public static let maxBytes = 7 * 1024 * 1024
    public static let maxBase64Bytes = ((maxBytes + 2) / 3) * 4

    public let id: UUID
    public let reloadKey: Int
    public let name: String
    public let data: Data

    public init(reloadKey: Int, name: String, data: Data) {
        self.id = UUID()
        self.reloadKey = reloadKey
        self.name = name
        self.data = data
    }

    public var hasValidArguments: Bool {
        reloadKey >= 0 && (1...255).contains(name.utf8.count) &&
            name != "." && name != ".." &&
            !name.contains("/") && !name.contains("\\") &&
            name.rangeOfCharacter(from: .controlCharacters) == nil &&
            data.count <= Self.maxBytes
    }
}

public struct GuestFileExportReply: Codable, Sendable {
    public let id: UUID
    public let saved: Bool
    public let error: String?

    public init(id: UUID, saved: Bool = false, error: String? = nil) {
        self.id = id
        self.saved = saved
        self.error = error
    }
}
