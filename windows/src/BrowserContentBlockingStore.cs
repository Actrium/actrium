using System.Text.Json;
using System.Text.Json.Serialization;

namespace Actrium.Windows;

internal sealed class BrowserContentBlockingStore
{
    private const int CurrentVersion = 1;

    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
        WriteIndented = true,
    };

    private readonly string filePath;

    public BrowserContentBlockingStore(string filePath)
    {
        this.filePath = filePath;
    }

    public static BrowserContentBlockingStore CreateDefault()
    {
        return new BrowserContentBlockingStore(
            Path.Combine(WindowsAppPaths.GetLocalAppDataRoot(), "content-blocking.json"));
    }

    public BrowserContentBlockingSnapshot Load()
    {
        if (!File.Exists(filePath))
        {
            return BrowserContentBlockingSnapshot.Default();
        }

        try
        {
            using var stream = File.OpenRead(filePath);
            var snapshot = JsonSerializer.Deserialize<BrowserContentBlockingSnapshot>(stream, JsonOptions);
            return NormalizeSnapshot(snapshot);
        }
        catch (IOException)
        {
            return BrowserContentBlockingSnapshot.Default();
        }
        catch (JsonException)
        {
            return BrowserContentBlockingSnapshot.Default();
        }
        catch (UnauthorizedAccessException)
        {
            return BrowserContentBlockingSnapshot.Default();
        }
    }

    public void Save(BrowserContentBlockingSnapshot snapshot)
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

    public BrowserContentBlockingSnapshot CreateSnapshot(bool isEnabled)
    {
        return new BrowserContentBlockingSnapshot(CurrentVersion, isEnabled);
    }

    private static BrowserContentBlockingSnapshot NormalizeSnapshot(BrowserContentBlockingSnapshot? snapshot)
    {
        return snapshot is null
            ? BrowserContentBlockingSnapshot.Default()
            : new BrowserContentBlockingSnapshot(CurrentVersion, snapshot.IsEnabled);
    }
}

internal sealed record BrowserContentBlockingSnapshot(int Version, bool IsEnabled)
{
    public static BrowserContentBlockingSnapshot Default()
    {
        return new BrowserContentBlockingSnapshot(1, BrowserContentBlockingPolicy.DefaultIsEnabled);
    }
}
