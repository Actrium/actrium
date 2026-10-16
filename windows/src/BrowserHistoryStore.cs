using System.Text.Json;
using System.Text.Json.Serialization;
using Microsoft.Data.Sqlite;

namespace Actrium.Windows;

internal sealed class BrowserHistoryStore
{
    private const int CurrentVersion = 1;
    private const int MaxPersistedItems = 500;
    private const int MaxTitleLength = 160;
    private const long MinPersistedUnixSeconds = 0;
    private const long MaxPersistedUnixSeconds = 253402300799;

    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
        WriteIndented = true,
    };

    private const string LegacyMigrationKey = "windows_browser_history_json_migrated_v1";

    private readonly BrowserStorageDatabase database;
    private readonly string legacyFilePath;

    public BrowserHistoryStore(string filePath)
        : this(BrowserStorageDatabase.CreateForLegacyFile(filePath), filePath)
    {
    }

    internal BrowserHistoryStore(BrowserStorageDatabase database, string legacyFilePath)
    {
        this.database = database;
        this.legacyFilePath = legacyFilePath;
    }

    public static BrowserHistoryStore CreateDefault()
    {
        return new BrowserHistoryStore(
            Path.Combine(WindowsAppPaths.GetLocalAppDataRoot(), "browser-history.json"));
    }

    public BrowserHistorySnapshot Load()
    {
        try
        {
            ImportLegacySnapshotIfNeeded();
            return CreateSnapshot(database.LoadHistoryItems(MaxPersistedItems));
        }
        catch (SqliteException)
        {
            return BrowserHistorySnapshot.Empty();
        }
        catch (IOException)
        {
            return BrowserHistorySnapshot.Empty();
        }
        catch (UnauthorizedAccessException)
        {
            return BrowserHistorySnapshot.Empty();
        }
    }

    public void Save(BrowserHistorySnapshot snapshot)
    {
        var normalized = NormalizeSnapshot(snapshot);
        database.SaveHistorySnapshot(normalized);
    }

    public void AddVisit(string? title, string? address, DateTimeOffset visitedAt)
    {
        if (!TryCreateItem(title, address, visitedAt, out var newItem))
        {
            return;
        }

        var existingItem = Load().Items.FirstOrDefault(item =>
            string.Equals(item.Address, newItem.Address, StringComparison.OrdinalIgnoreCase));
        var persistedItem = newItem;
        if (existingItem is not null)
        {
            persistedItem = newItem with
            {
                VisitCount = Math.Max(1, existingItem.VisitCount) + 1,
            };
        }

        database.UpsertHistoryItem(persistedItem);
        database.InsertHistoryVisit(newItem);
        database.PruneHistoryItems(MaxPersistedItems);
    }

    public void Clear()
    {
        database.DeleteAllHistory();
    }

    public bool RemoveAddress(string? address)
    {
        if (!TryNormalizeHistoryAddress(address, out var normalizedAddress))
        {
            return false;
        }

        return database.DeleteHistoryByAddress(normalizedAddress);
    }

    public BrowserHistorySnapshot CreateSnapshot(IReadOnlyList<BrowserHistoryItemSnapshot> items)
    {
        return NormalizeSnapshot(new BrowserHistorySnapshot(CurrentVersion, items));
    }

    internal static bool TryCreateItem(
        string? title,
        string? address,
        DateTimeOffset visitedAt,
        out BrowserHistoryItemSnapshot item)
    {
        item = new BrowserHistoryItemSnapshot(string.Empty, string.Empty, 0, 0);

        if (!TryNormalizeHistoryAddress(address, out var normalizedAddress)
            || !Uri.TryCreate(normalizedAddress, UriKind.Absolute, out var uri)
            || !IsRecordableUri(uri))
        {
            return false;
        }

        item = new BrowserHistoryItemSnapshot(
            NormalizeTitle(title, uri),
            normalizedAddress,
            NormalizeUnixSeconds(visitedAt.ToUnixTimeSeconds()),
            VisitCount: 1);
        return true;
    }

    internal static bool TryNormalizeHistoryAddress(string? address, out string normalizedAddress)
    {
        if (string.IsNullOrWhiteSpace(address)
            || NavigationPolicy.IsInternalAddress(address)
            || !NavigationPolicy.TryNormalizeAddress(address, out normalizedAddress)
            || !Uri.TryCreate(normalizedAddress, UriKind.Absolute, out var uri)
            || !IsRecordableUri(uri))
        {
            normalizedAddress = string.Empty;
            return false;
        }

        return true;
    }

    private static BrowserHistorySnapshot NormalizeSnapshot(BrowserHistorySnapshot? snapshot)
    {
        // The Rust core owns snapshot normalization (canonical addresses,
        // titles, dedupe, ordering, caps); its output is persisted verbatim.
        // Unparseable or missing core output is contract drift and must fail
        // loudly.
        var snapshotJson = JsonSerializer.Serialize(snapshot, JsonOptions);
        var normalizedJson = ActriumCore.NormalizeBrowserHistorySnapshotJson(snapshotJson);
        try
        {
            return JsonSerializer.Deserialize<BrowserHistorySnapshot>(normalizedJson, JsonOptions)
                ?? throw new InvalidOperationException(
                    "Browser history snapshot normalization from the Rust core returned null.");
        }
        catch (JsonException error)
        {
            throw new InvalidOperationException(
                "Browser history snapshot JSON from the Rust core could not be parsed.",
                error);
        }
    }

    private static bool IsRecordableUri(Uri uri)
    {
        return uri.Scheme == Uri.UriSchemeHttp
            || uri.Scheme == Uri.UriSchemeHttps
            || uri.Scheme == Uri.UriSchemeFile;
    }

    private static string NormalizeTitle(string? title, Uri uri)
    {
        var normalizedTitle = string.IsNullOrWhiteSpace(title)
            ? uri.Host
            : title.Trim();
        if (string.IsNullOrWhiteSpace(normalizedTitle))
        {
            normalizedTitle = "Visited Page";
        }

        return normalizedTitle.Length <= MaxTitleLength
            ? normalizedTitle
            : normalizedTitle[..MaxTitleLength];
    }

    private static long NormalizeUnixSeconds(long unixSeconds)
    {
        return Math.Clamp(unixSeconds, MinPersistedUnixSeconds, MaxPersistedUnixSeconds);
    }

    private void ImportLegacySnapshotIfNeeded()
    {
        if (database.HasMetaFlag(LegacyMigrationKey))
        {
            return;
        }

        if (database.HasHistoryItems())
        {
            database.SetMetaFlag(LegacyMigrationKey);
            return;
        }

        var legacySnapshot = LoadLegacySnapshot();
        if (legacySnapshot is null)
        {
            return;
        }

        database.SaveHistorySnapshot(legacySnapshot);
        database.SetMetaFlag(LegacyMigrationKey);
    }

    private BrowserHistorySnapshot? LoadLegacySnapshot()
    {
        if (!File.Exists(legacyFilePath))
        {
            return null;
        }

        try
        {
            using var stream = File.OpenRead(legacyFilePath);
            var snapshot = JsonSerializer.Deserialize<BrowserHistorySnapshot>(stream, JsonOptions);
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
}

internal sealed record BrowserHistorySnapshot(
    int Version,
    IReadOnlyList<BrowserHistoryItemSnapshot> Items)
{
    public static BrowserHistorySnapshot Empty()
    {
        return new BrowserHistorySnapshot(1, []);
    }
}

internal sealed record BrowserHistoryItemSnapshot(
    string Title,
    string Address,
    long VisitedAtUnixSeconds,
    int VisitCount);
