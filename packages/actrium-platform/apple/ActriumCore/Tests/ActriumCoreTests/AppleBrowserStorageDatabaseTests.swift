import Foundation
import XCTest
@testable import ActriumCore

final class AppleBrowserStorageDatabaseTests: XCTestCase {
    func testCloudProviderAdapterObjectSpacePlanContract() throws {
        let contract = try jsonObject(browserStorageSyncCloudProviderAdapterContractJson())
        XCTAssertEqual(contract["planType"] as? String, "cloudProviderAdapterPlan")
        let objectSpace = contract["objectSpace"] as? [String: Any]
        XCTAssertEqual(objectSpace?["rootPath"] as? String, "Actro/BrowserSync")
        XCTAssertTrue((contract["operationBoundary"] as? String ?? "").contains("coarse rounds"))

        let localFolderCapabilities = browserStorageSyncCloudLocalFolderCapabilitiesJson()
        let ready = try jsonObject(browserStorageSyncCloudProviderAdapterPlanJson(
            adapterSnapshotJson: localFolderCapabilities,
            observedAtUnixSeconds: 1_690
        ))
        XCTAssertEqual(ready["planType"] as? String, "cloudProviderAdapterPlan")
        XCTAssertEqual(ready["classification"] as? String, "ready")
        XCTAssertEqual(ready["adapterFamily"] as? String, "localFolder")
        XCTAssertEqual(ready["productionReady"] as? Bool, true)
        XCTAssertEqual(ready["shouldAllowSync"] as? Bool, true)
        XCTAssertEqual(ready["shouldBlockSync"] as? Bool, false)
        let objectLayout = ready["objectLayout"] as? [String: Any]
        XCTAssertEqual(objectLayout?["manifestPath"] as? String, "Actro/BrowserSync/manifest.json")
        let prefixes = ready["requiredPrefixes"] as? [[String: Any]] ?? []
        XCTAssertTrue(prefixes.contains { $0["prefix"] as? String == "Actro/BrowserSync/logs/" })

        let setup = try jsonObject(browserStorageSyncCloudProviderAdapterPlanJson(
            adapterSnapshotJson: """
            {
                "providerKind": "localFolder",
                "rootPath": "Wrong/BrowserSync",
                "revisionKind": "contentHash",
                "supportsPrefixList": true,
                "supportsPagedList": true,
                "supportsConditionalWrite": true,
                "supportsConditionalDelete": true,
                "supportsReadIfChanged": true,
                "supportsAtomicRename": false,
                "supportsAppFolder": true,
                "backgroundSync": "platformScheduled",
                "conflictResolution": "optimisticRevision",
                "maxObjectBytes": 8388608,
                "maxPageSize": 1000
            }
            """,
            observedAtUnixSeconds: 1_691
        ))
        XCTAssertEqual(setup["classification"] as? String, "providerSetupRequired")
        XCTAssertEqual(setup["shouldBlockSync"] as? Bool, true)
        XCTAssertEqual(setup["rootPathAction"] as? String, "configureActroBrowserSyncRoot")
    }

    func testCloudProviderConnectionStatusPolicyContract() throws {
        let contract = try jsonObject(browserStorageSyncCloudProviderConnectionContractJson())
        XCTAssertEqual(contract["planType"] as? String, "cloudProviderConnectionStatusPolicy")
        XCTAssertTrue((contract["productConstraint"] as? String ?? "").contains("No Actrium product server"))

        let ready = try jsonObject(browserStorageSyncCloudProviderConnectionStatusPolicyJson(
            providerSnapshotJson: """
            {
                "providerKind": "localFolder",
                "scope": "profile",
                "deviceId": "device-apple",
                "connectionState": "connected",
                "credentialState": "available",
                "membershipStatus": "active",
                "cloudStoreReady": true
            }
            """,
            retryAttempt: 0,
            observedAtUnixSeconds: 1_700
        ))
        XCTAssertEqual(ready["classification"] as? String, "ready")
        XCTAssertEqual(ready["shouldAttemptSync"] as? Bool, true)
        XCTAssertEqual(ready["shouldAttemptMembership"] as? Bool, false)
        XCTAssertEqual(ready["schedulerAction"] as? String, "idle")

        let membership = try jsonObject(browserStorageSyncCloudProviderConnectionStatusPolicyJson(
            providerSnapshotJson: """
            {
                "providerKind": "iCloudDrive",
                "providerAccountId": "icloud-user",
                "scope": "profile",
                "deviceId": "device-apple",
                "connectionState": "connected",
                "credentialState": "available",
                "membershipStatus": "missing",
                "cloudStoreReady": true
            }
            """,
            retryAttempt: 0,
            observedAtUnixSeconds: 1_701
        ))
        XCTAssertEqual(membership["classification"] as? String, "membershipRequired")
        XCTAssertEqual(membership["shouldAttemptMembership"] as? Bool, true)
        XCTAssertEqual(membership["shouldAttemptSync"] as? Bool, false)
        XCTAssertEqual(membership["providerAction"] as? String, "writeCloudMembership")

        let retry = try jsonObject(browserStorageSyncCloudProviderConnectionStatusPolicyJson(
            providerSnapshotJson: """
            {
                "providerKind": "oneDrive",
                "providerAccountId": "onedrive-user",
                "connectionState": "connecting",
                "credentialState": "available",
                "membershipStatus": "active",
                "cloudStoreReady": true,
                "lastProviderErrorKind": "network"
            }
            """,
            retryAttempt: 1,
            observedAtUnixSeconds: 1_702
        ))
        XCTAssertEqual(retry["classification"] as? String, "providerUnavailable")
        XCTAssertEqual(retry["shouldRetry"] as? Bool, true)
        XCTAssertEqual(retry["retryDelaySeconds"] as? Int, 10)
        XCTAssertEqual(retry["schedulerAction"] as? String, "scheduleRetry")
    }

    func testCloudProviderErrorClassificationPolicyContract() throws {
        let policy = try jsonObject(browserStorageSyncCloudProviderErrorPolicyJson())
        XCTAssertEqual(policy["planType"] as? String, "cloudProviderErrorClassification")
        XCTAssertTrue((policy["policyBoundary"] as? String ?? "").contains("coarse native/provider rounds"))

        let reread = try jsonObject(browserStorageSyncCloudProviderErrorClassificationJson(
            providerErrorsJson: """
            {
                "providerKind": "iCloudDrive",
                "errors": [
                    {
                        "operationIndex": 1,
                        "operationType": "writeManifest",
                        "path": "Actro/BrowserSync/manifest.json",
                        "providerStatusCode": 412,
                        "message": "etag mismatch"
                    }
                ]
            }
            """,
            retryAttempt: 0,
            observedAtUnixSeconds: 1_703
        ))
        XCTAssertEqual(reread["classification"] as? String, "cloudStateRereadRequired")
        XCTAssertEqual(reread["shouldRereadCloud"] as? Bool, true)
        XCTAssertEqual(reread["schedulerAction"] as? String, "runCloudReread")
        let rereadErrors = reread["errors"] as? [[String: Any]] ?? []
        XCTAssertEqual(rereadErrors.first?["canonicalErrorKind"] as? String, "conflict")

        let diagnostics = try jsonObject(browserStorageSyncCloudProviderDiagnosticsJson(
            capabilityPreflightJson: "",
            compatibilityPreflightJson: "",
            providerErrorClassificationJson: try jsonString(reread),
            cycleStatusPolicyJson: "",
            scheduleStateJson: "",
            observedAtUnixSeconds: 1_705
        ))
        XCTAssertEqual(diagnostics["classification"] as? String, "cloudConflict")
        XCTAssertEqual(diagnostics["providerErrorClassification"] as? String, "cloudStateRereadRequired")
        XCTAssertEqual(diagnostics["pendingCloudReread"] as? Bool, true)

        let retry = try jsonObject(browserStorageSyncCloudProviderErrorClassificationJson(
            providerErrorsJson: """
            {
                "providerKind": "oneDrive",
                "operationResults": [
                    {
                        "operationIndex": 2,
                        "operationType": "listLogs",
                        "success": false,
                        "providerStatusCode": 503
                    }
                ]
            }
            """,
            retryAttempt: 1,
            observedAtUnixSeconds: 1_704
        ))
        XCTAssertEqual(retry["classification"] as? String, "providerUnavailable")
        XCTAssertEqual(retry["shouldRetry"] as? Bool, true)
        XCTAssertEqual(retry["retryDelaySeconds"] as? Int, 10)
        XCTAssertEqual(retry["providerAction"] as? String, "retryProviderOperation")
    }

