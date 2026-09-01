using System.Text.Json;
using System.Text.Json.Serialization;

namespace Actro.Windows;

internal sealed class BrowserStartupSettingsStore
{
    private const int CurrentVersion = 1;

    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
        WriteIndented = true,
    };

    private readonly string filePath;

    public BrowserStartupSettingsStore(string filePath)
    {
        this.filePath = filePath;
    }

    public static BrowserStartupSettingsStore CreateDefault()
    {
        return new BrowserStartupSettingsStore(
            Path.Combine(WindowsAppPaths.GetLocalAppDataRoot(), "startup-settings.json"));
    }

    public BrowserStartupSettingsSnapshot Load()
    {
        if (!File.Exists(filePath))
        {
            return BrowserStartupSettingsSnapshot.Default();
        }

        try
        {
            using var stream = File.OpenRead(filePath);
            var snapshot = JsonSerializer.Deserialize<BrowserStartupSettingsSnapshot>(stream, JsonOptions);
            return NormalizeSnapshot(snapshot);
        }
        catch (IOException)
        {
            return BrowserStartupSettingsSnapshot.Default();
        }
        catch (JsonException)
        {
            return BrowserStartupSettingsSnapshot.Default();
        }
        catch (UnauthorizedAccessException)
        {
            return BrowserStartupSettingsSnapshot.Default();
        }
    }

    public void Save(BrowserStartupSettingsSnapshot snapshot)
    {
        var normalized = NormalizeSnapshot(snapshot);
        var directory = Path.GetDirectoryName(filePath);
        if (!string.IsNullOrWhiteSpace(directory))
        {
            Directory.CreateDirectory(directory);
        }

        using var stream = File.Create(filePath);
        JsonSerializer.Serialize(stream, normalized, JsonOptions);
    }

    public BrowserStartupSettingsSnapshot CreateSnapshot(bool restorePreviousSession)
    {
        return new BrowserStartupSettingsSnapshot(CurrentVersion, restorePreviousSession);
    }

    private static BrowserStartupSettingsSnapshot NormalizeSnapshot(BrowserStartupSettingsSnapshot? snapshot)
    {
        return snapshot is null
            ? BrowserStartupSettingsSnapshot.Default()
            : new BrowserStartupSettingsSnapshot(CurrentVersion, snapshot.RestorePreviousSession);
    }
}

internal sealed record BrowserStartupSettingsSnapshot(int Version, bool RestorePreviousSession)
{
    public static BrowserStartupSettingsSnapshot Default()
    {
        return new BrowserStartupSettingsSnapshot(1, RestorePreviousSession: true);
    }
}
