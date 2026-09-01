package io.actrium.actro.data.sync

import io.actrium.actro.rust.ActroCore
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import org.json.JSONException
import org.json.JSONObject

data class BrowserStorageSyncSchedulerSnapshot(
    val retryAttempt: Int = 0,
    val schedulerAction: String = "idle",
    val nextRunDelaySeconds: Long? = null,
    val nextRunAtUnixSeconds: Long? = null,
    val waitingForCredentials: Boolean = false,
    val requiresUserAction: Boolean = false,
    val pendingRunScheduled: Boolean = false,
    val lastResultJson: String? = null
)

class BrowserStorageSyncScheduler(
    private val coordinator: BrowserStorageSyncCoordinator,
    private val scope: CoroutineScope,
    private val schedulesFollowUp: Boolean = true,
    private val clock: () -> Long = { browserStorageSyncSchedulerCurrentUnixSeconds() },
    private val delayRunner: suspend (Long) -> Unit = { seconds ->
        if (seconds > 0) {
            delay(seconds * 1_000L)
        }
    }
) {
    private val stateLock = Any()
    private var retryAttempt: Int = 0
    private var pendingJob: Job? = null
    private var snapshotValue = BrowserStorageSyncSchedulerSnapshot()

    suspend fun runNow(
        generatedAtUnixSeconds: Long = clock(),
        limitPerCollection: Int = 200,
        observedAtUnixSeconds: Long = clock()
    ): String {
        cancelPendingSync()
        return runCycle(
            generatedAtUnixSeconds = generatedAtUnixSeconds,
            limitPerCollection = limitPerCollection,
            observedAtUnixSeconds = observedAtUnixSeconds,
        )
    }

    fun scheduleAfterLocalChange(delaySeconds: Long = 2) {
        val delay = delaySeconds.coerceAtLeast(0)
        synchronized(stateLock) {
            retryAttempt = 0
            snapshotValue = BrowserStorageSyncSchedulerSnapshot(
                retryAttempt = 0,
                schedulerAction = "localChange",
                nextRunDelaySeconds = delay,
                nextRunAtUnixSeconds = clock() + delay,
                pendingRunScheduled = true,
                lastResultJson = snapshotValue.lastResultJson,
            )
        }
        installPendingJob(delay)
    }

    fun cancelPendingSync() {
        synchronized(stateLock) {
            pendingJob?.cancel()
            pendingJob = null
            snapshotValue = snapshotValue.copy(pendingRunScheduled = false)
        }
    }

    fun snapshot(): BrowserStorageSyncSchedulerSnapshot =
        synchronized(stateLock) { snapshotValue }

    fun statusJson(): String =
        ActroCore.browserStorageSyncSchedulerStatusJson(snapshot().toJSONObject().toString())

    private suspend fun runScheduledSync() {
        synchronized(stateLock) {
            pendingJob = null
            snapshotValue = snapshotValue.copy(pendingRunScheduled = false)
        }
        runCycle()
    }

    private suspend fun runCycle(
        generatedAtUnixSeconds: Long = clock(),
        limitPerCollection: Int = 200,
        observedAtUnixSeconds: Long = clock()
    ): String {
        val attempt = synchronized(stateLock) { retryAttempt }
        val resultJson = coordinator.runSyncCycle(
            generatedAtUnixSeconds = generatedAtUnixSeconds,
            limitPerCollection = limitPerCollection,
            retryAttempt = attempt,
            observedAtUnixSeconds = observedAtUnixSeconds,
        )
        applyResult(resultJson)
        return resultJson
    }

    private fun applyResult(resultJson: String) {
        val scheduleState = resultJson.toJSONObjectOrNull()
            ?.optJSONObject("scheduleState")

        if (scheduleState == null) {
            synchronized(stateLock) {
                retryAttempt = 0
                pendingJob?.cancel()
                pendingJob = null
                snapshotValue = BrowserStorageSyncSchedulerSnapshot(lastResultJson = resultJson)
            }
            return
        }

        val schedulerAction = scheduleState.optString("schedulerAction")
        val nextRetryAttempt = scheduleState.optInt("nextRetryAttempt", 0)
        val nextRunDelaySeconds = scheduleState.optNullableLong("nextRunDelaySeconds")
        val nextRunAtUnixSeconds = scheduleState.optNullableLong("nextRunAtUnixSeconds")
        val waitingForCredentials = scheduleState.optBoolean("waitingForCredentials", false)
        val requiresUserAction = scheduleState.optBoolean("requiresUserAction", false)
        val resetRetryAttempt = scheduleState.optBoolean("resetRetryAttempt", false)
        val updatedRetryAttempt = if (resetRetryAttempt) 0 else nextRetryAttempt
        val shouldSchedule = schedulesFollowUp &&
            nextRunDelaySeconds != null &&
            !waitingForCredentials

        synchronized(stateLock) {
            retryAttempt = updatedRetryAttempt
            snapshotValue = BrowserStorageSyncSchedulerSnapshot(
                retryAttempt = retryAttempt,
                schedulerAction = schedulerAction,
                nextRunDelaySeconds = nextRunDelaySeconds,
                nextRunAtUnixSeconds = nextRunAtUnixSeconds,
                waitingForCredentials = waitingForCredentials,
                requiresUserAction = requiresUserAction,
                pendingRunScheduled = shouldSchedule,
                lastResultJson = resultJson,
            )
        }

        if (shouldSchedule) {
            installPendingJob(nextRunDelaySeconds)
        } else {
            synchronized(stateLock) {
                pendingJob?.cancel()
                pendingJob = null
            }
        }
    }

    private fun installPendingJob(delaySeconds: Long) {
        synchronized(stateLock) {
            pendingJob?.cancel()
            pendingJob = scope.launch {
                delayRunner(delaySeconds)
                runScheduledSync()
            }
        }
    }
}

private fun String.toJSONObjectOrNull(): JSONObject? =
    try {
        if (this == "null") null else JSONObject(this)
    } catch (_: JSONException) {
        null
    }

private fun JSONObject.optNullableLong(name: String): Long? =
    if (has(name) && !isNull(name)) optLong(name) else null

private fun BrowserStorageSyncSchedulerSnapshot.toJSONObject(): JSONObject =
    JSONObject()
        .put("retryAttempt", retryAttempt)
        .put("schedulerAction", schedulerAction)
        .put("nextRunDelaySeconds", nextRunDelaySeconds ?: JSONObject.NULL)
        .put("nextRunAtUnixSeconds", nextRunAtUnixSeconds ?: JSONObject.NULL)
        .put("waitingForCredentials", waitingForCredentials)
        .put("requiresUserAction", requiresUserAction)
        .put("pendingRunScheduled", pendingRunScheduled)
        .put("lastResultJson", lastResultJson ?: JSONObject.NULL)

private fun browserStorageSyncSchedulerCurrentUnixSeconds(): Long =
    System.currentTimeMillis() / 1000
