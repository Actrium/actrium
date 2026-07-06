using System.Text.Json;
using System.Text.Json.Nodes;

namespace Actro.Windows;

internal enum BrowserSyncProviderRouting
{
    // No provider is configured: browser sync falls through to the
    // endpoint-token reference-server branch (development path).
    NotConfigured,

    // The device has not joined the cloud store yet (no persisted
    // membership marker): run the one-time membership join round first.
    RunLocalFolderMembership,

    // Connection is ready: run the local-folder sync cycle round.
    RunLocalFolderCycle,

    // The connection status policy demands setup, user action, or a retry
    // before any round may run.
    Blocked,
}

internal sealed record BrowserSyncProviderConnectionPlan(
    BrowserSyncProviderRouting Routing,
    JsonObject? ProviderSnapshot,
    JsonObject? ConnectionPolicy);

// Assembles the provider snapshot for the Rust cloud provider connection
// contract and derives the coordinator routing from the resulting policy.
// Pure by design: no IO — the accept-time folder preflight and each round's
// own capability/compatibility preflights are the authoritative folder
// checks, so the snapshot is fully determined by persisted settings plus
// in-memory session state.
internal static class BrowserSyncProviderConnectionPlanner
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);

    // The "provider present AND (not localFolder OR root present)" predicate
    // lives in the shared Rust normalization contract, which stamps the
    // result onto the snapshot's Configured flag. Reading it here keeps the
    // rule single-sourced instead of re-deriving it natively. Callers pass
    // store-loaded (already normalized) snapshots.
    public static bool IsConfigured(BrowserSyncProviderSettingsSnapshot? settings)
    {
        return settings?.Configured ?? false;
    }

    public static JsonObject BuildProviderSnapshot(
        BrowserSyncProviderSettingsSnapshot settings,
        string? lastProviderErrorKind)
    {
        var snapshot = new JsonObject
        {
            ["providerKind"] = settings.ProviderKind,
            ["scope"] = settings.Scope,
            ["deviceId"] = settings.DeviceId,
        };

        if (settings.ProviderKind == BrowserSyncProviderSettingsStore.LocalFolderProviderKind)
        {
            // Local folder is connection-free: there is no session to open,
            // so a configured folder is by definition "connected", and the
            // credential state is constantly "available" because there are
            // no secrets. (The connection contract normalizes unknown
            // connection tokens to "disconnected", which would misclassify
            // a configured local folder as signInRequired.)
            snapshot["connectionState"] = "connected";
            snapshot["credentialState"] = "available";
            // The persisted membership marker is the only cold-start
            // membership signal: a membership round is a write to the
            // shared cloud folder, and repeating it on every app launch is
            // wrong even though the device-document upsert is idempotent.
            // "missing" routes this run to a one-time join; after a
            // successful join the marker persists and later launches go
            // straight to the cycle round.
            snapshot["membershipStatus"] =
                settings.MembershipJoinedAtUnixSeconds is not null ? "active" : "missing";
            // No IO here: the folder was validated when it was accepted and
            // the cycle round re-validates it authoritatively, so the store
            // is reported ready and a real storage failure comes back as a
            // storage-scoped provider error on the next round.
            snapshot["cloudStoreReady"] = true;
        }

        if (!string.IsNullOrEmpty(lastProviderErrorKind))
        {
            snapshot["lastProviderErrorKind"] = lastProviderErrorKind;
        }

        return snapshot;
    }

    public static BrowserSyncProviderConnectionPlan Plan(
        BrowserSyncProviderSettingsSnapshot? settings,
        string? lastProviderErrorKind,
        uint retryAttempt,
        long observedAtUnixSeconds)
    {
        if (!IsConfigured(settings))
        {
            // Short-circuit without calling the policy: an unconfigured
            // provider is the endpoint branch's territory, not a blocked
            // provider connection.
            return new BrowserSyncProviderConnectionPlan(
                BrowserSyncProviderRouting.NotConfigured,
                ProviderSnapshot: null,
                ConnectionPolicy: null);
        }

        var providerSnapshot = BuildProviderSnapshot(settings!, lastProviderErrorKind);
        // The Rust core owns the connection classification and action rules.
        // A result that is not a JSON object is contract drift and must fail
        // loudly instead of degrading to a hand-copied C# policy.
        var connectionPolicy = JsonNodeFromObjectJson(
            ActroCore.BuildBrowserStorageSyncCloudProviderConnectionStatusPolicyJson(
                providerSnapshot.ToJsonString(JsonOptions),
                retryAttempt,
                observedAtUnixSeconds)) as JsonObject
            ?? throw new InvalidOperationException(
                "Browser sync cloud provider connection status policy from the Rust core is not a JSON object.");

        var isLocalFolder = settings!.ProviderKind
            == BrowserSyncProviderSettingsStore.LocalFolderProviderKind;
        var routing = isLocalFolder
            && connectionPolicy["shouldAttemptMembership"]?.GetValue<bool>() == true
                ? BrowserSyncProviderRouting.RunLocalFolderMembership
                : isLocalFolder && connectionPolicy["shouldAttemptSync"]?.GetValue<bool>() == true
                    ? BrowserSyncProviderRouting.RunLocalFolderCycle
                    : BrowserSyncProviderRouting.Blocked;

        return new BrowserSyncProviderConnectionPlan(routing, providerSnapshot, connectionPolicy);
    }

    // Translates the connection status policy output (schedulerAction /
    // retryDelaySeconds / userAction / resetRetryAttempt) into the field
    // shape the sync scheduler consumes. The HTTP-era status policy helper
    // must not be fed cloud-only classifications, and no round envelope
    // exists on the Blocked routing, so this pure translation is the only
    // scheduleState source for blocked provider connections.
    public static JsonObject BuildBlockedScheduleState(
        JsonObject connectionPolicy,
        uint retryAttempt,
        long observedAtUnixSeconds)
    {
        var schedulerAction = connectionPolicy["schedulerAction"]?.GetValue<string>() ?? string.Empty;
        var userAction = connectionPolicy["userAction"]?.GetValue<string>() ?? string.Empty;
        var requiresUserAction = userAction.Length > 0
            && !string.Equals(userAction, "none", StringComparison.Ordinal);

        var scheduleState = new JsonObject
        {
            ["planType"] = "cloudProviderConnectionScheduleState",
            ["classification"] = connectionPolicy["classification"]?.GetValue<string>() ?? string.Empty,
            ["schedulerAction"] = schedulerAction,
            // Local folder never waits for credentials; cloud providers
            // that do surface it through their own classifications, not
            // through the endpoint credential wait.
            ["waitingForCredentials"] = false,
            ["requiresUserAction"] = requiresUserAction,
            ["userAction"] = userAction,
            ["resetRetryAttempt"] = connectionPolicy["resetRetryAttempt"]?.GetValue<bool>() ?? false,
        };

        if (TryReadUInt(connectionPolicy["retryDelaySeconds"]) is uint retryDelaySeconds)
        {
            scheduleState["nextRunDelaySeconds"] = retryDelaySeconds;
            scheduleState["nextRunAtUnixSeconds"] = observedAtUnixSeconds + retryDelaySeconds;
        }

        if (string.Equals(schedulerAction, "scheduleRetry", StringComparison.Ordinal))
        {
            scheduleState["nextRetryAttempt"] = retryAttempt + 1;
        }

        return scheduleState;
    }

    // Maps the provider error classification a round's diagnostics report
    // (e.g. "storageSetupRequired") into the error-kind vocabulary the
    // connection contract matches on (e.g. "storage") so the next planner
    // run classifies the connection truthfully within this session.
    public static string? MapProviderErrorClassification(string? providerErrorClassification)
    {
        return providerErrorClassification switch
        {
            null or "" or "ready" => null,
            "authExpired" => "auth",
            "quotaExceeded" => "quota",
            "adapterUnsupported" => "adapterUnsupported",
            "recoveryRequired" => "recoveryRequired",
            "providerUnavailable" => "providerUnavailable",
            "storageSetupRequired" => "storage",
            // Rereads are handled inside the rounds themselves; they are
            // not a connection-level condition.
            "cloudStateRereadRequired" => null,
            // Unknown tokens pass through; the connection contract ignores
            // kinds it does not match.
            _ => providerErrorClassification,
        };
    }

    // Membership- or storage-scoped provider errors mean the shared cloud
    // store no longer knows this device (folder wiped, store reset by
    // another device): clearing the persisted membership marker makes the
    // next run re-join. A wrongly cleared marker costs exactly one
    // corrective membership round, so this stays deliberately narrow.
    public static bool ShouldClearMembershipMarker(string? providerErrorClassification)
    {
        return providerErrorClassification switch
        {
            "storageSetupRequired" or "recoveryRequired" => true,
            "membershipRequired" or "deviceMembershipMissing" or "deviceMembershipRevoked" => true,
            _ => false,
        };
    }

    private static uint? TryReadUInt(JsonNode? node)
    {
        if (node is not JsonValue value)
        {
            return null;
        }

        return value.TryGetValue<uint>(out var parsed) ? parsed : null;
    }

    private static JsonNode? JsonNodeFromObjectJson(string json)
    {
        if (string.Equals(json, "null", StringComparison.Ordinal))
        {
            return null;
        }

        try
        {
            return JsonNode.Parse(json);
        }
        catch (JsonException)
        {
            return null;
        }
    }
}
