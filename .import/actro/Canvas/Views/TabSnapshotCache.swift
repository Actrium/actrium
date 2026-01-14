//
//  TabSnapshotCache.swift
//  Canvas
//
//  Dual-slot snapshot cache for Tab mode switching animations.
//  Slot A: Full-screen snapshot for transition animation
//  Slot B: Filtered thumbnail for Overview grid display
//

import AppKit
import CoreImage

// MARK: - Tab Snapshot Cache

/// Dual-slot snapshot cache for each tab
struct TabSnapshotCache {
    /// Slot A: Full-screen snapshot captured during Focus mode
    /// Used for Focus → Overview transition animation
    var fullScreenSnapshot: NSImage?

    /// Slot B: Filtered thumbnail (blur + desaturate) for Overview display
    /// Generated from Slot A after capture
    var filteredThumbnail: NSImage?

    /// Last capture timestamp for cache invalidation
    var lastCaptureTime: Date?

    /// Check if cache has valid snapshot
    var hasValidSnapshot: Bool {
        fullScreenSnapshot != nil
    }
}

// MARK: - Tab Snapshot Cache Manager

/// Global manager for tab snapshot caches
/// Thread-safe singleton that manages dual-slot caches for all tabs
@MainActor
final class TabSnapshotCacheManager {
    static let shared = TabSnapshotCacheManager()

    private var caches: [UUID: TabSnapshotCache] = [:]

    /// CIContext for efficient image filtering
    private let ciContext = CIContext(options: [.useSoftwareRenderer: false])

    private init() {}

    // MARK: - Cache Access

    /// Get cache for a tab (returns nil if no cache exists)
    func cache(for tabId: UUID) -> TabSnapshotCache? {
        caches[tabId]
    }

    /// Get full-screen snapshot (Slot A) for a tab
    func fullScreenSnapshot(for tabId: UUID) -> NSImage? {
        caches[tabId]?.fullScreenSnapshot
    }

    /// Get filtered thumbnail (Slot B) for a tab
    func filteredThumbnail(for tabId: UUID) -> NSImage? {
        caches[tabId]?.filteredThumbnail
    }

    // MARK: - Cache Update

    /// Set full-screen snapshot (Slot A) and auto-generate filtered thumbnail (Slot B)
    func setFullScreenSnapshot(_ image: NSImage, for tabId: UUID) {
        var cache = caches[tabId] ?? TabSnapshotCache()
        cache.fullScreenSnapshot = image
        cache.lastCaptureTime = Date()
        caches[tabId] = cache

        NSLog("📸 TabSnapshotCacheManager: Slot A set for tab %@, size=%@",
              String(tabId.uuidString.prefix(8)), NSStringFromSize(image.size))

        // Auto-generate filtered thumbnail in background
        generateFilteredThumbnail(from: image, for: tabId)
    }

    /// Generate filtered thumbnail (Slot B) from full-screen snapshot
    /// Applies blur + desaturation for Overview display
    func generateFilteredThumbnail(from fullScreen: NSImage, for tabId: UUID) {
        // Run filter on background queue to avoid blocking main thread
        Task.detached(priority: .userInitiated) { [weak self] in
            guard let self = self else { return }

            let filtered = await self.applyOverviewFilters(to: fullScreen)

            await MainActor.run {
                guard var cache = self.caches[tabId] else { return }
                cache.filteredThumbnail = filtered
                self.caches[tabId] = cache

                NSLog("📸 TabSnapshotCacheManager: Slot B generated for tab %@",
                      String(tabId.uuidString.prefix(8)))
            }
        }
    }

    /// Invalidate cache for a tab
    func invalidate(tabId: UUID) {
        caches.removeValue(forKey: tabId)
        NSLog("🗑️ TabSnapshotCacheManager: Cache invalidated for tab %@",
              String(tabId.uuidString.prefix(8)))
    }

    /// Invalidate all caches
    func invalidateAll() {
        caches.removeAll()
        NSLog("🗑️ TabSnapshotCacheManager: All caches invalidated")
    }

    // MARK: - Image Filtering

    /// Apply Overview mode filters: slight blur + desaturation
    /// Creates a visually distinct thumbnail for the Overview grid
    private func applyOverviewFilters(to image: NSImage) async -> NSImage? {
        guard let cgImage = image.cgImage(forProposedRect: nil, context: nil, hints: nil) else {
            return image
        }

        let ciImage = CIImage(cgImage: cgImage)

        // Apply Gaussian blur (subtle, radius ~3-5 points)
        guard let blurFilter = CIFilter(name: "CIGaussianBlur") else {
            return image
        }
        blurFilter.setValue(ciImage, forKey: kCIInputImageKey)
        blurFilter.setValue(4.0, forKey: kCIInputRadiusKey)

        guard let blurredImage = blurFilter.outputImage else {
            return image
        }

        // Apply color controls for desaturation (reduce saturation to ~70%)
        guard let colorFilter = CIFilter(name: "CIColorControls") else {
            return NSImage(cgImage: cgImage, size: image.size)
        }
        colorFilter.setValue(blurredImage, forKey: kCIInputImageKey)
        colorFilter.setValue(0.7, forKey: kCIInputSaturationKey)  // 70% saturation
        colorFilter.setValue(0.0, forKey: kCIInputBrightnessKey)  // No brightness change
        colorFilter.setValue(1.0, forKey: kCIInputContrastKey)    // No contrast change

        guard let outputImage = colorFilter.outputImage else {
            return image
        }

        // Render to CGImage
        // Use original image extent to avoid filter edge artifacts
        let extent = ciImage.extent
        guard let finalCGImage = ciContext.createCGImage(outputImage, from: extent) else {
            return image
        }

        return NSImage(cgImage: finalCGImage, size: image.size)
    }
}

