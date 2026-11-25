import ActriumIPC
import Foundation

@main
struct RendererAttachmentCacheRegression {
    static func main() async throws {
        if CommandLine.arguments.count == 2 {
            let directory = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
            let cache = RendererAttachmentCache(directory: directory)
            cache.load(tabId: "worker", reloadKey: 1)
            let file = RendererAttachmentCache.File(name: "note.txt", data: Data([1]), thumbnail: nil, canPreview: false)
            _ = try await cache.store([file], tabId: "worker", reloadKey: 1)
            exit(17)
        }
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer {
            do { try FileManager.default.removeItem(at: root) }
            catch { fatalError("Could not remove test fixture: \(error)") }
        }
        try await verifyCache(in: root.appendingPathComponent("renderer-one"))
        try verifyExitedWorkerCleanup(in: root)
        try verifyPreviewMetadata()
        print("PASS: attachment cache preview, upload, limits, tab scope, and cleanup")
    }

    private static func verifyCache(in directory: URL) async throws {
        let cache = RendererAttachmentCache(directory: directory, maxBytes: 5, maxEntries: 2)
        let tab = "tab"
        cache.load(tabId: tab, reloadKey: 1)
        cache.load(tabId: "other", reloadKey: 1)
        let original = Data([1, 2, 3])
        let image = RendererAttachmentCache.File(name: "photo.png", data: original, thumbnail: nil, canPreview: true)
        let file = RendererAttachmentCache.File(name: "note.txt", data: Data([4]), thumbnail: nil, canPreview: false)
        let stored = try await cache.store([image, file], tabId: tab, reloadKey: 1)
        let photo = stored[0]
        let note = stored[1]
        let url = try await cache.previewURL(photo.id, tabId: tab, reloadKey: 1)
        let upload = try await cache.readForUpload(photo.id, tabId: tab, reloadKey: 1)
        precondition(photo.thumbnail == nil && photo.canPreview)
        precondition(upload.filename == photo.name && upload.base64 == original.base64EncodedString())
        try await expectFailure(.notAnImage) { _ = try await cache.previewURL(note.id, tabId: tab, reloadKey: 1) }
        try await expectFailure(.unavailable) { try await cache.release(photo.id, tabId: "other", reloadKey: 1) }
        try await expectFailure(.full) { _ = try await cache.store([image], tabId: tab, reloadKey: 1) }
        try await cache.release(note.id, tabId: tab, reloadKey: 1)
        try await cache.release(note.id, tabId: tab, reloadKey: 1)
        cache.load(tabId: tab, reloadKey: 2)
        try await expectFailure(.unavailable) { _ = try await cache.readForUpload(photo.id, tabId: tab, reloadKey: 1) }
        try await expectFailure(.unavailable) { try await cache.release(photo.id, tabId: tab, reloadKey: 1) }
        precondition(!FileManager.default.fileExists(atPath: url.path))
        _ = try await cache.store([image], tabId: tab, reloadKey: 2)
        cache.close(tabId: tab)
        try await expectFailure(.unavailable) { _ = try await cache.store([image], tabId: tab, reloadKey: 2) }
        precondition(!FileManager.default.fileExists(atPath: directory.path))

    }

    private static func verifyExitedWorkerCleanup(in root: URL) throws {
        let directory = root.appendingPathComponent("renderer-exited")
        let sibling = root.appendingPathComponent("renderer-two")
        try FileManager.default.createDirectory(at: sibling, withIntermediateDirectories: false)
        let worker = Process()
        worker.executableURL = URL(fileURLWithPath: CommandLine.arguments[0])
        worker.arguments = [directory.path]
        try worker.run()
        worker.waitUntilExit()
        precondition(worker.terminationStatus == 17)
        precondition(FileManager.default.fileExists(atPath: directory.path))
        try RendererAttachmentDirectory.remove(directory)
        try RendererAttachmentDirectory.remove(directory)
        precondition(!FileManager.default.fileExists(atPath: directory.path))
        precondition(FileManager.default.fileExists(atPath: sibling.path))
    }

    private static func verifyPreviewMetadata() throws {
        let png = Data(base64Encoded: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAACklEQVR4nGMAAQAABQABDQottAAAAABJRU5ErkJggg==")!
        let withoutThumbnail = RendererAttachmentThumbnail.inspect(data: png, fileExtension: "")
        precondition(withoutThumbnail.canPreview && withoutThumbnail.thumbnail == nil)
        let photo = RendererAttachmentThumbnail.inspect(data: png, fileExtension: "png")
        precondition(photo.canPreview && photo.thumbnail != nil)
        precondition(!RendererAttachmentThumbnail.inspect(data: Data("text".utf8), fileExtension: "png").canPreview)
        let file = GuestFileImportReply.File(name: "image", data: png, canPreview: withoutThumbnail.canPreview)
        let encoded = try JSONEncoder().encode(file)
        let decoded = try JSONDecoder().decode(GuestFileImportReply.File.self, from: encoded)
        precondition(decoded.canPreview == true && decoded.thumbnail == nil)
    }

    private static func expectFailure(
        _ expected: RendererAttachmentCache.Failure, _ operation: @Sendable () async throws -> Void
    ) async throws {
        do {
            try await operation()
            fatalError("Expected \(expected)")
        } catch let error as RendererAttachmentCache.Failure {
            precondition(error == expected)
        }
    }
}
