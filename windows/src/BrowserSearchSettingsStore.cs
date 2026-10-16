using System.Text.Json;
using System.Text.Json.Serialization;

namespace Actrium.Windows;

internal sealed class BrowserSearchSettingsStore
{
    private const int CurrentVersion = 1;

    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
        WriteIndented = true,
    };

    private readonly string filePath;

    public BrowserSearchSettingsStore(string filePath)
    {
        this.filePath = filePath;
    }

    public static BrowserSearchSettingsStore CreateDefault()
    {
        return new BrowserSearchSettingsStore(
            Path.Combine(WindowsAppPaths.GetLocalAppDataRoot(), "search-settings.json"));
    }

    public BrowserSearchSettingsSnapshot Load()
    {
        if (!File.Exists(filePath))
        {
            return BrowserSearchSettingsSnapshot.Default();
        }

        try
        {
            using var stream = File.OpenRead(filePath);
            var snapshot = JsonSerializer.Deserialize<BrowserSearchSettingsSnapshot>(stream, JsonOptions);
            return NormalizeSnapshot(snapshot);
        }
        catch (IOException)
        {
            return BrowserSearchSettingsSnapshot.Default();
        }
        catch (JsonException)
        {
            return BrowserSearchSettingsSnapshot.Default();
        }
        catch (UnauthorizedAccessException)
        {
            return BrowserSearchSettingsSnapshot.Default();
        }
    }

    public void Save(BrowserSearchSettingsSnapshot snapshot)
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

    public BrowserSearchSettingsSnapshot CreateSnapshot(string? engineId)
    {
        return new BrowserSearchSettingsSnapshot(
            CurrentVersion,
            BrowserSearchEnginePolicy.NormalizeEngineId(engineId));
    }

    private static BrowserSearchSettingsSnapshot NormalizeSnapshot(BrowserSearchSettingsSnapshot? snapshot)
    {
        return snapshot is null
            ? BrowserSearchSettingsSnapshot.Default()
            : new BrowserSearchSettingsSnapshot(
                CurrentVersion,
                BrowserSearchEnginePolicy.NormalizeEngineId(snapshot.SearchEngineId));
    }
}

internal sealed record BrowserSearchSettingsSnapshot(
    int Version,
    string SearchEngineId)
{
    public static BrowserSearchSettingsSnapshot Default()
    {
        return new BrowserSearchSettingsSnapshot(1, BrowserSearchEnginePolicy.DefaultEngineId);
    }
}