    func testMigratesBrowserStorageSQLiteV1V2V3ToV6() throws {
        for legacyVersion in 1...3 {
            let databaseURL = temporaryDatabaseURL()
            defer { deleteDatabaseFiles(databaseURL) }

            try createLegacyBrowserStorageDatabase(at: databaseURL, schemaVersion: legacyVersion)
            let database = try AppleBrowserStorageDatabase(path: databaseURL.path)

            let history = database.loadRecentHistory(limit: 10)
            XCTAssertEqual(history.count, 1)
            XCTAssertEqual(history.first?.address, "https://example.com/history-v\(legacyVersion)")
            XCTAssertEqual(history.first?.visitCount, 2)

            let bookmarks = database.loadRecentBookmarks(limit: 10)
            XCTAssertEqual(bookmarks.count, 1)
            XCTAssertEqual(bookmarks.first?.address, "https://example.com/bookmark-v\(legacyVersion)")
            XCTAssertEqual(bookmarks.first?.ownerKind, "user")
            XCTAssertEqual(bookmarks.first?.ownerId, "")

            let session = database.loadSession()
            XCTAssertEqual(session?.tabs.count, 1)
            XCTAssertEqual(session?.tabs.first?.address, "https://example.com/session-v\(legacyVersion)")

            try assertBrowserStorageV6Shape(databaseURL, legacyVersion: legacyVersion)

            let batch = try jsonObject(database.exportDirtySyncBatch(
                scope: "profile",
                deviceId: "device-apple",
                generatedAtUnixSeconds: 1234,
                limitPerCollection: 20
            ))
            XCTAssertEqual(batch["scope"] as? String, "profile")
            XCTAssertEqual(batch["deviceId"] as? String, "device-apple")
            XCTAssertTrue(batchHasRecord(batch, recordType: "historyPage", recordId: "https://example.com/history-v\(legacyVersion)"))
            XCTAssertTrue(batchHasRecord(batch, recordType: "historyVisit", recordId: "visit-v\(legacyVersion)"))
            XCTAssertTrue(batchHasRecord(batch, recordType: "bookmark", recordId: "bookmark-v\(legacyVersion)"))
            XCTAssertTrue(batchHasBookmarkOwner(batch, recordId: "bookmark-v\(legacyVersion)", ownerKind: "user", ownerId: ""))
            XCTAssertTrue(batchHasRecord(batch, recordType: "session", recordId: "default"))
            XCTAssertTrue(batchHasRecord(batch, recordType: "sessionTab"))

            let pushRequest = try jsonObject(database.exportDirtySyncPushRequest(
                scope: "profile",
                deviceId: "device-apple",
                generatedAtUnixSeconds: 1235,
                limitPerCollection: 20
            ))
            XCTAssertEqual(pushRequest["requestType"] as? String, "push")
            XCTAssertNotNil(pushRequest["pushBatch"] as? [String: Any])

            XCTAssertTrue(database.applySyncAckBatch("""
                {
                    "version": 1,
                    "storageSchemaVersion": 6,
                    "scope": "profile",
                    "deviceId": "device-apple",
                    "syncedAtUnixSeconds": 1240,
                    "records": [
                        {
                            "recordType": "historyPage",
                            "recordId": "https://example.com/history-v\(legacyVersion)",
                            "syncRevision": 999
                        }
                    ],
                    "cursors": []
                }
                """))
            XCTAssertEqual(queryScalarString(databaseURL, "SELECT sync_state FROM browser_history_pages WHERE address = 'https://example.com/history-v\(legacyVersion)'"), "synced")
            XCTAssertEqual(queryScalarString(databaseURL, "SELECT sync_revision FROM browser_history_pages WHERE address = 'https://example.com/history-v\(legacyVersion)'"), "999")

            let remoteBatchJson = browserStorageSyncBatchFromDirtyRowsJson(
                scope: "profile",
                deviceId: "device-apple",
                generatedAtUnixSeconds: 1250,
                rowsJson: """
                    [
                        {
                            "record_type": "historyPage",
                            "record_id": "https://example.com/remote-v\(legacyVersion)",
                            "sync_revision": \(1200 + legacyVersion),
                            "sync_state": "remote",
                            "origin_device_id": "device-remote",
                            "updated_at": 1250,
                            "address": "https://example.com/remote-v\(legacyVersion)",
                            "title": "Remote \(legacyVersion)",
                            "visit_count": 3,
                            "last_visited_at": 1250,
                            "created_at": 1240
                        }
                    ]
                    """
            )
            let cycleResponseJson = browserStorageSyncCycleResponseJson(
                remoteBatchJson: remoteBatchJson,
                ackJson: """
                    {
                        "version": 1,
                        "storageSchemaVersion": 6,
                        "scope": "profile",
                        "deviceId": "device-apple",
                        "syncedAtUnixSeconds": 1251,
                        "records": [],
                        "cursors": []
                    }
                    """
            )
            let cycleResult = try jsonObject(database.applySyncCycleResponse(cycleResponseJson))
            XCTAssertEqual(cycleResult["action"] as? String, "applyRemoteThenAck")
            XCTAssertEqual(cycleResult["ackApplied"] as? Bool, true)
            XCTAssertEqual(cycleResult["cursorAckDeferred"] as? Bool, false)
            XCTAssertEqual(queryScalarString(databaseURL, "SELECT title FROM browser_history_pages WHERE address = 'https://example.com/remote-v\(legacyVersion)'"), "Remote \(legacyVersion)")
            XCTAssertEqual(queryScalarString(databaseURL, "SELECT cursor FROM browser_sync_cursors WHERE scope = 'profile' AND collection = 'historyPages'"), "\(1200 + legacyVersion)")

            let cycleRequestPlan = try jsonObject(database.exportSyncCycleRequestPlan(
                baseUrl: "http://127.0.0.1:8787/",
                accessToken: "token-apple-123",
                scope: "profile",
                deviceId: "device-apple",
                generatedAtUnixSeconds: 1260,
                limitPerCollection: 20
            ))
            XCTAssertEqual(cycleRequestPlan["method"] as? String, "POST")
            XCTAssertEqual(cycleRequestPlan["path"] as? String, "/v1/browser-storage/sync/cycle")
            XCTAssertEqual(cycleRequestPlan["url"] as? String, "http://127.0.0.1:8787/v1/browser-storage/sync/cycle")
            let headers = cycleRequestPlan["headers"] as? [[String: Any]] ?? []
            XCTAssertTrue(headers.contains { header in
                header["name"] as? String == "Authorization" &&
                    header["value"] as? String == "Bearer token-apple-123"
            })
            let requestBody = try jsonObject(cycleRequestPlan["bodyJson"] as? String ?? "{}")
            XCTAssertEqual((requestBody["pushRequest"] as? [String: Any])?["requestType"] as? String, "push")
            let cursors = requestBody["cursors"] as? [[String: Any]] ?? []
            XCTAssertTrue(cursors.contains { cursor in
                cursor["collection"] as? String == "historyPages" &&
                    cursor["cursor"] as? String == "\(1200 + legacyVersion)"
            })

            XCTAssertTrue(database.deleteBookmark(address: "https://example.com/bookmark-v\(legacyVersion)"))
            XCTAssertEqual(queryScalarString(databaseURL, "SELECT is_deleted FROM browser_bookmarks WHERE id = 'bookmark-v\(legacyVersion)'"), "1")
            XCTAssertFalse(queryScalarString(databaseURL, "SELECT deleted_at FROM browser_bookmarks WHERE id = 'bookmark-v\(legacyVersion)'")?.isEmpty ?? true)

            let tombstoneBatch = try jsonObject(database.exportDirtySyncBatch(
                scope: "profile",
                deviceId: "device-apple",
                generatedAtUnixSeconds: 1236,
                limitPerCollection: 20
            ))
            XCTAssertTrue(batchHasDeletedBookmark(tombstoneBatch, recordId: "bookmark-v\(legacyVersion)"))
        }
    }

    func testSaveSessionAdoptsCoreNormalizedSnapshot() throws {
        let databaseURL = temporaryDatabaseURL()
        defer { deleteDatabaseFiles(databaseURL) }

        let database = try AppleBrowserStorageDatabase(path: databaseURL.path)
        let longTitle = String(repeating: "t", count: 200)
        let snapshot = AppleBrowserSessionSnapshot(
            activeTabIndex: 9,
            tabs: [
                AppleBrowserSessionTab(
                    id: "",
                    title: longTitle,
                    address: "https://user:secret@example.com:443/private",
                    createdAtUnixSeconds: 1_600
                ),
                AppleBrowserSessionTab(
                    id: "tab-docs",
                    title: "   ",
                    address: "https://example.com/docs",
                    createdAtUnixSeconds: 1_601
                ),
            ],
            surfaceKind: "macos"
        )
        XCTAssertTrue(database.saveSession(snapshot: snapshot))

        let restored = database.loadSession()
        XCTAssertEqual(restored?.tabs.count, 2)
        XCTAssertEqual(restored?.activeTabIndex, 1)
        XCTAssertEqual(restored?.tabs.first?.address, "https://example.com/private")
        XCTAssertEqual(restored?.tabs.first?.title.count, 160)
        XCTAssertEqual(restored?.tabs.first?.createdAtUnixSeconds, 1_600)
        XCTAssertEqual(restored?.tabs.last?.id, "tab-docs")
        XCTAssertEqual(restored?.tabs.last?.title, "New Tab")
        XCTAssertEqual(restored?.tabs.last?.createdAtUnixSeconds, 1_601)
        XCTAssertEqual(restored?.surfaceKind, "macos")

        // Stable tab ids are deterministic: saving the same snapshot again
        // resolves the empty-id tab to the same Rust-derived id.
        let stableId = restored?.tabs.first?.id ?? ""
        XCTAssertFalse(stableId.isEmpty)
        XCTAssertTrue(database.saveSession(snapshot: snapshot))
        XCTAssertEqual(database.loadSession()?.tabs.first?.id, stableId)
    }

