import Foundation
import XCTest
@testable import Actrium

final class LocalRealmFrontFileTests: XCTestCase {
    func testApprovedFilenameAllowsRebuildButRejectsSymlink() throws {
        let fm = FileManager.default
        let directory = fm.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try fm.createDirectory(at: directory, withIntermediateDirectories: false)
        defer { try? fm.removeItem(at: directory) }
        let package = directory.appendingPathComponent("front.actr")
        let other = directory.appendingPathComponent("other.actr")
        try Data("first".utf8).write(to: package)
        let approved = try LocalRealmFrontFile(selectedURL: package)
        XCTAssertEqual(try approved.readArchive(), Data("first".utf8))

        try Data("rebuilt".utf8).write(to: package, options: .atomic)
        XCTAssertEqual(try approved.readArchive(), Data("rebuilt".utf8))

        try Data("different".utf8).write(to: other)
        try fm.removeItem(at: package)
        try fm.createSymbolicLink(at: package, withDestinationURL: other)
        XCTAssertThrowsError(try approved.readArchive())
    }
}
