//
//  TabSnapshotCache.swift
//  Actrium
//
//  Single-buffer snapshot cache for Tab mode switching animations.
//
//  Each tab keeps exactly one image — the latest capture of its page. The
//  cache is refreshed whenever the page changes (navigation, URL change) or
//  when geometry changes (window resize), so the mode-transition animation
//  always reads an immediately available, current image. Display layers
//  scale it to their own bounds (contentsGravity = .resize), which is why no
//  separate focus/overview buffers are kept.
//
//  Storage is synchronous: an image written here is visible to the very next
//  transition, with no async processing window in which the animation could
//  fall back to the gray base layer.
//

import AppKit

// MARK: - Tab Snapshot Cache

/// Latest-snapshot cache for a single tab
struct TabSnapshotCache {
    /// The most recent capture of the tab's page, at the size it was taken
    var snapshot: NSImage?
    var snapshotTime: Date?

    /// Check if cache has a valid snapshot for animation
    var hasValidSnapshot: Bool {
        snapshot != nil
    }
}

// MARK: - Tab Snapshot Cache Manager

/// Global manager for tab snapshot caches
/// Thread-safe singleton that manages one latest snapshot per tab
@MainActor
final class TabSnapshotCacheManager {
    static let shared = TabSnapshotCacheManager()

    private var caches: [UUID: TabSnapshotCache] = [:]

    private init() {}

    // MARK: - Cache Access

    /// Get cache for a tab (returns nil if no cache exists)
    func cache(for tabId: UUID) -> TabSnapshotCache? {
        caches[tabId]
    }

    /// Snapshot used when zooming INTO this tab (focus zoom)
    func focusSnapshot(for tabId: UUID) -> NSImage? {
        caches[tabId]?.snapshot
    }

    /// Snapshot used when zooming OUT to this tab in the grid
    func overviewSnapshot(for tabId: UUID) -> NSImage? {
        caches[tabId]?.snapshot
    }

    /// Latest snapshot for Overview grid display (legacy API name)
    func filteredThumbnail(for tabId: UUID) -> NSImage? {
        caches[tabId]?.snapshot
    }

    /// The tab's single latest snapshot (nil if never captured)
    func snapshot(for tabId: UUID) -> NSImage? {
        caches[tabId]?.snapshot
    }

    // MARK: - Cache Update

    /// Store the latest snapshot for a tab.
    /// Only the most recent image is kept — the previous one is replaced
    /// synchronously, so a transition started right after this call always
    /// reads current content and never falls back to the gray base layer.
    func setSnapshot(_ image: NSImage, for tabId: UUID) {
        var cache = caches[tabId] ?? TabSnapshotCache()
        cache.snapshot = image
        cache.snapshotTime = Date()
        caches[tabId] = cache

        NSLog("📸 TabSnapshotCache: snapshot updated for tab %@, size=%@",
              String(tabId.uuidString.prefix(8)), NSStringFromSize(image.size))
    }

    /// Set Focus snapshot (large, viewport size)
    /// Kept for source compatibility — funnels into the single latest buffer.
    func setFocusSnapshot(_ image: NSImage, for tabId: UUID, applyBlur: Bool = true) {
        setSnapshot(image, for: tabId)
    }

    /// Set Overview snapshot (small, card size)
    /// Kept for source compatibility — funnels into the single latest buffer.
    /// The stored image keeps its capture resolution; display layers scale it.
    func setOverviewSnapshot(
        _ image: NSImage,
        for tabId: UUID,
        targetSize: CGSize? = nil,
        applyBlur: Bool = true
    ) {
        setSnapshot(image, for: tabId)
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
}
