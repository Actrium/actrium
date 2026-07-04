namespace Actro.Windows;

internal static class BrowserPermissionPolicy
{
    public static BrowserPermissionPrompt BuildPrompt(string permissionKind, string? uri)
    {
        var permissionLabel = DescribePermission(permissionKind);
        var origin = DescribeOrigin(uri);
        return new BrowserPermissionPrompt(
            $"Allow {permissionLabel}?",
            $"{origin} wants to use {permissionLabel}.",
            permissionLabel,
            origin);
    }

    public static string DescribePermission(string permissionKind)
    {
        return permissionKind switch
        {
            "Camera" => "camera",
            "Microphone" => "microphone",
            "Geolocation" => "location",
            "Notifications" => "notifications",
            "OtherSensors" => "device sensors",
            "ClipboardRead" => "clipboard read access",
            "MultipleAutomaticDownloads" => "multiple automatic downloads",
            "FileReadWrite" => "file read/write access",
            "Autoplay" => "autoplay",
            "LocalFonts" => "local fonts",
            "MidiSystemExclusiveAccess" => "MIDI system exclusive access",
            "WindowManagement" => "window management",
            "UnknownPermission" or "" => "a browser permission",
            _ => SplitPascalCase(permissionKind),
        };
    }

    public static string DescribeOrigin(string? rawUri)
    {
        if (string.IsNullOrWhiteSpace(rawUri)
            || !Uri.TryCreate(rawUri, UriKind.Absolute, out var uri))
        {
            return "This page";
        }

        if (uri.Scheme == Uri.UriSchemeFile)
        {
            return "A local file";
        }

        if (uri.Scheme == Uri.UriSchemeHttp || uri.Scheme == Uri.UriSchemeHttps)
        {
            var port = uri.IsDefaultPort ? string.Empty : $":{uri.Port}";
            return $"{uri.Scheme}://{uri.Host}{port}";
        }

        return string.IsNullOrWhiteSpace(uri.Host)
            ? uri.Scheme
            : $"{uri.Scheme}://{uri.Host}";
    }

    private static string SplitPascalCase(string value)
    {
        if (string.IsNullOrWhiteSpace(value))
        {
            return "a browser permission";
        }

        var words = new List<string>();
        var start = 0;
        for (var index = 1; index < value.Length; index++)
        {
            if (char.IsUpper(value[index]) && !char.IsUpper(value[index - 1]))
            {
                words.Add(value[start..index]);
                start = index;
            }
        }

        words.Add(value[start..]);
        return string.Join(" ", words).ToLowerInvariant();
    }
}

internal sealed record BrowserPermissionPrompt(
    string Title,
    string Message,
    string PermissionLabel,
    string Origin);
