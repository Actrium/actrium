//
//  TabSnapshotCache.swift
//  Actro
//
//  Dual-buffer snapshot cache for Tab mode switching animations.
//  - Focus Snapshot: Full viewport size image for Focus mode display
//  - Overview Snapshot: Card-sized image for Overview grid display
//  Both images are pre-processed with slight blur to hide rendering artifacts.
//

import AppKit
import CoreImage

// MARK: - Tab Snapshot Cache

/// Dual-buffer snapshot cache for each tab
struct TabSnapshotCache {
    /// Focus mode snapshot (large, viewport size, with slight blur)
    /// Used when zooming INTO this tab
    var focusSnapshot: NSImage?
    var focusSnapshotTime: Date?

    /// Overview mode snapshot (small, card size, with slight blur)
    /// Used when zooming OUT to show this tab in the grid
    var overviewSnapshot: NSImage?
    var overviewSnapshotTime: Date?

    /// Check if cache has valid snapshots for animation
    var hasValidSnapshots: Bool {
        focusSnapshot != nil && overviewSnapshot != nil
    }
}

// MARK: - Tab Snapshot Cache Manager

/// Global manager for tab snapshot caches
/// Thread-safe singleton that manages dual-buffer caches for all tabs
@MainActor
final class TabSnapshotCacheManager {
    static let shared = TabSnapshotCacheManager()

    private var caches: [UUID: TabSnapshotCache] = [:]

    /// CIContext for efficient image filtering
    private let ciContext = CIContext(options: [.useSoftwareRenderer: false])

    /// Blur radius for pre-processing (subtle blur to hide artifacts)
    private let blurRadius: CGFloat = 2.0

    private init() {}

    // MARK: - Cache Access

    /// Get cache for a tab (returns nil if no cache exists)
    func cache(for tabId: UUID) -> TabSnapshotCache? {
        caches[tabId]
    }

    /// Get Focus snapshot (large) for a tab
    func focusSnapshot(for tabId: UUID) -> NSImage? {
        caches[tabId]?.focusSnapshot
    }

    /// Get Overview snapshot (small) for a tab
    func overviewSnapshot(for tabId: UUID) -> NSImage? {
        caches[tabId]?.overviewSnapshot
    }

    /// Get filtered thumbnail for Overview grid display (legacy API, returns overviewSnapshot)
    func filteredThumbnail(for tabId: UUID) -> NSImage? {
        caches[tabId]?.overviewSnapshot
    }

    // MARK: - Cache Update

    /// Set Focus snapshot (large, viewport size)
    /// Called when entering Focus mode for this tab
    /// - Parameters:
    ///   - image: Raw screenshot of the tab content
    ///   - tabId: Tab identifier
    ///   - applyBlur: Whether to apply blur filter (default: true)
    func setFocusSnapshot(_ image: NSImage, for tabId: UUID, applyBlur: Bool = true) {
        var cache = caches[tabId] ?? TabSnapshotCache()

        if applyBlur {
            // Apply blur in background
            Task.detached(priority: .userInitiated) { [weak self] in
                guard let self = self else { return }
                let processed = await self.applyBlurFilter(to: image)

                await MainActor.run {
                    guard var cache = self.caches[tabId] else {
                        var newCache = TabSnapshotCache()
                        newCache.focusSnapshot = processed
                        newCache.focusSnapshotTime = Date()
                        self.caches[tabId] = newCache
                        return
                    }
                    cache.focusSnapshot = processed
                    cache.focusSnapshotTime = Date()
                    self.caches[tabId] = cache

                    NSLog("📸 TabSnapshotCache: Focus snapshot set for tab %@, size=%@",
                          String(tabId.uuidString.prefix(8)), NSStringFromSize(image.size))
                }
            }
        } else {
            cache.focusSnapshot = image
            cache.focusSnapshotTime = Date()
            caches[tabId] = cache

            NSLog("📸 TabSnapshotCache: Focus snapshot set for tab %@ (no blur), size=%@",
                  String(tabId.uuidString.prefix(8)), NSStringFromSize(image.size))
        }
    }

