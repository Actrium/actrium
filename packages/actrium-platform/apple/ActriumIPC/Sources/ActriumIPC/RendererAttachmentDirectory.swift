import Foundation

#if os(macOS)
import Darwin

public enum RendererAttachmentDirectory {
    public static func url(rendererID: String, in container: URL) -> URL? {
        guard rendererID.hasPrefix("renderer-"),
              let id = UUID(uuidString: String(rendererID.dropFirst("renderer-".count))) else { return nil }
        return root(in: container).appendingPathComponent(id.uuidString, isDirectory: true)
    }

    /// Recover the cache when idle, then protect it for the lifetime of the returned handle.
    public static func acquireHostLock(in container: URL) throws -> FileHandle {
        let cacheRoot = root(in: container)
        let handle = try openLock(for: cacheRoot)
        if try lock(handle, mode: LOCK_EX | LOCK_NB) {
            do { try remove(cacheRoot) }
            catch { NSLog("Could not recover the attachment cache: %@", error.localizedDescription) }
        }
        _ = try lock(handle, mode: LOCK_SH)
        return handle
    }

    /// Keep protecting the cache if the renderer outlives its Host.
    public static func acquireRendererLock(for directory: URL) throws -> FileHandle {
        let handle = try openLock(for: directory.deletingLastPathComponent())
        _ = try lock(handle, mode: LOCK_SH)
        return handle
    }

    private static func root(in container: URL) -> URL {
        container.appendingPathComponent("AttachmentCaches", isDirectory: true)
    }

    private static func openLock(for cacheRoot: URL) throws -> FileHandle {
        // Keep the lock inode outside the tree that startup recovery can remove.
        let path = cacheRoot.deletingLastPathComponent()
            .appendingPathComponent(cacheRoot.lastPathComponent + ".lock").path
        let flags = O_RDWR | O_CREAT | O_CLOEXEC | O_NOFOLLOW
        let descriptor = Darwin.open(path, flags, 0o600)
        guard descriptor >= 0 else {
            throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno), userInfo: [NSFilePathErrorKey: path])
        }
        return FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
    }

    private static func lock(_ handle: FileHandle, mode: Int32) throws -> Bool {
        while flock(handle.fileDescriptor, mode) != 0 {
            let code = errno
            if code == EINTR { continue }
            if mode & LOCK_NB != 0 && code == EWOULDBLOCK { return false }
            throw NSError(domain: NSPOSIXErrorDomain, code: Int(code))
        }
        return true
    }

    public static func remove(_ directory: URL) throws {
        do {
            try FileManager.default.removeItem(at: directory)
        } catch let error as CocoaError where error.code == .fileNoSuchFile {
            // The renderer may already have removed its directory.
        }
    }
}
#endif
