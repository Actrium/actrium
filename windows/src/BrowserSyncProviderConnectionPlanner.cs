using System.Text.Json;
using System.Text.Json.Nodes;

namespace Actrium.Windows;

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

// Marshals the persisted provider-settings snapshot into the Rust cloud
// provider connection contract and maps the Rust-returned routing token back
// into the coordinator's routing. The connection vocabulary now lives entirely
// in the Rust contract: the localFolder connection constants, the round
// classification -> error-kind map, the membership/cycle/blocked routing, and
// the blocked-case scheduler field shape are all emitted by the contract, so
// this planner only handles the native pre-gate (IsConfigured) and coarse
// marshalling. Pure by design: no IO — the accept-time folder preflight and
// each round's own capability/compatibility preflights are the authoritative
// folder checks.
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

    // Coarse marshalling only: the Rust contract fills the localFolder
    // connection constants from providerKind and derives membership routing
    // from the marker flag, so the native side no longer hard-codes the
    // "configured folder is connected" snapshot.
    public static JsonObject BuildProviderSnapshot(
        BrowserSyncProviderSettingsSnapshot settings,
        string? lastRoundClassification)
    {
        var snapshot = new JsonObject
        {
            ["providerKind"] = settings.ProviderKind,
            ["scope"] = settings.Scope,
            ["deviceId"] = settings.DeviceId,
            // The persisted membership marker is the only cold-start
            // membership signal: a membership round writes the shared cloud
            // folder, so repeating it on every launch is wrong. The Rust
            // contract turns this flag into the membership-vs-cycle routing.
            ["membershipMarkerPresent"] = settings.MembershipJoinedAtUnixSeconds is not null,
        };

        if (!string.IsNullOrEmpty(lastRoundClassification))
        {
            // Raw last-round classification: the Rust contract owns the map
            // into the connection error-kind vocabulary and the resulting
            // classification for this run.
            snapshot["providerRoundClassification"] = lastRoundClassification;
        }

        return snapshot;
    }

    public static BrowserSyncProviderConnectionPlan Plan(
        BrowserSyncProviderSettingsSnapshot? settings,
        string? lastRoundClassification,
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

        var providerSnapshot = BuildProviderSnapshot(settings!, lastRoundClassification);
        // The Rust core owns the connection classification, routing token, and
        // blocked scheduler state. A result that is not a JSON object is
        // contract drift and must fail loudly instead of degrading to a
        // hand-copied C# policy.
        var connectionPolicy = JsonNodeFromObjectJson(
            ActriumCore.BuildBrowserStorageSyncCloudProviderConnectionStatusPolicyJson(
                providerSnapshot.ToJsonString(JsonOptions),
                retryAttempt,
                observedAtUnixSeconds)) as JsonObject
            ?? throw new InvalidOperationException(
                "Browser sync cloud provider connection status policy from the Rust core is not a JSON object.");

        var routing = MapRoutingToken(connectionPolicy["routingToken"]?.GetValue<string>());
        return new BrowserSyncProviderConnectionPlan(routing, providerSnapshot, connectionPolicy);
    }

    // notConfigured is the native pre-gate above (IsConfigured), so the Rust
    // contract only emits membership/cycle/blocked. Unknown tokens fail safe
    // to Blocked: no round runs unless the contract explicitly says to.
    private static BrowserSyncProviderRouting MapRoutingToken(string? routingToken)
    {
        return routingToken switch
        {
            "membership" => BrowserSyncProviderRouting.RunLocalFolderMembership,
            "cycle" => BrowserSyncProviderRouting.RunLocalFolderCycle,
            _ => BrowserSyncProviderRouting.Blocked,
        };
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
