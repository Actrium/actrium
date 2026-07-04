using System.Text.Json;

namespace Actro.Windows;

internal static class BrowserSyncStatusPolicy
{
    public const string SyncingStatus = "Syncing browser data...";
    public const string FailedStatus = "Browser data sync failed";
    public const string UnavailableStatus = "Browser data sync status unavailable";

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
                message += $" (last result: {FormatClassification(lastClassification!)})";
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
            "idle" => "Browser data sync is up to date",
            "pending" => "Browser data sync is pending",
            "retryScheduled" when nextRunDelaySeconds.HasValue =>
                $"Browser data sync retry scheduled in {nextRunDelaySeconds.Value}s",
            "retryScheduled" => "Browser data sync retry scheduled",
            "waitingForCredentials" => "Browser data sync needs sign-in",
            "requiresUserAction" when string.Equals(
                lastClassification,
                "clientUpgradeRequired",
                StringComparison.Ordinal) => "Browser data sync requires an app update",
            "requiresUserAction" => "Browser data sync needs attention",
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
        return classification switch
        {
            "credentialsUnavailable" => "credentials unavailable",
            "serverUnavailable" => "server unavailable",
            "clientUpgradeRequired" => "app update required",
            "validationError" => "validation error",
            _ => classification,
        };
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
