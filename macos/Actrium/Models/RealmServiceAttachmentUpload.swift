import ActrBindings
import ActriumIPC
import Foundation
import SwiftProtobuf

/// Terminal/validation failures for the attachment upload transport. Every
/// case maps onto a `RealmServiceFailure` so the renderer's Promise rejections
/// carry a stable code instead of a generic transport error.
enum RealmServiceAttachmentUploadError: LocalizedError, Equatable {
    case notPermitted
    case invalidArgument(String)
    case sessionConflict(String)
    case unknownStream(String)
    case notAChunkStream(String)
    case stillPreparing(String)
    case streamTerminated(String)
    case unavailable(String)
    case rejected(Int32, String?)
    case cancelled

    var errorDescription: String? { message }

    var message: String {
        switch self {
        case .notPermitted:
            return "This tab is not permitted to upload attachments."
        case .invalidArgument(let message),
             .sessionConflict(let message),
             .unknownStream(let message),
             .notAChunkStream(let message),
             .stillPreparing(let message),
             .streamTerminated(let message),
             .unavailable(let message):
            return message
        case .rejected(let statusCode, let message):
            return message ?? "The Service rejected the attachment upload (statusCode \(statusCode))."
        case .cancelled:
            return "The attachment session was cancelled."
        }
    }

    var code: String {
        switch self {
        case .notPermitted: return "permission_denied"
        case .invalidArgument: return "invalid_argument"
        case .sessionConflict: return "session_conflict"
        case .unknownStream: return "unknown_stream"
        case .notAChunkStream: return "not_a_chunk_stream"
        case .stillPreparing: return "still_preparing"
        case .streamTerminated: return "stream_terminated"
        case .unavailable: return "unavailable"
        case .rejected: return "attachment_prepare_rejected"
        case .cancelled: return "cancelled"
        }
    }

    var failure: RealmServiceFailure {
        RealmServiceFailure(code: code, message: message)
    }
}

/// Static policy for the fixed `host:service.uploadAttachment` transport:
/// capability gating, field bounds, and the checksum/sequence rules shared by
/// the Host binding. Everything here is pure so it can be unit tested without
/// a running actor node.
enum RealmServiceAttachmentUploadPolicy {
    /// Manifest permission that unlocks the transport.
    static let permission = "service.attachmentUpload"
    /// Declared `hostModules.service.rpc` entry that reserves the transport.
    /// It is verified at prepare time but never exposed as a business route.
    static let transportRoute = "uploadAttachment"

    static let md5HexLength = 32
    static let maxIdBytes = 128
    static let maxFilenameBytes = 256
    static let maxMetadataEntries = 8
    static let maxMetadataKeyBytes = 128
    static let maxMetadataValueBytes = 256

    /// Metadata entry that terminates an upload stream.
    static let endOfStreamMetadata = RealmServiceAttachmentMetadata(key: "eos", value: "true")

    static let validTypeRawValues: Set<Int32> = [
        Int32(Ask_AttachmentType.image.rawValue),
        Int32(Ask_AttachmentType.document.rawValue),
        Int32(Ask_AttachmentType.audio.rawValue),
        Int32(Ask_AttachmentType.video.rawValue),
        Int32(Ask_AttachmentType.other.rawValue),
    ]

    static func isPermitted(permissions: [String], declaredRoutes: Set<String>) -> Bool {
        // Match the TS capability expander exactly: the method capability,
        // its contract id, and the two module aliases that cover every
        // Service method. Never accept a substring or unrelated permission.
        let accepted: Set<String> = [
            permission,
            "host:service.uploadAttachment",
            "service",
            "host:service",
        ]
        return permissions.contains(where: accepted.contains)
            && declaredRoutes.contains(transportRoute)
    }

    /// Business routes must exclude the attachment transport: upload
    /// attachment is only reachable through the dedicated prepare/chunk
    /// pipeline, never through the generic RealmService.call path.
    static func businessRoutes(_ declaredRoutes: Set<String>) -> Set<String> {
        declaredRoutes.subtracting([transportRoute])
    }

