package io.actrium.actr.dsl

import android.content.ContextWrapper
import io.actrium.actr.NetworkAvailability
import io.actrium.actr.NetworkSnapshot
import io.actrium.actr.NetworkTransportFlags
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.TimeUnit
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.runBlocking

/**
 * Drives the real [NetworkMonitor] callback wiring through a scripted
 * [DefaultNetworkSystem]: registration, system-delivered initial state,
 * stale-capability filtering, reordered loss delivery, live manual refresh,
 * and stop-time unregistration with state reset.
 */
class NetworkMonitorWiringTest {
    private class ScriptedNetworkSystem : DefaultNetworkSystem {
        var events: DefaultNetworkEvents? = null
            private set
        var registerCount = 0
            private set
        var unregisterCount = 0
            private set
        var currentPath: DefaultNetworkPath? = null

        override fun register(events: DefaultNetworkEvents) {
            registerCount += 1
            this.events = events
        }

        override fun unregister() {
            unregisterCount += 1
            events = null
        }

        override fun currentPath(): DefaultNetworkPath? = currentPath

        fun deliverAvailable(networkHandle: Long) {
            requireNotNull(events).onAvailable(networkHandle)
        }

        fun deliverCapabilities(path: DefaultNetworkPath) {
            requireNotNull(events).onCapabilitiesChanged(path)
        }

        fun deliverLost(networkHandle: Long) {
            requireNotNull(events).onLost(networkHandle)
        }
    }

    private fun wifiPath(
        networkHandle: Long,
        availability: NetworkAvailability,
    ): DefaultNetworkPath =
        DefaultNetworkPath(
            networkHandle = networkHandle,
            availability = availability,
            transport =
                NetworkTransportFlags(
                    wifi = true,
                    cellular = false,
                    ethernet = false,
                    vpn = false,
                    other = false,
                ),
        )

    @Test
    fun `monitor registers once and publishes system-delivered initial state in order`() =
        runBlocking {
            val system = ScriptedNetworkSystem()
            val snapshots = CopyOnWriteArrayList<NetworkSnapshot>()
            val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
            try {
                val monitor =
                    NetworkMonitor(
                        context = ContextWrapper(null),
                        scope = scope,
                        onNetworkPathChanged = { snapshots += it },
                    )
                monitor.networkSystemFactory = { system }

                monitor.startMonitoring()
                assertEquals(1, system.registerCount, "expected exactly one system registration")

                // registerDefaultNetworkCallback re-announces the current
                // default immediately: available first, then its capabilities.
                system.deliverAvailable(100)
                system.deliverCapabilities(wifiPath(100, NetworkAvailability.AVAILABLE))

                // FIFO marker: once the manual-check snapshot lands, every
                // earlier delivery has been published and no stragglers can
                // arrive after the assertions below.
                system.currentPath = wifiPath(100, NetworkAvailability.AVAILABLE)
                monitor.triggerNetworkCheck()
                snapshots.awaitCount(3)

                assertEquals(
                    listOf(
                        NetworkAvailability.UNKNOWN,
                        NetworkAvailability.AVAILABLE,
                        NetworkAvailability.AVAILABLE,
                    ),
                    snapshots.map { it.availability },
                    "published availabilities in callback order",
                )
                assertEquals(
                    listOf(1uL, 2uL, 3uL),
                    snapshots.map { it.sequence },
                    "sequences are monotonic and gap-free",
                )
                // The UNKNOWN step asserts handle identity only; transport
                // claims arrive with the first capabilities snapshot.
                assertFalse(
                    snapshots[0].transport.wifi,
                    "unknown step carries no transport claim",
                )
                assertTrue(
                    snapshots.drop(1).all { it.transport.wifi },
                    "wifi transport is carried through",
                )
                assertTrue(monitor.isConnected(), "validated default reads as connected")
            } finally {
                scope.cancel()
            }
        }

    @Test
    fun `late callbacks for a replaced default never regress the published path`() =
        runBlocking {
            val system = ScriptedNetworkSystem()
            val snapshots = CopyOnWriteArrayList<NetworkSnapshot>()
            val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
            try {
                val monitor =
                    NetworkMonitor(
                        context = ContextWrapper(null),
                        scope = scope,
                        onNetworkPathChanged = { snapshots += it },
                    )
                monitor.networkSystemFactory = { system }
                monitor.startMonitoring()

                system.deliverAvailable(100)
                system.deliverCapabilities(wifiPath(100, NetworkAvailability.AVAILABLE))
                snapshots.awaitCount(2)

                // Same-transport replacement while the old handle's loss is
                // still missing: the replacement must step through UNKNOWN.
                system.deliverAvailable(101)
                snapshots.awaitCount(3)
                assertEquals(NetworkAvailability.UNKNOWN, snapshots[2].availability)

                // Stale capabilities and the reordered loss of the old handle
                // publish nothing: the next real transition lands at index 3.
                system.deliverCapabilities(wifiPath(100, NetworkAvailability.AVAILABLE))
                system.deliverLost(100)
                system.deliverCapabilities(wifiPath(101, NetworkAvailability.AVAILABLE))
                snapshots.awaitCount(4)
                assertEquals(NetworkAvailability.AVAILABLE, snapshots[3].availability)

                // Losing the replacement clears the path; the stale loss stays inert.
                system.deliverLost(101)
                snapshots.awaitCount(5)
                assertEquals(NetworkAvailability.UNAVAILABLE, snapshots[4].availability)
                assertFalse(monitor.isConnected())
            } finally {
                scope.cancel()
            }
        }

