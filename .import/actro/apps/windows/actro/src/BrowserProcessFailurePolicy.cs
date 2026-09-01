namespace Actro.Windows;

internal static class BrowserProcessFailurePolicy
{
    public const string DefaultKind = "Unknown";
    public const string DefaultStatus = "WebView2 process failed";
    public const string RecoveryFailedStatus = "WebView2 recovery failed";

    public static string FormatStatus(string? processFailedKind)
    {
        return $"{DefaultStatus}: {NormalizeKind(processFailedKind)}";
    }

    public static string FormatRecoveryStatus(string? processFailedKind)
    {
        return $"{FormatStatus(processFailedKind)}. Reloading tab...";
    }

    public static string FormatRecoveryFailedStatus(string? processFailedKind)
    {
        return $"{RecoveryFailedStatus}: {NormalizeKind(processFailedKind)}";
    }

    public static bool ShouldReloadTab(string? processFailedKind)
    {
        return NormalizeKind(processFailedKind) is
            "RenderProcessExited" or
            "RenderProcessUnresponsive" or
            "FrameRenderProcessExited";
    }

    private static string NormalizeKind(string? processFailedKind)
    {
        return string.IsNullOrWhiteSpace(processFailedKind)
            ? DefaultKind
            : processFailedKind.Trim();
    }
}
