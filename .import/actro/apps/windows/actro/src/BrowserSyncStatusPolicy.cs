using System.Text.Json;

namespace Actro.Windows;

// Renders the scheduler's browser sync status JSON as the one-line status
// message. All display wording lives in BrowserStrings (status.sync.*);
// this policy owns only the status/classification mapping rules. Unknown
// classification tokens render raw so contract growth stays visible on
// screen instead of hiding behind a generic label. Harness-linked:
// WinForms-free, output pinned by the status string parity matrix.
internal static class BrowserSyncStatusPolicy
{
    public static string SyncingStatus => BrowserStrings.Get("status.sync.syncing");
    public static string FailedStatus => BrowserStrings.Get("status.sync.failed");
    public static string UnavailableStatus => BrowserStrings.Get("status.sync.unavailable");

    public static string FormatStatusJson(string? statusJson)
    {
        if (string.IsNullOrWhiteSpace(statusJson))
        {
            return UnavailableStatus;
        }

        try
        {
            using var document = JsonDocument.Parse(statusJson);
            var root = document.RootElement;
            var status = JsonString(root, "status");
            if (string.IsNullOrWhiteSpace(status))
            {
                return UnavailableStatus;
            }

            var lastClassification = JsonString(root, "lastClassification");
            var message = FormatStatus(status, JsonUInt32(root, "nextRunDelaySeconds"), lastClassification);
            if (ShouldAppendLastClassification(status, lastClassification))
            {
                message = BrowserStrings.Format(
                    "status.sync.lastResultFormat",
                    message,
                    FormatClassification(lastClassification!));
            }

            return message;
        }
        catch (JsonException)
        {
            return UnavailableStatus;
        }
    }

    private static string FormatStatus(string status, uint? nextRunDelaySeconds, string? lastClassification)
    {
        return status switch
        {
            "idle" => BrowserStrings.Get("status.sync.idle"),
            "pending" => BrowserStrings.Get("status.sync.pending"),
            "retryScheduled" when nextRunDelaySeconds.HasValue =>
                BrowserStrings.Format("status.sync.retryScheduledFormat", nextRunDelaySeconds.Value),
            "retryScheduled" => BrowserStrings.Get("status.sync.retryScheduled"),
            // In the post-endpoint architecture a credentialsUnavailable
            // classification is reachable only when no provider is configured
            // and no usable endpoint session exists — i.e. the "nothing
            // configured" state, not a sign-in prompt. The dev endpoint path
            // returning authRequired also lands on waitingForCredentials but
            // carries a different lastClassification, so the two stay
            // disambiguated and only that case still reads "needs sign-in".
            "waitingForCredentials" when string.Equals(
                lastClassification,
                "credentialsUnavailable",
                StringComparison.Ordinal) => BrowserStrings.Get("status.sync.notConfigured"),
            "waitingForCredentials" => BrowserStrings.Get("status.sync.needsSignIn"),
            "requiresUserAction" when string.Equals(
                lastClassification,
                "clientUpgradeRequired",
                StringComparison.Ordinal) => BrowserStrings.Get("status.sync.requiresAppUpdate"),
            "requiresUserAction" => BrowserStrings.Get("status.sync.needsAttention"),
            _ => UnavailableStatus,
        };
    }

    private static bool ShouldAppendLastClassification(string status, string? classification)
    {
        if (string.Equals(status, "waitingForCredentials", StringComparison.Ordinal)
            && string.Equals(classification, "credentialsUnavailable", StringComparison.Ordinal))
        {
            return false;
        }

        if (string.Equals(status, "requiresUserAction", StringComparison.Ordinal)
            && string.Equals(classification, "clientUpgradeRequired", StringComparison.Ordinal))
        {
            return false;
        }

        return !string.IsNullOrWhiteSpace(classification)
            && !string.Equals(classification, "ok", StringComparison.OrdinalIgnoreCase)
            && !string.Equals(classification, "success", StringComparison.OrdinalIgnoreCase);
    }

    private static string FormatClassification(string classification)
    {
        // Deliberately not BrowserStrings.Get: its fail-soft fallback is the
        // key itself, and "status.sync.result.mysteryToken" on screen is
        // worse than the raw contract token.
        return BrowserStrings.All.TryGetValue($"status.sync.result.{classification}", out var text)
            ? text
            : classification;
    }

    private static string? JsonString(JsonElement root, string propertyName)
    {
        return root.TryGetProperty(propertyName, out var value) && value.ValueKind == JsonValueKind.String
            ? value.GetString()
            : null;
    }

    private static uint? JsonUInt32(JsonElement root, string propertyName)
    {
        if (!root.TryGetProperty(propertyName, out var value) || value.ValueKind != JsonValueKind.Number)
        {
            return null;
        }

        return value.TryGetUInt32(out var result) ? result : null;
    }
}
