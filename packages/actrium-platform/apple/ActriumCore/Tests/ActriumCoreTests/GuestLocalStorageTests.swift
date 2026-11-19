import Foundation
import XCTest
@testable import ActriumCore

final class GuestLocalStorageTests: XCTestCase {
    func testPersistenceIsolationOverwriteAndIdempotentRemoval() throws {
        let path = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: path) }
        let first = GuestLocalStorage(path: path.path)
        let initial = try first.getItem(origin: "one", key: "")
        XCTAssertNil(initial)
        try first.setItem(origin: "one", key: "", value: "")
        try first.setItem(origin: "one", key: "\u{0}x", value: "first")
        try first.setItem(origin: "one", key: "\u{0}x", value: "\u{96ea}\u{0}🌲")

        let reopened = GuestLocalStorage(path: path.path)
        let empty = try reopened.getItem(origin: "one", key: "")
        let persisted = try reopened.getItem(origin: "one", key: "\u{0}x")
        let other = try reopened.getItem(origin: "two", key: "\u{0}x")
        XCTAssertEqual(empty, "")
        XCTAssertEqual(persisted, "\u{96ea}\u{0}🌲")
        XCTAssertNil(other)
        try reopened.removeItem(origin: "one", key: "\u{0}x")
        try reopened.removeItem(origin: "one", key: "\u{0}x")
        let removed = try reopened.getItem(origin: "one", key: "\u{0}x")
        XCTAssertNil(removed)
    }

    func testRustValidationErrorSurvivesUniFFI() {
        let store = GuestLocalStorage(path: ":memory:")
        XCTAssertThrowsError(try store.getItem(origin: "one", key: String(repeating: "\u{96ea}", count: 342))) { error in
            guard case GuestLocalStorageError.InvalidKey(let message) = error else {
                XCTFail("Expected the Rust invalid key error, got \(error)")
                return
            }
            XCTAssertEqual(message, "Local storage keys are limited to 1024 UTF-8 bytes.")
        }
    }
}
