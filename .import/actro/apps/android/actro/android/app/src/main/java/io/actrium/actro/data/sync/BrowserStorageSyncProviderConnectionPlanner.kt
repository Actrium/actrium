package io.actrium.actro.data.sync

import io.actrium.actro.rust.ActroCore
import org.json.JSONException
import org.json.JSONObject

enum class BrowserSyncProviderRouting {
    // No provider is configured: browser sync falls through to the
    // endpoint-token reference-server branch (development path).
    NOT_CONFIGURED,

    // The device has not joined the cloud store yet (no persisted membership
    // marker): run the one-time membership join round first.
    RUN_LOCAL_FOLDER_MEMBERSHIP,

    // Connection is ready: run the local-folder sync cycle round.
    RUN_LOCAL_FOLDER_CYCLE,

    // The connection status policy demands setup, user action, or a retry
    // before any round may run.
    BLOCKED,
}

data class BrowserSyncProviderConnectionPlan(
    val routing: BrowserSyncProviderRouting,
    val providerSnapshot: JSONObject?,
    val connectionPolicy: JSONObject?
)

/**
 * Assembles the provider snapshot for the Rust cloud provider connection
 * contract and derives the coordinator routing from the resulting policy.
 *
 * Pure by design: no IO -- the accept-time folder preflight and each round's
 * own capability / compatibility preflights are the authoritative folder
 * checks, so the snapshot is fully determined by persisted settings plus
 * in-memory session state. This mirrors the Windows and macOS
 * BrowserSyncProviderConnectionPlanner so all three platforms route over the
 * exact same Rust connection-status-policy contract.
 */
object BrowserStorageSyncProviderConnectionPlanner {
    fun isConfigured(settings: BrowserStorageSyncProviderSettings?): Boolean =
        settings?.isConfigured ?: false

    fun buildProviderSnapshot(
        settings: BrowserStorageSyncProviderSettings,
        lastProviderErrorKind: String?
    ): JSONObject {
        val snapshot = JSONObject()
            .put("providerKind", settings.providerKind.orEmpty())
            .put("scope", settings.scope)
            .put("deviceId", settings.deviceId)

        if (settings.providerKind == BrowserStorageSyncProviderSettingsStore.LOCAL_FOLDER_PROVIDER_KIND) {
            // Local folder is connection-free: there is no session to open, so
            // a configured folder is by definition "connected", and the
            // credential state is constantly "available" because there are no
            // secrets. (The connection contract normalizes unknown connection
            // tokens to "disconnected", which would misclassify a configured
            // local folder as signInRequired.)
            snapshot.put("connectionState", "connected")
            snapshot.put("credentialState", "available")
            // The persisted membership marker is the only cold-start membership
            // signal: a membership round is a write to the shared cloud folder,
            // and repeating it on every launch is wrong even though the
            // device-document upsert is idempotent. "missing" routes this run to
            // a one-time join; after a successful join the marker persists and
            // later launches go straight to the cycle round.
            snapshot.put(
                "membershipStatus",
                if (settings.membershipJoinedAtUnixSeconds != null) "active" else "missing",
            )
            // No IO here: the folder was validated when it was accepted and the
            // cycle round re-validates it authoritatively, so the store is
            // reported ready and a real storage failure comes back as a
            // storage-scoped provider error on the next round.
            snapshot.put("cloudStoreReady", true)
        }

        if (!lastProviderErrorKind.isNullOrEmpty()) {
            snapshot.put("lastProviderErrorKind", lastProviderErrorKind)
        }

        return snapshot
    }

