import UIKit

extension UIView {
    /// Captures a snapshot of the view as a UIImage
    func takeSnapshot() -> UIImage? {
        UIGraphicsBeginImageContextWithOptions(bounds.size, false, UIScreen.main.scale)
        defer { UIGraphicsEndImageContext() }
        
        // Use drawHierarchy for more accurate capture of complex views like RCTRootView
        drawHierarchy(in: bounds, afterScreenUpdates: true)
        
        return UIGraphicsGetImageFromCurrentImageContext()
    }
}

extension UIImage {
    /// Crops the image to the specified rect
    func cropped(to rect: CGRect) -> UIImage? {
        let scaledRect = CGRect(
            x: rect.origin.x * scale,
            y: rect.origin.y * scale,
            width: rect.size.width * scale,
            height: rect.size.height * scale
        )

        guard let imageRef = cgImage?.cropping(to: scaledRect) else {
            return nil
        }

        return UIImage(cgImage: imageRef, scale: scale, orientation: imageOrientation)
    }

    /// 将图片降采样到指定宽度，保持原始宽高比。
    /// 用于标签页快照缩略图，避免因宽高比不一致导致拉伸变形。
    /// - Parameter targetWidth: 目标宽度（pt，@1x），高度按比例自动计算。
    /// - Returns: @1x 降采样图片，内存占用大幅降低。
    func downsampled(toWidth targetWidth: CGFloat) -> UIImage {
        let scale = targetWidth / size.width
        let targetHeight = size.height * scale
        let targetSize = CGSize(width: targetWidth, height: targetHeight)
        let renderer = UIGraphicsImageRenderer(size: targetSize)
        return renderer.image { _ in
            draw(in: CGRect(origin: .zero, size: targetSize))
        }
    }
}
