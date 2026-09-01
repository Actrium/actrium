package io.actrium.actr.dsl

import io.actrium.actr.AppLifecycleState
import io.actrium.actr.NetworkAvailability
import io.actrium.actr.NetworkTransportFlags
import kotlin.test.Test
import kotlin.test.assertContains
import kotlin.test.assertEquals
import kotlin.test.assertSame
import kotlin.test.assertTrue

class MobileEventAdapterStateTest {
    @Test
    fun `authoritative initial phase is emitted once and cannot overwrite an observer event`() {
        val foreground = MobileEventAdapterState()
        assertEquals(
            AppLifecycleState.Foreground(0uL),
            foreground.initializePhase(isForeground = true, nowMs = 1_000),
        )
        assertEquals(null, foreground.initializePhase(isForeground = false, nowMs = 2_000))

        val background = MobileEventAdapterState()
        assertSame(
            AppLifecycleState.Background,
            background.initializePhase(isForeground = false, nowMs = 1_000),
        )
        assertEquals(
            AppLifecycleState.Foreground(5_000uL),
            background.enterForeground(6_000),
        )

        val observerWonRace = MobileEventAdapterState()
        observerWonRace.enterBackground(1_000)
        assertEquals(null, observerWonRace.initializePhase(isForeground = true, nowMs = 2_000))
    }

    @Test
    fun `duplicate background preserves the first transition timestamp`() {
        val state = MobileEventAdapterState()

        assertSame(AppLifecycleState.Background, state.enterBackground(1_000))
        assertEquals(null, state.enterBackground(56_000))
        assertEquals(
            AppLifecycleState.Foreground(65_000uL),
            state.enterForeground(66_000),
        )
    }

    @Test
    fun `lifecycle timestamps map to clamped foreground duration`() {
        val state = MobileEventAdapterState()

        assertSame(AppLifecycleState.Background, state.enterBackground(1_000))
        assertEquals(
            AppLifecycleState.Foreground(60_000uL),
            state.enterForeground(61_000),
        )
        assertEquals(
            AppLifecycleState.Foreground(0uL),
            state.enterForeground(70_000),
        )

        state.enterBackground(80_000)
        assertEquals(
            AppLifecycleState.Foreground(0uL),
            state.enterForeground(79_000),
        )
    }

    @Test
    fun `lifecycle event source reads injected monotonic clock at each call site`() {
        val readings = ArrayDeque(listOf(10_000L, 70_000L))
        val source =
            MobileLifecycleEventSource(
                clock = MonotonicClock { readings.removeFirst() },
            )

        assertSame(AppLifecycleState.Background, source.enterBackground())
        assertEquals(
            AppLifecycleState.Foreground(60_000uL),
            source.enterForeground(),
        )
        assertTrue(readings.isEmpty())
    }

    @Test
    fun `network snapshots preserve all fields and allocate monotonic sequences`() {
        val state = MobileEventAdapterState()
        val wifi =
            NetworkTransportFlags(
                wifi = true,
                cellular = false,
                ethernet = false,
                vpn = false,
                other = false,
            )
        val vpnCellular =
            NetworkTransportFlags(
                wifi = false,
                cellular = true,
                ethernet = false,
                vpn = true,
                other = false,
            )

        val first =
            state.snapshot(
                NetworkAvailability.AVAILABLE,
                wifi,
                isExpensive = false,
                isConstrained = false,
            )
        val second =
            state.snapshot(
                NetworkAvailability.UNKNOWN,
                vpnCellular,
                isExpensive = true,
                isConstrained = true,
            )

        assertEquals(1uL, first.sequence)
        assertEquals(NetworkAvailability.AVAILABLE, first.availability)
        assertTrue(first.transport.wifi)
        assertEquals(2uL, second.sequence)
        assertEquals(NetworkAvailability.UNKNOWN, second.availability)
        assertEquals(vpnCellular, second.transport)
        assertTrue(second.isExpensive)
        assertTrue(second.isConstrained)
    }

    @Test
    fun `network availability requires a validated non-suspended Internet path`() {
        assertEquals(
            NetworkAvailability.AVAILABLE,
            classifyNetworkAvailability(
                hasInternet = true,
                isValidated = true,
                isNotSuspended = true,
            ),
        )
        assertEquals(
            NetworkAvailability.UNKNOWN,
            classifyNetworkAvailability(
                hasInternet = false,
                isValidated = true,
                isNotSuspended = true,
            ),
        )
        assertEquals(
            NetworkAvailability.UNKNOWN,
            classifyNetworkAvailability(
                hasInternet = true,
                isValidated = false,
                isNotSuspended = true,
            ),
        )
        assertEquals(
            NetworkAvailability.UNKNOWN,
            classifyNetworkAvailability(
                hasInternet = true,
                isValidated = true,
                isNotSuspended = false,
            ),
        )
    }