    func testSyncClientExecutesInjectedCycleTransportAndAppliesResponse() async throws {
        let databaseURL = temporaryDatabaseURL()
        defer { deleteDatabaseFiles(databaseURL) }

        let database = try AppleBrowserStorageDatabase(path: databaseURL.path)
        XCTAssertTrue(database.upsertHistoryItem(AppleBrowserHistoryItem(
            title: "Local",
            address: "https://example.com/local",
            visitedAtUnixSeconds: 1_300
        )))

        let remoteBatchJson = browserStorageSyncBatchFromDirtyRowsJson(
            scope: "profile",
            deviceId: "device-apple",
            generatedAtUnixSeconds: 1_310,
            rowsJson: """
                [
                    {
                        "record_type": "historyPage",
                        "record_id": "https://example.com/sync-client-remote",
                        "sync_revision": 2001,
                        "sync_state": "remote",
                        "origin_device_id": "device-remote",
                        "updated_at": 1310,
                        "address": "https://example.com/sync-client-remote",
                        "title": "Remote From Client",
                        "visit_count": 2,
                        "last_visited_at": 1310,
                        "created_at": 1309
                    }
                ]
                """
        )
        let cycleResponseJson = browserStorageSyncCycleResponseJson(
            remoteBatchJson: remoteBatchJson,
            ackJson: """
                {
                    "version": 1,
                    "storageSchemaVersion": 6,
                    "scope": "profile",
                    "deviceId": "device-apple",
                    "syncedAtUnixSeconds": 1311,
                    "records": [],
                    "cursors": []
                }
                """
        )
        let transport = AppleBrowserStorageSyncFakeTransport(responseJson: cycleResponseJson)
        let client = AppleBrowserStorageSyncClient(database: database, transport: transport)

        let result = try jsonObject(await client.runSyncCycle(
            baseUrl: "http://127.0.0.1:8787/",
            accessToken: "token-apple-client",
            scope: "profile",
            deviceId: "device-apple",
            generatedAtUnixSeconds: 1_312,
            limitPerCollection: 20
        ))

        XCTAssertTrue(result["applied"] as? Bool ?? false)
        XCTAssertEqual(result["classification"] as? String, "success")
        XCTAssertEqual(transport.lastRequest?.method, "POST")
        XCTAssertEqual(
            transport.lastRequest?.url.absoluteString,
            "http://127.0.0.1:8787/v1/browser-storage/sync/cycle"
        )
        XCTAssertEqual(transport.lastRequest?.headers["Authorization"], "Bearer token-apple-client")
        let requestBody = try jsonObject(transport.lastRequest?.bodyJson ?? "{}")
        XCTAssertEqual((requestBody["pushRequest"] as? [String: Any])?["requestType"] as? String, "push")
        XCTAssertEqual(
            queryScalarString(
                databaseURL,
                "SELECT title FROM browser_history_pages WHERE address = 'https://example.com/sync-client-remote'"
            ),
            "Remote From Client"
        )
    }

