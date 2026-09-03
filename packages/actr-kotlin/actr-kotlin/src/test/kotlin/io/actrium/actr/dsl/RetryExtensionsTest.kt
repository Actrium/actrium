package io.actrium.actr.dsl

import io.actrium.actr.ActrException
import kotlinx.coroutines.runBlocking
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith

/**
 * RFC-0507 SDK retry boundary tests.
 *
 * Framework-owned Call/Tell/DataStream operations must not be replayed by the
 * generic Kotlin helper. In particular, DeliveryUncertain means the payload
 * may already have entered the transport and is never retried by default.
 */
class RetryExtensionsTest {
    @Test
    fun `withRetry does not replay delivery uncertain`() = runBlocking {
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
    fun `default predicate excludes recoverable actr exceptions until opted in`() = runBlocking {
        var attempts = 0
        val error = ActrException.Unavailable(msg = "route warming up")

        // Default: even a transient, recoverable ActrException is never
        // replayed — the framework owns readiness/transport retry, and a
        // generic replay could duplicate a delivery.
        val notRetried =
            assertFailsWith<ActrException.Unavailable> {
                withRetry(maxAttempts = 5, initialDelayMs = 0) {
                    attempts += 1
                    throw error
                }
            }
        assertEquals(1, attempts)
        assertEquals("route warming up", notRetried.msg)

        // Opt-in: an operation that knows itself to be idempotent may replay
        // ActrException failures explicitly.
        attempts = 0
        assertFailsWith<ActrException.Unavailable> {
            withRetry(maxAttempts = 3, initialDelayMs = 0, shouldRetry = { true }) {
                attempts += 1
                throw error
            }
        }
        assertEquals(3, attempts)
    }
}
