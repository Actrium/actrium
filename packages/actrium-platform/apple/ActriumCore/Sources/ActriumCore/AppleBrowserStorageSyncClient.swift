import Foundation

public struct AppleBrowserStorageSyncHTTPRequest: Equatable {
    public var method: String
    public var url: URL
    public var headers: [String: String]
    public var bodyJson: String

    public init(method: String, url: URL, headers: [String: String], bodyJson: String) {
        self.method = method
        self.url = url
        self.headers = headers
        self.bodyJson = bodyJson
    }
}

public struct AppleBrowserStorageSyncHTTPResponse: Equatable {
    public var statusCode: Int
    public var bodyJson: String
    public var errorMessage: String?

    public init(statusCode: Int, bodyJson: String, errorMessage: String? = nil) {
        self.statusCode = statusCode
        self.bodyJson = bodyJson
        self.errorMessage = errorMessage
    }
}

public protocol AppleBrowserStorageSyncTransport {
    func send(_ request: AppleBrowserStorageSyncHTTPRequest) async -> AppleBrowserStorageSyncHTTPResponse
}

public struct AppleBrowserStorageURLSessionTransport: AppleBrowserStorageSyncTransport {
    public init() {}

    public func send(_ request: AppleBrowserStorageSyncHTTPRequest) async -> AppleBrowserStorageSyncHTTPResponse {
        var urlRequest = URLRequest(url: request.url)
        urlRequest.httpMethod = request.method
        request.headers.forEach { key, value in
            urlRequest.setValue(value, forHTTPHeaderField: key)
        }
        urlRequest.httpBody = request.bodyJson.data(using: .utf8)

        do {
            let (data, response) = try await URLSession.shared.data(for: urlRequest)
            let statusCode = (response as? HTTPURLResponse)?.statusCode ?? 0
            return AppleBrowserStorageSyncHTTPResponse(
                statusCode: statusCode,
                bodyJson: String(data: data, encoding: .utf8) ?? "",
                errorMessage: nil
            )
        } catch {
            return AppleBrowserStorageSyncHTTPResponse(
                statusCode: 0,
                bodyJson: "",
                errorMessage: error.localizedDescription
            )
        }
    }
}

public final class AppleBrowserStorageSyncClient {
    private let database: AppleBrowserStorageDatabase
    private let transport: AppleBrowserStorageSyncTransport

    public init(
        database: AppleBrowserStorageDatabase,
        transport: AppleBrowserStorageSyncTransport = AppleBrowserStorageURLSessionTransport()
    ) {
        self.database = database
        self.transport = transport
    }

    public func runSyncCycle(
        baseUrl: String,
        accessToken: String,
        scope: String = "default",
        deviceId: String,
        generatedAtUnixSeconds: Int64? = nil,
        limitPerCollection: Int = 200
    ) async -> String {
        let requestPlanJson = database.exportSyncCycleRequestPlan(
            baseUrl: baseUrl,
            accessToken: accessToken,
            scope: scope,
            deviceId: deviceId,
            generatedAtUnixSeconds: generatedAtUnixSeconds,
            limitPerCollection: limitPerCollection
        )

        guard requestPlanJson != "null",
              let requestPlan = AppleBrowserStorageSyncCycleRequestPlan(json: requestPlanJson),
              let request = requestPlan.httpRequest
        else {
            return appleBrowserStorageSyncClientResultJson(
                requestPlan: appleBrowserStorageJsonObject(requestPlanJson),
                transportResult: [
                    "classification": "clientPlanUnavailable",
                    "shouldRetry": false,
                    "clientAction": "skipCycle",
                ],
                applyResult: nil,
                applied: false
            )
        }

        let response = await transport.send(request)
        let transportResultJson = browserStorageSyncCycleTransportResultJson(
            statusCode: Int32(response.statusCode),
            responseJson: response.bodyJson,
            errorMessage: response.errorMessage ?? ""
        )
        let transportResult = appleBrowserStorageJsonObject(transportResultJson)
        var applyResult: Any?
        var applied = false

        if appleBrowserStorageJsonString(transportResult?["classification"]) == "success",
           let responseValue = transportResult?["response"],
           let responseJson = appleBrowserStorageJsonStringValue(responseValue) {
            let applyResultJson = database.applySyncCycleResponse(responseJson)
            if let applyObject = appleBrowserStorageJsonObject(applyResultJson) {
                applyResult = applyObject
                applied = true
            }
        }

        return appleBrowserStorageSyncClientResultJson(
            requestPlan: appleBrowserStorageJsonObject(requestPlanJson),
            transportResult: transportResult,
            applyResult: applyResult,
            applied: applied
        )
    }

