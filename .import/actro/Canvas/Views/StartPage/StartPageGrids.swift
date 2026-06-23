import SwiftUI

// MARK: - Favorites Grid

/// Grid of favorite bookmarks, matching RN StartPage layout
struct FavoritesGrid: View {
    let bookmarks: [Bookmark]
    let onSelect: (Bookmark) -> Void

    // Layout constants matching RN implementation
    private let iconSize: CGFloat = 64
    private let itemWidth: CGFloat = 80
    private let spacing: CGFloat = 20

    var body: some View {
        LazyVGrid(
            columns: [GridItem(.adaptive(minimum: itemWidth), spacing: spacing)],
            alignment: .leading,
            spacing: spacing
        ) {
            ForEach(bookmarks.prefix(16)) { bookmark in
                FavoriteItem(bookmark: bookmark)
                    .onTapGesture {
                        onSelect(bookmark)
                    }
            }
        }
    }
}

/// Single favorite item
private struct FavoriteItem: View {
    let bookmark: Bookmark

    private let iconSize: CGFloat = 64
    private let titleHeight: CGFloat = 30
    private let itemWidth: CGFloat = 80

    var body: some View {
        let firstChar = String(bookmark.title.prefix(1)).uppercased()
        let colors = StartPageColors.colors(for: bookmark.title)

        VStack(spacing: 8) {
            ZStack {
                RoundedRectangle(cornerRadius: 12)
                    .fill(colors.background)
                    .frame(width: iconSize, height: iconSize)

                Text(firstChar)
                    .font(.system(size: 28, weight: .medium))
                    .foregroundColor(colors.text)
            }

            Text(bookmark.title)
                .font(.system(size: 11))
                .foregroundColor(StartPageColors.favoriteTitle)
                .lineLimit(2)
                .truncationMode(.tail)
                .multilineTextAlignment(.center)
                .frame(width: itemWidth, height: titleHeight, alignment: .top)
        }
        .frame(width: itemWidth, height: iconSize + 8 + titleHeight, alignment: .top)
        .contentShape(Rectangle())
    }
}

// MARK: - History Grid

/// Grid of recent history items, matching RN StartPage layout
struct HistoryGrid: View {
    let items: [HistoryItem]
    let onSelect: (HistoryItem) -> Void

    // Layout constants matching RN implementation
    private let iconSize: CGFloat = 56
    private let itemWidth: CGFloat = 70
    private let spacing: CGFloat = 16

    var body: some View {
        LazyVGrid(
            columns: [GridItem(.adaptive(minimum: itemWidth), spacing: spacing)],
            alignment: .leading,
            spacing: spacing
        ) {
            ForEach(items.prefix(16)) { item in
                HistoryGridItem(item: item)
                    .onTapGesture {
                        onSelect(item)
                    }
            }
        }
    }
}

/// Single history item
private struct HistoryGridItem: View {
    let item: HistoryItem

    private let iconSize: CGFloat = 56
    private let titleHeight: CGFloat = 14
    private let itemWidth: CGFloat = 70

    var body: some View {
        let domain = extractDomain(from: item.appURL)
        let firstChar = String(domain.prefix(1)).uppercased()

        VStack(spacing: 6) {
            ZStack {
                RoundedRectangle(cornerRadius: 10)
                    .fill(StartPageColors.historyIconBackground)
                    .frame(width: iconSize, height: iconSize)

                Text(firstChar)
                    .font(.system(size: 24, weight: .medium))
                    .foregroundColor(StartPageColors.historyIconText)
            }

            Text(domain)
                .font(.system(size: 10))
                .foregroundColor(StartPageColors.historyTitle)
                .lineLimit(1)
                .truncationMode(.tail)
                .frame(width: itemWidth, height: titleHeight, alignment: .top)
        }
        .frame(width: itemWidth, height: iconSize + 6 + titleHeight, alignment: .top)
        .contentShape(Rectangle())
    }

    /// Extract domain from URL, removing www. prefix
    private func extractDomain(from url: URL?) -> String {
        guard let url = url, let host = url.host else {
            return url?.absoluteString ?? ""
        }
        return host.hasPrefix("www.") ? String(host.dropFirst(4)) : host
    }
}

// MARK: - Empty State

/// Empty state view when no bookmarks or history
struct StartPageEmptyState: View {
    var body: some View {
        VStack(spacing: 16) {
            Text("🌐")
                .font(.system(size: 64))
                .opacity(0.3)

            Text("Enter a URL or drop an app")
                .font(.system(size: 16))
                .foregroundColor(StartPageColors.emptyTitle)

            Text("Your favorites and recent history will appear here")
                .font(.system(size: 13))
                .foregroundColor(StartPageColors.emptySubtitle)
                .multilineTextAlignment(.center)
        }
        .padding(.vertical, 120)
    }
}

// MARK: - Section Header

/// Section header with title
struct StartPageSectionHeader: View {
    let title: String

    var body: some View {
        Text(title)
            .font(.system(size: 20, weight: .semibold))
            .foregroundColor(StartPageColors.sectionTitle)
    }
}
