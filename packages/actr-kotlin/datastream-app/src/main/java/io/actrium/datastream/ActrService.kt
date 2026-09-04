package io.actrium.datastream

import android.content.Context
import android.util.Log
import io.actrium.actr.ActrId
import io.actrium.actr.PayloadType
import io.actrium.actr.dsl.ActrContext
import io.actrium.actr.dsl.ActrNode
import io.actrium.actr.dsl.ActrRef
import io.actrium.actr.dsl.DynamicWorkload
import io.actrium.actr.dsl.ErrorEvent
import io.actrium.actr.dsl.LogCallback
import io.actrium.actr.dsl.Manifest
import io.actrium.actr.dsl.PeerEvent
import io.actrium.actr.dsl.RpcEnvelope
import io.actrium.actr.dsl.SignalingObserver
import io.actrium.actr.dsl.WebRtcObserver
import io.actrium.actr.dsl.WebRtcPeerState
import io.actrium.actr.dsl.WebRtcPeerStatus
import io.actrium.actr.dsl.WebSocketObserver
import io.actrium.actr.dsl.Workload
import io.actrium.actr.dsl.call
import io.actrium.actr.dsl.dynamicWorkload
import io.actrium.actr.dsl.linkedWithMonitoring
import io.actrium.actr.dsl.setLogCallback
import io.actrium.actr.dsl.toActrType
import io.actrium.datastream.proto.StartProbeRequest
import io.actrium.datastream.proto.StartProbeResponse
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.withContext
import java.io.File
import java.io.RandomAccessFile
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

private const val MAX_LOG_LINES = 2000
private const val MAX_RECEIVED_ECHO_LINES = 200
private const val MAX_FILE_LOG_SIZE = 20L * 1024L * 1024L // 20 MB
private const val MAX_ECHO_RETRIES = 3
private const val TAG = "DataStreamApp"
private const val DEFAULT_COLD_START_ECHO = "hello"

enum class SignalingHookStatus { INITIAL, CONNECTING, CONNECTED, DISCONNECTED }
enum class WebSocketHookStatus { INITIAL, CONNECTING, CONNECTED, DISCONNECTED }
enum class WebSocketHookEventKind { CONNECTING, CONNECTED, DISCONNECTED }
enum class WebRtcHookEventKind { CONNECTING, CONNECTED, DISCONNECTED }

/// Single immutable snapshot of all @Published-equivalent UI state. Updated
/// atomically via `MutableStateFlow.update`; the Compose UI collects `uiState`.
data class DataStreamUiState(
    val status: String = "Starting ACTR node...",
    val errorMessage: String? = null,
    val results: List<ProbeResult> = emptyList(),
    val isRunning: Boolean = false,
    val isSendingStream: Boolean = false,
    val logLines: List<String> = emptyList(),
    val receivedEchoLines: List<String> = emptyList(),
    val echoResult: String? = null,
    val isSendingEcho: Boolean = false,
    val signalingReady: Boolean = false,
    val foregroundRecoveryPending: Boolean = false,
    val webRtcStatus: WebRtcPeerStatus = WebRtcPeerStatus.IDLE,
    val signalingHookStatus: SignalingHookStatus = SignalingHookStatus.INITIAL,
    val signalingHookEventCount: Int = 0,
    val webSocketHookStatus: WebSocketHookStatus = WebSocketHookStatus.INITIAL,
    val webSocketHookEventCount: Int = 0,
    val webSocketConnectingCount: Int = 0,
    val webSocketConnectedCount: Int = 0,
    val webSocketDisconnectedCount: Int = 0,
    val webRtcHookEventCount: Int = 0,
    val webRtcConnectedCount: Int = 0,
    val webRtcDisconnectedCount: Int = 0,
    val webRtcRecoveringCount: Int = 0,
    val webRtcIdleCount: Int = 0,
)

private enum class ActrServiceError { MISSING_CONFIG_TEMPLATE }

/**
 * Orders foreground-triggered work behind the runtime's asynchronous lifecycle
 * reconciliation without changing ActrNode.onAppForeground's fire-and-forget
 * API.
 *
 * The wait is bounded by [pendingGraceMs]. The runtime confirms reconciliation
 * via a lifecycle marker delivered on a debounced/coalesced broadcast stream, so
 * that marker can be lost or arrive out of order — a strict request/completion
 * counter then wedges the gate forever (Send Echo stuck grey, no auto echo). A
 * time-bounded wait keeps the "hold a beat for reconciliation" safety without
 * deadlocking; the real send gate is enforced independently by `signalingReady`
 * / `webRtcStatus` in `canSend`.
 */
