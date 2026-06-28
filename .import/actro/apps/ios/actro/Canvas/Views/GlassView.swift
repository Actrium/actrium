import SwiftUI
import UIKit
import UniformTypeIdentifiers

// MARK: - Glass View (Transparent Drag Overlay)

/// 透明的"玻璃"视图，覆盖在所有内容之上，专门捕获拖拽事件
/// 关键特性：hitTest 返回 nil，不阻挡点击和其他触摸事件
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
        // 设置拖放交互
        let interaction = UIDropInteraction(delegate: self)
        addInteraction(interaction)
        self.dropInteraction = interaction
        
        // 设置为完全透明
        backgroundColor = .clear
        isUserInteractionEnabled = true
    }

    // 关键：让点击和触摸事件穿透此视图
    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        // 只有在拖放操作时才接收事件
        return nil  // 不拦截任何触摸事件
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
        // 检查是否包含文件 URL
        return session.hasItemsConforming(toTypeIdentifiers: [UTType.fileURL.identifier])
    }
    
    func dropInteraction(_ interaction: UIDropInteraction, sessionDidUpdate session: UIDropSession) -> UIDropProposal {
        // 返回复制操作
        return UIDropProposal(operation: .copy)
    }
    
    func dropInteraction(_ interaction: UIDropInteraction, performDrop session: UIDropSession) {
        // 处理拖放操作
        session.loadObjects(ofClass: URL.self) { [weak self] items in
            guard let self = self,
                  let urls = items as? [URL],
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

/// 只提供 Glass View 的轻量级包装，用于拖拽处理
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
