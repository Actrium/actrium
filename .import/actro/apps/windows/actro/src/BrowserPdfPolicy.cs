namespace Actro.Windows;

internal static class BrowserPdfPolicy
{
    public const string DefaultPdfFileName = "actro-page.pdf";

    public static string GetSuggestedFileName(string? title, string? address)
    {
        var candidate = !string.IsNullOrWhiteSpace(title) && !string.Equals(title.Trim(), "New Tab", StringComparison.OrdinalIgnoreCase)
            ? title
            : BrowserDownloadPolicy.GetSuggestedFileName(null, address);
        var sanitized = BrowserDownloadPolicy.SanitizeFileName(candidate);
        if (string.Equals(sanitized, BrowserDownloadPolicy.FallbackFileName, StringComparison.OrdinalIgnoreCase))
        {
            return DefaultPdfFileName;
        }

        return sanitized.EndsWith(".pdf", StringComparison.OrdinalIgnoreCase)
            ? sanitized
            : $"{sanitized}.pdf";
    }

    public static string FormatSaveStatus(bool succeeded, string filePath)
    {
        if (!succeeded)
        {
            return "Save as PDF failed";
        }

        var fileName = Path.GetFileName(filePath);
        return string.IsNullOrWhiteSpace(fileName)
            ? "Saved as PDF"
            : $"Saved {fileName}";
    }
}
