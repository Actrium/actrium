import CryptoKit
import Foundation

/// Realm trust survives restarts and is independent of the evictable package cache.
@MainActor
enum RealmFrontTrustStore {
    enum Kind: String {
        case authority, profile, publisher
    }

    static func read(_ kind: Kind, name: String) throws -> String? {
        do {
            return try String(contentsOf: file(kind, name: name), encoding: .utf8)
        } catch let error as CocoaError where error.code == .fileReadNoSuchFile {
            return nil
        }
    }

    static func save(_ value: String, _ kind: Kind, name: String) throws {
        let url = try file(kind, name: name)
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data(value.utf8).write(to: url, options: .atomic)
    }

    private static func file(_ kind: Kind, name: String) throws -> URL {
        let root = try FileManager.default.url(
            for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true
        ).appendingPathComponent("Actrium/RealmTrust", isDirectory: true)
        let digest = SHA256.hash(data: Data(name.utf8)).map { String(format: "%02x", $0) }.joined()
        return root.appendingPathComponent("\(kind.rawValue)-\(digest).json")
    }
}
