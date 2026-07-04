namespace Actro.Windows;

internal static class BrowserExternalProtocolPolicy
{
    private static readonly HashSet<string> BlockedExternalSchemes = new(StringComparer.OrdinalIgnoreCase)
    {
        "about",
        "actro",
        "blob",
        "data",
        "file",
        "filesystem",
        "http",
        "https",
        "javascript",
        "vbscript",
        "ws",
        "wss",
    };

    public static bool TryCreateExternalProtocolPrompt(
        string address,
        out BrowserExternalProtocolPrompt prompt)
    {
        if (!Uri.TryCreate(address.Trim(), UriKind.Absolute, out var uri)
            || !IsExternalProtocolUri(uri))
        {
            prompt = BrowserExternalProtocolPrompt.Empty;
            return false;
        }

        var scheme = uri.Scheme.ToLowerInvariant();
        prompt = new BrowserExternalProtocolPrompt(
            "Open external app?",
            $"This page wants to open a {scheme} link in another app.",
            uri.ToString(),
            scheme);
        return true;
    }

    public static bool IsExternalProtocolAddress(string address)
    {
        return Uri.TryCreate(address.Trim(), UriKind.Absolute, out var uri)
            && IsExternalProtocolUri(uri);
    }

    private static bool IsExternalProtocolUri(Uri uri)
    {
        return !NavigationPolicy.IsAllowedNavigationUri(uri)
            && !BlockedExternalSchemes.Contains(uri.Scheme);
    }
}

internal sealed record BrowserExternalProtocolPrompt(
    string Title,
    string Message,
    string Address,
    string Scheme)
{
    public static BrowserExternalProtocolPrompt Empty { get; } = new(
        string.Empty,
        string.Empty,
        string.Empty,
        string.Empty);
}
