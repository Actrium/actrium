import Foundation

/// Holds the originals selected by one renderer's active documents.
final class RendererAttachmentCache: @unchecked Sendable {
    struct File: Sendable {
        let name: String
        let data: Data
        let thumbnail: Data?
        let canPreview: Bool
    }

    struct Attachment: Sendable {
        let id: UUID
        let name: String
        let size: Int
        let thumbnail: Data?
        let canPreview: Bool
    }

    enum Failure: LocalizedError {
        case unavailable
        case full
        case notAnImage

        var errorDescription: String? {
            switch self {
            case .unavailable: "This attachment is unavailable in the current document."
            case .full: "The attachment cache is full. Remove an attachment or close an unused tab."
            case .notAnImage: "This attachment has no image preview."
            }
        }
    }

    private struct Entry {
        let tabId: String
        let url: URL
        let name: String
        let size: Int
        let canPreview: Bool
    }

    private let cacheDirectory: URL
    private let maxBytes: Int
    private let maxEntries: Int
    // Serialize cache state and its filesystem operations in lifecycle order.
    private let queue = DispatchQueue(label: "io.actrium.renderer.attachment-cache", qos: .userInitiated)
    private var reloadKeys: [String: Int] = [:]
    private var entries: [UUID: Entry] = [:]

    init(
        directory: URL,
        maxBytes: Int = 64 * 1024 * 1024, maxEntries: Int = 64
    ) {
        cacheDirectory = directory
        self.maxBytes = maxBytes
        self.maxEntries = maxEntries
    }

    func load(tabId: String, reloadKey: Int) {
        queue.async {
            guard self.reloadKeys[tabId] != reloadKey else { return }
            self.closeOnQueue(tabId: tabId)
            self.reloadKeys[tabId] = reloadKey
        }
    }

    func store(_ files: [File], tabId: String, reloadKey: Int) async throws -> [Attachment] {
        try await perform {
            try self.storeOnQueue(files, tabId: tabId, reloadKey: reloadKey)
        }
    }

    private func storeOnQueue(_ files: [File], tabId: String, reloadKey: Int) throws -> [Attachment] {
        try checkScope(tabId: tabId, reloadKey: reloadKey)
        guard !files.isEmpty else { return [] }
        let used = entries.values.reduce(0) { $0 + $1.size }
        guard files.count <= maxEntries - entries.count,
              files.reduce(0, { $0 + $1.data.count }) <= maxBytes - used else { throw Failure.full }
        try FileManager.default.createDirectory(
            at: cacheDirectory, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700]
        )
        var created: [Attachment] = []
        do {
            for file in files {
                created.append(try write(file, tabId: tabId))
            }
            return created
        } catch {
            for file in created { discard(file.id) }
            throw error
        }
    }

    private func write(_ file: File, tabId: String) throws -> Attachment {
        let id = UUID()
        let suffix = (file.name as NSString).pathExtension
        let leaf = suffix.range(of: "^[a-zA-Z0-9]{1,12}$", options: .regularExpression) == nil
            ? id.uuidString : "\(id.uuidString).\(suffix)"
        let url = cacheDirectory.appendingPathComponent(leaf, isDirectory: false)
        try file.data.write(to: url, options: [.atomic])
        entries[id] = Entry(tabId: tabId, url: url, name: file.name, size: file.data.count, canPreview: file.canPreview)
        return Attachment(
            id: id, name: file.name, size: file.data.count, thumbnail: file.thumbnail, canPreview: file.canPreview
        )
    }

    func readForUpload(_ id: UUID, tabId: String, reloadKey: Int) async throws -> (filename: String, base64: String) {
        try await perform {
            let entry = try self.entry(id, tabId: tabId, reloadKey: reloadKey)
            return (entry.name, try Data(contentsOf: entry.url).base64EncodedString())
        }
    }

    func previewURL(_ id: UUID, tabId: String, reloadKey: Int) async throws -> URL {
        try await perform {
            let entry = try self.entry(id, tabId: tabId, reloadKey: reloadKey)
            guard entry.canPreview else { throw Failure.notAnImage }
            return entry.url
        }
    }

    func release(_ id: UUID, tabId: String, reloadKey: Int) async throws {
        try await perform {
            try self.checkScope(tabId: tabId, reloadKey: reloadKey)
            guard let entry = self.entries[id] else { return }
            guard entry.tabId == tabId else { throw Failure.unavailable }
            try FileManager.default.removeItem(at: entry.url)
            self.entries.removeValue(forKey: id)
        }
    }

    func close(tabId: String) {
        queue.async { self.closeOnQueue(tabId: tabId) }
    }

    func finishClosing() async {
        await withCheckedContinuation { continuation in
            queue.async { continuation.resume() }
        }
    }

    private func closeOnQueue(tabId: String) {
        reloadKeys.removeValue(forKey: tabId)
        for id in entries.compactMap({ $0.value.tabId == tabId ? $0.key : nil }) { discard(id) }
        if entries.isEmpty, FileManager.default.fileExists(atPath: cacheDirectory.path) {
            do { try FileManager.default.removeItem(at: cacheDirectory) }
            catch { NSLog("Could not remove the attachment cache directory: %@", error.localizedDescription) }
        }
    }

    private func perform<T: Sendable>(_ operation: @escaping @Sendable () throws -> T) async throws -> T {
        try await withCheckedThrowingContinuation { continuation in
            queue.async { continuation.resume(with: Result(catching: operation)) }
        }
    }

    private func checkScope(tabId: String, reloadKey: Int) throws {
        guard reloadKeys[tabId] == reloadKey else { throw Failure.unavailable }
    }

    private func entry(_ id: UUID, tabId: String, reloadKey: Int) throws -> Entry {
        try checkScope(tabId: tabId, reloadKey: reloadKey)
        guard let entry = entries[id], entry.tabId == tabId else { throw Failure.unavailable }
        return entry
    }

    private func discard(_ id: UUID) {
        guard let entry = entries.removeValue(forKey: id) else { return }
        do { try FileManager.default.removeItem(at: entry.url) }
        catch { NSLog("Could not remove a cached attachment: %@", error.localizedDescription) }
    }
}
