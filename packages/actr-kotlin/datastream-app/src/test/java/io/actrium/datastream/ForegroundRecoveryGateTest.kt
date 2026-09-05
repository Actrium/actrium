package io.actrium.datastream

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ForegroundRecoveryGateTest {
    @Test
    fun foregroundSendWaitsForRuntimeLifecycleCompletion() {
        val gate = ForegroundRecoveryGate()

        gate.onBackground()
        gate.onForeground(runtimeAvailable = true)

        assertTrue(gate.isRecoveryPending)
        assertFalse(gate.canAttemptSend)
        assertTrue(gate.onRuntimeForegroundHandled())
        assertFalse(gate.isRecoveryPending)
        assertTrue(gate.canAttemptSend)
    }

    @Test
    fun foregroundWithoutRuntimeDoesNotCreateUnfulfillableBarrier() {
        val gate = ForegroundRecoveryGate()

        gate.onBackground()
        gate.onForeground(runtimeAvailable = false)

        assertFalse(gate.isRecoveryPending)
        assertTrue(gate.canAttemptSend)
    }

    @Test
    fun completionWhileBackgroundedDoesNotPermitSend() {
        val gate = ForegroundRecoveryGate()

        gate.onBackground()
        gate.onForeground(runtimeAvailable = true)
        gate.onBackground()

        assertFalse(gate.onRuntimeForegroundHandled())
        assertFalse(gate.canAttemptSend)
    }

    @Test
    fun runtimeCompletionReleasesCurrentWait() {
        val gate = ForegroundRecoveryGate()

        gate.onBackground()
        gate.onForeground(runtimeAvailable = true)

        assertTrue(gate.isRecoveryPending)
        assertTrue(gate.onRuntimeForegroundHandled())
        assertFalse(gate.isRecoveryPending)
        assertTrue(gate.canAttemptSend)
    }

    @Test
    fun foregroundWaitSelfExpiresAfterGraceWhenMarkerIsLost() {
        // Start at a non-zero time: 0 is the gate's "not waiting" sentinel.
        var nowNanos = 1_000_000_000L
        val gate = ForegroundRecoveryGate(pendingGraceMs = 2_000L, nanoTime = { nowNanos })

        gate.onBackground()
        gate.onForeground(runtimeAvailable = true)

        assertTrue(gate.isRecoveryPending)
        assertFalse(gate.canAttemptSend)

        // The reconciliation marker never arrives (lost / out of order on the
        // runtime's debounced broadcast). Advancing past the grace window must
        // release the wait rather than wedge the gate (greyed Send Echo).
        nowNanos += 2_500L * 1_000_000L

        assertFalse(gate.isRecoveryPending)
        assertTrue(gate.canAttemptSend)
    }

    @Test
    fun unsolicitedRuntimeCompletionIsIgnored() {
        val gate = ForegroundRecoveryGate()

        assertTrue(gate.onRuntimeForegroundHandled())
        assertFalse(gate.isRecoveryPending)
        assertTrue(gate.canAttemptSend)
    }
}
