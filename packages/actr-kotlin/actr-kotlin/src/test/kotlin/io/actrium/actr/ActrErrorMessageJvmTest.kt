package io.actrium.actr

import java.io.PrintWriter
import java.io.StringWriter
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue

class ActrErrorMessageJvmTest {
    private fun cases(): List<Pair<ActrException, String>> {
        val text = "\u4e2d\u6587\n\"quoted\"\\path"
        return listOf(
            ActrException.Unavailable(text) to "unavailable: $text",
            ActrException.ConnectionNotReady(ConnectionNotReadyInfo(42uL)) to
                "connection not ready: retry after 42ms",
            ActrException.ConnectionNotReady(ConnectionNotReadyInfo(null)) to
                "connection not ready: no retry hint",
            ActrException.ConnectionNotReady(ConnectionNotReadyInfo(0uL)) to
                "connection not ready: retry after 0ms",
            ActrException.ConnectionNotReady(ConnectionNotReadyInfo(ULong.MAX_VALUE)) to
                "connection not ready: retry after 18446744073709551615ms",
            ActrException.TimedOut() to "timed out",
            ActrException.NotFound(text) to "not found: $text",
            ActrException.PermissionDenied(text) to "permission denied: $text",
            ActrException.InvalidArgument(text) to "invalid argument: $text",
            ActrException.UnknownRoute(text) to "unknown route: $text",
            ActrException.DependencyNotFound("echo", text) to "dependency 'echo' not found: $text",
            ActrException.DecodeFailure(text) to "decode failure: $text",
            ActrException.NotImplemented(text) to "not implemented: $text",
            ActrException.Internal(text) to "internal error: $text",
            ActrException.Config(text) to "config: $text",
            ActrException.DeliveryUncertain("op-1", text) to "delivery uncertain for operation op-1: $text",
            ActrException.Internal("") to "internal error: ",
        )
    }

    @Test
    fun messageAndLoggingRemainSafeWithoutNativeLibrary() {
        val key = "uniffi.component.actr.libraryOverride"
        val previous = System.getProperty(key)
        // Force a missing path even if the machine has libactr installed.
        System.setProperty(key, "/__actr_message_test_missing__/libactr.so")
        try {
            // Repeated access also exercises the JVM's failed-initializer state.
            repeat(2) {
                for ((error, expected) in cases()) {
                    assertEquals(expected, error.message)
                    assertEquals(expected, error.localizedMessage)
                    val rendered = "${error.javaClass.name}: $expected"
                    assertEquals(rendered, error.toString())
                    assertEquals("caught: $rendered", "caught: $error")
                    val output = StringWriter()
                    error.printStackTrace(PrintWriter(output))
                    assertTrue(output.toString().startsWith(rendered))
                }
            }
            val error = ActrException.ConnectionNotReady(ConnectionNotReadyInfo(42uL))
            assertEquals("connection not ready: retry after 42ms", error.message)
            assertEquals(42uL, error.info.retryAfterMs)
        } finally {
            if (previous == null) System.clearProperty(key) else System.setProperty(key, previous)
        }
    }

    @Test
    fun linkageAndInitializationFailuresUseVariantFields() {
        val failures = listOf(
            LinkageError("missing binding"),
            UnsatisfiedLinkError("missing libactr"),
            ExceptionInInitializerError(IllegalStateException("native initialization failed")),
            NoClassDefFoundError("native binding initializer previously failed"),
        )
        for (failure in failures) {
            for ((error, expected) in cases()) {
                assertEquals(expected, error.displayMessage { throw failure })
            }
        }
    }

    @Test
    fun successfulNativeMessagesArePreservedAndOtherFailuresPropagate() {
        val error = ActrException.Internal("local detail")
        assertEquals("native display", error.displayMessage { "native display" })
        assertFailsWith<IllegalStateException> {
            error.displayMessage { throw IllegalStateException("unexpected failure") }
        }
    }
}
