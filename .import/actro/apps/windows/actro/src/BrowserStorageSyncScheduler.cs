using System.Text.Json;

namespace Actro.Windows;

internal sealed record BrowserStorageSyncSchedulerSnapshot
{
    public uint RetryAttempt { get; init; }
    public string SchedulerAction { get; init; } = "idle";
    public uint? NextRunDelaySeconds { get; init; }
    public long? NextRunAtUnixSeconds { get; init; }
    public bool WaitingForCredentials { get; init; }
    public bool RequiresUserAction { get; init; }
    public bool PendingRunScheduled { get; init; }
    public string? LastResultJson { get; init; }
}

internal sealed class BrowserStorageSyncScheduler : IDisposable
{
    private static readonly JsonSerializerOptions StatusJsonOptions = new(JsonSerializerDefaults.Web);

    private readonly BrowserStorageSyncCoordinator coordinator;
    private readonly object gate = new();

    private uint retryAttempt;
    private System.Threading.Timer? pendingTimer;
    private BrowserStorageSyncSchedulerSnapshot snapshot = new();
    private bool disposed;

    public BrowserStorageSyncScheduler(BrowserStorageSyncCoordinator coordinator)
    {
        this.coordinator = coordinator;
    }

    public BrowserStorageSyncSchedulerSnapshot Snapshot
    {
        get
        {
            lock (gate)
            {
                return snapshot;
            }
        }
    }

    public string StatusJson => ActroCore.BuildBrowserStorageSyncSchedulerStatusJson(
        BuildStatusSnapshotJson(Snapshot));

    public async Task<string> RunNowAsync(
        long? generatedAtUnixSeconds = null,
        int limitPerCollection = 200,
        long? observedAtUnixSeconds = null,
        CancellationToken cancellationToken = default)
    {
        CancelPendingSync();
        return await RunCycleAsync(
            generatedAtUnixSeconds,
            limitPerCollection,
            observedAtUnixSeconds,
            cancellationToken).ConfigureAwait(false);
    }

    public void ScheduleAfterLocalChange(TimeSpan? delay = null)
    {
        var due = delay ?? TimeSpan.FromSeconds(2);
        if (due < TimeSpan.Zero)
        {
            due = TimeSpan.Zero;
        }

        lock (gate)
        {
            ThrowIfDisposed();
            retryAttempt = 0;
            snapshot = new BrowserStorageSyncSchedulerSnapshot
            {
                SchedulerAction = "localChange",
                NextRunDelaySeconds = (uint)Math.Ceiling(due.TotalSeconds),
                NextRunAtUnixSeconds = DateTimeOffset.UtcNow.ToUnixTimeSeconds() + (long)Math.Ceiling(due.TotalSeconds),
                PendingRunScheduled = true,
                LastResultJson = snapshot.LastResultJson,
            };
            InstallPendingTimerLocked(due);
        }
    }

    public void CancelPendingSync()
    {
        lock (gate)
        {
            pendingTimer?.Dispose();
            pendingTimer = null;
            snapshot = snapshot with { PendingRunScheduled = false };
        }
    }

    public void Dispose()
    {
        lock (gate)
        {
            if (disposed)
            {
                return;
            }

            disposed = true;
            pendingTimer?.Dispose();
            pendingTimer = null;
        }
    }

    private async Task<string> RunCycleAsync(
        long? generatedAtUnixSeconds,
        int limitPerCollection,
        long? observedAtUnixSeconds,
        CancellationToken cancellationToken)
    {
        uint attempt;
        lock (gate)
        {
            ThrowIfDisposed();
            attempt = retryAttempt;
        }

        var resultJson = await coordinator.RunSyncCycleAsync(
            generatedAtUnixSeconds,
            limitPerCollection,
            attempt,
            observedAtUnixSeconds,
            cancellationToken).ConfigureAwait(false);
        ApplyResult(resultJson);
        return resultJson;
    }

    private async Task RunScheduledAsync()
    {
        lock (gate)
        {
            if (disposed)
            {
                return;
            }

            pendingTimer?.Dispose();
            pendingTimer = null;
            snapshot = snapshot with { PendingRunScheduled = false };
        }

        try
        {
            await RunCycleAsync(
                generatedAtUnixSeconds: null,
                limitPerCollection: 200,
                observedAtUnixSeconds: null,
                cancellationToken: default).ConfigureAwait(false);
        }
        catch (ObjectDisposedException)
        {
        }
        catch (InvalidOperationException)
        {
        }
    }

