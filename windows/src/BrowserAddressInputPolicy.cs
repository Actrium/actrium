namespace Actro.Windows;

internal static class BrowserAddressInputPolicy
{
    public static string DefaultSearchUrlPrefix => BrowserSearchEnginePolicy.DefaultSearchUrlPrefix;

    public static bool TryResolveAddressInput(
        string input,
        out string resolvedAddress,
        out bool isSearch)
    {
        return TryResolveAddressInput(
            input,
            BrowserSearchEnginePolicy.DefaultEngineId,
            out resolvedAddress,
            out isSearch);
    }

    public static bool TryResolveAddressInput(
        string input,
        string searchEngineId,
        out string resolvedAddress,
        out bool isSearch)
    {
        var trimmed = input.Trim();
        if (string.IsNullOrWhiteSpace(trimmed))
        {
            resolvedAddress = string.Empty;
            isSearch = false;
            return false;
        }

        if (NavigationPolicy.IsHomeAddress(trimmed))
        {
            resolvedAddress = NavigationPolicy.HomeAddress;
            isSearch = false;
            return true;
        }

        if (NavigationPolicy.IsHistoryAddress(trimmed))
        {
            resolvedAddress = NavigationPolicy.HistoryAddress;
            isSearch = false;
            return true;
        }

        if (NavigationPolicy.IsBookmarksAddress(trimmed))
        {
            resolvedAddress = NavigationPolicy.BookmarksAddress;
            isSearch = false;
            return true;
        }

        if (NavigationPolicy.TryNormalizeAddress(trimmed, out var normalizedAddress))
        {
            resolvedAddress = normalizedAddress;
            isSearch = false;
            return true;
        }

        if (Uri.TryCreate(trimmed, UriKind.Absolute, out var explicitUri)
            && !NavigationPolicy.IsAllowedNavigationUri(explicitUri))
        {
            resolvedAddress = string.Empty;
            isSearch = false;
            return false;
        }

        resolvedAddress = BrowserSearchEnginePolicy.BuildSearchUrl(trimmed, searchEngineId);
        isSearch = true;
        return true;
    }
}
