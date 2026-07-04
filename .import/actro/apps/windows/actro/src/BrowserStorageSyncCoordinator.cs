using System.Text.Json;
using System.Text.Json.Nodes;

namespace Actro.Windows;

internal sealed record BrowserStorageSyncCredentials(
    string BaseUrl,
    string AccessToken,
    string Scope,
    string DeviceId)
{
    public bool IsUsable =>
        !string.IsNullOrWhiteSpace(BaseUrl)
        && !string.IsNullOrWhiteSpace(AccessToken)
        && !string.IsNullOrWhiteSpace(Scope)
        && !string.IsNullOrWhiteSpace(DeviceId);
}

internal interface IBrowserStorageSyncCredentialsProvider
{
    Task<BrowserStorageSyncCredentials?> GetCredentialsAsync(
        CancellationToken cancellationToken = default);
}

internal sealed class BrowserStorageSyncCoordinator
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);

    private readonly BrowserStorageSyncClient client;
    private readonly IBrowserStorageSyncCredentialsProvider credentialsProvider;

    public BrowserStorageSyncCoordinator(
        BrowserStorageSyncClient client,
        IBrowserStorageSyncCredentialsProvider credentialsProvider)
    {
        this.client = client;
        this.credentialsProvider = credentialsProvider;
    }

    public static BrowserStorageSyncCoordinator CreateDefault(
        IBrowserStorageSyncCredentialsProvider? credentialsProvider = null)
    {
        return new BrowserStorageSyncCoordinator(
            new BrowserStorageSyncClient(BrowserStorageDatabase.CreateDefault()),
            credentialsProvider ?? BrowserStorageSyncCredentialStore.CreateDefault());
    }

    public async Task<string> RunSyncCycleAsync(
        long? generatedAtUnixSeconds = null,
        int limitPerCollection = 200,
        uint retryAttempt = 0,
        long? observedAtUnixSeconds = null,
        CancellationToken cancellationToken = default)
    {
        var observedAt = observedAtUnixSeconds ?? DateTimeOffset.UtcNow.ToUnixTimeSeconds();
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

        var syncResultJson = await client.RunSyncCycleAsync(
            credentials.BaseUrl,
            credentials.AccessToken,
            credentials.Scope,
            credentials.DeviceId,
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
