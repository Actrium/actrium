using System.Security.Cryptography;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace Actro.Windows;

internal sealed class BrowserStorageSyncCredentialStore : IBrowserStorageSyncCredentialsProvider
{
    private const int CurrentVersion = 2;
    private const string DefaultScope = "default";

    // On-disk envelope formats. The access token is a bearer credential, so
    // production files are always DPAPI-protected for the current user.
    private const string DpapiFormat = "dpapi";
    private const string PlainFormat = "plain";

    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
        WriteIndented = true,
    };

    private readonly string filePath;

    public BrowserStorageSyncCredentialStore(string filePath)
    {
        this.filePath = filePath;
    }

    public static BrowserStorageSyncCredentialStore CreateDefault()
    {
        return new BrowserStorageSyncCredentialStore(
            Path.Combine(WindowsAppPaths.GetLocalAppDataRoot(), "browser-sync-credentials.json"));
    }

    public BrowserStorageSyncCredentialSnapshot? Load()
    {
        if (!File.Exists(filePath))
        {
            return null;
        }

        string fileText;
        try
        {
            fileText = File.ReadAllText(filePath);
        }
        catch (IOException)
        {
            return null;
        }
        catch (UnauthorizedAccessException)
        {
            return null;
        }

        string? envelopeFormat = null;
        try
        {
            using var document = JsonDocument.Parse(fileText);
            if (document.RootElement.ValueKind == JsonValueKind.Object
                && document.RootElement.TryGetProperty("format", out var formatElement))
            {
                envelopeFormat = formatElement.GetString() ?? string.Empty;
            }
        }
        catch (JsonException error)
        {
            // A present-but-unparseable credential file is corruption, not
            // "signed out"; treating it as absent would silently drop the
            // user's sync session.
            throw new InvalidOperationException(
                $"Browser sync credential file '{filePath}' is not valid JSON.",
                error);
        }

        if (envelopeFormat is null)
        {
            return LoadLegacyPlaintext(fileText);
        }

        return LoadEnvelope(envelopeFormat, fileText);
    }

    private BrowserStorageSyncCredentialSnapshot? LoadEnvelope(string format, string fileText)
    {
        var envelope = DeserializeEnvelope(fileText);
        byte[] payload;
        try
        {
            payload = Convert.FromBase64String(envelope.Payload ?? string.Empty);
        }
        catch (FormatException error)
        {
            throw new InvalidOperationException(
                $"Browser sync credential file '{filePath}' has a payload that is not valid base64.",
                error);
        }

        switch (format)
        {
            case DpapiFormat:
                if (!OperatingSystem.IsWindows())
                {
                    throw new InvalidOperationException(
                        $"Browser sync credential file '{filePath}' is DPAPI-protected and can only be read on the Windows account that saved it.");
                }

                try
                {
                    payload = ProtectedData.Unprotect(payload, optionalEntropy: null, DataProtectionScope.CurrentUser);
                }
                catch (CryptographicException error)
                {
                    throw new InvalidOperationException(
                        $"Browser sync credential file '{filePath}' could not be decrypted with DPAPI for the current user; the file may be corrupted or copied from another user or machine.",
                        error);
                }

                return NormalizeSnapshot(DeserializeSnapshotPayload(payload));
            case PlainFormat:
                var snapshot = NormalizeSnapshot(DeserializeSnapshotPayload(payload));
                if (OperatingSystem.IsWindows() && snapshot is not null)
                {
                    // A plain envelope on Windows is unprotected credential
                    // material (for example copied from a dev host); re-save
                    // immediately so it becomes DPAPI-protected.
                    Save(snapshot);
                }

                return snapshot;
            default:
                throw new InvalidOperationException(
                    $"Browser sync credential file '{filePath}' has unknown format '{format}'.");
        }
    }

    private BrowserStorageSyncCredentialSnapshot? LoadLegacyPlaintext(string fileText)
    {
        // Legacy files predate the envelope and stored the snapshot as
        // plaintext JSON. Read it once and migrate: on Windows the re-save
        // below rewrites it DPAPI-protected so plaintext never persists.
        BrowserStorageSyncCredentialSnapshot? snapshot;
        try
        {
            snapshot = NormalizeSnapshot(
                JsonSerializer.Deserialize<BrowserStorageSyncCredentialSnapshot>(fileText, JsonOptions));
        }
        catch (JsonException error)
        {
            throw new InvalidOperationException(
                $"Browser sync credential file '{filePath}' contains neither a credential envelope nor a legacy credential snapshot.",
                error);
        }

        if (snapshot is not null)
        {
            Save(snapshot);
        }

        return snapshot;
    }

    private BrowserStorageSyncCredentialEnvelope DeserializeEnvelope(string fileText)
    {
        try
        {
            return JsonSerializer.Deserialize<BrowserStorageSyncCredentialEnvelope>(fileText, JsonOptions)
                ?? throw new InvalidOperationException(
                    $"Browser sync credential file '{filePath}' deserialized to an empty envelope.");
        }
        catch (JsonException error)
        {
            throw new InvalidOperationException(
                $"Browser sync credential file '{filePath}' has a malformed credential envelope.",
                error);
        }
    }

    private BrowserStorageSyncCredentialSnapshot? DeserializeSnapshotPayload(byte[] payload)
    {
        try
        {
            return JsonSerializer.Deserialize<BrowserStorageSyncCredentialSnapshot>(payload, JsonOptions);
        }
        catch (JsonException error)
        {
            throw new InvalidOperationException(
                $"Browser sync credential file '{filePath}' has an envelope payload that is not a credential snapshot.",
                error);
        }
    }

    public void Save(BrowserStorageSyncCredentialSnapshot snapshot)
    {
        var normalized = NormalizeSnapshot(snapshot);
        if (normalized is null)
        {
            Clear();
            return;
        }

        var directory = Path.GetDirectoryName(filePath);
        if (!string.IsNullOrWhiteSpace(directory))
        {
            Directory.CreateDirectory(directory);
        }

        var payload = JsonSerializer.SerializeToUtf8Bytes(normalized, JsonOptions);
        BrowserStorageSyncCredentialEnvelope envelope;
        if (OperatingSystem.IsWindows())
        {
            envelope = new BrowserStorageSyncCredentialEnvelope(
                DpapiFormat,
                Convert.ToBase64String(
                    ProtectedData.Protect(payload, optionalEntropy: null, DataProtectionScope.CurrentUser)));
        }
        else
        {
            // Non-Windows hosts are the dev/test harness only (the Windows
            // test suite runs on macOS via verify:windows, where DPAPI throws
            // PlatformNotSupportedException); production is Windows-only, so
            // an explicit plain envelope is acceptable strictly in that
            // test-harness environment and is never written on Windows.
            envelope = new BrowserStorageSyncCredentialEnvelope(
                PlainFormat,
                Convert.ToBase64String(payload));
        }

        using var stream = File.Create(filePath);
        JsonSerializer.Serialize(stream, envelope, JsonOptions);
    }

    public void SaveSession(
        string environment,
        string accessToken,
        string? baseUrl = null,
        string scope = DefaultScope)
    {
        var normalizedEnvironment = NormalizeEnvironment(environment);
        Save(CreateSnapshot(
            string.IsNullOrWhiteSpace(baseUrl) ? DefaultBaseUrl(normalizedEnvironment) : baseUrl,
            accessToken,
            scope,
            deviceId: null,
            environment: normalizedEnvironment));
    }

    public void Clear()
    {
        try
        {
            if (File.Exists(filePath))
            {
                File.Delete(filePath);
            }
        }
        catch (IOException)
        {
        }
        catch (UnauthorizedAccessException)
        {
        }
    }

    public void ClearSession(string? environment = null)
    {
        var snapshot = Load();
        if (snapshot is null)
        {
            Clear();
            return;
        }

        var normalizedEnvironment = NormalizeText(environment);
        if (normalizedEnvironment.Length == 0
            || string.Equals(snapshot.Environment, normalizedEnvironment, StringComparison.Ordinal))
        {
            Clear();
        }
    }

    public BrowserStorageSyncCredentialSnapshot CreateSnapshot(
        string baseUrl,
        string accessToken,
        string scope = DefaultScope,
        string? deviceId = null,
        string? environment = null,
        long? savedAtUnixSeconds = null)
    {
        var normalizedEnvironment = NormalizeEnvironment(environment);
        return new BrowserStorageSyncCredentialSnapshot(
            CurrentVersion,
            normalizedEnvironment,
            NormalizeText(baseUrl).Length == 0 ? DefaultBaseUrl(normalizedEnvironment) : NormalizeText(baseUrl),
            NormalizeText(accessToken),
            NormalizeText(scope).Length == 0 ? DefaultScope : NormalizeText(scope),
            NormalizeText(deviceId).Length == 0 ? Guid.NewGuid().ToString() : NormalizeText(deviceId),
            savedAtUnixSeconds.GetValueOrDefault(DateTimeOffset.UtcNow.ToUnixTimeSeconds()));
    }

    public Task<BrowserStorageSyncCredentials?> GetCredentialsAsync(
        CancellationToken cancellationToken = default)
    {
        var snapshot = Load();
        if (snapshot is null)
        {
            return Task.FromResult<BrowserStorageSyncCredentials?>(null);
        }

        var credentials = new BrowserStorageSyncCredentials(
            snapshot.BaseUrl,
            snapshot.AccessToken,
            snapshot.Scope,
            snapshot.DeviceId);

        return Task.FromResult(credentials.IsUsable ? credentials : null);
    }

    private static BrowserStorageSyncCredentialSnapshot? NormalizeSnapshot(
        BrowserStorageSyncCredentialSnapshot? snapshot)
    {
        if (snapshot is null)
        {
            return null;
        }

        var normalized = new BrowserStorageSyncCredentialSnapshot(
            CurrentVersion,
            NormalizeEnvironment(snapshot.Environment),
            NormalizeText(snapshot.BaseUrl).Length == 0
                ? DefaultBaseUrl(NormalizeEnvironment(snapshot.Environment))
                : NormalizeText(snapshot.BaseUrl),
            NormalizeText(snapshot.AccessToken),
            NormalizeText(snapshot.Scope).Length == 0 ? DefaultScope : NormalizeText(snapshot.Scope),
            NormalizeText(snapshot.DeviceId).Length == 0
                ? Guid.NewGuid().ToString()
                : NormalizeText(snapshot.DeviceId),
            snapshot.SavedAtUnixSeconds > 0
                ? snapshot.SavedAtUnixSeconds
                : DateTimeOffset.UtcNow.ToUnixTimeSeconds());

        return string.IsNullOrWhiteSpace(normalized.AccessToken)
            || string.IsNullOrWhiteSpace(normalized.BaseUrl)
            ? null
            : normalized;
    }

    private static string NormalizeEnvironment(string? environment)
    {
        var normalized = NormalizeText(environment);
        return normalized.Length == 0 ? DefaultEnvironment() : normalized;
    }

    // The Rust core owns the auth environment and account endpoint contract;
    // an empty value is contract drift and must fail loudly instead of
    // degrading to a hand-copied C# endpoint.
    private static string DefaultEnvironment()
    {
        var environment = ActroCore.LoadAuthEnvironmentProd();
        return string.IsNullOrWhiteSpace(environment)
            ? throw new InvalidOperationException(
                "Default auth environment from the Rust core is empty.")
            : environment.Trim();
    }

    private static string DefaultBaseUrl(string environment)
    {
        var baseUrl = ActroCore.LoadAuthAccountBaseUrl(environment);
        return string.IsNullOrWhiteSpace(baseUrl)
            ? throw new InvalidOperationException(
                $"Auth account base URL from the Rust core is empty for environment '{environment}'.")
            : baseUrl.Trim();
    }

    private static string NormalizeText(string? value)
    {
        return value?.Trim() ?? string.Empty;
    }
}

// Self-describing on-disk wrapper for the credential snapshot: the payload is
// base64 of the snapshot JSON, DPAPI-protected when the format is "dpapi".
internal sealed record BrowserStorageSyncCredentialEnvelope(
    string? Format,
    string? Payload);

internal sealed record BrowserStorageSyncCredentialSnapshot(
    int Version,
    string Environment,
    string BaseUrl,
    string AccessToken,
    string Scope,
    string DeviceId,
    long SavedAtUnixSeconds);
