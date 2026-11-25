import Foundation

#if os(macOS)
public enum RendererAttachmentDirectory {
    public static func url(rendererID: String) -> URL? {
        guard rendererID.hasPrefix("renderer-"),
              let id = UUID(uuidString: String(rendererID.dropFirst("renderer-".count))),
              let container = FileManager.default.containerURL(
                forSecurityApplicationGroupIdentifier: "group.io.actrium.actro"
              ) else { return nil }
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
