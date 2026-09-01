using System;
using System.Runtime.InteropServices;
using System.Text;

namespace Actro.Windows;

internal static class ActroCore
{
    private const string LibraryName = "actro_core";

    public static ActroCoreStatus LoadStatus()
    {
        try
        {
            return ActroCoreStatus.Available(
                productName: ReadString(ActroCoreProductName()),
                cApiVersion: ActroCoreCApiVersion(),
                supportedContractName: ReadString(ActroCoreSupportedContractName()),
                supportedContractVersion: ActroCoreSupportedContractVersion(),
                coreSchemaVersion: ActroCoreSchemaVersion(),
                runtimeSchemaVersion: ActroCoreRuntimeSchemaVersion());
        }
        catch (DllNotFoundException error)
        {
            return ActroCoreStatus.Unavailable(error.Message);
        }
        catch (EntryPointNotFoundException error)
        {
            return ActroCoreStatus.Unavailable(error.Message);
        }
        catch (BadImageFormatException error)
        {
            return ActroCoreStatus.Unavailable(error.Message);
        }
    }

    public static IReadOnlyList<BrowserSearchEngineInfo> LoadBrowserSearchEngines()
    {
        var count = ActroCoreBrowserSearchEngineCount();
        var engines = new List<BrowserSearchEngineInfo>(checked((int)count));
        for (uint index = 0; index < count; index++)
        {
            engines.Add(new BrowserSearchEngineInfo(
                ReadString(ActroCoreBrowserSearchEngineId(index)),
                ReadString(ActroCoreBrowserSearchEngineDisplayName(index)),
                ReadString(ActroCoreBrowserSearchEngineUrlPrefix(index))));
        }

        return engines;
    }

    public static string LoadAuthEnvironmentProd()
    {
        return ReadString(ActroCoreAuthEnvironmentProd());
    }

    public static string LoadAuthAccountBaseUrl(string? environment)
    {
        return ReadString(ActroCoreAuthAccountBaseUrl(environment));
    }

    public static string LoadBrowserSearchEngineDefaultId()
    {
        return ReadString(ActroCoreBrowserSearchEngineDefaultId());
    }

    public static string NormalizeBrowserSearchEngineId(string? engineId)
    {
        return ReadString(ActroCoreBrowserSearchEngineNormalizeId(engineId));
    }

