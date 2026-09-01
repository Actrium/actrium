package io.actrium.actro.rust

typealias ActiveTabInfo = uniffi.actro_core.ActiveTabInfo
typealias ActiveTabRowInfo = uniffi.actro_core.ActiveTabRowInfo
typealias ActrActorTypeInfo = uniffi.actro_core.ActrActorTypeInfo
typealias ActrBridgeContractInfo = uniffi.actro_core.ActrBridgeContractInfo
typealias ActrManifestInfo = uniffi.actro_core.ActrManifestInfo
typealias ActrIntegrityFilePlanInfo = uniffi.actro_core.ActrIntegrityFilePlanInfo
typealias ActrIntegrityPlanInfo = uniffi.actro_core.ActrIntegrityPlanInfo
typealias AppSourceTypeInfo = uniffi.actro_core.AppSourceTypeInfo
typealias AsrBridgeContractInfo = uniffi.actro_core.AsrBridgeContractInfo
typealias AsrModelPlanInfo = uniffi.actro_core.AsrModelPlanInfo
typealias AsrSegmentInfo = uniffi.actro_core.AsrSegmentInfo
typealias BrowserDataContractInfo = uniffi.actro_core.BrowserDataContractInfo
typealias BrowserSearchEngineInfo = uniffi.actro_core.BrowserSearchEngineInfo
typealias BrowserStorageRepositorySqlInfo = uniffi.actro_core.BrowserStorageRepositorySqlInfo
typealias BrowserStorageSchemaInfo = uniffi.actro_core.BrowserStorageSchemaInfo
typealias BrowserStorageSyncSqlInfo = uniffi.actro_core.BrowserStorageSyncSqlInfo
typealias ContractGateInfo = uniffi.actro_core.ContractGateInfo
typealias ContractGateStatus = uniffi.actro_core.ContractGateStatus
typealias CoreException = uniffi.actro_core.CoreException
typealias CoreAppRepositorySqlInfo = uniffi.actro_core.CoreAppRepositorySqlInfo
typealias CoreBookmarkInfo = uniffi.actro_core.CoreBookmarkInfo
typealias CoreBookmarkRepositorySqlInfo = uniffi.actro_core.CoreBookmarkRepositorySqlInfo
typealias CoreBookmarkRowInfo = uniffi.actro_core.CoreBookmarkRowInfo
typealias CoreHistoryItemInfo = uniffi.actro_core.CoreHistoryItemInfo
typealias CoreHistoryItemRowInfo = uniffi.actro_core.CoreHistoryItemRowInfo
typealias CoreHistoryRepositorySqlInfo = uniffi.actro_core.CoreHistoryRepositorySqlInfo
typealias CoreInstalledAppInfo = uniffi.actro_core.CoreInstalledAppInfo
typealias CoreInstalledAppRowInfo = uniffi.actro_core.CoreInstalledAppRowInfo
typealias CoreSqlValueInfo = uniffi.actro_core.CoreSqlValueInfo
typealias CoreTagInfo = uniffi.actro_core.CoreTagInfo
typealias CoreTagRepositorySqlInfo = uniffi.actro_core.CoreTagRepositorySqlInfo
typealias CoreTagRowInfo = uniffi.actro_core.CoreTagRowInfo
typealias DefaultTagInfo = uniffi.actro_core.DefaultTagInfo
typealias EmailBridgeContractInfo = uniffi.actro_core.EmailBridgeContractInfo
typealias RuntimeContainerSizeChangeEventInfo = uniffi.actro_core.RuntimeContainerSizeChangeEventInfo
typealias RuntimeExtensionPanelVisibleEventInfo = uniffi.actro_core.RuntimeExtensionPanelVisibleEventInfo
typealias RuntimeRootPropsInfo = uniffi.actro_core.RuntimeRootPropsInfo
typealias RuntimeSystemRootPropsInfo = uniffi.actro_core.RuntimeSystemRootPropsInfo
typealias RuntimeSystemTabInfo = uniffi.actro_core.RuntimeSystemTabInfo
typealias RuntimeTabBookmarkEventInfo = uniffi.actro_core.RuntimeTabBookmarkEventInfo
typealias RuntimeTabBookmarkStatusInfo = uniffi.actro_core.RuntimeTabBookmarkStatusInfo
typealias RuntimeTabFullscreenEventInfo = uniffi.actro_core.RuntimeTabFullscreenEventInfo
typealias RuntimeTabInfo = uniffi.actro_core.RuntimeTabInfo
typealias RuntimeTabUpdateInfo = uniffi.actro_core.RuntimeTabUpdateInfo
typealias RuntimeTabUnreadInfo = uniffi.actro_core.RuntimeTabUnreadInfo
typealias SystemAppInfo = uniffi.actro_core.SystemAppInfo
typealias UserInfoBridgeContractInfo = uniffi.actro_core.UserInfoBridgeContractInfo

typealias PasswordStrengthInfo = uniffi.actro_vault.PasswordStrengthInfo
typealias PasswordStrengthLevel = uniffi.actro_vault.PasswordStrengthLevel
typealias VaultCategoryInfo = uniffi.actro_vault.VaultCategoryInfo
typealias VaultCoreException = uniffi.actro_vault.VaultCoreException
typealias VaultFieldInfo = uniffi.actro_vault.VaultFieldInfo
typealias VaultFieldTemplateInfo = uniffi.actro_vault.VaultFieldTemplateInfo

object ActroCore {
    fun activeTabFromRow(row: ActiveTabRowInfo): ActiveTabInfo = uniffi.actro_core.activeTabFromRow(row)

    fun activeTabRowsFromTabs(tabs: List<ActiveTabInfo>): List<ActiveTabRowInfo> =
        uniffi.actro_core.activeTabRowsFromTabs(tabs)

    fun activeTabToRow(
        tab: ActiveTabInfo,
        sortOrder: Int,
    ): ActiveTabRowInfo = uniffi.actro_core.activeTabToRow(tab, sortOrder)

