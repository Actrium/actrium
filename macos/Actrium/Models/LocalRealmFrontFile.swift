import Darwin
import Foundation

/// A Tab's read access to the selected filename in the selected directory.
/// Keeping the directory open permits atomic rebuilds without following a
/// replacement directory or a newly installed symlink on refresh.
nonisolated final class LocalRealmFrontFile: Sendable {
    private let directory: Int32
    private let filename: String

    init(selectedURL: URL) throws {
        let name = selectedURL.lastPathComponent
        guard selectedURL.isFileURL, name.hasSuffix(".actr"), name != ".actr" else {
            throw CocoaError(.fileReadUnsupportedScheme)
        }
        let parent = selectedURL.deletingLastPathComponent().resolvingSymlinksInPath().path
        let descriptor = Darwin.open(parent, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        guard descriptor >= 0 else { throw Self.openError() }
        guard Self.isLocalVolume(descriptor) else {
            Darwin.close(descriptor)
            throw CocoaError(.fileReadUnsupportedScheme)
        }
        directory = descriptor
        filename = name
    }

    /// The caller must show this exact approved path to the user before calling.
    /// Traverse each directory by descriptor so a symlink cannot redirect consent.
    init(approvedAbsolutePath path: String) throws {
        let components = path.split(separator: "/", omittingEmptySubsequences: false).dropFirst()
        guard path.hasPrefix("/"), !path.utf8.contains(0),
              let name = components.last, name.hasSuffix(".actr"), name != ".actr",
              components.allSatisfy({ !$0.isEmpty && $0 != "." && $0 != ".." }) else {
            throw CocoaError(.fileReadUnsupportedScheme)
        }
        let flags = O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC
        var descriptor = Darwin.open("/", flags)
        guard descriptor >= 0 else { throw Self.openError() }
        do {
            for component in components.dropLast() {
                let next = openat(descriptor, String(component), flags)
                guard next >= 0 else { throw Self.openError() }
                Darwin.close(descriptor)
                descriptor = next
                guard Self.isLocalVolume(descriptor) else {
                    throw CocoaError(.fileReadUnsupportedScheme)
                }
            }
        } catch {
            Darwin.close(descriptor)
            throw error
        }
        directory = descriptor
        filename = String(name)
    }

    deinit {
        Darwin.close(directory)
    }

    func readArchive() throws -> Data {
        let descriptor = openat(directory, filename, O_RDONLY | O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK)
        guard descriptor >= 0 else { throw Self.openError() }
        let file = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
        defer { try? file.close() }
        guard Self.isLocalVolume(descriptor) else {
            throw CocoaError(.fileReadUnsupportedScheme)
        }
        var info = stat()
        let limit = 64 * 1024 * 1024
        guard fstat(descriptor, &info) == 0, info.st_mode & mode_t(S_IFMT) == mode_t(S_IFREG),
              (1...Int64(limit)).contains(info.st_size) else {
            throw CocoaError(.fileReadCorruptFile)
        }
        var bytes = Data()
        bytes.reserveCapacity(Int(info.st_size))
        while let chunk = try file.read(upToCount: min(64 * 1024, limit + 1 - bytes.count)), !chunk.isEmpty {
            bytes.append(chunk)
            guard bytes.count <= limit else { throw CocoaError(.fileReadCorruptFile) }
        }
        guard !bytes.isEmpty else { throw CocoaError(.fileReadCorruptFile) }
        return bytes
    }

    private static func openError() -> CocoaError {
        switch errno {
        case ENOENT:
            return CocoaError(.fileReadNoSuchFile)
        case ELOOP, ENOTDIR:
            return CocoaError(.fileReadUnsupportedScheme)
        default:
            return CocoaError(.fileReadNoPermission)
        }
    }

    private static func isLocalVolume(_ descriptor: Int32) -> Bool {
        var volume = statfs()
        return fstatfs(descriptor, &volume) == 0 && volume.f_flags & UInt32(MNT_LOCAL) != 0
    }
}
