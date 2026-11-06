namespace Actrium.Windows;

internal static class NavigationPolicy
{
    public const string HomeAddress = "about:home";
    public const string HistoryAddress = "about:history";
    public const string BookmarksAddress = "about:bookmarks";

    public static bool TryNormalizeAddress(string address, out string normalizedAddress)
    {
        var trimmed = address.Trim();
        if (string.IsNullOrWhiteSpace(trimmed))
        {
            normalizedAddress = string.Empty;
            return false;
        }

        if (Uri.TryCreate(trimmed, UriKind.Absolute, out var absoluteUri))
        {
            if (IsAllowedNavigationUri(absoluteUri))
            {
                normalizedAddress = absoluteUri.AbsoluteUri;
                return true;
            }

            if (trimmed.Contains("://", StringComparison.Ordinal))
            {
                normalizedAddress = string.Empty;
                return false;
            }
        }

        if (TryNormalizeLocalFilePath(trimmed, out normalizedAddress))
        {
            return true;
        }

        if (trimmed.Contains(' ', StringComparison.Ordinal))
        {
            normalizedAddress = string.Empty;
            return false;
        }

        var withHttps = $"https://{trimmed}";
        if (Uri.TryCreate(withHttps, UriKind.Absolute, out var httpsUri)
            && !string.IsNullOrWhiteSpace(httpsUri.Host))
        {
            normalizedAddress = httpsUri.AbsoluteUri;
            return true;
        }

        normalizedAddress = string.Empty;
        return false;
    }

    public static bool TryNormalizeLocalFilePath(string path, out string normalizedAddress)
    {
        var trimmed = path.Trim();
        if (string.IsNullOrWhiteSpace(trimmed) || !Path.IsPathFullyQualified(trimmed))
        {
            normalizedAddress = string.Empty;
            return false;
        }

        try
        {
            var fileUri = new Uri(Path.GetFullPath(trimmed));
            if (IsAllowedNavigationUri(fileUri))
            {
                normalizedAddress = fileUri.AbsoluteUri;
                return true;
            }
        }
        catch (ArgumentException)
        {
        }
        catch (NotSupportedException)
        {
        }
        catch (PathTooLongException)
        {
        }

        normalizedAddress = string.Empty;
        return false;
    }

    public static bool IsHomeAddress(string address)
    {
        return string.Equals(address.Trim(), HomeAddress, StringComparison.OrdinalIgnoreCase);
    }

    public static bool IsHistoryAddress(string address)
    {
        return string.Equals(address.Trim(), HistoryAddress, StringComparison.OrdinalIgnoreCase);
    }

    public static bool IsBookmarksAddress(string address)
    {
        return string.Equals(address.Trim(), BookmarksAddress, StringComparison.OrdinalIgnoreCase);
    }

    public static bool IsInternalAddress(string address)
    {
        return IsHomeAddress(address) || IsHistoryAddress(address) || IsBookmarksAddress(address);
    }

    public static bool IsAllowedNavigationAddress(string address)
    {
        return Uri.TryCreate(address.Trim(), UriKind.Absolute, out var uri)
            && IsAllowedNavigationUri(uri);
    }

    public static bool IsAllowedNavigationUri(Uri uri)
    {
        return uri.Scheme == Uri.UriSchemeHttp
            || uri.Scheme == Uri.UriSchemeHttps
            || uri.Scheme == Uri.UriSchemeFile
            || uri.Scheme == "about";
    }

    /// <summary>
    /// WebView2 surfaces NavigateToString content as a data:text/html
    /// navigation (older runtimes reported about:blank). Only these exact
    /// shapes may bypass the address allow-list, and only while the tab
    /// has a pending app-generated document — arbitrary data: URIs stay
    /// blocked for top-level navigation, matching Chrome behavior.
    /// </summary>
    public static bool IsGeneratedDocumentUri(string uri)
    {
        var trimmed = uri.Trim();
        return trimmed.StartsWith("data:text/html", StringComparison.OrdinalIgnoreCase)
            || string.Equals(trimmed, "about:blank", StringComparison.OrdinalIgnoreCase);
    }
}