    public func runLocalFolderSyncCycle(
        rootDir: String,
        scope: String = "default",
        deviceId: String,
        generatedAtUnixSeconds: Int64? = nil,
        limitPerCollection: Int = 200,
        retryAttempt: UInt32 = 0,
        observedAtUnixSeconds: Int64? = nil,
        maxAttempts: UInt32 = 3
    ) async -> String {
        let generatedAt = generatedAtUnixSeconds ?? Int64(Date().timeIntervalSince1970)
        let observedAt = observedAtUnixSeconds ?? generatedAt
        let capabilityPreflight = appleBrowserStorageLocalFolderCapabilityPreflight(observedAt: observedAt)
        if appleBrowserStorageJsonBool(capabilityPreflight?["shouldBlockSync"]) {
            let providerDiagnostics = appleBrowserStorageCloudProviderDiagnostics(
                capabilityPreflight: capabilityPreflight,
                compatibilityPreflight: nil,
                statusPolicy: nil,
                scheduleState: nil,
                observedAt: observedAt
            )
            let observabilityEvent = appleBrowserStorageCloudObservabilityEvent(
                providerDiagnostics: providerDiagnostics,
                attemptPlan: nil,
                loopPlan: nil,
                observedAt: observedAt
            )
            return appleBrowserStorageLocalFolderSyncClientResultJson(
                rootDir: rootDir,
                pushRequest: nil,
                capabilityPreflight: capabilityPreflight,
                compatibilityPreflight: nil,
                schemaRolloutPlan: nil,
                schemaMigrationPlaybook: nil,
                recoverySnapshotRound: nil,
                cycleRound: nil,
                statusPolicy: nil,
                scheduleState: nil,
                attemptPlan: nil,
                loopPlan: nil,
                applyResult: nil,
                compactionRound: nil,
                cleanupRound: nil,
                providerDiagnostics: providerDiagnostics,
                observabilityEvent: observabilityEvent,
                classification: appleBrowserStorageJsonString(capabilityPreflight?["classification"]),
                shouldRetry: false,
                clientAction: appleBrowserStorageJsonString(capabilityPreflight?["clientAction"]),
                applied: false
            )
        }
        let compatibilityPreflight = appleBrowserStorageLocalFolderCompatibilityPreflight(
            rootDir: rootDir,
            observedAt: observedAt
        )
        if appleBrowserStorageJsonBool(compatibilityPreflight?["shouldBlockSync"]) {
            let schemaRolloutPlan = appleBrowserStorageCloudSchemaRolloutPlan(
                compatibilityPreflight: compatibilityPreflight,
                observedAt: observedAt
            )
            let schemaMigrationPlaybook = appleBrowserStorageCloudSchemaMigrationPlaybook(
                schemaRolloutPlan: schemaRolloutPlan,
                observedAt: observedAt
            )
            let recoverySnapshotRound = appleBrowserStorageLocalFolderRecoverySnapshotRound(
                rootDir: rootDir,
                schemaMigrationPlaybook: schemaMigrationPlaybook,
                compatibilityPreflight: compatibilityPreflight,
                observedAt: observedAt
            )
            let providerDiagnostics = appleBrowserStorageCloudProviderDiagnostics(
                capabilityPreflight: capabilityPreflight,
                compatibilityPreflight: compatibilityPreflight,
                statusPolicy: nil,
                scheduleState: nil,
                observedAt: observedAt
            )
            let observabilityEvent = appleBrowserStorageCloudObservabilityEvent(
                providerDiagnostics: providerDiagnostics,
                attemptPlan: nil,
                loopPlan: nil,
                observedAt: observedAt
            )
            return appleBrowserStorageLocalFolderSyncClientResultJson(
                rootDir: rootDir,
                pushRequest: nil,
                capabilityPreflight: capabilityPreflight,
                compatibilityPreflight: compatibilityPreflight,
                schemaRolloutPlan: schemaRolloutPlan,
                schemaMigrationPlaybook: schemaMigrationPlaybook,
                recoverySnapshotRound: recoverySnapshotRound,
                cycleRound: nil,
                statusPolicy: nil,
                scheduleState: nil,
                attemptPlan: nil,
                loopPlan: nil,
                applyResult: nil,
                compactionRound: nil,
                cleanupRound: nil,
                providerDiagnostics: providerDiagnostics,
                observabilityEvent: observabilityEvent,
                classification: appleBrowserStorageJsonString(compatibilityPreflight?["classification"]),
                shouldRetry: false,
                clientAction: appleBrowserStorageJsonString(compatibilityPreflight?["clientAction"]),
                applied: false
            )
        }
        let pushRequestJson = database.exportDirtySyncPushRequest(
            scope: scope,
            deviceId: deviceId,
            generatedAtUnixSeconds: generatedAt,
            limitPerCollection: limitPerCollection
        )
        guard pushRequestJson != "null" else {
            let providerDiagnostics = appleBrowserStorageCloudProviderDiagnostics(
                capabilityPreflight: capabilityPreflight,
                compatibilityPreflight: compatibilityPreflight,
                statusPolicy: nil,
                scheduleState: nil,
                observedAt: observedAt
            )
            let observabilityEvent = appleBrowserStorageCloudObservabilityEvent(
                providerDiagnostics: providerDiagnostics,
                attemptPlan: nil,
                loopPlan: nil,
                observedAt: observedAt
            )
            return appleBrowserStorageLocalFolderSyncClientResultJson(
                rootDir: rootDir,
                pushRequest: appleBrowserStorageJsonObject(pushRequestJson),
                capabilityPreflight: capabilityPreflight,
                compatibilityPreflight: compatibilityPreflight,
                schemaRolloutPlan: nil,
                schemaMigrationPlaybook: nil,
                recoverySnapshotRound: nil,
                cycleRound: nil,
                statusPolicy: nil,
                scheduleState: nil,
                attemptPlan: nil,
                loopPlan: nil,
                applyResult: nil,
                compactionRound: nil,
                cleanupRound: nil,
                providerDiagnostics: providerDiagnostics,
                observabilityEvent: observabilityEvent,
                classification: "clientPlanUnavailable",
                shouldRetry: false,
                clientAction: "skipCloudCycle",
                applied: false
            )
        }

        let cycleRoundJson = browserStorageSyncCloudLocalFolderCycleRoundJson(
            rootDir: rootDir,
            pushRequestJson: pushRequestJson,
            cursorsJson: database.syncCursorsJson(scope: scope),
            deviceId: deviceId,
            generatedAtUnixSeconds: generatedAt,
            maxLogObjectsPerCollection: UInt32(max(1, min(limitPerCollection, 1_000)))
        )
        guard cycleRoundJson != "null",
              let cycleRound = appleBrowserStorageJsonObject(cycleRoundJson)
        else {
            let providerDiagnostics = appleBrowserStorageCloudProviderDiagnostics(
                capabilityPreflight: capabilityPreflight,
                compatibilityPreflight: compatibilityPreflight,
                statusPolicy: nil,
                scheduleState: nil,
                observedAt: observedAt
            )
            let observabilityEvent = appleBrowserStorageCloudObservabilityEvent(
                providerDiagnostics: providerDiagnostics,
                attemptPlan: nil,
                loopPlan: nil,
                observedAt: observedAt
            )
            return appleBrowserStorageLocalFolderSyncClientResultJson(
                rootDir: rootDir,
                pushRequest: appleBrowserStorageJsonObject(pushRequestJson),
                capabilityPreflight: capabilityPreflight,
                compatibilityPreflight: compatibilityPreflight,
                schemaRolloutPlan: nil,
                schemaMigrationPlaybook: nil,
                recoverySnapshotRound: nil,
                cycleRound: nil,
                statusPolicy: nil,
                scheduleState: nil,
                attemptPlan: nil,
                loopPlan: nil,
                applyResult: nil,
                compactionRound: nil,
                cleanupRound: nil,
                providerDiagnostics: providerDiagnostics,
                observabilityEvent: observabilityEvent,
                classification: "clientPlanUnavailable",
                shouldRetry: false,
                clientAction: "skipCloudCycle",
                applied: false
            )
        }

        let statusPolicy = appleBrowserStorageJsonObject(
            browserStorageSyncCloudCycleStatusPolicyJson(
                cycleRoundJson: cycleRoundJson,
                retryAttempt: retryAttempt
            )
        )
        let scheduleState = appleBrowserStorageJsonObject(
            browserStorageSyncCloudCycleScheduleStateJson(
                statusPolicyJson: appleBrowserStorageOptionalJsonString(statusPolicy),
                observedAtUnixSeconds: observedAt
            )
        )
        let attemptPlan = appleBrowserStorageJsonObject(
            browserStorageSyncCloudCycleAttemptPlanJson(
                cycleRoundJson: cycleRoundJson,
                retryAttempt: retryAttempt,
                observedAtUnixSeconds: observedAt
            )
        )
        let cycleRoundsJson = appleBrowserStorageJsonStringValue([cycleRound]) ?? "[]"
        let loopPlan = appleBrowserStorageJsonObject(
            browserStorageSyncCloudCycleLoopPlanJson(
                cycleRoundsJson: cycleRoundsJson,
                retryAttempt: retryAttempt,
                observedAtUnixSeconds: observedAt,
                maxAttempts: maxAttempts
            )
        )
        let applyResult = appleBrowserStorageJsonObject(
            database.applyCloudCycleClientPlan(cycleRoundJson)
        )
        let compactionRound = appleBrowserStorageJsonObject(
            browserStorageSyncCloudLocalFolderCompactionRoundJson(
                rootDir: rootDir,
                deviceId: deviceId,
                generatedAtUnixSeconds: generatedAt
            )
        )
        let cleanupRound = appleBrowserStorageJsonObject(
            browserStorageSyncCloudLocalFolderCleanupRoundJson(
                rootDir: rootDir,
                cursorsJson: database.syncCursorsJson(scope: scope),
                deviceId: deviceId,
                generatedAtUnixSeconds: generatedAt
            )
        )
        let providerErrorClassification = appleBrowserStorageLocalFolderProviderErrorClassification(
            cycleRound: cycleRound,
            compactionRound: compactionRound,
            cleanupRound: cleanupRound,
            retryAttempt: retryAttempt,
            observedAt: observedAt
        )
        let providerDiagnostics = appleBrowserStorageCloudProviderDiagnostics(
            capabilityPreflight: capabilityPreflight,
            compatibilityPreflight: compatibilityPreflight,
            providerErrorClassification: providerErrorClassification,
            statusPolicy: statusPolicy,
            scheduleState: scheduleState,
            observedAt: observedAt
        )
        let observabilityEvent = appleBrowserStorageCloudObservabilityEvent(
            providerDiagnostics: providerDiagnostics,
            attemptPlan: attemptPlan,
            loopPlan: loopPlan,
            observedAt: observedAt
        )
        let classification = appleBrowserStorageJsonString(statusPolicy?["classification"])
        let clientAction = appleBrowserStorageJsonString(statusPolicy?["clientAction"]).isEmpty
            ? appleBrowserStorageJsonString(cycleRound["clientAction"])
            : appleBrowserStorageJsonString(statusPolicy?["clientAction"])

        return appleBrowserStorageLocalFolderSyncClientResultJson(
            rootDir: rootDir,
            pushRequest: appleBrowserStorageJsonObject(pushRequestJson),
            capabilityPreflight: capabilityPreflight,
            compatibilityPreflight: compatibilityPreflight,
            schemaRolloutPlan: nil,
            schemaMigrationPlaybook: nil,
            recoverySnapshotRound: nil,
            cycleRound: cycleRound,
            statusPolicy: statusPolicy,
            scheduleState: scheduleState,
            attemptPlan: attemptPlan,
            loopPlan: loopPlan,
            applyResult: applyResult,
            compactionRound: compactionRound,
            cleanupRound: cleanupRound,
            providerDiagnostics: providerDiagnostics,
            observabilityEvent: observabilityEvent,
            classification: classification,
            shouldRetry: appleBrowserStorageJsonBool(statusPolicy?["shouldRetry"]),
            clientAction: clientAction,
            applied: appleBrowserStorageJsonBool(applyResult?["applied"])
        )
    }

