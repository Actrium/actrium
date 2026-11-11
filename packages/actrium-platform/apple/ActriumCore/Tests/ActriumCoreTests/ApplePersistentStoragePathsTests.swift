import Foundation
import XCTest
@testable import ActriumCore

final class ApplePersistentStoragePathsTests: XCTestCase {
    func testStoresUseActriumNames() {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let directory = ApplePersistentStoragePaths.applicationDirectory(in: root)
        XCTAssertEqual(directory, root.appendingPathComponent("Actrium", isDirectory: true))
        XCTAssertEqual(ApplePersistentStoragePaths.appDatabasePath(in: directory),
                       directory.appendingPathComponent("actrium.db").path)
        XCTAssertEqual(AppleBrowserStorageDatabase.databasePath(in: directory),
                       directory.appendingPathComponent(AppleBrowserStorageDatabase.databaseName).path)
    }
}
