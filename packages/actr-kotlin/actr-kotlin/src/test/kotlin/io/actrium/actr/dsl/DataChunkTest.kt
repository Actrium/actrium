package io.actrium.actr.dsl

import io.actrium.actr.ActrException
import io.actrium.actr.DataChunkEventBridge
import io.actrium.actr.DataChunkStreamBridge
import io.actrium.actr.NoHandle
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.yield
import kotlin.test.Test
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class DataChunkTest {
    @Test
    fun `a second collector cannot close the active subscription`() =
        runTest {
            val reading = CompletableDeferred<Unit>()
            var closed = false
            val bridge =
                object : DataChunkStreamBridge(NoHandle) {
                    override suspend fun next(): DataChunkEventBridge? {
                        if (reading.isCompleted) {
                            throw ActrException.InvalidArgument("concurrent next()")
                        }
                        reading.complete(Unit)
                        awaitCancellation()
                    }

                    override suspend fun closeAsync() {
                        closed = true
                    }
                }
            val flow = bridge.asFlow()
            val collector = launch { flow.collect() }
            reading.await()
            assertFailsWith<ActrException.InvalidArgument> { flow.collect() }
            assertFailsWith<ActrException.InvalidArgument> { bridge.asFlow().collect() }
            assertFalse(closed)
            collector.cancelAndJoin()
            assertTrue(closed)
        }

    @Test
    fun `cancelling collection completes suspending stream cleanup`() =
        runTest {
            val reading = CompletableDeferred<Unit>()
            var closed = false
            val bridge =
                object : DataChunkStreamBridge(NoHandle) {
                    override suspend fun next(): DataChunkEventBridge? {
                        reading.complete(Unit)
                        awaitCancellation()
                    }

                    override suspend fun closeAsync() {
                        currentCoroutineContext().ensureActive()
                        yield()
                        closed = true
                    }
                }
            val collector = launch { bridge.asFlow().collect() }
            reading.await()
            collector.cancelAndJoin()
            assertTrue(closed)
        }
}
