package io.actrium.actr.dsl

import io.actrium.actr.ActrException
import kotlinx.coroutines.runBlocking
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith

/** RFC-0507 retry-boundary regression tests. */
class RetryExtensionsTest {
    @Test
    fun `delivery uncertain is never retried by default`() = runBlocking {
        var attempts = 0
        val error =
            ActrException.DeliveryUncertain(
                operationId = "kotlin-call-1",
                causeDetail = "WebRTC generation replaced before Response",
            )

        val thrown =
            assertFailsWith<ActrException.DeliveryUncertain> {
                withRetry(maxAttempts = 5, initialDelayMs = 0) {
                    attempts += 1
                    throw error
                }
            }

        assertEquals(1, attempts)
        assertEquals("kotlin-call-1", thrown.operationId)
        assertEquals("WebRTC generation replaced before Response", thrown.causeDetail)
    }

    @Test
    fun `default predicate preserves recoverable actr exception retries`() = runBlocking {
        var attempts = 0
        val error = ActrException.Unavailable(msg = "route warming up")

        assertFailsWith<ActrException.Unavailable> {
            withRetry(maxAttempts = 3, initialDelayMs = 0) {
                attempts += 1
                throw error
            }
        }
        assertEquals(3, attempts)

        attempts = 0
        assertFailsWith<ActrException.Unavailable> {
            withRetry(RetryConfig(maxAttempts = 2, initialDelayMs = 0)) {
                attempts += 1
                throw error
            }
        }
        assertEquals(2, attempts)
    }
}
