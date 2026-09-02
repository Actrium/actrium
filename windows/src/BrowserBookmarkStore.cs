using System.Text.Json;
using System.Text.Json.Serialization;
using Microsoft.Data.Sqlite;

namespace Actrium.Windows;

internal sealed class BrowserBookmarkStore
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

    private const string LegacyMigrationKey = "windows_browser_bookmarks_json_migrated_v1";

    private readonly BrowserStorageDatabase database;
    private readonly string legacyFilePath;

    public BrowserBookmarkStore(string filePath)
        : this(BrowserStorageDatabase.CreateForLegacyFile(filePath), filePath)
    {
    }

    internal BrowserBookmarkStore(BrowserStorageDatabase database, string legacyFilePath)
    {
        this.database = database;
        this.legacyFilePath = legacyFilePath;
    }

    public static BrowserBookmarkStore CreateDefault()
    {
        return new BrowserBookmarkStore(
            Path.Combine(WindowsAppPaths.GetLocalAppDataRoot(), "browser-bookmarks.json"));
    }

    public BrowserBookmarkSnapshot Load()
    {
        try
        {
            ImportLegacySnapshotIfNeeded();
            return CreateSnapshot(database.LoadBookmarkItems(MaxPersistedItems));
        }
        catch (SqliteException)
        {
            return BrowserBookmarkSnapshot.Empty();
        }
        catch (IOException)
        {
            return BrowserBookmarkSnapshot.Empty();
        }
        catch (UnauthorizedAccessException)
        {
            return BrowserBookmarkSnapshot.Empty();
        }
    }

    public void Save(BrowserBookmarkSnapshot snapshot)
    {
        var normalized = NormalizeSnapshot(snapshot);
        database.SaveBookmarkSnapshot(normalized);
    }

    public bool AddOrUpdate(string? title, string? address, DateTimeOffset createdAt)
    {
        if (!TryCreateItem(title, address, createdAt, out var newItem))
        {
            return false;
        }

        var existingItems = Load().Items;
        var mergedItems = new List<BrowserBookmarkItemSnapshot> { newItem };
        foreach (var item in existingItems)
        {
            if (string.Equals(item.Address, newItem.Address, StringComparison.OrdinalIgnoreCase))
            {
                mergedItems[0] = newItem with
                {
                    CreatedAtUnixSeconds = item.CreatedAtUnixSeconds,
                };
                continue;
            }

            mergedItems.Add(item);
        }

        Save(CreateSnapshot(mergedItems));
        return true;
    }

    public bool RemoveAddress(string? address)
    {
        if (!TryNormalizeBookmarkAddress(address, out var normalizedAddress))
        {
            return false;
        }

        return database.DeleteBookmarkByAddress(normalizedAddress);
    }

    public bool ContainsAddress(string? address)
    {
        if (!TryNormalizeBookmarkAddress(address, out var normalizedAddress))
        {
            return false;
        }

        return Load().Items.Any(item =>
            string.Equals(item.Address, normalizedAddress, StringComparison.OrdinalIgnoreCase));
    }

    public BrowserBookmarkSnapshot CreateSnapshot(IReadOnlyList<BrowserBookmarkItemSnapshot> items)
    {
        return NormalizeSnapshot(new BrowserBookmarkSnapshot(CurrentVersion, items));
    }

    public static bool CanBookmarkAddress(string? address)
    {
        return TryNormalizeBookmarkAddress(address, out _);
    }

    internal static bool TryCreateItem(
        string? title,
        string? address,
        DateTimeOffset createdAt,
        out BrowserBookmarkItemSnapshot item)
    {
        item = new BrowserBookmarkItemSnapshot(string.Empty, string.Empty, 0);

        if (!TryNormalizeBookmarkAddress(address, out var normalizedAddress)
            || !Uri.TryCreate(normalizedAddress, UriKind.Absolute, out var uri))
        {
            return false;
        }

        item = new BrowserBookmarkItemSnapshot(
            NormalizeTitle(title, uri),
            normalizedAddress,
            NormalizeUnixSeconds(createdAt.ToUnixTimeSeconds()));
        return true;
    }

    internal static bool TryNormalizeBookmarkAddress(string? address, out string normalizedAddress)
    {
        if (string.IsNullOrWhiteSpace(address)
            || NavigationPolicy.IsInternalAddress(address)
            || !NavigationPolicy.TryNormalizeAddress(address, out normalizedAddress)
            || !Uri.TryCreate(normalizedAddress, UriKind.Absolute, out var uri)
            || !IsBookmarkableUri(uri))
        {
            normalizedAddress = string.Empty;
            return false;
        }

        return true;
    }

    private static BrowserBookmarkSnapshot NormalizeSnapshot(BrowserBookmarkSnapshot? snapshot)
    {
        // The Rust core owns snapshot normalization (canonical addresses,
        // titles, dedupe, ordering, caps); its output is persisted verbatim.
        // Unparseable or missing core output is contract drift and must fail
        // loudly.
        var snapshotJson = JsonSerializer.Serialize(snapshot, JsonOptions);
        var normalizedJson = ActriumCore.NormalizeBrowserBookmarkSnapshotJson(snapshotJson);
        try
        {
            return JsonSerializer.Deserialize<BrowserBookmarkSnapshot>(normalizedJson, JsonOptions)
                ?? throw new InvalidOperationException(
                    "Browser bookmark snapshot normalization from the Rust core returned null.");
        }
        catch (JsonException error)
        {
            throw new InvalidOperationException(
                "Browser bookmark snapshot JSON from the Rust core could not be parsed.",
                error);
        }
    }

    private static bool IsBookmarkableUri(Uri uri)
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
            normalizedTitle = "Bookmarked Page";
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

        if (database.HasBookmarkItems())
        {
            database.SetMetaFlag(LegacyMigrationKey);
            return;
        }

        var legacySnapshot = LoadLegacySnapshot();
        if (legacySnapshot is null)
        {
            return;
        }

        database.SaveBookmarkSnapshot(legacySnapshot);
        database.SetMetaFlag(LegacyMigrationKey);
    }

    private BrowserBookmarkSnapshot? LoadLegacySnapshot()
    {
        if (!File.Exists(legacyFilePath))
        {
            return null;
        }

        try
        {
            using var stream = File.OpenRead(legacyFilePath);
            var snapshot = JsonSerializer.Deserialize<BrowserBookmarkSnapshot>(stream, JsonOptions);
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

internal sealed record BrowserBookmarkSnapshot(
    int Version,
    IReadOnlyList<BrowserBookmarkItemSnapshot> Items)
{
    public static BrowserBookmarkSnapshot Empty()
    {
        return new BrowserBookmarkSnapshot(1, []);
    }
}

internal sealed record BrowserBookmarkItemSnapshot(
    string Title,
    string Address,
    long CreatedAtUnixSeconds);