    private void ApplyResult(string resultJson)
    {
        if (!TryReadScheduleState(resultJson, out var scheduleState))
        {
            lock (gate)
            {
                retryAttempt = 0;
                pendingTimer?.Dispose();
                pendingTimer = null;
                snapshot = new BrowserStorageSyncSchedulerSnapshot
                {
                    LastResultJson = resultJson,
                };
            }
            return;
        }

        var state = scheduleState.GetValueOrDefault();
        var schedulerAction = JsonString(state, "schedulerAction");
        var nextRetryAttempt = JsonUInt(state, "nextRetryAttempt") ?? 0;
        var nextRunDelaySeconds = JsonUInt(state, "nextRunDelaySeconds");
        var nextRunAtUnixSeconds = JsonInt64(state, "nextRunAtUnixSeconds");
        var waitingForCredentials = JsonBool(state, "waitingForCredentials");
        var requiresUserAction = JsonBool(state, "requiresUserAction");
        var resetRetryAttempt = JsonBool(state, "resetRetryAttempt");
        var shouldSchedule = nextRunDelaySeconds.HasValue && !waitingForCredentials;

        lock (gate)
        {
            ThrowIfDisposed();
            retryAttempt = resetRetryAttempt ? 0 : nextRetryAttempt;
            snapshot = new BrowserStorageSyncSchedulerSnapshot
            {
                RetryAttempt = retryAttempt,
                SchedulerAction = schedulerAction,
                NextRunDelaySeconds = nextRunDelaySeconds,
                NextRunAtUnixSeconds = nextRunAtUnixSeconds,
                WaitingForCredentials = waitingForCredentials,
                RequiresUserAction = requiresUserAction,
                PendingRunScheduled = shouldSchedule,
                LastResultJson = resultJson,
            };

            if (nextRunDelaySeconds is uint delay && shouldSchedule)
            {
                InstallPendingTimerLocked(TimeSpan.FromSeconds(delay));
            }
            else
            {
                pendingTimer?.Dispose();
                pendingTimer = null;
            }
        }
    }

    private void InstallPendingTimerLocked(TimeSpan dueTime)
    {
        pendingTimer?.Dispose();
        pendingTimer = new System.Threading.Timer(
            static state =>
            {
                _ = ((BrowserStorageSyncScheduler)state!).RunScheduledAsync();
            },
            this,
            dueTime,
            Timeout.InfiniteTimeSpan);
    }

    private void ThrowIfDisposed()
    {
        if (disposed)
        {
            throw new ObjectDisposedException(nameof(BrowserStorageSyncScheduler));
        }
    }

    private static bool TryReadScheduleState(
        string resultJson,
        out JsonElement? scheduleState)
    {
        scheduleState = null;
        try
        {
            using var document = JsonDocument.Parse(resultJson);
            if (document.RootElement.TryGetProperty("scheduleState", out var state) &&
                state.ValueKind == JsonValueKind.Object)
            {
                scheduleState = state.Clone();
                return true;
            }
        }
        catch (JsonException)
        {
        }

        return false;
    }

    private static string JsonString(JsonElement value, string key)
    {
        return value.TryGetProperty(key, out var property) &&
            property.ValueKind == JsonValueKind.String
                ? property.GetString() ?? string.Empty
                : string.Empty;
    }

    private static bool JsonBool(JsonElement value, string key)
    {
        return value.TryGetProperty(key, out var property) &&
            property.ValueKind is JsonValueKind.True or JsonValueKind.False &&
            property.GetBoolean();
    }

    private static uint? JsonUInt(JsonElement value, string key)
    {
        if (!value.TryGetProperty(key, out var property) ||
            property.ValueKind == JsonValueKind.Null)
        {
            return null;
        }

        return property.TryGetUInt32(out var parsed) ? parsed : null;
    }

    private static long? JsonInt64(JsonElement value, string key)
    {
        if (!value.TryGetProperty(key, out var property) ||
            property.ValueKind == JsonValueKind.Null)
        {
            return null;
        }

        return property.TryGetInt64(out var parsed) ? parsed : null;
    }

    private static string BuildStatusSnapshotJson(BrowserStorageSyncSchedulerSnapshot snapshot)
    {
        return JsonSerializer.Serialize(snapshot, StatusJsonOptions);
    }
}
