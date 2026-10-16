namespace Actrium.Windows;

internal static class BrowserDownloadPolicy
{
    public const string FallbackFileName = "download";
    public const string OpenDownloadsFolderStatus = "Opened Downloads folder";
    public const string OpenDownloadsFolderFailedStatus = "Open Downloads folder failed";
    public const string NoCompletedDownloadStatus = "No completed download yet";
    public const string LastDownloadUnavailableStatus = "Last download is not available";
    public const string OpenedLastDownloadStatus = "Opened last download";
    public const string OpenLastDownloadFailedStatus = "Open last download failed";
    public const string RevealedLastDownloadStatus = "Shown last download in folder";
    public const string RevealLastDownloadFailedStatus = "Show last download in folder failed";

    private static readonly HashSet<char> WindowsInvalidFileNameChars =
        ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

    public static string GetSuggestedFileName(string? resultFilePath, string? uri)
    {
        var fileName = ExtractFileName(resultFilePath);
        if (string.IsNullOrWhiteSpace(fileName))
        {
            fileName = ExtractFileNameFromUri(uri);
        }

        return SanitizeFileName(fileName);
    }

    public static string SanitizeFileName(string? fileName)
    {
        var trimmed = fileName?.Trim() ?? string.Empty;
        if (string.IsNullOrWhiteSpace(trimmed))
        {
            return FallbackFileName;
        }

        var chars = trimmed
            .Select(character => IsInvalidFileNameCharacter(character) ? '_' : character)
            .ToArray();
        var sanitized = new string(chars).Trim().TrimEnd('.');
        return string.IsNullOrWhiteSpace(sanitized) ? FallbackFileName : sanitized;
    }

    public static string FormatProgress(long bytesReceived, long totalBytesToReceive)
    {
        if (bytesReceived < 0)
        {
            bytesReceived = 0;
        }

        if (totalBytesToReceive > 0)
        {
            var percentage = Math.Clamp(bytesReceived * 100.0 / totalBytesToReceive, 0, 100);
            return $"{percentage:0}% ({FormatBytes(bytesReceived)} of {FormatBytes(totalBytesToReceive)})";
        }

        return FormatBytes(bytesReceived);
    }

    public static string GetDefaultDownloadsDirectory()
    {
        return ResolveDownloadsDirectory(
            Environment.GetFolderPath(Environment.SpecialFolder.UserProfile),
            Directory.Exists);
    }

    internal static string ResolveDownloadsDirectory(string? userProfile, Func<string, bool> directoryExists)
    {
        if (string.IsNullOrWhiteSpace(userProfile))
        {
            return string.Empty;
        }

        var downloads = Path.Combine(userProfile, "Downloads");
        return directoryExists(downloads) ? downloads : userProfile;
    }

    public static bool TryGetAvailableDownloadFile(
        string? filePath,
        Func<string, bool> fileExists,
        out string availableFilePath)
    {
        if (!string.IsNullOrWhiteSpace(filePath) && fileExists(filePath))
        {
            availableFilePath = filePath;
            return true;
        }

        availableFilePath = string.Empty;
        return false;
    }

    public static string FormatUnavailableDownloadStatus(string? filePath)
    {
        return string.IsNullOrWhiteSpace(filePath)
            ? NoCompletedDownloadStatus
            : LastDownloadUnavailableStatus;
    }

    public static string BuildExplorerSelectArguments(string filePath)
    {
        return $"/select,\"{filePath.Replace("\"", string.Empty)}\"";
    }

    private static string ExtractFileName(string? filePath)
    {
        if (string.IsNullOrWhiteSpace(filePath))
        {
            return string.Empty;
        }

        var normalizedPath = filePath.Replace('\\', '/').TrimEnd('/');
        var lastSeparator = normalizedPath.LastIndexOf('/');
        return lastSeparator >= 0 ? normalizedPath[(lastSeparator + 1)..] : normalizedPath;
    }

    private static string ExtractFileNameFromUri(string? rawUri)
    {
        if (string.IsNullOrWhiteSpace(rawUri)
            || !Uri.TryCreate(rawUri, UriKind.Absolute, out var uri))
        {
            return string.Empty;
        }

        return Uri.UnescapeDataString(ExtractFileName(uri.LocalPath));
    }

    private static bool IsInvalidFileNameCharacter(char character)
    {
        return char.IsControl(character) || WindowsInvalidFileNameChars.Contains(character);
    }

    private static string FormatBytes(long bytes)
    {
        string[] units = ["B", "KB", "MB", "GB", "TB"];
        var value = (double)bytes;
        var unitIndex = 0;
        while (value >= 1024 && unitIndex < units.Length - 1)
        {
            value /= 1024;
            unitIndex++;
        }

        return unitIndex == 0
            ? $"{bytes} {units[unitIndex]}"
            : $"{value:0.0} {units[unitIndex]}";
    }
}
