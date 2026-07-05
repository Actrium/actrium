using System.Text.Json;
using System.Text.Json.Serialization;

namespace Actro.Windows;

// Versioned browser sync provider connection settings, sibling of the
// endpoint credential snapshot. The record intentionally never contains a
// credential field: the local-folder provider has no secrets, and later
// cloud providers keep their tokens in provider-SDK/OS-owned storage, so the
// file stays plain JSON without DPAPI protection.
internal sealed record BrowserSyncProviderSettingsSnapshot(
    int Version,
    string? ProviderKind,
    string? LocalFolderRootPath,
    string Scope,
    string DeviceId,
    long? MembershipJoinedAtUnixSeconds,
    long SavedAtUnixSeconds);

// Owner of the durable browser sync device identity on Windows: DeviceId is
// minted (or adopted from the legacy endpoint credential snapshot) on first
// load, persisted immediately, and survives Disconnect so reconnecting the
// same folder resumes the same device membership instead of minting a ghost
// device. MembershipJoinedAtUnixSeconds is the persisted connection outcome:
// without it every cold start would re-run the membership round, which is a
// write to the shared cloud folder; a stale-but-wrong marker costs exactly
// one corrective round because the coordinator clears it on membership- or
// storage-scoped provider errors.
internal sealed class BrowserSyncProviderSettingsStore
{
    public const string LocalFolderProviderKind = "localFolder";

    private const int CurrentVersion = 1;
    private const string DefaultScope = "default";

