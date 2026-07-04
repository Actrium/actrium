package io.actrium.actro.data.sync

import io.actrium.actro.rust.ActroCore
import org.json.JSONException
import org.json.JSONObject

data class BrowserStorageSyncCredentials(
    val baseUrl: String,
    val accessToken: String,
    val scope: String = "default",
    val deviceId: String
) {
    val isUsable: Boolean
        get() = baseUrl.isNotBlank() &&
            accessToken.isNotBlank() &&
            scope.isNotBlank() &&
            deviceId.isNotBlank()
}

interface BrowserStorageSyncCredentialsProvider {
    suspend fun browserStorageSyncCredentials(): BrowserStorageSyncCredentials?
}

class BrowserStorageSyncCoordinator(
    private val client: BrowserStorageSyncClient,
    private val credentialsProvider: BrowserStorageSyncCredentialsProvider
) {
    suspend fun runSyncCycle(
        generatedAtUnixSeconds: Long = browserStorageSyncCoordinatorCurrentUnixSeconds(),
        limitPerCollection: Int = 200,
        retryAttempt: Int = 0,
        observedAtUnixSeconds: Long = browserStorageSyncCoordinatorCurrentUnixSeconds()
    ): String {
        val credentials = credentialsProvider.browserStorageSyncCredentials()
        if (credentials?.isUsable != true) {
            return syncCoordinatorResultJson(
                credentialsAvailable = false,
                syncResult = JSONObject.NULL,
                classification = "credentialsUnavailable",
                shouldRetry = false,
                clientAction = "refreshCredentials",
                retryAttempt = retryAttempt,
                observedAtUnixSeconds = observedAtUnixSeconds,
            )
        }

        val syncResultJson = client.runSyncCycle(
            baseUrl = credentials.baseUrl,
            accessToken = credentials.accessToken,
            scope = credentials.scope,
            deviceId = credentials.deviceId,
            generatedAtUnixSeconds = generatedAtUnixSeconds,
            limitPerCollection = limitPerCollection,
        )
        val syncResult = syncResultJson.toSyncCoordinatorJsonObjectOrNull()

        return syncCoordinatorResultJson(
            credentialsAvailable = true,
            syncResult = syncResult ?: JSONObject.NULL,
            classification = syncResult?.optString("classification").orEmpty(),
            shouldRetry = syncResult?.optBoolean("shouldRetry", false) ?: false,
            clientAction = syncResult?.optString("clientAction").orEmpty(),
            retryAttempt = retryAttempt,
            observedAtUnixSeconds = observedAtUnixSeconds,
        )
    }
}

private fun syncCoordinatorResultJson(
    credentialsAvailable: Boolean,
    syncResult: Any,
    classification: String,
    shouldRetry: Boolean,
    clientAction: String,
    retryAttempt: Int,
    observedAtUnixSeconds: Long
): String {
    val statusPolicyJson = ActroCore.browserStorageSyncStatusPolicyJson(
        classification = classification,
        shouldRetry = shouldRetry,
        clientAction = clientAction,
        retryAttempt = retryAttempt,
    )
    return JSONObject()
        .put("version", 1)
        .put("storageSchemaVersion", ActroCore.browserStorageSchemaVersion().toLong())
        .put("endpointType", "syncCycle")
        .put("credentialsAvailable", credentialsAvailable)
        .put("syncResult", syncResult)
        .put("classification", classification)
        .put("shouldRetry", shouldRetry)
        .put("clientAction", clientAction)
        .put(
            "statusPolicy",
            statusPolicyJson.toSyncCoordinatorJsonObjectOrNull() ?: JSONObject.NULL,
        )
        .put(
            "scheduleState",
            ActroCore.browserStorageSyncScheduleStateJson(
                statusPolicyJson = statusPolicyJson,
                observedAtUnixSeconds = observedAtUnixSeconds,
            ).toSyncCoordinatorJsonObjectOrNull() ?: JSONObject.NULL,
        )
        .toString()
}

private fun String.toSyncCoordinatorJsonObjectOrNull(): JSONObject? =
    try {
        if (this == "null") null else JSONObject(this)
    } catch (_: JSONException) {
        null
    }

private fun browserStorageSyncCoordinatorCurrentUnixSeconds(): Long =
    System.currentTimeMillis() / 1000
