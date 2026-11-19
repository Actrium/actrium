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
    /// DispatchTime's monotonic uptime is shared by the Renderer and Host on this machine.
    public let deadlineUptimeNanoseconds: UInt64

    public init(
        id: UUID, documentId: UUID, operation: Operation, key: String, value: String?,
        deadlineUptimeNanoseconds: UInt64? = nil
    ) {
        self.id = id
        self.documentId = documentId
        self.operation = operation
        self.key = key
        self.value = value
        self.deadlineUptimeNanoseconds = deadlineUptimeNanoseconds
            ?? (DispatchTime.now() + .seconds(Self.timeoutSeconds)).uptimeNanoseconds
    }

    public var hasValidArguments: Bool {
        key.utf8.count <= 1024 && (value?.utf8.count ?? 0) <= 64 * 1024
            && (operation == .setItem) == (value != nil)
    }
}

public struct GuestLocalStorageReply: Codable, Sendable {
    public enum ErrorCode: String, Codable, Sendable {
        case timeout = "local_storage_timeout"
        case outcomeUnknown = "local_storage_outcome_unknown"
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