    public static string BuildBrowserSearchUrl(string query, string? engineId)
    {
        var safeQuery = query ?? string.Empty;
        var required = ActroCoreBrowserSearchUrl(safeQuery, engineId, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserSearchUrl(safeQuery, engineId, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static BrowserDataContractInfo LoadBrowserDataContract()
    {
        return new BrowserDataContractInfo(
            ActroCoreBrowserDataSchemaVersion(),
            ActroCoreBrowserHistoryMaxItems(),
            ActroCoreBrowserBookmarkMaxItems(),
            ActroCoreBrowserSessionMaxTabs(),
            ActroCoreBrowserDataMaxTitleLength(),
            ReadString(ActroCoreBrowserHomeAddress()),
            ReadString(ActroCoreBrowserHistoryAddress()),
            ReadString(ActroCoreBrowserBookmarksAddress()));
    }

    public static BrowserStorageSchemaInfo LoadBrowserStorageSchema()
    {
        return new BrowserStorageSchemaInfo(
            ActroCoreBrowserStorageSchemaVersion(),
            ReadString(ActroCoreBrowserStorageDatabaseName()),
            ReadBufferString(ActroCoreBrowserStorageSchemaStatementsJson),
            ReadBufferString(ActroCoreBrowserStorageTableNamesJson),
            ReadBufferString(ActroCoreBrowserStorageRepositorySqlJson));
    }

    public static string LoadBrowserStorageSchemaMigrationPlanJson()
    {
        return ReadBufferString(ActroCoreBrowserStorageSchemaMigrationPlanJson);
    }

    public static string LoadBrowserStorageSyncSqlJson()
    {
        return ReadBufferString(ActroCoreBrowserStorageSyncSqlJson);
    }

    public static string LoadBrowserStorageDataModelJson()
    {
        return ReadBufferString(ActroCoreBrowserStorageDataModelJson);
    }

    public static string LoadBrowserStorageSyncConflictPolicyJson()
    {
        return ReadBufferString(ActroCoreBrowserStorageSyncConflictPolicyJson);
    }

    public static string LoadBrowserStorageSyncMergePolicyJson()
    {
        return ReadBufferString(ActroCoreBrowserStorageSyncMergePolicyJson);
    }

    public static string LoadBrowserStorageAddressCanonicalizationPolicyJson()
    {
        return ReadBufferString(ActroCoreBrowserStorageAddressCanonicalizationPolicyJson);
    }

    public static string CanonicalizeBrowserStorageAddressesJson(string addressesJson)
    {
        var safeAddressesJson = addressesJson ?? string.Empty;
        var required = ActroCoreBrowserStorageCanonicalizeAddressesJson(safeAddressesJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageCanonicalizeAddressesJson(safeAddressesJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string LoadBrowserStorageBookmarkFolderTreeIntegrityPolicyJson()
    {
        return ReadBufferString(ActroCoreBrowserStorageBookmarkFolderTreeIntegrityPolicyJson);
    }

    public static string BuildBrowserStorageBookmarkFolderTreeIntegrityPlanJson(string foldersJson)
    {
        var safeFoldersJson = foldersJson ?? string.Empty;
        var required = ActroCoreBrowserStorageBookmarkFolderTreeIntegrityPlanJson(safeFoldersJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageBookmarkFolderTreeIntegrityPlanJson(safeFoldersJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string LoadBrowserStorageBookmarkRestorePlacementPolicyJson()
    {
        return ReadBufferString(ActroCoreBrowserStorageBookmarkRestorePlacementPolicyJson);
    }

    public static string BuildBrowserStorageBookmarkRestorePlacementPlanJson(string bookmarksJson)
    {
        var safeBookmarksJson = bookmarksJson ?? string.Empty;
        var required = ActroCoreBrowserStorageBookmarkRestorePlacementPlanJson(safeBookmarksJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageBookmarkRestorePlacementPlanJson(safeBookmarksJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string LoadBrowserStorageBookmarkOwnershipPolicyJson()
    {
        return ReadBufferString(ActroCoreBrowserStorageBookmarkOwnershipPolicyJson);
    }

    public static string BuildBrowserStorageBookmarkOwnershipPlanJson(string bookmarksJson)
    {
        var safeBookmarksJson = bookmarksJson ?? string.Empty;
        var required = ActroCoreBrowserStorageBookmarkOwnershipPlanJson(safeBookmarksJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageBookmarkOwnershipPlanJson(safeBookmarksJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string LoadBrowserStorageHistoryVisitAggregationPolicyJson()
    {
        return ReadBufferString(ActroCoreBrowserStorageHistoryVisitAggregationPolicyJson);
    }

    public static string BuildBrowserStorageHistoryVisitAggregationPlanJson(string visitsJson)
    {
        var safeVisitsJson = visitsJson ?? string.Empty;
        var required = ActroCoreBrowserStorageHistoryVisitAggregationPlanJson(safeVisitsJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageHistoryVisitAggregationPlanJson(safeVisitsJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string LoadBrowserStorageSessionTabOrderingPolicyJson()
    {
        return ReadBufferString(ActroCoreBrowserStorageSessionTabOrderingPolicyJson);
    }

    public static string BuildBrowserStorageSessionTabOrderingPlanJson(string tabsJson)
    {
        var safeTabsJson = tabsJson ?? string.Empty;
        var required = ActroCoreBrowserStorageSessionTabOrderingPlanJson(safeTabsJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSessionTabOrderingPlanJson(safeTabsJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSessionTabIdPlanJson(string tabsJson)
    {
        var safeTabsJson = tabsJson ?? string.Empty;
        var required = ActroCoreBrowserStorageSessionTabIdPlanJson(safeTabsJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSessionTabIdPlanJson(safeTabsJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string LoadBrowserStorageSessionActiveTabConsistencyPolicyJson()
    {
        return ReadBufferString(ActroCoreBrowserStorageSessionActiveTabConsistencyPolicyJson);
    }

    public static string BuildBrowserStorageSessionActiveTabConsistencyPlanJson(string sessionsJson)
    {
        var safeSessionsJson = sessionsJson ?? string.Empty;
        var required = ActroCoreBrowserStorageSessionActiveTabConsistencyPlanJson(safeSessionsJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSessionActiveTabConsistencyPlanJson(safeSessionsJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string LoadBrowserStorageRetentionPruningPolicyJson()
    {
        return ReadBufferString(ActroCoreBrowserStorageRetentionPruningPolicyJson);
    }

    public static string BuildBrowserStorageRetentionPruningPlanJson(string recordsJson)
    {
        var safeRecordsJson = recordsJson ?? string.Empty;
        var required = ActroCoreBrowserStorageRetentionPruningPlanJson(safeRecordsJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageRetentionPruningPlanJson(safeRecordsJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string NormalizeBrowserStorageSyncBatchJson(string batchJson)
    {
        var safeBatchJson = batchJson ?? string.Empty;
        var required = ActroCoreBrowserStorageSyncBatchNormalizeJson(safeBatchJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncBatchNormalizeJson(safeBatchJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncBatchFromDirtyRowsJson(
        string? scope,
        string? deviceId,
        long generatedAtUnixSeconds,
        string rowsJson)
    {
        var safeRowsJson = rowsJson ?? string.Empty;
        var required = ActroCoreBrowserStorageSyncBatchFromDirtyRowsJson(
            scope,
            deviceId,
            generatedAtUnixSeconds,
            safeRowsJson,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncBatchFromDirtyRowsJson(
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
        var required = ActroCoreBrowserStorageSyncPushRequestFromDirtyRowsJson(
            scope,
            deviceId,
            generatedAtUnixSeconds,
            safeRowsJson,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncPushRequestFromDirtyRowsJson(
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
        var required = ActroCoreBrowserStorageSyncAckBatchNormalizeJson(safeAckJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncAckBatchNormalizeJson(safeAckJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string NormalizeBrowserStorageSyncCursorsJson(string cursorsJson)
    {
        var safeCursorsJson = cursorsJson ?? string.Empty;
        var required = ActroCoreBrowserStorageSyncCursorsNormalizeJson(safeCursorsJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCursorsNormalizeJson(safeCursorsJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string LoadEmptyBrowserStorageSyncBatchJson(string? scope, string? deviceId, long generatedAtUnixSeconds)
    {
        var required = ActroCoreBrowserStorageSyncBatchEmptyJson(scope, deviceId, generatedAtUnixSeconds, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncBatchEmptyJson(scope, deviceId, generatedAtUnixSeconds, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncPushRequestJson(string batchJson)
    {
        var safeBatchJson = batchJson ?? string.Empty;
        var required = ActroCoreBrowserStorageSyncPushRequestJson(safeBatchJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncPushRequestJson(safeBatchJson, buffer, (UIntPtr)buffer.Length);
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
        var required = ActroCoreBrowserStorageSyncPullRequestJson(
            scope,
            deviceId,
            generatedAtUnixSeconds,
            safeCursorsJson,
            maxRecordsPerCollection,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncPullRequestJson(
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
        var required = ActroCoreBrowserStorageSyncCycleResponseJson(safeRemoteBatchJson, safeAckJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCycleResponseJson(safeRemoteBatchJson, safeAckJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string NormalizeBrowserStorageSyncCycleResponseJson(string responseJson)
    {
        var safeResponseJson = responseJson ?? string.Empty;
        var required = ActroCoreBrowserStorageSyncCycleResponseNormalizeJson(safeResponseJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCycleResponseNormalizeJson(safeResponseJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncClientCyclePlanJson(string responseJson)
    {
        var safeResponseJson = responseJson ?? string.Empty;
        var required = ActroCoreBrowserStorageSyncClientCyclePlanJson(safeResponseJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncClientCyclePlanJson(safeResponseJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncClientAckBatchAfterRemoteApplyJson(
        string responseJson,
        string remoteApplyResultJson)
    {
        var safeResponseJson = responseJson ?? string.Empty;
        var safeRemoteApplyResultJson = remoteApplyResultJson ?? string.Empty;
        var required = ActroCoreBrowserStorageSyncClientAckBatchAfterRemoteApplyJson(
            safeResponseJson,
            safeRemoteApplyResultJson,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncClientAckBatchAfterRemoteApplyJson(
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
        var required = ActroCoreBrowserStorageSyncClientCycleApplyResultJson(
            safeResponseJson,
            safeRemoteApplyResultJson,
            ackApplied,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncClientCycleApplyResultJson(
            safeResponseJson,
            safeRemoteApplyResultJson,
            ackApplied,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string LoadBrowserStorageSyncTransportContractJson()
    {
        return ReadBufferString(ActroCoreBrowserStorageSyncTransportContractJson);
    }

    public static string LoadBrowserStorageSyncCloudStoreContractJson()
    {
        return ReadBufferString(ActroCoreBrowserStorageSyncCloudStoreContractJson);
    }

    public static string LoadBrowserStorageSyncCloudProviderAdapterContractJson()
    {
        return ReadBufferString(ActroCoreBrowserStorageSyncCloudProviderAdapterContractJson);
    }

    public static string LoadBrowserStorageSyncCloudProviderConnectionContractJson()
    {
        return ReadBufferString(ActroCoreBrowserStorageSyncCloudProviderConnectionContractJson);
    }

    public static string LoadBrowserStorageSyncCloudProviderErrorPolicyJson()
    {
        return ReadBufferString(ActroCoreBrowserStorageSyncCloudProviderErrorPolicyJson);
    }

    public static string BuildBrowserStorageSyncCloudProviderAdapterPlanJson(
        string adapterSnapshotJson,
        long observedAtUnixSeconds)
    {
        var safeAdapterSnapshotJson = adapterSnapshotJson ?? string.Empty;
        var required = ActroCoreBrowserStorageSyncCloudProviderAdapterPlanJson(
            safeAdapterSnapshotJson,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudProviderAdapterPlanJson(
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
        var required = ActroCoreBrowserStorageSyncCloudStoreCapabilityPreflightJson(
            safeCapabilitiesJson,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudStoreCapabilityPreflightJson(
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
        var required = ActroCoreBrowserStorageSyncCloudProviderConnectionStatusPolicyJson(
            safeProviderSnapshotJson,
            retryAttempt,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudProviderConnectionStatusPolicyJson(
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
        var required = ActroCoreNormalizeBrowserSyncProviderSettingsJson(
            safeSettingsJson,
            isRootAbsolute,
            observedAtUnixSeconds,
            safeFreshDeviceId,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreNormalizeBrowserSyncProviderSettingsJson(
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
        var required = ActroCoreBrowserStorageSyncCloudProviderErrorClassificationJson(
            safeProviderErrorsJson,
            retryAttempt,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudProviderErrorClassificationJson(
            safeProviderErrorsJson,
            retryAttempt,
            observedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string LoadBrowserStorageSyncCloudLocalFolderCapabilitiesJson()
    {
        return ReadBufferString(ActroCoreBrowserStorageSyncCloudLocalFolderCapabilitiesJson);
    }

    public static string BuildBrowserStorageSyncCloudLocalFolderCapabilityPreflightJson(
        long observedAtUnixSeconds)
    {
        var required = ActroCoreBrowserStorageSyncCloudLocalFolderCapabilityPreflightJson(
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudLocalFolderCapabilityPreflightJson(
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
        var required = ActroCoreBrowserStorageSyncCloudLocalFolderCompatibilityPreflightJson(
            safeRootDir,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudLocalFolderCompatibilityPreflightJson(
            safeRootDir,
            observedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string LoadBrowserStorageSyncCloudDataModelJson()
    {
        return ReadBufferString(ActroCoreBrowserStorageSyncCloudDataModelJson);
    }

    public static string BuildBrowserStorageSyncCloudCompatibilityStatusJson(
        string documentKind,
        string documentJson,
        long observedAtUnixSeconds)
    {
        var safeDocumentKind = documentKind ?? string.Empty;
        var safeDocumentJson = documentJson ?? string.Empty;
        var required = ActroCoreBrowserStorageSyncCloudCompatibilityStatusJson(
            safeDocumentKind,
            safeDocumentJson,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudCompatibilityStatusJson(
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
        var required = ActroCoreBrowserStorageSyncCloudCompatibilityPreflightJson(
            safeDocumentsJson,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudCompatibilityPreflightJson(
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
        var required = ActroCoreBrowserStorageSyncCloudSchemaRolloutPlanJson(
            safeCompatibilityPreflightJson,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudSchemaRolloutPlanJson(
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
        var required = ActroCoreBrowserStorageSyncCloudSchemaMigrationPlaybookJson(
            safeSchemaRolloutPlanJson,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudSchemaMigrationPlaybookJson(
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
        var required = ActroCoreBrowserStorageSyncCloudRecoverySnapshotPlanJson(
            safeSchemaMigrationPlaybookJson,
            safeCompatibilityPreflightJson,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudRecoverySnapshotPlanJson(
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
        var required = ActroCoreBrowserStorageSyncCloudRecoverySnapshotResultPlanJson(
            safeRecoverySnapshotPlanJson,
            safeWriteResultsJson,
            generatedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudRecoverySnapshotResultPlanJson(
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
            ActroCoreBrowserStorageSyncCloudLocalFolderRecoverySnapshotRoundJson(
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
        var required = ActroCoreBrowserStorageSyncCloudDefaultManifestJson(
            scope,
            deviceId,
            generatedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudDefaultManifestJson(
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
        var required = ActroCoreBrowserStorageSyncCloudManifestNormalizeJson(
            safeManifestJson,
            fallbackScope,
            fallbackDeviceId,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudManifestNormalizeJson(
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
        var required = ActroCoreBrowserStorageSyncCloudDeviceNormalizeJson(
            safeDeviceJson,
            fallbackScope,
            fallbackDeviceId,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudDeviceNormalizeJson(
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
        var required = ActroCoreBrowserStorageSyncCloudLogObjectJson(
            safeCollection,
            sequence,
            safeBatchJson,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudLogObjectJson(
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
        var required = ActroCoreBrowserStorageSyncCloudLogObjectNormalizeJson(
            safeLogJson,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudLogObjectNormalizeJson(
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
        var required = ActroCoreBrowserStorageSyncCloudSnapshotObjectJson(
            safeCollection,
            generatedAtUnixSeconds,
            safeBatchJson,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudSnapshotObjectJson(
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
        var required = ActroCoreBrowserStorageSyncCloudSnapshotObjectNormalizeJson(
            safeSnapshotJson,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudSnapshotObjectNormalizeJson(
            safeSnapshotJson,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncCloudMembershipPlanJson(string requestJson)
    {
        var safeRequestJson = requestJson ?? string.Empty;
        var required = ActroCoreBrowserStorageSyncCloudMembershipPlanJson(
            safeRequestJson,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudMembershipPlanJson(
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
        var required = ActroCoreBrowserStorageSyncCloudMembershipResultPlanJson(
            safeMembershipPlanJson,
            safeWriteResultsJson,
            generatedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudMembershipResultPlanJson(
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
            ActroCoreBrowserStorageSyncCloudLocalFolderMembershipRoundJson(
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
            ActroCoreBrowserStorageSyncCloudLocalFolderCompactionRoundJson(
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
            ActroCoreBrowserStorageSyncCloudLocalFolderCleanupRoundJson(
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
        var required = ActroCoreBrowserStorageSyncCloudPushPlanJson(
            safeManifestJson,
            safeManifestRevision,
            safeDeviceJson,
            safeDeviceRevision,
            safePushRequestJson,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudPushPlanJson(
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
        var required = ActroCoreBrowserStorageSyncCloudPushResultPlanJson(
            safePushPlanJson,
            safeWriteResultsJson,
            generatedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudPushResultPlanJson(
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
        var required = ActroCoreBrowserStorageSyncCloudPullPlanJson(
            safeManifestJson,
            safeDeviceId,
            safeCursorsJson,
            generatedAtUnixSeconds,
            maxLogObjectsPerCollection,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudPullPlanJson(
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
        var required = ActroCoreBrowserStorageSyncCloudPullApplyPlanJson(
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
        ActroCoreBrowserStorageSyncCloudPullApplyPlanJson(
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
        var required = ActroCoreBrowserStorageSyncCloudPullContinuationPlanJson(
            safePullPlanJson,
            safeReadResultsJson,
            generatedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudPullContinuationPlanJson(
            safePullPlanJson,
            safeReadResultsJson,
            generatedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string LoadBrowserStorageSyncCloudHighWatermarkPolicyJson()
    {
        return ReadBufferString(ActroCoreBrowserStorageSyncCloudHighWatermarkPolicyJson);
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
        var required = ActroCoreBrowserStorageSyncCloudHighWatermarkPlanJson(
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
        ActroCoreBrowserStorageSyncCloudHighWatermarkPlanJson(
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
        var required = ActroCoreBrowserStorageSyncCloudCompactionPlanJson(
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
        ActroCoreBrowserStorageSyncCloudCompactionPlanJson(
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
        var required = ActroCoreBrowserStorageSyncCloudCleanupResultPlanJson(
            safeHighWatermarkPlanJson,
            safeDeleteResultsJson,
            generatedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudCleanupResultPlanJson(
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
        var required = ActroCoreBrowserStorageSyncCloudCompactionResultPlanJson(
            safeCompactionPlanJson,
            safeWriteResultsJson,
            safeDeleteResultsJson,
            generatedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudCompactionResultPlanJson(
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
        var required = ActroCoreBrowserStorageSyncCloudCycleClientPlanJson(
            safeCycleRoundJson,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudCycleClientPlanJson(
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
        var required = ActroCoreBrowserStorageSyncCloudCycleClientApplyResultJson(
            safeCycleRoundJson,
            safeStepResultsJson,
            safeRemoteApplyResultJson,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudCycleClientApplyResultJson(
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
        var required = ActroCoreBrowserStorageSyncCloudCycleStatusPolicyJson(
            safeCycleRoundJson,
            retryAttempt,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudCycleStatusPolicyJson(
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
        var required = ActroCoreBrowserStorageSyncCloudMembershipStatusPolicyJson(
            safeMembershipRoundJson,
            retryAttempt,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudMembershipStatusPolicyJson(
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
        var required = ActroCoreBrowserStorageSyncCloudCycleScheduleStateJson(
            safeStatusPolicyJson,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudCycleScheduleStateJson(
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
        var required = ActroCoreBrowserStorageSyncCloudMembershipScheduleStateJson(
            safeStatusPolicyJson,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudMembershipScheduleStateJson(
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
        var required = ActroCoreBrowserStorageSyncCloudProviderDiagnosticsJson(
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
        ActroCoreBrowserStorageSyncCloudProviderDiagnosticsJson(
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
        var required = ActroCoreBrowserStorageSyncCloudObservabilityEventJson(
            safeProviderDiagnosticsJson,
            safeAttemptPlanJson,
            safeLoopPlanJson,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudObservabilityEventJson(
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
        var required = ActroCoreBrowserStorageSyncCloudCycleAttemptPlanJson(
            safeCycleRoundJson,
            retryAttempt,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudCycleAttemptPlanJson(
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
        var required = ActroCoreBrowserStorageSyncCloudMembershipAttemptPlanJson(
            safeMembershipRoundJson,
            retryAttempt,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudMembershipAttemptPlanJson(
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
        var required = ActroCoreBrowserStorageSyncCloudMembershipLoopPlanJson(
            safeMembershipRoundsJson,
            retryAttempt,
            observedAtUnixSeconds,
            maxAttempts,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudMembershipLoopPlanJson(
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
        var required = ActroCoreBrowserStorageSyncCloudCycleLoopPlanJson(
            safeCycleRoundsJson,
            retryAttempt,
            observedAtUnixSeconds,
            maxAttempts,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCloudCycleLoopPlanJson(
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
            ActroCoreBrowserStorageSyncCloudLocalFolderCycleRoundJson(
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
        var required = ActroCoreBrowserStorageSyncCycleRequestPlanJson(
            safeBaseUrl,
            safeAccessToken,
            safePushRequestJson,
            safeCursorsJson,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCycleRequestPlanJson(
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
        var required = ActroCoreBrowserStorageSyncCycleTransportResultJson(
            statusCode,
            safeResponseJson,
            safeErrorMessage,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncCycleTransportResultJson(
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
        var required = ActroCoreBrowserStorageSyncStatusPolicyJson(
            safeClassification,
            shouldRetry,
            safeClientAction,
            retryAttempt,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncStatusPolicyJson(
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
        var required = ActroCoreBrowserStorageSyncScheduleStateJson(
            safeStatusPolicyJson,
            observedAtUnixSeconds,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncScheduleStateJson(
            safeStatusPolicyJson,
            observedAtUnixSeconds,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncSchedulerStatusJson(string schedulerSnapshotJson)
    {
        var safeSchedulerSnapshotJson = schedulerSnapshotJson ?? string.Empty;
        var required = ActroCoreBrowserStorageSyncSchedulerStatusJson(
            safeSchedulerSnapshotJson,
            null,
            UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncSchedulerStatusJson(
            safeSchedulerSnapshotJson,
            buffer,
            (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncAckPlanJson(string ackJson)
    {
        var safeAckJson = ackJson ?? string.Empty;
        var required = ActroCoreBrowserStorageSyncAckPlanJson(safeAckJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncAckPlanJson(safeAckJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncRemoteApplyPlanJson(string batchJson)
    {
        var safeBatchJson = batchJson ?? string.Empty;
        var required = ActroCoreBrowserStorageSyncRemoteApplyPlanJson(safeBatchJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncRemoteApplyPlanJson(safeBatchJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncRemoteApplyResultJson(string planJson, string resultJson)
    {
        var safePlanJson = planJson ?? string.Empty;
        var safeResultJson = resultJson ?? string.Empty;
        var required = ActroCoreBrowserStorageSyncRemoteApplyResultJson(safePlanJson, safeResultJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncRemoteApplyResultJson(safePlanJson, safeResultJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncServerMergePlanJson(string pushRequestJson)
    {
        var safePushRequestJson = pushRequestJson ?? string.Empty;
        var required = ActroCoreBrowserStorageSyncServerMergePlanJson(safePushRequestJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncServerMergePlanJson(safePushRequestJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string BuildBrowserStorageSyncServerMergeResultJson(string planJson, string resultJson)
    {
        var safePlanJson = planJson ?? string.Empty;
        var safeResultJson = resultJson ?? string.Empty;
        var required = ActroCoreBrowserStorageSyncServerMergeResultJson(safePlanJson, safeResultJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserStorageSyncServerMergeResultJson(safePlanJson, safeResultJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string NormalizeBrowserHistorySnapshotJson(string snapshotJson)
    {
        var safeSnapshotJson = snapshotJson ?? string.Empty;
        var required = ActroCoreBrowserHistorySnapshotNormalizeJson(safeSnapshotJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserHistorySnapshotNormalizeJson(safeSnapshotJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string NormalizeBrowserBookmarkSnapshotJson(string snapshotJson)
    {
        var safeSnapshotJson = snapshotJson ?? string.Empty;
        var required = ActroCoreBrowserBookmarkSnapshotNormalizeJson(safeSnapshotJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserBookmarkSnapshotNormalizeJson(safeSnapshotJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string NormalizeBrowserSessionSnapshotJson(string snapshotJson)
    {
        var safeSnapshotJson = snapshotJson ?? string.Empty;
        var required = ActroCoreBrowserSessionSnapshotNormalizeJson(safeSnapshotJson, null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserSessionSnapshotNormalizeJson(safeSnapshotJson, buffer, (UIntPtr)buffer.Length);
        return Encoding.UTF8.GetString(buffer, 0, requiredLength);
    }

    public static string LoadBrowserSessionHomeSnapshotJson()
    {
        var required = ActroCoreBrowserSessionHomeSnapshotJson(null, UIntPtr.Zero);
        var requiredLength = checked((int)required);
        var buffer = new byte[requiredLength + 1];
        ActroCoreBrowserSessionHomeSnapshotJson(buffer, (UIntPtr)buffer.Length);
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

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_c_api_version")]
    private static extern uint ActroCoreCApiVersion();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_product_name")]
    private static extern IntPtr ActroCoreProductName();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_supported_contract_name")]
    private static extern IntPtr ActroCoreSupportedContractName();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_supported_contract_version")]
    private static extern uint ActroCoreSupportedContractVersion();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_schema_version")]
    private static extern uint ActroCoreSchemaVersion();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_runtime_schema_version")]
    private static extern uint ActroCoreRuntimeSchemaVersion();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_auth_environment_prod")]
    private static extern IntPtr ActroCoreAuthEnvironmentProd();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_auth_account_base_url")]
    private static extern IntPtr ActroCoreAuthAccountBaseUrl(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? environment);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_search_engine_count")]
    private static extern uint ActroCoreBrowserSearchEngineCount();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_search_engine_default_id")]
    private static extern IntPtr ActroCoreBrowserSearchEngineDefaultId();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_search_engine_id")]
    private static extern IntPtr ActroCoreBrowserSearchEngineId(uint index);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_search_engine_display_name")]
    private static extern IntPtr ActroCoreBrowserSearchEngineDisplayName(uint index);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_search_engine_url_prefix")]
    private static extern IntPtr ActroCoreBrowserSearchEngineUrlPrefix(uint index);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_search_engine_normalize_id")]
    private static extern IntPtr ActroCoreBrowserSearchEngineNormalizeId(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? engineId);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_search_url")]
    private static extern UIntPtr ActroCoreBrowserSearchUrl(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string query,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? engineId,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_data_schema_version")]
    private static extern uint ActroCoreBrowserDataSchemaVersion();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_schema_version")]
    private static extern uint ActroCoreBrowserStorageSchemaVersion();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_database_name")]
    private static extern IntPtr ActroCoreBrowserStorageDatabaseName();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_schema_statements_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSchemaStatementsJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_schema_migration_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSchemaMigrationPlanJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_table_names_json")]
    private static extern UIntPtr ActroCoreBrowserStorageTableNamesJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_repository_sql_json")]
    private static extern UIntPtr ActroCoreBrowserStorageRepositorySqlJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_sql_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncSqlJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_data_model_json")]
    private static extern UIntPtr ActroCoreBrowserStorageDataModelJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_conflict_policy_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncConflictPolicyJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_merge_policy_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncMergePolicyJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_address_canonicalization_policy_json")]
    private static extern UIntPtr ActroCoreBrowserStorageAddressCanonicalizationPolicyJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_canonicalize_addresses_json")]
    private static extern UIntPtr ActroCoreBrowserStorageCanonicalizeAddressesJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string addressesJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_bookmark_folder_tree_integrity_policy_json")]
    private static extern UIntPtr ActroCoreBrowserStorageBookmarkFolderTreeIntegrityPolicyJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_bookmark_folder_tree_integrity_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageBookmarkFolderTreeIntegrityPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string foldersJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_bookmark_restore_placement_policy_json")]
    private static extern UIntPtr ActroCoreBrowserStorageBookmarkRestorePlacementPolicyJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_bookmark_restore_placement_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageBookmarkRestorePlacementPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string bookmarksJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_bookmark_ownership_policy_json")]
    private static extern UIntPtr ActroCoreBrowserStorageBookmarkOwnershipPolicyJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_bookmark_ownership_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageBookmarkOwnershipPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string bookmarksJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_history_visit_aggregation_policy_json")]
    private static extern UIntPtr ActroCoreBrowserStorageHistoryVisitAggregationPolicyJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_history_visit_aggregation_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageHistoryVisitAggregationPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string visitsJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_session_tab_ordering_policy_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSessionTabOrderingPolicyJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_session_tab_ordering_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSessionTabOrderingPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string tabsJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_session_tab_id_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSessionTabIdPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string tabsJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_session_active_tab_consistency_policy_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSessionActiveTabConsistencyPolicyJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_session_active_tab_consistency_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSessionActiveTabConsistencyPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string sessionsJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_retention_pruning_policy_json")]
    private static extern UIntPtr ActroCoreBrowserStorageRetentionPruningPolicyJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_retention_pruning_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageRetentionPruningPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string recordsJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_batch_normalize_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncBatchNormalizeJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string batchJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_batch_from_dirty_rows_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncBatchFromDirtyRowsJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? scope,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? deviceId,
        long generatedAtUnixSeconds,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string rowsJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_push_request_from_dirty_rows_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncPushRequestFromDirtyRowsJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? scope,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? deviceId,
        long generatedAtUnixSeconds,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string rowsJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_ack_batch_normalize_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncAckBatchNormalizeJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string ackJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cursors_normalize_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCursorsNormalizeJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string cursorsJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_batch_empty_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncBatchEmptyJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? scope,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? deviceId,
        long generatedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_push_request_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncPushRequestJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string batchJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_pull_request_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncPullRequestJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? scope,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? deviceId,
        long generatedAtUnixSeconds,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string cursorsJson,
        uint maxRecordsPerCollection,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cycle_response_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCycleResponseJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string remoteBatchJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string ackJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cycle_response_normalize_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCycleResponseNormalizeJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string responseJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_client_cycle_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncClientCyclePlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string responseJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_client_ack_batch_after_remote_apply_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncClientAckBatchAfterRemoteApplyJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string responseJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string remoteApplyResultJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_client_cycle_apply_result_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncClientCycleApplyResultJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string responseJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string remoteApplyResultJson,
        [MarshalAs(UnmanagedType.I1)] bool ackApplied,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_transport_contract_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncTransportContractJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_store_contract_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudStoreContractJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_provider_adapter_contract_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudProviderAdapterContractJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_provider_connection_contract_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudProviderConnectionContractJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_provider_error_policy_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudProviderErrorPolicyJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_provider_adapter_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudProviderAdapterPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string adapterSnapshotJson,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_store_capability_preflight_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudStoreCapabilityPreflightJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string capabilitiesJson,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_provider_connection_status_policy_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudProviderConnectionStatusPolicyJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string providerSnapshotJson,
        uint retryAttempt,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_provider_error_classification_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudProviderErrorClassificationJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string providerErrorsJson,
        uint retryAttempt,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_normalize_browser_sync_provider_settings_json")]
    private static extern UIntPtr ActroCoreNormalizeBrowserSyncProviderSettingsJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string settingsJson,
        [MarshalAs(UnmanagedType.I1)] bool isRootAbsolute,
        long observedAtUnixSeconds,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string freshDeviceId,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_local_folder_capabilities_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudLocalFolderCapabilitiesJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_local_folder_capability_preflight_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudLocalFolderCapabilityPreflightJson(
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_local_folder_compatibility_preflight_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudLocalFolderCompatibilityPreflightJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string rootDir,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_data_model_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudDataModelJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_compatibility_status_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudCompatibilityStatusJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string documentKind,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string documentJson,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_compatibility_preflight_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudCompatibilityPreflightJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string documentsJson,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_schema_rollout_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudSchemaRolloutPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string compatibilityPreflightJson,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_schema_migration_playbook_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudSchemaMigrationPlaybookJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string schemaRolloutPlanJson,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_recovery_snapshot_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudRecoverySnapshotPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string schemaMigrationPlaybookJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string compatibilityPreflightJson,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_recovery_snapshot_result_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudRecoverySnapshotResultPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string recoverySnapshotPlanJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string writeResultsJson,
        long generatedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_local_folder_recovery_snapshot_round_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudLocalFolderRecoverySnapshotRoundJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string rootDir,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string schemaMigrationPlaybookJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string compatibilityPreflightJson,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_default_manifest_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudDefaultManifestJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? scope,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? deviceId,
        long generatedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_manifest_normalize_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudManifestNormalizeJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string manifestJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? fallbackScope,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? fallbackDeviceId,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_device_normalize_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudDeviceNormalizeJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string deviceJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? fallbackScope,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? fallbackDeviceId,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_log_object_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudLogObjectJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string collection,
        long sequence,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string batchJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_log_object_normalize_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudLogObjectNormalizeJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string logJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_snapshot_object_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudSnapshotObjectJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string collection,
        long generatedAtUnixSeconds,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string batchJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_snapshot_object_normalize_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudSnapshotObjectNormalizeJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string snapshotJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_membership_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudMembershipPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string requestJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_membership_result_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudMembershipResultPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string membershipPlanJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string writeResultsJson,
        long generatedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_push_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudPushPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string manifestJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string manifestRevision,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string deviceJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string deviceRevision,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string pushRequestJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_push_result_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudPushResultPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string pushPlanJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string writeResultsJson,
        long generatedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_pull_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudPullPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string manifestJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string deviceId,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string cursorsJson,
        long generatedAtUnixSeconds,
        uint maxLogObjectsPerCollection,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_pull_apply_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudPullApplyPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string manifestJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string deviceId,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string cursorsJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string snapshotsJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string logsJson,
        long generatedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_pull_continuation_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudPullContinuationPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string pullPlanJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string readResultsJson,
        long generatedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_high_watermark_policy_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudHighWatermarkPolicyJson(
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_high_watermark_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudHighWatermarkPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string manifestJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string snapshotsJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string logsJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string cursorsJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string deviceId,
        long generatedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_cleanup_result_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudCleanupResultPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string highWatermarkPlanJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string deleteResultsJson,
        long generatedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_compaction_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudCompactionPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string manifestJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string manifestRevision,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string snapshotsJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string logsJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string deviceId,
        long generatedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_compaction_result_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudCompactionResultPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string compactionPlanJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string writeResultsJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string deleteResultsJson,
        long generatedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_cycle_client_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudCycleClientPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string cycleRoundJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_cycle_client_apply_result_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudCycleClientApplyResultJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string cycleRoundJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string stepResultsJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string remoteApplyResultJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_cycle_status_policy_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudCycleStatusPolicyJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string cycleRoundJson,
        uint retryAttempt,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_membership_status_policy_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudMembershipStatusPolicyJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string membershipRoundJson,
        uint retryAttempt,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_cycle_schedule_state_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudCycleScheduleStateJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string statusPolicyJson,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_membership_schedule_state_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudMembershipScheduleStateJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string statusPolicyJson,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_provider_diagnostics_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudProviderDiagnosticsJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string capabilityPreflightJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string compatibilityPreflightJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string providerErrorClassificationJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string cycleStatusPolicyJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string scheduleStateJson,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_observability_event_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudObservabilityEventJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string providerDiagnosticsJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string attemptPlanJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string loopPlanJson,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_cycle_attempt_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudCycleAttemptPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string cycleRoundJson,
        uint retryAttempt,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_membership_attempt_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudMembershipAttemptPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string membershipRoundJson,
        uint retryAttempt,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_membership_loop_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudMembershipLoopPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string membershipRoundsJson,
        uint retryAttempt,
        long observedAtUnixSeconds,
        uint maxAttempts,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_cycle_loop_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudCycleLoopPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string cycleRoundsJson,
        uint retryAttempt,
        long observedAtUnixSeconds,
        uint maxAttempts,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_local_folder_membership_round_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudLocalFolderMembershipRoundJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string rootDir,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string requestJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_local_folder_compaction_round_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudLocalFolderCompactionRoundJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string rootDir,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string deviceId,
        long generatedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_local_folder_cleanup_round_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudLocalFolderCleanupRoundJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string rootDir,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string cursorsJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string deviceId,
        long generatedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cloud_local_folder_cycle_round_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCloudLocalFolderCycleRoundJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string rootDir,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string pushRequestJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string cursorsJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string deviceId,
        long generatedAtUnixSeconds,
        uint maxLogObjectsPerCollection,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cycle_request_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCycleRequestPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string baseUrl,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string accessToken,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string pushRequestJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string cursorsJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_cycle_transport_result_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncCycleTransportResultJson(
        int statusCode,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string responseJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string errorMessage,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_status_policy_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncStatusPolicyJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string classification,
        [MarshalAs(UnmanagedType.I1)] bool shouldRetry,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string clientAction,
        uint retryAttempt,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_schedule_state_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncScheduleStateJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string statusPolicyJson,
        long observedAtUnixSeconds,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_scheduler_status_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncSchedulerStatusJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string schedulerSnapshotJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_ack_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncAckPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string ackJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_remote_apply_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncRemoteApplyPlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string batchJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_remote_apply_result_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncRemoteApplyResultJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string planJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string resultJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_server_merge_plan_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncServerMergePlanJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string pushRequestJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_storage_sync_server_merge_result_json")]
    private static extern UIntPtr ActroCoreBrowserStorageSyncServerMergeResultJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string planJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string resultJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_history_max_items")]
    private static extern uint ActroCoreBrowserHistoryMaxItems();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_bookmark_max_items")]
    private static extern uint ActroCoreBrowserBookmarkMaxItems();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_session_max_tabs")]
    private static extern uint ActroCoreBrowserSessionMaxTabs();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_data_max_title_length")]
    private static extern uint ActroCoreBrowserDataMaxTitleLength();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_home_address")]
    private static extern IntPtr ActroCoreBrowserHomeAddress();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_history_address")]
    private static extern IntPtr ActroCoreBrowserHistoryAddress();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_bookmarks_address")]
    private static extern IntPtr ActroCoreBrowserBookmarksAddress();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_history_snapshot_normalize_json")]
    private static extern UIntPtr ActroCoreBrowserHistorySnapshotNormalizeJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string snapshotJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_bookmark_snapshot_normalize_json")]
    private static extern UIntPtr ActroCoreBrowserBookmarkSnapshotNormalizeJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string snapshotJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_session_snapshot_normalize_json")]
    private static extern UIntPtr ActroCoreBrowserSessionSnapshotNormalizeJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string snapshotJson,
        [Out] byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "actro_core_browser_session_home_snapshot_json")]
    private static extern UIntPtr ActroCoreBrowserSessionHomeSnapshotJson(
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

internal sealed record ActroCoreStatus(
    bool IsAvailable,
    string ProductName,
    uint CApiVersion,
    string SupportedContractName,
    uint SupportedContractVersion,
    uint CoreSchemaVersion,
    uint RuntimeSchemaVersion,
    string? UnavailableReason)
{
    public static ActroCoreStatus Available(
        string productName,
        uint cApiVersion,
        string supportedContractName,
        uint supportedContractVersion,
        uint coreSchemaVersion,
        uint runtimeSchemaVersion)
    {
        return new ActroCoreStatus(
            IsAvailable: true,
            ProductName: productName,
            CApiVersion: cApiVersion,
            SupportedContractName: supportedContractName,
            SupportedContractVersion: supportedContractVersion,
            CoreSchemaVersion: coreSchemaVersion,
            RuntimeSchemaVersion: runtimeSchemaVersion,
            UnavailableReason: null);
    }

    public static ActroCoreStatus Unavailable(string reason)
    {
        return new ActroCoreStatus(
            IsAvailable: false,
            ProductName: "Actro",
            CApiVersion: 0,
            SupportedContractName: "ask",
            SupportedContractVersion: 0,
            CoreSchemaVersion: 0,
            RuntimeSchemaVersion: 0,
            UnavailableReason: reason);
    }
}
