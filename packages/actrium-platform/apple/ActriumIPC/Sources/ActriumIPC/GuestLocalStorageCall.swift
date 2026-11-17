import Foundation

public struct GuestLocalStorageCall: Codable, Sendable {
    public static let requestEvent = "com.actrium.localStorage.request"
    public static let responseEvent = "com.actrium.localStorage.response"
    public static let timeoutSeconds = 5

    public enum Operation: String, Codable, CaseIterable, Sendable {
        case getItem
        case setItem
        case removeItem
    }

    public let id: UUID
    public let documentId: UUID
    public let operation: Operation
    public let key: String
    public let value: String?

    public init(id: UUID, documentId: UUID, operation: Operation, key: String, value: String?) {
        self.id = id
        self.documentId = documentId
        self.operation = operation
        self.key = key
        self.value = value
    }

    public var hasValidArguments: Bool {
        key.utf8.count <= 1024 && (value?.utf8.count ?? 0) <= 64 * 1024
            && (operation == .setItem) == (value != nil)
    }
}

public struct GuestLocalStorageReply: Codable, Sendable {
    public let id: UUID
    public let value: String?
    public let error: String?

    public init(id: UUID, value: String? = nil, error: String? = nil) {
        self.id = id
        self.value = value
        self.error = error
    }
}
