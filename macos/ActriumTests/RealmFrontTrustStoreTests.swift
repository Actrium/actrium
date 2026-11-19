import Foundation
import Testing
@testable import Actrium

@MainActor
struct RealmFrontTrustStoreTests {
    @Test func acceptingANewRevisionKeepsTheInstalledSnapshot() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let name = "echo@realm.example"

        #expect(try RealmFrontTrustStore.read(.profile, name: name, directory: directory) == nil)
        try RealmFrontTrustStore.save("revision 1", .profile, name: name, directory: directory)
        try RealmFrontTrustStore.save("revision 1", .installedProfile, name: name, directory: directory)
        try RealmFrontTrustStore.save("revision 2", .profile, name: name, directory: directory)

        #expect(try RealmFrontTrustStore.read(.profile, name: name, directory: directory) == "revision 2")
        #expect(try RealmFrontTrustStore.read(.installedProfile, name: name, directory: directory) == "revision 1")
        #expect(try RealmFrontTrustStore.read(.profile, name: "other@realm.example", directory: directory) == nil)
    }
}
