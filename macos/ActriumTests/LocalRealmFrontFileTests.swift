import Darwin
import Foundation
import XCTest
@testable import Actrium

final class LocalRealmFrontFileTests: XCTestCase {
    func testApprovedAbsolutePathReportsMissingFilesAndRejectsSymlinkDirectories() throws {
        let fm = FileManager.default
        let physicalTemp = try XCTUnwrap(realpath(fm.temporaryDirectory.path, nil))
        defer { free(physicalTemp) }
        let directoryPath = String(cString: physicalTemp) + "/" + UUID().uuidString
        let directory = URL(fileURLWithPath: directoryPath, isDirectory: true)
        try fm.createDirectory(at: directory, withIntermediateDirectories: false)
        defer { try? fm.removeItem(at: directory) }

        let package = directory.appendingPathComponent("front.actr")
        let approved = try LocalRealmFrontFile(approvedAbsolutePath: directoryPath + "/front.actr")
        XCTAssertThrowsError(try approved.readArchive()) { error in
            XCTAssertEqual((error as? CocoaError)?.code, .fileReadNoSuchFile)
        }
        XCTAssertThrowsError(try LocalRealmFrontFile(
            approvedAbsolutePath: directoryPath + "/missing/front.actr"
        )) { error in
            XCTAssertEqual((error as? CocoaError)?.code, .fileReadNoSuchFile)
        }

        let bytes = Data("local package".utf8)
        try bytes.write(to: package)
        XCTAssertEqual(try approved.readArchive(), bytes)
        let link = directory.appendingPathComponent("link")
        try fm.createSymbolicLink(at: link, withDestinationURL: directory)
        XCTAssertThrowsError(try LocalRealmFrontFile(
            approvedAbsolutePath: directoryPath + "/link/front.actr"
        )) { error in
            XCTAssertEqual((error as? CocoaError)?.code, .fileReadUnsupportedScheme)
        }
    }

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