    fun actrBridgeContract(): ActrBridgeContractInfo = uniffi.actro_core.actrBridgeContract()

    @Throws(CoreException::class)
    fun parseActrActorType(raw: String): ActrActorTypeInfo = uniffi.actro_core.parseActrActorType(raw)

    fun appSourceTypeDownloaded(): String = uniffi.actro_core.appSourceTypeDownloaded()

    @Throws(CoreException::class)
    fun appSourceTypeInfo(raw: String): AppSourceTypeInfo = uniffi.actro_core.appSourceTypeInfo(raw)

    fun appSourceTypeLocalDev(): String = uniffi.actro_core.appSourceTypeLocalDev()

    fun browserSearchEngineDefaultId(): String = uniffi.actro_core.browserSearchEngineDefaultId()

    fun browserSearchEngineInfo(engineId: String?): BrowserSearchEngineInfo =
        uniffi.actro_core.browserSearchEngineInfo(engineId)

    fun browserSearchEngineNormalizeId(engineId: String?): String =
        uniffi.actro_core.browserSearchEngineNormalizeId(engineId)

    fun browserSearchEngines(): List<BrowserSearchEngineInfo> =
        uniffi.actro_core.browserSearchEngines()

    fun browserSearchUrl(query: String, engineId: String?): String =
        uniffi.actro_core.browserSearchUrl(query, engineId)

    fun browserDataContract(): BrowserDataContractInfo = uniffi.actro_core.browserDataContract()

    fun browserStorageSchema(): BrowserStorageSchemaInfo = uniffi.actro_core.browserStorageSchema()

    fun browserStorageSchemaVersion(): UInt = uniffi.actro_core.browserStorageSchemaVersion()

    fun browserStorageDatabaseName(): String = uniffi.actro_core.browserStorageDatabaseName()

    fun browserStorageSchemaStatements(): List<String> = uniffi.actro_core.browserStorageSchemaStatements()

    fun browserStorageSchemaMigrationPlanJson(): String =
        uniffi.actro_core.browserStorageSchemaMigrationPlanJson()

    fun browserStorageTableNames(): List<String> = uniffi.actro_core.browserStorageTableNames()

    fun browserStorageRepositorySql(): BrowserStorageRepositorySqlInfo =
        uniffi.actro_core.browserStorageRepositorySql()

    fun browserStorageSyncSql(): BrowserStorageSyncSqlInfo =
        uniffi.actro_core.browserStorageSyncSql()

    fun browserStorageDataModelJson(): String =
        uniffi.actro_core.browserStorageDataModelJson()

    fun browserStorageSyncConflictPolicyJson(): String =
        uniffi.actro_core.browserStorageSyncConflictPolicyJson()

    fun browserStorageSyncMergePolicyJson(): String =
        uniffi.actro_core.browserStorageSyncMergePolicyJson()

    fun browserStorageAddressCanonicalizationPolicyJson(): String =
        uniffi.actro_core.browserStorageAddressCanonicalizationPolicyJson()

    fun browserStorageCanonicalizeAddressesJson(addressesJson: String): String =
        uniffi.actro_core.browserStorageCanonicalizeAddressesJson(addressesJson)

    fun browserStorageBookmarkFolderTreeIntegrityPolicyJson(): String =
        uniffi.actro_core.browserStorageBookmarkFolderTreeIntegrityPolicyJson()

    fun browserStorageBookmarkFolderTreeIntegrityPlanJson(foldersJson: String): String =
        uniffi.actro_core.browserStorageBookmarkFolderTreeIntegrityPlanJson(foldersJson)

    fun browserStorageBookmarkRestorePlacementPolicyJson(): String =
        uniffi.actro_core.browserStorageBookmarkRestorePlacementPolicyJson()

    fun browserStorageBookmarkRestorePlacementPlanJson(bookmarksJson: String): String =
        uniffi.actro_core.browserStorageBookmarkRestorePlacementPlanJson(bookmarksJson)

    fun browserStorageBookmarkOwnershipPolicyJson(): String =
        uniffi.actro_core.browserStorageBookmarkOwnershipPolicyJson()

    fun browserStorageBookmarkOwnershipPlanJson(bookmarksJson: String): String =
        uniffi.actro_core.browserStorageBookmarkOwnershipPlanJson(bookmarksJson)

    fun browserStorageHistoryVisitAggregationPolicyJson(): String =
        uniffi.actro_core.browserStorageHistoryVisitAggregationPolicyJson()

    fun browserStorageHistoryVisitAggregationPlanJson(visitsJson: String): String =
        uniffi.actro_core.browserStorageHistoryVisitAggregationPlanJson(visitsJson)

    fun browserStorageSessionTabOrderingPolicyJson(): String =
        uniffi.actro_core.browserStorageSessionTabOrderingPolicyJson()

    fun browserStorageSessionTabOrderingPlanJson(tabsJson: String): String =
        uniffi.actro_core.browserStorageSessionTabOrderingPlanJson(tabsJson)

    fun browserStorageSessionTabIdPlanJson(tabsJson: String): String =
        uniffi.actro_core.browserStorageSessionTabIdPlanJson(tabsJson)

    fun browserStorageSessionActiveTabConsistencyPolicyJson(): String =
        uniffi.actro_core.browserStorageSessionActiveTabConsistencyPolicyJson()

    fun browserStorageSessionActiveTabConsistencyPlanJson(sessionsJson: String): String =
        uniffi.actro_core.browserStorageSessionActiveTabConsistencyPlanJson(sessionsJson)

    fun browserStorageRetentionPruningPolicyJson(): String =
        uniffi.actro_core.browserStorageRetentionPruningPolicyJson()

    fun browserStorageRetentionPruningPlanJson(recordsJson: String): String =
        uniffi.actro_core.browserStorageRetentionPruningPlanJson(recordsJson)