    fun plan(
        settings: BrowserStorageSyncProviderSettings?,
        lastProviderErrorKind: String?,
        retryAttempt: Int,
        observedAtUnixSeconds: Long
    ): BrowserSyncProviderConnectionPlan {
        if (!isConfigured(settings)) {
            // Short-circuit without calling the policy: an unconfigured provider
            // is the endpoint branch's territory, not a blocked connection.
            return BrowserSyncProviderConnectionPlan(
                routing = BrowserSyncProviderRouting.NOT_CONFIGURED,
                providerSnapshot = null,
                connectionPolicy = null,
            )
        }

        val providerSnapshot = buildProviderSnapshot(settings!!, lastProviderErrorKind)
        // The Rust core owns the connection classification and action rules. A
        // result that is not a JSON object is contract drift and falls back to
        // BLOCKED rather than a hand-copied policy.
        val connectionPolicy = ActroCore.browserStorageSyncCloudProviderConnectionStatusPolicyJson(
            providerSnapshotJson = providerSnapshot.toString(),
            retryAttempt = retryAttempt.coerceAtLeast(0).toUInt(),
            observedAtUnixSeconds = observedAtUnixSeconds,
        ).toJsonObjectOrNull()

        val isLocalFolder = settings.providerKind ==
            BrowserStorageSyncProviderSettingsStore.LOCAL_FOLDER_PROVIDER_KIND
        val routing = when {
            isLocalFolder && connectionPolicy?.optBoolean("shouldAttemptMembership", false) == true ->
                BrowserSyncProviderRouting.RUN_LOCAL_FOLDER_MEMBERSHIP
            isLocalFolder && connectionPolicy?.optBoolean("shouldAttemptSync", false) == true ->
                BrowserSyncProviderRouting.RUN_LOCAL_FOLDER_CYCLE
            else -> BrowserSyncProviderRouting.BLOCKED
        }

        return BrowserSyncProviderConnectionPlan(routing, providerSnapshot, connectionPolicy)
    }

    // Translates the connection status policy output (schedulerAction /
    // retryDelaySeconds / userAction / resetRetryAttempt) into the field shape
    // the sync scheduler consumes. The HTTP-era status policy helper must not be
    // fed cloud-only classifications, and no round envelope exists on the
    // BLOCKED routing, so this pure translation is the only scheduleState source
    // for blocked provider connections.
    fun buildBlockedScheduleState(
        connectionPolicy: JSONObject,
        retryAttempt: Int,
        observedAtUnixSeconds: Long
    ): JSONObject {
        val schedulerAction = connectionPolicy.optString("schedulerAction")
        val userAction = connectionPolicy.optString("userAction")
        val requiresUserAction = userAction.isNotEmpty() && userAction != "none"

        val scheduleState = JSONObject()
            .put("planType", "cloudProviderConnectionScheduleState")
            .put("classification", connectionPolicy.optString("classification"))
            .put("schedulerAction", schedulerAction)
            // Local folder never waits for credentials; cloud providers that do
            // surface it through their own classifications.
            .put("waitingForCredentials", false)
            .put("requiresUserAction", requiresUserAction)
            .put("userAction", userAction)
            .put("resetRetryAttempt", connectionPolicy.optBoolean("resetRetryAttempt", false))

        val retryDelaySeconds = connectionPolicy.optNullableLong("retryDelaySeconds")
        if (retryDelaySeconds != null) {
            scheduleState.put("nextRunDelaySeconds", retryDelaySeconds)
            scheduleState.put("nextRunAtUnixSeconds", observedAtUnixSeconds + retryDelaySeconds)
        }

        if (schedulerAction == "scheduleRetry") {
            scheduleState.put("nextRetryAttempt", retryAttempt + 1)
        }

        return scheduleState
    }

    // Maps the provider error classification a round's diagnostics reports into
    // the error-kind vocabulary the connection contract matches on so the next
    // planner run classifies the connection truthfully within this session.
    fun mapProviderErrorClassification(providerErrorClassification: String?): String? =
        when (providerErrorClassification) {
            null, "", "ready" -> null
            "authExpired" -> "auth"
            "quotaExceeded" -> "quota"
            "adapterUnsupported" -> "adapterUnsupported"
            "recoveryRequired" -> "recoveryRequired"
            "providerUnavailable" -> "providerUnavailable"
            "storageSetupRequired" -> "storage"
            // Rereads are handled inside the rounds themselves; not a
            // connection-level condition.
            "cloudStateRereadRequired" -> null
            // Unknown tokens pass through; the connection contract ignores kinds
            // it does not match.
            else -> providerErrorClassification
        }

    // Membership- or storage-scoped provider errors mean the shared cloud store
    // no longer knows this device (folder wiped, store reset by another device):
    // clearing the persisted membership marker makes the next run re-join. A
    // wrongly cleared marker costs exactly one corrective membership round, so
    // this stays deliberately narrow.
    fun shouldClearMembershipMarker(providerErrorClassification: String?): Boolean =
        when (providerErrorClassification) {
            "storageSetupRequired", "recoveryRequired" -> true
            "membershipRequired", "deviceMembershipMissing", "deviceMembershipRevoked" -> true
            else -> false
        }
}

private fun String.toJsonObjectOrNull(): JSONObject? =
    try {
        if (this == "null") null else JSONObject(this)
    } catch (_: JSONException) {
        null
    }

private fun JSONObject.optNullableLong(name: String): Long? =
    if (has(name) && !isNull(name)) optLong(name) else null