    public func runLocalFolderMembershipRound(
        rootDir: String,
        requestJson: String,
        observedAtUnixSeconds: Int64? = nil
    ) async -> String {
        let observedAt = observedAtUnixSeconds ?? Int64(Date().timeIntervalSince1970)
        let capabilityPreflight = appleBrowserStorageLocalFolderCapabilityPreflight(observedAt: observedAt)
        if appleBrowserStorageJsonBool(capabilityPreflight?["shouldBlockSync"]) {
            let providerDiagnostics = appleBrowserStorageCloudProviderDiagnostics(
                capabilityPreflight: capabilityPreflight,
                compatibilityPreflight: nil,
                statusPolicy: nil,
                scheduleState: nil,
                observedAt: observedAt
            )
            let observabilityEvent = appleBrowserStorageCloudObservabilityEvent(
                providerDiagnostics: providerDiagnostics,
                attemptPlan: nil,
                loopPlan: nil,
                observedAt: observedAt
            )
            return appleBrowserStorageLocalFolderMembershipClientResultJson(
                rootDir: rootDir,
                membershipRequest: appleBrowserStorageJsonObject(requestJson),
                capabilityPreflight: capabilityPreflight,
                compatibilityPreflight: nil,
                schemaRolloutPlan: nil,
                schemaMigrationPlaybook: nil,
                recoverySnapshotRound: nil,
                membershipRound: nil,
                statusPolicy: nil,
                scheduleState: nil,
                attemptPlan: nil,
                loopPlan: nil,
                providerDiagnostics: providerDiagnostics,
                observabilityEvent: observabilityEvent,
                classification: appleBrowserStorageJsonString(capabilityPreflight?["classification"]),
                completed: false,
                shouldReread: false,
                shouldRetry: false,
                clientAction: appleBrowserStorageJsonString(capabilityPreflight?["clientAction"])
            )
        }

        let membershipRoundJson = browserStorageSyncCloudLocalFolderMembershipRoundJson(
            rootDir: rootDir,
            requestJson: requestJson
        )
        guard membershipRoundJson != "null",
              let membershipRound = appleBrowserStorageJsonObject(membershipRoundJson)
        else {
            let providerDiagnostics = appleBrowserStorageCloudProviderDiagnostics(
                capabilityPreflight: capabilityPreflight,
                compatibilityPreflight: nil,
                statusPolicy: nil,
                scheduleState: nil,
                observedAt: observedAt
            )
            let observabilityEvent = appleBrowserStorageCloudObservabilityEvent(
                providerDiagnostics: providerDiagnostics,
                attemptPlan: nil,
                loopPlan: nil,
                observedAt: observedAt
            )
            return appleBrowserStorageLocalFolderMembershipClientResultJson(
                rootDir: rootDir,
                membershipRequest: appleBrowserStorageJsonObject(requestJson),
                capabilityPreflight: capabilityPreflight,
                compatibilityPreflight: nil,
                schemaRolloutPlan: nil,
                schemaMigrationPlaybook: nil,
                recoverySnapshotRound: nil,
                membershipRound: nil,
                statusPolicy: nil,
                scheduleState: nil,
                attemptPlan: nil,
                loopPlan: nil,
                providerDiagnostics: providerDiagnostics,
                observabilityEvent: observabilityEvent,
                classification: "clientPlanUnavailable",
                completed: false,
                shouldReread: false,
                shouldRetry: false,
                clientAction: "skipCloudMembership"
            )
        }

        let completed = appleBrowserStorageJsonBool(membershipRound["completed"])
        let planningErrorKind = appleBrowserStorageJsonString(membershipRound["planningErrorKind"])
        let classification = completed
            ? "success"
            : planningErrorKind.isEmpty ? "membershipRoundIncomplete" : planningErrorKind
        let statusPolicy = appleBrowserStorageCloudMembershipStatusPolicy(
            membershipRound: membershipRound,
            retryAttempt: 0
        )
        let scheduleState = appleBrowserStorageCloudMembershipScheduleState(
            statusPolicy: statusPolicy,
            observedAt: observedAt
        )
        let attemptPlan = appleBrowserStorageCloudMembershipAttemptPlan(
            membershipRound: membershipRound,
            retryAttempt: 0,
            observedAt: observedAt
        )
        let loopPlan = appleBrowserStorageCloudMembershipLoopPlan(
            membershipRound: membershipRound,
            retryAttempt: 0,
            observedAt: observedAt,
            maxAttempts: 3
        )
        let compatibilityPreflight = membershipRound["compatibilityPreflight"] as? [String: Any]
        let schemaRolloutPlan: [String: Any]?
        let schemaMigrationPlaybook: [String: Any]?
        let recoverySnapshotRound: [String: Any]?
        if appleBrowserStorageJsonBool(compatibilityPreflight?["shouldBlockSync"]) {
            schemaRolloutPlan = appleBrowserStorageCloudSchemaRolloutPlan(
                compatibilityPreflight: compatibilityPreflight,
                observedAt: observedAt
            )
            schemaMigrationPlaybook = appleBrowserStorageCloudSchemaMigrationPlaybook(
                schemaRolloutPlan: schemaRolloutPlan,
                observedAt: observedAt
            )
            recoverySnapshotRound = appleBrowserStorageLocalFolderRecoverySnapshotRound(
                rootDir: rootDir,
                schemaMigrationPlaybook: schemaMigrationPlaybook,
                compatibilityPreflight: compatibilityPreflight,
                observedAt: observedAt
            )
        } else {
            schemaRolloutPlan = nil
            schemaMigrationPlaybook = nil
            recoverySnapshotRound = nil
        }
        let providerErrorClassification = appleBrowserStorageLocalFolderProviderErrorClassification(
            membershipRound: membershipRound,
            recoverySnapshotRound: recoverySnapshotRound,
            retryAttempt: 0,
            observedAt: observedAt
        )
        let providerDiagnostics = appleBrowserStorageCloudProviderDiagnostics(
            capabilityPreflight: capabilityPreflight,
            compatibilityPreflight: compatibilityPreflight,
            providerErrorClassification: providerErrorClassification,
            statusPolicy: statusPolicy,
            scheduleState: scheduleState,
            observedAt: observedAt
        )
        let observabilityEvent = appleBrowserStorageCloudObservabilityEvent(
            providerDiagnostics: providerDiagnostics,
            attemptPlan: attemptPlan,
            loopPlan: loopPlan,
            observedAt: observedAt
        )

        return appleBrowserStorageLocalFolderMembershipClientResultJson(
            rootDir: rootDir,
            membershipRequest: appleBrowserStorageJsonObject(requestJson),
            capabilityPreflight: capabilityPreflight,
            compatibilityPreflight: compatibilityPreflight,
            schemaRolloutPlan: schemaRolloutPlan,
            schemaMigrationPlaybook: schemaMigrationPlaybook,
            recoverySnapshotRound: recoverySnapshotRound,
            membershipRound: membershipRound,
            statusPolicy: statusPolicy,
            scheduleState: scheduleState,
            attemptPlan: attemptPlan,
            loopPlan: loopPlan,
            providerDiagnostics: providerDiagnostics,
            observabilityEvent: observabilityEvent,
            classification: appleBrowserStorageJsonString(statusPolicy?["classification"]).isEmpty
                ? classification
                : appleBrowserStorageJsonString(statusPolicy?["classification"]),
            completed: statusPolicy?["completed"] == nil
                ? completed
                : appleBrowserStorageJsonBool(statusPolicy?["completed"]),
            shouldReread: statusPolicy?["shouldReread"] == nil
                ? appleBrowserStorageJsonBool(membershipRound["shouldReread"])
                : appleBrowserStorageJsonBool(statusPolicy?["shouldReread"]),
            shouldRetry: statusPolicy?["shouldRetry"] == nil
                ? appleBrowserStorageJsonBool(membershipRound["shouldRetry"])
                : appleBrowserStorageJsonBool(statusPolicy?["shouldRetry"]),
            clientAction: appleBrowserStorageJsonString(statusPolicy?["clientAction"]).isEmpty
                ? appleBrowserStorageJsonString(membershipRound["clientAction"])
                : appleBrowserStorageJsonString(statusPolicy?["clientAction"])
        )
    }
}

