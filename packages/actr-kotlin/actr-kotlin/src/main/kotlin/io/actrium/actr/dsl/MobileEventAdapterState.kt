package io.actrium.actr.dsl

import io.actrium.actr.AppLifecycleState
import io.actrium.actr.NetworkAvailability
import io.actrium.actr.NetworkSnapshot
import io.actrium.actr.NetworkTransportFlags
import java.util.concurrent.atomic.AtomicLong

internal fun interface MonotonicClock {
    fun nowMs(): Long
}

internal class MobileLifecycleEventSource(
    private val state: MobileEventAdapterState = MobileEventAdapterState(),
    private val clock: MonotonicClock,
) {
    fun initializePhase(isForeground: Boolean): AppLifecycleState? =
        state.initializePhase(isForeground, clock.nowMs())

    fun enterBackground(): AppLifecycleState? = state.enterBackground(clock.nowMs())

    fun enterForeground(): AppLifecycleState = state.enterForeground(clock.nowMs())
}

internal class MobileEventAdapterState {
    private val sequenceCounter = AtomicLong(0)

    @Volatile
    private var backgroundEnteredAtMs: Long? = null

    @Volatile
    private var lifecycleInitialized = false

    @Synchronized
    fun initializePhase(
        isForeground: Boolean,
        nowMs: Long,
    ): AppLifecycleState? {
        if (lifecycleInitialized) {
            return null
        }
        lifecycleInitialized = true
        return if (isForeground) {
            backgroundEnteredAtMs = null
            AppLifecycleState.Foreground(0uL)
        } else {
            backgroundEnteredAtMs = nowMs
            AppLifecycleState.Background
        }
    }

    @Synchronized
    fun enterBackground(nowMs: Long): AppLifecycleState? {
        lifecycleInitialized = true
        if (backgroundEnteredAtMs != null) {
            return null
        }
        backgroundEnteredAtMs = nowMs
        return AppLifecycleState.Background
    }

    @Synchronized
    fun enterForeground(nowMs: Long): AppLifecycleState {
        lifecycleInitialized = true
        val backgroundDurationMs =
            backgroundEnteredAtMs?.let { start ->
                (nowMs - start).coerceAtLeast(0)
            } ?: 0L
        backgroundEnteredAtMs = null
        return AppLifecycleState.Foreground(backgroundDurationMs.toULong())
    }

    fun snapshot(
        availability: NetworkAvailability,
        transport: NetworkTransportFlags,
        isExpensive: Boolean,
        isConstrained: Boolean,
    ): NetworkSnapshot =
        NetworkSnapshot(
            sequence = sequenceCounter.incrementAndGet().toULong(),
            availability = availability,
            transport = transport,
            isExpensive = isExpensive,
            isConstrained = isConstrained,
        )
}

internal data class DefaultNetworkPath(
    val networkHandle: Long?,
    val availability: NetworkAvailability,
    /**
     * Whether a capability snapshot backed this path. An `UNKNOWN` path with
     * `capabilitiesKnown = false` describes a present default without a
     * readable snapshot — not a network still validating — and is reported
     * distinctly so diagnostics do not conflate the two.
     */
    val capabilitiesKnown: Boolean = true,
    val transport: NetworkTransportFlags =
        NetworkTransportFlags(
            wifi = false,
            cellular = false,
            ethernet = false,
            vpn = false,
            other = false,
        ),
    val isExpensive: Boolean = false,
    val isConstrained: Boolean = false,
    val isValidated: Boolean = false,
    val isSuspended: Boolean = false,
) {
    fun diagnosticLine(
        sequence: ULong,
        reason: String,
    ): String =
        "network_monitor.path_transition reason=${diagnosticReasonToken(reason)} sequence=$sequence " +
            "networkHandle=${networkHandle ?: "none"} availability=$availability " +
            "capsKnown=$capabilitiesKnown " +
            "validated=$isValidated suspended=$isSuspended " +
            "wifi=${transport.wifi} cellular=${transport.cellular} " +
            "ethernet=${transport.ethernet} vpn=${transport.vpn} " +
            "expensive=$isExpensive constrained=$isConstrained"

    companion object {
        fun unavailable(): DefaultNetworkPath =
            DefaultNetworkPath(
                networkHandle = null,
                availability = NetworkAvailability.UNAVAILABLE,
            )
    }
}

