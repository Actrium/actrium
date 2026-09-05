package io.actrium.datastream

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ForegroundEchoTriggerTest {
    @Test
    fun initialResumeDoesNotRequestEcho() {
        val trigger = ForegroundEchoTrigger()

        assertFalse(trigger.onResume())
        assertFalse(trigger.onWindowFocusChanged(hasFocus = true, isResumed = true))
    }

    @Test
    fun activityPauseResumeRequestsExactlyOneEcho() {
        val trigger = ForegroundEchoTrigger()

        assertFalse(trigger.onWindowFocusChanged(hasFocus = false, isResumed = true))
        trigger.onPause()
        assertTrue(trigger.onResume())
        assertFalse(trigger.onWindowFocusChanged(hasFocus = true, isResumed = true))
    }

    @Test
    fun quickSettingsFocusRoundTripRequestsEchoWhileActivityStaysResumed() {
        val trigger = ForegroundEchoTrigger()

        assertFalse(trigger.onWindowFocusChanged(hasFocus = false, isResumed = true))
        assertTrue(trigger.onWindowFocusChanged(hasFocus = true, isResumed = true))
        assertFalse(trigger.onWindowFocusChanged(hasFocus = true, isResumed = true))
    }

    @Test
    fun focusChangesOutsideResumedStateDoNotRequestEcho() {
        val trigger = ForegroundEchoTrigger()

        assertFalse(trigger.onWindowFocusChanged(hasFocus = false, isResumed = false))
        assertFalse(trigger.onWindowFocusChanged(hasFocus = true, isResumed = false))
        assertFalse(trigger.onResume())
    }

    @Test
    fun focusReturningBeforeResumeStillProducesOnlyLifecycleEcho() {
        val trigger = ForegroundEchoTrigger()

        trigger.onPause()
        assertFalse(trigger.onWindowFocusChanged(hasFocus = false, isResumed = false))
        assertFalse(trigger.onWindowFocusChanged(hasFocus = true, isResumed = false))
        assertTrue(trigger.onResume())
        assertFalse(trigger.onWindowFocusChanged(hasFocus = true, isResumed = true))
    }
}
