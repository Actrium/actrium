import Foundation

public struct GuestLocalStorageCall: Codable, Sendable {
    public static let requestEvent = "com.actrium.localStorage.request"
    public static let responseEvent = "com.actrium.localStorage.response"
    public static let timeoutSeconds = 5
    public static let maxRequestBytes = 7 * 1024 * 1024
    public static let maxSingleRequestBytes = 512 * 1024

    public enum Operation: String, Codable, CaseIterable, Sendable {
        case getItem
        case setItem
        case removeItem
        case replaceItems
    }

    public struct Replacement: Codable, Sendable {
        public struct Item: Codable, Sendable {
            public let key: String
            public let value: String
        }

        public let expected: String?
        public let remove: [String]
        public let set: [Item]

        fileprivate var hasValidArguments: Bool {
            remove.count <= 1024 && set.count <= 1024 &&
                (expected?.utf8.count ?? 0) <= 64 * 1024 &&
                remove.allSatisfy { $0.utf8.count <= 1024 } &&
                set.allSatisfy { $0.key.utf8.count <= 1024 && $0.value.utf8.count <= 64 * 1024 }
        }
    }

    public let id: UUID
    public let documentId: UUID
    public let operation: Operation
    public let key: String
    public let value: String?
    public let replacement: Replacement?
    /// DispatchTime's monotonic uptime is shared by the Renderer and Host on this machine.
    public let deadlineUptimeNanoseconds: UInt64

    public init(
        id: UUID, documentId: UUID, operation: Operation, key: String, value: String?,
        replacement: Replacement? = nil, deadlineUptimeNanoseconds: UInt64? = nil
    ) {
        self.id = id
        self.documentId = documentId
        self.operation = operation
        self.key = key
        self.value = value
        self.replacement = replacement
        self.deadlineUptimeNanoseconds = deadlineUptimeNanoseconds
            ?? (DispatchTime.now() + .seconds(Self.timeoutSeconds)).uptimeNanoseconds
    }

    public var hasValidArguments: Bool {
        guard key.utf8.count <= 1024 && (value?.utf8.count ?? 0) <= 64 * 1024,
              (operation == .setItem) == (value != nil) else { return false }
        if operation == .replaceItems { return replacement?.hasValidArguments == true }
        return replacement == nil
    }
}

public struct GuestLocalStorageReply: Codable, Sendable {
    public enum ErrorCode: String, Codable, Sendable {
        case timeout = "local_storage_timeout"
        case outcomeUnknown = "local_storage_outcome_unknown"
        case quotaExceeded = "local_storage_quota_exceeded"
        case conflict = "local_storage_conflict"
    }

    public let id: UUID
    public let value: String?
    public let error: String?
    public let errorCode: ErrorCode?

    public init(id: UUID, value: String? = nil, error: String? = nil, errorCode: ErrorCode? = nil) {
        self.id = id
        self.value = value
        self.error = error
        self.errorCode = errorCode
    }

    public static func expiredBeforeStart(id: UUID) -> Self {
        Self(
            id: id, error: "The local storage operation timed out before it started; this request made no changes.",
            errorCode: .timeout
        )
    }

    /// A missing write reply cannot prove that the database operation failed.
    public static func timedOut(_ call: GuestLocalStorageCall) -> Self {
        if call.operation == .getItem {
            return Self(id: call.id, error: "The local storage read timed out.", errorCode: .timeout)
        }
        return Self(
            id: call.id,
            error: "The local storage write timed out. Its result is unknown; it may still take effect.",
            errorCode: .outcomeUnknown
        )
    }
}