    fun normalizeBrowserStorageSyncBatchJson(batchJson: String): String =
        uniffi.actro_core.normalizeBrowserStorageSyncBatchJson(batchJson)

    fun browserStorageSyncBatchFromDirtyRowsJson(
        scope: String,
        deviceId: String,
        generatedAtUnixSeconds: Long,
        rowsJson: String,
    ): String =
        uniffi.actro_core.browserStorageSyncBatchFromDirtyRowsJson(
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
        uniffi.actro_core.browserStorageSyncPushRequestFromDirtyRowsJson(
            scope,
            deviceId,
            generatedAtUnixSeconds,
            rowsJson,
        )

    fun normalizeBrowserStorageSyncAckBatchJson(ackJson: String): String =
        uniffi.actro_core.normalizeBrowserStorageSyncAckBatchJson(ackJson)

    fun normalizeBrowserStorageSyncCursorsJson(cursorsJson: String): String =
        uniffi.actro_core.normalizeBrowserStorageSyncCursorsJson(cursorsJson)

    fun emptyBrowserStorageSyncBatchJson(
        scope: String,
        deviceId: String,
        generatedAtUnixSeconds: Long,
    ): String =
        uniffi.actro_core.emptyBrowserStorageSyncBatchJson(scope, deviceId, generatedAtUnixSeconds)

    fun browserStorageSyncPushRequestJson(batchJson: String): String =
        uniffi.actro_core.browserStorageSyncPushRequestJson(batchJson)

    fun browserStorageSyncPullRequestJson(
        scope: String,
        deviceId: String,
        generatedAtUnixSeconds: Long,
        cursorsJson: String,
        maxRecordsPerCollection: UInt,
    ): String =
        uniffi.actro_core.browserStorageSyncPullRequestJson(
            scope,
            deviceId,
            generatedAtUnixSeconds,
            cursorsJson,
            maxRecordsPerCollection,
        )

    fun browserStorageSyncCycleResponseJson(remoteBatchJson: String, ackJson: String): String =
        uniffi.actro_core.browserStorageSyncCycleResponseJson(remoteBatchJson, ackJson)

    fun normalizeBrowserStorageSyncCycleResponseJson(responseJson: String): String =
        uniffi.actro_core.normalizeBrowserStorageSyncCycleResponseJson(responseJson)

    fun browserStorageSyncClientCyclePlanJson(responseJson: String): String =
        uniffi.actro_core.browserStorageSyncClientCyclePlanJson(responseJson)

    fun browserStorageSyncClientAckBatchAfterRemoteApplyJson(
        responseJson: String,
        remoteApplyResultJson: String,
    ): String =
        uniffi.actro_core.browserStorageSyncClientAckBatchAfterRemoteApplyJson(
            responseJson,
            remoteApplyResultJson,
        )

    fun browserStorageSyncClientCycleApplyResultJson(
        responseJson: String,
        remoteApplyResultJson: String,
        ackApplied: Boolean,
    ): String =
        uniffi.actro_core.browserStorageSyncClientCycleApplyResultJson(
            responseJson,
            remoteApplyResultJson,
            ackApplied,
        )

    fun browserStorageSyncTransportContractJson(): String =
        uniffi.actro_core.browserStorageSyncTransportContractJson()

    fun browserStorageSyncCloudStoreContractJson(): String =
        uniffi.actro_core.browserStorageSyncCloudStoreContractJson()

    fun browserStorageSyncCloudProviderAdapterContractJson(): String =
        uniffi.actro_core.browserStorageSyncCloudProviderAdapterContractJson()

    fun browserStorageSyncCloudProviderConnectionContractJson(): String =
        uniffi.actro_core.browserStorageSyncCloudProviderConnectionContractJson()

    fun browserStorageSyncCloudProviderErrorPolicyJson(): String =
        uniffi.actro_core.browserStorageSyncCloudProviderErrorPolicyJson()

    fun browserStorageSyncCloudProviderAdapterPlanJson(
        adapterSnapshotJson: String,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actro_core.browserStorageSyncCloudProviderAdapterPlanJson(
            adapterSnapshotJson,
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudStoreCapabilityPreflightJson(
        capabilitiesJson: String,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actro_core.browserStorageSyncCloudStoreCapabilityPreflightJson(
            capabilitiesJson,
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudProviderConnectionStatusPolicyJson(
        providerSnapshotJson: String,
        retryAttempt: UInt,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actro_core.browserStorageSyncCloudProviderConnectionStatusPolicyJson(
            providerSnapshotJson,
            retryAttempt,
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudProviderErrorClassificationJson(
        providerErrorsJson: String,
        retryAttempt: UInt,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actro_core.browserStorageSyncCloudProviderErrorClassificationJson(
            providerErrorsJson,
            retryAttempt,
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudLocalFolderCapabilitiesJson(): String =
        uniffi.actro_core.browserStorageSyncCloudLocalFolderCapabilitiesJson()

    fun browserStorageSyncCloudLocalFolderCapabilityPreflightJson(
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actro_core.browserStorageSyncCloudLocalFolderCapabilityPreflightJson(
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudLocalFolderCompatibilityPreflightJson(
        rootDir: String,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actro_core.browserStorageSyncCloudLocalFolderCompatibilityPreflightJson(
            rootDir,
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudDataModelJson(): String =
        uniffi.actro_core.browserStorageSyncCloudDataModelJson()

    fun browserStorageSyncCloudCompatibilityStatusJson(
        documentKind: String,
        documentJson: String,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actro_core.browserStorageSyncCloudCompatibilityStatusJson(
            documentKind,
            documentJson,
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudCompatibilityPreflightJson(
        documentsJson: String,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actro_core.browserStorageSyncCloudCompatibilityPreflightJson(
            documentsJson,
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudSchemaRolloutPlanJson(
        compatibilityPreflightJson: String,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actro_core.browserStorageSyncCloudSchemaRolloutPlanJson(
            compatibilityPreflightJson,
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudSchemaMigrationPlaybookJson(
        schemaRolloutPlanJson: String,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actro_core.browserStorageSyncCloudSchemaMigrationPlaybookJson(
            schemaRolloutPlanJson,
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudRecoverySnapshotPlanJson(
        schemaMigrationPlaybookJson: String,
        compatibilityPreflightJson: String,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actro_core.browserStorageSyncCloudRecoverySnapshotPlanJson(
            schemaMigrationPlaybookJson,
            compatibilityPreflightJson,
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudRecoverySnapshotResultPlanJson(
        recoverySnapshotPlanJson: String,
        writeResultsJson: String,
        generatedAtUnixSeconds: Long,
    ): String =
        uniffi.actro_core.browserStorageSyncCloudRecoverySnapshotResultPlanJson(
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
        uniffi.actro_core.browserStorageSyncCloudLocalFolderRecoverySnapshotRoundJson(
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
        uniffi.actro_core.browserStorageSyncCloudDefaultManifestJson(
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
        uniffi.actro_core.normalizeBrowserStorageSyncCloudManifestJson(
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
        uniffi.actro_core.normalizeBrowserStorageSyncCloudDeviceJson(
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
        uniffi.actro_core.browserStorageSyncCloudLogObjectJson(
            collection,
            sequence,
            batchJson,
        )

    fun normalizeBrowserStorageSyncCloudLogObjectJson(logJson: String): String =
        uniffi.actro_core.normalizeBrowserStorageSyncCloudLogObjectJson(logJson)

    fun browserStorageSyncCloudSnapshotObjectJson(
        collection: String,
        generatedAtUnixSeconds: Long,
        batchJson: String,
    ): String =
        uniffi.actro_core.browserStorageSyncCloudSnapshotObjectJson(
            collection,
            generatedAtUnixSeconds,
            batchJson,
        )

    fun normalizeBrowserStorageSyncCloudSnapshotObjectJson(snapshotJson: String): String =
        uniffi.actro_core.normalizeBrowserStorageSyncCloudSnapshotObjectJson(snapshotJson)

    fun browserStorageSyncCloudMembershipPlanJson(requestJson: String): String =
        uniffi.actro_core.browserStorageSyncCloudMembershipPlanJson(requestJson)

    fun browserStorageSyncCloudMembershipResultPlanJson(
        membershipPlanJson: String,
        writeResultsJson: String,
        generatedAtUnixSeconds: Long,
    ): String =
        uniffi.actro_core.browserStorageSyncCloudMembershipResultPlanJson(
            membershipPlanJson,
            writeResultsJson,
            generatedAtUnixSeconds,
        )

    fun browserStorageSyncCloudLocalFolderMembershipRoundJson(
        rootDir: String,
        requestJson: String,
    ): String =
        uniffi.actro_core.browserStorageSyncCloudLocalFolderMembershipRoundJson(
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
        uniffi.actro_core.browserStorageSyncCloudPushPlanJson(
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
        uniffi.actro_core.browserStorageSyncCloudPushResultPlanJson(
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
        uniffi.actro_core.browserStorageSyncCloudPullPlanJson(
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
        uniffi.actro_core.browserStorageSyncCloudPullApplyPlanJson(
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
        uniffi.actro_core.browserStorageSyncCloudPullContinuationPlanJson(
            pullPlanJson,
            readResultsJson,
            generatedAtUnixSeconds,
        )

    fun browserStorageSyncCloudHighWatermarkPolicyJson(): String =
        uniffi.actro_core.browserStorageSyncCloudHighWatermarkPolicyJson()

    fun browserStorageSyncCloudHighWatermarkPlanJson(
        manifestJson: String,
        snapshotsJson: String,
        logsJson: String,
        cursorsJson: String,
        deviceId: String,
        generatedAtUnixSeconds: Long,
    ): String =
        uniffi.actro_core.browserStorageSyncCloudHighWatermarkPlanJson(
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
        uniffi.actro_core.browserStorageSyncCloudCleanupResultPlanJson(
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
        uniffi.actro_core.browserStorageSyncCloudCompactionPlanJson(
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
        uniffi.actro_core.browserStorageSyncCloudCompactionResultPlanJson(
            compactionPlanJson,
            writeResultsJson,
            deleteResultsJson,
            generatedAtUnixSeconds,
        )

    fun browserStorageSyncCloudCycleClientPlanJson(cycleRoundJson: String): String =
        uniffi.actro_core.browserStorageSyncCloudCycleClientPlanJson(cycleRoundJson)

    fun browserStorageSyncCloudCycleClientApplyResultJson(
        cycleRoundJson: String,
        stepResultsJson: String,
        remoteApplyResultJson: String,
    ): String =
        uniffi.actro_core.browserStorageSyncCloudCycleClientApplyResultJson(
            cycleRoundJson,
            stepResultsJson,
            remoteApplyResultJson,
        )

    fun browserStorageSyncCloudCycleStatusPolicyJson(
        cycleRoundJson: String,
        retryAttempt: Int,
    ): String =
        uniffi.actro_core.browserStorageSyncCloudCycleStatusPolicyJson(
            cycleRoundJson,
            retryAttempt.coerceAtLeast(0).toUInt(),
        )

    fun browserStorageSyncCloudMembershipStatusPolicyJson(
        membershipRoundJson: String,
        retryAttempt: Int,
    ): String =
        uniffi.actro_core.browserStorageSyncCloudMembershipStatusPolicyJson(
            membershipRoundJson,
            retryAttempt.coerceAtLeast(0).toUInt(),
        )

    fun browserStorageSyncCloudCycleScheduleStateJson(
        statusPolicyJson: String,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actro_core.browserStorageSyncCloudCycleScheduleStateJson(
            statusPolicyJson,
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudMembershipScheduleStateJson(
        statusPolicyJson: String,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actro_core.browserStorageSyncCloudMembershipScheduleStateJson(
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
        uniffi.actro_core.browserStorageSyncCloudProviderDiagnosticsJson(
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
        uniffi.actro_core.browserStorageSyncCloudObservabilityEventJson(
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
        uniffi.actro_core.browserStorageSyncCloudCycleAttemptPlanJson(
            cycleRoundJson,
            retryAttempt.coerceAtLeast(0).toUInt(),
            observedAtUnixSeconds,
        )

    fun browserStorageSyncCloudMembershipAttemptPlanJson(
        membershipRoundJson: String,
        retryAttempt: Int,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actro_core.browserStorageSyncCloudMembershipAttemptPlanJson(
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
        uniffi.actro_core.browserStorageSyncCloudMembershipLoopPlanJson(
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
        uniffi.actro_core.browserStorageSyncCloudCycleLoopPlanJson(
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
        uniffi.actro_core.browserStorageSyncCloudLocalFolderCycleRoundJson(
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
        uniffi.actro_core.browserStorageSyncCloudLocalFolderCompactionRoundJson(
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
        uniffi.actro_core.browserStorageSyncCloudLocalFolderCleanupRoundJson(
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
        uniffi.actro_core.browserStorageSyncCycleRequestPlanJson(
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
        uniffi.actro_core.browserStorageSyncCycleTransportResultJson(
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
        uniffi.actro_core.browserStorageSyncStatusPolicyJson(
            classification,
            shouldRetry,
            clientAction,
            retryAttempt.coerceAtLeast(0).toUInt(),
        )

    fun browserStorageSyncScheduleStateJson(
        statusPolicyJson: String,
        observedAtUnixSeconds: Long,
    ): String =
        uniffi.actro_core.browserStorageSyncScheduleStateJson(
            statusPolicyJson,
            observedAtUnixSeconds,
        )

    fun browserStorageSyncSchedulerStatusJson(schedulerSnapshotJson: String): String =
        uniffi.actro_core.browserStorageSyncSchedulerStatusJson(schedulerSnapshotJson)

    fun browserStorageSyncAckPlanJson(ackJson: String): String =
        uniffi.actro_core.browserStorageSyncAckPlanJson(ackJson)

    fun browserStorageSyncRemoteApplyPlanJson(batchJson: String): String =
        uniffi.actro_core.browserStorageSyncRemoteApplyPlanJson(batchJson)

    fun browserStorageSyncRemoteApplyResultJson(planJson: String, resultJson: String): String =
        uniffi.actro_core.browserStorageSyncRemoteApplyResultJson(planJson, resultJson)

    fun browserStorageSyncServerMergePlanJson(pushRequestJson: String): String =
        uniffi.actro_core.browserStorageSyncServerMergePlanJson(pushRequestJson)

    fun browserStorageSyncServerMergeResultJson(planJson: String, resultJson: String): String =
        uniffi.actro_core.browserStorageSyncServerMergeResultJson(planJson, resultJson)

    fun browserHistorySnapshotNormalizeJson(snapshotJson: String): String =
        uniffi.actro_core.browserHistorySnapshotNormalizeJson(snapshotJson)

    fun browserBookmarkSnapshotNormalizeJson(snapshotJson: String): String =
        uniffi.actro_core.browserBookmarkSnapshotNormalizeJson(snapshotJson)

    fun browserSessionSnapshotNormalizeJson(snapshotJson: String): String =
        uniffi.actro_core.browserSessionSnapshotNormalizeJson(snapshotJson)

    fun browserSessionHomeSnapshotJson(): String = uniffi.actro_core.browserSessionHomeSnapshotJson()

    fun asrModelLanguageDir(language: String): String = uniffi.actro_core.asrModelLanguageDir(language)

    fun asrModelPlan(language: String): AsrModelPlanInfo = uniffi.actro_core.asrModelPlan(language)

    fun asrBridgeContract(): AsrBridgeContractInfo = uniffi.actro_core.asrBridgeContract()

    fun authEnvironmentBeta(): String = uniffi.actro_core.authEnvironmentBeta()

    fun authEnvironmentDev(): String = uniffi.actro_core.authEnvironmentDev()

    fun authEnvironmentFromAppUrl(url: String): String = uniffi.actro_core.authEnvironmentFromAppUrl(url)

    fun authEnvironmentFromUrl(url: String): String = uniffi.actro_core.authEnvironmentFromUrl(url)

    fun authEnvironmentProd(): String = uniffi.actro_core.authEnvironmentProd()

    fun authEnvironmentStorageSuffix(environment: String): String =
        uniffi.actro_core.authEnvironmentStorageSuffix(environment)

    fun authAccountBaseUrl(environment: String): String = uniffi.actro_core.authAccountBaseUrl(environment)

    fun authResourceUploaderBaseUrl(environment: String): String =
        uniffi.actro_core.authResourceUploaderBaseUrl(environment)

    fun authAccessControlPath(appId: String, userId: String): String =
        uniffi.actro_core.authAccessControlPath(appId, userId)

    fun authClientAppPath(appId: String): String = uniffi.actro_core.authClientAppPath(appId)

    fun authResourceUploadTokenPath(): String = uniffi.actro_core.authResourceUploadTokenPath()

    fun authAccessControlUrlFromAppUrl(appUrl: String, appId: String, userId: String): String? =
        uniffi.actro_core.authAccessControlUrlFromAppUrl(appUrl, appId, userId)

    fun authIsAuthorizedCode(code: Int): Boolean = uniffi.actro_core.authIsAuthorizedCode(code)

    fun authAccessControlStatus(success: Boolean, hasCode: Boolean, code: Int): String =
        uniffi.actro_core.authAccessControlStatus(success, hasCode, code)

    fun authApiResponseSuccess(code: Int): Boolean = uniffi.actro_core.authApiResponseSuccess(code)

    fun authIsClientAppNotFoundCode(code: Int): Boolean =
        uniffi.actro_core.authIsClientAppNotFoundCode(code)

    fun authIsClientAppNotFoundMessage(message: String): Boolean =
        uniffi.actro_core.authIsClientAppNotFoundMessage(message)

    fun coreSchemaStatements(): List<String> = uniffi.actro_core.coreSchemaStatements()

    fun coreSchemaTableNames(): List<String> = uniffi.actro_core.coreSchemaTableNames()

    fun coreSchemaVersion(): UInt = uniffi.actro_core.coreSchemaVersion()

    fun coreHistoryRepositorySql(): CoreHistoryRepositorySqlInfo =
        uniffi.actro_core.coreHistoryRepositorySql()

    fun coreHistoryItemFromRow(row: CoreHistoryItemRowInfo): CoreHistoryItemInfo =
        uniffi.actro_core.coreHistoryItemFromRow(row)

    fun coreHistoryInsertVisitParams(
        id: String,
        title: String,
        appUrl: String?,
        appId: String?,
        visitTimeSeconds: Long,
    ): List<CoreSqlValueInfo> =
        uniffi.actro_core.coreHistoryInsertVisitParams(
            id,
            title,
            appUrl,
            appId,
            visitTimeSeconds,
        )

    fun coreHistorySearchTitleParam(query: String): String? =
        uniffi.actro_core.coreHistorySearchTitleParam(query)

    fun coreTagRepositorySql(): CoreTagRepositorySqlInfo =
        uniffi.actro_core.coreTagRepositorySql()

    fun coreTagFromRow(row: CoreTagRowInfo): CoreTagInfo =
        uniffi.actro_core.coreTagFromRow(row)

    fun coreTagInsertParams(
        id: String,
        name: String,
        color: String?,
        isSystem: Boolean,
    ): List<CoreSqlValueInfo> =
        uniffi.actro_core.coreTagInsertParams(id, name, color, isSystem)

    fun coreTagUpdateParams(
        name: String,
        color: String?,
        id: String,
    ): List<CoreSqlValueInfo> =
        uniffi.actro_core.coreTagUpdateParams(name, color, id)

    fun coreBookmarkRepositorySql(): CoreBookmarkRepositorySqlInfo =
        uniffi.actro_core.coreBookmarkRepositorySql()

    fun coreBookmarkFromRow(row: CoreBookmarkRowInfo): CoreBookmarkInfo =
        uniffi.actro_core.coreBookmarkFromRow(row)

    fun coreBookmarkInsertParams(bookmark: CoreBookmarkInfo): List<CoreSqlValueInfo> =
        uniffi.actro_core.coreBookmarkInsertParams(bookmark)

    fun coreBookmarkSearchParams(query: String): List<String> =
        uniffi.actro_core.coreBookmarkSearchParams(query)

    fun coreBookmarkUpdateParams(bookmark: CoreBookmarkInfo): List<CoreSqlValueInfo> =
        uniffi.actro_core.coreBookmarkUpdateParams(bookmark)

    fun coreAppRepositorySql(): CoreAppRepositorySqlInfo =
        uniffi.actro_core.coreAppRepositorySql()

    fun coreAppSearchParams(query: String): List<String> =
        uniffi.actro_core.coreAppSearchParams(query)

    fun coreInstalledAppFromRow(row: CoreInstalledAppRowInfo): CoreInstalledAppInfo? =
        uniffi.actro_core.coreInstalledAppFromRow(row)

    @Throws(CoreException::class)
    fun coreInstalledAppInsertParams(app: CoreInstalledAppInfo): List<CoreSqlValueInfo> =
        uniffi.actro_core.coreInstalledAppInsertParams(app)

    @Throws(CoreException::class)
    fun coreInstalledAppUpdateParams(app: CoreInstalledAppInfo): List<CoreSqlValueInfo> =
        uniffi.actro_core.coreInstalledAppUpdateParams(app)

    fun defaultTags(): List<DefaultTagInfo> = uniffi.actro_core.defaultTags()

    fun defaultSystemTags(): List<DefaultTagInfo> = uniffi.actro_core.defaultSystemTags()

    fun defaultTagColors(): List<String> = uniffi.actro_core.defaultTagColors()

    fun defaultRuntimeAppStatus(): String = uniffi.actro_core.defaultRuntimeAppStatus()

    fun emailBridgeContract(): EmailBridgeContractInfo = uniffi.actro_core.emailBridgeContract()

    fun evaluateContract(
        name: String?,
        version: UInt,
    ): ContractGateInfo = uniffi.actro_core.evaluateContract(name, version)

    fun extractAppIdFromUrl(url: String): String? = uniffi.actro_core.extractAppIdFromUrl(url)

    fun isAssetPath(path: String): Boolean = uniffi.actro_core.isAssetPath(path)

    @Throws(CoreException::class)
    fun createActrIntegrityPlan(manifestJson: String): ActrIntegrityPlanInfo? =
        uniffi.actro_core.createActrIntegrityPlan(manifestJson)

    fun normalizeIntegrityAlgorithm(raw: String): String? =
        uniffi.actro_core.normalizeIntegrityAlgorithm(raw)

    fun normalizeIntegrityPath(path: String): String? = uniffi.actro_core.normalizeIntegrityPath(path)

    fun normalizeSha256Hex(hash: String): String? = uniffi.actro_core.normalizeSha256Hex(hash)

    fun normalizePath(path: String): String = uniffi.actro_core.normalizePath(path)

    fun normalizeRuntimeAppStatus(value: String): String? = uniffi.actro_core.normalizeRuntimeAppStatus(value)

    fun parseVoskPartialText(json: String): String = uniffi.actro_core.parseVoskPartialText(json)

    fun parseVoskResultText(json: String): String = uniffi.actro_core.parseVoskResultText(json)

    fun postProcessZhAsrSegments(
        text: String,
        segments: List<AsrSegmentInfo>,
    ): String = uniffi.actro_core.postProcessZhAsrSegments(text, segments)

    @Throws(CoreException::class)
    fun parseActrManifest(json: String): ActrManifestInfo = uniffi.actro_core.parseActrManifest(json)

    fun parseAppSourceType(raw: String): String? = uniffi.actro_core.parseAppSourceType(raw)

    fun parseSystemAppType(raw: String): String? = uniffi.actro_core.parseSystemAppType(raw)

    fun relativePathFrom(
        absolutePath: String,
        baseDir: String,
    ): String = uniffi.actro_core.relativePathFrom(absolutePath, baseDir)

    fun runtimeAppStatusAlive(): String = uniffi.actro_core.runtimeAppStatusAlive()

    fun runtimeAppStatusDead(): String = uniffi.actro_core.runtimeAppStatusDead()

    fun runtimeAuthContextAppIdKey(): String = uniffi.actro_core.runtimeAuthContextAppIdKey()

    fun runtimeAuthContextAppUrlKey(): String = uniffi.actro_core.runtimeAuthContextAppUrlKey()

    fun runtimeAuthContextClientIdKey(): String = uniffi.actro_core.runtimeAuthContextClientIdKey()

    fun runtimeAuthContextTabIdKey(): String = uniffi.actro_core.runtimeAuthContextTabIdKey()

    fun runtimeLoginResultCodeKey(): String = uniffi.actro_core.runtimeLoginResultCodeKey()

    fun runtimeLoginResultErrorKey(): String = uniffi.actro_core.runtimeLoginResultErrorKey()

    fun runtimeLoginResultEventName(): String = uniffi.actro_core.runtimeLoginResultEventName()

    fun runtimeLoginResultIdTokenKey(): String = uniffi.actro_core.runtimeLoginResultIdTokenKey()

    fun runtimeLoginResultMessageKey(): String = uniffi.actro_core.runtimeLoginResultMessageKey()

    fun runtimeLoginResultTypeKey(): String = uniffi.actro_core.runtimeLoginResultTypeKey()

    fun runtimeLoginTypeApple(): String = uniffi.actro_core.runtimeLoginTypeApple()

    fun runtimeLoginTypeGoogle(): String = uniffi.actro_core.runtimeLoginTypeGoogle()

    fun runtimeLoginTypeWechat(): String = uniffi.actro_core.runtimeLoginTypeWechat()

    fun runtimeBridgeAppUrlKey(): String = uniffi.actro_core.runtimeBridgeAppUrlKey()

    fun runtimeBridgeContainerHeightKey(): String = uniffi.actro_core.runtimeBridgeContainerHeightKey()

    fun runtimeBridgeContainerWidthKey(): String = uniffi.actro_core.runtimeBridgeContainerWidthKey()

    fun runtimeBridgeIconKey(): String = uniffi.actro_core.runtimeBridgeIconKey()

    fun runtimeBridgeIsBookmarkedKey(): String = uniffi.actro_core.runtimeBridgeIsBookmarkedKey()

    fun runtimeBridgeMarkedKey(): String = uniffi.actro_core.runtimeBridgeMarkedKey()

    fun runtimeBridgeStatusKey(): String = uniffi.actro_core.runtimeBridgeStatusKey()

    fun runtimeBridgeTabIdKey(): String = uniffi.actro_core.runtimeBridgeTabIdKey()

    fun runtimeBridgeTitleKey(): String = uniffi.actro_core.runtimeBridgeTitleKey()

    fun runtimeBridgeUnreadKey(): String = uniffi.actro_core.runtimeBridgeUnreadKey()

    fun runtimeBridgeVisibleKey(): String = uniffi.actro_core.runtimeBridgeVisibleKey()

    fun runtimeTabEnteredFullscreenEventName(): String =
        uniffi.actro_core.runtimeTabEnteredFullscreenEventName()

    fun runtimeTabExitedFullscreenEventName(): String =
        uniffi.actro_core.runtimeTabExitedFullscreenEventName()

    fun runtimeTabBookmarkChangedEventName(): String =
        uniffi.actro_core.runtimeTabBookmarkChangedEventName()

    fun runtimeExtensionPanelVisibleChangeEventName(): String =
        uniffi.actro_core.runtimeExtensionPanelVisibleChangeEventName()

    fun runtimeContainerSizeChangeEventName(): String =
        uniffi.actro_core.runtimeContainerSizeChangeEventName()

    fun runtimeSearchInputChangedEventName(): String =
        uniffi.actro_core.runtimeSearchInputChangedEventName()

    fun runtimeContainerSizeChangeEvent(
        containerWidth: Int,
        containerHeight: Int,
    ): RuntimeContainerSizeChangeEventInfo =
        uniffi.actro_core.runtimeContainerSizeChangeEvent(containerWidth, containerHeight)

    fun runtimeExtensionPanelVisibleEvent(
        tabId: String,
        visible: Boolean,
    ): RuntimeExtensionPanelVisibleEventInfo =
        uniffi.actro_core.runtimeExtensionPanelVisibleEvent(tabId, visible)

    fun runtimeRootPropsFromActiveTabs(
        tab: ActiveTabInfo,
        tabs: List<ActiveTabInfo>,
        containerHeight: Int,
        containerWidth: Int,
    ): RuntimeRootPropsInfo =
        uniffi.actro_core.runtimeRootPropsFromActiveTabs(
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
        uniffi.actro_core.runtimeRootPropsFromRuntimeTabs(
            tab,
            tabs,
            isLeftPanelVisible,
            isRightPanelVisible,
            containerHeight,
            containerWidth,
            appUrl,
        )

    fun runtimeRootPropsCurrentTabIdKey(): String =
        uniffi.actro_core.runtimeRootPropsCurrentTabIdKey()

    fun runtimeRootPropsIsLeftPanelVisibleKey(): String =
        uniffi.actro_core.runtimeRootPropsIsLeftPanelVisibleKey()

    fun runtimeRootPropsIsRightPanelVisibleKey(): String =
        uniffi.actro_core.runtimeRootPropsIsRightPanelVisibleKey()

    fun runtimeRootPropsSingleTabModeKey(): String =
        uniffi.actro_core.runtimeRootPropsSingleTabModeKey()

    fun runtimeRootPropsTabKey(): String = uniffi.actro_core.runtimeRootPropsTabKey()

    fun runtimeRootPropsTabsKey(): String = uniffi.actro_core.runtimeRootPropsTabsKey()

    fun runtimeTabFromActiveTab(tab: ActiveTabInfo): RuntimeTabInfo =
        uniffi.actro_core.runtimeTabFromActiveTab(tab)

    fun runtimeTabBookmarkEvent(
        tabId: String,
        isBookmarked: Boolean,
    ): RuntimeTabBookmarkEventInfo =
        uniffi.actro_core.runtimeTabBookmarkEvent(tabId, isBookmarked)

    fun runtimeTabBookmarkStatusFromFields(
        tabId: String?,
        marked: Boolean?,
    ): RuntimeTabBookmarkStatusInfo? =
        uniffi.actro_core.runtimeTabBookmarkStatusFromFields(tabId, marked)

    fun runtimeTabFullscreenEvent(tabId: String): RuntimeTabFullscreenEventInfo =
        uniffi.actro_core.runtimeTabFullscreenEvent(tabId)

    fun runtimeTabPropsBackgroundColorKey(): String =
        uniffi.actro_core.runtimeTabPropsBackgroundColorKey()

    fun runtimeTabPropsHtmlContentKey(): String =
        uniffi.actro_core.runtimeTabPropsHtmlContentKey()

    fun runtimeTabPropsIdKey(): String = uniffi.actro_core.runtimeTabPropsIdKey()

    fun runtimeTabPropsReloadKeyKey(): String = uniffi.actro_core.runtimeTabPropsReloadKeyKey()

    fun runtimeTabPropsRightPanelDefaultVisibleKey(): String =
        uniffi.actro_core.runtimeTabPropsRightPanelDefaultVisibleKey()

    fun runtimeTabPropsUnifiedScriptKey(): String =
        uniffi.actro_core.runtimeTabPropsUnifiedScriptKey()

    fun runtimeTabUpdateFromFields(
        tabId: String?,
        title: String?,
        icon: String?,
        appUrl: String?,
        status: String?,
    ): RuntimeTabUpdateInfo? =
        uniffi.actro_core.runtimeTabUpdateFromFields(tabId, title, icon, appUrl, status)

    fun runtimeTabUnreadFromFields(
        tabId: String?,
        unread: Boolean?,
    ): RuntimeTabUnreadInfo? =
        uniffi.actro_core.runtimeTabUnreadFromFields(tabId, unread)

    @Throws(CoreException::class)
    fun runtimeSystemRootProps(systemAppType: String): RuntimeSystemRootPropsInfo =
        uniffi.actro_core.runtimeSystemRootProps(systemAppType)

    fun systemAppTypeBookmarks(): String = uniffi.actro_core.systemAppTypeBookmarks()

    fun systemAppTypeGlobalSearch(): String = uniffi.actro_core.systemAppTypeGlobalSearch()

    fun systemAppTypeSettings(): String = uniffi.actro_core.systemAppTypeSettings()

    fun supportedContractName(): String = uniffi.actro_core.supportedContractName()

    fun supportedContractVersion(): UInt = uniffi.actro_core.supportedContractVersion()

    @Throws(CoreException::class)
    fun systemAppInfo(raw: String): SystemAppInfo = uniffi.actro_core.systemAppInfo(raw)

    fun userInfoBridgeContract(): UserInfoBridgeContractInfo =
        uniffi.actro_core.userInfoBridgeContract()

    @Throws(CoreException::class)
    fun validateActrManifest(json: String) = uniffi.actro_core.validateActrManifest(json)
}

object ActroVault {
    @Throws(VaultCoreException::class)
    fun createVerificationHash(
        password: String,
        saltBase64: String,
    ): String = uniffi.actro_vault.createVaultVerificationHash(password, saltBase64)

    @Throws(VaultCoreException::class)
    fun decryptFields(
        keyBase64: String,
        ciphertextBase64: String,
    ): List<VaultFieldInfo> = uniffi.actro_vault.decryptVaultFields(keyBase64, ciphertextBase64)

    @Throws(VaultCoreException::class)
    fun decryptString(
        keyBase64: String,
        ciphertextBase64: String,
    ): String = uniffi.actro_vault.decryptVaultString(keyBase64, ciphertextBase64)

    @Throws(VaultCoreException::class)
    fun deriveKeyBase64(
        password: String,
        saltBase64: String,
    ): String = uniffi.actro_vault.deriveVaultKeyBase64(password, saltBase64)

    @Throws(VaultCoreException::class)
    fun encryptFields(
        keyBase64: String,
        fields: List<VaultFieldInfo>,
    ): String = uniffi.actro_vault.encryptVaultFields(keyBase64, fields)

    @Throws(VaultCoreException::class)
    fun encryptString(
        keyBase64: String,
        plaintext: String,
    ): String = uniffi.actro_vault.encryptVaultString(keyBase64, plaintext)

    fun generateSaltBase64(): String = uniffi.actro_vault.generateVaultSaltBase64()

    fun parseCategory(raw: String): String? = uniffi.actro_vault.parseVaultCategory(raw)

    @Throws(VaultCoreException::class)
    fun categoryInfo(raw: String): VaultCategoryInfo = uniffi.actro_vault.vaultCategoryInfo(raw)

    fun categoryTemplates(): List<VaultCategoryInfo> = uniffi.actro_vault.vaultCategoryTemplates()

    fun passwordStrength(password: String): PasswordStrengthInfo = uniffi.actro_vault.vaultPasswordStrength(password)

    @Throws(VaultCoreException::class)
    fun verifyPassword(
        password: String,
        saltBase64: String,
        verificationHashBase64: String,
    ): Boolean = uniffi.actro_vault.verifyVaultPassword(password, saltBase64, verificationHashBase64)
}
