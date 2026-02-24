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
}
