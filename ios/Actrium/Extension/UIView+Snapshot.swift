import UIKit

extension UIView {
    /// Captures a snapshot of the view as a UIImage
    func takeSnapshot() -> UIImage? {
        let format = UIGraphicsImageRendererFormat()
        format.scale = traitCollection.displayScale
        let renderer = UIGraphicsImageRenderer(bounds: bounds, format: format)

        return renderer.image { _ in
            // Use drawHierarchy for more accurate capture of complex views like RCTRootView
            drawHierarchy(in: bounds, afterScreenUpdates: true)
        }
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

    /// Lowers the sample to the specified width and maintains the original width ratio.
    /// The snapshot for the tabs avoids stretching to deformation due to a wide-high-scale difference.
    /// - Parameter targetWidth: Target width (%2)pt, @1x) is calculated automatically on a pro rata basis.
    /// - Returns: @1x The memory occupancy was significantly reduced by the reduction of the sampled picture.
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
