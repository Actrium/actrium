package io.actrium.actr

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith

/** Requires the matching libactr.so; exercises the real Rust error writer. */
@RunWith(AndroidJUnit4::class)
class ActrErrorMessageTest {
    @Test
    fun rustThrownErrorsRetainDisplayTextAndClassification() {
        val text = "\u4e2d\u6587\n\"quoted\"\\path"
        val cases = listOf(
            ActrException.Unavailable(text) to "unavailable: $text",
            ActrException.ConnectionNotReady(ConnectionNotReadyInfo(42uL)) to
                "connection not ready: retry_after_ms=Some(42)",
            ActrException.ConnectionNotReady(ConnectionNotReadyInfo(null)) to
                "connection not ready: retry_after_ms=None",
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
        for ((original, expected) in cases) {
            assertEquals(expected, original.message)
            assertEquals(expected, actrErrorMessage(original))
            try {
                actrErrorThrowForTest(original)
                fail("expected Rust to throw")
            } catch (error: ActrException) {
                assertEquals(original.javaClass, error.javaClass)
                assertEquals(expected, error.message)
                assertEquals(actrErrorKind(original), actrErrorKind(error))
                assertEquals(actrErrorIsRetryable(original), actrErrorIsRetryable(error))
            }
        }
    }
}