    @Test
    fun `same transport reconnect emits unknown before available when loss is missed`() {
        val tracker = DefaultNetworkPathTracker()
        val wifi =
            NetworkTransportFlags(
                wifi = true,
                cellular = false,
                ethernet = false,
                vpn = false,
                other = false,
            )
        val first =
            DefaultNetworkPath(
                networkHandle = 100,
                availability = NetworkAvailability.AVAILABLE,
                transport = wifi,
            )
        val replacement = first.copy(networkHandle = 101)

        assertEquals(
            listOf(
                DefaultNetworkPath(
                    networkHandle = 100,
                    availability = NetworkAvailability.UNKNOWN,
                ),
            ),
            tracker.onAvailable(100),
        )
        assertEquals(listOf(first), tracker.onCapabilitiesChanged(first))
        assertEquals(
            listOf(
                DefaultNetworkPath(
                    networkHandle = 101,
                    availability = NetworkAvailability.UNKNOWN,
                ),
            ),
            tracker.onAvailable(101),
        )
        assertEquals(listOf(replacement), tracker.onCapabilitiesChanged(replacement))
    }

    @Test
    fun `validation completion is material and stale loss cannot clear replacement`() {
        val tracker = DefaultNetworkPathTracker()
        val validating =
            DefaultNetworkPath(
                networkHandle = 200,
                availability = NetworkAvailability.UNKNOWN,
                transport =
                    NetworkTransportFlags(
                        wifi = true,
                        cellular = false,
                        ethernet = false,
                        vpn = false,
                        other = false,
                    ),
            )
        val available = validating.copy(availability = NetworkAvailability.AVAILABLE)

        tracker.onAvailable(200)
        assertEquals(listOf(validating), tracker.onCapabilitiesChanged(validating))
        assertEquals(listOf(available), tracker.onCapabilitiesChanged(available))

        val replacement = available.copy(networkHandle = 201)
        tracker.onAvailable(201)
        assertTrue(tracker.onCapabilitiesChanged(available).isEmpty())
        assertEquals(listOf(replacement), tracker.onCapabilitiesChanged(replacement))
        assertTrue(tracker.onLost(200).isEmpty())
        assertEquals(replacement, tracker.current())
        assertEquals(listOf(DefaultNetworkPath.unavailable()), tracker.onLost(201))
    }

    @Test
    fun `default network callback adapter publishes ordered current-handle transitions`() {
        val tracker = DefaultNetworkPathTracker()
        val published = mutableListOf<Pair<String, List<DefaultNetworkPath>>>()
        val adapter =
            DefaultNetworkCallbackAdapter(tracker) { transitions, reason ->
                published += reason to transitions
            }
        val oldAvailable =
            DefaultNetworkPath(
                networkHandle = 400,
                availability = NetworkAvailability.AVAILABLE,
            )
        val newAvailable = oldAvailable.copy(networkHandle = 401)

        adapter.onAvailable(400)
        adapter.onAvailable(401)
        adapter.onCapabilitiesChanged(oldAvailable)
        adapter.onCapabilitiesChanged(newAvailable)
        adapter.onLost(400)
        adapter.onLost(401)

        assertEquals(
            listOf(
                "default_on_available" to
                    listOf(
                        DefaultNetworkPath(
                            networkHandle = 400,
                            availability = NetworkAvailability.UNKNOWN,
                        ),
                    ),
                "default_on_available" to
                    listOf(
                        DefaultNetworkPath(
                            networkHandle = 401,
                            availability = NetworkAvailability.UNKNOWN,
                        ),
                    ),
                "default_capabilities_changed" to listOf(newAvailable),
                "default_on_lost" to listOf(DefaultNetworkPath.unavailable()),
            ),
            published,
        )
    }

    @Test
    fun `network diagnostic line carries durable recovery fields`() {
        val path =
            DefaultNetworkPath(
                networkHandle = 301,
                availability = NetworkAvailability.AVAILABLE,
                transport =
                    NetworkTransportFlags(
                        wifi = true,
                        cellular = false,
                        ethernet = false,
                        vpn = false,
                        other = false,
                    ),
                isValidated = true,
                isSuspended = false,
            )

        val line =
            path.diagnosticLine(
                sequence = 9uL,
                reason = "default onAvailable handle=301",
            )

        assertContains(line, "networkHandle=301")
        assertContains(line, "reason=default_onAvailable_handle_301")
        assertContains(line, "sequence=9")
        assertContains(line, "availability=AVAILABLE")
        assertContains(line, "validated=true")
        assertContains(line, "suspended=false")
    }
}
