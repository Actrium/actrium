using System.Diagnostics;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace Actro.Windows;

internal sealed record BrowserStorageSyncCredentials(
    string BaseUrl,
    string AccessToken,
    string Scope)
{
    // No device id here: the reference-server round sources the durable
    // device identity from the provider settings store, so the endpoint
    // credential no longer carries one.
    public bool IsUsable =>
        !string.IsNullOrWhiteSpace(BaseUrl)
        && !string.IsNullOrWhiteSpace(AccessToken)
        && !string.IsNullOrWhiteSpace(Scope);
}

internal interface IBrowserStorageSyncCredentialsProvider
{
    Task<BrowserStorageSyncCredentials?> GetCredentialsAsync(
        CancellationToken cancellationToken = default);
}

internal sealed class BrowserStorageSyncCoordinator
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);

    // Once per process, not per coordinator: the provider-active line marks
    // which sync path this process runs on, and repeating it every cycle
    // would drown the interesting per-cycle logs.
    private static int providerActiveLogged;

    private readonly BrowserStorageSyncClient client;
    private readonly IBrowserStorageSyncCredentialsProvider credentialsProvider;
    private readonly BrowserSyncProviderSettingsStore? providerSettingsStore;

    // Session-scoped provider error state: at cold start no round has run,
    // so the planner snapshot carries no round classification; within a
    // session it holds the last round's raw provider error classification,
    // which the Rust connection contract folds into the error-kind vocabulary.
    private string? lastProviderRoundClassification;

    // Session-scoped summary of the last executed provider round envelope
    // (endpoint type, classification, Rust provider diagnostics, and the
    // provider error classification plan), kept for the settings dialog's
    // diagnostics detail. In-memory only by design: diagnostics describe
    // this session's rounds, and persisting them would turn a display
    // detail into state to migrate.
    private string? lastProviderRoundDiagnosticsJson;

    public string? LastProviderRoundDiagnosticsJson => lastProviderRoundDiagnosticsJson;

    public BrowserStorageSyncCoordinator(
        BrowserStorageSyncClient client,
        IBrowserStorageSyncCredentialsProvider credentialsProvider,
        BrowserSyncProviderSettingsStore? providerSettingsStore = null)
    {
        this.client = client;
        this.credentialsProvider = credentialsProvider;
        this.providerSettingsStore = providerSettingsStore;
    }

    public static BrowserStorageSyncCoordinator CreateDefault(
        IBrowserStorageSyncCredentialsProvider? credentialsProvider = null,
        BrowserSyncProviderSettingsStore? providerSettingsStore = null)
    {
        return new BrowserStorageSyncCoordinator(
            new BrowserStorageSyncClient(BrowserStorageDatabase.CreateDefault()),
            credentialsProvider ?? BrowserStorageSyncCredentialStore.CreateDefault(),
            providerSettingsStore ?? BrowserSyncProviderSettingsStore.CreateDefault());
    }

    public async Task<string> RunSyncCycleAsync(
        long? generatedAtUnixSeconds = null,
        int limitPerCollection = 200,
        uint retryAttempt = 0,
        long? observedAtUnixSeconds = null,
        CancellationToken cancellationToken = default)
    {
        var observedAt = observedAtUnixSeconds ?? DateTimeOffset.UtcNow.ToUnixTimeSeconds();

        // Provider branch first: a configured cloud provider connection owns
        // browser sync; only an unconfigured provider falls through to the
        // endpoint-token reference-server branch below.
        if (providerSettingsStore is not null)
        {
            var providerResultJson = await RunProviderCycleAsync(
                generatedAtUnixSeconds,
                limitPerCollection,
                retryAttempt,
                observedAt,
                cancellationToken).ConfigureAwait(false);
            if (providerResultJson is not null)
            {
                return providerResultJson;
            }
        }

        var credentials = await credentialsProvider.GetCredentialsAsync(cancellationToken).ConfigureAwait(false);
        if (credentials?.IsUsable != true)
        {
            return SyncCoordinatorResultJson(
                credentialsAvailable: false,
                syncResult: null,
                classification: "credentialsUnavailable",
                shouldRetry: false,
                clientAction: "refreshCredentials",
                retryAttempt,
                observedAt);
        }

        // Loud on purpose, every executed cycle: leftover endpoint
        // credentials from the transitional dialog would otherwise
        // reactivate reference-server sync silently.
        Trace.TraceInformation("browser sync cycle via reference server (development)");

        var syncResultJson = await client.RunSyncCycleAsync(
            credentials.BaseUrl,
            credentials.AccessToken,
            credentials.Scope,
            // Durable device identity lives in the provider settings store,
            // not the endpoint credential. In production the store is always
            // present (an unconfigured provider still holds a minted id); the
            // fallback covers the dev/test coordinator built without one.
            providerSettingsStore?.Load().DeviceId ?? string.Empty,
            generatedAtUnixSeconds,
            limitPerCollection,
            cancellationToken).ConfigureAwait(false);
        var syncResult = JsonNodeFromObjectJson(syncResultJson) as JsonObject;

        return SyncCoordinatorResultJson(
            credentialsAvailable: true,
            syncResult,
            syncResult?["classification"]?.GetValue<string>() ?? string.Empty,
            syncResult?["shouldRetry"]?.GetValue<bool>() ?? false,
            syncResult?["clientAction"]?.GetValue<string>() ?? string.Empty,
            retryAttempt,
            observedAt);
    }

    // Returns null only for the NotConfigured routing so the caller falls
    // through to the endpoint branch; every other routing produces a final
    // coordinator result.
    private async Task<string?> RunProviderCycleAsync(
        long? generatedAtUnixSeconds,
        int limitPerCollection,
        uint retryAttempt,
        long observedAtUnixSeconds,
        CancellationToken cancellationToken)
    {
        var settings = providerSettingsStore!.Load();
        var plan = BrowserSyncProviderConnectionPlanner.Plan(
            settings,
            lastProviderRoundClassification,
            retryAttempt,
            observedAtUnixSeconds);
        if (plan.Routing == BrowserSyncProviderRouting.NotConfigured)
        {
            return null;
        }

        if (Interlocked.Exchange(ref providerActiveLogged, 1) == 0)
        {
            Trace.TraceInformation(
                "browser sync using {0} provider; reference server path inactive",
                settings.ProviderKind);
        }

        if (plan.Routing == BrowserSyncProviderRouting.Blocked)
        {
            return ProviderBlockedResultJson(settings, plan);
        }

        if (plan.Routing == BrowserSyncProviderRouting.RunLocalFolderMembership)
        {
            var membershipResultJson = await client.RunLocalFolderMembershipRoundAsync(
                settings.LocalFolderRootPath!,
                BuildMembershipJoinRequestJson(settings, generatedAtUnixSeconds ?? observedAtUnixSeconds),
                observedAtUnixSeconds,
                cancellationToken).ConfigureAwait(false);
            var membershipEnvelope = JsonNodeFromObjectJson(membershipResultJson) as JsonObject;
            ApplyProviderRoundOutcome(membershipEnvelope);

            if (membershipEnvelope?["completed"]?.GetValue<bool>() != true)
            {
                return ProviderRoundResultJson(membershipEnvelope, "cloudLocalFolderMembership");
            }

            // Persist the membership marker so later app launches go
            // straight to the cycle round instead of re-writing the shared
            // membership document on every start.
            providerSettingsStore.SaveMembershipJoined(observedAtUnixSeconds);
            settings = providerSettingsStore.Load();
            plan = BrowserSyncProviderConnectionPlanner.Plan(
                settings,
                lastProviderRoundClassification,
                retryAttempt,
                observedAtUnixSeconds);
            if (plan.Routing != BrowserSyncProviderRouting.RunLocalFolderCycle)
            {
                // Re-evaluation after a successful join is expected to say
                // "run the cycle"; anything else (fresh provider error,
                // policy change) is reported honestly instead of forcing a
                // cycle the policy no longer allows.
                return plan.Routing == BrowserSyncProviderRouting.Blocked
                    ? ProviderBlockedResultJson(settings, plan)
                    : ProviderRoundResultJson(membershipEnvelope, "cloudLocalFolderMembership");
            }
        }

        var cycleResultJson = await client.RunLocalFolderSyncCycleAsync(
            settings.LocalFolderRootPath!,
            settings.Scope,
            settings.DeviceId,
            generatedAtUnixSeconds,
            limitPerCollection,
            retryAttempt,
            observedAtUnixSeconds,
            cancellationToken: cancellationToken).ConfigureAwait(false);
        var cycleEnvelope = JsonNodeFromObjectJson(cycleResultJson) as JsonObject;
        ApplyProviderRoundOutcome(cycleEnvelope);
        return ProviderRoundResultJson(cycleEnvelope, "cloudLocalFolderCycle");
    }

    // Reads the round's raw provider error classification into the session
    // state (the next planner run hands it to the Rust connection contract to
    // fold into the error-kind vocabulary), keeps the round's diagnostics
    // summary for the settings dialog, and clears the persisted membership
    // marker when the Rust round diagnostics say the shared store no longer
    // knows this device so the next run re-joins.
    private void ApplyProviderRoundOutcome(JsonObject? roundEnvelope)
    {
        lastProviderRoundDiagnosticsJson = BuildProviderRoundDiagnosticsSummaryJson(roundEnvelope);
        var providerDiagnostics = roundEnvelope?["providerDiagnostics"];
        lastProviderRoundClassification =
            providerDiagnostics?["providerErrorClassification"]?.GetValue<string>();
        if (providerDiagnostics?["shouldClearMembershipMarker"]?.GetValue<bool>() == true)
        {
            providerSettingsStore!.ClearMembershipMarker();
            Trace.TraceWarning(
                "browser sync provider round reported a membership/storage-scoped error ({0}); cleared the membership marker so the next run re-joins",
                lastProviderRoundClassification);
        }
    }

    // Distills the round envelope into the fields the diagnostics formatter
    // reads. An unparseable envelope clears the summary: a stale diagnosis
    // from an earlier round would misattribute whatever just went wrong.
    private static string? BuildProviderRoundDiagnosticsSummaryJson(JsonObject? roundEnvelope)
    {
        if (roundEnvelope is null)
        {
            return null;
        }

        var summary = new JsonObject
        {
            ["endpointType"] = roundEnvelope["endpointType"]?.GetValue<string>() ?? string.Empty,
            ["classification"] = roundEnvelope["classification"]?.GetValue<string>() ?? string.Empty,
            ["providerErrorClassification"] = CloneJsonNode(roundEnvelope["providerErrorClassification"]),
            ["providerDiagnostics"] = CloneJsonNode(roundEnvelope["providerDiagnostics"]),
        };

        return summary.ToJsonString(JsonOptions);
    }

    // Round routings: the round envelope already carries the cloud
    // scheduler-shaped scheduleState and statusPolicy; lift the last
    // executed round's fields to the top level the scheduler reads.
    private static string ProviderRoundResultJson(
        JsonObject? roundEnvelope,
        string fallbackEndpointType)
    {
        var payload = new JsonObject
        {
            ["version"] = 1,
            ["storageSchemaVersion"] = BrowserStorageDatabase.SchemaVersion,
            ["endpointType"] = roundEnvelope?["endpointType"]?.GetValue<string>() ?? fallbackEndpointType,
            // Local folder never waits for credentials: there are none.
            ["credentialsAvailable"] = true,
            ["providerKind"] = roundEnvelope?["providerKind"]?.GetValue<string>()
                ?? BrowserSyncProviderSettingsStore.LocalFolderProviderKind,
            ["syncResult"] = roundEnvelope,
            ["classification"] = roundEnvelope?["classification"]?.GetValue<string>() ?? string.Empty,
            ["shouldRetry"] = roundEnvelope?["shouldRetry"]?.GetValue<bool>() ?? false,
            ["clientAction"] = roundEnvelope?["clientAction"]?.GetValue<string>() ?? string.Empty,
            ["statusPolicy"] = CloneJsonNode(roundEnvelope?["statusPolicy"]),
            ["scheduleState"] = CloneJsonNode(roundEnvelope?["scheduleState"]),
        };

        return payload.ToJsonString(JsonOptions);
    }

    // Blocked routing: no round runs and no round envelope exists, so the
    // scheduleState is the Rust connection contract's blocked-case translation
    // of the status policy into the scheduler's field shape, emitted alongside
    // the policy itself.
    private static string ProviderBlockedResultJson(
        BrowserSyncProviderSettingsSnapshot settings,
        BrowserSyncProviderConnectionPlan plan)
    {
        var connectionPolicy = plan.ConnectionPolicy!;
        var payload = new JsonObject
        {
            ["version"] = 1,
            ["storageSchemaVersion"] = BrowserStorageDatabase.SchemaVersion,
            ["endpointType"] = "cloudProviderConnection",
            ["credentialsAvailable"] = true,
            ["providerKind"] = settings.ProviderKind,
            ["syncResult"] = null,
            ["classification"] = connectionPolicy["classification"]?.GetValue<string>() ?? string.Empty,
            ["shouldRetry"] = connectionPolicy["shouldRetry"]?.GetValue<bool>() ?? false,
            ["clientAction"] = connectionPolicy["providerAction"]?.GetValue<string>() ?? string.Empty,
            ["statusPolicy"] = CloneJsonNode(connectionPolicy),
            ["scheduleState"] = CloneJsonNode(connectionPolicy["blockedScheduleState"]),
        };

        return payload.ToJsonString(JsonOptions);
    }

    private static string BuildMembershipJoinRequestJson(
        BrowserSyncProviderSettingsSnapshot settings,
        long generatedAtUnixSeconds)
    {
        var request = new JsonObject
        {
            ["scope"] = settings.Scope,
            ["actorDeviceId"] = settings.DeviceId,
            ["memberDeviceId"] = settings.DeviceId,
            ["displayName"] = Environment.MachineName,
            ["platform"] = "Windows",
            ["membershipAction"] = "connect",
            ["generatedAtUnixSeconds"] = generatedAtUnixSeconds,
        };

        return request.ToJsonString(JsonOptions);
    }

    private static string SyncCoordinatorResultJson(
        bool credentialsAvailable,
        JsonObject? syncResult,
        string classification,
        bool shouldRetry,
        string clientAction,
        uint retryAttempt,
        long observedAtUnixSeconds)
    {
        var statusPolicy = BuildStatusPolicy(
            classification,
            shouldRetry,
            clientAction,
            retryAttempt);
        var scheduleState = BuildScheduleState(statusPolicy, observedAtUnixSeconds);
        var payload = new JsonObject
        {
            ["version"] = 1,
            ["storageSchemaVersion"] = BrowserStorageDatabase.SchemaVersion,
            ["endpointType"] = "syncCycle",
            ["credentialsAvailable"] = credentialsAvailable,
            ["syncResult"] = syncResult,
            ["classification"] = classification,
            ["shouldRetry"] = shouldRetry,
            ["clientAction"] = clientAction,
            ["statusPolicy"] = statusPolicy,
            ["scheduleState"] = scheduleState,
        };

        return payload.ToJsonString(JsonOptions);
    }

    // The Rust core owns the sync status policy and schedule state rules.
    // A result that is not a JSON object is contract drift and must fail
    // loudly instead of degrading to a hand-copied C# policy.
    private static JsonObject BuildStatusPolicy(
        string classification,
        bool shouldRetry,
        string clientAction,
        uint retryAttempt)
    {
        return JsonNodeFromObjectJson(ActroCore.BuildBrowserStorageSyncStatusPolicyJson(
            classification,
            shouldRetry,
            clientAction,
            retryAttempt)) as JsonObject
            ?? throw new InvalidOperationException(
                "Browser sync status policy from the Rust core is not a JSON object.");
    }

    private static JsonObject BuildScheduleState(
        JsonObject statusPolicy,
        long observedAtUnixSeconds)
    {
        return JsonNodeFromObjectJson(ActroCore.BuildBrowserStorageSyncScheduleStateJson(
            statusPolicy.ToJsonString(JsonOptions),
            observedAtUnixSeconds)) as JsonObject
            ?? throw new InvalidOperationException(
                "Browser sync schedule state from the Rust core is not a JSON object.");
    }

    private static JsonNode? CloneJsonNode(JsonNode? node)
    {
        // JsonNode instances are single-parent: lifting a field out of a
        // round envelope that also stays embedded requires a copy.
        return node is null ? null : JsonNode.Parse(node.ToJsonString(JsonOptions));
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
