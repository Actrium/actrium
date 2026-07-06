package io.actrium.actro.data.sync

import android.os.Build
import android.util.Log
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
    private val credentialsProvider: BrowserStorageSyncCredentialsProvider,
    // Optional so the endpoint-only test/dev coordinator keeps working; in
    // production the stack always injects the store (an unconfigured provider
    // still holds a minted device id) and the provider branch owns browser sync.
    private val providerSettingsStore: BrowserStorageSyncProviderSettingsStore? = null
) {
    // Session-scoped provider error state: at cold start no round has run, so
    // the planner snapshot carries no error kind; within a session it reflects
    // the last round's provider error classification. Volatile because the
    // singleton coordinator can be driven from different scheduler coroutines.
    @Volatile
    private var lastProviderErrorKind: String? = null

    suspend fun runSyncCycle(
        generatedAtUnixSeconds: Long = browserStorageSyncCoordinatorCurrentUnixSeconds(),
        limitPerCollection: Int = 200,
        retryAttempt: Int = 0,
        observedAtUnixSeconds: Long = browserStorageSyncCoordinatorCurrentUnixSeconds()
    ): String {
        // Provider branch first: a configured cloud provider connection owns
        // browser sync; only an unconfigured provider falls through to the
        // endpoint-token reference-server branch below.
        if (providerSettingsStore != null) {
            val providerResultJson = runProviderCycle(
                generatedAtUnixSeconds = generatedAtUnixSeconds,
                limitPerCollection = limitPerCollection,
                retryAttempt = retryAttempt,
                observedAtUnixSeconds = observedAtUnixSeconds,
                store = providerSettingsStore,
            )
            if (providerResultJson != null) {
                return providerResultJson
            }
        }

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

    // Returns null only for the NOT_CONFIGURED routing so the caller falls
    // through to the endpoint branch; every other routing produces a final
    // coordinator result.
    private suspend fun runProviderCycle(
        generatedAtUnixSeconds: Long,
        limitPerCollection: Int,
        retryAttempt: Int,
        observedAtUnixSeconds: Long,
        store: BrowserStorageSyncProviderSettingsStore
    ): String? {
        var settings = store.load()
        var plan = BrowserStorageSyncProviderConnectionPlanner.plan(
            settings = settings,
            lastProviderErrorKind = lastProviderErrorKind,
            retryAttempt = retryAttempt,
            observedAtUnixSeconds = observedAtUnixSeconds,
        )
        if (plan.routing == BrowserSyncProviderRouting.NOT_CONFIGURED) {
            return null
        }
        if (plan.routing == BrowserSyncProviderRouting.BLOCKED) {
            return providerBlockedResultJson(settings, plan, retryAttempt, observedAtUnixSeconds)
        }

        if (plan.routing == BrowserSyncProviderRouting.RUN_LOCAL_FOLDER_MEMBERSHIP) {
            val rootDir = settings.localFolderRootPath ?: return null
            val membershipEnvelope = client.runLocalFolderMembershipRound(
                rootDir = rootDir,
                requestJson = buildMembershipJoinRequestJson(settings, generatedAtUnixSeconds),
                observedAtUnixSeconds = observedAtUnixSeconds,
            ).toSyncCoordinatorJsonObjectOrNull()
            applyProviderRoundOutcome(membershipEnvelope, store)

            if (membershipEnvelope?.optBoolean("completed", false) != true) {
                return providerRoundResultJson(membershipEnvelope, "cloudLocalFolderMembership")
            }

            // Persist the membership marker so later launches go straight to the
            // cycle round instead of re-writing the shared membership document
            // on every start.
            store.saveMembershipJoined(observedAtUnixSeconds)
            settings = store.load()
            plan = BrowserStorageSyncProviderConnectionPlanner.plan(
                settings = settings,
                lastProviderErrorKind = lastProviderErrorKind,
                retryAttempt = retryAttempt,
                observedAtUnixSeconds = observedAtUnixSeconds,
            )
            if (plan.routing != BrowserSyncProviderRouting.RUN_LOCAL_FOLDER_CYCLE) {
                // Re-evaluation after a successful join is expected to say "run
                // the cycle"; anything else (fresh provider error, policy change)
                // is reported honestly instead of forcing a cycle the policy no
                // longer allows.
                return if (plan.routing == BrowserSyncProviderRouting.BLOCKED) {
                    providerBlockedResultJson(settings, plan, retryAttempt, observedAtUnixSeconds)
                } else {
                    providerRoundResultJson(membershipEnvelope, "cloudLocalFolderMembership")
                }
            }
        }

        val cycleRootDir = settings.localFolderRootPath ?: return null
        val cycleEnvelope = client.runLocalFolderSyncCycle(
            rootDir = cycleRootDir,
            scope = settings.scope,
            deviceId = settings.deviceId,
            generatedAtUnixSeconds = generatedAtUnixSeconds,
            limitPerCollection = limitPerCollection,
            retryAttempt = retryAttempt,
            observedAtUnixSeconds = observedAtUnixSeconds,
        ).toSyncCoordinatorJsonObjectOrNull()
        applyProviderRoundOutcome(cycleEnvelope, store)
        return providerRoundResultJson(cycleEnvelope, "cloudLocalFolderCycle")
    }

    // Reads the round's provider error classification into the session error
    // state and clears the persisted membership marker on membership- or
    // storage-scoped errors so the next run re-joins the cloud store.
    private fun applyProviderRoundOutcome(
        roundEnvelope: JSONObject?,
        store: BrowserStorageSyncProviderSettingsStore
    ) {
        val providerErrorClassification = roundEnvelope
            ?.optJSONObject("providerDiagnostics")
            ?.optString("providerErrorClassification")
            ?.takeIf { it.isNotEmpty() }
        lastProviderErrorKind = BrowserStorageSyncProviderConnectionPlanner
            .mapProviderErrorClassification(providerErrorClassification)
        if (BrowserStorageSyncProviderConnectionPlanner.shouldClearMembershipMarker(providerErrorClassification)) {
            store.clearMembershipMarker()
            Log.w(
                TAG,
                "Browser sync provider round reported a membership/storage-scoped error " +
                    "($providerErrorClassification); cleared the membership marker so the next run re-joins",
            )
        }
    }

    private fun buildMembershipJoinRequestJson(
        settings: BrowserStorageSyncProviderSettings,
        generatedAtUnixSeconds: Long
    ): String =
        JSONObject()
            .put("scope", settings.scope)
            .put("actorDeviceId", settings.deviceId)
            .put("memberDeviceId", settings.deviceId)
            .put("displayName", Build.MODEL ?: "Android")
            .put("platform", "Android")
            .put("membershipAction", "connect")
            .put("generatedAtUnixSeconds", generatedAtUnixSeconds)
            .toString()

    // Round routings: the round envelope already carries the cloud
    // scheduler-shaped scheduleState and statusPolicy; lift the last executed
    // round's fields to the top level the scheduler reads.
    private fun providerRoundResultJson(
        roundEnvelope: JSONObject?,
        fallbackEndpointType: String
    ): String =
        JSONObject()
            .put("version", 1)
            .put("storageSchemaVersion", ActroCore.browserStorageSchemaVersion().toLong())
            .put(
                "endpointType",
                roundEnvelope?.optString("endpointType")?.takeIf { it.isNotEmpty() } ?: fallbackEndpointType,
            )
            // Local folder never waits for credentials: there are none.
            .put("credentialsAvailable", true)
            .put(
                "providerKind",
                roundEnvelope?.optString("providerKind")?.takeIf { it.isNotEmpty() }
                    ?: BrowserStorageSyncProviderSettingsStore.LOCAL_FOLDER_PROVIDER_KIND,
            )
            .put("syncResult", roundEnvelope ?: JSONObject.NULL)
            .put("classification", roundEnvelope?.optString("classification").orEmpty())
            .put("shouldRetry", roundEnvelope?.optBoolean("shouldRetry", false) ?: false)
            .put("clientAction", roundEnvelope?.optString("clientAction").orEmpty())
            .put("statusPolicy", roundEnvelope?.optJSONObject("statusPolicy") ?: JSONObject.NULL)
            .put("scheduleState", roundEnvelope?.optJSONObject("scheduleState") ?: JSONObject.NULL)
            .toString()

    // Blocked routing: no round runs and no envelope exists, so the
    // scheduleState is the planner's pure translation of the connection status
    // policy into the scheduler's field shape.
    private fun providerBlockedResultJson(
        settings: BrowserStorageSyncProviderSettings,
        plan: BrowserSyncProviderConnectionPlan,
        retryAttempt: Int,
        observedAtUnixSeconds: Long
    ): String {
        val connectionPolicy = plan.connectionPolicy ?: JSONObject()
        val scheduleState = BrowserStorageSyncProviderConnectionPlanner.buildBlockedScheduleState(
            connectionPolicy = connectionPolicy,
            retryAttempt = retryAttempt,
            observedAtUnixSeconds = observedAtUnixSeconds,
        )
        return JSONObject()
            .put("version", 1)
            .put("storageSchemaVersion", ActroCore.browserStorageSchemaVersion().toLong())
            .put("endpointType", "cloudProviderConnection")
            .put("credentialsAvailable", true)
            .put("providerKind", settings.providerKind ?: JSONObject.NULL)
            .put("syncResult", JSONObject.NULL)
            .put("classification", connectionPolicy.optString("classification"))
            .put("shouldRetry", connectionPolicy.optBoolean("shouldRetry", false))
            .put("clientAction", connectionPolicy.optString("providerAction"))
            .put("statusPolicy", connectionPolicy)
            .put("scheduleState", scheduleState)
            .toString()
    }

    private companion object {
        private const val TAG = "BrowserSyncCoordinator"
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
