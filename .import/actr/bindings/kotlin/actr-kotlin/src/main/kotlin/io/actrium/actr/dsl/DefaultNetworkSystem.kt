package io.actrium.actr.dsl

import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.os.Build
import android.util.Log
import io.actrium.actr.NetworkAvailability
import io.actrium.actr.NetworkTransportFlags

/**
 * Default-network events as Android delivers them for
 * [ConnectivityManager.registerDefaultNetworkCallback]: callbacks arrive
 * in order on the registration's callback thread, and the registration-time
 * re-announcement of the current default (if any) starts with
 * [onAvailable]. The monitor relies only on that concrete contract —
 * in-order delivery on one thread — to treat a non-current handle as stale.
 */
internal interface DefaultNetworkEvents {
    fun onAvailable(networkHandle: Long)

    fun onCapabilitiesChanged(path: DefaultNetworkPath)

    fun onLost(networkHandle: Long)
}

/**
 * The ConnectivityManager boundary [NetworkMonitor] drives. JVM tests script a
 * fake to exercise the monitor's real callback wiring; production registers a
 * default-network callback and reads the live default network.
 */
internal interface DefaultNetworkSystem {
    /**
     * Register for default-network events. Android re-announces the current
     * default network, if any, through [events] immediately.
     */
    fun register(events: DefaultNetworkEvents)

    /** Undo the last [register]. */
    fun unregister()

    /**
     * The live default-network path, or `null` when there is no default
     * network. A present default without capabilities reads as `UNKNOWN`.
     */
    fun currentPath(): DefaultNetworkPath?
}

/** Production [DefaultNetworkSystem] over [ConnectivityManager]. */
internal class ConnectivityManagerDefaultNetworkSystem(
    private val manager: ConnectivityManager,
) : DefaultNetworkSystem {
    private val registrationLock = Any()
    private var callback: ConnectivityManager.NetworkCallback? = null

    override fun register(events: DefaultNetworkEvents) =
        synchronized(registrationLock) {
            // A previous registration (repeated startup, or a startup whose
            // failure was swallowed by the monitor's catch) must not leak its
            // callback into ConnectivityManager's internal listener list.
            unregisterLocked()
            val registered =
                object : ConnectivityManager.NetworkCallback() {
                    override fun onAvailable(network: Network) {
                        super.onAvailable(network)
                        events.onAvailable(network.networkHandle)
                    }

                    override fun onCapabilitiesChanged(
                        network: Network,
                        networkCapabilities: NetworkCapabilities,
                    ) {
                        super.onCapabilitiesChanged(network, networkCapabilities)
                        events.onCapabilitiesChanged(
                            defaultNetworkPath(network, networkCapabilities),
                        )
                    }

                    override fun onLost(network: Network) {
                        super.onLost(network)
                        events.onLost(network.networkHandle)
                    }

                    override fun onLinkPropertiesChanged(
                        network: Network,
                        linkProperties: android.net.LinkProperties,
                    ) {
                        super.onLinkPropertiesChanged(network, linkProperties)
                        Log.d(TAG, "Network link properties changed: $network")
                    }
                }
            manager.registerDefaultNetworkCallback(registered)
            callback = registered
        }

    override fun unregister() = synchronized(registrationLock) { unregisterLocked() }

    private fun unregisterLocked() {
        callback?.let { manager.unregisterNetworkCallback(it) }
        callback = null
    }

    override fun currentPath(): DefaultNetworkPath? {
        val network = manager.activeNetwork ?: return null
        val capabilities = manager.getNetworkCapabilities(network)
        return if (capabilities != null) {
            defaultNetworkPath(network, capabilities)
        } else {
            // A present default without a readable snapshot is distinct from
            // one still validating: mark the snapshot as missing.
            DefaultNetworkPath(
                networkHandle = network.networkHandle,
                availability = NetworkAvailability.UNKNOWN,
                capabilitiesKnown = false,
            )
        }
    }

    private companion object {
        private const val TAG = "NetworkMonitor"
    }
}

/** Map one Android capabilities snapshot onto the tracked default-network path. */
internal fun defaultNetworkPath(
    network: Network,
    capabilities: NetworkCapabilities,
): DefaultNetworkPath {
    val hasInternet =
        capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
    val isValidated =
        capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
    val isNotSuspended =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.P ||
            capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_SUSPENDED)
    val wifi = capabilities.hasTransport(NetworkCapabilities.TRANSPORT_WIFI)
    val cellular = capabilities.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR)
    val ethernet = capabilities.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET)
    val vpn = capabilities.hasTransport(NetworkCapabilities.TRANSPORT_VPN)

    return DefaultNetworkPath(
        networkHandle = network.networkHandle,
        availability =
            classifyNetworkAvailability(
                hasInternet = hasInternet,
                isValidated = isValidated,
                isNotSuspended = isNotSuspended,
            ),
        transport =
            NetworkTransportFlags(
                wifi = wifi,
                cellular = cellular,
                ethernet = ethernet,
                vpn = vpn,
                other = !wifi && !cellular && !ethernet && !vpn,
            ),
        isExpensive =
            !capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED),
        isConstrained =
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                !capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_CONGESTED)
            } else {
                false
            },
        isValidated = isValidated,
        isSuspended = !isNotSuspended,
    )
}
