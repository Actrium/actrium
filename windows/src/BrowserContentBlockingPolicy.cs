namespace Actro.Windows;

internal static class BrowserContentBlockingPolicy
{
    public const bool DefaultIsEnabled = true;
    public const int BlockedStatusCode = 403;
    public const string BlockedReasonPhrase = "Blocked";
    public const string BlockedResponseHeaders = "Content-Type: text/plain; charset=utf-8\r\nCache-Control: no-store";

    private static readonly string[] BlockedHostSuffixes =
    [
        "2mdn.net",
        "adnxs.com",
        "adsrvr.org",
        "doubleclick.net",
        "facebook.net",
        "google-analytics.com",
        "googlesyndication.com",
        "googletagmanager.com",
        "scorecardresearch.com",
    ];

    private static readonly string[] BlockedHostLabels =
    [
        "ad",
        "ads",
        "adservice",
        "analytics",
        "metrics",
        "pixel",
        "stat",
        "stats",
        "tracker",
        "tracking",
    ];

    private static readonly string[] BlockedPathSegments =
    [
        "/adservice/",
        "/ads/",
        "/advertising/",
        "/analytics/",
        "/banners/",
        "/pagead/",
        "/pixel/",
        "/track/",
        "/tracker/",
        "/tracking/",
    ];

    public static bool ShouldBlock(string? requestUri, string? resourceContext)
    {
        if (string.Equals(NormalizeResourceContext(resourceContext), "Document", StringComparison.OrdinalIgnoreCase))
        {
            return false;
        }

        if (!Uri.TryCreate(requestUri, UriKind.Absolute, out var uri))
        {
            return false;
        }

        if (uri.Scheme != Uri.UriSchemeHttp && uri.Scheme != Uri.UriSchemeHttps)
        {
            return false;
        }

        if (uri.IsLoopback)
        {
            return false;
        }

        var host = uri.Host.TrimEnd('.').ToLowerInvariant();
        if (string.IsNullOrWhiteSpace(host))
        {
            return false;
        }

        return HostMatchesBlockedSuffix(host)
            || HostContainsBlockedLabel(host)
            || PathContainsBlockedSegment(uri.AbsolutePath);
    }

    public static string FormatToggleStatus(bool isEnabled)
    {
        return isEnabled
            ? "Content blocking enabled"
            : "Content blocking disabled";
    }

    private static string NormalizeResourceContext(string? resourceContext)
    {
        return string.IsNullOrWhiteSpace(resourceContext) ? string.Empty : resourceContext.Trim();
    }

    private static bool HostMatchesBlockedSuffix(string host)
    {
        return BlockedHostSuffixes.Any(suffix =>
            string.Equals(host, suffix, StringComparison.Ordinal)
            || host.EndsWith($".{suffix}", StringComparison.Ordinal));
    }

    private static bool HostContainsBlockedLabel(string host)
    {
        return host
            .Split('.', StringSplitOptions.RemoveEmptyEntries)
            .Any(label => BlockedHostLabels.Contains(label, StringComparer.Ordinal));
    }

    private static bool PathContainsBlockedSegment(string path)
    {
        var normalizedPath = path.ToLowerInvariant();
        return BlockedPathSegments.Any(segment => normalizedPath.Contains(segment, StringComparison.Ordinal));
    }
}
