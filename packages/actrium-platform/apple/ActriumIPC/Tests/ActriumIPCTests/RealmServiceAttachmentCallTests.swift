import Foundation
import XCTest
@testable import ActriumIPC

final class RealmServiceAttachmentCallTests: XCTestCase {
    func testAttachmentProtobufRoundTrip() throws {
        var request = Ask_PrepareAttachmentUploadRequest()
        request.id = "attachment-1"
        request.filename = "document.pdf"
        request.type = .document
        request.streamID = "chunk-stream"
        request.checksum = "900150983cd24fb0d6963f7d28e17f72"
        request.checksumAlgorithm = "md5"
        request.sizeBytes = 3
        request.resultStreamID = "result-stream"

        let decoded = try Ask_PrepareAttachmentUploadRequest(serializedBytes: request.serializedData())
        XCTAssertEqual(decoded, request)
        XCTAssertEqual(decoded.type.rawValue, 2)
        XCTAssertEqual(decoded.resultStreamID, "result-stream")

        var result = Ask_AttachmentUploadResult()
        result.id = request.id
        result.status = .completed
        let decodedResult = try Ask_AttachmentUploadResult(serializedBytes: result.serializedData())
        XCTAssertEqual(decodedResult.status, .completed)
        XCTAssertEqual(decodedResult.statusCode, 0)
        XCTAssertTrue(decodedResult.errorMessage.isEmpty)
    }

    func testPrepareCallRoundTripWithBoundedLargeFields() throws {
        let call = RealmServiceAttachmentPrepareCall(
            attachmentId: String(repeating: "a", count: 256),
            filename: String(repeating: "f", count: 1024),
            type: Int32(Ask_AttachmentType.document.rawValue),
            streamId: String(repeating: "s", count: 256),
            checksum: "900150983cd24fb0d6963f7d28e17f72",
            checksumAlgorithm: "md5",
            sizeBytes: UInt64(RealmServiceAttachmentIPC.maxBytes),
            resultStreamId: String(repeating: "r", count: 256)
        )

        let decoded = try JSONDecoder().decode(
            RealmServiceAttachmentPrepareCall.self, from: JSONEncoder().encode(call)
        )
        XCTAssertEqual(decoded.id, call.id)
        XCTAssertEqual(decoded.filename.utf8.count, 1024)
        XCTAssertEqual(decoded.sizeBytes, UInt64(RealmServiceAttachmentIPC.maxBytes))
        XCTAssertEqual(decoded.streamId, call.streamId)
        XCTAssertNotEqual(decoded.streamId, decoded.resultStreamId)
    }

    func testChunkCallPreservesEmptyAndBoundaryPayloads() throws {
        for payload in [Data(), Data(repeating: 0xA5, count: RealmServiceAttachmentIPC.chunkSizeBytes)] {
            let call = RealmServiceAttachmentChunkCall(
                streamId: "chunk-stream", attachmentId: "attachment-1", sequence: 1,
                payload: payload,
                metadata: [RealmServiceAttachmentMetadata(key: "eos", value: "true")]
            )
            let decoded = try JSONDecoder().decode(
                RealmServiceAttachmentChunkCall.self, from: JSONEncoder().encode(call)
            )
            XCTAssertEqual(decoded.payload, payload)
            XCTAssertEqual(decoded.sequence, 1)
            XCTAssertEqual(decoded.metadata, call.metadata)
        }
    }

    func testResultAndFailureRepliesRoundTrip() throws {
        let event = RealmServiceAttachmentResultEvent(
            streamId: "result-stream", attachmentId: "attachment-1",
            status: Int32(Ask_AttachmentUploadStatus.failed.rawValue),
            statusCode: 41, errorMessage: "checksum mismatch"
        )
        let decodedEvent = try JSONDecoder().decode(
            RealmServiceAttachmentResultEvent.self, from: JSONEncoder().encode(event)
        )
        XCTAssertEqual(decodedEvent.streamId, event.streamId)
        XCTAssertEqual(decodedEvent.status, 2)
        XCTAssertEqual(decodedEvent.statusCode, 41)
        XCTAssertEqual(decodedEvent.errorMessage, "checksum mismatch")

        let failure = RealmServiceFailure(code: "timed_out", message: "offline")
        let reply = RealmServiceAttachmentChunkReply(id: UUID(), failure: failure)
        let decodedReply = try JSONDecoder().decode(
            RealmServiceAttachmentChunkReply.self, from: JSONEncoder().encode(reply)
        )
        XCTAssertEqual(decodedReply.id, reply.id)
        XCTAssertEqual(decodedReply.failure?.code, "timed_out")
        XCTAssertEqual(decodedReply.failure?.message, "offline")
    }
}