/**
 * Serializes Android default-network callbacks into one authoritative path.
 *
 * A new network handle always passes through [NetworkAvailability.UNKNOWN],
 * even when its first capabilities snapshot is already validated. This makes a
 * same-transport reconnect material to the runtime when Android omits or delays
 * the old handle's `onLost` callback.
 */
internal class DefaultNetworkPathTracker {
    private var current = DefaultNetworkPath.unavailable()

    @Synchronized
    fun current(): DefaultNetworkPath = current

    @Synchronized
    fun reset() {
        current = DefaultNetworkPath.unavailable()
    }

    @Synchronized
    fun onAvailable(networkHandle: Long): List<DefaultNetworkPath> = observe(networkHandle, null)

    @Synchronized
    fun onCapabilitiesChanged(observed: DefaultNetworkPath): List<DefaultNetworkPath> {
        if (observed.networkHandle != current.networkHandle) return emptyList()
        return commit(observed)
    }

    @Synchronized
    fun onLost(networkHandle: Long): List<DefaultNetworkPath> {
        if (current.networkHandle != networkHandle) return emptyList()
        return commit(DefaultNetworkPath.unavailable())
    }

    @Synchronized
    fun refresh(observed: DefaultNetworkPath?): List<DefaultNetworkPath> =
        if (observed == null) {
            commit(DefaultNetworkPath.unavailable())
        } else {
            observe(requireNotNull(observed.networkHandle), observed)
        }

    private fun observe(
        networkHandle: Long,
        observed: DefaultNetworkPath?,
    ): List<DefaultNetworkPath> {
        val transitions = mutableListOf<DefaultNetworkPath>()
        if (current.networkHandle != networkHandle) {
            val unknown =
                observed
                    ?.copy(availability = NetworkAvailability.UNKNOWN)
                    ?: DefaultNetworkPath(
                        networkHandle = networkHandle,
                        availability = NetworkAvailability.UNKNOWN,
                    )
            transitions += commit(unknown)
        }
        if (observed != null) transitions += commit(observed)
        return transitions
    }

    private fun commit(next: DefaultNetworkPath): List<DefaultNetworkPath> {
        if (current == next) return emptyList()
        current = next
        return listOf(next)
    }
}

/** The callback-order seam shared by Android wiring and deterministic JVM tests. */
internal class DefaultNetworkCallbackAdapter(
    private val tracker: DefaultNetworkPathTracker,
    private val publish: (List<DefaultNetworkPath>, String) -> Unit,
) {
    fun onAvailable(networkHandle: Long) {
        dispatch(tracker.onAvailable(networkHandle), "default_on_available")
    }

    fun onCapabilitiesChanged(observed: DefaultNetworkPath) {
        dispatch(tracker.onCapabilitiesChanged(observed), "default_capabilities_changed")
    }

    fun onLost(networkHandle: Long) {
        dispatch(tracker.onLost(networkHandle), "default_on_lost")
    }

    private fun dispatch(
        transitions: List<DefaultNetworkPath>,
        reason: String,
    ) {
        if (transitions.isNotEmpty()) publish(transitions, reason)
    }
}

internal fun diagnosticReasonToken(reason: String): String =
    reason.map { character ->
        if (character.isLetterOrDigit() || character == '_' || character == '-' || character == '.') {
            character
        } else {
            '_'
        }
    }.joinToString(separator = "")

internal fun classifyNetworkAvailability(
    hasInternet: Boolean,
    isValidated: Boolean,
    isNotSuspended: Boolean,
): NetworkAvailability =
    if (hasInternet && isValidated && isNotSuspended) {
        NetworkAvailability.AVAILABLE
    } else {
        NetworkAvailability.UNKNOWN
    }