private struct AppleBrowserStorageSyncCycleRequestPlan: Decodable {
    let method: String
    let url: String
    let headers: [AppleBrowserStorageSyncRequestHeader]
    let bodyJson: String

    init?(json: String) {
        guard let data = json.data(using: .utf8),
              let plan = try? JSONDecoder().decode(Self.self, from: data)
        else {
            return nil
        }

        self = plan
    }

    var httpRequest: AppleBrowserStorageSyncHTTPRequest? {
        guard let requestUrl = URL(string: url) else { return nil }
        return AppleBrowserStorageSyncHTTPRequest(
            method: method,
            url: requestUrl,
            headers: Dictionary(uniqueKeysWithValues: headers.map { ($0.name, $0.value) }),
            bodyJson: bodyJson
        )
    }
}

private struct AppleBrowserStorageSyncRequestHeader: Decodable {
    let name: String
    let value: String
}

private func appleBrowserStorageSyncClientResultJson(
    requestPlan: [String: Any]?,
    transportResult: [String: Any]?,
    applyResult: Any?,
    applied: Bool
) -> String {
    let transportResult = transportResult ?? [:]
    let payload: [String: Any] = [
        "version": 1,
        "storageSchemaVersion": AppleBrowserStorageDatabase.schemaVersion,
        "endpointType": "syncCycle",
        "requestPlan": requestPlan ?? NSNull(),
        "transportResult": transportResult,
        "applyResult": applyResult ?? NSNull(),
        "applied": applied,
        "classification": appleBrowserStorageJsonString(transportResult["classification"]),
        "shouldRetry": appleBrowserStorageJsonBool(transportResult["shouldRetry"]),
        "clientAction": appleBrowserStorageJsonString(transportResult["clientAction"]),
    ]

    guard JSONSerialization.isValidJSONObject(payload),
          let data = try? JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys])
    else {
        return "null"
    }

    return String(data: data, encoding: .utf8) ?? "null"
}