    // The four roadmap providers of the cloud-drive architecture. Every
    // other token is contract drift and rejects to "no provider configured"
    // instead of flowing an unknown kind into the Rust connection contract.
    private static readonly string[] KnownProviderKinds =
    [
        LocalFolderProviderKind,
        "iCloudDrive",
        "googleDrive",
        "oneDrive",
    ];

    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
        WriteIndented = true,
    };

    private readonly string filePath;
    private readonly Func<string?>? legacyDeviceIdSource;

    public BrowserSyncProviderSettingsStore(
        string filePath,
        Func<string?>? legacyDeviceIdSource = null)
    {
        this.filePath = filePath;
        this.legacyDeviceIdSource = legacyDeviceIdSource;
    }

    public static BrowserSyncProviderSettingsStore CreateDefault(
        BrowserStorageSyncCredentialStore? legacyCredentialStore = null)
    {
        var credentialStore = legacyCredentialStore
            ?? BrowserStorageSyncCredentialStore.CreateDefault();
        return new BrowserSyncProviderSettingsStore(
            Path.Combine(WindowsAppPaths.GetLocalAppDataRoot(), "browser-sync-provider.json"),
            () => LoadLegacyDeviceId(credentialStore));
    }

    public BrowserSyncProviderSettingsSnapshot Load()
    {
        if (!File.Exists(filePath))
        {
            // First load: mint the durable device identity, adopting the
            // legacy endpoint credential device id when one exists so the
            // device stays continuous across the endpoint-to-provider
            // transition, and persist it immediately so it survives app
            // restarts and later disconnects.
            var created = Normalize(new BrowserSyncProviderSettingsSnapshot(
                CurrentVersion,
                ProviderKind: null,
                LocalFolderRootPath: null,
                DefaultScope,
                legacyDeviceIdSource?.Invoke() ?? string.Empty,
                MembershipJoinedAtUnixSeconds: null,
                SavedAtUnixSeconds: 0));
            return Save(created);
        }

        string fileText;
        try
        {
            fileText = File.ReadAllText(filePath);
        }
        catch (IOException error)
        {
            // An unreadable-but-present settings file must not silently
            // degrade to "not configured" and re-mint the device identity.
            throw new InvalidOperationException(
                $"Browser sync provider settings file '{filePath}' exists but could not be read.",
                error);
        }
        catch (UnauthorizedAccessException error)
        {
            throw new InvalidOperationException(
                $"Browser sync provider settings file '{filePath}' exists but could not be read.",
                error);
        }

        try
        {
            return Normalize(
                JsonSerializer.Deserialize<BrowserSyncProviderSettingsSnapshot>(fileText, JsonOptions));
        }
        catch (JsonException error)
        {
            // Corruption is not "disconnected": failing loudly preserves the
            // membership marker and device identity for manual recovery.
            throw new InvalidOperationException(
                $"Browser sync provider settings file '{filePath}' is not a valid settings snapshot.",
                error);
        }
    }

    public BrowserSyncProviderSettingsSnapshot Save(BrowserSyncProviderSettingsSnapshot snapshot)
    {
        var normalized = Normalize(snapshot);
        var directory = Path.GetDirectoryName(filePath);
        if (!string.IsNullOrWhiteSpace(directory))
        {
            Directory.CreateDirectory(directory);
        }

        using var stream = File.Create(filePath);
        JsonSerializer.Serialize(stream, normalized, JsonOptions);
        return normalized;
    }

    // Builds a snapshot over the current durable device id; the membership
    // marker intentionally defaults to null because a new (or re-targeted)
    // provider connection has not joined the new store yet.
    public BrowserSyncProviderSettingsSnapshot CreateSnapshot(
        string? providerKind,
        string? localFolderRootPath,
        string? scope = null,
        long? membershipJoinedAtUnixSeconds = null,
        long? savedAtUnixSeconds = null)
    {
        var current = Load();
        return Normalize(current with
        {
            ProviderKind = providerKind,
            LocalFolderRootPath = localFolderRootPath,
            Scope = scope ?? current.Scope,
            MembershipJoinedAtUnixSeconds = membershipJoinedAtUnixSeconds,
            SavedAtUnixSeconds = savedAtUnixSeconds ?? 0,
        });
    }

    public BrowserSyncProviderSettingsSnapshot SaveMembershipJoined(long joinedAtUnixSeconds)
    {
        return Save(Load() with
        {
            MembershipJoinedAtUnixSeconds = joinedAtUnixSeconds,
            SavedAtUnixSeconds = 0,
        });
    }

    public BrowserSyncProviderSettingsSnapshot ClearMembershipMarker()
    {
        return Save(Load() with
        {
            MembershipJoinedAtUnixSeconds = null,
            SavedAtUnixSeconds = 0,
        });
    }

    // Disconnect means "stop provider sync": the provider fields and the
    // membership marker are cleared, but DeviceId is durable identity and
    // stays, so a later reconnect resumes the same cloud device document.
    public BrowserSyncProviderSettingsSnapshot Disconnect()
    {
        return Save(Load() with
        {
            ProviderKind = null,
            LocalFolderRootPath = null,
            MembershipJoinedAtUnixSeconds = null,
            SavedAtUnixSeconds = 0,
        });
    }

    public static BrowserSyncProviderSettingsSnapshot Normalize(
        BrowserSyncProviderSettingsSnapshot? snapshot)
    {
        var providerKind = NormalizeProviderKind(snapshot?.ProviderKind);
        var rootPath = NormalizeText(snapshot?.LocalFolderRootPath);
        // The local-folder root must be absolute: a relative path would
        // silently resolve against the process working directory and sync
        // into the wrong folder. Rejecting it degrades the snapshot to "not
        // configured" instead of running against a guessed location. The
        // root path is a local-folder concept only.
        string? normalizedRootPath =
            providerKind == LocalFolderProviderKind
            && rootPath.Length > 0
            && Path.IsPathRooted(rootPath)
                ? rootPath
                : null;
        var configured = providerKind is not null
            && (providerKind != LocalFolderProviderKind || normalizedRootPath is not null);
        var membershipJoinedAt = snapshot?.MembershipJoinedAtUnixSeconds;

        return new BrowserSyncProviderSettingsSnapshot(
            CurrentVersion,
            providerKind,
            normalizedRootPath,
            NormalizeText(snapshot?.Scope).Length == 0 ? DefaultScope : NormalizeText(snapshot?.Scope),
            NormalizeText(snapshot?.DeviceId).Length == 0
                ? Guid.NewGuid().ToString()
                : NormalizeText(snapshot?.DeviceId),
            // A membership marker without a configured provider is
            // meaningless and would leak into the next connection.
            configured && membershipJoinedAt > 0 ? membershipJoinedAt : null,
            snapshot?.SavedAtUnixSeconds > 0
                ? snapshot.SavedAtUnixSeconds
                : DateTimeOffset.UtcNow.ToUnixTimeSeconds());
    }

    private static string? NormalizeProviderKind(string? providerKind)
    {
        var normalized = NormalizeText(providerKind);
        return KnownProviderKinds.FirstOrDefault(
            known => string.Equals(known, normalized, StringComparison.Ordinal));
    }

    private static string? LoadLegacyDeviceId(BrowserStorageSyncCredentialStore credentialStore)
    {
        try
        {
            return credentialStore.Load()?.DeviceId;
        }
        catch (InvalidOperationException)
        {
            // A corrupt endpoint credential file fails loudly on the
            // endpoint path itself; it must not also block provider
            // settings, so adoption just falls back to minting.
            return null;
        }
    }

    private static string NormalizeText(string? value)
    {
        return value?.Trim() ?? string.Empty;
    }
}
