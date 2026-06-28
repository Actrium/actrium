import SwiftUI
import AppKit
import UniformTypeIdentifiers

// MARK: - Glass View (Transparent Drag Overlay)

/// 透明的"玻璃"视图，覆盖在所有内容之上，专门捕获拖拽事件
/// 关键特性：hitTest 返回 nil，不阻挡点击和其他鼠标事件
class GlassView: NSView {
    var onDropACTR: ((URL) -> Void)?

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        // 注册接受文件 URL 拖拽
        registerForDraggedTypes([.fileURL])
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        registerForDraggedTypes([.fileURL])
    }

    // 关键：让点击和鼠标事件穿透此视图
    override func hitTest(_ point: NSPoint) -> NSView? {
        return nil  // 不拦截任何点击事件
    }

    // 关键：空绘制，完全透明
    override func draw(_ dirtyRect: NSRect) {
        // 不绘制任何内容，完全透明
    }

    // MARK: - NSDraggingDestination

    override func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation {
        guard let fileURL = getFileURL(from: sender),
              isValidDropTarget(fileURL) else {
            return []
        }
        return .copy
    }

    override func draggingUpdated(_ sender: NSDraggingInfo) -> NSDragOperation {
        guard let fileURL = getFileURL(from: sender),
              isValidDropTarget(fileURL) else {
            return []
        }
        return .copy
    }

    override func performDragOperation(_ sender: NSDraggingInfo) -> Bool {
        guard let fileURL = getFileURL(from: sender),
              isValidDropTarget(fileURL) else {
            return false
        }

        DispatchQueue.main.async { [weak self] in
            self?.onDropACTR?(fileURL)
        }

        return true
    }

    /// Check if URL is a valid drop target (.actr file or directory with manifest.json)
    private func isValidDropTarget(_ url: URL) -> Bool {
        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory) else {
            return false
        }

        if isDirectory.boolValue {
            // Directory - check for manifest.json
            let manifestPath = url.appendingPathComponent("manifest.json").path
            return FileManager.default.fileExists(atPath: manifestPath)
        } else {
            // File - check for .actr extension
            return url.pathExtension.lowercased() == "actr"
        }
    }

    private func getFileURL(from sender: NSDraggingInfo) -> URL? {
        let pasteboard = sender.draggingPasteboard

        // 方法 1：从 file URL 类型获取
        if let urls = pasteboard.readObjects(forClasses: [NSURL.self], options: nil) as? [URL],
           let url = urls.first {
            return url
        }

        // 方法 2：从 string 获取
        if let urlString = pasteboard.string(forType: .fileURL),
           let url = URL(string: urlString) {
            return url
        }

        // 方法 3：从 property list 获取
        if let types = pasteboard.types,
           types.contains(.fileURL),
           let data = pasteboard.data(forType: .fileURL),
           let url = URL(dataRepresentation: data, relativeTo: nil) {
            return url
        }

        return nil
    }
}

// MARK: - Glass View Only Representable

/// 只提供 Glass View 的轻量级包装，用于拖拽处理
struct GlassViewOnlyRepresentable: NSViewRepresentable {
    let onDropACTR: (URL) -> Void

    func makeNSView(context: Context) -> GlassView {
        let glass = GlassView(frame: .zero)
        glass.onDropACTR = onDropACTR
        return glass
    }

    func updateNSView(_ nsView: GlassView, context: Context) {
        nsView.onDropACTR = onDropACTR
    }
}