    func testSyncClientRunsLocalFolderCloudCycleAndAppliesClientPlan() async throws {
        let remoteDatabaseURL = temporaryDatabaseURL()
        let localDatabaseURL = temporaryDatabaseURL()
        let cloudRootURL = temporaryDirectoryURL()
        defer {
            deleteDatabaseFiles(remoteDatabaseURL)
            deleteDatabaseFiles(localDatabaseURL)
            try? FileManager.default.removeItem(at: cloudRootURL)
        }

        let remoteDatabase = try AppleBrowserStorageDatabase(path: remoteDatabaseURL.path)
        XCTAssertTrue(remoteDatabase.recordHistoryVisit(AppleBrowserHistoryItem(
            title: "Remote Local Folder",
            address: "https://example.com/apple-local-folder-remote",
            visitedAtUnixSeconds: 1_700
        )))

        let remoteClient = AppleBrowserStorageSyncClient(database: remoteDatabase)
        let seedResult = try jsonObject(await remoteClient.runLocalFolderSyncCycle(
            rootDir: cloudRootURL.path,
            scope: "profile",
            deviceId: "device-remote",
            generatedAtUnixSeconds: 1_710,
            limitPerCollection: 20,
            observedAtUnixSeconds: 1_710
        ))

        XCTAssertEqual(seedResult["endpointType"] as? String, "cloudLocalFolderCycle")
        XCTAssertEqual(seedResult["providerKind"] as? String, "localFolder")
        XCTAssertEqual(
            (seedResult["capabilityPreflight"] as? [String: Any])?["planType"] as? String,
            "cloudStoreCapabilityPreflight"
        )
        XCTAssertEqual(
            (seedResult["capabilityPreflight"] as? [String: Any])?["classification"] as? String,
            "ready"
        )
        let seedCapabilities = (seedResult["capabilityPreflight"] as? [String: Any])?["capabilities"] as? [String: Any]
        XCTAssertEqual(seedCapabilities?["providerKind"] as? String, "localfolder")
        XCTAssertEqual(seedCapabilities?["revisionKind"] as? String, "contenthash")
        XCTAssertEqual((seedResult["capabilityPreflight"] as? [String: Any])?["shouldAllowSync"] as? Bool, true)
        XCTAssertEqual(
            (seedResult["compatibilityPreflight"] as? [String: Any])?["planType"] as? String,
            "cloudCompatibilityPreflight"
        )
        XCTAssertEqual(
            (seedResult["compatibilityPreflight"] as? [String: Any])?["classification"] as? String,
            "compatible"
        )
        XCTAssertEqual((seedResult["compatibilityPreflight"] as? [String: Any])?["documentCount"] as? Int, 0)
        XCTAssertTrue(seedResult["schemaRolloutPlan"] is NSNull)
        XCTAssertTrue(seedResult["schemaMigrationPlaybook"] is NSNull)
        XCTAssertTrue(seedResult["recoverySnapshotRound"] is NSNull)
        XCTAssertEqual(seedResult["classification"] as? String, "success")
        XCTAssertEqual((seedResult["applyResult"] as? [String: Any])?["ackApplied"] as? Bool, true)
        XCTAssertEqual(
            (seedResult["compactionRound"] as? [String: Any])?["planType"] as? String,
            "compactionRound"
        )
        XCTAssertEqual((seedResult["compactionRound"] as? [String: Any])?["completed"] as? Bool, true)
        XCTAssertEqual(
            (seedResult["compactionRound"] as? [String: Any])?["clientAction"] as? String,
            "nothingToCompact"
        )
        XCTAssertEqual(
            (seedResult["cleanupRound"] as? [String: Any])?["planType"] as? String,
            "cleanupRound"
        )
        XCTAssertEqual((seedResult["cleanupRound"] as? [String: Any])?["cleanupCompleted"] as? Bool, true)
        XCTAssertEqual(
            (seedResult["cleanupRound"] as? [String: Any])?["clientAction"] as? String,
            "nothingToCleanup"
        )
        XCTAssertEqual(
            (seedResult["attemptPlan"] as? [String: Any])?["clientPlanAction"] as? String,
            "applyLocalAck"
        )
        XCTAssertEqual(
            (seedResult["scheduleState"] as? [String: Any])?["planType"] as? String,
            "cloudCycleScheduleState"
        )
        XCTAssertEqual(
            (seedResult["loopPlan"] as? [String: Any])?["planType"] as? String,
            "cloudCycleLoopPlan"
        )
        XCTAssertEqual((seedResult["loopPlan"] as? [String: Any])?["roundCount"] as? Int, 1)
        XCTAssertEqual(
            (seedResult["loopPlan"] as? [String: Any])?["finalAction"] as? String,
            "applyLocalAck"
        )
        XCTAssertEqual(
            (seedResult["providerDiagnostics"] as? [String: Any])?["planType"] as? String,
            "cloudProviderDiagnostics"
        )
        XCTAssertEqual(
            (seedResult["providerDiagnostics"] as? [String: Any])?["cycleClassification"] as? String,
            "success"
        )
        XCTAssertEqual(
            (seedResult["providerDiagnostics"] as? [String: Any])?["compatibilityClassification"] as? String,
            "compatible"
        )
        XCTAssertEqual(
            (seedResult["observabilityEvent"] as? [String: Any])?["planType"] as? String,
            "cloudObservabilityEvent"
        )
        XCTAssertEqual(
            (seedResult["observabilityEvent"] as? [String: Any])?["classification"] as? String,
            "success"
        )
        XCTAssertEqual((seedResult["observabilityEvent"] as? [String: Any])?["redactedExportAllowed"] as? Bool, true)
        XCTAssertTrue(FileManager.default.fileExists(
            atPath: cloudRootURL.appendingPathComponent("Actro/BrowserSync/manifest.json").path
        ))

        let localDatabase = try AppleBrowserStorageDatabase(path: localDatabaseURL.path)
        let localClient = AppleBrowserStorageSyncClient(database: localDatabase)
        let pullResult = try jsonObject(await localClient.runLocalFolderSyncCycle(
            rootDir: cloudRootURL.path,
            scope: "profile",
            deviceId: "device-apple",
            generatedAtUnixSeconds: 1_720,
            limitPerCollection: 20,
            observedAtUnixSeconds: 1_720
        ))

        XCTAssertEqual(pullResult["endpointType"] as? String, "cloudLocalFolderCycle")
        XCTAssertEqual(
            (pullResult["capabilityPreflight"] as? [String: Any])?["classification"] as? String,
            "ready"
        )
        XCTAssertEqual(
            (pullResult["compatibilityPreflight"] as? [String: Any])?["planType"] as? String,
            "cloudCompatibilityPreflight"
        )
        XCTAssertEqual(
            (pullResult["compatibilityPreflight"] as? [String: Any])?["classification"] as? String,
            "compatible"
        )
        XCTAssertGreaterThan((pullResult["compatibilityPreflight"] as? [String: Any])?["documentCount"] as? Int ?? 0, 0)
        XCTAssertTrue(pullResult["schemaRolloutPlan"] is NSNull)
        XCTAssertTrue(pullResult["schemaMigrationPlaybook"] is NSNull)
        XCTAssertTrue(pullResult["recoverySnapshotRound"] is NSNull)
        XCTAssertEqual(pullResult["classification"] as? String, "success")
        XCTAssertEqual(pullResult["applied"] as? Bool, true)
        XCTAssertEqual((pullResult["applyResult"] as? [String: Any])?["remoteApplied"] as? Bool, true)
        XCTAssertEqual(
            (pullResult["compactionRound"] as? [String: Any])?["planType"] as? String,
            "compactionRound"
        )
        XCTAssertEqual((pullResult["compactionRound"] as? [String: Any])?["completed"] as? Bool, true)
        XCTAssertEqual(
            (pullResult["compactionRound"] as? [String: Any])?["clientAction"] as? String,
            "nothingToCompact"
        )
        XCTAssertEqual(
            (pullResult["cleanupRound"] as? [String: Any])?["planType"] as? String,
            "cleanupRound"
        )
        XCTAssertEqual((pullResult["cleanupRound"] as? [String: Any])?["cleanupCompleted"] as? Bool, true)
        XCTAssertEqual(
            (pullResult["cleanupRound"] as? [String: Any])?["clientAction"] as? String,
            "nothingToCleanup"
        )
        XCTAssertEqual(
            (pullResult["attemptPlan"] as? [String: Any])?["clientPlanAction"] as? String,
            "applyRemoteBatch"
        )
        XCTAssertEqual(
            (pullResult["scheduleState"] as? [String: Any])?["planType"] as? String,
            "cloudCycleScheduleState"
        )
        XCTAssertEqual(
            (pullResult["loopPlan"] as? [String: Any])?["planType"] as? String,
            "cloudCycleLoopPlan"
        )
        XCTAssertEqual(
            (pullResult["loopPlan"] as? [String: Any])?["finalAction"] as? String,
            "applyRemoteBatch"
        )
        XCTAssertEqual(
            (pullResult["providerDiagnostics"] as? [String: Any])?["planType"] as? String,
            "cloudProviderDiagnostics"
        )
        XCTAssertEqual(
            (pullResult["providerDiagnostics"] as? [String: Any])?["cycleClassification"] as? String,
            "success"
        )
        XCTAssertEqual(
            (pullResult["providerDiagnostics"] as? [String: Any])?["compatibilityClassification"] as? String,
            "compatible"
        )
        XCTAssertEqual(
            (pullResult["observabilityEvent"] as? [String: Any])?["planType"] as? String,
            "cloudObservabilityEvent"
        )
        XCTAssertEqual(
            (pullResult["observabilityEvent"] as? [String: Any])?["classification"] as? String,
            "success"
        )
        XCTAssertEqual((pullResult["observabilityEvent"] as? [String: Any])?["redactedExportAllowed"] as? Bool, true)
        XCTAssertEqual(
            queryScalarString(
                localDatabaseURL,
                "SELECT title FROM browser_history_pages WHERE address = 'https://example.com/apple-local-folder-remote'"
            ),
            "Remote Local Folder"
        )
        XCTAssertEqual(
            queryScalarString(
                localDatabaseURL,
                "SELECT COUNT(*) FROM browser_history_visits WHERE address = 'https://example.com/apple-local-folder-remote'"
            ),
            "1"
        )

        let manifestURL = cloudRootURL.appendingPathComponent("Actro/BrowserSync/manifest.json")
        var manifest = try jsonObject(String(data: Data(contentsOf: manifestURL), encoding: .utf8) ?? "{}")
        manifest["storageSchemaVersion"] = AppleBrowserStorageDatabase.schemaVersion + 1
        try JSONSerialization.data(withJSONObject: manifest, options: [.sortedKeys]).write(to: manifestURL)

        let blockedResult = try jsonObject(await localClient.runLocalFolderSyncCycle(
            rootDir: cloudRootURL.path,
            scope: "profile",
            deviceId: "device-apple",
            generatedAtUnixSeconds: 1_730,
            limitPerCollection: 20,
            observedAtUnixSeconds: 1_730
        ))
        XCTAssertEqual(blockedResult["classification"] as? String, "clientUpgradeRequired")
        XCTAssertEqual(blockedResult["applied"] as? Bool, false)
        XCTAssertTrue(blockedResult["cycleRound"] is NSNull)
        XCTAssertTrue(blockedResult["compactionRound"] is NSNull)
        XCTAssertTrue(blockedResult["cleanupRound"] is NSNull)
        XCTAssertEqual(
            (blockedResult["compatibilityPreflight"] as? [String: Any])?["planType"] as? String,
            "cloudCompatibilityPreflight"
        )
        XCTAssertEqual(
            (blockedResult["compatibilityPreflight"] as? [String: Any])?["classification"] as? String,
            "clientUpgradeRequired"
        )
        XCTAssertEqual((blockedResult["compatibilityPreflight"] as? [String: Any])?["shouldBlockSync"] as? Bool, true)
        let schemaRolloutPlan = try XCTUnwrap(blockedResult["schemaRolloutPlan"] as? [String: Any])
        XCTAssertEqual(schemaRolloutPlan["planType"] as? String, "cloudSchemaRolloutPlan")
        XCTAssertEqual(schemaRolloutPlan["classification"] as? String, "clientUpgradeRequired")
        XCTAssertEqual(schemaRolloutPlan["shouldBlockSync"] as? Bool, true)
        let schemaMigrationPlaybook = try XCTUnwrap(blockedResult["schemaMigrationPlaybook"] as? [String: Any])
        XCTAssertEqual(schemaMigrationPlaybook["planType"] as? String, "cloudSchemaMigrationPlaybook")
        XCTAssertEqual(schemaMigrationPlaybook["classification"] as? String, "clientUpgradeRequired")
        XCTAssertEqual(schemaMigrationPlaybook["requiresClientUpgrade"] as? Bool, true)
        XCTAssertEqual(schemaMigrationPlaybook["requiresProviderBackup"] as? Bool, false)
        let recoverySnapshotRound = try XCTUnwrap(blockedResult["recoverySnapshotRound"] as? [String: Any])
        XCTAssertEqual(recoverySnapshotRound["planType"] as? String, "recoverySnapshotRound")
        XCTAssertEqual(recoverySnapshotRound["completed"] as? Bool, true)
        XCTAssertEqual(recoverySnapshotRound["clientAction"] as? String, "upgradeClient")
        XCTAssertEqual(
            (recoverySnapshotRound["resultPlan"] as? [String: Any])?["planType"] as? String,
            "cloudRecoverySnapshotResult"
        )
        XCTAssertEqual(
            (recoverySnapshotRound["resultPlan"] as? [String: Any])?["snapshotRequested"] as? Bool,
            false
        )
        XCTAssertEqual(
            (blockedResult["providerDiagnostics"] as? [String: Any])?["classification"] as? String,
            "clientUpgradeRequired"
        )
        XCTAssertEqual(
            (blockedResult["providerDiagnostics"] as? [String: Any])?["compatibilityClassification"] as? String,
            "clientUpgradeRequired"
        )
    }

