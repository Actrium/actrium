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
            let received = try Actrium_Ipc_InputEvent(serializedBytes: event.serializedData())
            XCTAssertEqual(received, event)
        }
        XCTAssertNotEqual(kinds[0], kinds[2])
    }
}
