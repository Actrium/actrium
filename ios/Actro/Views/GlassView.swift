import SwiftUI
import UIKit
import UniformTypeIdentifiers

// MARK: - Glass View (Transparent Drag Overlay)

/// Transparent"Glass"View, overwrite all contents, capture drag events
/// Key characteristics:hitTest Back nil, do not block clicks and other touch events
class GlassView: UIView {
    var onDropACTR: ((URL) -> Void)?
    private var dropInteraction: UIDropInteraction?

    override init(frame: CGRect) {
        super.init(frame: frame)
        setupDropInteraction()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        setupDropInteraction()
    }

    private func setupDropInteraction() {
        // Set drag-and-drop interaction
        let interaction = UIDropInteraction(delegate: self)
        addInteraction(interaction)
        self.dropInteraction = interaction
        
        // Set to Full Transparency
        backgroundColor = .clear
        isUserInteractionEnabled = true
    }

    // Key: let click and touch events penetrate this view
    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        // Only take event when you drag or drop
        return nil  // Do not intercept any touching incidents
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
}

// MARK: - UIDropInteractionDelegate

extension GlassView: UIDropInteractionDelegate {
    func dropInteraction(_ interaction: UIDropInteraction, canHandle session: UIDropSession) -> Bool {
        // Check to include files URL
        return session.hasItemsConforming(toTypeIdentifiers: [UTType.fileURL.identifier])
    }
    
    func dropInteraction(_ interaction: UIDropInteraction, sessionDidUpdate session: UIDropSession) -> UIDropProposal {
        // Back copy operation
        return UIDropProposal(operation: .copy)
    }
    
    func dropInteraction(_ interaction: UIDropInteraction, performDrop session: UIDropSession) {
        // Process Drag-and-Drop Operations
        _ = session.loadObjects(ofClass: URL.self) { [weak self] urls in
            guard let self = self,
                  let fileURL = urls.first,
                  self.isValidDropTarget(fileURL) else {
                return
            }
            
            DispatchQueue.main.async {
                self.onDropACTR?(fileURL)
            }
        }
    }
}

// MARK: - Glass View Only Representable

/// Provide only Glass View Lightweight packaging for drag and drag treatment
struct GlassViewOnlyRepresentable: UIViewRepresentable {
    let onDropACTR: (URL) -> Void

    func makeUIView(context: Context) -> GlassView {
        let glass = GlassView(frame: .zero)
        glass.onDropACTR = onDropACTR
        return glass
    }

    func updateUIView(_ uiView: GlassView, context: Context) {
        uiView.onDropACTR = onDropACTR
    }
}