    func testSyncClientRunsLocalFolderCloudMembershipRound() async throws {
        let databaseURL = temporaryDatabaseURL()
        let cloudRootURL = temporaryDirectoryURL()
        defer {
            deleteDatabaseFiles(databaseURL)
            try? FileManager.default.removeItem(at: cloudRootURL)
        }

        let database = try AppleBrowserStorageDatabase(path: databaseURL.path)
        let client = AppleBrowserStorageSyncClient(database: database)
        let joinRequestJson = try jsonString([
            "scope": "profile",
            "actorDeviceId": "device-apple",
            "memberDeviceId": "device-apple",
            "displayName": "Actrium Apple",
            "platform": "Apple",
            "membershipAction": "connect",
            "generatedAtUnixSeconds": 1_810,
        ])
        let joinResult = try jsonObject(await client.runLocalFolderMembershipRound(
            rootDir: cloudRootURL.path,
            requestJson: joinRequestJson,
            observedAtUnixSeconds: 1_810
        ))

        XCTAssertEqual(joinResult["endpointType"] as? String, "cloudLocalFolderMembership")
        XCTAssertEqual(joinResult["providerKind"] as? String, "localFolder")
        XCTAssertEqual(
            (joinResult["capabilityPreflight"] as? [String: Any])?["classification"] as? String,
            "ready"
        )
        XCTAssertEqual(joinResult["classification"] as? String, "success")
        XCTAssertEqual(
            (joinResult["compatibilityPreflight"] as? [String: Any])?["classification"] as? String,
            "compatible"
        )
        XCTAssertEqual((joinResult["compatibilityPreflight"] as? [String: Any])?["documentCount"] as? Int, 0)
        XCTAssertTrue(joinResult["schemaRolloutPlan"] is NSNull)
        XCTAssertTrue(joinResult["schemaMigrationPlaybook"] is NSNull)
        XCTAssertTrue(joinResult["recoverySnapshotRound"] is NSNull)
        let joinStatusPolicy = try XCTUnwrap(joinResult["statusPolicy"] as? [String: Any])
        XCTAssertEqual(joinStatusPolicy["planType"] as? String, "cloudMembershipStatusPolicy")
        XCTAssertEqual(joinStatusPolicy["classification"] as? String, "success")
        XCTAssertEqual(joinStatusPolicy["clientAction"] as? String, "finishCloudDeviceJoined")
        let joinScheduleState = try XCTUnwrap(joinResult["scheduleState"] as? [String: Any])
        XCTAssertEqual(joinScheduleState["planType"] as? String, "cloudMembershipScheduleState")
        XCTAssertEqual(joinScheduleState["classification"] as? String, "success")
        XCTAssertEqual(joinScheduleState["schedulerAction"] as? String, "idle")
        XCTAssertEqual(joinScheduleState["nextRetryAttempt"] as? Int, 0)
        let joinAttemptPlan = try XCTUnwrap(joinResult["attemptPlan"] as? [String: Any])
        XCTAssertEqual(joinAttemptPlan["planType"] as? String, "cloudMembershipAttemptPlan")
        XCTAssertEqual(joinAttemptPlan["classification"] as? String, "success")
        XCTAssertEqual(joinAttemptPlan["attemptAction"] as? String, "finishCloudDeviceJoined")
        XCTAssertEqual(joinAttemptPlan["shouldFinishMembership"] as? Bool, true)
        let joinLoopPlan = try XCTUnwrap(joinResult["loopPlan"] as? [String: Any])
        XCTAssertEqual(joinLoopPlan["planType"] as? String, "cloudMembershipLoopPlan")
        XCTAssertEqual(joinLoopPlan["roundCount"] as? Int, 1)
        XCTAssertEqual(joinLoopPlan["finalAction"] as? String, "finishCloudDeviceJoined")
        let joinLoopSummary = try XCTUnwrap(joinLoopPlan["loopSummary"] as? [String: Any])
        XCTAssertEqual(joinLoopSummary["attemptCount"] as? Int, 1)
        XCTAssertEqual(joinLoopSummary["completedAttemptCount"] as? Int, 1)
        XCTAssertEqual(joinLoopSummary["finishMembershipAttemptCount"] as? Int, 1)
        XCTAssertEqual(joinLoopSummary["finalAction"] as? String, "finishCloudDeviceJoined")
        XCTAssertEqual(
            (joinResult["providerDiagnostics"] as? [String: Any])?["planType"] as? String,
            "cloudProviderDiagnostics"
        )
        XCTAssertEqual(
            (joinResult["providerDiagnostics"] as? [String: Any])?["compatibilityClassification"] as? String,
            "compatible"
        )
        XCTAssertEqual(
            (joinResult["providerDiagnostics"] as? [String: Any])?["scheduleClassification"] as? String,
            "success"
        )
        XCTAssertEqual(
            (joinResult["observabilityEvent"] as? [String: Any])?["planType"] as? String,
            "cloudObservabilityEvent"
        )
        XCTAssertEqual((joinResult["observabilityEvent"] as? [String: Any])?["finalAction"] as? String, "finishCloudDeviceJoined")
        XCTAssertEqual(joinResult["completed"] as? Bool, true)
        XCTAssertEqual(joinResult["shouldRetry"] as? Bool, false)
        XCTAssertEqual(joinResult["shouldReread"] as? Bool, false)
        XCTAssertEqual(joinResult["clientAction"] as? String, "finishCloudDeviceJoined")
        XCTAssertEqual(
            (joinResult["membershipRound"] as? [String: Any])?["planType"] as? String,
            "membershipRound"
        )
        XCTAssertEqual(
            (joinResult["membershipRound"] as? [String: Any])?["membershipAction"] as? String,
            "join"
        )
        XCTAssertTrue(FileManager.default.fileExists(
            atPath: cloudRootURL.appendingPathComponent("Actro/BrowserSync/manifest.json").path
        ))
        XCTAssertTrue(FileManager.default.fileExists(
            atPath: cloudRootURL.appendingPathComponent("Actro/BrowserSync/devices/device-apple.json").path
        ))

        let revokeRequestJson = try jsonString([
            "scope": "profile",
            "actorDeviceId": "device-apple",
            "memberDeviceId": "device-apple",
            "membershipAction": "revoke",
            "generatedAtUnixSeconds": 1_830,
        ])
        let revokeResult = try jsonObject(await client.runLocalFolderMembershipRound(
            rootDir: cloudRootURL.path,
            requestJson: revokeRequestJson,
            observedAtUnixSeconds: 1_830
        ))

        XCTAssertEqual(revokeResult["endpointType"] as? String, "cloudLocalFolderMembership")
        XCTAssertEqual(revokeResult["classification"] as? String, "success")
        XCTAssertEqual(
            (revokeResult["compatibilityPreflight"] as? [String: Any])?["classification"] as? String,
            "compatible"
        )
        XCTAssertGreaterThan((revokeResult["compatibilityPreflight"] as? [String: Any])?["documentCount"] as? Int ?? 0, 0)
        XCTAssertEqual(
            (revokeResult["providerDiagnostics"] as? [String: Any])?["compatibilityClassification"] as? String,
            "compatible"
        )
        XCTAssertEqual(
            (revokeResult["observabilityEvent"] as? [String: Any])?["planType"] as? String,
            "cloudObservabilityEvent"
        )
        XCTAssertTrue(revokeResult["schemaRolloutPlan"] is NSNull)
        XCTAssertTrue(revokeResult["schemaMigrationPlaybook"] is NSNull)
        XCTAssertTrue(revokeResult["recoverySnapshotRound"] is NSNull)
        let revokeStatusPolicy = try XCTUnwrap(revokeResult["statusPolicy"] as? [String: Any])
        XCTAssertEqual(revokeStatusPolicy["planType"] as? String, "cloudMembershipStatusPolicy")
        XCTAssertEqual(revokeStatusPolicy["classification"] as? String, "success")
        XCTAssertEqual(revokeStatusPolicy["clientAction"] as? String, "finishCloudDeviceRevoked")
        let revokeScheduleState = try XCTUnwrap(revokeResult["scheduleState"] as? [String: Any])
        XCTAssertEqual(revokeScheduleState["planType"] as? String, "cloudMembershipScheduleState")
        XCTAssertEqual(revokeScheduleState["classification"] as? String, "success")
        XCTAssertEqual(revokeScheduleState["schedulerAction"] as? String, "idle")
        let revokeAttemptPlan = try XCTUnwrap(revokeResult["attemptPlan"] as? [String: Any])
        XCTAssertEqual(revokeAttemptPlan["planType"] as? String, "cloudMembershipAttemptPlan")
        XCTAssertEqual(revokeAttemptPlan["classification"] as? String, "success")
        XCTAssertEqual(revokeAttemptPlan["attemptAction"] as? String, "finishCloudDeviceRevoked")
        let revokeLoopPlan = try XCTUnwrap(revokeResult["loopPlan"] as? [String: Any])
        XCTAssertEqual(revokeLoopPlan["planType"] as? String, "cloudMembershipLoopPlan")
        XCTAssertEqual(revokeLoopPlan["roundCount"] as? Int, 1)
        XCTAssertEqual(revokeLoopPlan["finalAction"] as? String, "finishCloudDeviceRevoked")
        let revokeLoopSummary = try XCTUnwrap(revokeLoopPlan["loopSummary"] as? [String: Any])
        XCTAssertEqual(revokeLoopSummary["attemptCount"] as? Int, 1)
        XCTAssertEqual(revokeLoopSummary["finishMembershipAttemptCount"] as? Int, 1)
        XCTAssertEqual(revokeLoopSummary["finalAction"] as? String, "finishCloudDeviceRevoked")
        XCTAssertEqual(revokeResult["completed"] as? Bool, true)
        XCTAssertEqual(revokeResult["clientAction"] as? String, "finishCloudDeviceRevoked")
        XCTAssertEqual(
            (revokeResult["membershipRound"] as? [String: Any])?["membershipAction"] as? String,
            "revoke"
        )
        XCTAssertEqual(
            (revokeResult["membershipRound"] as? [String: Any])?["memberStatus"] as? String,
            "revoked"
        )

        let manifestURL = cloudRootURL.appendingPathComponent("Actro/BrowserSync/manifest.json")
        var manifest = try jsonObject(String(contentsOf: manifestURL, encoding: .utf8))
        let revokedDevice = (manifest["devices"] as? [[String: Any]])?.contains { device in
            device["deviceId"] as? String == "device-apple"
                && device["status"] as? String == "revoked"
        }
        XCTAssertTrue(revokedDevice ?? false)

        manifest["storageSchemaVersion"] = AppleBrowserStorageDatabase.schemaVersion + 1
        try jsonString(manifest).write(to: manifestURL, atomically: true, encoding: .utf8)
        let blockedJoinRequestJson = try jsonString([
            "scope": "profile",
            "actorDeviceId": "device-apple",
            "memberDeviceId": "device-apple-2",
            "displayName": "Actrium Apple 2",
            "platform": "Apple",
            "membershipAction": "connect",
            "generatedAtUnixSeconds": 1_840,
        ])
        let blockedJoinResult = try jsonObject(await client.runLocalFolderMembershipRound(
            rootDir: cloudRootURL.path,
            requestJson: blockedJoinRequestJson,
            observedAtUnixSeconds: 1_840
        ))
        XCTAssertEqual(blockedJoinResult["classification"] as? String, "clientUpgradeRequired")
        XCTAssertEqual(blockedJoinResult["completed"] as? Bool, false)
        XCTAssertEqual(
            (blockedJoinResult["compatibilityPreflight"] as? [String: Any])?["shouldBlockSync"] as? Bool,
            true
        )
        XCTAssertEqual(
            (blockedJoinResult["membershipRound"] as? [String: Any])?["planningErrorKind"] as? String,
            "clientUpgradeRequired"
        )
        let blockedStatusPolicy = try XCTUnwrap(blockedJoinResult["statusPolicy"] as? [String: Any])
        XCTAssertEqual(blockedStatusPolicy["planType"] as? String, "cloudMembershipStatusPolicy")
        XCTAssertEqual(blockedStatusPolicy["classification"] as? String, "clientUpgradeRequired")
        XCTAssertEqual(blockedStatusPolicy["clientAction"] as? String, "upgradeClient")
        let blockedScheduleState = try XCTUnwrap(blockedJoinResult["scheduleState"] as? [String: Any])
        XCTAssertEqual(blockedScheduleState["planType"] as? String, "cloudMembershipScheduleState")
        XCTAssertEqual(blockedScheduleState["classification"] as? String, "clientUpgradeRequired")
        XCTAssertEqual(blockedScheduleState["requiresUserAction"] as? Bool, true)
        XCTAssertEqual(blockedScheduleState["userAction"] as? String, "upgradeClient")
        let blockedAttemptPlan = try XCTUnwrap(blockedJoinResult["attemptPlan"] as? [String: Any])
        XCTAssertEqual(blockedAttemptPlan["planType"] as? String, "cloudMembershipAttemptPlan")
        XCTAssertEqual(blockedAttemptPlan["classification"] as? String, "clientUpgradeRequired")
        XCTAssertEqual(blockedAttemptPlan["attemptAction"] as? String, "upgradeClient")
        XCTAssertEqual(blockedAttemptPlan["requiresUserAction"] as? Bool, true)
        XCTAssertEqual(blockedAttemptPlan["shouldStopUntilRecovery"] as? Bool, true)
        let blockedLoopPlan = try XCTUnwrap(blockedJoinResult["loopPlan"] as? [String: Any])
        XCTAssertEqual(blockedLoopPlan["planType"] as? String, "cloudMembershipLoopPlan")
        XCTAssertEqual(blockedLoopPlan["roundCount"] as? Int, 1)
        XCTAssertEqual(blockedLoopPlan["finalAction"] as? String, "upgradeClient")
        XCTAssertEqual(blockedLoopPlan["shouldStopUntilRecovery"] as? Bool, true)
        let blockedLoopSummary = try XCTUnwrap(blockedLoopPlan["loopSummary"] as? [String: Any])
        XCTAssertEqual(blockedLoopSummary["attemptCount"] as? Int, 1)
        XCTAssertEqual(blockedLoopSummary["stopUntilRecoveryAttemptCount"] as? Int, 1)
        XCTAssertEqual(blockedLoopSummary["finalAction"] as? String, "upgradeClient")
        XCTAssertEqual(blockedLoopSummary["finalClassification"] as? String, "clientUpgradeRequired")
        XCTAssertEqual(
            (blockedJoinResult["providerDiagnostics"] as? [String: Any])?["classification"] as? String,
            "clientUpgradeRequired"
        )
        XCTAssertEqual(
            (blockedJoinResult["providerDiagnostics"] as? [String: Any])?["compatibilityClassification"] as? String,
            "clientUpgradeRequired"
        )
        XCTAssertEqual(
            (blockedJoinResult["providerDiagnostics"] as? [String: Any])?["scheduleClassification"] as? String,
            "clientUpgradeRequired"
        )
        XCTAssertEqual(
            (blockedJoinResult["observabilityEvent"] as? [String: Any])?["planType"] as? String,
            "cloudObservabilityEvent"
        )
        let blockedMembershipRollout = try XCTUnwrap(blockedJoinResult["schemaRolloutPlan"] as? [String: Any])
        XCTAssertEqual(blockedMembershipRollout["planType"] as? String, "cloudSchemaRolloutPlan")
        XCTAssertEqual(blockedMembershipRollout["classification"] as? String, "clientUpgradeRequired")
        let blockedMembershipPlaybook = try XCTUnwrap(blockedJoinResult["schemaMigrationPlaybook"] as? [String: Any])
        XCTAssertEqual(blockedMembershipPlaybook["planType"] as? String, "cloudSchemaMigrationPlaybook")
        XCTAssertEqual(blockedMembershipPlaybook["classification"] as? String, "clientUpgradeRequired")
        XCTAssertEqual(blockedMembershipPlaybook["requiresProviderBackup"] as? Bool, false)
        let blockedMembershipRecoveryRound = try XCTUnwrap(blockedJoinResult["recoverySnapshotRound"] as? [String: Any])
        XCTAssertEqual(blockedMembershipRecoveryRound["planType"] as? String, "recoverySnapshotRound")
        XCTAssertEqual(blockedMembershipRecoveryRound["completed"] as? Bool, true)
        XCTAssertEqual(blockedMembershipRecoveryRound["clientAction"] as? String, "upgradeClient")
        XCTAssertEqual(
            (blockedMembershipRecoveryRound["resultPlan"] as? [String: Any])?["snapshotRequested"] as? Bool,
            false
        )
        XCTAssertFalse(FileManager.default.fileExists(
            atPath: cloudRootURL.appendingPathComponent("Actro/BrowserSync/devices/device-apple-2.json").path
        ))
    }

