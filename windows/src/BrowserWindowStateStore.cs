using System.Text.Json;
using System.Text.Json.Serialization;

namespace Actro.Windows;

internal sealed class BrowserWindowStateStore
{
    private const int CurrentVersion = 1;

    internal const int MinimumWidth = 960;
    internal const int MinimumHeight = 640;

    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
        WriteIndented = true,
    };

    private readonly string filePath;

    public BrowserWindowStateStore(string filePath)
    {
        this.filePath = filePath;
    }

    public static BrowserWindowStateStore CreateDefault()
    {
        return new BrowserWindowStateStore(
            Path.Combine(WindowsAppPaths.GetLocalAppDataRoot(), "browser-window.json"));
    }

    public BrowserWindowStateSnapshot? Load()
    {
        if (!File.Exists(filePath))
        {
            return null;
        }

        try
        {
            using var stream = File.OpenRead(filePath);
            var snapshot = JsonSerializer.Deserialize<BrowserWindowStateSnapshot>(stream, JsonOptions);
            return NormalizeSnapshot(snapshot);
        }
        catch (IOException)
        {
            return null;
        }
        catch (JsonException)
        {
            return null;
        }
        catch (UnauthorizedAccessException)
        {
            return null;
        }
    }

    public void Save(BrowserWindowStateSnapshot snapshot)
    {
        var normalized = NormalizeSnapshot(snapshot);
        if (normalized is null)
        {
            return;
        }

        var directory = Path.GetDirectoryName(filePath);
        if (!string.IsNullOrWhiteSpace(directory))
        {
            Directory.CreateDirectory(directory);
        }

        using var stream = File.Create(filePath);
        JsonSerializer.Serialize(stream, normalized, JsonOptions);
    }

    public BrowserWindowStateSnapshot CreateSnapshot(
        int x,
        int y,
        int width,
        int height,
        bool isMaximized)
    {
        return NormalizeSnapshot(new BrowserWindowStateSnapshot(
            CurrentVersion,
            x,
            y,
            width,
            height,
            isMaximized))!;
    }

    private static BrowserWindowStateSnapshot? NormalizeSnapshot(BrowserWindowStateSnapshot? snapshot)
    {
        if (snapshot is null)
        {
            return null;
        }

        return new BrowserWindowStateSnapshot(
            CurrentVersion,
            snapshot.X,
            snapshot.Y,
            Math.Max(MinimumWidth, snapshot.Width),
            Math.Max(MinimumHeight, snapshot.Height),
            snapshot.IsMaximized);
    }
}

internal sealed record BrowserWindowStateSnapshot(
    int Version,
    int X,
    int Y,
    int Width,
    int Height,
    bool IsMaximized);
