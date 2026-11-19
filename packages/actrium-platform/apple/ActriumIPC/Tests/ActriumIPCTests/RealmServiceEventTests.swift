import Foundation
import XCTest
@testable import ActriumIPC

final class RealmServiceEventTests: XCTestCase {
    func testEventRoundTripPreservesSubscriptionAndBinaryPayload() throws {
        let subscriptionId = UUID()
        let event = RealmServiceEvent(
            subscriptionId: subscriptionId,
            event: "counter.changed",
            payload: Data([0, 1, 2, 255])
        )

        let decoded = try JSONDecoder().decode(
            RealmServiceEvent.self, from: JSONEncoder().encode(event)
        )
        XCTAssertEqual(decoded.subscriptionId, subscriptionId)
        XCTAssertEqual(decoded.event, "counter.changed")
        XCTAssertEqual(decoded.payload, Data([0, 1, 2, 255]))
    }

    func testSubscriptionFailureRoundTrip() throws {
        let id = UUID()
        let reply = RealmServiceEventReply(
            id: id,
            failure: RealmServiceFailure(code: "permission_denied", message: "Revoked")
        )

        let decoded = try JSONDecoder().decode(
            RealmServiceEventReply.self, from: JSONEncoder().encode(reply)
        )
        XCTAssertEqual(decoded.id, id)
        XCTAssertEqual(decoded.failure?.code, "permission_denied")
        XCTAssertEqual(decoded.failure?.message, "Revoked")
    }
}
