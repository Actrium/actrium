using System.Net.Http;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace Actrium.Windows;

internal sealed record BrowserStorageSyncHttpRequest(
    string Method,
    Uri Url,
    IReadOnlyDictionary<string, string> Headers,
    string BodyJson);

internal sealed record BrowserStorageSyncHttpResponse(
    int StatusCode,
    string BodyJson,
    string? ErrorMessage = null);

internal interface IBrowserStorageSyncTransport
{
    Task<BrowserStorageSyncHttpResponse> SendAsync(
        BrowserStorageSyncHttpRequest request,
        CancellationToken cancellationToken = default);
}

internal sealed class HttpClientBrowserStorageSyncTransport : IBrowserStorageSyncTransport, IDisposable
{
    private readonly HttpClient httpClient;
    private readonly bool ownsHttpClient;

    public HttpClientBrowserStorageSyncTransport()
        : this(new HttpClient(), ownsHttpClient: true)
    {
    }

    public HttpClientBrowserStorageSyncTransport(HttpClient httpClient)
        : this(httpClient, ownsHttpClient: false)
    {
    }

    private HttpClientBrowserStorageSyncTransport(HttpClient httpClient, bool ownsHttpClient)
    {
        this.httpClient = httpClient;
        this.ownsHttpClient = ownsHttpClient;
    }

    public async Task<BrowserStorageSyncHttpResponse> SendAsync(
        BrowserStorageSyncHttpRequest request,
        CancellationToken cancellationToken = default)
    {
        try
        {
            using var requestMessage = new HttpRequestMessage(new HttpMethod(request.Method), request.Url)
            {
                Content = new StringContent(request.BodyJson, Encoding.UTF8, "application/json"),
            };

            foreach (var header in request.Headers)
            {
                if (string.Equals(header.Key, "Content-Type", StringComparison.OrdinalIgnoreCase))
                {
                    continue;
                }

                requestMessage.Headers.TryAddWithoutValidation(header.Key, header.Value);
            }

            using var response = await httpClient.SendAsync(requestMessage, cancellationToken).ConfigureAwait(false);
            return new BrowserStorageSyncHttpResponse(
                (int)response.StatusCode,
                await response.Content.ReadAsStringAsync(cancellationToken).ConfigureAwait(false));
        }
        catch (Exception error) when (
            error is HttpRequestException
            || error is TaskCanceledException
            || error is InvalidOperationException
            || error is IOException)
        {
            return new BrowserStorageSyncHttpResponse(0, string.Empty, error.Message);
        }
    }

    public void Dispose()
    {
        if (ownsHttpClient)
        {
            httpClient.Dispose();
        }
    }
}

