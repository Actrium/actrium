package io.actrium.app.rust

typealias ActiveTabInfo = uniffi.actrium_core.ActiveTabInfo
typealias ActiveTabRowInfo = uniffi.actrium_core.ActiveTabRowInfo
typealias ActrActorTypeInfo = uniffi.actrium_core.ActrActorTypeInfo
typealias ActrBridgeContractInfo = uniffi.actrium_core.ActrBridgeContractInfo
typealias ActrPackageManifestInfo = uniffi.actrium_core.ActrPackageManifestInfo
typealias PackageIntegrityFilePlanInfo = uniffi.actrium_core.PackageIntegrityFilePlanInfo
typealias PackageIntegrityPlanInfo = uniffi.actrium_core.PackageIntegrityPlanInfo
typealias AppSourceTypeInfo = uniffi.actrium_core.AppSourceTypeInfo
typealias AsrBridgeContractInfo = uniffi.actrium_core.AsrBridgeContractInfo
typealias AsrModelPlanInfo = uniffi.actrium_core.AsrModelPlanInfo
typealias AsrSegmentInfo = uniffi.actrium_core.AsrSegmentInfo
typealias BrowserDataContractInfo = uniffi.actrium_core.BrowserDataContractInfo
typealias BrowserSearchEngineInfo = uniffi.actrium_core.BrowserSearchEngineInfo
typealias BrowserStorageRepositorySqlInfo = uniffi.actrium_core.BrowserStorageRepositorySqlInfo
typealias BrowserStorageSchemaInfo = uniffi.actrium_core.BrowserStorageSchemaInfo
typealias BrowserStorageSyncSqlInfo = uniffi.actrium_core.BrowserStorageSyncSqlInfo
typealias ContractGateInfo = uniffi.actrium_core.ContractGateInfo
typealias ContractGateStatus = uniffi.actrium_core.ContractGateStatus
typealias CoreException = uniffi.actrium_core.CoreException
typealias CoreAppRepositorySqlInfo = uniffi.actrium_core.CoreAppRepositorySqlInfo
typealias CoreBookmarkInfo = uniffi.actrium_core.CoreBookmarkInfo
typealias CoreBookmarkRepositorySqlInfo = uniffi.actrium_core.CoreBookmarkRepositorySqlInfo
typealias CoreBookmarkRowInfo = uniffi.actrium_core.CoreBookmarkRowInfo
typealias CoreHistoryItemInfo = uniffi.actrium_core.CoreHistoryItemInfo
typealias CoreHistoryItemRowInfo = uniffi.actrium_core.CoreHistoryItemRowInfo
typealias CoreHistoryRepositorySqlInfo = uniffi.actrium_core.CoreHistoryRepositorySqlInfo
typealias CoreInstalledAppInfo = uniffi.actrium_core.CoreInstalledAppInfo
typealias CoreInstalledAppRowInfo = uniffi.actrium_core.CoreInstalledAppRowInfo
typealias CoreSqlValueInfo = uniffi.actrium_core.CoreSqlValueInfo
typealias CoreTagInfo = uniffi.actrium_core.CoreTagInfo
typealias CoreTagRepositorySqlInfo = uniffi.actrium_core.CoreTagRepositorySqlInfo
typealias CoreTagRowInfo = uniffi.actrium_core.CoreTagRowInfo
typealias DefaultTagInfo = uniffi.actrium_core.DefaultTagInfo
typealias EmailBridgeContractInfo = uniffi.actrium_core.EmailBridgeContractInfo
typealias RuntimeContainerSizeChangeEventInfo = uniffi.actrium_core.RuntimeContainerSizeChangeEventInfo
typealias RuntimeExtensionPanelVisibleEventInfo = uniffi.actrium_core.RuntimeExtensionPanelVisibleEventInfo
typealias RuntimeRootPropsInfo = uniffi.actrium_core.RuntimeRootPropsInfo
typealias RuntimeSystemRootPropsInfo = uniffi.actrium_core.RuntimeSystemRootPropsInfo
typealias RuntimeSystemTabInfo = uniffi.actrium_core.RuntimeSystemTabInfo
typealias RuntimeTabBookmarkEventInfo = uniffi.actrium_core.RuntimeTabBookmarkEventInfo
typealias RuntimeTabBookmarkStatusInfo = uniffi.actrium_core.RuntimeTabBookmarkStatusInfo
typealias RuntimeTabFullscreenEventInfo = uniffi.actrium_core.RuntimeTabFullscreenEventInfo
typealias RuntimeTabInfo = uniffi.actrium_core.RuntimeTabInfo
typealias RuntimeTabUpdateInfo = uniffi.actrium_core.RuntimeTabUpdateInfo
typealias RuntimeTabUnreadInfo = uniffi.actrium_core.RuntimeTabUnreadInfo
typealias SystemAppInfo = uniffi.actrium_core.SystemAppInfo
typealias UserInfoBridgeContractInfo = uniffi.actrium_core.UserInfoBridgeContractInfo

typealias PasswordStrengthInfo = uniffi.actrium_vault.PasswordStrengthInfo
typealias PasswordStrengthLevel = uniffi.actrium_vault.PasswordStrengthLevel
typealias VaultCategoryInfo = uniffi.actrium_vault.VaultCategoryInfo
typealias VaultCoreException = uniffi.actrium_vault.VaultCoreException
typealias VaultFieldInfo = uniffi.actrium_vault.VaultFieldInfo
typealias VaultFieldTemplateInfo = uniffi.actrium_vault.VaultFieldTemplateInfo

object ActriumCore {
    fun activeTabFromRow(row: ActiveTabRowInfo): ActiveTabInfo = uniffi.actrium_core.activeTabFromRow(row)

    fun activeTabRowsFromTabs(tabs: List<ActiveTabInfo>): List<ActiveTabRowInfo> =
        uniffi.actrium_core.activeTabRowsFromTabs(tabs)

    fun activeTabToRow(
        tab: ActiveTabInfo,
        sortOrder: Int,
    ): ActiveTabRowInfo = uniffi.actrium_core.activeTabToRow(tab, sortOrder)

    fun actrBridgeContract(): ActrBridgeContractInfo = uniffi.actrium_core.actrBridgeContract()

    @Throws(CoreException::class)
    fun parseActrActorType(raw: String): ActrActorTypeInfo = uniffi.actrium_core.parseActrActorType(raw)

    fun appSourceTypeDownloaded(): String = uniffi.actrium_core.appSourceTypeDownloaded()

    @Throws(CoreException::class)
    fun appSourceTypeInfo(raw: String): AppSourceTypeInfo = uniffi.actrium_core.appSourceTypeInfo(raw)

