namespace Actro.Windows;

internal static class BrowserHistoryDocumentPolicy
{
    public const string Title = "History";

    private const long MinDisplayUnixSeconds = 0;
    private const long MaxDisplayUnixSeconds = 253402300799;

    public static string BuildDocument(IReadOnlyList<BrowserHistoryItemSnapshot> items)
    {
        return BrowserLocalListDocumentPolicy.BuildDocument(
            new BrowserLocalListDocumentOptions(
                Title,
                "No history yet.",
                "Search history",
                "Search history",
                "No matching history.",
                "data-remove-history",
                "removeHistoryEntry",
                items
                    .OrderByDescending(item => item.VisitedAtUnixSeconds)
                    .Select(BuildHistoryItem)
                    .ToList()));
    }

    private static BrowserLocalListDocumentItem BuildHistoryItem(BrowserHistoryItemSnapshot item)
    {
        return new BrowserLocalListDocumentItem(
            item.Title,
            item.Address,
            $"{FormatVisitTime(item.VisitedAtUnixSeconds)}{FormatVisitCount(item.VisitCount)}",
            $"{item.Title} {item.Address}");
    }

    private static string FormatVisitTime(long visitedAtUnixSeconds)
    {
        return DateTimeOffset
            .FromUnixTimeSeconds(Math.Clamp(visitedAtUnixSeconds, MinDisplayUnixSeconds, MaxDisplayUnixSeconds))
            .ToLocalTime()
            .ToString("yyyy-MM-dd HH:mm");
    }

    private static string FormatVisitCount(int visitCount)
    {
        return visitCount > 1 ? $" - {visitCount} visits" : string.Empty;
    }
}