internal sealed class BrowserStorageSyncClient
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);

    private readonly BrowserStorageDatabase database;
    private readonly IBrowserStorageSyncTransport transport;

    public BrowserStorageSyncClient(
        BrowserStorageDatabase database,
        IBrowserStorageSyncTransport? transport = null)
    {
        this.database = database;
        this.transport = transport ?? new HttpClientBrowserStorageSyncTransport();
    }

    public async Task<string> RunSyncCycleAsync(
        string baseUrl,
        string accessToken,
        string scope,
        string deviceId,
        long? generatedAtUnixSeconds = null,
        int limitPerCollection = 200,
        CancellationToken cancellationToken = default)
    {
        var requestPlanJson = database.ExportSyncCycleRequestPlan(
            baseUrl,
            accessToken,
            scope,
            deviceId,
            generatedAtUnixSeconds ?? DateTimeOffset.UtcNow.ToUnixTimeSeconds(),
            limitPerCollection);
        var requestPlan = BrowserStorageSyncCycleRequestPlan.Parse(requestPlanJson);
        if (requestPlan is null || !requestPlan.TryBuildHttpRequest(out var request))
        {
            return SyncClientResultJson(
                JsonNodeFromObjectJson(requestPlanJson),
                new JsonObject
                {
                    ["classification"] = "clientPlanUnavailable",
                    ["shouldRetry"] = false,
                    ["clientAction"] = "skipCycle",
                },
                null,
                applied: false);
        }

        var response = await transport.SendAsync(request, cancellationToken).ConfigureAwait(false);
        var transportResultJson = ActriumCore.BuildBrowserStorageSyncCycleTransportResultJson(
            response.StatusCode,
            response.BodyJson,
            response.ErrorMessage ?? string.Empty);
        var transportResult = JsonNodeFromObjectJson(transportResultJson) as JsonObject ?? [];
        JsonNode? applyResult = null;
        var applied = false;

        if (string.Equals(
                transportResult["classification"]?.GetValue<string>(),
                "success",
                StringComparison.Ordinal))
        {
            var responseJson = transportResult["response"]?.ToJsonString(JsonOptions) ?? response.BodyJson;
            var applyResultJson = database.ApplySyncCycleResponse(responseJson);
            applyResult = JsonNodeFromObjectJson(applyResultJson);
            applied = applyResult is JsonObject;
        }

        return SyncClientResultJson(
            JsonNodeFromObjectJson(requestPlanJson),
            transportResult,
            applyResult,
            applied);
    }

    public Task<string> RunLocalFolderSyncCycleAsync(
        string rootDir,
        string scope,
        string deviceId,
        long? generatedAtUnixSeconds = null,
        int limitPerCollection = 200,
        uint retryAttempt = 0,
        long? observedAtUnixSeconds = null,
        uint maxAttempts = 3,
        CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        var generatedAt = generatedAtUnixSeconds ?? DateTimeOffset.UtcNow.ToUnixTimeSeconds();
        var observedAt = observedAtUnixSeconds ?? generatedAt;
        var capabilityPreflight = BuildLocalFolderCapabilityPreflight(observedAt);
        if (capabilityPreflight?["shouldBlockSync"]?.GetValue<bool>() == true)
        {
            var blockedProviderDiagnostics = BuildCloudProviderDiagnostics(capabilityPreflight, null, null, null, observedAt);
            var blockedObservabilityEvent = BuildCloudObservabilityEvent(
                blockedProviderDiagnostics,
                null,
                null,
                observedAt);
            return Task.FromResult(LocalFolderSyncClientResultJson(
                rootDir: rootDir,
                pushRequest: null,
                capabilityPreflight: capabilityPreflight,
                compatibilityPreflight: null,
                schemaRolloutPlan: null,
                schemaMigrationPlaybook: null,
                recoverySnapshotRound: null,
                cycleRound: null,
                statusPolicy: null,
                scheduleState: null,
                attemptPlan: null,
                loopPlan: null,
                applyResult: null,
                compactionRound: null,
                cleanupRound: null,
                providerErrorClassification: null,
                providerDiagnostics: blockedProviderDiagnostics,
                observabilityEvent: blockedObservabilityEvent,
                classification: capabilityPreflight["classification"]?.GetValue<string>() ?? string.Empty,
                shouldRetry: false,
                clientAction: capabilityPreflight["clientAction"]?.GetValue<string>() ?? string.Empty,
                applied: false));
        }
        var compatibilityPreflight = BuildLocalFolderCompatibilityPreflight(rootDir, observedAt);
        if (compatibilityPreflight?["shouldBlockSync"]?.GetValue<bool>() == true)
        {
            var schemaRolloutPlan = BuildCloudSchemaRolloutPlan(compatibilityPreflight, observedAt);
            var schemaMigrationPlaybook = BuildCloudSchemaMigrationPlaybook(schemaRolloutPlan, observedAt);
            var recoverySnapshotRound = BuildLocalFolderRecoverySnapshotRound(
                rootDir,
                schemaMigrationPlaybook,
                compatibilityPreflight,
                observedAt);
            var blockedProviderDiagnostics = BuildCloudProviderDiagnostics(
                capabilityPreflight,
                compatibilityPreflight,
                null,
                null,
                observedAt);
            var blockedObservabilityEvent = BuildCloudObservabilityEvent(
                blockedProviderDiagnostics,
                null,
                null,
                observedAt);
            return Task.FromResult(LocalFolderSyncClientResultJson(
                rootDir: rootDir,
                pushRequest: null,
                capabilityPreflight: capabilityPreflight,
                compatibilityPreflight: compatibilityPreflight,
                schemaRolloutPlan: schemaRolloutPlan,
                schemaMigrationPlaybook: schemaMigrationPlaybook,
                recoverySnapshotRound: recoverySnapshotRound,
                cycleRound: null,
                statusPolicy: null,
                scheduleState: null,
                attemptPlan: null,
                loopPlan: null,
                applyResult: null,
                compactionRound: null,
                cleanupRound: null,
                providerErrorClassification: null,
                providerDiagnostics: blockedProviderDiagnostics,
                observabilityEvent: blockedObservabilityEvent,
                classification: compatibilityPreflight["classification"]?.GetValue<string>() ?? string.Empty,
                shouldRetry: false,
                clientAction: compatibilityPreflight["clientAction"]?.GetValue<string>() ?? string.Empty,
                applied: false));
        }
        var pushRequestJson = database.ExportDirtySyncPushRequest(
            scope,
            deviceId,
            generatedAt,
            limitPerCollection);
        if (string.Equals(pushRequestJson, "null", StringComparison.Ordinal))
        {
            var unavailablePushProviderDiagnostics = BuildCloudProviderDiagnostics(
                capabilityPreflight,
                compatibilityPreflight,
                null,
                null,
                observedAt);
            var unavailablePushObservabilityEvent = BuildCloudObservabilityEvent(
                unavailablePushProviderDiagnostics,
                null,
                null,
                observedAt);
            return Task.FromResult(LocalFolderSyncClientResultJson(
                rootDir,
                JsonNodeFromObjectJson(pushRequestJson),
                capabilityPreflight,
                compatibilityPreflight,
                null,
                null,
                null,
                null,
                null,
                null,
                null,
                null,
                null,
                null,
                null,
                null,
                unavailablePushProviderDiagnostics,
                unavailablePushObservabilityEvent,
                "clientPlanUnavailable",
                false,
                "skipCloudCycle",
                false));
        }

        string cycleRoundJson;
        cycleRoundJson = ActriumCore.BuildBrowserStorageSyncCloudLocalFolderCycleRoundJson(
            rootDir,
            pushRequestJson,
            database.SyncCursorsJson(scope),
            deviceId,
            generatedAt,
            (uint)Math.Clamp(limitPerCollection, 1, 1000));

        var cycleRound = JsonNodeFromObjectJson(cycleRoundJson) as JsonObject;
        if (cycleRound is null)
        {
            var unavailableCycleProviderDiagnostics = BuildCloudProviderDiagnostics(
                capabilityPreflight,
                compatibilityPreflight,
                null,
                null,
                observedAt);
            var unavailableCycleObservabilityEvent = BuildCloudObservabilityEvent(
                unavailableCycleProviderDiagnostics,
                null,
                null,
                observedAt);
            return Task.FromResult(LocalFolderSyncClientResultJson(
                rootDir,
                JsonNodeFromObjectJson(pushRequestJson),
                capabilityPreflight,
                compatibilityPreflight,
                null,
                null,
                null,
                null,
                null,
                null,
                null,
                null,
                null,
                null,
                null,
                null,
                unavailableCycleProviderDiagnostics,
                unavailableCycleObservabilityEvent,
                "clientPlanUnavailable",
                false,
                "skipCloudCycle",
                false));
        }

        var statusPolicy = BuildCloudCycleStatusPolicy(cycleRoundJson, retryAttempt);
        var scheduleState = BuildCloudCycleScheduleState(statusPolicy, observedAt);
        var attemptPlan = BuildCloudCycleAttemptPlan(cycleRoundJson, retryAttempt, observedAt);
        var loopPlan = BuildCloudCycleLoopPlan(cycleRoundJson, retryAttempt, observedAt, maxAttempts);
        var applyResultJson = database.ApplyCloudCycleClientPlan(cycleRoundJson);
        var applyResult = JsonNodeFromObjectJson(applyResultJson) as JsonObject;
        var compactionRound = BuildLocalFolderCompactionRound(rootDir, deviceId, generatedAt);
        var cleanupRound = BuildLocalFolderCleanupRound(
            rootDir,
            database.SyncCursorsJson(scope),
            deviceId,
            generatedAt);
        var providerErrorClassification = BuildLocalFolderProviderErrorClassification(
            cycleRound: cycleRound,
            compactionRound: compactionRound,
            cleanupRound: cleanupRound,
            retryAttempt: retryAttempt,
            observedAtUnixSeconds: observedAt);
        var providerDiagnostics = BuildCloudProviderDiagnostics(
            capabilityPreflight,
            compatibilityPreflight,
            statusPolicy,
            scheduleState,
            observedAt,
            providerErrorClassification);
        var observabilityEvent = BuildCloudObservabilityEvent(
            providerDiagnostics,
            attemptPlan,
            loopPlan,
            observedAt);
        var classification = statusPolicy?["classification"]?.GetValue<string>() ?? string.Empty;
        var shouldRetry = statusPolicy?["shouldRetry"]?.GetValue<bool>() ?? false;
        var clientAction = statusPolicy?["clientAction"]?.GetValue<string>()
            ?? cycleRound["clientAction"]?.GetValue<string>()
            ?? string.Empty;

        return Task.FromResult(LocalFolderSyncClientResultJson(
            rootDir,
            JsonNodeFromObjectJson(pushRequestJson),
            capabilityPreflight,
            compatibilityPreflight,
            null,
            null,
            null,
            cycleRound,
            statusPolicy,
            scheduleState,
            attemptPlan,
            loopPlan,
            applyResult,
            compactionRound,
            cleanupRound,
            providerErrorClassification,
            providerDiagnostics,
            observabilityEvent,
            classification,
            shouldRetry,
            clientAction,
            applyResult?["applied"]?.GetValue<bool>() ?? false));
    }

    public Task<string> RunLocalFolderMembershipRoundAsync(
        string rootDir,
        string requestJson,
        long? observedAtUnixSeconds = null,
        CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        var safeRootDir = rootDir ?? string.Empty;
        var safeRequestJson = requestJson ?? string.Empty;
        var observedAt = observedAtUnixSeconds ?? DateTimeOffset.UtcNow.ToUnixTimeSeconds();
        var capabilityPreflight = BuildLocalFolderCapabilityPreflight(observedAt);
        if (capabilityPreflight?["shouldBlockSync"]?.GetValue<bool>() == true)
        {
            var blockedProviderDiagnostics = BuildCloudProviderDiagnostics(capabilityPreflight, null, null, null, observedAt);
            var blockedObservabilityEvent = BuildCloudObservabilityEvent(
                blockedProviderDiagnostics,
                null,
                null,
                observedAt);
            return Task.FromResult(LocalFolderMembershipClientResultJson(
                rootDir: safeRootDir,
                membershipRequest: JsonNodeFromObjectJson(safeRequestJson),
                capabilityPreflight: capabilityPreflight,
                compatibilityPreflight: null,
                schemaRolloutPlan: null,
                schemaMigrationPlaybook: null,
                recoverySnapshotRound: null,
                membershipRound: null,
                statusPolicy: null,
                scheduleState: null,
                attemptPlan: null,
                loopPlan: null,
                providerErrorClassification: null,
                providerDiagnostics: blockedProviderDiagnostics,
                observabilityEvent: blockedObservabilityEvent,
                classification: capabilityPreflight["classification"]?.GetValue<string>() ?? string.Empty,
                completed: false,
                shouldReread: false,
                shouldRetry: false,
                clientAction: capabilityPreflight["clientAction"]?.GetValue<string>() ?? string.Empty));
        }

        string membershipRoundJson;
        membershipRoundJson = ActriumCore.RunBrowserStorageSyncCloudLocalFolderMembershipRoundJson(
            safeRootDir,
            safeRequestJson);

        var membershipRound = JsonNodeFromObjectJson(membershipRoundJson) as JsonObject;
        if (membershipRound is null)
        {
            var unavailableProviderDiagnostics = BuildCloudProviderDiagnostics(capabilityPreflight, null, null, null, observedAt);
            var unavailableObservabilityEvent = BuildCloudObservabilityEvent(
                unavailableProviderDiagnostics,
                null,
                null,
                observedAt);
            return Task.FromResult(LocalFolderMembershipClientResultJson(
                safeRootDir,
                JsonNodeFromObjectJson(safeRequestJson),
                capabilityPreflight,
                null,
                null,
                null,
                null,
                null,
                null,
                null,
                null,
                null,
                null,
                unavailableProviderDiagnostics,
                unavailableObservabilityEvent,
                "clientPlanUnavailable",
                false,
                false,
                false,
                "skipCloudMembership"));
        }

        var completed = membershipRound["completed"]?.GetValue<bool>() ?? false;
        var shouldReread = membershipRound["shouldReread"]?.GetValue<bool>() ?? false;
        var shouldRetry = membershipRound["shouldRetry"]?.GetValue<bool>() ?? false;
        var planningErrorKind = membershipRound["planningErrorKind"]?.GetValue<string>() ?? string.Empty;
        var classification = completed
            ? "success"
            : !string.IsNullOrEmpty(planningErrorKind)
                ? planningErrorKind
                : "membershipRoundIncomplete";
        var statusPolicy = BuildCloudMembershipStatusPolicy(membershipRound, 0);
        var scheduleState = BuildCloudMembershipScheduleState(statusPolicy, observedAt);
        var attemptPlan = BuildCloudMembershipAttemptPlan(membershipRound, 0, observedAt);
        var loopPlan = BuildCloudMembershipLoopPlan(membershipRoundJson, 0, observedAt, 3);
        // Clone: the preflight node stays parented under membershipRound, and
        // the result payload inserts both nodes side by side.
        var compatibilityPreflight = membershipRound["compatibilityPreflight"] is JsonObject roundPreflight
            ? CloneJsonNode(roundPreflight) as JsonObject
            : null;
        JsonObject? schemaRolloutPlan = null;
        JsonObject? schemaMigrationPlaybook = null;
        JsonObject? recoverySnapshotRound = null;
        if (compatibilityPreflight?["shouldBlockSync"]?.GetValue<bool>() == true)
        {
            schemaRolloutPlan = BuildCloudSchemaRolloutPlan(compatibilityPreflight, observedAt);
            schemaMigrationPlaybook = BuildCloudSchemaMigrationPlaybook(schemaRolloutPlan, observedAt);
            recoverySnapshotRound = BuildLocalFolderRecoverySnapshotRound(
                safeRootDir,
                schemaMigrationPlaybook,
                compatibilityPreflight,
                observedAt);
        }
        var providerErrorClassification = BuildLocalFolderProviderErrorClassification(
            membershipRound: membershipRound,
            recoverySnapshotRound: recoverySnapshotRound,
            retryAttempt: 0,
            observedAtUnixSeconds: observedAt);
        var providerDiagnostics = BuildCloudProviderDiagnostics(
            capabilityPreflight,
            compatibilityPreflight,
            statusPolicy,
            scheduleState,
            observedAt,
            providerErrorClassification);
        var observabilityEvent = BuildCloudObservabilityEvent(
            providerDiagnostics,
            attemptPlan,
            loopPlan,
            observedAt);
        var statusClassification = statusPolicy?["classification"]?.GetValue<string>() ?? string.Empty;
        var statusClientAction = statusPolicy?["clientAction"]?.GetValue<string>() ?? string.Empty;

        return Task.FromResult(LocalFolderMembershipClientResultJson(
            safeRootDir,
            JsonNodeFromObjectJson(safeRequestJson),
            capabilityPreflight,
            compatibilityPreflight,
            schemaRolloutPlan,
            schemaMigrationPlaybook,
            recoverySnapshotRound,
            membershipRound,
            statusPolicy,
            scheduleState,
            attemptPlan,
            loopPlan,
            providerErrorClassification,
            providerDiagnostics,
            observabilityEvent,
            string.IsNullOrEmpty(statusClassification) ? classification : statusClassification,
            statusPolicy?["completed"]?.GetValue<bool>() ?? completed,
            statusPolicy?["shouldReread"]?.GetValue<bool>() ?? shouldReread,
            statusPolicy?["shouldRetry"]?.GetValue<bool>() ?? shouldRetry,
            string.IsNullOrEmpty(statusClientAction)
                ? membershipRound["clientAction"]?.GetValue<string>() ?? string.Empty
                : statusClientAction));
    }

    private static string SyncClientResultJson(
        JsonNode? requestPlan,
        JsonObject transportResult,
        JsonNode? applyResult,
        bool applied)
    {
        var payload = new JsonObject
        {
            ["version"] = 1,
            ["storageSchemaVersion"] = BrowserStorageDatabase.SchemaVersion,
            ["endpointType"] = "syncCycle",
            ["requestPlan"] = requestPlan,
            ["transportResult"] = transportResult,
            ["applyResult"] = applyResult,
            ["applied"] = applied,
            ["classification"] = transportResult["classification"]?.GetValue<string>() ?? string.Empty,
            ["shouldRetry"] = transportResult["shouldRetry"]?.GetValue<bool>() ?? false,
            ["clientAction"] = transportResult["clientAction"]?.GetValue<string>() ?? string.Empty,
        };

        return payload.ToJsonString(JsonOptions);
    }

    private static string LocalFolderSyncClientResultJson(
        string rootDir,
        JsonNode? pushRequest,
        JsonObject? capabilityPreflight,
        JsonObject? compatibilityPreflight,
        JsonObject? schemaRolloutPlan,
        JsonObject? schemaMigrationPlaybook,
        JsonObject? recoverySnapshotRound,
        JsonObject? cycleRound,
        JsonObject? statusPolicy,
        JsonObject? scheduleState,
        JsonObject? attemptPlan,
        JsonObject? loopPlan,
        JsonObject? applyResult,
        JsonObject? compactionRound,
        JsonObject? cleanupRound,
        JsonObject? providerErrorClassification,
        JsonObject? providerDiagnostics,
        JsonObject? observabilityEvent,
        string classification,
        bool shouldRetry,
        string clientAction,
        bool applied)
    {
        var payload = new JsonObject
        {
            ["version"] = 1,
            ["storageSchemaVersion"] = BrowserStorageDatabase.SchemaVersion,
            ["endpointType"] = "cloudLocalFolderCycle",
            ["providerKind"] = "localFolder",
            ["rootDir"] = rootDir,
            ["pushRequest"] = pushRequest,
            ["capabilityPreflight"] = capabilityPreflight,
            ["compatibilityPreflight"] = compatibilityPreflight,
            ["schemaRolloutPlan"] = schemaRolloutPlan,
            ["schemaMigrationPlaybook"] = schemaMigrationPlaybook,
            ["recoverySnapshotRound"] = recoverySnapshotRound,
            ["cycleRound"] = cycleRound,
            ["statusPolicy"] = statusPolicy,
            ["scheduleState"] = scheduleState,
            ["attemptPlan"] = attemptPlan,
            ["loopPlan"] = loopPlan,
            ["applyResult"] = applyResult,
            ["compactionRound"] = compactionRound,
            ["cleanupRound"] = cleanupRound,
            // Null on rounds whose failed-operation batch is empty; the
            // diagnostics surface renders the error detail (normalized
            // kinds, object paths, retry/reread intent) from this plan.
            ["providerErrorClassification"] = providerErrorClassification,
            ["providerDiagnostics"] = providerDiagnostics,
            ["observabilityEvent"] = observabilityEvent,
            ["applied"] = applied,
            ["classification"] = classification,
            ["shouldRetry"] = shouldRetry,
            ["clientAction"] = clientAction,
        };

        return payload.ToJsonString(JsonOptions);
    }

    private static string LocalFolderMembershipClientResultJson(
        string rootDir,
        JsonNode? membershipRequest,
        JsonObject? capabilityPreflight,
        JsonObject? compatibilityPreflight,
        JsonObject? schemaRolloutPlan,
        JsonObject? schemaMigrationPlaybook,
        JsonObject? recoverySnapshotRound,
        JsonObject? membershipRound,
        JsonObject? statusPolicy,
        JsonObject? scheduleState,
        JsonObject? attemptPlan,
        JsonObject? loopPlan,
        JsonObject? providerErrorClassification,
        JsonObject? providerDiagnostics,
        JsonObject? observabilityEvent,
        string classification,
        bool completed,
        bool shouldReread,
        bool shouldRetry,
        string clientAction)
    {
        var payload = new JsonObject
        {
            ["version"] = 1,
            ["storageSchemaVersion"] = BrowserStorageDatabase.SchemaVersion,
            ["endpointType"] = "cloudLocalFolderMembership",
            ["providerKind"] = "localFolder",
            ["rootDir"] = rootDir,
            ["membershipRequest"] = membershipRequest,
            ["capabilityPreflight"] = capabilityPreflight,
            ["compatibilityPreflight"] = compatibilityPreflight,
            ["schemaRolloutPlan"] = schemaRolloutPlan,
            ["schemaMigrationPlaybook"] = schemaMigrationPlaybook,
            ["recoverySnapshotRound"] = recoverySnapshotRound,
            ["membershipRound"] = membershipRound,
            ["statusPolicy"] = statusPolicy,
            ["scheduleState"] = scheduleState,
            ["attemptPlan"] = attemptPlan,
            ["loopPlan"] = loopPlan,
            // Null on rounds whose failed-operation batch is empty; the
            // diagnostics surface renders the error detail from this plan.
            ["providerErrorClassification"] = providerErrorClassification,
            ["providerDiagnostics"] = providerDiagnostics,
            ["observabilityEvent"] = observabilityEvent,
            ["completed"] = completed,
            ["shouldReread"] = shouldReread,
            ["shouldRetry"] = shouldRetry,
            ["classification"] = classification,
            ["clientAction"] = clientAction,
        };

        return payload.ToJsonString(JsonOptions);
    }

    private static JsonObject? BuildLocalFolderCapabilityPreflight(long observedAtUnixSeconds)
    {
        return JsonNodeFromObjectJson(
            ActriumCore.BuildBrowserStorageSyncCloudLocalFolderCapabilityPreflightJson(
                observedAtUnixSeconds)) as JsonObject;
    }

    private static JsonObject? BuildLocalFolderCompatibilityPreflight(string rootDir, long observedAtUnixSeconds)
    {
        return JsonNodeFromObjectJson(
            ActriumCore.BuildBrowserStorageSyncCloudLocalFolderCompatibilityPreflightJson(
                rootDir,
                observedAtUnixSeconds)) as JsonObject;
    }

    private static JsonObject? BuildLocalFolderCleanupRound(
        string rootDir,
        string cursorsJson,
        string deviceId,
        long generatedAtUnixSeconds)
    {
        return JsonNodeFromObjectJson(
            ActriumCore.RunBrowserStorageSyncCloudLocalFolderCleanupRoundJson(
                rootDir,
                cursorsJson,
                deviceId,
                generatedAtUnixSeconds)) as JsonObject;
    }

    private static JsonObject? BuildLocalFolderCompactionRound(
        string rootDir,
        string deviceId,
        long generatedAtUnixSeconds)
    {
        return JsonNodeFromObjectJson(
            ActriumCore.RunBrowserStorageSyncCloudLocalFolderCompactionRoundJson(
                rootDir,
                deviceId,
                generatedAtUnixSeconds)) as JsonObject;
    }

    private static JsonObject? BuildCloudSchemaRolloutPlan(
        JsonObject? compatibilityPreflight,
        long observedAtUnixSeconds)
    {
        return JsonNodeFromObjectJson(
            ActriumCore.BuildBrowserStorageSyncCloudSchemaRolloutPlanJson(
                compatibilityPreflight?.ToJsonString(JsonOptions) ?? string.Empty,
                observedAtUnixSeconds)) as JsonObject;
    }

    private static JsonObject? BuildCloudSchemaMigrationPlaybook(
        JsonObject? schemaRolloutPlan,
        long observedAtUnixSeconds)
    {
        return JsonNodeFromObjectJson(
            ActriumCore.BuildBrowserStorageSyncCloudSchemaMigrationPlaybookJson(
                schemaRolloutPlan?.ToJsonString(JsonOptions) ?? string.Empty,
                observedAtUnixSeconds)) as JsonObject;
    }

    private static JsonObject? BuildLocalFolderRecoverySnapshotRound(
        string rootDir,
        JsonObject? schemaMigrationPlaybook,
        JsonObject? compatibilityPreflight,
        long observedAtUnixSeconds)
    {
        return JsonNodeFromObjectJson(
            ActriumCore.BuildBrowserStorageSyncCloudLocalFolderRecoverySnapshotRoundJson(
                rootDir,
                schemaMigrationPlaybook?.ToJsonString(JsonOptions) ?? string.Empty,
                compatibilityPreflight?.ToJsonString(JsonOptions) ?? string.Empty,
                observedAtUnixSeconds)) as JsonObject;
    }

    private static JsonObject? BuildCloudCycleStatusPolicy(string cycleRoundJson, uint retryAttempt)
    {
        return JsonNodeFromObjectJson(
            ActriumCore.BuildBrowserStorageSyncCloudCycleStatusPolicyJson(
                cycleRoundJson,
                retryAttempt)) as JsonObject;
    }

    private static JsonObject? BuildCloudCycleScheduleState(JsonObject? statusPolicy, long observedAtUnixSeconds)
    {
        if (statusPolicy is null)
        {
            return null;
        }

        return JsonNodeFromObjectJson(
            ActriumCore.BuildBrowserStorageSyncCloudCycleScheduleStateJson(
                statusPolicy.ToJsonString(JsonOptions),
                observedAtUnixSeconds)) as JsonObject;
    }

    private static JsonObject? BuildCloudMembershipStatusPolicy(JsonObject? membershipRound, uint retryAttempt)
    {
        if (membershipRound is null)
        {
            return null;
        }

        return JsonNodeFromObjectJson(
            ActriumCore.BuildBrowserStorageSyncCloudMembershipStatusPolicyJson(
                membershipRound.ToJsonString(JsonOptions),
                retryAttempt)) as JsonObject;
    }

    private static JsonObject? BuildCloudMembershipScheduleState(JsonObject? statusPolicy, long observedAtUnixSeconds)
    {
        if (statusPolicy is null)
        {
            return null;
        }

        return JsonNodeFromObjectJson(
            ActriumCore.BuildBrowserStorageSyncCloudMembershipScheduleStateJson(
                statusPolicy.ToJsonString(JsonOptions),
                observedAtUnixSeconds)) as JsonObject;
    }

    private static JsonObject? BuildCloudMembershipAttemptPlan(JsonObject? membershipRound, uint retryAttempt, long observedAtUnixSeconds)
    {
        if (membershipRound is null)
        {
            return null;
        }

        return JsonNodeFromObjectJson(
            ActriumCore.BuildBrowserStorageSyncCloudMembershipAttemptPlanJson(
                membershipRound.ToJsonString(JsonOptions),
                retryAttempt,
                observedAtUnixSeconds)) as JsonObject;
    }

    private static JsonObject? BuildCloudMembershipLoopPlan(
        string membershipRoundJson,
        uint retryAttempt,
        long observedAtUnixSeconds,
        uint maxAttempts)
    {
        return JsonNodeFromObjectJson(
            ActriumCore.BuildBrowserStorageSyncCloudMembershipLoopPlanJson(
                $"[{membershipRoundJson}]",
                retryAttempt,
                observedAtUnixSeconds,
                maxAttempts)) as JsonObject;
    }

    private static JsonObject? BuildLocalFolderProviderErrorClassification(
        JsonObject? cycleRound = null,
        JsonObject? membershipRound = null,
        JsonObject? compactionRound = null,
        JsonObject? cleanupRound = null,
        JsonObject? recoverySnapshotRound = null,
        uint retryAttempt = 0,
        long observedAtUnixSeconds = 0)
    {
        var payload = new JsonObject
        {
            ["planType"] = "localFolderProviderRoundErrors",
            ["providerKind"] = "localFolder",
        };
        if (cycleRound is not null)
        {
            payload["cycleRound"] = CloneJsonNode(cycleRound);
        }
        if (membershipRound is not null)
        {
            payload["membershipRound"] = CloneJsonNode(membershipRound);
        }
        if (compactionRound is not null)
        {
            payload["compactionRound"] = CloneJsonNode(compactionRound);
        }
        if (cleanupRound is not null)
        {
            payload["cleanupRound"] = CloneJsonNode(cleanupRound);
        }
        if (recoverySnapshotRound is not null)
        {
            payload["recoverySnapshotRound"] = CloneJsonNode(recoverySnapshotRound);
        }

        var classification = JsonNodeFromObjectJson(
            ActriumCore.BuildBrowserStorageSyncCloudProviderErrorClassificationJson(
                payload.ToJsonString(JsonOptions),
                retryAttempt,
                observedAtUnixSeconds)) as JsonObject;
        return classification?["failedCount"]?.GetValue<int>() > 0 ? classification : null;
    }

    private static JsonObject? BuildCloudProviderDiagnostics(
        JsonObject? capabilityPreflight,
        JsonObject? compatibilityPreflight,
        JsonObject? statusPolicy,
        JsonObject? scheduleState,
        long observedAtUnixSeconds,
        JsonObject? providerErrorClassification = null)
    {
        return JsonNodeFromObjectJson(
            ActriumCore.BuildBrowserStorageSyncCloudProviderDiagnosticsJson(
                capabilityPreflight?.ToJsonString(JsonOptions) ?? string.Empty,
                compatibilityPreflight?.ToJsonString(JsonOptions) ?? string.Empty,
                providerErrorClassification?.ToJsonString(JsonOptions) ?? string.Empty,
                statusPolicy?.ToJsonString(JsonOptions) ?? string.Empty,
                scheduleState?.ToJsonString(JsonOptions) ?? string.Empty,
                observedAtUnixSeconds)) as JsonObject;
    }

    private static JsonObject? BuildCloudObservabilityEvent(
        JsonObject? providerDiagnostics,
        JsonObject? attemptPlan,
        JsonObject? loopPlan,
        long observedAtUnixSeconds)
    {
        return JsonNodeFromObjectJson(
            ActriumCore.BuildBrowserStorageSyncCloudObservabilityEventJson(
                providerDiagnostics?.ToJsonString(JsonOptions) ?? string.Empty,
                attemptPlan?.ToJsonString(JsonOptions) ?? string.Empty,
                loopPlan?.ToJsonString(JsonOptions) ?? string.Empty,
                observedAtUnixSeconds)) as JsonObject;
    }

    private static JsonObject? BuildCloudCycleAttemptPlan(
        string cycleRoundJson,
        uint retryAttempt,
        long observedAtUnixSeconds)
    {
        return JsonNodeFromObjectJson(
            ActriumCore.BuildBrowserStorageSyncCloudCycleAttemptPlanJson(
                cycleRoundJson,
                retryAttempt,
                observedAtUnixSeconds)) as JsonObject;
    }

    private static JsonObject? BuildCloudCycleLoopPlan(
        string cycleRoundJson,
        uint retryAttempt,
        long observedAtUnixSeconds,
        uint maxAttempts)
    {
        return JsonNodeFromObjectJson(
            ActriumCore.BuildBrowserStorageSyncCloudCycleLoopPlanJson(
                $"[{cycleRoundJson}]",
                retryAttempt,
                observedAtUnixSeconds,
                maxAttempts)) as JsonObject;
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

    private static JsonNode? CloneJsonNode(JsonNode node)
    {
        return JsonNode.Parse(node.ToJsonString(JsonOptions));
    }

    private sealed record BrowserStorageSyncCycleRequestPlan(
        string Method,
        string Url,
        IReadOnlyList<BrowserStorageSyncRequestHeader> Headers,
        string BodyJson)
    {
        public static BrowserStorageSyncCycleRequestPlan? Parse(string json)
        {
            if (string.Equals(json, "null", StringComparison.Ordinal))
            {
                return null;
            }

            try
            {
                return JsonSerializer.Deserialize<BrowserStorageSyncCycleRequestPlan>(json, JsonOptions);
            }
            catch (JsonException)
            {
                return null;
            }
        }

        public bool TryBuildHttpRequest(out BrowserStorageSyncHttpRequest request)
        {
            request = new BrowserStorageSyncHttpRequest(
                Method,
                new Uri("http://127.0.0.1"),
                new Dictionary<string, string>(StringComparer.Ordinal),
                BodyJson);

            if (!Uri.TryCreate(Url, UriKind.Absolute, out var uri))
            {
                return false;
            }

            request = new BrowserStorageSyncHttpRequest(
                Method,
                uri,
                Headers.ToDictionary(header => header.Name, header => header.Value, StringComparer.Ordinal),
                BodyJson);
            return true;
        }
    }

    private sealed record BrowserStorageSyncRequestHeader(string Name, string Value);
}
