using System.Text.Json;
using System.Text.Json.Serialization;
using Microsoft.Data.Sqlite;

namespace Actro.Windows;

internal sealed class BrowserSessionStore
{
    private const int CurrentVersion = 1;
    private const int MaxPersistedTabs = 50;

    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
        WriteIndented = true,
    };

    private const string LegacyMigrationKey = "windows_browser_session_json_migrated_v1";

    private readonly BrowserStorageDatabase database;
    private readonly string legacyFilePath;

    public BrowserSessionStore(string filePath)
        : this(BrowserStorageDatabase.CreateForLegacyFile(filePath), filePath)
    {
    }

    internal BrowserSessionStore(BrowserStorageDatabase database, string legacyFilePath)
    {
        this.database = database;
        this.legacyFilePath = legacyFilePath;
    }

    public static BrowserSessionStore CreateDefault()
    {
        return new BrowserSessionStore(
            Path.Combine(WindowsAppPaths.GetLocalAppDataRoot(), "browser-session.json"));
    }

    public BrowserSessionSnapshot? Load()
    {
        try
        {
            ImportLegacySnapshotIfNeeded();
            return NormalizeSnapshot(database.LoadSessionSnapshot(BrowserStorageDatabase.DefaultSessionId));
        }
        catch (SqliteException)
        {
            return null;
        }
        catch (IOException)
        {
            return null;
        }
        catch (UnauthorizedAccessException)
        {
            return null;
        }
    }

    public void Save(BrowserSessionSnapshot snapshot)
    {
        var normalized = NormalizeSnapshot(snapshot) ?? BrowserSessionSnapshot.Home();
        database.SaveSessionSnapshot(BrowserStorageDatabase.DefaultSessionId, normalized);
    }

    public BrowserSessionSnapshot CreateSnapshot(
        IReadOnlyList<BrowserSessionTabSnapshot> tabs,
        int activeTabIndex)
    {
        var sessionTabs = tabs
            .Take(MaxPersistedTabs)
            .ToList();

        return NormalizeSnapshot(new BrowserSessionSnapshot(CurrentVersion, activeTabIndex, sessionTabs))
            ?? BrowserSessionSnapshot.Home();
    }

    private static BrowserSessionSnapshot? NormalizeSnapshot(BrowserSessionSnapshot? snapshot)
    {
        // The Rust core owns snapshot normalization (canonical addresses,
        // titles, tab cap, active index clamp, stable tab ids); its output is
        // persisted verbatim. A null core result means the session resolved to
        // no persistable tabs. Unparseable core output is contract drift and
        // must fail loudly.
        var snapshotJson = JsonSerializer.Serialize(snapshot, JsonOptions);
        var normalizedJson = ActroCore.NormalizeBrowserSessionSnapshotJson(snapshotJson);
        try
        {
            return JsonSerializer.Deserialize<BrowserSessionSnapshot>(normalizedJson, JsonOptions);
        }
        catch (JsonException error)
        {
            throw new InvalidOperationException(
                "Browser session snapshot JSON from the Rust core could not be parsed.",
                error);
        }
    }

    // Validates a single tab for the in-memory closed-tab stack (reopen
    // closed tab); persisted snapshots go through the Rust normalization
    // above instead.
    internal static BrowserSessionTabSnapshot? NormalizeTab(BrowserSessionTabSnapshot tab)
    {
        var title = string.IsNullOrWhiteSpace(tab.Title) ? "New Tab" : tab.Title.Trim();
        var tabId = tab.Id;
        if (NavigationPolicy.IsHomeAddress(tab.Address))
        {
            return new BrowserSessionTabSnapshot(title, NavigationPolicy.HomeAddress, tabId);
        }

        if (NavigationPolicy.IsHistoryAddress(tab.Address))
        {
            return new BrowserSessionTabSnapshot(title, NavigationPolicy.HistoryAddress, tabId);
        }

        if (NavigationPolicy.IsBookmarksAddress(tab.Address))
        {
            return new BrowserSessionTabSnapshot(title, NavigationPolicy.BookmarksAddress, tabId);
        }

        if (NavigationPolicy.TryNormalizeAddress(tab.Address, out var normalizedAddress))
        {
            return new BrowserSessionTabSnapshot(title, normalizedAddress, tabId);
        }

        return null;
    }

    private void ImportLegacySnapshotIfNeeded()
    {
        if (database.HasMetaFlag(LegacyMigrationKey))
        {
            return;
        }

        if (database.HasSession(BrowserStorageDatabase.DefaultSessionId))
        {
            database.SetMetaFlag(LegacyMigrationKey);
            return;
        }

        var legacySnapshot = LoadLegacySnapshot();
        if (legacySnapshot is null)
        {
            return;
        }

        database.SaveSessionSnapshot(BrowserStorageDatabase.DefaultSessionId, legacySnapshot);
        database.SetMetaFlag(LegacyMigrationKey);
    }

    private BrowserSessionSnapshot? LoadLegacySnapshot()
    {
        if (!File.Exists(legacyFilePath))
        {
            return null;
        }

        try
        {
            using var stream = File.OpenRead(legacyFilePath);
            var snapshot = JsonSerializer.Deserialize<BrowserSessionSnapshot>(stream, JsonOptions);
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

internal sealed record BrowserSessionSnapshot(
    int Version,
    int ActiveTabIndex,
    IReadOnlyList<BrowserSessionTabSnapshot> Tabs,
    string OwnerKind = "device",
    string OwnerId = "",
    string SurfaceKind = "windows",
    string SurfaceId = "default")
{
    public static BrowserSessionSnapshot Home()
    {
        return new BrowserSessionSnapshot(
            1,
            0,
            [new BrowserSessionTabSnapshot("Actro", NavigationPolicy.HomeAddress)]);
    }
}

internal sealed record BrowserSessionTabSnapshot(string Title, string Address, string? Id = null);