    func testSyncCoordinatorSkipsCycleWhenCredentialsUnavailable() async throws {
        let databaseURL = temporaryDatabaseURL()
        defer { deleteDatabaseFiles(databaseURL) }

        let database = try AppleBrowserStorageDatabase(path: databaseURL.path)
        let transport = AppleBrowserStorageSyncFakeTransport(responseJson: "{}")
        let coordinator = AppleBrowserStorageSyncCoordinator(
            client: AppleBrowserStorageSyncClient(database: database, transport: transport),
            credentialsProvider: AppleBrowserStorageSyncStaticCredentialsProvider(credentials: nil)
        )

        let result = try jsonObject(await coordinator.runSyncCycle(
            generatedAtUnixSeconds: 1_400,
            limitPerCollection: 20,
            observedAtUnixSeconds: 1_500
        ))

        XCTAssertFalse(result["credentialsAvailable"] as? Bool ?? true)
        XCTAssertEqual(result["classification"] as? String, "credentialsUnavailable")
        XCTAssertFalse(result["shouldRetry"] as? Bool ?? true)
        XCTAssertEqual(result["clientAction"] as? String, "refreshCredentials")
        XCTAssertEqual(
            (result["statusPolicy"] as? [String: Any])?["schedulerAction"] as? String,
            "waitForCredentials"
        )
        XCTAssertEqual(
            (result["statusPolicy"] as? [String: Any])?["credentialAction"] as? String,
            "refreshUserSession"
        )
        XCTAssertEqual(
            (result["scheduleState"] as? [String: Any])?["schedulerAction"] as? String,
            "waitForCredentials"
        )
        XCTAssertEqual(
            (result["scheduleState"] as? [String: Any])?["nextRetryAttempt"] as? Int,
            0
        )
        XCTAssertTrue((result["scheduleState"] as? [String: Any])?["nextRunAtUnixSeconds"] is NSNull)
        XCTAssertEqual(
            (result["scheduleState"] as? [String: Any])?["waitingForCredentials"] as? Bool,
            true
        )
        XCTAssertNil(transport.lastRequest)
    }

