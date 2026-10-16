using System;
using System.Runtime.InteropServices;
using System.Text;

namespace Actrium.Windows;

internal static class ActriumCore
{
    private const string LibraryName = "actrium_core";

    public static ActriumCoreStatus LoadStatus()
    {
        try
        {
            return ActriumCoreStatus.Available(
                productName: ReadString(ActriumCoreProductName()),
                cApiVersion: ActriumCoreCApiVersion(),
                supportedContractName: ReadString(ActriumCoreSupportedContractName()),
                supportedContractVersion: ActriumCoreSupportedContractVersion(),
                coreSchemaVersion: ActriumCoreSchemaVersion(),
                runtimeSchemaVersion: ActriumCoreRuntimeSchemaVersion());
        }
        catch (DllNotFoundException error)
        {
            return ActriumCoreStatus.Unavailable(error.Message);
        }
        catch (EntryPointNotFoundException error)
        {
            return ActriumCoreStatus.Unavailable(error.Message);
        }
        catch (BadImageFormatException error)
        {
            return ActriumCoreStatus.Unavailable(error.Message);
        }
    }

    public static IReadOnlyList<BrowserSearchEngineInfo> LoadBrowserSearchEngines()
    {
        var count = ActriumCoreBrowserSearchEngineCount();
        var engines = new List<BrowserSearchEngineInfo>(checked((int)count));
        for (uint index = 0; index < count; index++)
        {
            engines.Add(new BrowserSearchEngineInfo(
                ReadString(ActriumCoreBrowserSearchEngineId(index)),
                ReadString(ActriumCoreBrowserSearchEngineDisplayName(index)),
                ReadString(ActriumCoreBrowserSearchEngineUrlPrefix(index))));
        }

        return engines;
    }

    public static string LoadAuthEnvironmentProd()
    {
        return ReadString(ActriumCoreAuthEnvironmentProd());
    }

    public static string LoadAuthAccountBaseUrl(string? environment)
    {
        return ReadString(ActriumCoreAuthAccountBaseUrl(environment));
    }

    public static string LoadBrowserSearchEngineDefaultId()
    {
        return ReadString(ActriumCoreBrowserSearchEngineDefaultId());
    }

    public static string NormalizeBrowserSearchEngineId(string? engineId)
    {
        return ReadString(ActriumCoreBrowserSearchEngineNormalizeId(engineId));
    }

