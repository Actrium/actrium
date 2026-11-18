import Foundation

public struct GuestFileImportCall: Codable, Sendable {
    public static let capability = "host:file-import.pick"
    public static let requestEvent = "com.actrium.fileImport.request"
    public static let responseEvent = "com.actrium.fileImport.response"
    public static let timeoutSeconds = 300
    public static let maxRequestBytes = 32 * 1024

    // Leave room for Base64, JSON and protobuf framing in the 10,000,000-byte IPC limit.
    public static let maxBytes = 7 * 1024 * 1024

    public enum Source: String, Codable, Sendable {
        case files
        case photos
    }

    public let id: UUID
    public let reloadKey: Int
    public let source: Source
    public let accept: [String]
    public let multiple: Bool

    public init(reloadKey: Int, source: Source, accept: [String], multiple: Bool) {
        id = UUID()
        self.reloadKey = reloadKey
        self.source = source
        self.accept = accept
        self.multiple = multiple
    }

    public var hasValidArguments: Bool {
        reloadKey >= 0 && (source == .files || accept.isEmpty)
            && accept.count <= 32 && accept.allSatisfy { (1...128).contains($0.utf8.count) }
    }
}

public struct GuestFileImportReply: Codable, Sendable {
    public struct File: Codable, Sendable {
        public let name: String
        public let data: Data

        public init(name: String, data: Data) {
            self.name = name
            self.data = data
        }
    }

    public let id: UUID
    public let files: [File]
    public let error: String?

    public init(id: UUID, files: [File] = [], error: String? = nil) {
        self.id = id
        self.files = files
        self.error = error
    }

    public func encodedJSON() throws -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = .withoutEscapingSlashes
        return String(decoding: try encoder.encode(self), as: UTF8.self)
    }
}