    /// Set Overview snapshot (small, card size)
    /// Called when exiting Focus mode for this tab
    /// - Parameters:
    ///   - image: Raw screenshot of the tab content (will be resized)
    ///   - tabId: Tab identifier
    ///   - targetSize: Target card size for Overview display
    ///   - applyBlur: Whether to apply blur filter (default: true)
    func setOverviewSnapshot(_ image: NSImage, for tabId: UUID, targetSize: CGSize? = nil, applyBlur: Bool = true) {
        // 同步路径：无模糊时直接设置（resize 也同步执行）
        if !applyBlur {
            var processed = image
            let originalSize = image.size
            let originalRatio = originalSize.width / originalSize.height

            // 同步 resize
            if let targetSize = targetSize, targetSize.width > 0 && targetSize.height > 0 {
                let targetRatio = targetSize.width / targetSize.height
                NSLog("📸 TabSnapshotCache: Overview resize - original=%.1fx%.1f (ratio=%.4f), target=%.1fx%.1f (ratio=%.4f)",
                      originalSize.width, originalSize.height, originalRatio,
                      targetSize.width, targetSize.height, targetRatio)
                processed = resizeImageSync(image, to: targetSize)
            }

            var cache = caches[tabId] ?? TabSnapshotCache()
            cache.overviewSnapshot = processed
            cache.overviewSnapshotTime = Date()
            caches[tabId] = cache

            NSLog("📸 TabSnapshotCache: Overview snapshot set for tab %@ (sync), size=%@",
                  String(tabId.uuidString.prefix(8)), NSStringFromSize(processed.size))
            return
        }

        // 异步路径：有模糊时后台处理
        Task.detached(priority: .userInitiated) { [weak self] in
            guard let self = self else { return }

            var processed = image

            // Resize if target size specified
            if let targetSize = targetSize, targetSize.width > 0 && targetSize.height > 0 {
                processed = await self.resizeImage(image, to: targetSize)
            }

            // Apply blur
            processed = await self.applyBlurFilter(to: processed) ?? processed

            // Capture final value for sendable closure
            let finalImage = processed
            let finalSize = processed.size

            await MainActor.run {
                var cache = self.caches[tabId] ?? TabSnapshotCache()
                cache.overviewSnapshot = finalImage
                cache.overviewSnapshotTime = Date()
                self.caches[tabId] = cache

                NSLog("📸 TabSnapshotCache: Overview snapshot set for tab %@ (async), size=%@",
                      String(tabId.uuidString.prefix(8)), NSStringFromSize(finalSize))
            }
        }
    }

    /// 同步 resize 图像（用于需要立即可用的场景）
    private func resizeImageSync(_ image: NSImage, to targetSize: CGSize) -> NSImage {
        let newImage = NSImage(size: targetSize)
        newImage.lockFocus()

        NSGraphicsContext.current?.imageInterpolation = .high

        image.draw(
            in: NSRect(origin: .zero, size: targetSize),
            from: NSRect(origin: .zero, size: image.size),
            operation: .copy,
            fraction: 1.0
        )

        newImage.unlockFocus()
        return newImage
    }

    /// Set both snapshots at once (for initial setup or refresh)
    func setSnapshots(focus: NSImage, overview: NSImage, for tabId: UUID) {
        setFocusSnapshot(focus, for: tabId)
        setOverviewSnapshot(overview, for: tabId)
    }

    /// Invalidate cache for a tab
    func invalidate(tabId: UUID) {
        caches.removeValue(forKey: tabId)
        NSLog("🗑️ TabSnapshotCache: Cache invalidated for tab %@",
              String(tabId.uuidString.prefix(8)))
    }

    /// Invalidate all caches
    func invalidateAll() {
        caches.removeAll()
        NSLog("🗑️ TabSnapshotCache: All caches invalidated")
    }

    // MARK: - Image Processing

    /// Apply subtle blur filter to hide rendering artifacts
    private func applyBlurFilter(to image: NSImage) async -> NSImage? {
        guard let cgImage = image.cgImage(forProposedRect: nil, context: nil, hints: nil) else {
            return image
        }

        let ciImage = CIImage(cgImage: cgImage)

        guard let blurFilter = CIFilter(name: "CIGaussianBlur") else {
            return image
        }
        blurFilter.setValue(ciImage, forKey: kCIInputImageKey)
        blurFilter.setValue(blurRadius, forKey: kCIInputRadiusKey)

        guard let blurredImage = blurFilter.outputImage else {
            return image
        }

        // Render to CGImage (use original extent to avoid edge artifacts)
        let extent = ciImage.extent
        guard let finalCGImage = ciContext.createCGImage(blurredImage, from: extent) else {
            return image
        }

        return NSImage(cgImage: finalCGImage, size: image.size)
    }

    /// Resize image to target size with high quality
    private func resizeImage(_ image: NSImage, to targetSize: CGSize) async -> NSImage {
        let newImage = NSImage(size: targetSize)
        newImage.lockFocus()

        NSGraphicsContext.current?.imageInterpolation = .high

        image.draw(
            in: NSRect(origin: .zero, size: targetSize),
            from: NSRect(origin: .zero, size: image.size),
            operation: .copy,
            fraction: 1.0
        )

        newImage.unlockFocus()
        return newImage
    }
}
