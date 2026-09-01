using System.Text.Json.Nodes;

namespace Actro.Windows;

// Renders the Connection row of the Browser Data Sync settings dialog. The
// Rust connection contract owns the classification and user-action
// vocabulary; BrowserStrings owns the user-facing wording
// (status.syncProvider.<classification> and
// status.syncProvider.action.<userAction>); unknown tokens fall back to the
// raw token so a contract addition degrades to a visible-but-unpolished
// label instead of throwing or rendering a string-table key.
internal static class BrowserSyncProviderStatusFormatter
{
    private const string ClassificationKeyPrefix = "status.syncProvider.";
    private const string ActionKeyPrefix = "status.syncProvider.action.";

    public static string FormatNotConfigured()
    {
        return BrowserStrings.Get("status.syncProvider.notConfigured");
    }

    // Truthfulness rule: when no provider is configured but an endpoint
    // credential session exists, the Connection row must not claim "not
    // configured" next to a live "Last sync" row driven by endpoint sync —
    // it names the reference-server (development) state explicitly.
    public static string FormatReferenceServerActive()
    {
        return BrowserStrings.Get("status.syncProvider.referenceServerActive");
    }

    public static string FormatConnectionPolicy(JsonObject? connectionPolicy)
    {
        if (connectionPolicy is null)
        {
            return FormatNotConfigured();
        }

        return Format(
            connectionPolicy["classification"]?.GetValue<string>(),
            connectionPolicy["userAction"]?.GetValue<string>());
    }

    public static string Format(string? classification, string? userAction)
    {
        var normalizedClassification = classification?.Trim() ?? string.Empty;
        if (normalizedClassification.Length == 0)
        {
            // Only an absent policy produces an empty classification; render
            // it as the unconfigured state rather than an empty row.
            return FormatNotConfigured();
        }

        var classificationText = LookupOrRawToken(ClassificationKeyPrefix, normalizedClassification);
        var normalizedAction = userAction?.Trim() ?? string.Empty;
        if (normalizedAction.Length == 0
            || string.Equals(normalizedAction, "none", StringComparison.Ordinal))
        {
            return classificationText;
        }

        return BrowserStrings.Format(
            "status.syncProvider.withActionFormat",
            classificationText,
            LookupOrRawToken(ActionKeyPrefix, normalizedAction));
    }

    private static string LookupOrRawToken(string keyPrefix, string token)
    {
        // Deliberately not BrowserStrings.Get: its fail-soft fallback is the
        // key itself, and "status.syncProvider.mysteryState" on screen is
        // worse than the raw contract token.
        return BrowserStrings.All.TryGetValue(keyPrefix + token, out var text) ? text : token;
    }
}