    fun appSourceTypeLocalDev(): String = uniffi.actrium_core.appSourceTypeLocalDev()

    fun browserSearchEngineDefaultId(): String = uniffi.actrium_core.browserSearchEngineDefaultId()

    fun browserSearchEngineInfo(engineId: String?): BrowserSearchEngineInfo =
        uniffi.actrium_core.browserSearchEngineInfo(engineId)

    fun browserSearchEngineNormalizeId(engineId: String?): String =
        uniffi.actrium_core.browserSearchEngineNormalizeId(engineId)

    fun browserSearchEngines(): List<BrowserSearchEngineInfo> =
        uniffi.actrium_core.browserSearchEngines()

    fun browserSearchUrl(query: String, engineId: String?): String =
        uniffi.actrium_core.browserSearchUrl(query, engineId)

    fun browserDataContract(): BrowserDataContractInfo = uniffi.actrium_core.browserDataContract()

    fun browserStorageSchema(): BrowserStorageSchemaInfo = uniffi.actrium_core.browserStorageSchema()

    fun browserStorageSchemaVersion(): UInt = uniffi.actrium_core.browserStorageSchemaVersion()

    fun browserStorageDatabaseName(): String = uniffi.actrium_core.browserStorageDatabaseName()

    fun browserStorageSchemaStatements(): List<String> = uniffi.actrium_core.browserStorageSchemaStatements()

    fun browserStorageSchemaMigrationPlanJson(): String =
        uniffi.actrium_core.browserStorageSchemaMigrationPlanJson()

    fun browserStorageTableNames(): List<String> = uniffi.actrium_core.browserStorageTableNames()

    fun browserStorageRepositorySql(): BrowserStorageRepositorySqlInfo =
        uniffi.actrium_core.browserStorageRepositorySql()

    fun browserStorageSyncSql(): BrowserStorageSyncSqlInfo =
        uniffi.actrium_core.browserStorageSyncSql()

    fun browserStorageDataModelJson(): String =
        uniffi.actrium_core.browserStorageDataModelJson()

    fun browserStorageSyncConflictPolicyJson(): String =
        uniffi.actrium_core.browserStorageSyncConflictPolicyJson()

    fun browserStorageSyncMergePolicyJson(): String =
        uniffi.actrium_core.browserStorageSyncMergePolicyJson()

    fun browserStorageAddressCanonicalizationPolicyJson(): String =
        uniffi.actrium_core.browserStorageAddressCanonicalizationPolicyJson()

    fun browserStorageCanonicalizeAddressesJson(addressesJson: String): String =
        uniffi.actrium_core.browserStorageCanonicalizeAddressesJson(addressesJson)

    fun browserStorageBookmarkFolderTreeIntegrityPolicyJson(): String =
        uniffi.actrium_core.browserStorageBookmarkFolderTreeIntegrityPolicyJson()

    fun browserStorageBookmarkFolderTreeIntegrityPlanJson(foldersJson: String): String =
        uniffi.actrium_core.browserStorageBookmarkFolderTreeIntegrityPlanJson(foldersJson)

    fun browserStorageBookmarkRestorePlacementPolicyJson(): String =
        uniffi.actrium_core.browserStorageBookmarkRestorePlacementPolicyJson()

    fun browserStorageBookmarkRestorePlacementPlanJson(bookmarksJson: String): String =
        uniffi.actrium_core.browserStorageBookmarkRestorePlacementPlanJson(bookmarksJson)

    fun browserStorageBookmarkOwnershipPolicyJson(): String =
        uniffi.actrium_core.browserStorageBookmarkOwnershipPolicyJson()

    fun browserStorageBookmarkOwnershipPlanJson(bookmarksJson: String): String =
        uniffi.actrium_core.browserStorageBookmarkOwnershipPlanJson(bookmarksJson)

    fun browserStorageHistoryVisitAggregationPolicyJson(): String =
        uniffi.actrium_core.browserStorageHistoryVisitAggregationPolicyJson()

    fun browserStorageHistoryVisitAggregationPlanJson(visitsJson: String): String =
        uniffi.actrium_core.browserStorageHistoryVisitAggregationPlanJson(visitsJson)

    fun browserStorageSessionTabOrderingPolicyJson(): String =
        uniffi.actrium_core.browserStorageSessionTabOrderingPolicyJson()

    fun browserStorageSessionTabOrderingPlanJson(tabsJson: String): String =
        uniffi.actrium_core.browserStorageSessionTabOrderingPlanJson(tabsJson)

    fun browserStorageSessionTabIdPlanJson(tabsJson: String): String =
        uniffi.actrium_core.browserStorageSessionTabIdPlanJson(tabsJson)

    fun browserStorageSessionActiveTabConsistencyPolicyJson(): String =
        uniffi.actrium_core.browserStorageSessionActiveTabConsistencyPolicyJson()

    fun browserStorageSessionActiveTabConsistencyPlanJson(sessionsJson: String): String =
        uniffi.actrium_core.browserStorageSessionActiveTabConsistencyPlanJson(sessionsJson)

    fun browserStorageRetentionPruningPolicyJson(): String =
        uniffi.actrium_core.browserStorageRetentionPruningPolicyJson()

    fun browserStorageRetentionPruningPlanJson(recordsJson: String): String =
        uniffi.actrium_core.browserStorageRetentionPruningPlanJson(recordsJson)

    fun normalizeBrowserStorageSyncBatchJson(batchJson: String): String =
        uniffi.actrium_core.normalizeBrowserStorageSyncBatchJson(batchJson)

    fun browserStorageSyncBatchFromDirtyRowsJson(
        scope: String,
        deviceId: String,
        generatedAtUnixSeconds: Long,
        rowsJson: String,
    ): String =
        uniffi.actrium_core.browserStorageSyncBatchFromDirtyRowsJson(
            scope,
            deviceId,
            generatedAtUnixSeconds,
            rowsJson,
        )

    fun browserStorageSyncPushRequestFromDirtyRowsJson(
        scope: String,
        deviceId: String,
        generatedAtUnixSeconds: Long,
        rowsJson: String,
    ): String =
        uniffi.actrium_core.browserStorageSyncPushRequestFromDirtyRowsJson(
            scope,
            deviceId,
            generatedAtUnixSeconds,
            rowsJson,
        )

    fun normalizeBrowserStorageSyncAckBatchJson(ackJson: String): String =
        uniffi.actrium_core.normalizeBrowserStorageSyncAckBatchJson(ackJson)

    fun normalizeBrowserStorageSyncCursorsJson(cursorsJson: String): String =
        uniffi.actrium_core.normalizeBrowserStorageSyncCursorsJson(cursorsJson)

