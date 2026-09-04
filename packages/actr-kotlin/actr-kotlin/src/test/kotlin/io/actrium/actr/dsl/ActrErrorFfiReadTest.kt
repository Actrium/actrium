package io.actrium.actr.dsl

import io.actrium.actr.ActrException
import io.actrium.actr.FfiConverterString
import io.actrium.actr.FfiConverterTypeActrError
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertTrue

/**
 * Exercise `FfiConverterTypeActrError.read` against the UniFFI error-buffer
 * layout the Rust writer produces. The Kotlin unit suite runs JVM-only (no
 * native runtime), so the buffer here is assembled field-by-field exactly as
 * the Rust error writer serializes it: the variant discriminant as an Int,
 * then each payload field through its own converter in declaration order.
 * The Rust-writer half of this round trip is covered on the Swift side via
 * `actrErrorThrowForTest`, which throws a Rust-generated case-13 error
 * through the real FFI boundary.
 */
class ActrErrorFfiReadTest {
    @Test
    fun `case 13 buffer lifts both delivery-uncertain fields`() {
        val buf = ByteBuffer.allocate(256).order(ByteOrder.BIG_ENDIAN)
        buf.putInt(13)
        FfiConverterString.write("stream-1#4", buf)
        FfiConverterString.write("write may have entered transport", buf)
        buf.flip()

        val error = FfiConverterTypeActrError.read(buf)

        val uncertain = assertIs<ActrException.DeliveryUncertain>(error)
        assertEquals("stream-1#4", uncertain.operationId)
        assertEquals("write may have entered transport", uncertain.causeDetail)
    }

    @Test
    fun `writer and reader agree on the case 13 layout`() {
        val original =
            ActrException.DeliveryUncertain(
                operationId = "stream-9#2",
                causeDetail = "ambiguous partial write",
            )

        val size = FfiConverterTypeActrError.allocationSize(original)
        val buf = ByteBuffer.allocate(size.toInt()).order(ByteOrder.BIG_ENDIAN)
        FfiConverterTypeActrError.write(original, buf)
        buf.flip()

        val roundTrip = FfiConverterTypeActrError.read(buf)
        val uncertain = assertIs<ActrException.DeliveryUncertain>(roundTrip)
        assertEquals(original.operationId, uncertain.operationId)
        assertEquals(original.causeDetail, uncertain.causeDetail)
    }

    @Test
    fun `unknown discriminant is rejected loudly`() {
        val buf = ByteBuffer.allocate(8).order(ByteOrder.BIG_ENDIAN)
        buf.putInt(0x7fff)
        buf.flip()

        val error = runCatching { FfiConverterTypeActrError.read(buf) }.exceptionOrNull()
        assertTrue(
            error is RuntimeException,
            "reading an unknown discriminant must fail loudly, got $error",
        )
    }
}
