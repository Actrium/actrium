import Foundation
import XCTest
@testable import ActriumCore

final class ApplePersistentStoragePathsTests: XCTestCase {
    func testExistingStoresWinOverRenamedStoresWithoutMovingJournals() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let legacy = root.appendingPathComponent("Actro", isDirectory: true)
        let current = root.appendingPathComponent("Actrium", isDirectory: true)
        try FileManager.default.createDirectory(at: legacy, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: current, withIntermediateDirectories: true)
        let mainDatabase = legacy.appendingPathComponent("actro.db")
        try Data("existing vault".utf8).write(to: mainDatabase)
        try Data().write(to: legacy.appendingPathComponent("actrium.db"))
        try Data("pending journal".utf8).write(to: legacy.appendingPathComponent("actro.db-wal"))

        XCTAssertEqual(ApplePersistentStoragePaths.applicationDirectory(in: root), legacy)
        XCTAssertEqual(ApplePersistentStoragePaths.appDatabasePath(in: legacy), mainDatabase.path)
        XCTAssertEqual(try Data(contentsOf: legacy.appendingPathComponent("actro.db-wal")),
                       Data("pending journal".utf8))

        let database = try AppleBrowserStorageDatabase(path: legacy.appendingPathComponent("actro_browser.db").path)
        XCTAssertTrue(database.setMetaFlag("existing-install"))
        try Data().write(to: legacy.appendingPathComponent("actrium_browser.db"))
        let reopened = try AppleBrowserStorageDatabase(path: AppleBrowserStorageDatabase.databasePath(in: legacy))
        XCTAssertTrue(reopened.hasMetaFlag("existing-install"))
    }

    func testFreshInstallUsesCurrentNames() {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let directory = ApplePersistentStoragePaths.applicationDirectory(in: root)
        XCTAssertEqual(directory, root.appendingPathComponent("Actrium", isDirectory: true))
        XCTAssertEqual(ApplePersistentStoragePaths.appDatabasePath(in: directory),
                       directory.appendingPathComponent("actrium.db").path)
        XCTAssertEqual(AppleBrowserStorageDatabase.databasePath(in: directory),
                       directory.appendingPathComponent(AppleBrowserStorageDatabase.databaseName).path)
    }
}
