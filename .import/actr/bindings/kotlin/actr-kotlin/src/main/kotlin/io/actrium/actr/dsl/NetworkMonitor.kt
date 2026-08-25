package io.actrium.actr.dsl

import android.app.ActivityManager
import android.content.Context
import android.net.ConnectivityManager
import android.os.SystemClock
import android.util.Log
import io.actrium.actr.ActrException
import io.actrium.actr.AppLifecycleState
import io.actrium.actr.CleanupReason
import io.actrium.actr.NetworkAvailability
import io.actrium.actr.NetworkSnapshot
import io.actrium.actr.NetworkTransportFlags
import io.actrium.actr.ReconnectReason
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/**
 * NetworkMonitor - Independent network state monitor
 *
 * Features:
 * - Monitor WiFi, mobile network, Ethernet, and VPN connection state changes
 * - Builds full NetworkSnapshot from Android system APIs and reports via
 *   handleNetworkPathChanged
 * - Supports app lifecycle callbacks (background/foreground) via
 *   handleAppLifecycleChanged
 * - Supports cleanupConnections and forceReconnect for explicit lifecycle ops
 * - Auto-detect initial network state
 * - Injects one authoritative initial app lifecycle state
 * - Support manual network state checks
 *
 * Logging output examples:
 * - Network path change events (full NetworkSnapshot)
 * - App lifecycle change events (background/foreground with duration)
 * - Current network status summary
 *
 * Usage:
 * 1. Create instance: NetworkMonitor(context, scope, onNetworkPathChanged, onAppLifecycleChanged)
 * 2. Start monitoring: startMonitoring()
 * 3. Stop monitoring: stopMonitoring() (usually called in Activity.onDestroy)
 * 4. Manual check: triggerNetworkCheck()
 * 5. Get status: getCurrentNetworkStatus()
 * 6. Cleanup/reconnect: cleanupConnections(), forceReconnect()
 *
 * Integration with ActrNode:
 * ```kotlin
 * val networkMonitor = NetworkMonitor.create(context, lifecycleScope) { system }
 * networkMonitor.startMonitoring()
 * ```
 *
 * On foreground return, report background_duration_ms to the Rust runtime,
 * which decides whether to connection-probe or force-reconnect.
 */
