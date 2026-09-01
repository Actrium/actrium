import Foundation

public struct AppleBrowserStorageSyncCredentials: Equatable {
    public var baseUrl: String
    public var accessToken: String
    public var scope: String
    public var deviceId: String

    public init(
        baseUrl: String,
        accessToken: String,
        scope: String = "default",
        deviceId: String
    ) {
        self.baseUrl = baseUrl
        self.accessToken = accessToken
        self.scope = scope
        self.deviceId = deviceId
    }

    var isUsable: Bool {
        !baseUrl.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty &&
            !accessToken.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty &&
            !scope.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty &&
            !deviceId.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }
}

public protocol AppleBrowserStorageSyncCredentialsProvider {
    func browserStorageSyncCredentials() async -> AppleBrowserStorageSyncCredentials?
}

public final class AppleBrowserStorageSyncCoordinator {
    private let client: AppleBrowserStorageSyncClient
    private let credentialsProvider: AppleBrowserStorageSyncCredentialsProvider

    public init(
        client: AppleBrowserStorageSyncClient,
        credentialsProvider: AppleBrowserStorageSyncCredentialsProvider
    ) {
        self.client = client
        self.credentialsProvider = credentialsProvider
    }

    public func runSyncCycle(
        generatedAtUnixSeconds: Int64? = nil,
        limitPerCollection: Int = 200,
        retryAttempt: UInt32 = 0,
        observedAtUnixSeconds: Int64? = nil
    ) async -> String {
        let observedAtUnixSeconds = observedAtUnixSeconds ?? appleBrowserStorageSyncCoordinatorCurrentUnixSeconds()

        guard let credentials = await credentialsProvider.browserStorageSyncCredentials(),
              credentials.isUsable
        else {
            return appleBrowserStorageSyncCoordinatorResultJson(
                credentialsAvailable: false,
                syncResult: nil,
                classification: "credentialsUnavailable",
                shouldRetry: false,
                clientAction: "refreshCredentials",
                retryAttempt: retryAttempt,
                observedAtUnixSeconds: observedAtUnixSeconds
            )
        }

        let syncResultJson = await client.runSyncCycle(
            baseUrl: credentials.baseUrl,
            accessToken: credentials.accessToken,
            scope: credentials.scope,
            deviceId: credentials.deviceId,
            generatedAtUnixSeconds: generatedAtUnixSeconds,
            limitPerCollection: limitPerCollection
        )
        let syncResult = appleBrowserStorageSyncCoordinatorJsonObject(syncResultJson)

        return appleBrowserStorageSyncCoordinatorResultJson(
            credentialsAvailable: true,
            syncResult: syncResult,
            classification: appleBrowserStorageSyncCoordinatorJsonString(syncResult?["classification"]),
            shouldRetry: appleBrowserStorageSyncCoordinatorJsonBool(syncResult?["shouldRetry"]),
            clientAction: appleBrowserStorageSyncCoordinatorJsonString(syncResult?["clientAction"]),
            retryAttempt: retryAttempt,
            observedAtUnixSeconds: observedAtUnixSeconds
        )
    }
}

private func appleBrowserStorageSyncCoordinatorResultJson(
    credentialsAvailable: Bool,
    syncResult: [String: Any]?,
    classification: String,
    shouldRetry: Bool,
    clientAction: String,
    retryAttempt: UInt32,
    observedAtUnixSeconds: Int64
) -> String {
    let statusPolicyJson = browserStorageSyncStatusPolicyJson(
        classification: classification,
        shouldRetry: shouldRetry,
        clientAction: clientAction,
        retryAttempt: retryAttempt
    )
    let statusPolicy = appleBrowserStorageSyncCoordinatorJsonObject(statusPolicyJson)
    let scheduleState = appleBrowserStorageSyncCoordinatorJsonObject(browserStorageSyncScheduleStateJson(
        statusPolicyJson: statusPolicyJson,
        observedAtUnixSeconds: observedAtUnixSeconds
    ))
    let payload: [String: Any] = [
        "version": 1,
        "storageSchemaVersion": AppleBrowserStorageDatabase.schemaVersion,
        "endpointType": "syncCycle",
        "credentialsAvailable": credentialsAvailable,
        "syncResult": syncResult ?? NSNull(),
        "classification": classification,
        "shouldRetry": shouldRetry,
        "clientAction": clientAction,
        "statusPolicy": statusPolicy ?? NSNull(),
        "scheduleState": scheduleState ?? NSNull(),
    ]

    guard JSONSerialization.isValidJSONObject(payload),
          let data = try? JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys])
    else {
        return "null"
    }

    return String(data: data, encoding: .utf8) ?? "null"
}

private func appleBrowserStorageSyncCoordinatorJsonObject(_ json: String) -> [String: Any]? {
    guard json != "null",
          let data = json.data(using: .utf8)
    else {
        return nil
    }

    return try? JSONSerialization.jsonObject(with: data) as? [String: Any]
}

private func appleBrowserStorageSyncCoordinatorJsonString(_ value: Any?) -> String {
    value as? String ?? ""
}

private func appleBrowserStorageSyncCoordinatorJsonBool(_ value: Any?) -> Bool {
    if let value = value as? Bool {
        return value
    }

    if let value = value as? NSNumber {
        return value.boolValue
    }

    return false
}

private func appleBrowserStorageSyncCoordinatorCurrentUnixSeconds() -> Int64 {
    Int64(Date().timeIntervalSince1970)
}
