import SwiftUI
import AppKit
import UniformTypeIdentifiers

// MARK: - Glass View (Transparent Drag Overlay)

/// Transparent"Glass"View, overwrite all contents, capture drag events
/// Key characteristics:hitTest Back nilDo not block clicks and other mouse events
class GlassView: NSView {
    var onDropACTR: ((URL) -> Void)?

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        // Registration acceptance document URL Drag
        registerForDraggedTypes([.fileURL])
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        registerForDraggedTypes([.fileURL])
    }

    // Key: let clicks and mouse events penetrate this view
    override func hitTest(_ point: NSPoint) -> NSView? {
        return nil  // Do not intercept any hits
    }

    // Key: Empty drawing, full transparency
    override func draw(_ dirtyRect: NSRect) {
        // No content drawn, full transparency
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

    /// Check if URL is a valid drop target (.actr file or directory with manifest.toml)
    private func isValidDropTarget(_ url: URL) -> Bool {
        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory) else {
            return false
        }

        if isDirectory.boolValue {
            // Directory - check for manifest.toml
            let manifestPath = url.appendingPathComponent("manifest.toml").path
            return FileManager.default.fileExists(atPath: manifestPath)
        } else {
            // File - check for .actr extension
            return url.pathExtension.lowercased() == "actr"
        }
    }

    private func getFileURL(from sender: NSDraggingInfo) -> URL? {
        let pasteboard = sender.draggingPasteboard

        // Methodology 1: From file URL Type of acquisition
        if let urls = pasteboard.readObjects(forClasses: [NSURL.self], options: nil) as? [URL],
           let url = urls.first {
            return url
        }

        // Methodology 2: From string Access
        if let urlString = pasteboard.string(forType: .fileURL),
           let url = URL(string: urlString) {
            return url
        }

        // Methodology 3: From property list Access
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

/// Provide only Glass View Lightweight packaging for drag and drag treatment
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
