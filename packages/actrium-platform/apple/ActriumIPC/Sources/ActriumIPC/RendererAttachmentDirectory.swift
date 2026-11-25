import Foundation

#if os(macOS)
public enum RendererAttachmentDirectory {
    public static func url(rendererID: String, in container: URL) -> URL? {
        guard rendererID.hasPrefix("renderer-"),
              let id = UUID(uuidString: String(rendererID.dropFirst("renderer-".count))) else { return nil }
        return container.appendingPathComponent("AttachmentCaches", isDirectory: true)
            .appendingPathComponent(id.uuidString, isDirectory: true)
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