    func testSyncCoordinatorRunsClientWithAvailableCredentials() async throws {
        let databaseURL = temporaryDatabaseURL()
        defer { deleteDatabaseFiles(databaseURL) }

        let database = try AppleBrowserStorageDatabase(path: databaseURL.path)
        let cycleResponseJson = browserStorageSyncCycleResponseJson(
            remoteBatchJson: emptyBrowserStorageSyncBatchJson(
                scope: "profile",
                deviceId: "device-apple",
                generatedAtUnixSeconds: 1_410
            ),
            ackJson: """
                {
                    "version": 1,
                    "storageSchemaVersion": 6,
                    "scope": "profile",
                    "deviceId": "device-apple",
                    "syncedAtUnixSeconds": 1411,
                    "records": [],
                    "cursors": []
                }
                """
        )
        let transport = AppleBrowserStorageSyncFakeTransport(responseJson: cycleResponseJson)
        let coordinator = AppleBrowserStorageSyncCoordinator(
            client: AppleBrowserStorageSyncClient(database: database, transport: transport),
            credentialsProvider: AppleBrowserStorageSyncStaticCredentialsProvider(
                credentials: AppleBrowserStorageSyncCredentials(
                    baseUrl: "http://127.0.0.1:8787/",
                    accessToken: "token-apple-coordinator",
                    scope: "profile",
                    deviceId: "device-apple"
                )
            )
        )

        let result = try jsonObject(await coordinator.runSyncCycle(
            generatedAtUnixSeconds: 1_412,
            limitPerCollection: 20,
            retryAttempt: 3,
            observedAtUnixSeconds: 1_512
        ))

        XCTAssertTrue(result["credentialsAvailable"] as? Bool ?? false)
        XCTAssertEqual(result["classification"] as? String, "success")
        XCTAssertEqual(transport.lastRequest?.headers["Authorization"], "Bearer token-apple-coordinator")
        XCTAssertEqual(
            (result["statusPolicy"] as? [String: Any])?["schedulerAction"] as? String,
            "idle"
        )
        XCTAssertEqual(
            (result["statusPolicy"] as? [String: Any])?["resetRetryAttempt"] as? Bool,
            true
        )
        XCTAssertEqual(
            (result["scheduleState"] as? [String: Any])?["schedulerAction"] as? String,
            "idle"
        )
        XCTAssertEqual(
            (result["scheduleState"] as? [String: Any])?["nextRetryAttempt"] as? Int,
            0
        )
        XCTAssertTrue((result["scheduleState"] as? [String: Any])?["nextRunAtUnixSeconds"] is NSNull)
        XCTAssertEqual(
            (result["syncResult"] as? [String: Any])?["classification"] as? String,
            "success"
        )
    }

    func testSyncCoordinatorSchedulesRetryFromStatusPolicy() async throws {
        let databaseURL = temporaryDatabaseURL()
        defer { deleteDatabaseFiles(databaseURL) }

        let database = try AppleBrowserStorageDatabase(path: databaseURL.path)
        let transport = AppleBrowserStorageSyncFakeTransport(responseJson: "", statusCode: 503)
        let coordinator = AppleBrowserStorageSyncCoordinator(
            client: AppleBrowserStorageSyncClient(database: database, transport: transport),
            credentialsProvider: AppleBrowserStorageSyncStaticCredentialsProvider(
                credentials: AppleBrowserStorageSyncCredentials(
                    baseUrl: "http://127.0.0.1:8787/",
                    accessToken: "token-apple-retry",
                    scope: "profile",
                    deviceId: "device-apple"
                )
            )
        )

        let result = try jsonObject(await coordinator.runSyncCycle(
            generatedAtUnixSeconds: 1_420,
            limitPerCollection: 20,
            retryAttempt: 2,
            observedAtUnixSeconds: 2_000
        ))

        XCTAssertEqual(result["classification"] as? String, "serverUnavailable")
        XCTAssertEqual(
            (result["statusPolicy"] as? [String: Any])?["schedulerAction"] as? String,
            "scheduleRetry"
        )
        XCTAssertEqual(
            (result["scheduleState"] as? [String: Any])?["nextRetryAttempt"] as? Int,
            3
        )
        XCTAssertEqual(
            (result["scheduleState"] as? [String: Any])?["nextRunDelaySeconds"] as? Int,
            20
        )
        XCTAssertEqual(
            (result["scheduleState"] as? [String: Any])?["nextRunAtUnixSeconds"] as? Int,
            2_020
        )
    }

    func testSyncSchedulerConsumesScheduleStateAndResetsAfterSuccess() async throws {
        let databaseURL = temporaryDatabaseURL()
        defer { deleteDatabaseFiles(databaseURL) }

        let database = try AppleBrowserStorageDatabase(path: databaseURL.path)
        let transport = AppleBrowserStorageSyncFakeTransport(responseJson: "", statusCode: 503)
        let coordinator = AppleBrowserStorageSyncCoordinator(
            client: AppleBrowserStorageSyncClient(database: database, transport: transport),
            credentialsProvider: AppleBrowserStorageSyncStaticCredentialsProvider(
                credentials: AppleBrowserStorageSyncCredentials(
                    baseUrl: "http://127.0.0.1:8787/",
                    accessToken: "token-apple-scheduler",
                    scope: "profile",
                    deviceId: "device-apple"
                )
            )
        )
        let scheduler = AppleBrowserStorageSyncScheduler(coordinator: coordinator)

        _ = await scheduler.runNow(
            generatedAtUnixSeconds: 1_430,
            limitPerCollection: 20,
            observedAtUnixSeconds: 3_000
        )
        let retrySnapshot = await scheduler.snapshot()
        XCTAssertEqual(retrySnapshot.schedulerAction, "scheduleRetry")
        XCTAssertEqual(retrySnapshot.retryAttempt, 1)
        XCTAssertEqual(retrySnapshot.nextRunDelaySeconds, 5)
        XCTAssertEqual(retrySnapshot.nextRunAtUnixSeconds, 3_005)
        XCTAssertTrue(retrySnapshot.pendingRunScheduled)

        let retryStatus = try jsonObject(await scheduler.statusJson())
        XCTAssertEqual(retryStatus["status"] as? String, "retryScheduled")
        XCTAssertEqual(retryStatus["lastClassification"] as? String, "serverUnavailable")
        XCTAssertEqual(retryStatus["pendingRunScheduled"] as? Bool, true)

        await scheduler.cancelPendingSync()

        transport.statusCode = 200
        transport.responseJson = browserStorageSyncCycleResponseJson(
            remoteBatchJson: emptyBrowserStorageSyncBatchJson(
                scope: "profile",
                deviceId: "device-apple",
                generatedAtUnixSeconds: 1_440
            ),
            ackJson: """
                {
                    "version": 1,
                    "storageSchemaVersion": 6,
                    "scope": "profile",
                    "deviceId": "device-apple",
                    "syncedAtUnixSeconds": 1441,
                    "records": [],
                    "cursors": []
                }
                """
        )

        _ = await scheduler.runNow(
            generatedAtUnixSeconds: 1_442,
            limitPerCollection: 20,
            observedAtUnixSeconds: 3_010
        )
        let successSnapshot = await scheduler.snapshot()
        XCTAssertEqual(successSnapshot.schedulerAction, "idle")
        XCTAssertEqual(successSnapshot.retryAttempt, 0)
        XCTAssertNil(successSnapshot.nextRunAtUnixSeconds)
        XCTAssertFalse(successSnapshot.pendingRunScheduled)

        let successStatus = try jsonObject(await scheduler.statusJson())
        XCTAssertEqual(successStatus["status"] as? String, "idle")
        XCTAssertEqual(successStatus["retryAttempt"] as? Int, 0)
        XCTAssertEqual(successStatus["pendingRunScheduled"] as? Bool, false)
    }
}

private final class AppleBrowserStorageSyncFakeTransport: AppleBrowserStorageSyncTransport {
    var responseJson: String
    var statusCode: Int
    private(set) var lastRequest: AppleBrowserStorageSyncHTTPRequest?

    init(responseJson: String, statusCode: Int = 200) {
        self.responseJson = responseJson
        self.statusCode = statusCode
    }

    func send(_ request: AppleBrowserStorageSyncHTTPRequest) async -> AppleBrowserStorageSyncHTTPResponse {
        lastRequest = request
        return AppleBrowserStorageSyncHTTPResponse(statusCode: statusCode, bodyJson: responseJson)
    }
}

private struct AppleBrowserStorageSyncStaticCredentialsProvider: AppleBrowserStorageSyncCredentialsProvider {
    let credentials: AppleBrowserStorageSyncCredentials?

    func browserStorageSyncCredentials() async -> AppleBrowserStorageSyncCredentials? {
        credentials
    }
}

private func temporaryDatabaseURL() -> URL {
    FileManager.default.temporaryDirectory
        .appendingPathComponent("actrium-apple-tests-\(UUID().uuidString).db")
}

private func temporaryDirectoryURL() -> URL {
    FileManager.default.temporaryDirectory
        .appendingPathComponent("actrium-apple-tests-\(UUID().uuidString)")
}

private func deleteDatabaseFiles(_ url: URL) {
    let fileManager = FileManager.default
    try? fileManager.removeItem(at: url)
    try? fileManager.removeItem(at: URL(fileURLWithPath: "\(url.path)-shm"))
    try? fileManager.removeItem(at: URL(fileURLWithPath: "\(url.path)-wal"))
}

