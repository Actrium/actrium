import Foundation

public enum RealmServiceAttachmentIPC {
    public static let chunkSizeBytes = 48 * 1024
    public static let maxBytes = 7 * 1024 * 1024
    public static let prepareTimeoutMs = 20_000
    public static let prepareRoute = "ask.AskService.PrepareAttachmentUpload"
}

/// Internal availability probe. The renderer queries the Host whether the
/// owning tab actually has a Realm Service binding that permits the fixed upload
/// transport; browser-wide module presence is not enough.
public struct RealmServiceAttachmentAvailabilityCall: Codable, Sendable {
    public static let requestEvent = "com.actrium.realmService.attachment.availability.request"
    public static let responseEvent = "com.actrium.realmService.attachment.availability.response"

    public let id: UUID

    public init(id: UUID = UUID()) {
        self.id = id
    }
}

public struct RealmServiceAttachmentAvailabilityReply: Codable, Sendable {
    public let id: UUID
    public let available: Bool
    public let failure: RealmServiceFailure?

    public init(id: UUID, available: Bool) {
        self.id = id
        self.available = available
        self.failure = nil
    }

    public init(id: UUID, failure: RealmServiceFailure) {
        self.id = id
        self.available = false
        self.failure = failure
    }
}

public struct RealmServiceAttachmentMetadata: Codable, Sendable, Equatable {
    public let key: String
    public let value: String

    public init(key: String, value: String) {
        self.key = key
        self.value = value
    }
}

public struct RealmServiceAttachmentPrepareCall: Codable, Sendable {
    public static let requestEvent = "com.actrium.realmService.attachment.prepare.request"
    public static let responseEvent = "com.actrium.realmService.attachment.prepare.response"

    public let id: UUID
    public let attachmentId: String
    public let filename: String
    public let type: Int32
    public let streamId: String
    public let checksum: String
    public let checksumAlgorithm: String
    public let sizeBytes: UInt64
    public let resultStreamId: String

    public init(
        id: UUID = UUID(), attachmentId: String, filename: String, type: Int32,
        streamId: String, checksum: String, checksumAlgorithm: String,
        sizeBytes: UInt64, resultStreamId: String
    ) {
        self.id = id
        self.attachmentId = attachmentId
        self.filename = filename
        self.type = type
        self.streamId = streamId
        self.checksum = checksum
        self.checksumAlgorithm = checksumAlgorithm
        self.sizeBytes = sizeBytes
        self.resultStreamId = resultStreamId
    }
}

public struct RealmServiceAttachmentPrepareReply: Codable, Sendable {
    public let id: UUID
    public let attachmentId: String?
    public let statusCode: Int32
    public let errorMessage: String?
    public let failure: RealmServiceFailure?

    public init(id: UUID, attachmentId: String, statusCode: Int32, errorMessage: String? = nil) {
        self.id = id
        self.attachmentId = attachmentId
        self.statusCode = statusCode
        self.errorMessage = errorMessage
        self.failure = nil
    }

    public init(id: UUID, failure: RealmServiceFailure) {
        self.id = id
        self.attachmentId = nil
        self.statusCode = -1
        self.errorMessage = failure.message
        self.failure = failure
    }
}

public struct RealmServiceAttachmentChunkCall: Codable, Sendable {
    public static let requestEvent = "com.actrium.realmService.attachment.chunk.request"
    public static let responseEvent = "com.actrium.realmService.attachment.chunk.response"

    public let id: UUID
    public let streamId: String
    public let attachmentId: String
    public let sequence: UInt64
    public let payload: Data
    public let metadata: [RealmServiceAttachmentMetadata]

    public init(
        id: UUID = UUID(), streamId: String, attachmentId: String, sequence: UInt64,
        payload: Data, metadata: [RealmServiceAttachmentMetadata] = []
    ) {
        self.id = id
        self.streamId = streamId
        self.attachmentId = attachmentId
        self.sequence = sequence
        self.payload = payload
        self.metadata = metadata
    }
}

public struct RealmServiceAttachmentChunkReply: Codable, Sendable {
    public let id: UUID
    public let failure: RealmServiceFailure?

    public init(id: UUID) {
        self.id = id
        self.failure = nil
    }

    public init(id: UUID, failure: RealmServiceFailure) {
        self.id = id
        self.failure = failure
    }
}

public struct RealmServiceAttachmentUnregisterCall: Codable, Sendable {
    public static let requestEvent = "com.actrium.realmService.attachment.unregister.request"
    public static let responseEvent = "com.actrium.realmService.attachment.unregister.response"

    public let id: UUID
    public let streamId: String

    public init(id: UUID = UUID(), streamId: String) {
        self.id = id
        self.streamId = streamId
    }
}

public struct RealmServiceAttachmentUnregisterReply: Codable, Sendable {
    public let id: UUID
    public let failure: RealmServiceFailure?

    public init(id: UUID) {
        self.id = id
        self.failure = nil
    }

    public init(id: UUID, failure: RealmServiceFailure) {
        self.id = id
        self.failure = failure
    }
}

public struct RealmServiceAttachmentResultEvent: Codable, Sendable {
    public static let event = "com.actrium.realmService.attachment.result"
    public static let nativeEventName = "RealmServiceAttachmentResult"

    public let streamId: String
    public let attachmentId: String
    public let status: Int32
    public let statusCode: Int32
    public let errorMessage: String?

    public init(
        streamId: String, attachmentId: String, status: Int32,
        statusCode: Int32, errorMessage: String? = nil
    ) {
        self.streamId = streamId
        self.attachmentId = attachmentId
        self.status = status
        self.statusCode = statusCode
        self.errorMessage = errorMessage
    }
}
