import Darwin
import Foundation

/// A Tab's read access to the selected filename in the selected directory.
/// Keeping the directory open permits atomic rebuilds without following a
/// replacement directory or a newly installed symlink on refresh.
nonisolated final class LocalRealmFrontFile: Sendable {
    private let directory: Int32
    private let filename: String

    init(selectedURL: URL) throws {
        guard selectedURL.isFileURL, selectedURL.pathExtension.lowercased() == "actr" else {
            throw CocoaError(.fileReadUnsupportedScheme)
        }
        let parent = selectedURL.deletingLastPathComponent().resolvingSymlinksInPath().path
        let descriptor = Darwin.open(parent, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        guard descriptor >= 0 else { throw CocoaError(.fileReadNoPermission) }
        directory = descriptor
        filename = selectedURL.lastPathComponent
    }

    deinit {
        Darwin.close(directory)
    }

    func readArchive() throws -> Data {
        let descriptor = openat(directory, filename, O_RDONLY | O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK)
        guard descriptor >= 0 else { throw CocoaError(.fileReadNoPermission) }
        let file = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
        defer { try? file.close() }
        var info = stat()
        let limit = 64 * 1024 * 1024
        guard fstat(descriptor, &info) == 0, info.st_mode & mode_t(S_IFMT) == mode_t(S_IFREG),
              (1...Int64(limit)).contains(info.st_size),
              let bytes = try file.read(upToCount: limit + 1),
              !bytes.isEmpty, bytes.count <= limit else {
            throw CocoaError(.fileReadCorruptFile)
        }
        return bytes
    }
}
