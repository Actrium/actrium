import ActriumIPC
import Foundation
import ImageIO
import UniformTypeIdentifiers

nonisolated enum RendererAttachmentThumbnail {
    private static let rasterExtensions: Set<String> = [
        "png", "jpg", "jpeg", "jpe", "gif", "webp", "bmp", "tif", "tiff", "heic", "heif", "avif",
    ]
    private static let rasterTypes: Set<String> = [
        UTType.png.identifier, UTType.jpeg.identifier, UTType.gif.identifier, UTType.bmp.identifier,
        UTType.tiff.identifier, "org.webmproject.webp", "public.heic", "public.heif", "public.avif",
    ]

    static func make(data: Data, fileExtension: String) -> Data? {
        guard rasterExtensions.contains(fileExtension.lowercased()),
              let source = CGImageSourceCreateWithData(data as CFData, [kCGImageSourceShouldCache: false] as CFDictionary),
              let type = CGImageSourceGetType(source) as String?,
              rasterTypes.contains(type),
              let metadata = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let width = metadata[kCGImagePropertyPixelWidth] as? Int, (1...8192).contains(width),
              let height = metadata[kCGImagePropertyPixelHeight] as? Int, (1...8192).contains(height) else { return nil }
        let options: [CFString: Any] = [
            kCGImageSourceThumbnailMaxPixelSize: 128,
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
        ]
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary) else { return nil }
        return encode(image, quality: 0.7) ?? encode(image, quality: 0.42)
    }

    private static func encode(_ image: CGImage, quality: Double) -> Data? {
        let output = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(output, UTType.jpeg.identifier as CFString, 1, nil) else { return nil }
        CGImageDestinationAddImage(destination, image, [kCGImageDestinationLossyCompressionQuality: quality] as CFDictionary)
        guard CGImageDestinationFinalize(destination), output.length <= GuestFileImportCall.maxThumbnailBytes else { return nil }
        return output as Data
    }
}