private func appleBrowserStorageLocalFolderSyncClientResultJson(
    rootDir: String,
    pushRequest: [String: Any]?,
    capabilityPreflight: [String: Any]?,
    compatibilityPreflight: [String: Any]?,
    schemaRolloutPlan: [String: Any]?,
    schemaMigrationPlaybook: [String: Any]?,
    recoverySnapshotRound: [String: Any]?,
    cycleRound: [String: Any]?,
    statusPolicy: [String: Any]?,
    scheduleState: [String: Any]?,
    attemptPlan: [String: Any]?,
    loopPlan: [String: Any]?,
    applyResult: [String: Any]?,
    compactionRound: [String: Any]?,
    cleanupRound: [String: Any]?,
    providerDiagnostics: [String: Any]?,
    observabilityEvent: [String: Any]?,
    classification: String,
    shouldRetry: Bool,
    clientAction: String,
    applied: Bool
) -> String {
    let payload: [String: Any] = [
        "version": 1,
        "storageSchemaVersion": AppleBrowserStorageDatabase.schemaVersion,
        "endpointType": "cloudLocalFolderCycle",
        "providerKind": "localFolder",
        "rootDir": rootDir,
        "pushRequest": pushRequest ?? NSNull(),
        "capabilityPreflight": capabilityPreflight ?? NSNull(),
        "compatibilityPreflight": compatibilityPreflight ?? NSNull(),
        "schemaRolloutPlan": schemaRolloutPlan ?? NSNull(),
        "schemaMigrationPlaybook": schemaMigrationPlaybook ?? NSNull(),
        "recoverySnapshotRound": recoverySnapshotRound ?? NSNull(),
        "cycleRound": cycleRound ?? NSNull(),
        "statusPolicy": statusPolicy ?? NSNull(),
        "scheduleState": scheduleState ?? NSNull(),
        "attemptPlan": attemptPlan ?? NSNull(),
        "loopPlan": loopPlan ?? NSNull(),
        "applyResult": applyResult ?? NSNull(),
        "compactionRound": compactionRound ?? NSNull(),
        "cleanupRound": cleanupRound ?? NSNull(),
        "providerDiagnostics": providerDiagnostics ?? NSNull(),
        "observabilityEvent": observabilityEvent ?? NSNull(),
        "applied": applied,
        "classification": classification,
        "shouldRetry": shouldRetry,
        "clientAction": clientAction,
    ]

    guard JSONSerialization.isValidJSONObject(payload),
          let data = try? JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys])
    else {
        return "null"
    }

    return String(data: data, encoding: .utf8) ?? "null"
}

