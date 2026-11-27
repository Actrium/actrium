import Foundation

public struct RealmServiceCall: Codable, Sendable {
    public static let requestEvent = "com.actrium.realmService.request"
    public static let responseEvent = "com.actrium.realmService.response"
    public static let maxStreamChunkBytes = GuestFileImportCall.maxBytes

    public let id: UUID
    public let route: String
    public let payload: Data
    public let timeoutMs: Int
    public let streamChunk: StreamChunk?

    public struct StreamChunk: Codable, Sendable {
        public let id: String
        public let sequence: UInt64

        public init(id: String, sequence: UInt64) {
            self.id = id
            self.sequence = sequence
        }
    }

    public init(id: UUID, route: String, payload: Data, timeoutMs: Int, streamChunk: StreamChunk? = nil) {
        self.id = id
        self.route = route
        self.payload = payload
        self.timeoutMs = timeoutMs
        self.streamChunk = streamChunk
    }
}

public struct RealmServiceFailure: Codable, Sendable {
    public let code: String
    public let message: String
    public let domain: String?
    public let details: [String: String]?

    public init(code: String, message: String, domain: String? = nil, details: [String: String]? = nil) {
        self.code = code
        self.message = message
        self.domain = domain
        self.details = details
    }

    public init(error: Error) {
        let source = error as NSError
        let details = source.userInfo.mapValues { $0 as? String ?? String(reflecting: $0) }
        self.init(code: String(source.code), message: source.localizedDescription, domain: source.domain, details: details)
    }
}

public struct RealmServiceReply: Codable, Sendable {
    public let id: UUID
    public let payload: Data?
    public let error: String?
    public let failure: RealmServiceFailure?

    public init(id: UUID, payload: Data? = nil, error: String? = nil) {
        self.id = id
        self.payload = payload
        self.error = error
        self.failure = nil
    }

    public init(id: UUID, failure: RealmServiceFailure) {
        self.id = id
        self.payload = nil
        self.error = failure.message
        self.failure = failure
    }
}
