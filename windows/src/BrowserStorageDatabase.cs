using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Linq;
using Microsoft.Data.Sqlite;

namespace Actrium.Windows;

internal sealed class BrowserStorageDatabase
{
    public const string DefaultSessionId = "default";

    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);
    private static readonly Lazy<BrowserStorageContract> SharedContract = new(BrowserStorageContract.Load);
    private static readonly Lazy<BrowserStorageSyncSqlContract> SharedSyncSql = new(BrowserStorageSyncSqlContract.Load);

    private readonly string databasePath;

    public BrowserStorageDatabase(string databasePath)
    {
        this.databasePath = databasePath;
    }

    public string DatabasePath => databasePath;

    public static int SchemaVersion => Contract.SchemaVersion;

    private static BrowserStorageContract Contract => SharedContract.Value;

    private static BrowserStorageSyncSqlContract SyncSql => SharedSyncSql.Value;

    public static BrowserStorageDatabase CreateDefault()
    {
        return new BrowserStorageDatabase(
            GetDatabasePathInDirectory(WindowsAppPaths.GetLocalAppDataRoot()));
    }

    public static BrowserStorageDatabase CreateForLegacyFile(string legacyFilePath)
    {
        return new BrowserStorageDatabase(GetDatabasePathForLegacyFile(legacyFilePath));
    }

    internal static string GetDatabasePathForLegacyFile(string legacyFilePath)
    {
        var directory = Path.GetDirectoryName(legacyFilePath);
        var fileName = Path.GetFileName(legacyFilePath);
        if (string.Equals(fileName, "browser-history.json", StringComparison.OrdinalIgnoreCase)
            || string.Equals(fileName, "browser-bookmarks.json", StringComparison.OrdinalIgnoreCase)
            || string.Equals(fileName, "browser-session.json", StringComparison.OrdinalIgnoreCase))
        {
            return GetDatabasePathInDirectory(
                string.IsNullOrWhiteSpace(directory) ? WindowsAppPaths.GetLocalAppDataRoot() : directory);
        }

        return Path.ChangeExtension(legacyFilePath, ".db");
    }

    internal static string GetDatabasePathInDirectory(string directory)
    {
        // Open in place so uncheckpointed WAL transactions remain attached to the database.
        var legacyPath = Path.Combine(directory, "actro_browser.db");
        return File.Exists(legacyPath) ? legacyPath : Path.Combine(directory, Contract.DatabaseName);
    }

    public bool HasMetaFlag(string key)
    {
        try
        {
            using var connection = OpenInitializedConnection();
            using var command = CreateCommand(
                connection,
                null,
                "SELECT value FROM browser_schema_meta WHERE key = ? LIMIT 1",
                key);
            return command.ExecuteScalar() is not null;
        }
        catch (SqliteException)
        {
            return false;
        }
        catch (IOException)
        {
            return false;
        }
        catch (UnauthorizedAccessException)
        {
            return false;
        }
    }

    public void SetMetaFlag(string key, string value = "1")
    {
        using var connection = OpenInitializedConnection();
        using var command = CreateCommand(
            connection,
            null,
            "INSERT OR REPLACE INTO browser_schema_meta (key, value) VALUES (?, ?)",
            key,
            value);
        command.ExecuteNonQuery();
    }

    public bool HasHistoryItems()
    {
        return Exists("SELECT 1 FROM browser_history_pages WHERE deleted_at IS NULL LIMIT 1");
    }

    public bool HasBookmarkItems()
    {
        return Exists("SELECT 1 FROM browser_bookmarks WHERE is_deleted = 0 AND deleted_at IS NULL LIMIT 1");
    }

    public bool HasSession(string sessionId)
    {
        return Exists("SELECT 1 FROM browser_sessions WHERE id = ? LIMIT 1", sessionId);
    }

    public IReadOnlyList<BrowserHistoryItemSnapshot> LoadHistoryItems(int limit)
    {
        using var connection = OpenInitializedConnection();
        using var command = CreateCommand(
            connection,
            null,
            Contract.Sql.SelectHistoryRecent,
            limit);
        using var reader = command.ExecuteReader();
        var items = new List<BrowserHistoryItemSnapshot>();
        while (reader.Read())
        {
            items.Add(new BrowserHistoryItemSnapshot(
                reader.GetString(1),
                reader.GetString(0),
                reader.GetInt64(3),
                Convert.ToInt32(reader.GetInt64(2))));
        }

        return items;
    }

    public void SaveHistorySnapshot(BrowserHistorySnapshot snapshot)
    {
        using var connection = OpenInitializedConnection();
        using var transaction = connection.BeginTransaction();
        using (var deleteCommand = CreateCommand(
                   connection,
                   transaction,
                   Contract.Sql.DeleteAllHistory))
        {
            deleteCommand.ExecuteNonQuery();
        }

        foreach (var item in snapshot.Items)
        {
            UpsertHistoryItem(connection, transaction, item);
        }

        transaction.Commit();
    }

    public void UpsertHistoryItem(BrowserHistoryItemSnapshot item)
    {
        using var connection = OpenInitializedConnection();
        UpsertHistoryItem(connection, null, item);
    }

    public void InsertHistoryVisit(BrowserHistoryItemSnapshot item, string transition = "typed")
    {
        using var connection = OpenInitializedConnection();
        using var command = CreateCommand(
            connection,
            null,
            Contract.Sql.InsertHistoryVisit,
            Guid.NewGuid().ToString("N"),
            item.Address,
            item.Title,
            item.VisitedAtUnixSeconds,
            transition);
        command.ExecuteNonQuery();
    }

    public bool DeleteHistoryByAddress(string address)
    {
        using var connection = OpenInitializedConnection();
        using var command = CreateCommand(
            connection,
            null,
            Contract.Sql.DeleteHistoryByAddress,
            address);
        return command.ExecuteNonQuery() > 0;
    }

    public void DeleteAllHistory()
    {
        using var connection = OpenInitializedConnection();
        using var command = CreateCommand(connection, null, Contract.Sql.DeleteAllHistory);
        command.ExecuteNonQuery();
    }

    public void PruneHistoryItems(int maxItems)
    {
        using var connection = OpenInitializedConnection();
        using var command = CreateCommand(
            connection,
            null,
            """
            DELETE FROM browser_history_pages
            WHERE address IN (
                SELECT address
                FROM browser_history_pages
                ORDER BY last_visited_at DESC
                LIMIT -1 OFFSET ?
            )
            """,
            maxItems);
        command.ExecuteNonQuery();
    }

    public IReadOnlyList<BrowserBookmarkItemSnapshot> LoadBookmarkItems(int limit)
    {
        using var connection = OpenInitializedConnection();
        using var command = CreateCommand(
            connection,
            null,
            Contract.Sql.SelectBookmarksRecent,
            limit);
        using var reader = command.ExecuteReader();
        var items = new List<BrowserBookmarkItemSnapshot>();
        while (reader.Read())
        {
            items.Add(new BrowserBookmarkItemSnapshot(
                reader.GetString(2),
                reader.GetString(3),
                reader.GetInt64(4)));
        }

        return items;
    }

    public void SaveBookmarkSnapshot(BrowserBookmarkSnapshot snapshot)
    {
        using var connection = OpenInitializedConnection();
        using var transaction = connection.BeginTransaction();
        using (var deleteCommand = CreateCommand(
                   connection,
                   transaction,
                   Contract.Sql.DeleteAllBookmarks))
        {
            deleteCommand.ExecuteNonQuery();
        }

        for (var index = 0; index < snapshot.Items.Count; index++)
        {
            UpsertBookmarkItem(connection, transaction, snapshot.Items[index], index);
        }

        transaction.Commit();
    }

    public bool DeleteBookmarkByAddress(string address)
    {
        using var connection = OpenInitializedConnection();
        using var command = CreateCommand(
            connection,
            null,
            Contract.Sql.DeleteBookmarkByAddress,
            address);
        return command.ExecuteNonQuery() > 0;
    }

    public void SaveSessionSnapshot(string sessionId, BrowserSessionSnapshot snapshot)
    {
        var now = DateTimeOffset.UtcNow.ToUnixTimeSeconds();
        var tabIdRequests = new List<(string? Id, int Index, string Address)>(snapshot.Tabs.Count);
        for (var index = 0; index < snapshot.Tabs.Count; index++)
        {
            tabIdRequests.Add((snapshot.Tabs[index].Id, index, snapshot.Tabs[index].Address));
        }

        var tabIds = ResolveSessionTabIdPlanIds(tabIdRequests);
        using var connection = OpenInitializedConnection();
        using var transaction = connection.BeginTransaction();
        using (var upsertSessionCommand = CreateCommand(
                   connection,
                   transaction,
                   Contract.Sql.UpsertSession,
                   sessionId,
                   snapshot.ActiveTabIndex,
                   snapshot.OwnerKind,
                   snapshot.OwnerId,
                   snapshot.SurfaceKind,
                   snapshot.SurfaceId,
                   now,
                   now))
        {
            upsertSessionCommand.ExecuteNonQuery();
        }

        using (var deleteTabsCommand = CreateCommand(
                   connection,
                   transaction,
                   Contract.Sql.DeleteSessionTabs,
                   sessionId))
        {
            deleteTabsCommand.ExecuteNonQuery();
        }

        for (var index = 0; index < snapshot.Tabs.Count; index++)
        {
            var tab = snapshot.Tabs[index];
            using var insertTabCommand = CreateCommand(
                connection,
                transaction,
                Contract.Sql.InsertSessionTab,
                sessionId,
                index,
                tabIds[index],
                tab.Title,
                tab.Address,
                now,
                now);
            insertTabCommand.ExecuteNonQuery();
        }

        transaction.Commit();
    }

    public BrowserSessionSnapshot? LoadSessionSnapshot(string sessionId)
    {
        using var connection = OpenInitializedConnection();
        using var sessionCommand = CreateCommand(
            connection,
            null,
            Contract.Sql.SelectSession,
            sessionId);
        var activeTabIndexValue = sessionCommand.ExecuteScalar();
        if (activeTabIndexValue is null)
        {
            return null;
        }

        using var tabsCommand = CreateCommand(
            connection,
            null,
            Contract.Sql.SelectSessionTabs,
            sessionId);
        using var reader = tabsCommand.ExecuteReader();
        var tabRows = new List<(string Title, string Address, string? TabId, int TabIndex)>();
        while (reader.Read())
        {
            tabRows.Add((
                reader.GetString(2),
                reader.GetString(3),
                reader.IsDBNull(1) ? null : reader.GetString(1),
                reader.GetInt32(0)));
        }

        if (tabRows.Count == 0)
        {
            return null;
        }

        var tabIds = ResolveSessionTabIdPlanIds(
            tabRows.Select(row => (row.TabId, row.TabIndex, row.Address)).ToList());
        var tabs = new List<BrowserSessionTabSnapshot>(tabRows.Count);
        for (var index = 0; index < tabRows.Count; index++)
        {
            tabs.Add(new BrowserSessionTabSnapshot(
                tabRows[index].Title,
                tabRows[index].Address,
                tabIds[index]));
        }

        return new BrowserSessionSnapshot(
            1,
            Convert.ToInt32((long)activeTabIndexValue),
            tabs);
    }

    public string ApplyRemoteSyncBatch(string batchJson)
    {
        try
        {
            var planJson = ActriumCore.BuildBrowserStorageSyncRemoteApplyPlanJson(batchJson);
            return string.Equals(planJson, "null", StringComparison.Ordinal)
                ? "null"
                : ApplyRemoteSyncPlan(planJson);
        }
        catch (Exception error) when (IsStorageExecutionException(error))
        {
            return "null";
        }
    }

    public string ApplyRemoteSyncPlan(string planJson)
    {
        try
        {
            var plan = JsonSerializer.Deserialize<BrowserStorageSyncExecutionPlan>(planJson, JsonOptions);
            if (plan is null)
            {
                return "null";
            }

            var resultBatch = new BrowserStorageSyncExecutionResultBatch(
                plan.Version,
                plan.StorageSchemaVersion,
                plan.Scope,
                plan.DeviceId,
                plan.GeneratedAtUnixSeconds,
                ExecuteRemoteApplyPlan(plan));
            var resultJson = JsonSerializer.Serialize(resultBatch, JsonOptions);
            return ActriumCore.BuildBrowserStorageSyncRemoteApplyResultJson(planJson, resultJson);
        }
        catch (JsonException)
        {
            return "null";
        }
    }

    public bool ApplySyncAckBatch(string ackJson)
    {
        var planJson = ActriumCore.BuildBrowserStorageSyncAckPlanJson(ackJson);
        return !string.Equals(planJson, "null", StringComparison.Ordinal)
            && ApplySyncAckPlan(planJson);
    }

    public bool ApplySyncAckPlan(string planJson)
    {
        try
        {
            var plan = JsonSerializer.Deserialize<BrowserStorageSyncExecutionPlan>(planJson, JsonOptions);
            return plan is not null && ExecuteSyncAckPlan(plan);
        }
        catch (JsonException)
        {
            return false;
        }
    }

    public string ApplySyncCycleResponse(string responseJson)
    {
        try
        {
            var cyclePlanJson = ActriumCore.BuildBrowserStorageSyncClientCyclePlanJson(responseJson);
            if (string.Equals(cyclePlanJson, "null", StringComparison.Ordinal))
            {
                return "null";
            }

            using var cyclePlanDocument = JsonDocument.Parse(cyclePlanJson);
            var cyclePlan = cyclePlanDocument.RootElement;
            if (!cyclePlan.TryGetProperty("remoteApplyPlan", out var remoteApplyPlanElement))
            {
                return "null";
            }

            var remoteApplyPlanJson = remoteApplyPlanElement.GetRawText();
            var remoteApplyPlan = JsonSerializer.Deserialize<BrowserStorageSyncExecutionPlan>(
                remoteApplyPlanJson,
                JsonOptions);
            if (remoteApplyPlan is null)
            {
                return "null";
            }

            var remoteApplyResultJson = "null";
            if (remoteApplyPlan.Operations.Count > 0)
            {
                remoteApplyResultJson = ApplyRemoteSyncPlan(remoteApplyPlanJson);
            }

            var ackJson = ActriumCore.BuildBrowserStorageSyncClientAckBatchAfterRemoteApplyJson(
                responseJson,
                remoteApplyResultJson);
            if (string.Equals(ackJson, "null", StringComparison.Ordinal))
            {
                return "null";
            }

            var ackApplied = ApplySyncAckBatch(ackJson);
            return ActriumCore.BuildBrowserStorageSyncClientCycleApplyResultJson(
                responseJson,
                remoteApplyResultJson,
                ackApplied);
        }
        catch (JsonException)
        {
            return "null";
        }
    }

    public string ApplyCloudCycleClientPlan(string cycleRoundJson)
    {
        try
        {
            var clientPlanJson = ActriumCore.BuildBrowserStorageSyncCloudCycleClientPlanJson(cycleRoundJson);
            if (string.Equals(clientPlanJson, "null", StringComparison.Ordinal))
            {
                return "null";
            }

            using var clientPlanDocument = JsonDocument.Parse(clientPlanJson);
            var clientPlan = clientPlanDocument.RootElement;
            var stepResults = new List<Dictionary<string, object?>>();
            var remoteApplyResultJson = "null";

            if (clientPlan.TryGetProperty("steps", out var steps) && steps.ValueKind == JsonValueKind.Array)
            {
                foreach (var step in steps.EnumerateArray())
                {
                    var stepType = step.TryGetProperty("stepType", out var stepTypeElement)
                        ? stepTypeElement.GetString() ?? string.Empty
                        : string.Empty;
                    var required = !step.TryGetProperty("required", out var requiredElement)
                        || requiredElement.GetBoolean();
                    var success = true;

                    if (string.Equals(stepType, "applyLocalAck", StringComparison.Ordinal))
                    {
                        success = TryGetExecutionPlan(clientPlan, "ackPlan", out var ackPlan)
                            && ackPlan is not null
                            && ExecuteSyncAckPlan(ackPlan);
                    }
                    else if (string.Equals(stepType, "applyRemoteBatch", StringComparison.Ordinal))
                    {
                        if (clientPlan.TryGetProperty("remoteApplyPlan", out var remoteApplyPlanElement))
                        {
                            remoteApplyResultJson = ApplyRemoteSyncPlan(remoteApplyPlanElement.GetRawText());
                            success = !string.Equals(remoteApplyResultJson, "null", StringComparison.Ordinal);
                        }
                        else
                        {
                            success = false;
                        }
                    }

                    stepResults.Add(new Dictionary<string, object?>
                    {
                        ["stepType"] = stepType,
                        ["success"] = success,
                        ["error"] = null,
                    });

                    if (!success && required)
                    {
                        break;
                    }
                }
            }

            return ActriumCore.BuildBrowserStorageSyncCloudCycleClientApplyResultJson(
                cycleRoundJson,
                JsonSerializer.Serialize(stepResults, JsonOptions),
                remoteApplyResultJson);
        }
        catch (JsonException)
        {
            return "null";
        }
    }

    public string ExportDirtySyncBatch(
        string scope,
        string deviceId,
        long? generatedAtUnixSeconds = null,
        int limitPerCollection = 200)
    {
        try
        {
            return ActriumCore.BuildBrowserStorageSyncBatchFromDirtyRowsJson(
                scope,
                deviceId,
                generatedAtUnixSeconds ?? DateTimeOffset.UtcNow.ToUnixTimeSeconds(),
                DirtySyncRowsJson(limitPerCollection));
        }
        catch (Exception error) when (IsStorageExecutionException(error))
        {
            return "null";
        }
    }

    public string ExportDirtySyncPushRequest(
        string scope,
        string deviceId,
        long? generatedAtUnixSeconds = null,
        int limitPerCollection = 200)
    {
        return ActriumCore.BuildBrowserStorageSyncPushRequestFromDirtyRowsJson(
            scope,
            deviceId,
            generatedAtUnixSeconds ?? DateTimeOffset.UtcNow.ToUnixTimeSeconds(),
            DirtySyncRowsJson(limitPerCollection));
    }

    public string SyncCursorsJson(string scope)
    {
        try
        {
            var cursors = new List<BrowserStorageSyncCursor>();
            using var connection = OpenInitializedConnection();
            using var command = CreateCommand(
                connection,
                null,
                SyncSql.SelectSyncCursorsForScope,
                scope);
            using var reader = command.ExecuteReader();
            while (reader.Read())
            {
                cursors.Add(new BrowserStorageSyncCursor(
                    reader.GetString(0),
                    reader.GetString(1),
                    reader.IsDBNull(2) ? null : reader.GetInt64(2)));
            }
            return JsonSerializer.Serialize(cursors, JsonOptions);
        }
        catch (Exception error) when (IsStorageExecutionException(error))
        {
            return "[]";
        }
    }

    public string ExportSyncCycleRequestPlan(
        string baseUrl,
        string accessToken,
        string scope,
        string deviceId,
        long? generatedAtUnixSeconds = null,
        int limitPerCollection = 200)
    {
        var pushRequestJson = ExportDirtySyncPushRequest(
            scope,
            deviceId,
            generatedAtUnixSeconds,
            limitPerCollection);
        if (string.Equals(pushRequestJson, "null", StringComparison.Ordinal))
        {
            return "null";
        }

        return ActriumCore.BuildBrowserStorageSyncCycleRequestPlanJson(
            baseUrl,
            accessToken,
            pushRequestJson,
            SyncCursorsJson(scope));
    }

    private bool Exists(string sql, params object?[] values)
    {
        try
        {
            using var connection = OpenInitializedConnection();
            using var command = CreateCommand(connection, null, sql, values);
            return command.ExecuteScalar() is not null;
        }
        catch (SqliteException)
        {
            return false;
        }
        catch (IOException)
        {
            return false;
        }
        catch (UnauthorizedAccessException)
        {
            return false;
        }
    }

    private string DirtySyncRowsJson(int limitPerCollection)
    {
        var limit = Math.Clamp(limitPerCollection, 1, 1000);
        var rows = new List<Dictionary<string, object?>>();
        using var connection = OpenInitializedConnection();
        foreach (var statement in SyncSql.SelectDirtyStatements)
        {
            using var command = CreateCommand(connection, null, statement, limit);
            using var reader = command.ExecuteReader();
            while (reader.Read())
            {
                rows.Add(ReadJsonRow(reader));
            }
        }

        return JsonSerializer.Serialize(rows, JsonOptions);
    }

    private static Dictionary<string, object?> ReadJsonRow(SqliteDataReader reader)
    {
        var row = new Dictionary<string, object?>(StringComparer.Ordinal);
        for (var index = 0; index < reader.FieldCount; index++)
        {
            var value = reader.GetValue(index);
            row[reader.GetName(index)] = value is DBNull ? null : value;
        }
        return row;
    }

    private bool ExecuteSyncAckPlan(BrowserStorageSyncExecutionPlan plan)
    {
        SqliteConnection? connection = null;
        SqliteTransaction? transaction = null;
        try
        {
            connection = OpenInitializedConnection();
            transaction = connection.BeginTransaction();

            for (var ordinal = 0; ordinal < plan.Operations.Count; ordinal++)
            {
                var operation = plan.Operations[ordinal];
                var statement = SyncSql.StatementFor(operation.StatementKey);
                if (string.IsNullOrWhiteSpace(statement))
                {
                    TryRollback(transaction);
                    return false;
                }

                using var command = CreateCommand(
                    connection,
                    transaction,
                    statement,
                    operation.Params.Select(JsonElementToSqliteValue).ToArray());
                command.ExecuteNonQuery();
            }

            transaction.Commit();
            return true;
        }
        catch (Exception error) when (IsStorageExecutionException(error))
        {
            TryRollback(transaction);
            return false;
        }
        finally
        {
            transaction?.Dispose();
            connection?.Dispose();
        }
    }

    private static bool TryGetExecutionPlan(
        JsonElement parent,
        string propertyName,
        out BrowserStorageSyncExecutionPlan? plan)
    {
        plan = null;
        if (!parent.TryGetProperty(propertyName, out var planElement))
        {
            return false;
        }

        try
        {
            plan = JsonSerializer.Deserialize<BrowserStorageSyncExecutionPlan>(
                planElement.GetRawText(),
                JsonOptions);
            return plan is not null;
        }
        catch (JsonException)
        {
            return false;
        }
    }

    private IReadOnlyList<BrowserStorageSyncOperationResult> ExecuteRemoteApplyPlan(
        BrowserStorageSyncExecutionPlan plan)
    {
        SqliteConnection? connection = null;
        SqliteTransaction? transaction = null;
        try
        {
            connection = OpenInitializedConnection();
            transaction = connection.BeginTransaction();
            var results = new List<BrowserStorageSyncOperationResult>(plan.Operations.Count);

            for (var ordinal = 0; ordinal < plan.Operations.Count; ordinal++)
            {
                var operation = plan.Operations[ordinal];
                var statement = SyncSql.StatementFor(operation.StatementKey);
                if (string.IsNullOrWhiteSpace(statement))
                {
                    TryRollback(transaction);
                    return RemoteApplyFailureResults(
                        plan,
                        ordinal,
                        $"unknown statement key: {operation.StatementKey}");
                }

                try
                {
                    using var command = CreateCommand(
                        connection,
                        transaction,
                        statement,
                        operation.Params.Select(JsonElementToSqliteValue).ToArray());
                    results.Add(new BrowserStorageSyncOperationResult(
                        ordinal,
                        command.ExecuteNonQuery(),
                        null));
                }
                catch (Exception error) when (IsStorageExecutionException(error))
                {
                    TryRollback(transaction);
                    return RemoteApplyFailureResults(
                        plan,
                        ordinal,
                        $"statement execution failed: {operation.StatementKey}: {error.Message}");
                }
            }

            try
            {
                transaction.Commit();
            }
            catch (Exception error) when (IsStorageExecutionException(error))
            {
                TryRollback(transaction);
                return RemoteApplyFailureResults(
                    plan,
                    null,
                    $"commit transaction failed: {error.Message}");
            }

            return results;
        }
        catch (Exception error) when (IsStorageExecutionException(error))
        {
            TryRollback(transaction);
            return RemoteApplyFailureResults(plan, null, $"transaction failed: {error.Message}");
        }
        finally
        {
            transaction?.Dispose();
            connection?.Dispose();
        }
    }

    private static IReadOnlyList<BrowserStorageSyncOperationResult> RemoteApplyFailureResults(
        BrowserStorageSyncExecutionPlan plan,
        int? failedOrdinal,
        string message)
    {
        var results = new List<BrowserStorageSyncOperationResult>(plan.Operations.Count);
        for (var ordinal = 0; ordinal < plan.Operations.Count; ordinal++)
        {
            var error = message;
            if (failedOrdinal is int failed)
            {
                if (ordinal < failed)
                {
                    error = "rolled back";
                }
                else if (ordinal > failed)
                {
                    error = "not executed";
                }
            }
            results.Add(new BrowserStorageSyncOperationResult(ordinal, 0, error));
        }

        return results;
    }

    private static object? JsonElementToSqliteValue(JsonElement value)
    {
        return value.ValueKind switch
        {
            JsonValueKind.Null or JsonValueKind.Undefined => null,
            JsonValueKind.String => value.GetString(),
            JsonValueKind.True => 1L,
            JsonValueKind.False => 0L,
            JsonValueKind.Number when value.TryGetInt64(out var integer) => integer,
            JsonValueKind.Number => value.GetDouble(),
            _ => value.GetRawText(),
        };
    }

    private static void TryRollback(SqliteTransaction? transaction)
    {
        try
        {
            transaction?.Rollback();
        }
        catch (SqliteException)
        {
        }
        catch (InvalidOperationException)
        {
        }
    }

    private static bool IsStorageExecutionException(Exception error)
    {
        return error is SqliteException
            or InvalidOperationException
            or IOException
            or UnauthorizedAccessException;
    }

    private static void UpsertHistoryItem(
        SqliteConnection connection,
        SqliteTransaction? transaction,
        BrowserHistoryItemSnapshot item)
    {
        using var command = CreateCommand(
            connection,
            transaction,
            Contract.Sql.UpsertHistoryPage,
            item.Address,
            item.Title,
            item.VisitCount,
            item.VisitedAtUnixSeconds,
            item.VisitedAtUnixSeconds,
            item.VisitedAtUnixSeconds,
            null);
        command.ExecuteNonQuery();
    }

    private static void UpsertBookmarkItem(
        SqliteConnection connection,
        SqliteTransaction? transaction,
        BrowserBookmarkItemSnapshot item,
        int sortOrder)
    {
        using var command = CreateCommand(
            connection,
            transaction,
            Contract.Sql.UpsertBookmark,
            StableId("bookmark", item.Address),
            null,
            item.Title,
            item.Address,
            item.CreatedAtUnixSeconds,
            item.CreatedAtUnixSeconds,
            sortOrder,
            null,
            "user",
            string.Empty);
        command.ExecuteNonQuery();
    }

    private SqliteConnection OpenInitializedConnection()
    {
        var directory = Path.GetDirectoryName(databasePath);
        if (!string.IsNullOrWhiteSpace(directory))
        {
            Directory.CreateDirectory(directory);
        }

        var connectionString = new SqliteConnectionStringBuilder
        {
            DataSource = databasePath,
            Mode = SqliteOpenMode.ReadWriteCreate,
        }.ToString();
        var connection = new SqliteConnection(connectionString);
        connection.Open();

        foreach (var statement in Contract.SchemaStatements)
        {
            using var command = connection.CreateCommand();
            command.CommandText = statement;
            command.ExecuteNonQuery();
        }

        MigrateBrowserStorageSchemaIfNeeded(connection);
        return connection;
    }

    // Replays the Rust-owned incremental schema migration plan
    // (browser_storage_schema_migration_plan_json). All DDL literals, their
    // order, and the schema version stamp come from the shared contract; this
    // adapter only performs PRAGMA table_info column introspection and
    // statement execution.
    private static void MigrateBrowserStorageSchemaIfNeeded(SqliteConnection connection)
    {
        var plan = Contract.MigrationPlan;
        foreach (var column in plan.AddColumns)
        {
            if (ColumnExists(connection, column.Table, column.Column))
            {
                continue;
            }

            using var command = connection.CreateCommand();
            command.CommandText = column.Statement;
            command.ExecuteNonQuery();
        }

        foreach (var statement in plan.IdempotentStatements)
        {
            using var command = connection.CreateCommand();
            command.CommandText = statement;
            command.ExecuteNonQuery();
        }
    }

    private static bool ColumnExists(SqliteConnection connection, string table, string column)
    {
        using var command = connection.CreateCommand();
        command.CommandText = $"PRAGMA table_info({table})";
        using var reader = command.ExecuteReader();
        while (reader.Read())
        {
            if (string.Equals(reader.GetString(1), column, StringComparison.OrdinalIgnoreCase))
            {
                return true;
            }
        }

        return false;
    }

    // Resolves stable session tab ids for a whole session in one batched Rust
    // call (browser_storage_session_tab_id_plan_json), so the id fallback
    // derivation stays byte-identical across all platforms. The returned list
    // is always index-aligned with the request; if the plan payload does not
    // match the request shape the raw trimmed ids pass through unchanged.
    private static IReadOnlyList<string> ResolveSessionTabIdPlanIds(
        IReadOnlyList<(string? Id, int Index, string Address)> tabs)
    {
        var fallbackIds = tabs.Select(tab => tab.Id?.Trim() ?? string.Empty).ToList();
        if (tabs.Count == 0)
        {
            return fallbackIds;
        }

        var request = tabs.Select(tab => new
        {
            id = tab.Id ?? string.Empty,
            index = tab.Index,
            address = tab.Address,
        });
        var planJson = ActriumCore.BuildBrowserStorageSessionTabIdPlanJson(
            JsonSerializer.Serialize(request));

        try
        {
            using var plan = JsonDocument.Parse(planJson);
            if (plan.RootElement.ValueKind != JsonValueKind.Object
                || !plan.RootElement.TryGetProperty("tabIds", out var tabIdsElement)
                || tabIdsElement.ValueKind != JsonValueKind.Array
                || tabIdsElement.GetArrayLength() != tabs.Count)
            {
                return fallbackIds;
            }

            return tabIdsElement.EnumerateArray()
                .Select(element => element.GetString() ?? string.Empty)
                .ToList();
        }
        catch (JsonException)
        {
            return fallbackIds;
        }
    }

    private static SqliteCommand CreateCommand(
        SqliteConnection connection,
        SqliteTransaction? transaction,
        string sql,
        params object?[] values)
    {
        var command = connection.CreateCommand();
        command.Transaction = transaction;
        command.CommandText = BindPositionalParameters(sql, values.Length);
        for (var index = 0; index < values.Length; index++)
        {
            var parameter = command.CreateParameter();
            parameter.ParameterName = $"$p{index}";
            parameter.Value = values[index] ?? DBNull.Value;
            command.Parameters.Add(parameter);
        }

        return command;
    }

    private static string BindPositionalParameters(string sql, int valueCount)
    {
        var builder = new StringBuilder(sql.Length + valueCount * 3);
        var index = 0;
        foreach (var character in sql)
        {
            if (character != '?')
            {
                builder.Append(character);
                continue;
            }

            if (index >= valueCount)
            {
                throw new InvalidOperationException("SQL placeholder count exceeds provided values.");
            }

            builder.Append("$p").Append(index);
            index++;
        }

        if (index != valueCount)
        {
            throw new InvalidOperationException("Provided values exceed SQL placeholder count.");
        }

        return builder.ToString();
    }

    private static string StableId(string prefix, string value)
    {
        var bytes = SHA256.HashData(Encoding.UTF8.GetBytes(value.ToLowerInvariant()));
        return $"{prefix}-{Convert.ToHexString(bytes).ToLowerInvariant()}";
    }

    private sealed record BrowserStorageContract(
        int SchemaVersion,
        string DatabaseName,
        IReadOnlyList<string> SchemaStatements,
        BrowserStorageRepositorySqlContract Sql,
        BrowserStorageMigrationPlanContract MigrationPlan)
    {
        // Loads the Rust-owned browser storage contract. There is no C# copy
        // of the schema or SQL anymore: any drift (unparseable JSON, missing
        // statements, unexpected schema version) is a build/packaging defect
        // and must fail loudly instead of degrading silently.
        public static BrowserStorageContract Load()
        {
            var schema = ActriumCore.LoadBrowserStorageSchema();
            IReadOnlyList<string>? schemaStatements;
            BrowserStorageRepositorySqlContract? repositorySql;
            BrowserStorageMigrationPlanContract? migrationPlan;
            try
            {
                schemaStatements = JsonSerializer.Deserialize<IReadOnlyList<string>>(
                    schema.SchemaStatementsJson,
                    JsonOptions);
                repositorySql = JsonSerializer.Deserialize<BrowserStorageRepositorySqlContract>(
                    schema.RepositorySqlJson,
                    JsonOptions);
                migrationPlan = JsonSerializer.Deserialize<BrowserStorageMigrationPlanContract>(
                    ActriumCore.LoadBrowserStorageSchemaMigrationPlanJson(),
                    JsonOptions);
            }
            catch (JsonException error)
            {
                throw new InvalidOperationException(
                    "Browser storage contract JSON from the Rust core could not be parsed.",
                    error);
            }

            if (schema.SchemaVersion != 6
                || string.IsNullOrWhiteSpace(schema.DatabaseName)
                || schemaStatements is not { Count: > 0 }
                || repositorySql is null
                || !repositorySql.IsComplete
                || migrationPlan is null
                || !migrationPlan.IsComplete)
            {
                throw new InvalidOperationException(
                    $"Browser storage contract from the Rust core is incomplete (schema version {schema.SchemaVersion}).");
            }

            return new BrowserStorageContract(
                (int)schema.SchemaVersion,
                schema.DatabaseName,
                schemaStatements,
                repositorySql,
                migrationPlan);
        }
    }

    private sealed record BrowserStorageMigrationColumnContract
    {
        public string Table { get; init; } = string.Empty;
        public string Column { get; init; } = string.Empty;
        public string Statement { get; init; } = string.Empty;

        public bool IsComplete =>
            !string.IsNullOrWhiteSpace(Table)
            && !string.IsNullOrWhiteSpace(Column)
            && !string.IsNullOrWhiteSpace(Statement);
    }

    // Deserialized form of the Rust browser_storage_schema_migration_plan_json
    // contract. AddColumns must be applied in order (each gated on column
    // absence); IdempotentStatements are safe to execute unconditionally.
    private sealed record BrowserStorageMigrationPlanContract
    {
        public int SchemaVersion { get; init; }
        public IReadOnlyList<BrowserStorageMigrationColumnContract> AddColumns { get; init; } = [];
        public IReadOnlyList<string> IdempotentStatements { get; init; } = [];

        public bool IsComplete =>
            SchemaVersion == 6
            && AddColumns is { Count: > 0 }
            && IdempotentStatements is { Count: > 0 }
            && AddColumns.All(column => column.IsComplete)
            && IdempotentStatements.All(statement => !string.IsNullOrWhiteSpace(statement));
    }

    private sealed record BrowserStorageRepositorySqlContract
    {
        public string UpsertHistoryPage { get; init; } = string.Empty;
        public string InsertHistoryVisit { get; init; } = string.Empty;
        public string SelectHistoryRecent { get; init; } = string.Empty;
        public string DeleteHistoryByAddress { get; init; } = string.Empty;
        public string DeleteAllHistory { get; init; } = string.Empty;
        public string UpsertBookmark { get; init; } = string.Empty;
        public string SelectBookmarksRecent { get; init; } = string.Empty;
        public string DeleteBookmarkByAddress { get; init; } = string.Empty;
        public string DeleteAllBookmarks { get; init; } = string.Empty;
        public string UpsertSession { get; init; } = string.Empty;
        public string SelectSession { get; init; } = string.Empty;
        public string DeleteSessionTabs { get; init; } = string.Empty;
        public string InsertSessionTab { get; init; } = string.Empty;
        public string SelectSessionTabs { get; init; } = string.Empty;

        public bool IsComplete =>
            !string.IsNullOrWhiteSpace(UpsertHistoryPage)
            && !string.IsNullOrWhiteSpace(InsertHistoryVisit)
            && !string.IsNullOrWhiteSpace(SelectHistoryRecent)
            && !string.IsNullOrWhiteSpace(DeleteHistoryByAddress)
            && !string.IsNullOrWhiteSpace(DeleteAllHistory)
            && !string.IsNullOrWhiteSpace(UpsertBookmark)
            && !string.IsNullOrWhiteSpace(SelectBookmarksRecent)
            && !string.IsNullOrWhiteSpace(DeleteBookmarkByAddress)
            && !string.IsNullOrWhiteSpace(DeleteAllBookmarks)
            && !string.IsNullOrWhiteSpace(UpsertSession)
            && !string.IsNullOrWhiteSpace(SelectSession)
            && !string.IsNullOrWhiteSpace(DeleteSessionTabs)
            && !string.IsNullOrWhiteSpace(InsertSessionTab)
            && !string.IsNullOrWhiteSpace(SelectSessionTabs);
    }

    private sealed record BrowserStorageSyncExecutionPlan
    {
        public int Version { get; init; }
        public int StorageSchemaVersion { get; init; }
        public string Scope { get; init; } = string.Empty;
        public string DeviceId { get; init; } = string.Empty;
        public long GeneratedAtUnixSeconds { get; init; }
        public IReadOnlyList<BrowserStorageSyncOperation> Operations { get; init; } = [];
    }

    private sealed record BrowserStorageSyncOperation
    {
        public string StatementKey { get; init; } = string.Empty;
        public IReadOnlyList<JsonElement> Params { get; init; } = [];
    }

    private sealed record BrowserStorageSyncExecutionResultBatch(
        int Version,
        int StorageSchemaVersion,
        string Scope,
        string DeviceId,
        long GeneratedAtUnixSeconds,
        IReadOnlyList<BrowserStorageSyncOperationResult> Results);

    private sealed record BrowserStorageSyncOperationResult(
        int OperationIndex,
        long RowsAffected,
        string? Error);

    private sealed record BrowserStorageSyncCursor(
        string Collection,
        string Cursor,
        long? LastSyncedAtUnixSeconds);

    private sealed record BrowserStorageSyncSqlContract
    {
        public string SelectDirtyHistoryPages { get; init; } = string.Empty;
        public string SelectDirtyHistoryVisits { get; init; } = string.Empty;
        public string SelectDirtyBookmarkFolders { get; init; } = string.Empty;
        public string SelectDirtyBookmarks { get; init; } = string.Empty;
        public string SelectDirtySessions { get; init; } = string.Empty;
        public string SelectDirtySessionTabs { get; init; } = string.Empty;
        public string ApplyRemoteHistoryPage { get; init; } = string.Empty;
        public string ApplyRemoteHistoryVisit { get; init; } = string.Empty;
        public string ApplyRemoteBookmarkFolder { get; init; } = string.Empty;
        public string ApplyRemoteBookmark { get; init; } = string.Empty;
        public string ApplyRemoteSession { get; init; } = string.Empty;
        public string ApplyRemoteSessionTab { get; init; } = string.Empty;
        public string MarkHistoryPageSynced { get; init; } = string.Empty;
        public string MarkHistoryVisitSynced { get; init; } = string.Empty;
        public string MarkBookmarkFolderSynced { get; init; } = string.Empty;
        public string MarkBookmarkSynced { get; init; } = string.Empty;
        public string MarkSessionSynced { get; init; } = string.Empty;
        public string MarkSessionTabSynced { get; init; } = string.Empty;
        public string UpsertSyncCursor { get; init; } = string.Empty;
        public string SelectSyncCursorsForScope { get; init; } = string.Empty;

        public IReadOnlyList<string> SelectDirtyStatements => new[]
        {
            SelectDirtyHistoryPages,
            SelectDirtyHistoryVisits,
            SelectDirtyBookmarkFolders,
            SelectDirtyBookmarks,
            SelectDirtySessions,
            SelectDirtySessionTabs,
        };

        public bool IsComplete =>
            !string.IsNullOrWhiteSpace(SelectDirtyHistoryPages)
            && !string.IsNullOrWhiteSpace(SelectDirtyHistoryVisits)
            && !string.IsNullOrWhiteSpace(SelectDirtyBookmarkFolders)
            && !string.IsNullOrWhiteSpace(SelectDirtyBookmarks)
            && !string.IsNullOrWhiteSpace(SelectDirtySessions)
            && !string.IsNullOrWhiteSpace(SelectDirtySessionTabs)
            && !string.IsNullOrWhiteSpace(ApplyRemoteHistoryPage)
            && !string.IsNullOrWhiteSpace(ApplyRemoteHistoryVisit)
            && !string.IsNullOrWhiteSpace(ApplyRemoteBookmarkFolder)
            && !string.IsNullOrWhiteSpace(ApplyRemoteBookmark)
            && !string.IsNullOrWhiteSpace(ApplyRemoteSession)
            && !string.IsNullOrWhiteSpace(ApplyRemoteSessionTab)
            && !string.IsNullOrWhiteSpace(MarkHistoryPageSynced)
            && !string.IsNullOrWhiteSpace(MarkHistoryVisitSynced)
            && !string.IsNullOrWhiteSpace(MarkBookmarkFolderSynced)
            && !string.IsNullOrWhiteSpace(MarkBookmarkSynced)
            && !string.IsNullOrWhiteSpace(MarkSessionSynced)
            && !string.IsNullOrWhiteSpace(MarkSessionTabSynced)
            && !string.IsNullOrWhiteSpace(UpsertSyncCursor)
            && !string.IsNullOrWhiteSpace(SelectSyncCursorsForScope);

        public string? StatementFor(string statementKey)
        {
            var statement = statementKey switch
            {
                "applyRemoteHistoryPage" => ApplyRemoteHistoryPage,
                "applyRemoteHistoryVisit" => ApplyRemoteHistoryVisit,
                "applyRemoteBookmarkFolder" => ApplyRemoteBookmarkFolder,
                "applyRemoteBookmark" => ApplyRemoteBookmark,
                "applyRemoteSession" => ApplyRemoteSession,
                "applyRemoteSessionTab" => ApplyRemoteSessionTab,
                "markHistoryPageSynced" => MarkHistoryPageSynced,
                "markHistoryVisitSynced" => MarkHistoryVisitSynced,
                "markBookmarkFolderSynced" => MarkBookmarkFolderSynced,
                "markBookmarkSynced" => MarkBookmarkSynced,
                "markSessionSynced" => MarkSessionSynced,
                "markSessionTabSynced" => MarkSessionTabSynced,
                "upsertSyncCursor" => UpsertSyncCursor,
                "selectSyncCursorsForScope" => SelectSyncCursorsForScope,
                _ => null,
            };
            return string.IsNullOrWhiteSpace(statement) ? null : statement;
        }

        // Loads the Rust-owned sync SQL contract. Contract drift (unparseable
        // JSON or missing statements) is a build/packaging defect and must
        // fail loudly instead of degrading to an empty contract.
        public static BrowserStorageSyncSqlContract Load()
        {
            BrowserStorageSyncSqlContract? syncSql;
            try
            {
                syncSql = JsonSerializer.Deserialize<BrowserStorageSyncSqlContract>(
                    ActriumCore.LoadBrowserStorageSyncSqlJson(),
                    JsonOptions);
            }
            catch (JsonException error)
            {
                throw new InvalidOperationException(
                    "Browser storage sync SQL contract JSON from the Rust core could not be parsed.",
                    error);
            }

            if (syncSql is not { IsComplete: true })
            {
                throw new InvalidOperationException(
                    "Browser storage sync SQL contract from the Rust core is incomplete.");
            }

            return syncSql;
        }
    }
}
