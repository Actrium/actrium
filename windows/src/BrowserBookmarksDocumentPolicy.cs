namespace Actrium.Windows;

internal static class BrowserBookmarksDocumentPolicy
{
    public const string Title = "Bookmarks";

    private const long MinDisplayUnixSeconds = 0;
    private const long MaxDisplayUnixSeconds = 253402300799;

    public static string BuildDocument(IReadOnlyList<BrowserBookmarkItemSnapshot> items)
    {
        return BrowserLocalListDocumentPolicy.BuildDocument(
            new BrowserLocalListDocumentOptions(
                Title,
                "No bookmarks yet.",
                "Search bookmarks",
                "Search bookmarks",
                "No matching bookmarks.",
                "data-remove-bookmark",
                "removeBookmark",
                items
                    .OrderByDescending(item => item.CreatedAtUnixSeconds)
                    .Select(BuildBookmarkItem)
                    .ToList()));
    }

    private static BrowserLocalListDocumentItem BuildBookmarkItem(BrowserBookmarkItemSnapshot item)
    {
        return new BrowserLocalListDocumentItem(
            item.Title,
            item.Address,
            FormatCreatedTime(item.CreatedAtUnixSeconds),
            $"{item.Title} {item.Address}");
    }

    private static string FormatCreatedTime(long createdAtUnixSeconds)
    {
        return DateTimeOffset
            .FromUnixTimeSeconds(Math.Clamp(createdAtUnixSeconds, MinDisplayUnixSeconds, MaxDisplayUnixSeconds))
            .ToLocalTime()
            .ToString("yyyy-MM-dd HH:mm");
    }
}