    public static string BuildBrowserSearchUrl(string query, string? engineId)
    {
        var safeQuery = query ?? string.Empty;
        var required = ActriumCoreBrowserSearchUrl(safeQuery, engineId, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserSearchUrl(safeQuery, engineId, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static BrowserDataContractInfo LoadBrowserDataContract()
    {
        return new BrowserDataContractInfo(
            ActriumCoreBrowserDataSchemaVersion(),
            ActriumCoreBrowserHistoryMaxItems(),
            ActriumCoreBrowserBookmarkMaxItems(),
            ActriumCoreBrowserSessionMaxTabs(),
            ActriumCoreBrowserDataMaxTitleLength(),
            ReadString(ActriumCoreBrowserHomeAddress()),
            ReadString(ActriumCoreBrowserHistoryAddress()),
            ReadString(ActriumCoreBrowserBookmarksAddress()));
    }

    public static BrowserStorageSchemaInfo LoadBrowserStorageSchema()
    {
        return new BrowserStorageSchemaInfo(
            ActriumCoreBrowserStorageSchemaVersion(),
            ReadString(ActriumCoreBrowserStorageDatabaseName()),
            ReadBufferString(ActriumCoreBrowserStorageSchemaStatementsJson),
            ReadBufferString(ActriumCoreBrowserStorageTableNamesJson),
            ReadBufferString(ActriumCoreBrowserStorageRepositorySqlJson));
    }

    public static string LoadBrowserStorageSchemaMigrationPlanJson()
    {
        return ReadBufferString(ActriumCoreBrowserStorageSchemaMigrationPlanJson);
    }

    public static string LoadBrowserStorageSyncSqlJson()
    {
        return ReadBufferString(ActriumCoreBrowserStorageSyncSqlJson);
    }

    public static string LoadBrowserStorageDataModelJson()
    {
        return ReadBufferString(ActriumCoreBrowserStorageDataModelJson);
    }

    public static string LoadBrowserStorageSyncConflictPolicyJson()
    {
        return ReadBufferString(ActriumCoreBrowserStorageSyncConflictPolicyJson);
    }

    public static string LoadBrowserStorageSyncMergePolicyJson()
    {
        return ReadBufferString(ActriumCoreBrowserStorageSyncMergePolicyJson);
    }

    public static string LoadBrowserStorageAddressCanonicalizationPolicyJson()
    {
        return ReadBufferString(ActriumCoreBrowserStorageAddressCanonicalizationPolicyJson);
    }

    public static string CanonicalizeBrowserStorageAddressesJson(string addressesJson)
    {
        var safeAddressesJson = addressesJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageCanonicalizeAddressesJson(safeAddressesJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageCanonicalizeAddressesJson(safeAddressesJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string LoadBrowserStorageBookmarkFolderTreeIntegrityPolicyJson()
    {
        return ReadBufferString(ActriumCoreBrowserStorageBookmarkFolderTreeIntegrityPolicyJson);
    }

    public static string BuildBrowserStorageBookmarkFolderTreeIntegrityPlanJson(string foldersJson)
    {
        var safeFoldersJson = foldersJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageBookmarkFolderTreeIntegrityPlanJson(safeFoldersJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageBookmarkFolderTreeIntegrityPlanJson(safeFoldersJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string LoadBrowserStorageBookmarkRestorePlacementPolicyJson()
    {
        return ReadBufferString(ActriumCoreBrowserStorageBookmarkRestorePlacementPolicyJson);
    }

    public static string BuildBrowserStorageBookmarkRestorePlacementPlanJson(string bookmarksJson)
    {
        var safeBookmarksJson = bookmarksJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageBookmarkRestorePlacementPlanJson(safeBookmarksJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageBookmarkRestorePlacementPlanJson(safeBookmarksJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string LoadBrowserStorageBookmarkOwnershipPolicyJson()
    {
        return ReadBufferString(ActriumCoreBrowserStorageBookmarkOwnershipPolicyJson);
    }

    public static string BuildBrowserStorageBookmarkOwnershipPlanJson(string bookmarksJson)
    {
        var safeBookmarksJson = bookmarksJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageBookmarkOwnershipPlanJson(safeBookmarksJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageBookmarkOwnershipPlanJson(safeBookmarksJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string LoadBrowserStorageHistoryVisitAggregationPolicyJson()
    {
        return ReadBufferString(ActriumCoreBrowserStorageHistoryVisitAggregationPolicyJson);
    }

    public static string BuildBrowserStorageHistoryVisitAggregationPlanJson(string visitsJson)
    {
        var safeVisitsJson = visitsJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageHistoryVisitAggregationPlanJson(safeVisitsJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageHistoryVisitAggregationPlanJson(safeVisitsJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string LoadBrowserStorageSessionTabOrderingPolicyJson()
    {
        return ReadBufferString(ActriumCoreBrowserStorageSessionTabOrderingPolicyJson);
    }

    public static string BuildBrowserStorageSessionTabOrderingPlanJson(string tabsJson)
    {
        var safeTabsJson = tabsJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSessionTabOrderingPlanJson(safeTabsJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSessionTabOrderingPlanJson(safeTabsJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSessionTabIdPlanJson(string tabsJson)
    {
        var safeTabsJson = tabsJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSessionTabIdPlanJson(safeTabsJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSessionTabIdPlanJson(safeTabsJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string LoadBrowserStorageSessionActiveTabConsistencyPolicyJson()
    {
        return ReadBufferString(ActriumCoreBrowserStorageSessionActiveTabConsistencyPolicyJson);
    }

    public static string BuildBrowserStorageSessionActiveTabConsistencyPlanJson(string sessionsJson)
    {
        var safeSessionsJson = sessionsJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSessionActiveTabConsistencyPlanJson(safeSessionsJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSessionActiveTabConsistencyPlanJson(safeSessionsJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string LoadBrowserStorageRetentionPruningPolicyJson()
    {
        return ReadBufferString(ActriumCoreBrowserStorageRetentionPruningPolicyJson);
    }

    public static string BuildBrowserStorageRetentionPruningPlanJson(string recordsJson)
    {
        var safeRecordsJson = recordsJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageRetentionPruningPlanJson(safeRecordsJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageRetentionPruningPlanJson(safeRecordsJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string NormalizeBrowserStorageSyncBatchJson(string batchJson)
    {
        var safeBatchJson = batchJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncBatchNormalizeJson(safeBatchJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncBatchNormalizeJson(safeBatchJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncBatchFromDirtyRowsJson(
        string? scope,
        string? deviceId,
        long generatedAtUnixSeconds,
        string rowsJson)
    {
        var safeRowsJson = rowsJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncBatchFromDirtyRowsJson(
            scope,
            deviceId,
            generatedAtUnixSeconds,
            safeRowsJson,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncBatchFromDirtyRowsJson(
            scope,
            deviceId,
            generatedAtUnixSeconds,
            safeRowsJson,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncPushRequestFromDirtyRowsJson(
        string? scope,
        string? deviceId,
        long generatedAtUnixSeconds,
        string rowsJson)
    {
        var safeRowsJson = rowsJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncPushRequestFromDirtyRowsJson(
            scope,
            deviceId,
            generatedAtUnixSeconds,
            safeRowsJson,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncPushRequestFromDirtyRowsJson(
            scope,
            deviceId,
            generatedAtUnixSeconds,
            safeRowsJson,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string NormalizeBrowserStorageSyncAckBatchJson(string ackJson)
    {
        var safeAckJson = ackJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncAckBatchNormalizeJson(safeAckJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncAckBatchNormalizeJson(safeAckJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string NormalizeBrowserStorageSyncCursorsJson(string cursorsJson)
    {
        var safeCursorsJson = cursorsJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCursorsNormalizeJson(safeCursorsJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCursorsNormalizeJson(safeCursorsJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string LoadEmptyBrowserStorageSyncBatchJson(string? scope, string? deviceId, long generatedAtUnixSeconds)
    {
        var required = ActriumCoreBrowserStorageSyncBatchEmptyJson(scope, deviceId, generatedAtUnixSeconds, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncBatchEmptyJson(scope, deviceId, generatedAtUnixSeconds, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncPushRequestJson(string batchJson)
    {
        var safeBatchJson = batchJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncPushRequestJson(safeBatchJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncPushRequestJson(safeBatchJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncPullRequestJson(
        string? scope,
        string? deviceId,
        long generatedAtUnixSeconds,
        string cursorsJson,
        uint maxRecordsPerCollection)
    {
        var safeCursorsJson = cursorsJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncPullRequestJson(
            scope,
            deviceId,
            generatedAtUnixSeconds,
            safeCursorsJson,
            maxRecordsPerCollection,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncPullRequestJson(
            scope,
            deviceId,
            generatedAtUnixSeconds,
            safeCursorsJson,
            maxRecordsPerCollection,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCycleResponseJson(string remoteBatchJson, string ackJson)
    {
        var safeRemoteBatchJson = remoteBatchJson ?? string.Empty;
        var safeAckJson = ackJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCycleResponseJson(safeRemoteBatchJson, safeAckJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCycleResponseJson(safeRemoteBatchJson, safeAckJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string NormalizeBrowserStorageSyncCycleResponseJson(string responseJson)
    {
        var safeResponseJson = responseJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCycleResponseNormalizeJson(safeResponseJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCycleResponseNormalizeJson(safeResponseJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncClientCyclePlanJson(string responseJson)
    {
        var safeResponseJson = responseJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncClientCyclePlanJson(safeResponseJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncClientCyclePlanJson(safeResponseJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncClientAckBatchAfterRemoteApplyJson(
        string responseJson,
        string remoteApplyResultJson)
    {
        var safeResponseJson = responseJson ?? string.Empty;
        var safeRemoteApplyResultJson = remoteApplyResultJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncClientAckBatchAfterRemoteApplyJson(
            safeResponseJson,
            safeRemoteApplyResultJson,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncClientAckBatchAfterRemoteApplyJson(
            safeResponseJson,
            safeRemoteApplyResultJson,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncClientCycleApplyResultJson(
        string responseJson,
        string remoteApplyResultJson,
        bool ackApplied)
    {
        var safeResponseJson = responseJson ?? string.Empty;
        var safeRemoteApplyResultJson = remoteApplyResultJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncClientCycleApplyResultJson(
            safeResponseJson,
            safeRemoteApplyResultJson,
            ackApplied,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncClientCycleApplyResultJson(
            safeResponseJson,
            safeRemoteApplyResultJson,
            ackApplied,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string LoadBrowserStorageSyncTransportContractJson()
    {
        return ReadBufferString(ActriumCoreBrowserStorageSyncTransportContractJson);
    }

    public static string LoadBrowserStorageSyncCloudStoreContractJson()
    {
        return ReadBufferString(ActriumCoreBrowserStorageSyncCloudStoreContractJson);
    }

    public static string LoadBrowserStorageSyncCloudProviderAdapterContractJson()
    {
        return ReadBufferString(ActriumCoreBrowserStorageSyncCloudProviderAdapterContractJson);
    }

    public static string LoadBrowserStorageSyncCloudProviderConnectionContractJson()
    {
        return ReadBufferString(ActriumCoreBrowserStorageSyncCloudProviderConnectionContractJson);
    }

    public static string LoadBrowserStorageSyncCloudProviderErrorPolicyJson()
    {
        return ReadBufferString(ActriumCoreBrowserStorageSyncCloudProviderErrorPolicyJson);
    }

    public static string BuildBrowserStorageSyncCloudProviderAdapterPlanJson(
        string adapterSnapshotJson,
        long observedAtUnixSeconds)
    {
        var safeAdapterSnapshotJson = adapterSnapshotJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudProviderAdapterPlanJson(
            safeAdapterSnapshotJson,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudProviderAdapterPlanJson(
            safeAdapterSnapshotJson,
            observedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudStoreCapabilityPreflightJson(
        string capabilitiesJson,
        long observedAtUnixSeconds)
    {
        var safeCapabilitiesJson = capabilitiesJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudStoreCapabilityPreflightJson(
            safeCapabilitiesJson,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudStoreCapabilityPreflightJson(
            safeCapabilitiesJson,
            observedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudProviderConnectionStatusPolicyJson(
        string providerSnapshotJson,
        uint retryAttempt,
        long observedAtUnixSeconds)
    {
        var safeProviderSnapshotJson = providerSnapshotJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudProviderConnectionStatusPolicyJson(
            safeProviderSnapshotJson,
            retryAttempt,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudProviderConnectionStatusPolicyJson(
            safeProviderSnapshotJson,
            retryAttempt,
            observedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string NormalizeBrowserSyncProviderSettingsJson(
        string settingsJson,
        bool isRootAbsolute,
        long observedAtUnixSeconds,
        string freshDeviceId)
    {
        var safeSettingsJson = settingsJson ?? string.Empty;
        var safeFreshDeviceId = freshDeviceId ?? string.Empty;
        var required = ActriumCoreNormalizeBrowserSyncProviderSettingsJson(
            safeSettingsJson,
            isRootAbsolute,
            observedAtUnixSeconds,
            safeFreshDeviceId,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreNormalizeBrowserSyncProviderSettingsJson(
            safeSettingsJson,
            isRootAbsolute,
            observedAtUnixSeconds,
            safeFreshDeviceId,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudProviderErrorClassificationJson(
        string providerErrorsJson,
        uint retryAttempt,
        long observedAtUnixSeconds)
    {
        var safeProviderErrorsJson = providerErrorsJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudProviderErrorClassificationJson(
            safeProviderErrorsJson,
            retryAttempt,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudProviderErrorClassificationJson(
            safeProviderErrorsJson,
            retryAttempt,
            observedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string LoadBrowserStorageSyncCloudLocalFolderCapabilitiesJson()
    {
        return ReadBufferString(ActriumCoreBrowserStorageSyncCloudLocalFolderCapabilitiesJson);
    }

    public static string BuildBrowserStorageSyncCloudLocalFolderCapabilityPreflightJson(
        long observedAtUnixSeconds)
    {
        var required = ActriumCoreBrowserStorageSyncCloudLocalFolderCapabilityPreflightJson(
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudLocalFolderCapabilityPreflightJson(
            observedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudLocalFolderCompatibilityPreflightJson(
        string rootDir,
        long observedAtUnixSeconds)
    {
        var safeRootDir = rootDir ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudLocalFolderCompatibilityPreflightJson(
            safeRootDir,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudLocalFolderCompatibilityPreflightJson(
            safeRootDir,
            observedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string LoadBrowserStorageSyncCloudDataModelJson()
    {
        return ReadBufferString(ActriumCoreBrowserStorageSyncCloudDataModelJson);
    }

    public static string BuildBrowserStorageSyncCloudCompatibilityStatusJson(
        string documentKind,
        string documentJson,
        long observedAtUnixSeconds)
    {
        var safeDocumentKind = documentKind ?? string.Empty;
        var safeDocumentJson = documentJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudCompatibilityStatusJson(
            safeDocumentKind,
            safeDocumentJson,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudCompatibilityStatusJson(
            safeDocumentKind,
            safeDocumentJson,
            observedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudCompatibilityPreflightJson(
        string documentsJson,
        long observedAtUnixSeconds)
    {
        var safeDocumentsJson = documentsJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudCompatibilityPreflightJson(
            safeDocumentsJson,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudCompatibilityPreflightJson(
            safeDocumentsJson,
            observedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudSchemaRolloutPlanJson(
        string compatibilityPreflightJson,
        long observedAtUnixSeconds)
    {
        var safeCompatibilityPreflightJson = compatibilityPreflightJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudSchemaRolloutPlanJson(
            safeCompatibilityPreflightJson,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudSchemaRolloutPlanJson(
            safeCompatibilityPreflightJson,
            observedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudSchemaMigrationPlaybookJson(
        string schemaRolloutPlanJson,
        long observedAtUnixSeconds)
    {
        var safeSchemaRolloutPlanJson = schemaRolloutPlanJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudSchemaMigrationPlaybookJson(
            safeSchemaRolloutPlanJson,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudSchemaMigrationPlaybookJson(
            safeSchemaRolloutPlanJson,
            observedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudRecoverySnapshotPlanJson(
        string schemaMigrationPlaybookJson,
        string compatibilityPreflightJson,
        long observedAtUnixSeconds)
    {
        var safeSchemaMigrationPlaybookJson = schemaMigrationPlaybookJson ?? string.Empty;
        var safeCompatibilityPreflightJson = compatibilityPreflightJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudRecoverySnapshotPlanJson(
            safeSchemaMigrationPlaybookJson,
            safeCompatibilityPreflightJson,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudRecoverySnapshotPlanJson(
            safeSchemaMigrationPlaybookJson,
            safeCompatibilityPreflightJson,
            observedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudRecoverySnapshotResultPlanJson(
        string recoverySnapshotPlanJson,
        string writeResultsJson,
        long generatedAtUnixSeconds)
    {
        var safeRecoverySnapshotPlanJson = recoverySnapshotPlanJson ?? string.Empty;
        var safeWriteResultsJson = writeResultsJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudRecoverySnapshotResultPlanJson(
            safeRecoverySnapshotPlanJson,
            safeWriteResultsJson,
            generatedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudRecoverySnapshotResultPlanJson(
            safeRecoverySnapshotPlanJson,
            safeWriteResultsJson,
            generatedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudLocalFolderRecoverySnapshotRoundJson(
        string? rootDir,
        string schemaMigrationPlaybookJson,
        string compatibilityPreflightJson,
        long observedAtUnixSeconds)
    {
        var safeRootDir = rootDir ?? string.Empty;
        var safeSchemaMigrationPlaybookJson = schemaMigrationPlaybookJson ?? string.Empty;
        var safeCompatibilityPreflightJson = compatibilityPreflightJson ?? string.Empty;
        return ReadMutatingRoundBufferString((buffer, bufferLen) =>
            ActriumCoreBrowserStorageSyncCloudLocalFolderRecoverySnapshotRoundJson(
                safeRootDir,
                safeSchemaMigrationPlaybookJson,
                safeCompatibilityPreflightJson,
                observedAtUnixSeconds,
                buffer,
                bufferLen));
    }

    public static string BuildBrowserStorageSyncCloudDefaultManifestJson(
        string? scope,
        string? deviceId,
        long generatedAtUnixSeconds)
    {
        var required = ActriumCoreBrowserStorageSyncCloudDefaultManifestJson(
            scope,
            deviceId,
            generatedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudDefaultManifestJson(
            scope,
            deviceId,
            generatedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string NormalizeBrowserStorageSyncCloudManifestJson(
        string manifestJson,
        string? fallbackScope,
        string? fallbackDeviceId,
        long observedAtUnixSeconds)
    {
        var safeManifestJson = manifestJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudManifestNormalizeJson(
            safeManifestJson,
            fallbackScope,
            fallbackDeviceId,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudManifestNormalizeJson(
            safeManifestJson,
            fallbackScope,
            fallbackDeviceId,
            observedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string NormalizeBrowserStorageSyncCloudDeviceJson(
        string deviceJson,
        string? fallbackScope,
        string? fallbackDeviceId,
        long observedAtUnixSeconds)
    {
        var safeDeviceJson = deviceJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudDeviceNormalizeJson(
            safeDeviceJson,
            fallbackScope,
            fallbackDeviceId,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudDeviceNormalizeJson(
            safeDeviceJson,
            fallbackScope,
            fallbackDeviceId,
            observedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudLogObjectJson(
        string collection,
        long sequence,
        string batchJson)
    {
        var safeCollection = collection ?? string.Empty;
        var safeBatchJson = batchJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudLogObjectJson(
            safeCollection,
            sequence,
            safeBatchJson,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudLogObjectJson(
            safeCollection,
            sequence,
            safeBatchJson,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string NormalizeBrowserStorageSyncCloudLogObjectJson(string logJson)
    {
        var safeLogJson = logJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudLogObjectNormalizeJson(
            safeLogJson,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudLogObjectNormalizeJson(
            safeLogJson,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudSnapshotObjectJson(
        string collection,
        long generatedAtUnixSeconds,
        string batchJson)
    {
        var safeCollection = collection ?? string.Empty;
        var safeBatchJson = batchJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudSnapshotObjectJson(
            safeCollection,
            generatedAtUnixSeconds,
            safeBatchJson,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudSnapshotObjectJson(
            safeCollection,
            generatedAtUnixSeconds,
            safeBatchJson,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string NormalizeBrowserStorageSyncCloudSnapshotObjectJson(string snapshotJson)
    {
        var safeSnapshotJson = snapshotJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudSnapshotObjectNormalizeJson(
            safeSnapshotJson,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudSnapshotObjectNormalizeJson(
            safeSnapshotJson,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudMembershipPlanJson(string requestJson)
    {
        var safeRequestJson = requestJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudMembershipPlanJson(
            safeRequestJson,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudMembershipPlanJson(
            safeRequestJson,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudMembershipResultPlanJson(
        string membershipPlanJson,
        string writeResultsJson,
        long generatedAtUnixSeconds)
    {
        var safeMembershipPlanJson = membershipPlanJson ?? string.Empty;
        var safeWriteResultsJson = writeResultsJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudMembershipResultPlanJson(
            safeMembershipPlanJson,
            safeWriteResultsJson,
            generatedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudMembershipResultPlanJson(
            safeMembershipPlanJson,
            safeWriteResultsJson,
            generatedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string RunBrowserStorageSyncCloudLocalFolderMembershipRoundJson(
        string rootDir,
        string requestJson)
    {
        var safeRootDir = rootDir ?? string.Empty;
        var safeRequestJson = requestJson ?? string.Empty;
        return ReadMutatingRoundBufferString((buffer, bufferLen) =>
            ActriumCoreBrowserStorageSyncCloudLocalFolderMembershipRoundJson(
                safeRootDir,
                safeRequestJson,
                buffer,
                bufferLen));
    }

    public static string RunBrowserStorageSyncCloudLocalFolderCompactionRoundJson(
        string rootDir,
        string deviceId,
        long generatedAtUnixSeconds)
    {
        var safeRootDir = rootDir ?? string.Empty;
        var safeDeviceId = deviceId ?? string.Empty;
        return ReadMutatingRoundBufferString((buffer, bufferLen) =>
            ActriumCoreBrowserStorageSyncCloudLocalFolderCompactionRoundJson(
                safeRootDir,
                safeDeviceId,
                generatedAtUnixSeconds,
                buffer,
                bufferLen));
    }

    public static string RunBrowserStorageSyncCloudLocalFolderCleanupRoundJson(
        string rootDir,
        string cursorsJson,
        string deviceId,
        long generatedAtUnixSeconds)
    {
        var safeRootDir = rootDir ?? string.Empty;
        var safeCursorsJson = cursorsJson ?? string.Empty;
        var safeDeviceId = deviceId ?? string.Empty;
        return ReadMutatingRoundBufferString((buffer, bufferLen) =>
            ActriumCoreBrowserStorageSyncCloudLocalFolderCleanupRoundJson(
                safeRootDir,
                safeCursorsJson,
                safeDeviceId,
                generatedAtUnixSeconds,
                buffer,
                bufferLen));
    }

    public static string BuildBrowserStorageSyncCloudPushPlanJson(
        string manifestJson,
        string manifestRevision,
        string deviceJson,
        string deviceRevision,
        string pushRequestJson)
    {
        var safeManifestJson = manifestJson ?? string.Empty;
        var safeManifestRevision = manifestRevision ?? string.Empty;
        var safeDeviceJson = deviceJson ?? string.Empty;
        var safeDeviceRevision = deviceRevision ?? string.Empty;
        var safePushRequestJson = pushRequestJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudPushPlanJson(
            safeManifestJson,
            safeManifestRevision,
            safeDeviceJson,
            safeDeviceRevision,
            safePushRequestJson,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudPushPlanJson(
            safeManifestJson,
            safeManifestRevision,
            safeDeviceJson,
            safeDeviceRevision,
            safePushRequestJson,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudPushResultPlanJson(
        string pushPlanJson,
        string writeResultsJson,
        long generatedAtUnixSeconds)
    {
        var safePushPlanJson = pushPlanJson ?? string.Empty;
        var safeWriteResultsJson = writeResultsJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudPushResultPlanJson(
            safePushPlanJson,
            safeWriteResultsJson,
            generatedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudPushResultPlanJson(
            safePushPlanJson,
            safeWriteResultsJson,
            generatedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudPullPlanJson(
        string manifestJson,
        string deviceId,
        string cursorsJson,
        long generatedAtUnixSeconds,
        uint maxLogObjectsPerCollection)
    {
        var safeManifestJson = manifestJson ?? string.Empty;
        var safeDeviceId = deviceId ?? string.Empty;
        var safeCursorsJson = cursorsJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudPullPlanJson(
            safeManifestJson,
            safeDeviceId,
            safeCursorsJson,
            generatedAtUnixSeconds,
            maxLogObjectsPerCollection,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudPullPlanJson(
            safeManifestJson,
            safeDeviceId,
            safeCursorsJson,
            generatedAtUnixSeconds,
            maxLogObjectsPerCollection,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudPullApplyPlanJson(
        string manifestJson,
        string deviceId,
        string cursorsJson,
        string snapshotsJson,
        string logsJson,
        long generatedAtUnixSeconds)
    {
        var safeManifestJson = manifestJson ?? string.Empty;
        var safeDeviceId = deviceId ?? string.Empty;
        var safeCursorsJson = cursorsJson ?? string.Empty;
        var safeSnapshotsJson = snapshotsJson ?? string.Empty;
        var safeLogsJson = logsJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudPullApplyPlanJson(
            safeManifestJson,
            safeDeviceId,
            safeCursorsJson,
            safeSnapshotsJson,
            safeLogsJson,
            generatedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudPullApplyPlanJson(
            safeManifestJson,
            safeDeviceId,
            safeCursorsJson,
            safeSnapshotsJson,
            safeLogsJson,
            generatedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudPullContinuationPlanJson(
        string pullPlanJson,
        string readResultsJson,
        long generatedAtUnixSeconds)
    {
        var safePullPlanJson = pullPlanJson ?? string.Empty;
        var safeReadResultsJson = readResultsJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudPullContinuationPlanJson(
            safePullPlanJson,
            safeReadResultsJson,
            generatedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudPullContinuationPlanJson(
            safePullPlanJson,
            safeReadResultsJson,
            generatedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string LoadBrowserStorageSyncCloudHighWatermarkPolicyJson()
    {
        return ReadBufferString(ActriumCoreBrowserStorageSyncCloudHighWatermarkPolicyJson);
    }

    public static string BuildBrowserStorageSyncCloudHighWatermarkPlanJson(
        string manifestJson,
        string snapshotsJson,
        string logsJson,
        string cursorsJson,
        string deviceId,
        long generatedAtUnixSeconds)
    {
        var safeManifestJson = manifestJson ?? string.Empty;
        var safeSnapshotsJson = snapshotsJson ?? string.Empty;
        var safeLogsJson = logsJson ?? string.Empty;
        var safeCursorsJson = cursorsJson ?? string.Empty;
        var safeDeviceId = deviceId ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudHighWatermarkPlanJson(
            safeManifestJson,
            safeSnapshotsJson,
            safeLogsJson,
            safeCursorsJson,
            safeDeviceId,
            generatedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudHighWatermarkPlanJson(
            safeManifestJson,
            safeSnapshotsJson,
            safeLogsJson,
            safeCursorsJson,
            safeDeviceId,
            generatedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudCompactionPlanJson(
        string manifestJson,
        string manifestRevision,
        string snapshotsJson,
        string logsJson,
        string deviceId,
        long generatedAtUnixSeconds)
    {
        var safeManifestJson = manifestJson ?? string.Empty;
        var safeManifestRevision = manifestRevision ?? string.Empty;
        var safeSnapshotsJson = snapshotsJson ?? string.Empty;
        var safeLogsJson = logsJson ?? string.Empty;
        var safeDeviceId = deviceId ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudCompactionPlanJson(
            safeManifestJson,
            safeManifestRevision,
            safeSnapshotsJson,
            safeLogsJson,
            safeDeviceId,
            generatedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudCompactionPlanJson(
            safeManifestJson,
            safeManifestRevision,
            safeSnapshotsJson,
            safeLogsJson,
            safeDeviceId,
            generatedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudCleanupResultPlanJson(
        string highWatermarkPlanJson,
        string deleteResultsJson,
        long generatedAtUnixSeconds)
    {
        var safeHighWatermarkPlanJson = highWatermarkPlanJson ?? string.Empty;
        var safeDeleteResultsJson = deleteResultsJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudCleanupResultPlanJson(
            safeHighWatermarkPlanJson,
            safeDeleteResultsJson,
            generatedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudCleanupResultPlanJson(
            safeHighWatermarkPlanJson,
            safeDeleteResultsJson,
            generatedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudCompactionResultPlanJson(
        string compactionPlanJson,
        string writeResultsJson,
        string deleteResultsJson,
        long generatedAtUnixSeconds)
    {
        var safeCompactionPlanJson = compactionPlanJson ?? string.Empty;
        var safeWriteResultsJson = writeResultsJson ?? string.Empty;
        var safeDeleteResultsJson = deleteResultsJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudCompactionResultPlanJson(
            safeCompactionPlanJson,
            safeWriteResultsJson,
            safeDeleteResultsJson,
            generatedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudCompactionResultPlanJson(
            safeCompactionPlanJson,
            safeWriteResultsJson,
            safeDeleteResultsJson,
            generatedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudCycleClientPlanJson(string cycleRoundJson)
    {
        var safeCycleRoundJson = cycleRoundJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudCycleClientPlanJson(
            safeCycleRoundJson,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudCycleClientPlanJson(
            safeCycleRoundJson,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudCycleClientApplyResultJson(
        string cycleRoundJson,
        string stepResultsJson,
        string remoteApplyResultJson)
    {
        var safeCycleRoundJson = cycleRoundJson ?? string.Empty;
        var safeStepResultsJson = stepResultsJson ?? string.Empty;
        var safeRemoteApplyResultJson = remoteApplyResultJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudCycleClientApplyResultJson(
            safeCycleRoundJson,
            safeStepResultsJson,
            safeRemoteApplyResultJson,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudCycleClientApplyResultJson(
            safeCycleRoundJson,
            safeStepResultsJson,
            safeRemoteApplyResultJson,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudCycleStatusPolicyJson(
        string cycleRoundJson,
        uint retryAttempt)
    {
        var safeCycleRoundJson = cycleRoundJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudCycleStatusPolicyJson(
            safeCycleRoundJson,
            retryAttempt,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudCycleStatusPolicyJson(
            safeCycleRoundJson,
            retryAttempt,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudMembershipStatusPolicyJson(
        string membershipRoundJson,
        uint retryAttempt)
    {
        var safeMembershipRoundJson = membershipRoundJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudMembershipStatusPolicyJson(
            safeMembershipRoundJson,
            retryAttempt,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudMembershipStatusPolicyJson(
            safeMembershipRoundJson,
            retryAttempt,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudCycleScheduleStateJson(
        string statusPolicyJson,
        long observedAtUnixSeconds)
    {
        var safeStatusPolicyJson = statusPolicyJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudCycleScheduleStateJson(
            safeStatusPolicyJson,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudCycleScheduleStateJson(
            safeStatusPolicyJson,
            observedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudMembershipScheduleStateJson(
        string statusPolicyJson,
        long observedAtUnixSeconds)
    {
        var safeStatusPolicyJson = statusPolicyJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudMembershipScheduleStateJson(
            safeStatusPolicyJson,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudMembershipScheduleStateJson(
            safeStatusPolicyJson,
            observedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudProviderDiagnosticsJson(
        string capabilityPreflightJson,
        string compatibilityPreflightJson,
        string providerErrorClassificationJson,
        string cycleStatusPolicyJson,
        string scheduleStateJson,
        long observedAtUnixSeconds)
    {
        var safeCapabilityPreflightJson = capabilityPreflightJson ?? string.Empty;
        var safeCompatibilityPreflightJson = compatibilityPreflightJson ?? string.Empty;
        var safeProviderErrorClassificationJson = providerErrorClassificationJson ?? string.Empty;
        var safeCycleStatusPolicyJson = cycleStatusPolicyJson ?? string.Empty;
        var safeScheduleStateJson = scheduleStateJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudProviderDiagnosticsJson(
            safeCapabilityPreflightJson,
            safeCompatibilityPreflightJson,
            safeProviderErrorClassificationJson,
            safeCycleStatusPolicyJson,
            safeScheduleStateJson,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudProviderDiagnosticsJson(
            safeCapabilityPreflightJson,
            safeCompatibilityPreflightJson,
            safeProviderErrorClassificationJson,
            safeCycleStatusPolicyJson,
            safeScheduleStateJson,
            observedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudObservabilityEventJson(
        string providerDiagnosticsJson,
        string attemptPlanJson,
        string loopPlanJson,
        long observedAtUnixSeconds)
    {
        var safeProviderDiagnosticsJson = providerDiagnosticsJson ?? string.Empty;
        var safeAttemptPlanJson = attemptPlanJson ?? string.Empty;
        var safeLoopPlanJson = loopPlanJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudObservabilityEventJson(
            safeProviderDiagnosticsJson,
            safeAttemptPlanJson,
            safeLoopPlanJson,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudObservabilityEventJson(
            safeProviderDiagnosticsJson,
            safeAttemptPlanJson,
            safeLoopPlanJson,
            observedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudCycleAttemptPlanJson(
        string cycleRoundJson,
        uint retryAttempt,
        long observedAtUnixSeconds)
    {
        var safeCycleRoundJson = cycleRoundJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudCycleAttemptPlanJson(
            safeCycleRoundJson,
            retryAttempt,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudCycleAttemptPlanJson(
            safeCycleRoundJson,
            retryAttempt,
            observedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudMembershipAttemptPlanJson(
        string membershipRoundJson,
        uint retryAttempt,
        long observedAtUnixSeconds)
    {
        var safeMembershipRoundJson = membershipRoundJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudMembershipAttemptPlanJson(
            safeMembershipRoundJson,
            retryAttempt,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudMembershipAttemptPlanJson(
            safeMembershipRoundJson,
            retryAttempt,
            observedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudMembershipLoopPlanJson(
        string membershipRoundsJson,
        uint retryAttempt,
        long observedAtUnixSeconds,
        uint maxAttempts)
    {
        var safeMembershipRoundsJson = membershipRoundsJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudMembershipLoopPlanJson(
            safeMembershipRoundsJson,
            retryAttempt,
            observedAtUnixSeconds,
            maxAttempts,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudMembershipLoopPlanJson(
            safeMembershipRoundsJson,
            retryAttempt,
            observedAtUnixSeconds,
            maxAttempts,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudCycleLoopPlanJson(
        string cycleRoundsJson,
        uint retryAttempt,
        long observedAtUnixSeconds,
        uint maxAttempts)
    {
        var safeCycleRoundsJson = cycleRoundsJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCloudCycleLoopPlanJson(
            safeCycleRoundsJson,
            retryAttempt,
            observedAtUnixSeconds,
            maxAttempts,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCloudCycleLoopPlanJson(
            safeCycleRoundsJson,
            retryAttempt,
            observedAtUnixSeconds,
            maxAttempts,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudLocalFolderCycleRoundJson(
        string rootDir,
        string pushRequestJson,
        string cursorsJson,
        string deviceId,
        long generatedAtUnixSeconds,
        uint maxLogObjectsPerCollection)
    {
        var safeRootDir = rootDir ?? string.Empty;
        var safePushRequestJson = pushRequestJson ?? string.Empty;
        var safeCursorsJson = cursorsJson ?? string.Empty;
        var safeDeviceId = deviceId ?? string.Empty;
        return ReadMutatingRoundBufferString((buffer, bufferLen) =>
            ActriumCoreBrowserStorageSyncCloudLocalFolderCycleRoundJson(
                safeRootDir,
                safePushRequestJson,
                safeCursorsJson,
                safeDeviceId,
                generatedAtUnixSeconds,
                maxLogObjectsPerCollection,
                buffer,
                bufferLen));
    }

    public static string BuildBrowserStorageSyncCycleRequestPlanJson(
        string baseUrl,
        string accessToken,
        string pushRequestJson,
        string cursorsJson)
    {
        var safeBaseUrl = baseUrl ?? string.Empty;
        var safeAccessToken = accessToken ?? string.Empty;
        var safePushRequestJson = pushRequestJson ?? string.Empty;
        var safeCursorsJson = cursorsJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCycleRequestPlanJson(
            safeBaseUrl,
            safeAccessToken,
            safePushRequestJson,
            safeCursorsJson,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCycleRequestPlanJson(
            safeBaseUrl,
            safeAccessToken,
            safePushRequestJson,
            safeCursorsJson,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCycleTransportResultJson(
        int statusCode,
        string responseJson,
        string errorMessage)
    {
        var safeResponseJson = responseJson ?? string.Empty;
        var safeErrorMessage = errorMessage ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncCycleTransportResultJson(
            statusCode,
            safeResponseJson,
            safeErrorMessage,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncCycleTransportResultJson(
            statusCode,
            safeResponseJson,
            safeErrorMessage,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncStatusPolicyJson(
        string classification,
        bool shouldRetry,
        string clientAction,
        uint retryAttempt)
    {
        var safeClassification = classification ?? string.Empty;
        var safeClientAction = clientAction ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncStatusPolicyJson(
            safeClassification,
            shouldRetry,
            safeClientAction,
            retryAttempt,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncStatusPolicyJson(
            safeClassification,
            shouldRetry,
            safeClientAction,
            retryAttempt,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncScheduleStateJson(
        string statusPolicyJson,
        long observedAtUnixSeconds)
    {
        var safeStatusPolicyJson = statusPolicyJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncScheduleStateJson(
            safeStatusPolicyJson,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncScheduleStateJson(
            safeStatusPolicyJson,
            observedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncSchedulerStatusJson(string schedulerSnapshotJson)
    {
        var safeSchedulerSnapshotJson = schedulerSnapshotJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncSchedulerStatusJson(
            safeSchedulerSnapshotJson,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncSchedulerStatusJson(
            safeSchedulerSnapshotJson,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncAckPlanJson(string ackJson)
    {
        var safeAckJson = ackJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncAckPlanJson(safeAckJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncAckPlanJson(safeAckJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncRemoteApplyPlanJson(string batchJson)
    {
        var safeBatchJson = batchJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncRemoteApplyPlanJson(safeBatchJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncRemoteApplyPlanJson(safeBatchJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncRemoteApplyResultJson(string planJson, string resultJson)
    {
        var safePlanJson = planJson ?? string.Empty;
        var safeResultJson = resultJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncRemoteApplyResultJson(safePlanJson, safeResultJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncRemoteApplyResultJson(safePlanJson, safeResultJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncServerMergePlanJson(string pushRequestJson)
    {
        var safePushRequestJson = pushRequestJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncServerMergePlanJson(safePushRequestJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncServerMergePlanJson(safePushRequestJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncServerMergeResultJson(string planJson, string resultJson)
    {
        var safePlanJson = planJson ?? string.Empty;
        var safeResultJson = resultJson ?? string.Empty;
        var required = ActriumCoreBrowserStorageSyncServerMergeResultJson(safePlanJson, safeResultJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserStorageSyncServerMergeResultJson(safePlanJson, safeResultJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string NormalizeBrowserHistorySnapshotJson(string snapshotJson)
    {
        var safeSnapshotJson = snapshotJson ?? string.Empty;
        var required = ActriumCoreBrowserHistorySnapshotNormalizeJson(safeSnapshotJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserHistorySnapshotNormalizeJson(safeSnapshotJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string NormalizeBrowserBookmarkSnapshotJson(string snapshotJson)
    {
        var safeSnapshotJson = snapshotJson ?? string.Empty;
        var required = ActriumCoreBrowserBookmarkSnapshotNormalizeJson(safeSnapshotJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserBookmarkSnapshotNormalizeJson(safeSnapshotJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string NormalizeBrowserSessionSnapshotJson(string snapshotJson)
    {
        var safeSnapshotJson = snapshotJson ?? string.Empty;
        var required = ActriumCoreBrowserSessionSnapshotNormalizeJson(safeSnapshotJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserSessionSnapshotNormalizeJson(safeSnapshotJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string LoadBrowserSessionHomeSnapshotJson()
    {
        var required = ActriumCoreBrowserSessionHomeSnapshotJson(null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActriumCoreBrowserSessionHomeSnapshotJson(buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    private static string ReadString(IntPtr pointer)
    {
        return pointer == IntPtr.Zero ? string.Empty : Marshal.PtrToStringUTF8(pointer) ?? string.Empty;
    }

    private static string ReadBufferString(Func<byte[]?, UIntPtr, UIntPtr> writeBuffer)
    {
        var required = writeBuffer(null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        writeBuffer(buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    // Local-folder cloud rounds mutate provider state on every native call,
    // so the probe-then-fill pattern above would execute the round twice and
    // the two executions can produce JSON of different lengths, truncating
    // the payload decoded from the probe length. Invoke once with a generous
    // buffer instead; when a payload does not fit, the retry re-runs the
    // round and decodes that run's own complete payload.
    private static string ReadMutatingRoundBufferString(Func<byte[], UIntPtr, UIntPtr> writeBuffer)
    {
        var buffer = new byte[256 * 1024];
        while (true)
        {
            var requiredLength = checked((int)writeBuffer(buffer, (UIntPtr)buffer.Length));
            if (requiredLength < buffer.Length)
            {
                return Encoding.UTF8.GetString(buffer, 0, requiredLength);
            }

            buffer = new byte[checked(requiredLength + 4096)];
        }
    }

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_c_api_version")]
    private static extern uint ActriumCoreCApiVersion();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_product_name")]
    private static extern IntPtr ActriumCoreProductName();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_supported_contract_name")]
    private static extern IntPtr ActriumCoreSupportedContractName();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_supported_contract_version")]
    private static extern uint ActriumCoreSupportedContractVersion();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_schema_version")]
    private static extern uint ActriumCoreSchemaVersion();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_runtime_schema_version")]
    private static extern uint ActriumCoreRuntimeSchemaVersion();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_auth_environment_prod")]
    private static extern IntPtr ActriumCoreAuthEnvironmentProd();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_auth_account_base_url")]
    private static extern IntPtr ActriumCoreAuthAccountBaseUrl(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? environment);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_search_engine_count")]
    private static extern uint ActriumCoreBrowserSearchEngineCount();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_search_engine_default_id")]
    private static extern IntPtr ActriumCoreBrowserSearchEngineDefaultId();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_search_engine_id")]
    private static extern IntPtr ActriumCoreBrowserSearchEngineId(uint index);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_search_engine_display_name")]
    private static extern IntPtr ActriumCoreBrowserSearchEngineDisplayName(uint index);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_search_engine_url_prefix")]
    private static extern IntPtr ActriumCoreBrowserSearchEngineUrlPrefix(uint index);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_search_engine_normalize_id")]
    private static extern IntPtr ActriumCoreBrowserSearchEngineNormalizeId(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? engineId);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_search_url")]
    private static extern UIntPtr ActriumCoreBrowserSearchUrl(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string query,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? engineId,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_data_schema_version")]
    private static extern uint ActriumCoreBrowserDataSchemaVersion();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_schema_version")]
    private static extern uint ActriumCoreBrowserStorageSchemaVersion();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_database_name")]
    private static extern IntPtr ActriumCoreBrowserStorageDatabaseName();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_schema_statements_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSchemaStatementsJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_schema_migration_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSchemaMigrationPlanJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_table_names_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageTableNamesJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_repository_sql_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageRepositorySqlJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_sql_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncSqlJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_data_model_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageDataModelJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_conflict_policy_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncConflictPolicyJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_merge_policy_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncMergePolicyJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_address_canonicalization_policy_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageAddressCanonicalizationPolicyJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_canonicalize_addresses_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageCanonicalizeAddressesJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string addressesJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_bookmark_folder_tree_integrity_policy_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageBookmarkFolderTreeIntegrityPolicyJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_bookmark_folder_tree_integrity_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageBookmarkFolderTreeIntegrityPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string foldersJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_bookmark_restore_placement_policy_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageBookmarkRestorePlacementPolicyJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_bookmark_restore_placement_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageBookmarkRestorePlacementPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string bookmarksJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_bookmark_ownership_policy_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageBookmarkOwnershipPolicyJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_bookmark_ownership_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageBookmarkOwnershipPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string bookmarksJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_history_visit_aggregation_policy_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageHistoryVisitAggregationPolicyJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_history_visit_aggregation_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageHistoryVisitAggregationPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string visitsJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_session_tab_ordering_policy_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSessionTabOrderingPolicyJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_session_tab_ordering_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSessionTabOrderingPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string tabsJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_session_tab_id_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSessionTabIdPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string tabsJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_session_active_tab_consistency_policy_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSessionActiveTabConsistencyPolicyJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_session_active_tab_consistency_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSessionActiveTabConsistencyPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string sessionsJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_retention_pruning_policy_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageRetentionPruningPolicyJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_retention_pruning_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageRetentionPruningPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string recordsJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_batch_normalize_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncBatchNormalizeJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string batchJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_batch_from_dirty_rows_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncBatchFromDirtyRowsJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? scope,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? deviceId,
        long generatedAtUnixSeconds,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string rowsJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_push_request_from_dirty_rows_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncPushRequestFromDirtyRowsJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? scope,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? deviceId,
        long generatedAtUnixSeconds,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string rowsJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_ack_batch_normalize_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncAckBatchNormalizeJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string ackJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cursors_normalize_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCursorsNormalizeJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string cursorsJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_batch_empty_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncBatchEmptyJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? scope,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? deviceId,
        long generatedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_push_request_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncPushRequestJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string batchJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_pull_request_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncPullRequestJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? scope,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? deviceId,
        long generatedAtUnixSeconds,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string cursorsJson,
        uint maxRecordsPerCollection,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cycle_response_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCycleResponseJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string remoteBatchJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string ackJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cycle_response_normalize_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCycleResponseNormalizeJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string responseJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_client_cycle_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncClientCyclePlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string responseJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_client_ack_batch_after_remote_apply_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncClientAckBatchAfterRemoteApplyJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string responseJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string remoteApplyResultJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_client_cycle_apply_result_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncClientCycleApplyResultJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string responseJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string remoteApplyResultJson,
        [MarshalAs(UnmanagedType.I1)] bool ackApplied,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_transport_contract_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncTransportContractJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_store_contract_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudStoreContractJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_provider_adapter_contract_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudProviderAdapterContractJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_provider_connection_contract_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudProviderConnectionContractJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_provider_error_policy_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudProviderErrorPolicyJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_provider_adapter_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudProviderAdapterPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string adapterSnapshotJson,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_store_capability_preflight_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudStoreCapabilityPreflightJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string capabilitiesJson,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_provider_connection_status_policy_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudProviderConnectionStatusPolicyJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string providerSnapshotJson,
        uint retryAttempt,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_provider_error_classification_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudProviderErrorClassificationJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string providerErrorsJson,
        uint retryAttempt,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_normalize_browser_sync_provider_settings_json")]
    private static extern UIntPtr ActriumCoreNormalizeBrowserSyncProviderSettingsJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string settingsJson,
        [MarshalAs(UnmanagedType.I1)] bool isRootAbsolute,
        long observedAtUnixSeconds,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string freshDeviceId,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_local_folder_capabilities_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudLocalFolderCapabilitiesJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_local_folder_capability_preflight_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudLocalFolderCapabilityPreflightJson(
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_local_folder_compatibility_preflight_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudLocalFolderCompatibilityPreflightJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string rootDir,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_data_model_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudDataModelJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_compatibility_status_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudCompatibilityStatusJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string documentKind,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string documentJson,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_compatibility_preflight_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudCompatibilityPreflightJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string documentsJson,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_schema_rollout_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudSchemaRolloutPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string compatibilityPreflightJson,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_schema_migration_playbook_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudSchemaMigrationPlaybookJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string schemaRolloutPlanJson,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_recovery_snapshot_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudRecoverySnapshotPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string schemaMigrationPlaybookJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string compatibilityPreflightJson,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_recovery_snapshot_result_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudRecoverySnapshotResultPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string recoverySnapshotPlanJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string writeResultsJson,
        long generatedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_local_folder_recovery_snapshot_round_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudLocalFolderRecoverySnapshotRoundJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string rootDir,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string schemaMigrationPlaybookJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string compatibilityPreflightJson,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_default_manifest_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudDefaultManifestJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? scope,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? deviceId,
        long generatedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_manifest_normalize_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudManifestNormalizeJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string manifestJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? fallbackScope,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? fallbackDeviceId,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_device_normalize_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudDeviceNormalizeJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string deviceJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? fallbackScope,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? fallbackDeviceId,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_log_object_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudLogObjectJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string collection,
        long sequence,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string batchJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_log_object_normalize_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudLogObjectNormalizeJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string logJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_snapshot_object_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudSnapshotObjectJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string collection,
        long generatedAtUnixSeconds,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string batchJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_snapshot_object_normalize_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudSnapshotObjectNormalizeJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string snapshotJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_membership_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudMembershipPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string requestJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_membership_result_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudMembershipResultPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string membershipPlanJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string writeResultsJson,
        long generatedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_push_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudPushPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string manifestJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string manifestRevision,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string deviceJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string deviceRevision,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string pushRequestJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_push_result_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudPushResultPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string pushPlanJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string writeResultsJson,
        long generatedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_pull_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudPullPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string manifestJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string deviceId,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string cursorsJson,
        long generatedAtUnixSeconds,
        uint maxLogObjectsPerCollection,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_pull_apply_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudPullApplyPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string manifestJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string deviceId,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string cursorsJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string snapshotsJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string logsJson,
        long generatedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_pull_continuation_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudPullContinuationPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string pullPlanJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string readResultsJson,
        long generatedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_high_watermark_policy_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudHighWatermarkPolicyJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_high_watermark_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudHighWatermarkPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string manifestJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string snapshotsJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string logsJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string cursorsJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string deviceId,
        long generatedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_cleanup_result_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudCleanupResultPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string highWatermarkPlanJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string deleteResultsJson,
        long generatedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_compaction_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudCompactionPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string manifestJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string manifestRevision,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string snapshotsJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string logsJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string deviceId,
        long generatedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_compaction_result_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudCompactionResultPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string compactionPlanJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string writeResultsJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string deleteResultsJson,
        long generatedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_cycle_client_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudCycleClientPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string cycleRoundJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_cycle_client_apply_result_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudCycleClientApplyResultJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string cycleRoundJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string stepResultsJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string remoteApplyResultJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_cycle_status_policy_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudCycleStatusPolicyJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string cycleRoundJson,
        uint retryAttempt,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_membership_status_policy_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudMembershipStatusPolicyJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string membershipRoundJson,
        uint retryAttempt,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_cycle_schedule_state_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudCycleScheduleStateJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string statusPolicyJson,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_membership_schedule_state_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudMembershipScheduleStateJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string statusPolicyJson,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_provider_diagnostics_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudProviderDiagnosticsJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string capabilityPreflightJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string compatibilityPreflightJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string providerErrorClassificationJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string cycleStatusPolicyJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string scheduleStateJson,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_observability_event_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudObservabilityEventJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string providerDiagnosticsJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string attemptPlanJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string loopPlanJson,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_cycle_attempt_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudCycleAttemptPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string cycleRoundJson,
        uint retryAttempt,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_membership_attempt_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudMembershipAttemptPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string membershipRoundJson,
        uint retryAttempt,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_membership_loop_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudMembershipLoopPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string membershipRoundsJson,
        uint retryAttempt,
        long observedAtUnixSeconds,
        uint maxAttempts,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_cycle_loop_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudCycleLoopPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string cycleRoundsJson,
        uint retryAttempt,
        long observedAtUnixSeconds,
        uint maxAttempts,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_local_folder_membership_round_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudLocalFolderMembershipRoundJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string rootDir,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string requestJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_local_folder_compaction_round_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudLocalFolderCompactionRoundJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string rootDir,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string deviceId,
        long generatedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_local_folder_cleanup_round_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudLocalFolderCleanupRoundJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string rootDir,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string cursorsJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string deviceId,
        long generatedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cloud_local_folder_cycle_round_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCloudLocalFolderCycleRoundJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string rootDir,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string pushRequestJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string cursorsJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string deviceId,
        long generatedAtUnixSeconds,
        uint maxLogObjectsPerCollection,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cycle_request_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCycleRequestPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string baseUrl,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string accessToken,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string pushRequestJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string cursorsJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_cycle_transport_result_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncCycleTransportResultJson(
        int statusCode,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string responseJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string errorMessage,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_status_policy_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncStatusPolicyJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string classification,
        [MarshalAs(UnmanagedType.I1)] bool shouldRetry,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string clientAction,
        uint retryAttempt,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_schedule_state_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncScheduleStateJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string statusPolicyJson,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_scheduler_status_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncSchedulerStatusJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string schedulerSnapshotJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_ack_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncAckPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string ackJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_remote_apply_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncRemoteApplyPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string batchJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_remote_apply_result_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncRemoteApplyResultJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string planJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string resultJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_server_merge_plan_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncServerMergePlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string pushRequestJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_storage_sync_server_merge_result_json")]
    private static extern UIntPtr ActriumCoreBrowserStorageSyncServerMergeResultJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string planJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string resultJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_history_max_items")]
    private static extern uint ActriumCoreBrowserHistoryMaxItems();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_bookmark_max_items")]
    private static extern uint ActriumCoreBrowserBookmarkMaxItems();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_session_max_tabs")]
    private static extern uint ActriumCoreBrowserSessionMaxTabs();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_data_max_title_length")]
    private static extern uint ActriumCoreBrowserDataMaxTitleLength();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_home_address")]
    private static extern IntPtr ActriumCoreBrowserHomeAddress();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_history_address")]
    private static extern IntPtr ActriumCoreBrowserHistoryAddress();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_bookmarks_address")]
    private static extern IntPtr ActriumCoreBrowserBookmarksAddress();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_history_snapshot_normalize_json")]
    private static extern UIntPtr ActriumCoreBrowserHistorySnapshotNormalizeJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string snapshotJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_bookmark_snapshot_normalize_json")]
    private static extern UIntPtr ActriumCoreBrowserBookmarkSnapshotNormalizeJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string snapshotJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_session_snapshot_normalize_json")]
    private static extern UIntPtr ActriumCoreBrowserSessionSnapshotNormalizeJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string snapshotJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actrium_core_browser_session_home_snapshot_json")]
    private static extern UIntPtr ActriumCoreBrowserSessionHomeSnapshotJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);
}

internal sealed record BrowserDataContractInfo(
    uint SchemaVersion,
    uint MaxHistoryItems,
    uint MaxBookmarkItems,
    uint MaxSessionTabs,
    uint MaxTitleLength,
    string HomeAddress,
    string HistoryAddress,
    string BookmarksAddress);

internal sealed record BrowserStorageSchemaInfo(
    uint SchemaVersion,
    string DatabaseName,
    string SchemaStatementsJson,
    string TableNamesJson,
    string RepositorySqlJson);

internal sealed record ActriumCoreStatus(
    bool IsAvailable,
    string ProductName,
    uint CApiVersion,
    string SupportedContractName,
    uint SupportedContractVersion,
    uint CoreSchemaVersion,
    uint RuntimeSchemaVersion,
    string? UnavailableReason)
{
    public static ActriumCoreStatus Available(
        string productName,
        uint cApiVersion,
        string supportedContractName,
        uint supportedContractVersion,
        uint coreSchemaVersion,
        uint runtimeSchemaVersion)
    {
        return new ActriumCoreStatus(
            IsAvailable: true,
            ProductName: productName,
            CApiVersion: cApiVersion,
            SupportedContractName: supportedContractName,
            SupportedContractVersion: supportedContractVersion,
            CoreSchemaVersion: coreSchemaVersion,
            RuntimeSchemaVersion: runtimeSchemaVersion,
            UnavailableReason: null);
    }

    public static ActriumCoreStatus Unavailable(string reason)
    {
        return new ActriumCoreStatus(
            IsAvailable: false,
            ProductName: "Actrium",
            CApiVersion: 0,
            SupportedContractName: "ask",
            SupportedContractVersion: 0,
            CoreSchemaVersion: 0,
            RuntimeSchemaVersion: 0,
            UnavailableReason: reason);
    }
}