private func createLegacyBrowserStorageDatabase(at url: URL, schemaVersion: Int) throws {
    let database = try AppleSQLiteDatabase(path: url.path)
    let syncColumns = schemaVersion >= 3
        ? """
          ,
              sync_state TEXT NOT NULL DEFAULT 'local',
              sync_revision INTEGER NOT NULL DEFAULT 0,
              origin_device_id TEXT NOT NULL DEFAULT '',
              last_synced_at INTEGER,
              deleted_at INTEGER
          """
        : ""
    let tabIdColumn = schemaVersion >= 2
        ? "tab_id TEXT NOT NULL DEFAULT '',"
        : ""

    execute(database, "PRAGMA foreign_keys = OFF")
    execute(database, """
        CREATE TABLE browser_schema_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )
        """)
    execute(database, "INSERT INTO browser_schema_meta (key, value) VALUES ('browser_storage_schema_version', '\(schemaVersion)')")

    execute(database, """
        CREATE TABLE browser_history_pages (
            address TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            visit_count INTEGER NOT NULL DEFAULT 1,
            last_visited_at INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            favicon TEXT\(syncColumns)
        )
        """)
    execute(database, """
        CREATE TABLE browser_history_visits (
            id TEXT PRIMARY KEY,
            address TEXT NOT NULL,
            title TEXT NOT NULL,
            visited_at INTEGER NOT NULL,
            transition TEXT\(syncColumns)
        )
        """)
    execute(database, """
        CREATE TABLE browser_bookmark_folders (
            id TEXT PRIMARY KEY,
            parent_id TEXT,
            title TEXT NOT NULL,
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL\(syncColumns)
        )
        """)
    execute(database, """
        CREATE TABLE browser_bookmarks (
            id TEXT PRIMARY KEY,
            folder_id TEXT,
            title TEXT NOT NULL,
            address TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            sort_order INTEGER NOT NULL DEFAULT 0,
            favicon TEXT,
            is_deleted INTEGER NOT NULL DEFAULT 0\(syncColumns)
        )
        """)
    execute(database, """
        CREATE TABLE browser_sessions (
            id TEXT PRIMARY KEY,
            active_tab_index INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL\(syncColumns)
        )
        """)
    execute(database, """
        CREATE TABLE browser_session_tabs (
            session_id TEXT NOT NULL,
            tab_index INTEGER NOT NULL,
            \(tabIdColumn)
            title TEXT NOT NULL,
            address TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL\(syncColumns),
            PRIMARY KEY (session_id, tab_index)
        )
        """)

    execute(database, """
        INSERT INTO browser_history_pages (address, title, visit_count, last_visited_at, created_at, updated_at, favicon)
        VALUES ('https://example.com/history-v\(schemaVersion)', 'History v\(schemaVersion)', 2, \(100 + schemaVersion), \(90 + schemaVersion), \(100 + schemaVersion), NULL)
        """)
    execute(database, """
        INSERT INTO browser_history_visits (id, address, title, visited_at, transition)
        VALUES ('visit-v\(schemaVersion)', 'https://example.com/history-v\(schemaVersion)', 'History v\(schemaVersion)', \(100 + schemaVersion), 'typed')
        """)
    execute(database, """
        INSERT INTO browser_bookmarks (id, folder_id, title, address, created_at, updated_at, sort_order, favicon, is_deleted)
        VALUES ('bookmark-v\(schemaVersion)', NULL, 'Bookmark v\(schemaVersion)', 'https://example.com/bookmark-v\(schemaVersion)', \(200 + schemaVersion), \(200 + schemaVersion), 0, NULL, 0)
        """)
    execute(database, """
        INSERT INTO browser_sessions (id, active_tab_index, created_at, updated_at)
        VALUES ('default', 0, \(300 + schemaVersion), \(300 + schemaVersion))
        """)

    if schemaVersion >= 2 {
        execute(database, """
            INSERT INTO browser_session_tabs (session_id, tab_index, tab_id, title, address, created_at, updated_at)
            VALUES ('default', 0, 'tab-v\(schemaVersion)', 'Session v\(schemaVersion)', 'https://example.com/session-v\(schemaVersion)', \(300 + schemaVersion), \(300 + schemaVersion))
            """)
    } else {
        execute(database, """
            INSERT INTO browser_session_tabs (session_id, tab_index, title, address, created_at, updated_at)
            VALUES ('default', 0, 'Session v\(schemaVersion)', 'https://example.com/session-v\(schemaVersion)', \(300 + schemaVersion), \(300 + schemaVersion))
            """)
    }
}

private func assertBrowserStorageV6Shape(_ url: URL, legacyVersion: Int) throws {
    XCTAssertEqual(queryScalarString(url, "SELECT value FROM browser_schema_meta WHERE key = 'browser_storage_schema_version'"), "6")
    XCTAssertTrue(tableExists(url, "browser_sync_cursors"))
    XCTAssertTrue(indexExists(url, "idx_browser_sync_cursors_updated"))
    XCTAssertTrue(indexExists(url, "idx_browser_session_tabs_tab_id"))

    for table in [
        "browser_history_pages",
        "browser_history_visits",
        "browser_bookmark_folders",
        "browser_bookmarks",
        "browser_sessions",
        "browser_session_tabs",
    ] {
        XCTAssertTrue(columnExists(url, table: table, column: "sync_state"))
        XCTAssertTrue(columnExists(url, table: table, column: "sync_revision"))
        XCTAssertTrue(columnExists(url, table: table, column: "origin_device_id"))
        XCTAssertTrue(columnExists(url, table: table, column: "last_synced_at"))
        XCTAssertTrue(columnExists(url, table: table, column: "deleted_at"))
    }

    XCTAssertTrue(columnExists(url, table: "browser_session_tabs", column: "tab_id"))
    XCTAssertTrue(columnExists(url, table: "browser_bookmarks", column: "owner_kind"))
    XCTAssertTrue(columnExists(url, table: "browser_bookmarks", column: "owner_id"))
    XCTAssertTrue(columnExists(url, table: "browser_sessions", column: "owner_kind"))
    XCTAssertTrue(columnExists(url, table: "browser_sessions", column: "owner_id"))
    XCTAssertTrue(columnExists(url, table: "browser_sessions", column: "surface_kind"))
    XCTAssertTrue(columnExists(url, table: "browser_sessions", column: "surface_id"))
    XCTAssertEqual(queryScalarString(url, "SELECT owner_kind FROM browser_bookmarks WHERE id = 'bookmark-v\(legacyVersion)'"), "user")
    XCTAssertEqual(queryScalarString(url, "SELECT owner_id FROM browser_bookmarks WHERE id = 'bookmark-v\(legacyVersion)'"), "")
    XCTAssertEqual(queryScalarString(url, "SELECT owner_kind FROM browser_sessions WHERE id = 'default'"), "device")
    XCTAssertEqual(queryScalarString(url, "SELECT owner_id FROM browser_sessions WHERE id = 'default'"), "")
    XCTAssertEqual(queryScalarString(url, "SELECT surface_kind FROM browser_sessions WHERE id = 'default'"), "browser")
    XCTAssertEqual(queryScalarString(url, "SELECT surface_id FROM browser_sessions WHERE id = 'default'"), "default")
}

private func batchHasRecord(_ batch: [String: Any], recordType: String, recordId: String? = nil) -> Bool {
    guard let records = batch["records"] as? [[String: Any]] else { return false }
    return records.contains { record in
        guard record["recordType"] as? String == recordType else { return false }
        return recordId == nil || record["recordId"] as? String == recordId
    }
}

private func batchHasDeletedBookmark(_ batch: [String: Any], recordId: String) -> Bool {
    guard let records = batch["records"] as? [[String: Any]] else { return false }
    return records.contains { record in
        guard record["recordType"] as? String == "bookmark",
              record["recordId"] as? String == recordId,
              record["deletedAtUnixSeconds"] is NSNumber,
              let payload = record["payload"] as? [String: Any]
        else {
            return false
        }
        return payload["isDeleted"] as? Bool == true
    }
}

private func batchHasBookmarkOwner(_ batch: [String: Any], recordId: String, ownerKind: String, ownerId: String) -> Bool {
    guard let records = batch["records"] as? [[String: Any]] else { return false }
    return records.contains { record in
        guard record["recordType"] as? String == "bookmark",
              record["recordId"] as? String == recordId,
              let payload = record["payload"] as? [String: Any]
        else {
            return false
        }
        return payload["ownerKind"] as? String == ownerKind &&
            payload["ownerId"] as? String == ownerId
    }
}

private func jsonObject(_ json: String) throws -> [String: Any] {
    let data = Data(json.utf8)
    guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
        XCTFail("Expected JSON object, got: \(json)")
        return [:]
    }
    return object
}

private func jsonString(_ object: Any) throws -> String {
    let data = try JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
    return String(data: data, encoding: .utf8) ?? "null"
}

private func tableExists(_ url: URL, _ table: String) -> Bool {
    queryScalarString(
        url,
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = '\(table)' LIMIT 1"
    ) == "1"
}

private func indexExists(_ url: URL, _ index: String) -> Bool {
    queryScalarString(
        url,
        "SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = '\(index)' LIMIT 1"
    ) == "1"
}

private func columnExists(_ url: URL, table: String, column: String) -> Bool {
    guard let database = try? AppleSQLiteDatabase(path: url.path) else { return false }
    return database.query("PRAGMA table_info(\(table))").contains { row in
        row["name"] as? String == column
    }
}

private func queryScalarString(_ url: URL, _ sql: String) -> String? {
    guard let database = try? AppleSQLiteDatabase(path: url.path),
          let value = database.queryScalar(sql)
    else {
        return nil
    }
    return "\(value)"
}

private func execute(_ database: AppleSQLiteDatabase, _ sql: String, file: StaticString = #filePath, line: UInt = #line) {
    XCTAssertTrue(database.execute(sql), file: file, line: line)
}