internal class ForegroundRecoveryGate(
    private val pendingGraceMs: Long = DEFAULT_PENDING_GRACE_MS,
    private val nanoTime: () -> Long = System::nanoTime,
) {
    private var isForeground = true

    /** Nano-time the current foreground began awaiting reconciliation; 0 = not waiting. */
    @Volatile
    private var pendingSinceNanos = 0L

    val isRecoveryPending: Boolean
        get() = isForeground && pendingSinceNanos != 0L &&
            (nanoTime() - pendingSinceNanos) < pendingGraceMs * NANOS_PER_MS

    val canAttemptSend: Boolean
        get() = isForeground && !isRecoveryPending

    fun onForeground(runtimeAvailable: Boolean) {
        isForeground = true
        // Only wait for a reconciliation the runtime is actually there to confirm.
        pendingSinceNanos = if (runtimeAvailable) nanoTime() else 0L
    }

    fun onBackground() {
        isForeground = false
        pendingSinceNanos = 0L
    }

    /**
     * Runtime confirmed the foreground reconciliation: release the wait now.
     * Returns true only when the app is still foregrounded.
     */
    fun onRuntimeForegroundHandled(): Boolean {
        pendingSinceNanos = 0L
        return canAttemptSend
    }

    companion object {
        const val DEFAULT_PENDING_GRACE_MS = 2_000L
        private const val NANOS_PER_MS = 1_000_000L
    }
}