private func appleBrowserStorageLocalFolderMembershipClientResultJson(
    rootDir: String,
    membershipRequest: [String: Any]?,
    capabilityPreflight: [String: Any]?,
    compatibilityPreflight: [String: Any]?,
    schemaRolloutPlan: [String: Any]?,
    schemaMigrationPlaybook: [String: Any]?,
    recoverySnapshotRound: [String: Any]?,
    membershipRound: [String: Any]?,
    statusPolicy: [String: Any]?,
    scheduleState: [String: Any]?,
    attemptPlan: [String: Any]?,
    loopPlan: [String: Any]?,
    providerDiagnostics: [String: Any]?,
    observabilityEvent: [String: Any]?,
    classification: String,
    completed: Bool,
    shouldReread: Bool,
    shouldRetry: Bool,
    clientAction: String
) -> String {
    let payload: [String: Any] = [
        "version": 1,
        "storageSchemaVersion": AppleBrowserStorageDatabase.schemaVersion,
        "endpointType": "cloudLocalFolderMembership",
        "providerKind": "localFolder",
        "rootDir": rootDir,
        "membershipRequest": membershipRequest ?? NSNull(),
        "capabilityPreflight": capabilityPreflight ?? NSNull(),
        "compatibilityPreflight": compatibilityPreflight ?? NSNull(),
        "schemaRolloutPlan": schemaRolloutPlan ?? NSNull(),
        "schemaMigrationPlaybook": schemaMigrationPlaybook ?? NSNull(),
        "recoverySnapshotRound": recoverySnapshotRound ?? NSNull(),
        "membershipRound": membershipRound ?? NSNull(),
        "statusPolicy": statusPolicy ?? NSNull(),
        "scheduleState": scheduleState ?? NSNull(),
        "attemptPlan": attemptPlan ?? NSNull(),
        "loopPlan": loopPlan ?? NSNull(),
        "providerDiagnostics": providerDiagnostics ?? NSNull(),
        "observabilityEvent": observabilityEvent ?? NSNull(),
        "completed": completed,
        "shouldReread": shouldReread,
        "shouldRetry": shouldRetry,
        "classification": classification,
        "clientAction": clientAction,
    ]

    guard JSONSerialization.isValidJSONObject(payload),
          let data = try? JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys])
    else {
        return "null"
    }

    return String(data: data, encoding: .utf8) ?? "null"
}