    @Test
    fun `manual check refreshes from the live system and stop unregisters and resets`() =
        runBlocking {
            val system = ScriptedNetworkSystem()
            val snapshots = CopyOnWriteArrayList<NetworkSnapshot>()
            val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
            try {
                val monitor =
                    NetworkMonitor(
                        context = ContextWrapper(null),
                        scope = scope,
                        onNetworkPathChanged = { snapshots += it },
                    )
                monitor.networkSystemFactory = { system }
                monitor.startMonitoring()

                system.deliverAvailable(200)
                system.deliverCapabilities(wifiPath(200, NetworkAvailability.AVAILABLE))
                snapshots.awaitCount(2)

                // A manual check against the same live path re-publishes the
                // current snapshot but introduces no transition steps.
                system.currentPath = wifiPath(200, NetworkAvailability.AVAILABLE)
                monitor.triggerNetworkCheck()
                snapshots.awaitCount(3)
                assertEquals(NetworkAvailability.AVAILABLE, snapshots[2].availability)

                // A live path switch observed by manual refresh steps through
                // UNKNOWN before the validated state, like the callback path.
                system.currentPath = wifiPath(300, NetworkAvailability.AVAILABLE)
                monitor.triggerNetworkCheck()
                snapshots.awaitCount(5)
                assertEquals(
                    listOf(NetworkAvailability.UNKNOWN, NetworkAvailability.AVAILABLE),
                    snapshots.subList(3, 5).map { it.availability },
                )

                // The live default disappearing between refreshes must
                // publish UNAVAILABLE, not retain connected state.
                system.currentPath = null
                monitor.triggerNetworkCheck()
                snapshots.awaitCount(6)
                assertEquals(
                    NetworkAvailability.UNAVAILABLE,
                    snapshots[5].availability,
                    "a vanished default publishes unavailable on manual refresh",
                )

                monitor.stopMonitoring()
                assertEquals(1, system.unregisterCount)
                assertEquals("No network connection", monitor.getCurrentNetworkStatus())
                assertFalse(monitor.isConnected())
            } finally {
                scope.cancel()
            }
        }


    @Test
    fun `foreground snapshot reconciles against the live default network`() =
        runBlocking {
            val system = ScriptedNetworkSystem()
            val snapshots = CopyOnWriteArrayList<NetworkSnapshot>()
            val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
            try {
                val monitor =
                    NetworkMonitor(
                        context = ContextWrapper(null),
                        scope = scope,
                        onNetworkPathChanged = { snapshots += it },
                    )
                monitor.networkSystemFactory = { system }
                monitor.startMonitoring()

                system.deliverAvailable(100)
                system.deliverCapabilities(wifiPath(100, NetworkAvailability.AVAILABLE))
                snapshots.awaitCount(2)

                // While the app was away, the default silently switched to
                // handle 300; the foreground return must publish the live
                // path, stepping through UNKNOWN, not the stale tracker state.
                system.currentPath = wifiPath(300, NetworkAvailability.AVAILABLE)
                monitor.onAppForeground()
                snapshots.awaitCount(4)
                assertEquals(
                    listOf(
                        NetworkAvailability.UNKNOWN,
                        NetworkAvailability.AVAILABLE,
                    ),
                    snapshots.subList(2, 4).map { it.availability },
                    "foreground snapshot publishes the reconciled live path",
                )
            } finally {
                scope.cancel()
            }
        }

    @Test
    fun `callbacks from a previous registration cannot touch the live tracker`() =
        runBlocking {
            val systemA = ScriptedNetworkSystem()
            val snapshots = CopyOnWriteArrayList<NetworkSnapshot>()
            val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
            try {
                val monitor =
                    NetworkMonitor(
                        context = ContextWrapper(null),
                        scope = scope,
                        onNetworkPathChanged = { snapshots += it },
                    )
                monitor.networkSystemFactory = { systemA }
                monitor.startMonitoring()
                systemA.deliverAvailable(100)
                systemA.deliverCapabilities(wifiPath(100, NetworkAvailability.AVAILABLE))
                snapshots.awaitCount(2)
                val staleEvents = requireNotNull(systemA.events)

                monitor.stopMonitoring()
                val systemB = ScriptedNetworkSystem()
                monitor.networkSystemFactory = { systemB }
                monitor.startMonitoring()

                // A late callback from the dead registration mutates only the
                // retired tracker: the live monitor publishes nothing.
                staleEvents.onLost(100)
                Thread.sleep(150)
                assertEquals(2, snapshots.size, "stale callback leaked into the live registration")

                systemB.deliverAvailable(200)
                systemB.deliverCapabilities(wifiPath(200, NetworkAvailability.AVAILABLE))
                snapshots.awaitCount(4)
                assertEquals(
                    listOf(NetworkAvailability.UNKNOWN, NetworkAvailability.AVAILABLE),
                    snapshots.subList(2, 4).map { it.availability },
                    "the fresh registration tracks independently",
                )
                assertTrue(monitor.isConnected(), "validated replacement reads as connected")
            } finally {
                scope.cancel()
            }
        }

    /** Deliveries run on the queue's IO dispatcher; wait for the expected count. */
    private fun List<NetworkSnapshot>.awaitCount(count: Int) {
        val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(10)
        while (size < count) {
            check(System.nanoTime() < deadline) {
                "expected $count snapshots, saw $size: $this"
            }
            Thread.sleep(20)
        }
    }
}
