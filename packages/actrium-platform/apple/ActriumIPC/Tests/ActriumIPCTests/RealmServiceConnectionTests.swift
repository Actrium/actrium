import ActriumIPC
import Foundation
import XCTest

final class RealmServiceConnectionTests: XCTestCase {
    func testConnectionBindingAndReloadKeySurviveIPC() throws {
        let connectionId = UUID()
        let request = RealmServiceCall(
            id: UUID(), route: "upload.Begin", payload: Data([1]), timeoutMs: 5_000,
            streamChunk: .init(id: "stream", sequence: 42),
            connection: .init(action: .use, id: connectionId), reloadKey: 7
        )
        let decoded = try JSONDecoder().decode(RealmServiceCall.self, from: JSONEncoder().encode(request))
        XCTAssertEqual(decoded.connection?.action, .use)
        XCTAssertEqual(decoded.connection?.id, connectionId)
        XCTAssertEqual(decoded.reloadKey, 7)
        XCTAssertEqual(decoded.streamChunk?.sequence, 42)

        let reply = RealmServiceReply(id: request.id, payload: Data([2]), connectionId: connectionId)
        let decodedReply = try JSONDecoder().decode(RealmServiceReply.self, from: JSONEncoder().encode(reply))
        XCTAssertEqual(decodedReply.connectionId, connectionId)
    }
}