private func appleBrowserStorageLocalFolderCapabilityPreflight(observedAt: Int64) -> [String: Any]? {
    return appleBrowserStorageJsonObject(
        browserStorageSyncCloudLocalFolderCapabilityPreflightJson(
            observedAtUnixSeconds: observedAt
        )
    )
}

private func appleBrowserStorageLocalFolderCompatibilityPreflight(rootDir: String, observedAt: Int64) -> [String: Any]? {
    return appleBrowserStorageJsonObject(
        browserStorageSyncCloudLocalFolderCompatibilityPreflightJson(
            rootDir: rootDir,
            observedAtUnixSeconds: observedAt
        )
    )
}

private func appleBrowserStorageCloudSchemaRolloutPlan(
    compatibilityPreflight: [String: Any]?,
    observedAt: Int64
) -> [String: Any]? {
    appleBrowserStorageJsonObject(
        browserStorageSyncCloudSchemaRolloutPlanJson(
            compatibilityPreflightJson: appleBrowserStorageOptionalJsonString(compatibilityPreflight),
            observedAtUnixSeconds: observedAt
        )
    )
}

private func appleBrowserStorageCloudSchemaMigrationPlaybook(
    schemaRolloutPlan: [String: Any]?,
    observedAt: Int64
) -> [String: Any]? {
    appleBrowserStorageJsonObject(
        browserStorageSyncCloudSchemaMigrationPlaybookJson(
            schemaRolloutPlanJson: appleBrowserStorageOptionalJsonString(schemaRolloutPlan),
            observedAtUnixSeconds: observedAt
        )
    )
}

private func appleBrowserStorageLocalFolderRecoverySnapshotRound(
    rootDir: String,
    schemaMigrationPlaybook: [String: Any]?,
    compatibilityPreflight: [String: Any]?,
    observedAt: Int64
) -> [String: Any]? {
    appleBrowserStorageJsonObject(
        browserStorageSyncCloudLocalFolderRecoverySnapshotRoundJson(
            rootDir: rootDir,
            schemaMigrationPlaybookJson: appleBrowserStorageOptionalJsonString(schemaMigrationPlaybook),
            compatibilityPreflightJson: appleBrowserStorageOptionalJsonString(compatibilityPreflight),
            observedAtUnixSeconds: observedAt
        )
    )
}

private func appleBrowserStorageCloudMembershipStatusPolicy(
    membershipRound: [String: Any]?,
    retryAttempt: UInt32
) -> [String: Any]? {
    appleBrowserStorageJsonObject(
        browserStorageSyncCloudMembershipStatusPolicyJson(
            membershipRoundJson: appleBrowserStorageOptionalJsonString(membershipRound),
            retryAttempt: retryAttempt
        )
    )
}

private func appleBrowserStorageCloudMembershipScheduleState(
    statusPolicy: [String: Any]?,
    observedAt: Int64
) -> [String: Any]? {
    appleBrowserStorageJsonObject(
        browserStorageSyncCloudMembershipScheduleStateJson(
            statusPolicyJson: appleBrowserStorageOptionalJsonString(statusPolicy),
            observedAtUnixSeconds: observedAt
        )
    )
}

private func appleBrowserStorageCloudMembershipAttemptPlan(
    membershipRound: [String: Any]?,
    retryAttempt: UInt32,
    observedAt: Int64
) -> [String: Any]? {
    appleBrowserStorageJsonObject(
        browserStorageSyncCloudMembershipAttemptPlanJson(
            membershipRoundJson: appleBrowserStorageOptionalJsonString(membershipRound),
            retryAttempt: retryAttempt,
            observedAtUnixSeconds: observedAt
        )
    )
}

