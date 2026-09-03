package io.actrium.actr.dsl

import io.actrium.actr.ActrException
import io.actrium.actr.ErrorKind
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/** Protect the Kotlin-facing terminal delivery-uncertainty contract. */
class DeliveryUncertainExtensionsTest {
    @Test
    fun `delivery uncertain is terminal and retains caller diagnostics`() {
        val error =
            ActrException.DeliveryUncertain(
                operationId = "kotlin-call-1",
                causeDetail = "WebRTC generation replaced before Response",
            )

        assertFalse(error.isRecoverable)
        assertEquals(ErrorKind.INTERNAL, error.kind)
        assertTrue(error.userMessage.contains("kotlin-call-1"))
        assertTrue(error.userMessage.contains("Do not retry automatically"))
    }
}