    fun emptyBrowserStorageSyncBatchJson(
        scope: String,
        deviceId: String,
        generatedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.emptyBrowserStorageSyncBatchJson(scope, deviceId, generatedAtUnixSeconds)

    fun browserStorageSyncPushRequestJson(batchJson: String): String =
        uniffi.actrium_core.browserStorageSyncPushRequestJson(batchJson)

    fun browserStorageSyncPullRequestJson(
        scope: String,
        deviceId: String,
        generatedAtUnixSeconds: Long,
        cursorsJson: String,
        maxRecordsPerCollection: UInt,
    ): String =
        uniffi.actrium_core.browserStorageSyncPullRequestJson(
            scope,
            deviceId,
            generatedAtUnixSeconds,
            cursorsJson,
            maxRecordsPerCollection,
        )

    fun browserStorageSyncCycleResponseJson(remoteBatchJson: String, ackJson: String): String =
        uniffi.actrium_core.browserStorageSyncCycleResponseJson(remoteBatchJson, ackJson)

    fun normalizeBrowserStorageSyncCycleResponseJson(responseJson: String): String =
        uniffi.actrium_core.normalizeBrowserStorageSyncCycleResponseJson(responseJson)

    fun browserStorageSyncClientCyclePlanJson(responseJson: String): String =
        uniffi.actrium_core.browserStorageSyncClientCyclePlanJson(responseJson)

    fun browserStorageSyncClientAckBatchAfterRemoteApplyJson(
        responseJson: String,
        remoteApplyResultJson: String,
    ): String =
        uniffi.actrium_core.browserStorageSyncClientAckBatchAfterRemoteApplyJson(
            responseJson,
            remoteApplyResultJson,
        )

    fun browserStorageSyncClientCycleApplyResultJson(
        responseJson: String,
        remoteApplyResultJson: String,
        ackApplied: Boolean,
    ): String =
        uniffi.actrium_core.browserStorageSyncClientCycleApplyResultJson(
            responseJson,
            remoteApplyResultJson,
            ackApplied,
        )

    fun browserStorageSyncTransportContractJson(): String =
        uniffi.actrium_core.browserStorageSyncTransportContractJson()

    fun browserStorageSyncCloudStoreContractJson(): String =
        uniffi.actrium_core.browserStorageSyncCloudStoreContractJson()

    fun browserStorageSyncCloudProviderAdapterContractJson(): String =
        uniffi.actrium_core.browserStorageSyncCloudProviderAdapterContractJson()

    fun browserStorageSyncCloudProviderConnectionContractJson(): String =
        uniffi.actrium_core.browserStorageSyncCloudProviderConnectionContractJson()

    fun browserStorageSyncCloudProviderErrorPolicyJson(): String =
        uniffi.actrium_core.browserStorageSyncCloudProviderErrorPolicyJson()

    fun browserStorageSyncCloudProviderAdapterPlanJson(
        adapterSnapshotJson: String,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudProviderAdapterPlanJson(
            adapterSnapshotJson,
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudStoreCapabilityPreflightJson(
        capabilitiesJson: String,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudStoreCapabilityPreflightJson(
            capabilitiesJson,
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudProviderConnectionStatusPolicyJson(
        providerSnapshotJson: String,
        retryAttempt: UInt,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudProviderConnectionStatusPolicyJson(
            providerSnapshotJson,
            retryAttempt,
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudProviderErrorClassificationJson(
        providerErrorsJson: String,
        retryAttempt: UInt,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudProviderErrorClassificationJson(
            providerErrorsJson,
            retryAttempt,
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudLocalFolderCapabilitiesJson(): String =
        uniffi.actrium_core.browserStorageSyncCloudLocalFolderCapabilitiesJson()

    fun browserStorageSyncCloudLocalFolderCapabilityPreflightJson(
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudLocalFolderCapabilityPreflightJson(
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudLocalFolderCompatibilityPreflightJson(
        rootDir: String,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudLocalFolderCompatibilityPreflightJson(
            rootDir,
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudDataModelJson(): String =
        uniffi.actrium_core.browserStorageSyncCloudDataModelJson()

    fun browserStorageSyncCloudCompatibilityStatusJson(
        documentKind: String,
        documentJson: String,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudCompatibilityStatusJson(
            documentKind,
            documentJson,
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudCompatibilityPreflightJson(
        documentsJson: String,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudCompatibilityPreflightJson(
            documentsJson,
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudSchemaRolloutPlanJson(
        compatibilityPreflightJson: String,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudSchemaRolloutPlanJson(
            compatibilityPreflightJson,
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudSchemaMigrationPlaybookJson(
        schemaRolloutPlanJson: String,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudSchemaMigrationPlaybookJson(
            schemaRolloutPlanJson,
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudRecoverySnapshotPlanJson(
        schemaMigrationPlaybookJson: String,
        compatibilityPreflightJson: String,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudRecoverySnapshotPlanJson(
            schemaMigrationPlaybookJson,
            compatibilityPreflightJson,
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudRecoverySnapshotResultPlanJson(
        recoverySnapshotPlanJson: String,
        writeResultsJson: String,
        generatedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudRecoverySnapshotResultPlanJson(
            recoverySnapshotPlanJson,
            writeResultsJson,
            generatedAtUnixSeconds,
        )

    fun browserStorageSyncCloudLocalFolderRecoverySnapshotRoundJson(
        rootDir: String,
        schemaMigrationPlaybookJson: String,
        compatibilityPreflightJson: String,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudLocalFolderRecoverySnapshotRoundJson(
            rootDir,
            schemaMigrationPlaybookJson,
            compatibilityPreflightJson,
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudDefaultManifestJson(
        scope: String,
        deviceId: String,
        generatedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudDefaultManifestJson(
            scope,
            deviceId,
            generatedAtUnixSeconds,
        )

    fun normalizeBrowserStorageSyncCloudManifestJson(
        manifestJson: String,
        fallbackScope: String,
        fallbackDeviceId: String,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.normalizeBrowserStorageSyncCloudManifestJson(
            manifestJson,
            fallbackScope,
            fallbackDeviceId,
            observedAtUnixSeconds,
        )

    fun normalizeBrowserStorageSyncCloudDeviceJson(
        deviceJson: String,
        fallbackScope: String,
        fallbackDeviceId: String,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.normalizeBrowserStorageSyncCloudDeviceJson(
            deviceJson,
            fallbackScope,
            fallbackDeviceId,
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudLogObjectJson(
        collection: String,
        sequence: Long,
        batchJson: String,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudLogObjectJson(
            collection,
            sequence,
            batchJson,
        )

    fun normalizeBrowserStorageSyncCloudLogObjectJson(logJson: String): String =
        uniffi.actrium_core.normalizeBrowserStorageSyncCloudLogObjectJson(logJson)

    fun browserStorageSyncCloudSnapshotObjectJson(
        collection: String,
        generatedAtUnixSeconds: Long,
        batchJson: String,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudSnapshotObjectJson(
            collection,
            generatedAtUnixSeconds,
            batchJson,
        )

    fun normalizeBrowserStorageSyncCloudSnapshotObjectJson(snapshotJson: String): String =
        uniffi.actrium_core.normalizeBrowserStorageSyncCloudSnapshotObjectJson(snapshotJson)

    fun browserStorageSyncCloudMembershipPlanJson(requestJson: String): String =
        uniffi.actrium_core.browserStorageSyncCloudMembershipPlanJson(requestJson)

    fun browserStorageSyncCloudMembershipResultPlanJson(
        membershipPlanJson: String,
        writeResultsJson: String,
        generatedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudMembershipResultPlanJson(
            membershipPlanJson,
            writeResultsJson,
            generatedAtUnixSeconds,
        )

    fun browserStorageSyncCloudLocalFolderMembershipRoundJson(
        rootDir: String,
        requestJson: String,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudLocalFolderMembershipRoundJson(
            rootDir,
            requestJson,
        )

    fun browserStorageSyncCloudPushPlanJson(
        manifestJson: String,
        manifestRevision: String,
        deviceJson: String,
        deviceRevision: String,
        pushRequestJson: String,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudPushPlanJson(
            manifestJson,
            manifestRevision,
            deviceJson,
            deviceRevision,
            pushRequestJson,
        )

    fun browserStorageSyncCloudPushResultPlanJson(
        pushPlanJson: String,
        writeResultsJson: String,
        generatedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudPushResultPlanJson(
            pushPlanJson,
            writeResultsJson,
            generatedAtUnixSeconds,
        )

    fun browserStorageSyncCloudPullPlanJson(
        manifestJson: String,
        deviceId: String,
        cursorsJson: String,
        generatedAtUnixSeconds: Long,
        maxLogObjectsPerCollection: UInt,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudPullPlanJson(
            manifestJson,
            deviceId,
            cursorsJson,
            generatedAtUnixSeconds,
            maxLogObjectsPerCollection,
        )

    fun browserStorageSyncCloudPullApplyPlanJson(
        manifestJson: String,
        deviceId: String,
        cursorsJson: String,
        snapshotsJson: String,
        logsJson: String,
        generatedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudPullApplyPlanJson(
            manifestJson,
            deviceId,
            cursorsJson,
            snapshotsJson,
            logsJson,
            generatedAtUnixSeconds,
        )

    fun browserStorageSyncCloudPullContinuationPlanJson(
        pullPlanJson: String,
        readResultsJson: String,
        generatedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudPullContinuationPlanJson(
            pullPlanJson,
            readResultsJson,
            generatedAtUnixSeconds,
        )

    fun browserStorageSyncCloudHighWatermarkPolicyJson(): String =
        uniffi.actrium_core.browserStorageSyncCloudHighWatermarkPolicyJson()

    fun browserStorageSyncCloudHighWatermarkPlanJson(
        manifestJson: String,
        snapshotsJson: String,
        logsJson: String,
        cursorsJson: String,
        deviceId: String,
        generatedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudHighWatermarkPlanJson(
            manifestJson,
            snapshotsJson,
            logsJson,
            cursorsJson,
            deviceId,
            generatedAtUnixSeconds,
        )

    fun browserStorageSyncCloudCleanupResultPlanJson(
        highWatermarkPlanJson: String,
        deleteResultsJson: String,
        generatedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudCleanupResultPlanJson(
            highWatermarkPlanJson,
            deleteResultsJson,
            generatedAtUnixSeconds,
        )

    fun browserStorageSyncCloudCompactionPlanJson(
        manifestJson: String,
        manifestRevision: String,
        snapshotsJson: String,
        logsJson: String,
        deviceId: String,
        generatedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudCompactionPlanJson(
            manifestJson,
            manifestRevision,
            snapshotsJson,
            logsJson,
            deviceId,
            generatedAtUnixSeconds,
        )

    fun browserStorageSyncCloudCompactionResultPlanJson(
        compactionPlanJson: String,
        writeResultsJson: String,
        deleteResultsJson: String,
        generatedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudCompactionResultPlanJson(
            compactionPlanJson,
            writeResultsJson,
            deleteResultsJson,
            generatedAtUnixSeconds,
        )

    fun browserStorageSyncCloudCycleClientPlanJson(cycleRoundJson: String): String =
        uniffi.actrium_core.browserStorageSyncCloudCycleClientPlanJson(cycleRoundJson)

    fun browserStorageSyncCloudCycleClientApplyResultJson(
        cycleRoundJson: String,
        stepResultsJson: String,
        remoteApplyResultJson: String,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudCycleClientApplyResultJson(
            cycleRoundJson,
            stepResultsJson,
            remoteApplyResultJson,
        )

    fun browserStorageSyncCloudCycleStatusPolicyJson(
        cycleRoundJson: String,
        retryAttempt: Int,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudCycleStatusPolicyJson(
            cycleRoundJson,
            retryAttempt.coerceAtLeast(0).toUInt(),
        )

    fun browserStorageSyncCloudMembershipStatusPolicyJson(
        membershipRoundJson: String,
        retryAttempt: Int,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudMembershipStatusPolicyJson(
            membershipRoundJson,
            retryAttempt.coerceAtLeast(0).toUInt(),
        )

    fun browserStorageSyncCloudCycleScheduleStateJson(
        statusPolicyJson: String,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudCycleScheduleStateJson(
            statusPolicyJson,
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudMembershipScheduleStateJson(
        statusPolicyJson: String,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudMembershipScheduleStateJson(
            statusPolicyJson,
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudProviderDiagnosticsJson(
        capabilityPreflightJson: String,
        compatibilityPreflightJson: String,
        providerErrorClassificationJson: String,
        cycleStatusPolicyJson: String,
        scheduleStateJson: String,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudProviderDiagnosticsJson(
            capabilityPreflightJson,
            compatibilityPreflightJson,
            providerErrorClassificationJson,
            cycleStatusPolicyJson,
            scheduleStateJson,
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudObservabilityEventJson(
        providerDiagnosticsJson: String,
        attemptPlanJson: String,
        loopPlanJson: String,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudObservabilityEventJson(
            providerDiagnosticsJson,
            attemptPlanJson,
            loopPlanJson,
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudCycleAttemptPlanJson(
        cycleRoundJson: String,
        retryAttempt: Int,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudCycleAttemptPlanJson(
            cycleRoundJson,
            retryAttempt.coerceAtLeast(0).toUInt(),
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudMembershipAttemptPlanJson(
        membershipRoundJson: String,
        retryAttempt: Int,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudMembershipAttemptPlanJson(
            membershipRoundJson,
            retryAttempt.coerceAtLeast(0).toUInt(),
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudMembershipLoopPlanJson(
        membershipRoundsJson: String,
        retryAttempt: Int,
        observedAtUnixSeconds: Long,
        maxAttempts: Int,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudMembershipLoopPlanJson(
            membershipRoundsJson,
            retryAttempt.coerceAtLeast(0).toUInt(),
            observedAtUnixSeconds,
            maxAttempts.coerceAtLeast(0).toUInt(),
        )

    fun browserStorageSyncCloudCycleLoopPlanJson(
        cycleRoundsJson: String,
        retryAttempt: Int,
        observedAtUnixSeconds: Long,
        maxAttempts: Int,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudCycleLoopPlanJson(
            cycleRoundsJson,
            retryAttempt.coerceAtLeast(0).toUInt(),
            observedAtUnixSeconds,
            maxAttempts.coerceAtLeast(0).toUInt(),
        )

    fun browserStorageSyncCloudLocalFolderCycleRoundJson(
        rootDir: String,
        pushRequestJson: String,
        cursorsJson: String,
        deviceId: String,
        generatedAtUnixSeconds: Long,
        maxLogObjectsPerCollection: Int,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudLocalFolderCycleRoundJson(
            rootDir,
            pushRequestJson,
            cursorsJson,
            deviceId,
            generatedAtUnixSeconds,
            maxLogObjectsPerCollection.coerceAtLeast(0).toUInt(),
        )

    fun browserStorageSyncCloudLocalFolderCompactionRoundJson(
        rootDir: String,
        deviceId: String,
        generatedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudLocalFolderCompactionRoundJson(
            rootDir,
            deviceId,
            generatedAtUnixSeconds,
        )

    fun browserStorageSyncCloudLocalFolderCleanupRoundJson(
        rootDir: String,
        cursorsJson: String,
        deviceId: String,
        generatedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncCloudLocalFolderCleanupRoundJson(
            rootDir,
            cursorsJson,
            deviceId,
            generatedAtUnixSeconds,
        )

    fun browserStorageSyncCycleRequestPlanJson(
        baseUrl: String,
        accessToken: String,
        pushRequestJson: String,
        cursorsJson: String,
    ): String =
        uniffi.actrium_core.browserStorageSyncCycleRequestPlanJson(
            baseUrl,
            accessToken,
            pushRequestJson,
            cursorsJson,
        )

    fun browserStorageSyncCycleTransportResultJson(
        statusCode: Int,
        responseJson: String,
        errorMessage: String,
    ): String =
        uniffi.actrium_core.browserStorageSyncCycleTransportResultJson(
            statusCode,
            responseJson,
            errorMessage,
        )

    fun browserStorageSyncStatusPolicyJson(
        classification: String,
        shouldRetry: Boolean,
        clientAction: String,
        retryAttempt: Int,
    ): String =
        uniffi.actrium_core.browserStorageSyncStatusPolicyJson(
            classification,
            shouldRetry,
            clientAction,
            retryAttempt.coerceAtLeast(0).toUInt(),
        )

    fun browserStorageSyncScheduleStateJson(
        statusPolicyJson: String,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actrium_core.browserStorageSyncScheduleStateJson(
            statusPolicyJson,
            observedAtUnixSeconds,
        )

    fun browserStorageSyncSchedulerStatusJson(schedulerSnapshotJson: String): String =
        uniffi.actrium_core.browserStorageSyncSchedulerStatusJson(schedulerSnapshotJson)

    fun browserStorageSyncAckPlanJson(ackJson: String): String =
        uniffi.actrium_core.browserStorageSyncAckPlanJson(ackJson)

    fun browserStorageSyncRemoteApplyPlanJson(batchJson: String): String =
        uniffi.actrium_core.browserStorageSyncRemoteApplyPlanJson(batchJson)

    fun browserStorageSyncRemoteApplyResultJson(planJson: String, resultJson: String): String =
        uniffi.actrium_core.browserStorageSyncRemoteApplyResultJson(planJson, resultJson)

    fun browserStorageSyncServerMergePlanJson(pushRequestJson: String): String =
        uniffi.actrium_core.browserStorageSyncServerMergePlanJson(pushRequestJson)

    fun browserStorageSyncServerMergeResultJson(planJson: String, resultJson: String): String =
        uniffi.actrium_core.browserStorageSyncServerMergeResultJson(planJson, resultJson)

    fun browserHistorySnapshotNormalizeJson(snapshotJson: String): String =
        uniffi.actrium_core.browserHistorySnapshotNormalizeJson(snapshotJson)

    fun browserBookmarkSnapshotNormalizeJson(snapshotJson: String): String =
        uniffi.actrium_core.browserBookmarkSnapshotNormalizeJson(snapshotJson)

    fun browserSessionSnapshotNormalizeJson(snapshotJson: String): String =
        uniffi.actrium_core.browserSessionSnapshotNormalizeJson(snapshotJson)

    fun browserSessionHomeSnapshotJson(): String = uniffi.actrium_core.browserSessionHomeSnapshotJson()

    fun asrModelLanguageDir(language: String): String = uniffi.actrium_core.asrModelLanguageDir(language)

    fun asrModelPlan(language: String): AsrModelPlanInfo = uniffi.actrium_core.asrModelPlan(language)

    fun asrBridgeContract(): AsrBridgeContractInfo = uniffi.actrium_core.asrBridgeContract()

    fun authEnvironmentBeta(): String = uniffi.actrium_core.authEnvironmentBeta()

    fun authEnvironmentDev(): String = uniffi.actrium_core.authEnvironmentDev()

    fun authEnvironmentFromAppUrl(url: String): String = uniffi.actrium_core.authEnvironmentFromAppUrl(url)

    fun authEnvironmentFromUrl(url: String): String = uniffi.actrium_core.authEnvironmentFromUrl(url)

    fun authEnvironmentProd(): String = uniffi.actrium_core.authEnvironmentProd()

    fun authEnvironmentStorageSuffix(environment: String): String =
        uniffi.actrium_core.authEnvironmentStorageSuffix(environment)

    fun authAccountBaseUrl(environment: String): String = uniffi.actrium_core.authAccountBaseUrl(environment)

    fun authResourceUploaderBaseUrl(environment: String): String =
        uniffi.actrium_core.authResourceUploaderBaseUrl(environment)

    fun authAccessControlPath(appId: String, userId: String): String =
        uniffi.actrium_core.authAccessControlPath(appId, userId)

    fun authClientAppPath(appId: String): String = uniffi.actrium_core.authClientAppPath(appId)

    fun authResourceUploadTokenPath(): String = uniffi.actrium_core.authResourceUploadTokenPath()

    fun authAccessControlUrlFromAppUrl(appUrl: String, appId: String, userId: String): String? =
        uniffi.actrium_core.authAccessControlUrlFromAppUrl(appUrl, appId, userId)

    fun authIsAuthorizedCode(code: Int): Boolean = uniffi.actrium_core.authIsAuthorizedCode(code)

    fun authAccessControlStatus(success: Boolean, hasCode: Boolean, code: Int): String =
        uniffi.actrium_core.authAccessControlStatus(success, hasCode, code)

    fun authApiResponseSuccess(code: Int): Boolean = uniffi.actrium_core.authApiResponseSuccess(code)

    fun authIsClientAppNotFoundCode(code: Int): Boolean =
        uniffi.actrium_core.authIsClientAppNotFoundCode(code)

    fun authIsClientAppNotFoundMessage(message: String): Boolean =
        uniffi.actrium_core.authIsClientAppNotFoundMessage(message)

    fun coreSchemaStatements(): List<String> = uniffi.actrium_core.coreSchemaStatements()

    fun coreSchemaTableNames(): List<String> = uniffi.actrium_core.coreSchemaTableNames()

    fun coreSchemaVersion(): UInt = uniffi.actrium_core.coreSchemaVersion()

    fun coreHistoryRepositorySql(): CoreHistoryRepositorySqlInfo =
        uniffi.actrium_core.coreHistoryRepositorySql()

    fun coreHistoryItemFromRow(row: CoreHistoryItemRowInfo): CoreHistoryItemInfo =
        uniffi.actrium_core.coreHistoryItemFromRow(row)

    fun coreHistoryInsertVisitParams(
        id: String,
        title: String,
        appUrl: String?,
        appId: String?,
        visitTimeSeconds: Long,
    ): List<CoreSqlValueInfo> =
        uniffi.actrium_core.coreHistoryInsertVisitParams(
            id,
            title,
            appUrl,
            appId,
            visitTimeSeconds,
        )

    fun coreHistorySearchTitleParam(query: String): String? =
        uniffi.actrium_core.coreHistorySearchTitleParam(query)

    fun coreTagRepositorySql(): CoreTagRepositorySqlInfo =
        uniffi.actrium_core.coreTagRepositorySql()

    fun coreTagFromRow(row: CoreTagRowInfo): CoreTagInfo =
        uniffi.actrium_core.coreTagFromRow(row)

    fun coreTagInsertParams(
        id: String,
        name: String,
        color: String?,
        isSystem: Boolean,
    ): List<CoreSqlValueInfo> =
        uniffi.actrium_core.coreTagInsertParams(id, name, color, isSystem)

    fun coreTagUpdateParams(
        name: String,
        color: String?,
        id: String,
    ): List<CoreSqlValueInfo> =
        uniffi.actrium_core.coreTagUpdateParams(name, color, id)

    fun coreBookmarkRepositorySql(): CoreBookmarkRepositorySqlInfo =
        uniffi.actrium_core.coreBookmarkRepositorySql()

    fun coreBookmarkFromRow(row: CoreBookmarkRowInfo): CoreBookmarkInfo =
        uniffi.actrium_core.coreBookmarkFromRow(row)

    fun coreBookmarkInsertParams(bookmark: CoreBookmarkInfo): List<CoreSqlValueInfo> =
        uniffi.actrium_core.coreBookmarkInsertParams(bookmark)

    fun coreBookmarkSearchParams(query: String): List<String> =
        uniffi.actrium_core.coreBookmarkSearchParams(query)

    fun coreBookmarkUpdateParams(bookmark: CoreBookmarkInfo): List<CoreSqlValueInfo> =
        uniffi.actrium_core.coreBookmarkUpdateParams(bookmark)

    fun coreAppRepositorySql(): CoreAppRepositorySqlInfo =
        uniffi.actrium_core.coreAppRepositorySql()

    fun coreAppSearchParams(query: String): List<String> =
        uniffi.actrium_core.coreAppSearchParams(query)

    fun coreInstalledAppFromRow(row: CoreInstalledAppRowInfo): CoreInstalledAppInfo? =
        uniffi.actrium_core.coreInstalledAppFromRow(row)

    @Throws(CoreException::class)
    fun coreInstalledAppInsertParams(app: CoreInstalledAppInfo): List<CoreSqlValueInfo> =
        uniffi.actrium_core.coreInstalledAppInsertParams(app)

    @Throws(CoreException::class)
    fun coreInstalledAppUpdateParams(app: CoreInstalledAppInfo): List<CoreSqlValueInfo> =
        uniffi.actrium_core.coreInstalledAppUpdateParams(app)

    fun defaultTags(): List<DefaultTagInfo> = uniffi.actrium_core.defaultTags()

    fun defaultSystemTags(): List<DefaultTagInfo> = uniffi.actrium_core.defaultSystemTags()

    fun defaultTagColors(): List<String> = uniffi.actrium_core.defaultTagColors()

    fun defaultRuntimeAppStatus(): String = uniffi.actrium_core.defaultRuntimeAppStatus()

    fun emailBridgeContract(): EmailBridgeContractInfo = uniffi.actrium_core.emailBridgeContract()

    fun evaluateContract(
        name: String?,
        version: UInt,
    ): ContractGateInfo = uniffi.actrium_core.evaluateContract(name, version)

    fun extractAppIdFromUrl(url: String): String? = uniffi.actrium_core.extractAppIdFromUrl(url)

    fun isAssetPath(path: String): Boolean = uniffi.actrium_core.isAssetPath(path)

    @Throws(CoreException::class)
    fun createPackageIntegrityPlan(manifestJson: String): PackageIntegrityPlanInfo? =
        uniffi.actrium_core.createPackageIntegrityPlan(manifestJson)

    fun normalizeIntegrityAlgorithm(raw: String): String? =
        uniffi.actrium_core.normalizeIntegrityAlgorithm(raw)

    fun normalizeIntegrityPath(path: String): String? = uniffi.actrium_core.normalizeIntegrityPath(path)

    fun normalizeSha256Hex(hash: String): String? = uniffi.actrium_core.normalizeSha256Hex(hash)

    fun normalizePath(path: String): String = uniffi.actrium_core.normalizePath(path)

    fun normalizeRuntimeAppStatus(value: String): String? = uniffi.actrium_core.normalizeRuntimeAppStatus(value)

    fun parseVoskPartialText(json: String): String = uniffi.actrium_core.parseVoskPartialText(json)

    fun parseVoskResultText(json: String): String = uniffi.actrium_core.parseVoskResultText(json)

    fun postProcessZhAsrSegments(
        text: String,
        segments: List<AsrSegmentInfo>,
    ): String = uniffi.actrium_core.postProcessZhAsrSegments(text, segments)

    @Throws(CoreException::class)
    fun parseActrPackageManifest(json: String): ActrPackageManifestInfo = uniffi.actrium_core.parseActrPackageManifest(json)

    fun parseAppSourceType(raw: String): String? = uniffi.actrium_core.parseAppSourceType(raw)

    fun parseSystemAppType(raw: String): String? = uniffi.actrium_core.parseSystemAppType(raw)

    fun relativePathFrom(
        absolutePath: String,
        baseDir: String,
    ): String = uniffi.actrium_core.relativePathFrom(absolutePath, baseDir)

    fun runtimeAppStatusAlive(): String = uniffi.actrium_core.runtimeAppStatusAlive()

    fun runtimeAppStatusDead(): String = uniffi.actrium_core.runtimeAppStatusDead()

    fun runtimeAuthContextAppIdKey(): String = uniffi.actrium_core.runtimeAuthContextAppIdKey()

    fun runtimeAuthContextAppUrlKey(): String = uniffi.actrium_core.runtimeAuthContextAppUrlKey()

    fun runtimeAuthContextClientIdKey(): String = uniffi.actrium_core.runtimeAuthContextClientIdKey()

    fun runtimeAuthContextTabIdKey(): String = uniffi.actrium_core.runtimeAuthContextTabIdKey()

    fun runtimeLoginResultCodeKey(): String = uniffi.actrium_core.runtimeLoginResultCodeKey()

    fun runtimeLoginResultErrorKey(): String = uniffi.actrium_core.runtimeLoginResultErrorKey()

    fun runtimeLoginResultEventName(): String = uniffi.actrium_core.runtimeLoginResultEventName()

    fun runtimeLoginResultIdTokenKey(): String = uniffi.actrium_core.runtimeLoginResultIdTokenKey()

    fun runtimeLoginResultMessageKey(): String = uniffi.actrium_core.runtimeLoginResultMessageKey()

    fun runtimeLoginResultTypeKey(): String = uniffi.actrium_core.runtimeLoginResultTypeKey()

    fun runtimeLoginTypeApple(): String = uniffi.actrium_core.runtimeLoginTypeApple()

    fun runtimeLoginTypeGoogle(): String = uniffi.actrium_core.runtimeLoginTypeGoogle()

    fun runtimeLoginTypeWechat(): String = uniffi.actrium_core.runtimeLoginTypeWechat()

    fun runtimeBridgeAppUrlKey(): String = uniffi.actrium_core.runtimeBridgeAppUrlKey()

    fun runtimeBridgeContainerHeightKey(): String = uniffi.actrium_core.runtimeBridgeContainerHeightKey()

    fun runtimeBridgeContainerWidthKey(): String = uniffi.actrium_core.runtimeBridgeContainerWidthKey()

    fun runtimeBridgeIconKey(): String = uniffi.actrium_core.runtimeBridgeIconKey()

    fun runtimeBridgeIsBookmarkedKey(): String = uniffi.actrium_core.runtimeBridgeIsBookmarkedKey()

    fun runtimeBridgeMarkedKey(): String = uniffi.actrium_core.runtimeBridgeMarkedKey()

    fun runtimeBridgeStatusKey(): String = uniffi.actrium_core.runtimeBridgeStatusKey()

    fun runtimeBridgeTabIdKey(): String = uniffi.actrium_core.runtimeBridgeTabIdKey()

    fun runtimeBridgeTitleKey(): String = uniffi.actrium_core.runtimeBridgeTitleKey()

    fun runtimeBridgeUnreadKey(): String = uniffi.actrium_core.runtimeBridgeUnreadKey()

    fun runtimeBridgeVisibleKey(): String = uniffi.actrium_core.runtimeBridgeVisibleKey()

    fun runtimeTabEnteredFullscreenEventName(): String =
        uniffi.actrium_core.runtimeTabEnteredFullscreenEventName()

    fun runtimeTabExitedFullscreenEventName(): String =
        uniffi.actrium_core.runtimeTabExitedFullscreenEventName()

    fun runtimeTabBookmarkChangedEventName(): String =
        uniffi.actrium_core.runtimeTabBookmarkChangedEventName()

    fun runtimeExtensionPanelVisibleChangeEventName(): String =
        uniffi.actrium_core.runtimeExtensionPanelVisibleChangeEventName()

    fun runtimeContainerSizeChangeEventName(): String =
        uniffi.actrium_core.runtimeContainerSizeChangeEventName()

    fun runtimeSearchInputChangedEventName(): String =
        uniffi.actrium_core.runtimeSearchInputChangedEventName()

    fun runtimeContainerSizeChangeEvent(
        containerWidth: Int,
        containerHeight: Int,
    ): RuntimeContainerSizeChangeEventInfo =
        uniffi.actrium_core.runtimeContainerSizeChangeEvent(containerWidth, containerHeight)

    fun runtimeExtensionPanelVisibleEvent(
        tabId: String,
        visible: Boolean,
    ): RuntimeExtensionPanelVisibleEventInfo =
        uniffi.actrium_core.runtimeExtensionPanelVisibleEvent(tabId, visible)

    fun runtimeRootPropsFromActiveTabs(
        tab: ActiveTabInfo,
        tabs: List<ActiveTabInfo>,
        containerHeight: Int,
        containerWidth: Int,
    ): RuntimeRootPropsInfo =
        uniffi.actrium_core.runtimeRootPropsFromActiveTabs(
            tab,
            tabs,
            containerHeight,
            containerWidth,
        )

    fun runtimeRootPropsFromRuntimeTabs(
        tab: RuntimeTabInfo,
        tabs: List<RuntimeTabInfo>,
        isLeftPanelVisible: Boolean,
        isRightPanelVisible: Boolean,
        containerHeight: Int,
        containerWidth: Int,
        appUrl: String?,
    ): RuntimeRootPropsInfo =
        uniffi.actrium_core.runtimeRootPropsFromRuntimeTabs(
            tab,
            tabs,
            isLeftPanelVisible,
            isRightPanelVisible,
            containerHeight,
            containerWidth,
            appUrl,
        )

    fun runtimeRootPropsCurrentTabIdKey(): String =
        uniffi.actrium_core.runtimeRootPropsCurrentTabIdKey()

    fun runtimeRootPropsIsLeftPanelVisibleKey(): String =
        uniffi.actrium_core.runtimeRootPropsIsLeftPanelVisibleKey()

    fun runtimeRootPropsIsRightPanelVisibleKey(): String =
        uniffi.actrium_core.runtimeRootPropsIsRightPanelVisibleKey()

    fun runtimeRootPropsSingleTabModeKey(): String =
        uniffi.actrium_core.runtimeRootPropsSingleTabModeKey()

    fun runtimeRootPropsTabKey(): String = uniffi.actrium_core.runtimeRootPropsTabKey()

    fun runtimeRootPropsTabsKey(): String = uniffi.actrium_core.runtimeRootPropsTabsKey()

    fun runtimeTabFromActiveTab(tab: ActiveTabInfo): RuntimeTabInfo =
        uniffi.actrium_core.runtimeTabFromActiveTab(tab)

    fun runtimeTabBookmarkEvent(
        tabId: String,
        isBookmarked: Boolean,
    ): RuntimeTabBookmarkEventInfo =
        uniffi.actrium_core.runtimeTabBookmarkEvent(tabId, isBookmarked)

    fun runtimeTabBookmarkStatusFromFields(
        tabId: String?,
        marked: Boolean?,
    ): RuntimeTabBookmarkStatusInfo? =
        uniffi.actrium_core.runtimeTabBookmarkStatusFromFields(tabId, marked)

    fun runtimeTabFullscreenEvent(tabId: String): RuntimeTabFullscreenEventInfo =
        uniffi.actrium_core.runtimeTabFullscreenEvent(tabId)

    fun runtimeTabPropsBackgroundColorKey(): String =
        uniffi.actrium_core.runtimeTabPropsBackgroundColorKey()

    fun runtimeTabPropsHtmlContentKey(): String =
        uniffi.actrium_core.runtimeTabPropsHtmlContentKey()

    fun runtimeTabPropsIdKey(): String = uniffi.actrium_core.runtimeTabPropsIdKey()

    fun runtimeTabPropsReloadKeyKey(): String = uniffi.actrium_core.runtimeTabPropsReloadKeyKey()

    fun runtimeTabPropsRightPanelDefaultVisibleKey(): String =
        uniffi.actrium_core.runtimeTabPropsRightPanelDefaultVisibleKey()

    fun runtimeTabPropsUnifiedScriptKey(): String =
        uniffi.actrium_core.runtimeTabPropsUnifiedScriptKey()

    fun runtimeTabUpdateFromFields(
        tabId: String?,
        title: String?,
        icon: String?,
        appUrl: String?,
        status: String?,
    ): RuntimeTabUpdateInfo? =
        uniffi.actrium_core.runtimeTabUpdateFromFields(tabId, title, icon, appUrl, status)

    fun runtimeTabUnreadFromFields(
        tabId: String?,
        unread: Boolean?,
    ): RuntimeTabUnreadInfo? =
        uniffi.actrium_core.runtimeTabUnreadFromFields(tabId, unread)

    @Throws(CoreException::class)
    fun runtimeSystemRootProps(systemAppType: String): RuntimeSystemRootPropsInfo =
        uniffi.actrium_core.runtimeSystemRootProps(systemAppType)

    fun systemAppTypeBookmarks(): String = uniffi.actrium_core.systemAppTypeBookmarks()

    fun systemAppTypeGlobalSearch(): String = uniffi.actrium_core.systemAppTypeGlobalSearch()

    fun systemAppTypeSettings(): String = uniffi.actrium_core.systemAppTypeSettings()

    fun supportedContractName(): String = uniffi.actrium_core.supportedContractName()

    fun supportedContractVersion(): UInt = uniffi.actrium_core.supportedContractVersion()

    @Throws(CoreException::class)
    fun systemAppInfo(raw: String): SystemAppInfo = uniffi.actrium_core.systemAppInfo(raw)

    fun userInfoBridgeContract(): UserInfoBridgeContractInfo =
        uniffi.actrium_core.userInfoBridgeContract()

    @Throws(CoreException::class)
    fun validateActrPackageManifest(json: String) = uniffi.actrium_core.validateActrPackageManifest(json)
}

object ActriumVault {
    @Throws(VaultCoreException::class)
    fun createVerificationHash(
        password: String,
        saltBase64: String,
    ): String = uniffi.actrium_vault.createVaultVerificationHash(password, saltBase64)

    @Throws(VaultCoreException::class)
    fun decryptFields(
        keyBase64: String,
        ciphertextBase64: String,
    ): List<VaultFieldInfo> = uniffi.actrium_vault.decryptVaultFields(keyBase64, ciphertextBase64)

    @Throws(VaultCoreException::class)
    fun decryptString(
        keyBase64: String,
        ciphertextBase64: String,
    ): String = uniffi.actrium_vault.decryptVaultString(keyBase64, ciphertextBase64)

    @Throws(VaultCoreException::class)
    fun deriveKeyBase64(
        password: String,
        saltBase64: String,
    ): String = uniffi.actrium_vault.deriveVaultKeyBase64(password, saltBase64)

    @Throws(VaultCoreException::class)
    fun encryptFields(
        keyBase64: String,
        fields: List<VaultFieldInfo>,
    ): String = uniffi.actrium_vault.encryptVaultFields(keyBase64, fields)

    @Throws(VaultCoreException::class)
    fun encryptString(
        keyBase64: String,
        plaintext: String,
    ): String = uniffi.actrium_vault.encryptVaultString(keyBase64, plaintext)

    fun generateSaltBase64(): String = uniffi.actrium_vault.generateVaultSaltBase64()

    fun parseCategory(raw: String): String? = uniffi.actrium_vault.parseVaultCategory(raw)

    @Throws(VaultCoreException::class)
    fun categoryInfo(raw: String): VaultCategoryInfo = uniffi.actrium_vault.vaultCategoryInfo(raw)

    fun categoryTemplates(): List<VaultCategoryInfo> = uniffi.actrium_vault.vaultCategoryTemplates()

    fun passwordStrength(password: String): PasswordStrengthInfo = uniffi.actrium_vault.vaultPasswordStrength(password)

    @Throws(VaultCoreException::class)
    fun verifyPassword(
        password: String,
        saltBase64: String,
        verificationHashBase64: String,
    ): Boolean = uniffi.actrium_vault.verifyVaultPassword(password, saltBase64, verificationHashBase64)
}