class NetworkMonitor
    (
    private val context: Context,
    private val scope: CoroutineScope,
    private val onNetworkPathChanged: (suspend (NetworkSnapshot) -> Unit)? = null,
    private val onAppLifecycleChanged: (suspend (AppLifecycleState) -> Unit)? = null,
    private val onCleanupConnections: (suspend (CleanupReason) -> Unit)? = null,
    private val onForceReconnect: (suspend (ReconnectReason) -> Unit)? = null,
) {
    companion object {
        private const val TAG = "NetworkMonitor"

        /**
         * Create a NetworkMonitor integrated with ActrNode
         *
         * This factory method automatically forwards network events to ActrNode's
         * NetworkEventHandle, so users don't need to handle network events manually.
         *
         * @param context Android Context
         * @param scope CoroutineScope, typically use lifecycleScope
         * @param getSystem Function to get ActrNode instance (may return null, e.g. before initialization)
         * @param onNetworkStatusLog Optional log callback to display network status changes
         * @return NetworkMonitor instance
         *
         * Example:
         * ```kotlin
         * var system: ActrNode? = null
         * val monitor = NetworkMonitor.create(this, lifecycleScope, { system }) { msg ->
         *     Log.d("App", msg)
         * }
         * monitor.startMonitoring()
         *
         * // Initialize system later
         * system = ActrNode.fromPackageFile("config.toml", "dist/app.actr")
         * ```
         */
        fun create(
            context: Context,
            scope: CoroutineScope,
            getSystem: () -> ActrNode?,
            onNetworkStatusLog: ((String) -> Unit)? = null,
        ): NetworkMonitor {
            val monitorHandle =
                MonitorEventHandleCache<NetworkEventHandle> {
                    getSystem()?.createNetworkMonitorEventHandle()
                }

            suspend fun getMonitorHandle(): NetworkEventHandle? = monitorHandle.getOrCreate()

            return NetworkMonitor(
                context = context,
                scope = scope,
                onNetworkPathChanged = { snapshot ->
                    handleNetworkPathChangedWithHandle(
                        { getMonitorHandle() },
                        snapshot,
                        onNetworkStatusLog,
                    )
                },
                onAppLifecycleChanged = { state ->
                    handleAppLifecycleChangedWithHandle(
                        { getMonitorHandle() },
                        state,
                        onNetworkStatusLog,
                    )
                },
                onCleanupConnections = { reason ->
                    handleCleanupConnectionsWithHandle(
                        { getMonitorHandle() },
                        reason,
                        onNetworkStatusLog,
                    )
                },
                onForceReconnect = { reason ->
                    handleForceReconnectWithHandle(
                        { getMonitorHandle() },
                        reason,
                        onNetworkStatusLog,
                    )
                },
            ).also { monitor ->
                monitor.persistentDiagnosticLog = onNetworkStatusLog
            }
        }

        /**
         * Create a NetworkMonitor integrated with NetworkEventHandle
         *
         * This factory method automatically forwards network events to the specified
         * NetworkEventHandle.
         *
         * @param context Android Context
         * @param scope CoroutineScope, typically use lifecycleScope
         * @param getHandle Function to get NetworkEventHandle instance (may return null)
         * @param onNetworkStatusLog Optional log callback to display network status changes
         * @return NetworkMonitor instance
         */
        fun createWithHandle(
            context: Context,
            scope: CoroutineScope,
            getHandle: () -> NetworkEventHandle?,
            onNetworkStatusLog: ((String) -> Unit)? = null,
        ): NetworkMonitor =
            NetworkMonitor(
                context = context,
                scope = scope,
                onNetworkPathChanged = { snapshot ->
                    handleNetworkPathChangedWithHandle(
                        { getHandle() },
                        snapshot,
                        onNetworkStatusLog,
                    )
                },
                onAppLifecycleChanged = { state ->
                    handleAppLifecycleChangedWithHandle(
                        { getHandle() },
                        state,
                        onNetworkStatusLog,
                    )
                },
                onCleanupConnections = { reason ->
                    handleCleanupConnectionsWithHandle(
                        { getHandle() },
                        reason,
                        onNetworkStatusLog,
                    )
                },
                onForceReconnect = { reason ->
                    handleForceReconnectWithHandle(
                        { getHandle() },
                        reason,
                        onNetworkStatusLog,
                    )
                },
            ).also { monitor ->
                monitor.persistentDiagnosticLog = onNetworkStatusLog
            }

        // ---- Handle-based variants ----

        private suspend fun handleNetworkPathChangedWithHandle(
            getHandle: suspend () -> NetworkEventHandle?,
            snapshot: NetworkSnapshot,
            onLog: ((String) -> Unit)?,
        ) {
            val handle = getHandle()
            if (handle == null) {
                Log.d(
                    TAG,
                    "NetworkEventHandle not available, skipping network path changed event",
                )
                return
            }

            try {
                val result = handle.handleNetworkPathChangedCatching(snapshot)
                result
                    .onSuccess { eventResult ->
                        Log.i(
                            TAG,
                            "Network path changed event handled successfully: $eventResult",
                        )
                        onLog?.invoke(
                            "🌐 Network path changed - " +
                                "avail=${snapshot.availability}, " +
                                "wifi=${snapshot.transport.wifi}",
                        )
                    }.onFailure { error ->
                        if (error is ActrException.Internal) {
                            Log.d(TAG, "Runtime node no longer available, skipping network path changed event")
                        } else {
                            Log.e(TAG, "Failed to handle network path changed event", error)
                            onLog?.invoke("❌ Network path changed event failed: ${error.message}")
                        }
                    }
            } catch (e: Exception) {
                Log.e(TAG, "Error handling network path changed", e)
                onLog?.invoke("❌ Network path changed error: ${e.message}")
            }
        }

        private suspend fun handleAppLifecycleChangedWithHandle(
            getHandle: suspend () -> NetworkEventHandle?,
            state: AppLifecycleState,
            onLog: ((String) -> Unit)?,
        ) {
            val handle = getHandle()
            if (handle == null) {
                Log.d(TAG, "NetworkEventHandle not available, skipping app lifecycle event")
                return
            }

            try {
                val result = handle.handleAppLifecycleChangedCatching(state)
                result
                    .onSuccess { eventResult ->
                        Log.i(TAG, "App lifecycle event handled successfully: $eventResult")
                        val label = when (state) {
                            is AppLifecycleState.Background -> "Background"
                            is AppLifecycleState.Foreground ->
                                "Foreground (bg_duration=${state.backgroundDurationMs}ms)"
                        }
                        onLog?.invoke("📱 App lifecycle: $label")
                    }.onFailure { error ->
                        if (error is ActrException.Internal) {
                            Log.d(TAG, "Runtime node no longer available, skipping app lifecycle event")
                        } else {
                            Log.e(TAG, "Failed to handle app lifecycle event", error)
                            onLog?.invoke("❌ App lifecycle event failed: ${error.message}")
                        }
                    }
            } catch (e: Exception) {
                Log.e(TAG, "Error handling app lifecycle", e)
                onLog?.invoke("❌ App lifecycle error: ${e.message}")
            }
        }

        private suspend fun handleCleanupConnectionsWithHandle(
            getHandle: suspend () -> NetworkEventHandle?,
            reason: CleanupReason,
            onLog: ((String) -> Unit)?,
        ) {
            val handle = getHandle()
            if (handle == null) {
                Log.d(TAG, "NetworkEventHandle not available, skipping cleanup connections")
                return
            }

            try {
                val result = handle.cleanupConnectionsCatching(reason)
                result
                    .onSuccess { eventResult ->
                        Log.i(TAG, "Cleanup connections handled successfully: $eventResult")
                        onLog?.invoke("🧹 Cleanup connections: $reason")
                    }.onFailure { error ->
                        if (error is ActrException.Internal) {
                            Log.d(TAG, "Runtime node no longer available, skipping cleanup connections")
                        } else {
                            Log.e(TAG, "Failed to cleanup connections", error)
                            onLog?.invoke("❌ Cleanup connections failed: ${error.message}")
                        }
                    }
            } catch (e: Exception) {
                Log.e(TAG, "Error cleaning up connections", e)
                onLog?.invoke("❌ Cleanup connections error: ${e.message}")
            }
        }

        private suspend fun handleForceReconnectWithHandle(
            getHandle: suspend () -> NetworkEventHandle?,
            reason: ReconnectReason,
            onLog: ((String) -> Unit)?,
        ) {
            val handle = getHandle()
            if (handle == null) {
                Log.d(TAG, "NetworkEventHandle not available, skipping force reconnect")
                return
            }

            try {
                val result = handle.forceReconnectCatching(reason)
                result
                    .onSuccess { eventResult ->
                        Log.i(TAG, "Force reconnect handled successfully: $eventResult")
                        onLog?.invoke("🔄 Force reconnect: $reason")
                    }.onFailure { error ->
                        if (error is ActrException.Internal) {
                            Log.d(TAG, "Runtime node no longer available, skipping force reconnect")
                        } else {
                            Log.e(TAG, "Failed to force reconnect", error)
                            onLog?.invoke("❌ Force reconnect failed: ${error.message}")
                        }
                    }
            } catch (e: Exception) {
                Log.e(TAG, "Error force reconnecting", e)
                onLog?.invoke("❌ Force reconnect error: ${e.message}")
            }
        }
    }

    private var networkSystem: DefaultNetworkSystem? = null
    private var isMonitoring = false
    private var persistentDiagnosticLog: ((String) -> Unit)? = null

    /**
     * Test seam: when set, replaces ConnectivityManager resolution so JVM
     * tests can drive the real callback wiring through a scripted system.
     */
    internal var networkSystemFactory: (() -> DefaultNetworkSystem)? = null
    private val eventAdapter = MobileEventAdapterState()

    /**
     * Fresh per registration: a stale callback surviving an unregister race
     * mutates the abandoned tracker instance, never the live one.
     */
    @Volatile
    private var networkPathTracker = DefaultNetworkPathTracker()
    private val lifecycleEvents =
        MobileLifecycleEventSource(
            state = eventAdapter,
            clock = MonotonicClock { SystemClock.elapsedRealtime() },
        )
    private data class NetworkDelivery(
        val snapshot: NetworkSnapshot,
        val diagnostic: String,
        val gate: MobileEventDeliveryGate,
    )

    private data class LifecycleDelivery(
        val state: AppLifecycleState,
        val reportNetworkSnapshotAfter: Boolean,
        val gate: MobileEventDeliveryGate,
    )

    @Volatile private var deliveryGate = MobileEventDeliveryGate()
    private val networkDeliveries =
        SerializedDeliveryQueue<NetworkDelivery>(scope) { delivery ->
            try {
                delivery.gate.deliver {
                    runCatching { persistentDiagnosticLog?.invoke(delivery.diagnostic) }
                        .onFailure { error ->
                            Log.w(
                                TAG,
                                "Persistent network diagnostic callback failed: ${error.message}",
                            )
                        }
                    onNetworkPathChanged?.invoke(delivery.snapshot)
                }
            } catch (e: Exception) {
                Log.e(TAG, "Failed to handle network path change: ${e.message}", e)
            }
        }
    private val lifecycleDeliveries =
        SerializedDeliveryQueue<LifecycleDelivery>(scope) { delivery ->
            try {
                delivery.gate.deliver {
                    onAppLifecycleChanged?.invoke(delivery.state)
                    if (delivery.reportNetworkSnapshotAfter) {
                        // Reconcile with the live default network: a
                        // foreground return after a silent network switch
                        // must publish the real path, not the stale tracker
                        // state captured before the backgrounding.
                        val transitions = refreshDefaultNetworkState()
                        if (transitions.isEmpty()) {
                            notifyNetworkPathChanged(
                                path = networkPathTracker.current(),
                                reason = "foreground_snapshot",
                                gate = delivery.gate,
                            )
                        } else {
                            publishNetworkTransitions(
                                transitions,
                                reason = "foreground_snapshot",
                                gate = delivery.gate,
                            )
                        }
                    }
                }
            } catch (e: Exception) {
                Log.e(TAG, "Failed to deliver app lifecycle state: ${e.message}", e)
            }
        }

    /** Start network monitoring */
    fun startMonitoring() {
        if (isMonitoring) {
            Log.d(TAG, "Network monitoring already running")
            return
        }
        // Registration re-announces the current default synchronously; those
        // facts are only as deliverable as this scope's delivery queue. A
        // dead scope would leave Android connected and signaling unaware.
        if (!scope.isActive) {
            Log.e(TAG, "Monitor scope is no longer active; refusing to start network monitoring")
            return
        }

        try {
            deliveryGate = deliveryGate.rotated()
            networkPathTracker = DefaultNetworkPathTracker()
            networkSystem =
                networkSystemFactory?.invoke()
                    ?: (context.getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager)
                        ?.let { ConnectivityManagerDefaultNetworkSystem(it) }
            if (networkSystem == null) {
                Log.w(TAG, "ConnectivityManager unavailable; default-network tracking is disabled")
            }

            setupNetworkCallback()

            isMonitoring = true
            Log.i(TAG, "Starting network state monitoring...")

            injectInitialLifecycleState()

            // Log initial network state
            logCurrentNetworkState("initial state")
        } catch (e: Exception) {
            Log.e(TAG, "Failed to start network monitoring: ${e.message}", e)
        }
    }

    /** Stop network monitoring */
    fun stopMonitoring() {
        deliveryGate.close()
        if (!isMonitoring) {
            return
        }

        isMonitoring = false
        try {
            networkSystem?.unregister()
        } catch (e: Exception) {
            // Monitor-owned state resets independently of Android
            // unregistration: the delivery gate is already closed, so a
            // stale connected reading could never be corrected by callbacks.
            Log.e(TAG, "Failed to unregister network callback: ${e.message}", e)
        } finally {
            networkSystem = null
            networkPathTracker.reset()
            Log.i(TAG, "Stopped network monitoring")
        }
    }

    // ---- App Lifecycle Methods ----

    @Synchronized
    private fun injectInitialLifecycleState() {
        val processInfo = ActivityManager.RunningAppProcessInfo()
        ActivityManager.getMyMemoryState(processInfo)
        val isForeground =
            processInfo.importance == ActivityManager.RunningAppProcessInfo.IMPORTANCE_FOREGROUND
        val state = lifecycleEvents.initializePhase(isForeground) ?: return
        Log.i(TAG, "Initial app lifecycle state: ${if (isForeground) "foreground" else "background"}")
        dispatchLifecycleState(state, reportNetworkSnapshotAfter = isForeground)
    }

    private fun dispatchLifecycleState(
        state: AppLifecycleState,
        reportNetworkSnapshotAfter: Boolean = false,
    ) {
        if (!lifecycleDeliveries.enqueue(
                LifecycleDelivery(state, reportNetworkSnapshotAfter, deliveryGate),
            )
        ) {
            Log.w(TAG, "Lifecycle delivery skipped because its scope is no longer active")
        }
    }

    /**
     * Call this from Activity.onPause / onStop when the app goes to background.
     *
     * This records the background entry time and notifies the runtime.
     */
    @Synchronized
    fun onAppBackground() {
        // elapsedRealtime is suspend-aware and immune to wall-clock changes
        // (NTP, user adjustment); the runtime's short/long-background decision
        // depends on this duration being trustworthy.
        val state = lifecycleEvents.enterBackground()
        if (state == null) {
            Log.d(TAG, "Ignoring duplicate app background callback")
            return
        }
        Log.i(TAG, "App entered background on the suspend-aware monotonic clock")
        dispatchLifecycleState(state)
    }

    /**
     * Call this from Activity.onResume / onStart when the app returns to foreground.
     *
     * This computes the background duration, notifies the runtime, and reports
     * the current NetworkSnapshot.
     */
    @Synchronized
    fun onAppForeground() {
        val state = lifecycleEvents.enterForeground()
        val backgroundDurationMs = (state as AppLifecycleState.Foreground).backgroundDurationMs

        Log.i(TAG, "App returned to foreground, background duration: ${backgroundDurationMs}ms")
        dispatchLifecycleState(state, reportNetworkSnapshotAfter = true)
    }

    /** Cleanup connections without reconnecting (e.g. app terminating, user logout). */
    fun cleanupConnections(reason: CleanupReason = CleanupReason.MANUAL_RESET) {
        val gate = deliveryGate
        scope.launch(Dispatchers.IO) {
            try {
                gate.deliver { onCleanupConnections?.invoke(reason) }
            } catch (e: Exception) {
                Log.e(TAG, "Failed to cleanup connections: ${e.message}", e)
            }
        }
    }

    /** Force cleanup and reconnect (e.g. manual reconnect, long background). */
    fun forceReconnect(reason: ReconnectReason = ReconnectReason.MANUAL_RECONNECT) {
        val gate = deliveryGate
        scope.launch(Dispatchers.IO) {
            try {
                gate.deliver { onForceReconnect?.invoke(reason) }
            } catch (e: Exception) {
                Log.e(TAG, "Failed to force reconnect: ${e.message}", e)
            }
        }
    }

    // ---- NetworkSnapshot Construction ----

    /** Build a NetworkSnapshot from the current network state. */
    private fun buildCurrentNetworkSnapshot(
        path: DefaultNetworkPath = networkPathTracker.current(),
    ): NetworkSnapshot =
        eventAdapter.snapshot(
            availability = path.availability,
            transport = path.transport,
            isExpensive = path.isExpensive,
            isConstrained = path.isConstrained,
        )

    /** Track only the default network used for new connections. */
    private fun setupNetworkCallback() {
        val system = networkSystem ?: return
        val callbackGate = deliveryGate
        val callbackAdapter =
            DefaultNetworkCallbackAdapter(networkPathTracker) { transitions, reason ->
                publishNetworkTransitions(transitions, reason, callbackGate)
            }

        system.register(
            object : DefaultNetworkEvents {
                override fun onAvailable(networkHandle: Long) {
                    // API 26+ guarantees an ordered onCapabilitiesChanged next.
                    // Synchronous ConnectivityManager queries inside callbacks
                    // can return stale data, so only establish handle identity.
                    Log.i(TAG, "Default network available: handle=$networkHandle")
                    callbackAdapter.onAvailable(networkHandle)
                }

                override fun onCapabilitiesChanged(path: DefaultNetworkPath) {
                    Log.d(
                        TAG,
                        "Default network capabilities changed: handle=${path.networkHandle}, " +
                            "availability=${path.availability}, capsKnown=${path.capabilitiesKnown}, " +
                            "wifi=${path.transport.wifi}, cell=${path.transport.cellular}, " +
                            "vpn=${path.transport.vpn}",
                    )
                    callbackAdapter.onCapabilitiesChanged(path)
                }

                override fun onLost(networkHandle: Long) {
                    Log.w(TAG, "Default network lost: handle=$networkHandle")
                    callbackAdapter.onLost(networkHandle)
                }
            },
        )
    }

    private fun refreshDefaultNetworkState(): List<DefaultNetworkPath> {
        val system = networkSystem ?: return networkPathTracker.refresh(null)
        return networkPathTracker.refresh(system.currentPath())
    }

    private fun publishNetworkTransitions(
        transitions: List<DefaultNetworkPath>,
        reason: String,
        gate: MobileEventDeliveryGate = deliveryGate,
    ) {
        transitions.forEachIndexed { index, path ->
            notifyNetworkPathChanged(
                path = path,
                reason = if (transitions.size == 1) reason else "${reason}_step_${index + 1}",
                gate = gate,
            )
        }
    }

    /** Notify the upper layer about a network path change via the unified callback. */
    private fun notifyNetworkPathChanged(
        path: DefaultNetworkPath = networkPathTracker.current(),
        reason: String,
        gate: MobileEventDeliveryGate = deliveryGate,
    ) {
        val snapshot = buildCurrentNetworkSnapshot(path)
        val diagnostic = path.diagnosticLine(snapshot.sequence, reason)
        Log.i(
            TAG,
            diagnostic,
        )
        if (!networkDeliveries.enqueue(NetworkDelivery(snapshot, diagnostic, gate))) {
            Log.w(TAG, "Network path delivery skipped because its scope is no longer active")
        }
    }

    /** Log current network state */
    private fun logCurrentNetworkState(context: String = "") {
        val path = networkPathTracker.current()
        val contextStr = if (context.isNotEmpty()) " ($context)" else ""
        Log.i(
            TAG,
            "Current network state$contextStr: handle=${path.networkHandle}, " +
                "availability=${path.availability}, wifi=${path.transport.wifi}, " +
                "cell=${path.transport.cellular}, vpn=${path.transport.vpn}",
        )
    }

    /** Get current network status summary */
    fun getCurrentNetworkStatus(): String {
        val path = networkPathTracker.current()
        if (path.availability == NetworkAvailability.UNAVAILABLE) return "No network connection"
        if (path.availability == NetworkAvailability.UNKNOWN) {
            // Distinguish a still-validating default from one whose
            // capability snapshot is missing outright.
            return if (path.capabilitiesKnown) "Network validating" else "Network state unknown"
        }
        return when {
            path.transport.vpn -> "VPN"
            path.transport.wifi -> "WiFi"
            path.transport.cellular -> "Cellular"
            path.transport.ethernet -> "Ethernet"
            else -> "Network (unknown type)"
        }
    }

    /** Manually trigger network state check */
    fun triggerNetworkCheck() {
        Log.i(TAG, "Manually triggering network state check")
        val transitions = refreshDefaultNetworkState()
        logCurrentNetworkState("manual check")
        if (transitions.isEmpty()) {
            notifyNetworkPathChanged(reason = "manual_check")
        } else {
            publishNetworkTransitions(transitions, reason = "manual_check")
        }
    }

    /**
     * Check whether the default network is Internet-capable, validated, and
     * not suspended.
     *
     * Migration note: this previously returned `true` for any active network
     * with capabilities. A network that is still validating (captive portal,
     * slow validation) or suspended now reads as `false`; use
     * [getCurrentNetworkStatus] to distinguish a validating network from no
     * network at all.
     */
    fun isConnected(): Boolean =
        networkPathTracker.current().availability == NetworkAvailability.AVAILABLE

    /** Check if currently connected via WiFi */
    fun isWifi(): Boolean = networkPathTracker.current().transport.wifi

    /** Check if currently connected via mobile network */
    fun isCellular(): Boolean = networkPathTracker.current().transport.cellular

    /** Check if currently connected via VPN */
    fun isVpn(): Boolean = networkPathTracker.current().transport.vpn

    /** Check if currently connected via Ethernet */
    fun isEthernet(): Boolean = networkPathTracker.current().transport.ethernet

    /** Check if current network is metered/expensive */
    fun isNetworkExpensive(): Boolean = networkPathTracker.current().isExpensive

    /** Check if current network is constrained/congested */
    fun isNetworkConstrained(): Boolean = networkPathTracker.current().isConstrained
}

// ============================================================================
// ActrNode Integration — one-shot monitor setup
// ============================================================================

/**
 * Create and start a [NetworkMonitor] wired to this [ActrNode].
 *
 * The returned monitor is already started. The node reference is captured
 * immediately, so you don't need a lazy lambda.
 *
 * Example:
 * ```kotlin
 * val system = ActrNode.fromPackageFile("config.toml", "dist/app.actr")
 * val monitor = system.createNetworkMonitor(this, lifecycleScope) { Log.d("App", it) }
 * ```
 *
 * @param context Android Context (for ConnectivityManager)
 * @param scope CoroutineScope (typically lifecycleScope)
 * @param onNetworkStatusLog Optional callback for network status messages
 * @return A started [NetworkMonitor] instance
 */
fun ActrNode.createNetworkMonitor(
    context: android.content.Context,
    scope: kotlinx.coroutines.CoroutineScope,
    onNetworkStatusLog: ((String) -> Unit)? = null,
): NetworkMonitor {
    val monitor = NetworkMonitor.create(
        context = context,
        scope = scope,
        getSystem = { this },
        onNetworkStatusLog = onNetworkStatusLog,
    )
    monitor.startMonitoring()
    return monitor
}