    /// Maps a validated raw type value onto the wire enum. Caller must have
    /// already verified membership in `validTypeRawValues`.
    static func attachmentType(for rawValue: Int32) -> Ask_AttachmentType {
        Ask_AttachmentType(rawValue: Int(rawValue)) ?? .other
    }

    /// Strict RFC 4648 (padded) base64 used on every base64 boundary.
    static func strictBase64Data(_ text: String) -> Data? {
        if text.isEmpty { return Data() }
        var alphabet = Set<Unicode.Scalar>("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/".unicodeScalars)
        let padding: Unicode.Scalar = "="
        alphabet.insert(padding)

        let scalars = Array(text.unicodeScalars)
        guard scalars.count % 4 == 0, scalars.allSatisfy({ alphabet.contains($0) }) else {
            return nil
        }
        guard let firstPadding = scalars.firstIndex(of: padding) else {
            return Data(base64Encoded: text)
        }
        let tail = scalars[firstPadding...]
        guard tail.allSatisfy({ $0 == padding }), tail.count <= 2 else {
            return nil
        }
        return Data(base64Encoded: text)
    }

    /// Validates and normalizes a prepare call. Returns the lowercased MD5
    /// checksum to place on the wire.
    static func validatePrepare(_ call: RealmServiceAttachmentPrepareCall) throws -> String {
        guard (1...maxIdBytes).contains(call.attachmentId.utf8.count) else {
            throw RealmServiceAttachmentUploadError.invalidArgument(
                "attachmentId must be 1...\(maxIdBytes) bytes.")
        }
        guard (1...maxFilenameBytes).contains(call.filename.utf8.count) else {
            throw RealmServiceAttachmentUploadError.invalidArgument(
                "filename must be 1...\(maxFilenameBytes) bytes.")
        }
        guard Self.validTypeRawValues.contains(call.type) else {
            throw RealmServiceAttachmentUploadError.invalidArgument(
                "type must be one of image, document, audio, video, other.")
        }
        guard (1...maxIdBytes).contains(call.streamId.utf8.count),
              (1...maxIdBytes).contains(call.resultStreamId.utf8.count) else {
            throw RealmServiceAttachmentUploadError.invalidArgument(
                "streamId and resultStreamId must each be 1...\(maxIdBytes) bytes.")
        }
        guard call.streamId != call.resultStreamId else {
            throw RealmServiceAttachmentUploadError.invalidArgument(
                "streamId and resultStreamId must be different.")
        }
        let checksum = call.checksum.lowercased()
        guard checksum.utf8.count == md5HexLength,
              checksum.utf8.allSatisfy({ byte in
                  (48...57).contains(byte) || (97...102).contains(byte)
              }) else {
            throw RealmServiceAttachmentUploadError.invalidArgument(
                "checksum must be a 32 character lowercase hex MD5 digest.")
        }
        guard call.checksumAlgorithm == "md5" else {
            throw RealmServiceAttachmentUploadError.invalidArgument(
                "checksumAlgorithm must be md5.")
        }
        guard call.sizeBytes <= UInt64(RealmServiceAttachmentIPC.maxBytes) else {
            throw RealmServiceAttachmentUploadError.invalidArgument(
                "sizeBytes must be at most \(RealmServiceAttachmentIPC.maxBytes).")
        }
        return checksum
    }

    /// Validates chunk metadata shape. The end-of-stream entry may only
    /// appear alone, on the terminal chunk.
    static func validateChunkMetadata(_ metadata: [RealmServiceAttachmentMetadata]) throws -> Bool {
        guard !metadata.isEmpty else { return false }
        guard metadata.count == 1, metadata[0] == endOfStreamMetadata else {
            throw RealmServiceAttachmentUploadError.invalidArgument(
                "metadata is only allowed as a single eos=true entry on the final chunk.")
        }
        return true
    }
}