private func appleBrowserStorageCloudMembershipLoopPlan(
    membershipRound: [String: Any]?,
    retryAttempt: UInt32,
    observedAt: Int64,
    maxAttempts: UInt32
) -> [String: Any]? {
    let roundsJson: String
    if let membershipRound,
       JSONSerialization.isValidJSONObject([membershipRound]),
       let data = try? JSONSerialization.data(withJSONObject: [membershipRound], options: [.sortedKeys]),
       let json = String(data: data, encoding: .utf8) {
        roundsJson = json
    } else {
        roundsJson = "[]"
    }
    return appleBrowserStorageJsonObject(
        browserStorageSyncCloudMembershipLoopPlanJson(
            membershipRoundsJson: roundsJson,
            retryAttempt: retryAttempt,
            observedAtUnixSeconds: observedAt,
            maxAttempts: maxAttempts
        )
    )
}

private func appleBrowserStorageLocalFolderProviderErrorClassification(
    cycleRound: [String: Any]? = nil,
    membershipRound: [String: Any]? = nil,
    compactionRound: [String: Any]? = nil,
    cleanupRound: [String: Any]? = nil,
    recoverySnapshotRound: [String: Any]? = nil,
    retryAttempt: UInt32,
    observedAt: Int64
) -> [String: Any]? {
    var payload: [String: Any] = [
        "planType": "localFolderProviderRoundErrors",
        "providerKind": "localFolder",
    ]
    if let cycleRound {
        payload["cycleRound"] = cycleRound
    }
    if let membershipRound {
        payload["membershipRound"] = membershipRound
    }
    if let compactionRound {
        payload["compactionRound"] = compactionRound
    }
    if let cleanupRound {
        payload["cleanupRound"] = cleanupRound
    }
    if let recoverySnapshotRound {
        payload["recoverySnapshotRound"] = recoverySnapshotRound
    }

    let classification = appleBrowserStorageJsonObject(
        browserStorageSyncCloudProviderErrorClassificationJson(
            providerErrorsJson: appleBrowserStorageOptionalJsonString(payload),
            retryAttempt: retryAttempt,
            observedAtUnixSeconds: observedAt
        )
    )
    guard appleBrowserStorageJsonInt(classification?["failedCount"]) > 0 else {
        return nil
    }
    return classification
}

private func appleBrowserStorageCloudProviderDiagnostics(
    capabilityPreflight: [String: Any]?,
    compatibilityPreflight: [String: Any]?,
    providerErrorClassification: [String: Any]? = nil,
    statusPolicy: [String: Any]?,
    scheduleState: [String: Any]?,
    observedAt: Int64
) -> [String: Any]? {
    appleBrowserStorageJsonObject(
        browserStorageSyncCloudProviderDiagnosticsJson(
            capabilityPreflightJson: appleBrowserStorageOptionalJsonString(capabilityPreflight),
            compatibilityPreflightJson: appleBrowserStorageOptionalJsonString(compatibilityPreflight),
            providerErrorClassificationJson: appleBrowserStorageOptionalJsonString(providerErrorClassification),
            cycleStatusPolicyJson: appleBrowserStorageOptionalJsonString(statusPolicy),
            scheduleStateJson: appleBrowserStorageOptionalJsonString(scheduleState),
            observedAtUnixSeconds: observedAt
        )
    )
}

private func appleBrowserStorageCloudObservabilityEvent(
    providerDiagnostics: [String: Any]?,
    attemptPlan: [String: Any]?,
    loopPlan: [String: Any]?,
    observedAt: Int64
) -> [String: Any]? {
    appleBrowserStorageJsonObject(
        browserStorageSyncCloudObservabilityEventJson(
            providerDiagnosticsJson: appleBrowserStorageOptionalJsonString(providerDiagnostics),
            attemptPlanJson: appleBrowserStorageOptionalJsonString(attemptPlan),
            loopPlanJson: appleBrowserStorageOptionalJsonString(loopPlan),
            observedAtUnixSeconds: observedAt
        )
    )
}

private func appleBrowserStorageJsonObject(_ json: String) -> [String: Any]? {
    guard json != "null",
          let data = json.data(using: .utf8)
    else {
        return nil
    }
    return try? JSONSerialization.jsonObject(with: data) as? [String: Any]
}

private func appleBrowserStorageJsonStringValue(_ value: Any) -> String? {
    guard JSONSerialization.isValidJSONObject(value),
          let data = try? JSONSerialization.data(withJSONObject: value, options: [.sortedKeys])
    else {
        return nil
    }
    return String(data: data, encoding: .utf8)
}

private func appleBrowserStorageOptionalJsonString(_ value: Any?) -> String {
    guard let value else {
        return ""
    }
    return appleBrowserStorageJsonStringValue(value) ?? ""
}

private func appleBrowserStorageJsonString(_ value: Any?) -> String {
    value as? String ?? ""
}

private func appleBrowserStorageJsonBool(_ value: Any?) -> Bool {
    if let value = value as? Bool {
        return value
    }
    if let value = value as? NSNumber {
        return value.boolValue
    }
    return false
}

private func appleBrowserStorageJsonInt(_ value: Any?) -> Int {
    if let value = value as? Int {
        return value
    }
    if let value = value as? NSNumber {
        return value.intValue
    }
    return 0
}
