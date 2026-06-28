import SwiftUI

/// Native SwiftUI StartPage - Safari-style start page with favorites and history
/// Replaces the React Native StartPage for instant startup without XPC overhead
struct StartPageView: View {
    let bookmarks: [Bookmark]
    let recentHistory: [HistoryItem]
    let containerSize: CGSize
    let isCompactMode: Bool
    let onNavigate: (URL) -> Void

    var body: some View {
        // 🔑 Only use isCompactMode flag, not containerSize
        // Native views are rendered inside TabPanel and scaled by canvas transform,
        // so containerSize is cardSize (small), but actual display size is viewportSize (large)
        if isCompactMode {
            CompactStartPageView(bookmarkCount: bookmarks.count)
        } else {
            FullStartPageView(
                bookmarks: bookmarks,
                recentHistory: recentHistory,
                onNavigate: onNavigate
            )
        }
    }
}

// MARK: - Compact View

/// Compact view shown in Overview mode or small containers
private struct CompactStartPageView: View {
    let bookmarkCount: Int

    var body: some View {
        ZStack {
            StartPageColors.background

            VStack(spacing: 8) {
                Text("🏠")
                    .font(.system(size: 48))

                Text("Start Page")
                    .font(.system(size: 16, weight: .medium))
                    .foregroundColor(.white.opacity(0.8))

                if bookmarkCount > 0 {
                    Text("\(bookmarkCount) favorites")
                        .font(.system(size: 12))
                        .foregroundColor(.white.opacity(0.5))
                }
            }
        }
    }
}

// MARK: - Full View

/// Full start page view with Favorites and History grids
private struct FullStartPageView: View {
    let bookmarks: [Bookmark]
    let recentHistory: [HistoryItem]
    let onNavigate: (URL) -> Void

    var body: some View {
        ZStack(alignment: .topLeading) {
            StartPageColors.background

            ScrollView {
                VStack(alignment: .leading, spacing: 32) {
                    // Favorites Section
                    if !bookmarks.isEmpty {
                        VStack(alignment: .leading, spacing: 16) {
                            StartPageSectionHeader(title: "Favorites")
                            FavoritesGrid(bookmarks: bookmarks) { bookmark in
                                if let url = bookmark.appURL {
                                    onNavigate(url)
                                }
                            }
                        }
                    }

                    // Recently Visited Section
                    if !recentHistory.isEmpty {
                        VStack(alignment: .leading, spacing: 16) {
                            StartPageSectionHeader(title: "Recently Visited")
                            HistoryGrid(items: recentHistory) { item in
                                if let url = item.appURL {
                                    onNavigate(url)
                                }
                            }
                        }
                    }

                    // Empty State
                    if bookmarks.isEmpty && recentHistory.isEmpty {
                        StartPageEmptyState()
                            .frame(maxWidth: .infinity)
                    }
                }
                .padding(40)
            }
        }
    }
}

// MARK: - Preview

#if DEBUG
struct StartPageView_Previews: PreviewProvider {
    static var previews: some View {
        // Full view with content
        StartPageView(
            bookmarks: [
                Bookmark(title: "GitHub", appURL: URL(string: "https://github.com")),
                Bookmark(title: "Apple", appURL: URL(string: "https://apple.com")),
            ],
            recentHistory: [
                HistoryItem(title: "Google", appURL: URL(string: "https://google.com")),
            ],
            containerSize: CGSize(width: 800, height: 600),
            isCompactMode: false,
            onNavigate: { _ in }
        )
        .frame(width: 800, height: 600)
        .previewDisplayName("Full View")

        // Compact view
        StartPageView(
            bookmarks: [
                Bookmark(title: "GitHub", appURL: URL(string: "https://github.com")),
            ],
            recentHistory: [],
            containerSize: CGSize(width: 200, height: 150),
            isCompactMode: true,
            onNavigate: { _ in }
        )
        .frame(width: 200, height: 150)
        .previewDisplayName("Compact View")

        // Empty state
        StartPageView(
            bookmarks: [],
            recentHistory: [],
            containerSize: CGSize(width: 800, height: 600),
            isCompactMode: false,
            onNavigate: { _ in }
        )
        .frame(width: 800, height: 600)
        .previewDisplayName("Empty State")
    }
}
#endif
