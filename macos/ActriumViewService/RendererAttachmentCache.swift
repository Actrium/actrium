import Foundation

/// Holds the originals selected by one renderer's active documents.
@MainActor
final class RendererAttachmentCache {
    struct File {
        let name: String
        let data: Data
        let thumbnail: Data?
    }

    struct Attachment {
        let id: UUID
        let name: String
        let size: Int
        let thumbnail: Data?
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
        let isImage: Bool
    }

    private let cacheDirectory: URL
    private let maxBytes: Int
    private let maxEntries: Int
    private var reloadKeys: [String: Int] = [:]
    private var entries: [UUID: Entry] = [:]

    init(
        baseDirectory: URL = FileManager.default.temporaryDirectory,
        maxBytes: Int = 64 * 1024 * 1024, maxEntries: Int = 64
    ) {
        cacheDirectory = baseDirectory.appendingPathComponent("actrium-attachments-\(UUID().uuidString)", isDirectory: true)
        self.maxBytes = maxBytes
        self.maxEntries = maxEntries
    }

    func load(tabId: String, reloadKey: Int) {
        guard reloadKeys[tabId] != reloadKey else { return }
        close(tabId: tabId)
        reloadKeys[tabId] = reloadKey
    }

    func store(_ files: [File], tabId: String, reloadKey: Int) throws -> [Attachment] {
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
        entries[id] = Entry(tabId: tabId, url: url, name: file.name, size: file.data.count, isImage: file.thumbnail != nil)
        return Attachment(id: id, name: file.name, size: file.data.count, thumbnail: file.thumbnail)
    }

    func read(_ id: UUID, tabId: String, reloadKey: Int) throws -> File {
        let entry = try entry(id, tabId: tabId, reloadKey: reloadKey)
        return File(name: entry.name, data: try Data(contentsOf: entry.url), thumbnail: nil)
    }

    func previewURL(_ id: UUID, tabId: String, reloadKey: Int) throws -> URL {
        let entry = try entry(id, tabId: tabId, reloadKey: reloadKey)
        guard entry.isImage else { throw Failure.notAnImage }
        return entry.url
    }

    func release(_ id: UUID, tabId: String, reloadKey: Int) throws {
        try checkScope(tabId: tabId, reloadKey: reloadKey)
        guard let entry = entries[id] else { return }
        guard entry.tabId == tabId else { throw Failure.unavailable }
        try FileManager.default.removeItem(at: entry.url)
        entries.removeValue(forKey: id)
    }

    func close(tabId: String) {
        reloadKeys.removeValue(forKey: tabId)
        for id in entries.compactMap({ $0.value.tabId == tabId ? $0.key : nil }) { discard(id) }
        if entries.isEmpty, FileManager.default.fileExists(atPath: cacheDirectory.path) {
            do { try FileManager.default.removeItem(at: cacheDirectory) }
            catch { NSLog("Could not remove the attachment cache directory: %@", error.localizedDescription) }
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
