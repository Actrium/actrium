namespace Actrium.Windows;

internal static class BrowserPageActionPolicy
{
    public const string CopiedAddressStatus = "Copied page address";
    public const string NoCopyableAddressStatus = "No page address to copy";
    public const string OpenedExternalStatus = "Opened in default browser";
    public const string NoExternalAddressStatus = "This page cannot be opened in the default browser";
    public const string OpenExternalFailedStatus = "Open in default browser failed";

    public static bool TryGetCopyableAddress(string? address, out string copyableAddress)
    {
        if (string.IsNullOrWhiteSpace(address))
        {
            copyableAddress = string.Empty;
            return false;
        }

        if (NavigationPolicy.IsHomeAddress(address))
        {
            copyableAddress = NavigationPolicy.HomeAddress;
            return true;
        }

        if (NavigationPolicy.IsHistoryAddress(address))
        {
            copyableAddress = NavigationPolicy.HistoryAddress;
            return true;
        }

        if (NavigationPolicy.IsBookmarksAddress(address))
        {
            copyableAddress = NavigationPolicy.BookmarksAddress;
            return true;
        }

        if (NavigationPolicy.TryNormalizeAddress(address, out var normalizedAddress))
        {
            copyableAddress = normalizedAddress;
            return true;
        }

        copyableAddress = string.Empty;
        return false;
    }

    public static bool TryGetExternalAddress(string? address, out string externalAddress)
    {
        if (string.IsNullOrWhiteSpace(address)
            || NavigationPolicy.IsInternalAddress(address)
            || !NavigationPolicy.TryNormalizeAddress(address, out var normalizedAddress)
            || !Uri.TryCreate(normalizedAddress, UriKind.Absolute, out var uri))
        {
            externalAddress = string.Empty;
            return false;
        }

        if (uri.Scheme == Uri.UriSchemeHttp
            || uri.Scheme == Uri.UriSchemeHttps
            || uri.Scheme == Uri.UriSchemeFile)
        {
            externalAddress = normalizedAddress;
            return true;
        }

        externalAddress = string.Empty;
        return false;
    }
}
