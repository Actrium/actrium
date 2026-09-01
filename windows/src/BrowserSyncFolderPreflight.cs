using System.Text.Json;
using System.Text.Json.Nodes;

namespace Actro.Windows;

// Outcome of the local-folder acceptance pipeline. BlockedReasonKey is a
// stable token the settings dialog maps through BrowserStrings
// (dialog.syncSettings.folderBlocked.<key>); DetailText carries the blocking
// stage's action/classification token so the error line can say what to do,
// not just that acceptance failed.
internal sealed record BrowserSyncFolderPreflightResult(
    bool Accept,
    string? BlockedReasonKey,
    string DetailText);

// Accept gating for the local-folder provider connection: choosing a folder
// persists nothing until every stage passes. The stages run in cost order —
// platform probe (IO done by the dialog, booleans arrive here so this
// evaluator stays pure and harness-testable), provider capability gate,
// object-space adapter plan, and the cloud-bytes compatibility gate over
// whatever already lives in the folder. The Rust core owns every
// classification; this class only sequences the gates and extracts the
// blocking token.
internal static class BrowserSyncFolderPreflight
{
    public const string FolderMissingReason = "folderMissing";
    public const string FolderNotWritableReason = "folderNotWritable";
    public const string CapabilityBlockedReason = "capabilityBlocked";
    public const string AdapterBlockedReason = "adapterBlocked";
    public const string CompatibilityBlockedReason = "compatibilityBlocked";

    public static BrowserSyncFolderPreflightResult Evaluate(
        bool directoryExists,
        bool writeProbeSucceeded,
        string capabilityPreflightJson,
        string adapterPlanJson,
        string compatibilityPreflightJson)
    {
        // 1. Platform probe. A folder that does not exist or rejects the
        //    write probe can never host the sync store, so the contract
        //    gates are not even consulted.
        if (!directoryExists)
        {
            return Blocked(FolderMissingReason, string.Empty);
        }

        if (!writeProbeSucceeded)
        {
            return Blocked(FolderNotWritableReason, string.Empty);
        }

        // 2. Provider capability gate (ready / degraded / setup-required).
        //    An invalid plan is contract drift and blocks acceptance rather
        //    than letting an unvalidated folder into the settings store.
        var capabilityPreflight = ParseObject(capabilityPreflightJson);
        if (capabilityPreflight is null
            || ReadBool(capabilityPreflight, "shouldAllowSync") != true
            || ReadBool(capabilityPreflight, "shouldBlockSync") == true)
        {
            return Blocked(
                CapabilityBlockedReason,
                ReadString(capabilityPreflight, "classification"));
        }

        // 3. Object-space adapter plan: sync must be allowed; otherwise the
        //    rootPathAction (or the classification when no root action is
        //    named) is surfaced as the thing to fix.
        var adapterPlan = ParseObject(adapterPlanJson);
        if (adapterPlan is null || ReadBool(adapterPlan, "shouldAllowSync") != true)
        {
            var rootPathAction = ReadString(adapterPlan, "rootPathAction");
            return Blocked(
                AdapterBlockedReason,
                rootPathAction.Length > 0
                    ? rootPathAction
                    : ReadString(adapterPlan, "classification"));
        }

        // 4. Cloud-bytes compatibility gate: a clientUpgradeRequired or
        //    invalid outcome blocks acceptance with the policy's user action
        //    rendered as the error line.
        var compatibilityPreflight = ParseObject(compatibilityPreflightJson);
        if (compatibilityPreflight is null
            || ReadBool(compatibilityPreflight, "shouldBlockSync") == true)
        {
            var userAction = ReadString(compatibilityPreflight, "userAction");
            return Blocked(
                CompatibilityBlockedReason,
                userAction.Length > 0 && !string.Equals(userAction, "none", StringComparison.Ordinal)
                    ? userAction
                    : ReadString(compatibilityPreflight, "classification"));
        }

        return new BrowserSyncFolderPreflightResult(
            Accept: true,
            BlockedReasonKey: null,
            DetailText: string.Empty);
    }

    private static BrowserSyncFolderPreflightResult Blocked(string reasonKey, string detailText)
    {
        return new BrowserSyncFolderPreflightResult(
            Accept: false,
            BlockedReasonKey: reasonKey,
            DetailText: detailText);
    }

    private static string ReadString(JsonObject? plan, string propertyName)
    {
        return plan?[propertyName]?.GetValue<string>() ?? string.Empty;
    }

    private static bool? ReadBool(JsonObject plan, string propertyName)
    {
        return plan[propertyName] is JsonValue value && value.TryGetValue<bool>(out var parsed)
            ? parsed
            : null;
    }

    private static JsonObject? ParseObject(string json)
    {
        if (string.Equals(json, "null", StringComparison.Ordinal))
        {
            return null;
        }

        try
        {
            return JsonNode.Parse(json) as JsonObject;
        }
        catch (JsonException)
        {
            return null;
        }
    }
}
