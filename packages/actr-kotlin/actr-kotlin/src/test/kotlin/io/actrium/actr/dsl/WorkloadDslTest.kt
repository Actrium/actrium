package io.actrium.actr.dsl

import io.actrium.actr.ActrId
import io.actrium.actr.ActrType
import io.actrium.actr.NoHandle
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertSame
import kotlin.test.assertTrue

class WorkloadDslTest {
    private val ctx = ActrContext(NoHandle)

    @Test
    fun `workload builder accepts string type and forwards lifecycle handlers`() =
        runTest {
            val calls = mutableListOf<String>()
            val error =
                ErrorEvent(
                    source = "handler failed",
                    category = ErrorCategory.HANDLER_ERROR,
                    context = "dispatch",
                    timestampMs = 1234L,
                )
            val workload =
                workload {
                    type = "acme/EchoClient@1.2.3"
                    onStart {
                        calls += "start"
                        assertSame(ctx, it)
                    }
                    onReady {
                        calls += "ready"
                        assertSame(ctx, it)
                    }
                    onError { bridge, event ->
                        calls += "error:${event.source}"
                        assertSame(ctx, bridge)
                        assertSame(error, event)
                    }
                    onStop {
                        calls += "stop"
                        assertSame(ctx, it)
                    }
                }

            workload.onStart(ctx)
            workload.onReady(ctx)
            workload.onError(ctx, error)
            workload.onStop(ctx)

            assertEquals(listOf("start", "ready", "error:handler failed", "stop"), calls)
        }

    @Test
    fun `workload builder accepts direct and named actor types`() {
        val direct =
            WorkloadBuilder()
                .apply {
                    type(ActrType("acme", "DirectWorker", "2.0.0"))
                }
        val named =
            WorkloadBuilder()
                .apply {
                    type("demo", "NamedWorker", "3.1.4")
                }

        assertEquals("acme/DirectWorker@2.0.0", direct.type)
        assertNotNull(direct.build())
        assertEquals("demo/NamedWorker@3.1.4", named.type)
        assertNotNull(named.build())
    }

    @Test
    fun `workload builder requires a type`() {
        val missingType =
            assertFailsWith<IllegalArgumentException> {
                workload {}
            }

        assertTrue("type" in missingType.message.orEmpty())
    }

    @Test
    fun `simple workload tracks target server and rejects unimplemented dispatch`() =
        runTest {
            val target = actorId(42uL)
            val workload = SimpleWorkload("acme/EchoClient@1.0.0")

            assertNull(workload.getTargetServerId())
            workload.setTargetServerId(target)
            assertEquals(target, workload.getTargetServerId())

            val error =
                assertFailsWith<IllegalStateException> {
                    workload.dispatch(
                        ctx,
                        RpcEnvelope("echo.Echo", byteArrayOf(1, 2, 3), "req-1", timeoutMs = 0),
                    )
                }

            assertTrue("dispatch()" in error.message.orEmpty())
        }

    @Test
    fun `routed workload tracks target server and keeps default lifecycle noops`() =
        runTest {
            val workload =
                object : RoutedWorkload(typeString = "acme/RoutedClient@1.0.0") {}
            val target = actorId(43uL)

            assertNull(workload.getTargetServerId())
            workload.setTargetServerId(target)
            assertEquals(target, workload.getTargetServerId())

            workload.onStart(ctx)
            workload.onReady(ctx)
            workload.onError(
                ctx,
                ErrorEvent(
                    source = "ignored",
                    category = ErrorCategory.HANDLER_ERROR,
                    context = "test",
                    timestampMs = 5678L,
                ),
            )
            workload.onStop(ctx)

            val error =
                assertFailsWith<IllegalStateException> {
                    workload.dispatch(
                        ctx,
                        RpcEnvelope("echo.Echo", byteArrayOf(), "req-2", timeoutMs = 0),
                    )
                }

            assertTrue("dispatch()" in error.message.orEmpty())
        }

    private fun actorId(serialNumber: ULong): ActrId = ActrId(realm = "test@example.com", serialNumber = serialNumber)
}