/// Immutable per-upload state. One session binds the chunk stream, the
/// result stream, the discovered peer (fixed at prepare time) and the byte
/// accounting used to enforce the upload contract.
struct RealmServiceAttachmentSession: Equatable {
    enum State: String, Equatable {
        /// Result stream registered, prepare RPC still in flight.
        case preparing
        /// Prepare accepted; chunks may flow.
        case uploading
    }

    let attachmentId: String
    let chunkStreamId: String
    let resultStreamId: String
    let sizeBytes: UInt64
    let checksum: String
    var target: ActrId?
    var state: State
    var nextSequence: UInt64
    var receivedBytes: UInt64
    var reachedEndOfStream: Bool
}

/// How one chunk must be transported after validation.
struct RealmServiceAttachmentChunkPlan: Equatable {
    let target: ActrId
    let sequence: UInt64
    let isEndOfStream: Bool
}

/// Serializes the upload lifecycle of one Realm Service binding. The store
/// owns no transport: callers hand it validated inputs, receive a plan, and
/// perform the actual FFI/RPC work.
actor RealmServiceAttachmentSessionStore {
    private var sessions: [String: RealmServiceAttachmentSession] = [:]
    private var chunkStreams: [String: String] = [:]
    private var resultStreams: [String: String] = [:]

    init() {}

    /// Registers a session before the result stream goes live. Fails when
    /// either stream id collides with a live session of this binding.
    func registerPreparing(_ call: RealmServiceAttachmentPrepareCall, checksum: String) throws {
        guard sessions[call.attachmentId] == nil else {
            throw RealmServiceAttachmentUploadError.sessionConflict(
                "attachmentId \(call.attachmentId) already has an active upload.")
        }
        guard chunkStreams[call.streamId] == nil, resultStreams[call.streamId] == nil else {
            throw RealmServiceAttachmentUploadError.sessionConflict(
                "streamId \(call.streamId) is already in use by another upload.")
        }
        guard chunkStreams[call.resultStreamId] == nil, resultStreams[call.resultStreamId] == nil else {
            throw RealmServiceAttachmentUploadError.sessionConflict(
                "resultStreamId \(call.resultStreamId) is already in use by another upload.")
        }
        sessions[call.attachmentId] = RealmServiceAttachmentSession(
            attachmentId: call.attachmentId,
            chunkStreamId: call.streamId,
            resultStreamId: call.resultStreamId,
            sizeBytes: call.sizeBytes,
            checksum: checksum,
            target: nil,
            state: .preparing,
            nextSequence: 1,
            receivedBytes: 0,
            reachedEndOfStream: false
        )
        chunkStreams[call.streamId] = call.attachmentId
        resultStreams[call.resultStreamId] = call.attachmentId
    }

    /// Fixes the discovered peer once the prepare RPC succeeded.
    func markUploading(attachmentId: String, target: ActrId) throws {
        guard var session = sessions[attachmentId] else {
            throw RealmServiceAttachmentUploadError.unknownStream(
                "No attachment session for \(attachmentId).")
        }
        session.target = target
        session.state = .uploading
        sessions[attachmentId] = session
    }

    /// Validates a chunk and advances the session accounting. The mutation is
    /// optimistic: if the caller's transport send fails it must cancel the
    /// session, because the sequence window has already moved.
    ///
    /// End-of-stream is carried by the final data chunk itself: its payload
    /// completes the declared byte count and its metadata is exactly
    /// `eos=true`. An empty file is a single sequence-1 chunk with an empty
    /// payload and the eos metadata. No extra empty EOS chunk exists.
    func planChunk(
        streamId: String,
        sequence: UInt64,
        payloadBytes: Int,
        metadata: [RealmServiceAttachmentMetadata]
    ) throws -> (session: RealmServiceAttachmentSession, plan: RealmServiceAttachmentChunkPlan) {
        guard let attachmentId = chunkStreams[streamId], var session = sessions[attachmentId] else {
            if resultStreams[streamId] != nil {
                throw RealmServiceAttachmentUploadError.notAChunkStream(
                    "streamId \(streamId) is a result stream; chunks must use the prepare-registered chunk stream.")
            }
            throw RealmServiceAttachmentUploadError.unknownStream(
                "No attachment session for stream \(streamId).")
        }
        guard session.state == .uploading, let target = session.target else {
            throw RealmServiceAttachmentUploadError.stillPreparing(
                "The attachment upload has not finished preparing.")
        }
        guard !session.reachedEndOfStream else {
            throw RealmServiceAttachmentUploadError.streamTerminated(
                "The upload stream is already terminated.")
        }
        guard sequence == session.nextSequence else {
            throw RealmServiceAttachmentUploadError.invalidArgument(
                "Expected chunk sequence \(session.nextSequence), got \(sequence).")
        }
        guard payloadBytes <= RealmServiceAttachmentIPC.chunkSizeBytes else {
            throw RealmServiceAttachmentUploadError.invalidArgument(
                "Chunk payload must be at most \(RealmServiceAttachmentIPC.chunkSizeBytes) bytes.")
        }
        let isEndOfStream = try RealmServiceAttachmentUploadPolicy.validateChunkMetadata(metadata)
        let updatedBytes = session.receivedBytes + UInt64(payloadBytes)
        if isEndOfStream {
            guard updatedBytes == session.sizeBytes else {
                throw RealmServiceAttachmentUploadError.invalidArgument(
                    "End of stream must reach exactly the declared size: got \(updatedBytes) of \(session.sizeBytes) bytes.")
            }
            session.receivedBytes = updatedBytes
            session.reachedEndOfStream = true
        } else {
            guard updatedBytes < session.sizeBytes else {
                throw RealmServiceAttachmentUploadError.invalidArgument(
                    "The final chunk must carry the eos metadata and reach exactly the declared size.")
            }
            session.receivedBytes = updatedBytes
        }
        let plan = RealmServiceAttachmentChunkPlan(target: target, sequence: sequence, isEndOfStream: isEndOfStream)
        session.nextSequence += 1
        sessions[attachmentId] = session
        return (session, plan)
    }

    /// Looks up the session consuming a result stream.
    func session(forResultStreamId streamId: String) -> RealmServiceAttachmentSession? {
        guard let attachmentId = resultStreams[streamId] else { return nil }
        return sessions[attachmentId]
    }

    /// Terminal cleanup after a COMPLETED/FAILED result. Returns the removed
    /// session so the caller can unregister both streams.
    func finish(attachmentId: String) -> RealmServiceAttachmentSession? {
        remove(attachmentId: attachmentId)
    }

    /// Idempotent per-session cancellation.
    func cancel(attachmentId: String) -> RealmServiceAttachmentSession? {
        remove(attachmentId: attachmentId)
    }

    /// Removes the whole session referenced by either its chunk stream or
    /// its result stream. Unknown stream ids are a no-op so repeated
    /// unregistrations stay idempotent.
    func unregister(streamId: String) -> RealmServiceAttachmentSession? {
        if let attachmentId = chunkStreams[streamId] ?? resultStreams[streamId] {
            return remove(attachmentId: attachmentId)
        }
        return nil
    }

    /// Drops every session (renderer replacement, binding stop).
    func cancelAll() -> [RealmServiceAttachmentSession] {
        let all = sessions.values.sorted { $0.attachmentId < $1.attachmentId }
        sessions.removeAll()
        chunkStreams.removeAll()
        resultStreams.removeAll()
        return all
    }

    private func remove(attachmentId: String) -> RealmServiceAttachmentSession? {
        guard let session = sessions.removeValue(forKey: attachmentId) else { return nil }
        chunkStreams.removeValue(forKey: session.chunkStreamId)
        resultStreams.removeValue(forKey: session.resultStreamId)
        return session
    }
}
