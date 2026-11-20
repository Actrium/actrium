import ActriumCore
import Foundation
import Security
import Testing
@testable import Actrium

@MainActor
struct GuestStorageKeychainTests {
    @Test func firstOpenPersistsAKeyAndReopenUsesIt() async throws {
        let (folder, path) = try fixture()
        defer { try? FileManager.default.removeItem(at: folder) }
        let keychain = TestKeychain()
        let database = GuestStorageDatabase(databaseURL: path, keychain: keychain)
        try await database.run { try $0.setItem(origin: "a", key: "key", value: "value") }
        #expect(keychain.key?.count == 32)
        #expect(keychain.addCount == 1)

        let reopened = GuestStorageDatabase(databaseURL: path, keychain: keychain)
        #expect(try await reopened.run { try $0.getItem(origin: "a", key: "key") } == "value")
        #expect(keychain.addCount == 1)
    }

    @Test func unavailableOrMissingKeyPreservesTheDatabaseAndCanBeRetried() async throws {
        let (folder, path) = try fixture()
        defer { try? FileManager.default.removeItem(at: folder) }
        let keychain = TestKeychain()
        try await GuestStorageDatabase(databaseURL: path, keychain: keychain).run {
            try $0.setItem(origin: "a", key: "key", value: "value")
        }
        let original = try Data(contentsOf: path)
        let correctKey = keychain.key
        let reopened = GuestStorageDatabase(databaseURL: path, keychain: keychain)
        let locked = GuestStorageKeyError.unavailable(errSecInteractionNotAllowed)
        keychain.failure = locked
        await #expect(throws: locked) { try await reopened.run { _ in } }
        keychain.failure = nil
        keychain.key = nil
        await #expect(throws: GuestStorageKeyError.missing) { try await reopened.run { _ in } }
        #expect(try Data(contentsOf: path) == original)
        #expect(keychain.addCount == 1)

        keychain.key = correctKey
        #expect(try await reopened.run { try $0.getItem(origin: "a", key: "key") } == "value")
    }

    @Test func failedKeyWriteDoesNotCreateTheDatabase() async throws {
        let (folder, path) = try fixture()
        defer { try? FileManager.default.removeItem(at: folder) }
        let keychain = TestKeychain()
        let failure = GuestStorageKeyError.unavailable(errSecNotAvailable)
        keychain.addFailure = failure
        let database = GuestStorageDatabase(databaseURL: path, keychain: keychain)
        await #expect(throws: failure) { try await database.run { _ in } }
        #expect(!FileManager.default.fileExists(atPath: path.path))
    }

    @Test func duplicateKeychainItemUsesTheKeyPersistedByTheOtherLaunch() async throws {
        let (folder, path) = try fixture()
        defer { try? FileManager.default.removeItem(at: folder) }
        let keychain = TestKeychain()
        let existing = Data(repeating: 7, count: 32)
        keychain.competingKey = existing
        try await GuestStorageDatabase(databaseURL: path, keychain: keychain).run {
            try $0.setItem(origin: "a", key: "key", value: "value")
        }
        #expect(keychain.key == existing)
        let reopened = GuestStorageDatabase(databaseURL: path, keychain: keychain)
        #expect(try await reopened.run { try $0.getItem(origin: "a", key: "key") } == "value")
    }

    private func fixture() throws -> (URL, URL) {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: false)
        return (folder, folder.appendingPathComponent("guest.sqlite"))
    }
}

private final class TestKeychain: GuestStorageKeychain, @unchecked Sendable {
    var key: Data?
    var failure: GuestStorageKeyError?
    var addFailure: GuestStorageKeyError?
    var competingKey: Data?
    var addCount = 0

    func read() throws -> Data? {
        if let failure { throw failure }
        return key
    }

    func add(_ key: Data) throws -> Bool {
        if let addFailure { throw addFailure }
        addCount += 1
        if let competingKey {
            self.key = competingKey
            return false
        }
        self.key = key
        return true
    }
}
