import Foundation
import XCTest
@testable import ActriumIPC

final class RealmServiceReplyTests: XCTestCase {
    func testFailurePreservesUpstreamCodeAndDetailsAcrossIPC() throws {
        let id = UUID()
        let failure = RealmServiceFailure(
            code: "delivery_uncertain",
            message: "Delivery uncertain for op-1: offline",
            details: ["operationId": "op-1", "cause": "offline"]
        )
        let reply = RealmServiceReply(id: id, failure: failure)
        let decoded = try JSONDecoder().decode(RealmServiceReply.self, from: JSONEncoder().encode(reply))

        XCTAssertEqual(decoded.id, id)
        XCTAssertEqual(decoded.error, failure.message)
        XCTAssertEqual(decoded.failure?.code, failure.code)
        XCTAssertEqual(decoded.failure?.details, failure.details)
    }

    func testDecodesLegacyMessageWithoutStructuredFailure() throws {
        let id = UUID()
        let json = Data(#"{"id":"\#(id.uuidString)","error":"offline"}"#.utf8)
        let decoded = try JSONDecoder().decode(RealmServiceReply.self, from: json)

        XCTAssertEqual(decoded.id, id)
        XCTAssertEqual(decoded.error, "offline")
        XCTAssertNil(decoded.failure)
    }

    func testPreservesNativeErrorDomainAndCustomDetails() {
        let source = NSError(
            domain: "TestTransport", code: 73,
            userInfo: [NSLocalizedDescriptionKey: "offline", "operationId": "op-1"]
        )
        let failure = RealmServiceFailure(error: source)

        XCTAssertEqual(failure.code, "73")
        XCTAssertEqual(failure.message, "offline")
        XCTAssertEqual(failure.domain, "TestTransport")
        XCTAssertEqual(failure.details?["operationId"], "op-1")
    }
}
