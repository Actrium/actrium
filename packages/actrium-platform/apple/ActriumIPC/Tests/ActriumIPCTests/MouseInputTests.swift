import XCTest
@testable import ActriumIPC

final class MouseInputTests: XCTestCase {
    func testHoverAndDragRoundTripAsDistinctEvents() throws {
        let kinds: [Actrium_Ipc_InputEvent.EventType] = [.mouseMove, .mouseDown, .mouseDragged, .mouseUp]
        for kind in kinds {
            var event = Actrium_Ipc_InputEvent()
            event.type = kind
            event.x = 12.5
            event.y = 80.5
            event.button = 1
            if kind != .mouseMove {
                event.clickCount = 3
                event.eventNumber = 42
            }
            let received = try Actrium_Ipc_InputEvent(serializedBytes: event.serializedData())
            XCTAssertEqual(received, event)
        }
        XCTAssertNotEqual(kinds[0], kinds[2])
    }

    func testMouseMetadataDistinguishesMissingFromExplicitZero() throws {
        var event = Actrium_Ipc_InputEvent()
        event.type = .mouseDragged
        let legacy = try Actrium_Ipc_InputEvent(serializedBytes: event.serializedData())
        XCTAssertFalse(legacy.hasClickCount)
        XCTAssertFalse(legacy.hasEventNumber)

        event.clickCount = 0
        event.eventNumber = 0
        let received = try Actrium_Ipc_InputEvent(serializedBytes: event.serializedData())
        XCTAssertTrue(received.hasClickCount)
        XCTAssertTrue(received.hasEventNumber)
        XCTAssertEqual(received.clickCount, 0)
        XCTAssertEqual(received.eventNumber, 0)
    }
}