/// 1:1 port of ActrService.swift (@MainActor ObservableObject → a class whose
/// state lives in a `MutableStateFlow<DataStreamUiState>`, with mutations
/// serialized on `Dispatchers.Main`, mirroring Swift's @MainActor isolation).
class ActrService(
    private val appContext: Context,
    private val serviceScope: CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.Main),
) {
    val uiState = MutableStateFlow(DataStreamUiState())
    val state: StateFlow<DataStreamUiState> get() = uiState

    private var actrNode: ActrNode? = null
    private var actorRef: ActrRef? = null
    private var isStarting = false
    private val foregroundRecoveryGate = ForegroundRecoveryGate()

    val isReady: Boolean get() = actorRef != null

    /// Hook state is advisory. A foreground lifecycle notification is
    /// fire-and-forget, so do not reuse a pre-background CONNECTED value until
    /// NetworkMonitor confirms that runtime reconciliation has completed.
    val canSend: Boolean
        get() = uiState.value.let {
            foregroundRecoveryGate.canAttemptSend &&
                it.signalingReady &&
                (it.webRtcStatus == WebRtcPeerStatus.IDLE || it.webRtcStatus == WebRtcPeerStatus.CONNECTED) &&
                !it.isSendingEcho
        }

    val webRtcHookStatusName: String
        get() = when (uiState.value.webRtcStatus) {
            WebRtcPeerStatus.IDLE -> "idle"
            WebRtcPeerStatus.CONNECTING -> "connecting"
            WebRtcPeerStatus.CONNECTED -> "connected"
            WebRtcPeerStatus.RECOVERING -> "recovering"
        }

    /// Pending foreground echo message + retry state (see requestForegroundEcho
    /// / trySendEcho / handleEchoFailure).
    private var pendingEcho: String? = null
    private var echoRetryCount = 0
    private var echoFallbackJob: Job? = null
    private var gateWatchdogJob: Job? = null
    /// Wall-clock start of the in-flight echo (set in sendEcho, consumed in
    /// runEchoProbe to print send→response latency). Read across threads
    /// (runEchoProbe runs on IO) so it's @Volatile.
    @Volatile
    private var echoStartTime: Long = 0L

    // MARK: - File Logger

    private val fileLogLock = Any()
    private var fileLogRaf: RandomAccessFile? = null
    private val fileLogPath: String by lazy { File(appContext.filesDir, "datastream_app.log").absolutePath }

    private fun fileLog(message: String) {
        val ts = SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ssXXX", Locale.US).format(Date())
        val line = "[$ts] $message\n"
        synchronized(fileLogLock) {
            try {
                val raf = fileLogRaf ?: return
                if (raf.length() > MAX_FILE_LOG_SIZE) {
                    raf.setLength(0)
                }
                raf.seek(raf.length())
                raf.write(line.toByteArray(Charsets.UTF_8))
            } catch (e: Exception) {
                Log.w(TAG, "fileLog write failed: ${e.message}")
            }
        }
    }

    private fun setupFileLog() {
        synchronized(fileLogLock) {
            try {
                val file = File(fileLogPath)
                if (!file.exists()) file.createNewFile()
                fileLogRaf = RandomAccessFile(file, "rw")
                fileLog("[DataStreamApp] 📝 File log started at ${file.absolutePath}")
            } catch (e: Exception) {
                fileLog("[DataStreamApp] ⚠️ Failed to open file log handle at $fileLogPath")
            }
        }
    }

    private fun updateState(f: (DataStreamUiState) -> DataStreamUiState) {
        uiState.update(f)
    }

    // MARK: - Hook telemetry recorders

    fun recordSignalingHook(state: SignalingHookStatus) {
        updateState {
            it.copy(signalingHookStatus = state, signalingHookEventCount = it.signalingHookEventCount + 1)
        }
        fileLog("[DataStreamApp] hook_state.signaling state=$state events=${uiState.value.signalingHookEventCount}")
    }

    fun recordWebSocketHook(state: WebSocketHookStatus, event: WebSocketHookEventKind) {
        updateState {
            val next = it.copy(
                webSocketHookStatus = state,
                webSocketHookEventCount = it.webSocketHookEventCount + 1,
                webSocketConnectingCount = it.webSocketConnectingCount + if (event == WebSocketHookEventKind.CONNECTING) 1 else 0,
                webSocketConnectedCount = it.webSocketConnectedCount + if (event == WebSocketHookEventKind.CONNECTED) 1 else 0,
                webSocketDisconnectedCount = it.webSocketDisconnectedCount + if (event == WebSocketHookEventKind.DISCONNECTED) 1 else 0,
            )
            next
        }
        val s = uiState.value
        fileLog("[DataStreamApp] hook_state.websocket state=$state event=$event events=${s.webSocketHookEventCount} connecting=${s.webSocketConnectingCount} connected=${s.webSocketConnectedCount} disconnected=${s.webSocketDisconnectedCount}")
    }

    fun recordWebRtcHook(state: WebRtcPeerStatus, event: WebRtcHookEventKind) {
        updateState {
            val next = it.copy(
                webRtcStatus = state,
                webRtcHookEventCount = it.webRtcHookEventCount + 1,
                webRtcConnectedCount = it.webRtcConnectedCount + if (event == WebRtcHookEventKind.CONNECTED) 1 else 0,
                webRtcDisconnectedCount = it.webRtcDisconnectedCount + if (event == WebRtcHookEventKind.DISCONNECTED) 1 else 0,
                webRtcRecoveringCount = it.webRtcRecoveringCount +
                    (if (event == WebRtcHookEventKind.DISCONNECTED && state == WebRtcPeerStatus.RECOVERING) 1 else 0),
                webRtcIdleCount = it.webRtcIdleCount +
                    (if (event == WebRtcHookEventKind.DISCONNECTED && state == WebRtcPeerStatus.IDLE) 1 else 0),
            )
            next
        }
        val s = uiState.value
        fileLog("[DataStreamApp] hook_state.webrtc state=${webRtcHookStatusName} event=$event events=${s.webRtcHookEventCount} connected=${s.webRtcConnectedCount} disconnected=${s.webRtcDisconnectedCount} recovering=${s.webRtcRecoveringCount} idle=${s.webRtcIdleCount}")
    }

    // MARK: - Startup

    suspend fun startIfNeeded() {
        if (actorRef != null || isStarting) return
        isStarting = true
        try {
            setupFileLog()
            try {
                val configPath = materializeRuntimeConfig()

                // Actor type from manifest.toml [package] (the Android-idiomatic
                // equivalent of Swift's readManufacturerFromConfig + ActrType(
                // manufacturer:name:version:) — same org-zhj:DataStreamApp:0.1.0).
                val manifestPath = copyAssetToFiles("manifest.toml")
                val actorType = Manifest.from(File(manifestPath)).packageType()

                val handler = ProbeHandlerImpl(this)
                val workload: DynamicWorkload = dynamicWorkload(
                    lifecycle = ProbeLifecycleAdapter(ProbeServiceWorkload(handler)),
                    signaling = SignalingStateObserver(this),
                    websocket = PeerWebSocketObserver(this),
                    webrtc = PeerConnectionObserver(this),
                )

                // Must be called before any ActrNode is created.
                setLogCallback(ActrLogHandler(this))

                val node = linkedWithMonitoring(
                    configPath = configPath,
                    actorType = actorType,
                    workload = workload,
                    context = appContext,
                    scope = serviceScope,
                    onNetworkStatusLog = ::handleNetworkStatusLog,
                )
                val ref = node.start()

                actrNode = node
                actorRef = ref
                updateState { it.copy(status = "Ready: ${actorType.manufacturer}:${actorType.name}:${actorType.version}") }
                fileLog("[DataStreamApp] ✅ node started")
                // Reset retry state from any prior node, but keep a pending
                // foreground echo (armed during startup — e.g. the user
                // backgrounded+returned before the node was ready) and
                // re-trigger it now that actorRef is set. trySendEcho is
                // guarded on actorRef != null, so this is the first point at
                // which a startup-armed echo can actually send.
                echoFallbackJob?.cancel()
                echoFallbackJob = null
                echoRetryCount = 0
                if (pendingEcho != null) {
                    serviceScope.launch { trySendEcho() }
                } else {
                    // Cold start: the foreground trigger deliberately skips the
                    // first (cold-start) resume, so nothing else arms an echo on a
                    // fresh launch. Arm the initial echo here so the app validates
                    // connectivity as soon as the node and signaling are ready
                    // (SignalingStateObserver.onConnected re-triggers trySendEcho).
                    armEchoAndWatchdog(DEFAULT_COLD_START_ECHO)
                }
            } catch (e: Throwable) {
                updateState { it.copy(status = "ACTR startup failed: $e", errorMessage = "$e") }
                fileLog("[DataStreamApp] ❌ Startup failed: $e")
            }
        } finally {
            isStarting = false
        }
    }

    suspend fun stop() {
        val ref = actorRef
        val node = actrNode
        if (ref == null) return
        ref.stop()
        node?.close()
        actorRef = null
        actrNode = null
    }

    fun onAppForeground() {
        val node = actrNode
        foregroundRecoveryGate.onForeground(runtimeAvailable = node != null)
        updateState { it.copy(foregroundRecoveryPending = foregroundRecoveryGate.isRecoveryPending) }
        fileLog(
            "[DataStreamApp] foreground_recovery state=" +
                if (foregroundRecoveryGate.isRecoveryPending) "waiting" else "not-required",
        )
        node?.onAppForeground()
    }

    fun onAppBackground() {
        foregroundRecoveryGate.onBackground()
        updateState { it.copy(foregroundRecoveryPending = false) }
        actrNode?.onAppBackground()
    }

    fun cleanupConnections() { actrNode?.cleanupConnections() }

    /**
     * NetworkMonitor invokes this after the corresponding runtime operation has
     * completed. Its foreground success marker therefore acts as the demo's
     * readiness barrier.
     */
    private fun handleNetworkStatusLog(line: String) {
        fileLog("[DataStreamApp] network_monitor $line")
        val foregroundHandled = line.contains("App lifecycle: Foreground")
        if (!foregroundHandled) return

        serviceScope.launch {
            val wasPending = foregroundRecoveryGate.isRecoveryPending
            val canAttempt = foregroundRecoveryGate.onRuntimeForegroundHandled()
            updateState { it.copy(foregroundRecoveryPending = foregroundRecoveryGate.isRecoveryPending) }
            val recoveryState =
                when {
                    foregroundRecoveryGate.isRecoveryPending -> "waiting"
                    wasPending -> "complete"
                    else -> "ignored"
                }
            fileLog(
                "[DataStreamApp] foreground_recovery state=$recoveryState",
            )
            // The marker arrived, so the watchdog isn't needed anymore.
            gateWatchdogJob?.cancel()
            gateWatchdogJob = null
            if (wasPending && canAttempt) trySendEcho()
        }
    }

    val shouldAutoRun: Boolean get() = envFlag("ACTR_DATASTREAMAPP_AUTO_RUN")

    val autoStreamCount: Int?
        get() = System.getenv("ACTR_DATASTREAMAPP_AUTO_STREAM_COUNT")?.toIntOrNull()?.let { if (it > 0) it else null }

    val autoResultFilename: String? get() = System.getenv("ACTR_DATASTREAMAPP_AUTO_RESULT_FILE")

    private fun envFlag(name: String): Boolean = System.getenv(name) == "1"

    // MARK: - Probe entrypoints (call the local ProbeService.StartProbe RPC)

    suspend fun runAllProbes() {
        if (actorRef == null || uiState.value.isRunning) {
            Log.w(TAG, "runAllProbes: actorRef=${actorRef != null} isRunning=${uiState.value.isRunning}")
            return
        }
        fileLog("[DataStreamApp] runAllProbes: calling StartProbe RPC...")
        updateState {
            it.copy(isRunning = true, results = emptyList(), logLines = listOf("--- Starting DataStream probe run ---"))
        }
        // isRunning is cleared by the spawned probe task on completion.

        val req = StartProbeRequest.newBuilder()
            .setProbeName("run-all")
            .setTargetType(targetType())
            .build()
        try {
            val resp = callStartProbe(req, 30_000L)
            if (resp.started) {
                appendBoundedLog("StartProbe dispatched: msg=${resp.message}")
            } else {
                appendBoundedLog("[FAIL] StartProbe not started: ${resp.message}")
                updateState { it.copy(isRunning = false) }
            }
        } catch (e: Throwable) {
            appendBoundedLog("[FAIL] StartProbe RPC failed: $e")
            fileLog("[DataStreamApp] StartProbe RPC failed: $e")
            updateState { it.copy(isRunning = false) }
        }
    }

    suspend fun sendHelloStreamChunks(count: Int) {
        if (actorRef == null || uiState.value.isSendingStream || uiState.value.isRunning) {
            Log.w(TAG, "sendHelloStreamChunks: actorRef=${actorRef != null} isSending=${uiState.value.isSendingStream} isRunning=${uiState.value.isRunning}")
            return
        }
        if (count <= 0) {
            appendBoundedLog("[FAIL] chunk count must be greater than 0")
            return
        }

        fileLog("[DataStreamApp] sendHelloStreamChunks: count=$count")
        updateState {
            it.copy(
                isSendingStream = true,
                receivedEchoLines = emptyList(),
                logLines = listOf("--- Starting manual stream echo request: count=$count ---"),
            )
        }
        // isSendingStream is cleared by the spawned probe task on completion.

        val req = StartProbeRequest.newBuilder()
            .setProbeName("stream-echo:$count")
            .setTargetType(targetType())
            .build()
        try {
            val resp = callStartProbe(req, streamEchoRequestTimeoutMs(count))
            if (resp.started) {
                appendBoundedLog("StreamEcho dispatched: msg=${resp.message}")
            } else {
                appendBoundedLog("[FAIL] StreamEcho not started: ${resp.message}")
                updateState { it.copy(isSendingStream = false) }
            }
        } catch (e: Throwable) {
            appendBoundedLog("[FAIL] StreamEcho RPC failed: $e")
            fileLog("[DataStreamApp] StreamEcho RPC failed: $e")
            updateState { it.copy(isSendingStream = false) }
        }
    }

    suspend fun sendEcho(msg: String): Boolean {
        if (actorRef == null || uiState.value.isSendingEcho) {
            Log.w(TAG, "sendEcho: actorRef=${actorRef != null} isSending=${uiState.value.isSendingEcho}")
            return false
        }
        val trimmed = msg.trim()
        if (trimmed.isEmpty()) {
            appendBoundedLog("[FAIL] echo message must not be empty")
            return false
        }

        fileLog("[DataStreamApp] sendEcho: msg=$trimmed")
        echoStartTime = System.nanoTime()
        updateState {
            it.copy(isSendingEcho = true, echoResult = null, logLines = it.logLines + "--- Sending Echo call: msg=$trimmed ---")
        }
        // isSendingEcho is cleared by the spawned probe task on completion.

        val req = StartProbeRequest.newBuilder()
            .setProbeName("echo:$trimmed")
            .setTargetType(targetType())
            .build()
        try {
            val resp = callStartProbe(req, 30_000L)
            if (resp.started) {
                appendBoundedLog("Echo dispatched: msg=${resp.message}")
                return true
            } else {
                updateState { it.copy(echoResult = "[FAIL] ${resp.message}") }
                appendBoundedLog("[FAIL] Echo not started: ${resp.message}")
                updateState { it.copy(isSendingEcho = false) }
                return false
            }
        } catch (e: Throwable) {
            updateState { it.copy(echoResult = "[FAIL] $e") }
            appendBoundedLog("[FAIL] Echo RPC failed: $e")
            fileLog("[DataStreamApp] Echo RPC failed: $e")
            updateState { it.copy(isSendingEcho = false) }
            return false
        }
    }

    private suspend fun callStartProbe(req: StartProbeRequest, timeoutMs: Long): StartProbeResponse =
        withContext(Dispatchers.IO) {
            // Run the RPC off the main thread: the first remote-probe RPC
            // triggers ws peer establishment (discover + handshake) which can
            // block; keeping it off Main avoids MIUI's main-thread freeze
            // detector killing the process. (Swift runs probe work in
            // detached Tasks, off the main actor.)
            val ref = actorRef ?: throw IllegalStateException("actor not started")
            val bytes = ref.call(
                "local.ProbeService.StartProbe",
                req.toByteArray(),
                PayloadType.RPC_RELIABLE,
                timeoutMs,
            )
            StartProbeResponse.parseFrom(bytes)
        }

    private fun targetType(): String =
        System.getenv("ACTR_DATASTREAMAPP_TARGET_TYPE") ?: "org-zhj:DuplexStreamService:1.0.1"

    // MARK: - Log management

    fun exportLogFile(): File {
        val formatter = SimpleDateFormat("yyyyMMdd-HHmmss", Locale.US)
        val filename = "DataStreamApp-${formatter.format(Date())}.log"
        val out = File(appContext.cacheDir, filename)
        out.writeText(uiState.value.logLines.joinToString("\n") + "\n")
        return out
    }

    fun writeAutoResultFile(filename: String) {
        val safeFilename = filename.replace("/", "_")
        val dir = File(appContext.filesDir, "results").apply { mkdirs() }
        val out = File(dir, safeFilename)
        out.writeText(uiState.value.logLines.joinToString("\n") + "\n")
        fileLog("[DataStreamApp] wrote auto result file: ${out.absolutePath}")
    }

    fun appendBoundedLog(line: String) {
        updateState {
            val lines = it.logLines + line
            it.copy(logLines = if (lines.size > MAX_LOG_LINES) lines.drop(lines.size - MAX_LOG_LINES) else lines)
        }
    }

    /// Clears the in-app log display, received stream lines, echo result, and
    /// probe results. The on-disk file log is untouched (full history via
    /// exportLogFile()).
    fun clearLogs() {
        logEpoch.incrementAndGet()
        updateState { it.copy(logLines = emptyList(), receivedEchoLines = emptyList(), echoResult = null, results = emptyList()) }
    }

    fun logToFile(message: String) { fileLog(message) }

    fun appendStreamLog(line: String, receivedLine: String? = null) {
        appendBoundedLog(line)
        if (receivedLine != null) {
            updateState {
                val lines = it.receivedEchoLines + receivedLine
                it.copy(receivedEchoLines = if (lines.size > MAX_RECEIVED_ECHO_LINES) lines.drop(lines.size - MAX_RECEIVED_ECHO_LINES) else lines)
            }
        }
    }

    // MARK: - Foreground Echo (per ACTR WebRTC Hook doc)

    /// Called by the UI when the app returns to the foreground. Arms the echo,
    /// then waits for both runtime foreground reconciliation and hook readiness.
    fun requestForegroundEcho(msg: String) {
        val trimmed = msg.trim()
        if (trimmed.isEmpty()) return
        fileLog(
            "[DataStreamApp] requestForegroundEcho: msg='$trimmed', canSend=$canSend, " +
                "foregroundRecoveryPending=${foregroundRecoveryGate.isRecoveryPending}, " +
                "signalingReady=${uiState.value.signalingReady}, webRtcStatus=$webRtcHookStatusName",
        )
        armEchoAndWatchdog(trimmed)
    }

    /**
     * Arms the echo and attempts to send it. A watchdog backstops the foreground
     * gate: the runtime's reconciliation marker is normally near-instant, but it
     * can be lost or arrive out of order (the runtime debounces/coalesces
     * lifecycle facts), which would otherwise wedge the gate forever (greyed Send
     * Echo, no auto echo). Once the grace window elapses the watchdog releases the
     * gate, re-enables the Send button, and fires the echo. The watchdog is a
     * no-op when the echo already sent (pendingEcho consumed) or the marker
     * already cleared the gate.
     */
    private fun armEchoAndWatchdog(msg: String) {
        pendingEcho = msg
        echoRetryCount = 0
        echoFallbackJob?.cancel()
        echoFallbackJob = null
        gateWatchdogJob?.cancel()
        gateWatchdogJob = null
        if (foregroundRecoveryGate.canAttemptSend) {
            serviceScope.launch { trySendEcho() }
        } else {
            fileLog("[DataStreamApp] foreground echo armed; waiting for runtime foreground reconciliation")
            gateWatchdogJob = serviceScope.launch {
                delay(ForegroundRecoveryGate.DEFAULT_PENDING_GRACE_MS)
                foregroundRecoveryGate.onRuntimeForegroundHandled()
                updateState { it.copy(foregroundRecoveryPending = foregroundRecoveryGate.isRecoveryPending) }
                if (pendingEcho != null) {
                    fileLog("[DataStreamApp] echo watchdog fired (no reconciliation marker within grace)")
                    trySendEcho()
                }
            }
        }
    }

    /// Send the pending echo if canSend. Called on foreground and on every
    /// signalingReady / webRtcStatus change (hook-driven retries).
    suspend fun trySendEcho() {
        val msg = pendingEcho
        // Don't send during startup (actorRef == null): the node isn't ready,
        // so sendEcho would return false and trip the "echo failed, retry"
        // path — and startIfNeeded's post-start reset would cancel the retry.
        // pendingEcho stays armed; startIfNeeded re-triggers trySendEcho once
        // the node is ready.
        if (msg == null || actorRef == null || !canSend) return
        pendingEcho = null
        fileLog("[DataStreamApp] trySendEcho: sending msg='$msg' (retryCount=$echoRetryCount)")
        val ok = sendEcho(msg)
        if (!ok) handleEchoFailure(msg)
    }

    /// On send failure: re-arm pendingEcho (if retries remain) and start a
    /// fallback timer. The next canSend state change retriggers trySendEcho
    /// too; whichever fires first sends, the other is a no-op.
    private suspend fun handleEchoFailure(msg: String) {
        echoRetryCount += 1
        if (echoRetryCount > MAX_ECHO_RETRIES) {
            fileLog("[DataStreamApp] echo retry exhausted (${echoRetryCount - 1} failures)")
            appendBoundedLog("[FAIL] Foreground echo gave up after ${echoRetryCount - 1} retries")
            echoFallbackJob?.cancel()
            echoFallbackJob = null
            return
        }
        pendingEcho = msg
        fileLog("[DataStreamApp] echo failed, will retry $echoRetryCount/$MAX_ECHO_RETRIES on canSend or in 3s")
        appendBoundedLog("--- Foreground echo failed, retry $echoRetryCount/$MAX_ECHO_RETRIES ---")
        echoFallbackJob?.cancel()
        echoFallbackJob = serviceScope.launch {
            delay(3_000L)
            trySendEcho()
        }
    }

    private fun streamEchoRequestTimeoutMs(count: Int): Long = maxOf(60_000L, count.toLong() * 2_500L + 60_000L)

    // MARK: - Runtime config materialization

    /// Reads actr.toml from assets, appends hyper data_dir + trust config,
    /// writes to filesDir. (1:1 with Swift materializeRuntimeConfig.)
    private fun materializeRuntimeConfig(): String {
        val template = appContext.assets.open("actr.toml").use { it.readBytes().toString(Charsets.UTF_8) }
        val hyperDir = File(appContext.filesDir, "hyper").apply { mkdirs() }
        val config = template + "\n\n[hyper]\ndata_dir = \"${hyperDir.absolutePath}\"\n\n[hyper.trust]\nkind = \"dev_only\"\n"
        val configFile = File(appContext.filesDir, "actr.toml")
        configFile.writeText(config)
        return configFile.absolutePath
    }

    private fun copyAssetToFiles(assetName: String): String {
        val outFile = File(appContext.filesDir, assetName)
        outFile.parentFile?.mkdirs()
        appContext.assets.open(assetName).use { input ->
            outFile.outputStream().use { output -> input.copyTo(output) }
        }
        return outFile.absolutePath
    }

    private companion object {
        val logEpoch = java.util.concurrent.atomic.AtomicInteger(0)
    }

    // MARK: - ProbeService RPC Handler (private, real handler)

    /// Implements ProbeServiceHandler.startProbe. When this RPC fires, ctx is
    /// delivered — discover target then run all probes.
    private class ProbeHandlerImpl(private val service: ActrService) : ProbeServiceHandler {
        override suspend fun startProbe(req: StartProbeRequest, ctx: ActrContext): StartProbeResponse {
            service.fileLog("[DataStreamApp] 🔵 startProbe handler, discovering DuplexStreamService...")

            // Discover target synchronously so we can return immediately if not found.
            val targetTypeStr = req.targetType.ifEmpty { "org-zhj:DuplexStreamService:1.0.1" }
            val targetType = targetTypeStr.toActrType()
            val target: ActrId
            try {
                target = ctx.discover(targetType)
                service.fileLog("[DataStreamApp] Discovered target: ${target.type.manufacturer}:${target.type.name}")
            } catch (e: Throwable) {
                service.fileLog("[DataStreamApp] ❌ discover failed: $e")
                return StartProbeResponse.newBuilder().setStarted(false).setMessage("discover failed: $e").build()
            }

            val runner = DataStreamProbeRunner(ctx = ctx, target = target)

            // Spawn the probe work and return immediately so the actor mailbox
            // is freed for concurrent RPCs. ctx is an owned handle — safe to
            // capture here and use after this handler returns.
            streamEchoCount(req.probeName)?.let { count ->
                service.serviceScope.launch(Dispatchers.IO) { runStreamEchoProbe(runner, count) }
                return StartProbeResponse.newBuilder().setStarted(true).setMessage("stream-echo dispatched: count=$count").build()
            }

            echoMessage(req.probeName)?.let { msg ->
                service.serviceScope.launch(Dispatchers.IO) { runEchoProbe(runner, msg) }
                return StartProbeResponse.newBuilder().setStarted(true).setMessage("echo dispatched: msg=$msg").build()
            }

            service.serviceScope.launch(Dispatchers.IO) { runAllProbe(runner) }
            return StartProbeResponse.newBuilder().setStarted(true).setMessage("run-all dispatched").build()
        }

        // MARK: spawned probe tasks

        private suspend fun runStreamEchoProbe(runner: DataStreamProbeRunner, count: Int) {
            val result = runner.runHelloStream(count) { line, receivedLine ->
                service.appendStreamLog(line, receivedLine)
            }
            for (line in result.logLines) service.fileLog("[DataStreamApp] $line")
            val expectedLines = (1..count).map { "received: echo: hello $it" }
            val passCount = result.receivedLines.zip(expectedLines).count { (a, b) -> a == b }
            service.updateState {
                it.copy(receivedEchoLines = result.receivedLines, isSendingStream = false)
            }
            emitE2EResult("$passCount/$count")
        }

        private suspend fun runEchoProbe(runner: DataStreamProbeRunner, msg: String) {
            val echoRun = runner.runEcho(msg) { line -> service.appendBoundedLog(line) }
            for (line in echoRun.logLines) service.fileLog("[DataStreamApp] $line")
            val startNs = service.echoStartTime
            val elapsedMs = if (startNs > 0) (System.nanoTime() - startNs) / 1_000_000 else -1L
            service.echoStartTime = 0L
            service.updateState {
                it.copy(
                    echoResult = "${echoRun.message}  [send→response ${elapsedMs}ms]",
                    isSendingEcho = false,
                )
            }
            service.fileLog("[DataStreamApp] echo done: send→response ${elapsedMs}ms  msg='$msg' succeeded=${echoRun.succeeded}")
            service.appendBoundedLog("Echo round-trip (send→response): ${elapsedMs}ms")
            emitE2EResult("echo:${echoRun.echoedMsg}")
        }

        private suspend fun runAllProbe(runner: DataStreamProbeRunner) {
            val allResults = runner.runAll()
            for (r in allResults) {
                val status = if (r.passed) "PASS" else "FAIL"
                service.fileLog("[DataStreamApp] [$status] ${r.name} (${r.durationMs}ms): ${r.details}")
            }
            val passCount = allResults.count { it.passed }
            service.fileLog("[DataStreamApp] Done: $passCount/${allResults.size} passed")
            service.updateState {
                val logAdd = allResults.flatMap { it.logLines }
                it.copy(results = allResults, isRunning = false, logLines = it.logLines + logAdd)
            }
            emitE2EResult("$passCount/${allResults.size}")
        }

        private fun streamEchoCount(probeName: String): Int? {
            val prefix = "stream-echo:"
            if (!probeName.startsWith(prefix)) return null
            val value = probeName.removePrefix(prefix)
            return value.toIntOrNull()?.let { if (it > 0) it else null }
        }

        private fun echoMessage(probeName: String): String? {
            val prefix = "echo:"
            if (!probeName.startsWith(prefix)) return null
            return probeName.removePrefix(prefix)
        }
    }

    // MARK: - Lifecycle Adapter (private, real adapter)

    private class ProbeLifecycleAdapter(
        private val workload: ProbeServiceWorkload<ProbeHandlerImpl>,
    ) : Workload {
        override suspend fun onStart(ctx: ActrContext) {}
        override suspend fun onReady(ctx: ActrContext) {}
        override suspend fun onStop(ctx: ActrContext) {}

        override suspend fun onError(ctx: ActrContext, event: ErrorEvent) {
            fileLogGlobal("[DataStreamApp] ProbeLifecycleAdapter error: $event")
        }

        override suspend fun dispatch(ctx: ActrContext, envelope: RpcEnvelope): ByteArray =
            workload.dispatch(ctx, envelope)
    }

    // MARK: - Signaling Observer

    /// Drives signalingReady from the Signaling hook. signalingReady only
    /// means the route/discover layer is usable — NOT that a target's WebRTC
    /// is up (per the hook doc, don't conflate the two).
    private class SignalingStateObserver(private val service: ActrService) : SignalingObserver {
        override suspend fun onConnecting(ctx: ActrContext?) {
            fileLogGlobal("[DataStreamApp] SignalingObserver: onConnecting")
            withContext(Dispatchers.Main) {
                service.recordSignalingHook(SignalingHookStatus.CONNECTING)
                service.updateState { it.copy(signalingReady = false) }
            }
        }

        override suspend fun onConnected(ctx: ActrContext?) {
            fileLogGlobal("[DataStreamApp] SignalingObserver: onConnected")
            withContext(Dispatchers.Main) {
                service.recordSignalingHook(SignalingHookStatus.CONNECTED)
                service.updateState { it.copy(signalingReady = true) }
                service.trySendEcho()
            }
        }

        override suspend fun onDisconnected(ctx: ActrContext) {
            fileLogGlobal("[DataStreamApp] SignalingObserver: onDisconnected")
            withContext(Dispatchers.Main) {
                service.recordSignalingHook(SignalingHookStatus.DISCONNECTED)
                service.updateState { it.copy(signalingReady = false) }
            }
        }
    }

    // MARK: - WebSocket Peer Observer

    private class PeerWebSocketObserver(private val service: ActrService) : WebSocketObserver {
        override suspend fun onConnecting(ctx: ActrContext, event: PeerEvent) {
            fileLogGlobal("[DataStreamApp] WebSocketObserver: onConnecting")
            withContext(Dispatchers.Main) { service.recordWebSocketHook(WebSocketHookStatus.CONNECTING, WebSocketHookEventKind.CONNECTING) }
        }

        override suspend fun onConnected(ctx: ActrContext, event: PeerEvent) {
            fileLogGlobal("[DataStreamApp] WebSocketObserver: onConnected")
            withContext(Dispatchers.Main) { service.recordWebSocketHook(WebSocketHookStatus.CONNECTED, WebSocketHookEventKind.CONNECTED) }
        }

        override suspend fun onDisconnected(ctx: ActrContext, event: PeerEvent) {
            fileLogGlobal("[DataStreamApp] WebSocketObserver: onDisconnected")
            withContext(Dispatchers.Main) { service.recordWebSocketHook(WebSocketHookStatus.DISCONNECTED, WebSocketHookEventKind.DISCONNECTED) }
        }
    }

    // MARK: - WebRTC Peer Observer

    /// Drives webRtcStatus from the WebRTC hook. Per the hook doc:
    /// Idle/Connected are sendable; Connecting/Recovering are not. On
    /// onConnected, cancel any pending fallback timer and retry a pending echo
    /// immediately (connected hook wins).
    private class PeerConnectionObserver(private val service: ActrService) : WebRtcObserver {
        override suspend fun onConnecting(ctx: ActrContext, event: PeerEvent) {
            fileLogGlobal("[DataStreamApp] WebRTCObserver: onConnecting")
            withContext(Dispatchers.Main) { service.recordWebRtcHook(WebRtcPeerStatus.CONNECTING, WebRtcHookEventKind.CONNECTING) }
        }

        override suspend fun onConnected(ctx: ActrContext, event: PeerEvent) {
            fileLogGlobal("[DataStreamApp] WebRTCObserver: onConnected relayed=${event.relayed}")
            withContext(Dispatchers.Main) {
                service.recordWebRtcHook(WebRtcPeerStatus.CONNECTED, WebRtcHookEventKind.CONNECTED)
                service.echoFallbackJob?.cancel()
                service.trySendEcho()
            }
        }

        override suspend fun onDisconnected(ctx: ActrContext, event: PeerEvent) {
            // status is Recovering (will recover) or Idle (cleaned up, lazy-create).
            val status = event.status ?: WebRtcPeerStatus.IDLE
            fileLogGlobal("[DataStreamApp] WebRTCObserver: onDisconnected status=$status")
            withContext(Dispatchers.Main) {
                service.recordWebRtcHook(status, WebRtcHookEventKind.DISCONNECTED)
                // Idle is sendable (send creates a fresh connection): retry a
                // pending echo instead of waiting for onConnected (which never
                // comes after cleanup).
                if (status == WebRtcPeerStatus.IDLE) service.trySendEcho()
            }
        }

        override suspend fun onStateChanged(ctx: ActrContext, state: WebRtcPeerState) {
            fileLogGlobal("[DataStreamApp] WebRTCObserver: onStateChanged peer=${state.peer} status=${state.status} sendable=${state.sendable} rev=${state.revision}")
        }
    }

    // MARK: - Actr Rust Log Forwarding

    /// Filters + forwards actr runtime tracing logs to the file log + the
    /// in-app logLines buffer (cap 2000). The webrtc crate's internal logs
    /// (target "log"/"webrtc_*") are dropped entirely (they flood; actr_hyper
    /// already summarizes WebRTC state).
    private class ActrLogHandler(private val service: ActrService) : LogCallback {
        // nonisolated(unsafe)-style: the runtime calls onLog from arbitrary
        // threads; appends are atomic via MutableStateFlow.update.
        override fun onLog(level: String, target: String, message: String, timestampMs: Long) {
            if (target == "log" || target.startsWith("webrtc")) return
            val entry = "[actr|$level] $target: $message"
            fileLogGlobal(entry)
            val epoch = logEpoch.get()
            // Drop the entry if clearLogs() ran while it was being processed.
            if (logEpoch.get() != epoch) return
            service.appendBoundedLog(entry)
        }
    }
}

private fun fileLogGlobal(message: String) {
    // Routed through the singleton service instance's file logger when
    // available; ActrLogHandler / observers call this for diagnostic lines that
    // don't go through the UI buffer. Implemented as best-effort stderr logcat.
    Log.println(Log.INFO, TAG, message)
}

/// Emits the E2E result marker so a CI harness can grep for it (Swift prints
/// to stdout; Android has none, so logcat stands in).
private fun emitE2EResult(result: String) {
    Log.println(Log.INFO, TAG, "ACTR_E2E_RESULT:$result")
}
