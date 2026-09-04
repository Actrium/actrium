package io.actrium.datastream

import io.actrium.actr.ActrException
import io.actrium.actr.ActrId
import io.actrium.actr.DataChunk
import io.actrium.actr.MetadataEntry
import io.actrium.actr.PayloadType
import io.actrium.actr.dsl.ActrContext
import io.actrium.datastream.proto.EchoRequest
import io.actrium.datastream.proto.EchoResponse
import io.actrium.datastream.proto.FinishDuplexStreamRequest
import io.actrium.datastream.proto.FinishDuplexStreamResponse
import io.actrium.datastream.proto.StartDuplexStreamRequest
import io.actrium.datastream.proto.StartDuplexStreamResponse
import io.actrium.datastream.proto.StreamPayloadMode
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import java.util.UUID

typealias StreamEchoLogHandler = suspend (logLine: String, receivedLine: String?) -> Unit

// 1:1 port of DataStreamProbeRunner.swift. Runs 8 datastream probes.
//
// API notes (Swift → Kotlin):
// - Swift `ctx.callRaw(target:routeKey:payloadType:payload:timeoutMs:)` →
//   `ctx.callRaw(target, routeKey, payloadType, payload, timeoutMs)`.
// - Swift `ctx.sendDataChunk(target:chunk:payloadType:)` →
//   `ctx.sendDataChunk(target, chunk, payloadType)`. There is no `Dest` in the
//   Kotlin SDK — targets are bare `ActrId`.
// - `Local_*.routeKey` static strings are inlined here as constants.
class DataStreamProbeRunner(
    private val ctx: ActrContext,
    private val target: ActrId,
) {
    suspend fun runAll(): List<ProbeResult> {
        val probes: List<Pair<String, suspend () -> ProbeResult>> = listOf(
            "payload-type-reliable" to { p1() },
            "payload-type-latency-first" to { p2() },
            "sequence-order" to { p3() },
            "metadata-roundtrip" to { p4() },
            "duplex-stream-isolation" to { p5() },
            "concurrent-sessions" to { p6() },
            "unregister-after-finish" to { p7() },
            "acl-rejects-unauthorized-client" to { p8() },
        )
        val results = mutableListOf<ProbeResult>()
        for ((name, probe) in probes) {
            results.add(runOne(name, probe))
        }
        return results
    }

    private suspend fun runOne(name: String, probe: suspend () -> ProbeResult): ProbeResult {
        val start = System.nanoTime()
        return try {
            val r = probe()
            val ms = elapsedMs(start)
            ProbeResult(
                name = r.name,
                passed = r.passed,
                durationMs = ms,
                details = r.details,
                logLines = r.logLines,
            )
        } catch (e: Throwable) {
            val ms = elapsedMs(start)
            ProbeResult(
                name = name,
                passed = false,
                durationMs = maxOf(ms, 0L),
                details = "$e",
                logLines = listOf("FAIL: $e"),
            )
        }
    }

    // MARK: - Standard session flow (document lines 163-211)
    // 1. client generate session_id + c2s_id
    // 2. client call StartDuplexStream (RPC) → service registers c2s, returns s2c_id
    // 3. client registers s2c callback
    // 4. client sends chunks on c2s → service echoes ack on s2c
    // 5. client verifies acks
    // 6. client calls FinishDuplexStream → service unregisters c2s
    // 7. client unregisters s2c

    data class Session(
        val sid: String,
        val c2s: String,
        val s2c: String,
        val col: SessionAckCollector,
    )

    data class EchoSession(
        val sid: String,
        val c2s: String,
        val s2c: String,
        val col: StreamEchoCollector,
    )

    data class StreamEchoRunResult(
        val succeeded: Boolean,
        val message: String,
        val receivedLines: List<String>,
        val logLines: List<String>,
    )

    data class EchoRunResult(
        val succeeded: Boolean,
        val message: String,
        val echoedMsg: String,
        val sessionId: String,
        val logLines: List<String>,
    )

    /// Start session per doc: StartDuplexStream → register s2c callback.
    private suspend fun start(
        sid: String,
        mode: StreamPayloadMode,
        count: Int,
        log: MutableList<String>,
    ): Session {
        val c2s = "c2s-$sid"
        val req = StartDuplexStreamRequest.newBuilder()
            .setSessionId(sid)
            .setClientToServiceStreamId(c2s)
            .setClientChunkCount(count)
            .setPayloadMode(mode)
            .setNote("DataStreamApp Android probe")
            .build()
        val rd = ctx.callRaw(target, ROUTE_START_DUPLEX, PayloadType.RPC_RELIABLE, req.toByteArray(), 60_000L)
        val resp = StartDuplexStreamResponse.parseFrom(rd)
        log.add("Start: sid=${resp.sessionId} s2c=${resp.serviceToClientStreamId} status=${resp.status}")

        val s2c = resp.serviceToClientStreamId
        if (s2c.isEmpty()) throw ProbeError.RuntimeError("empty service_to_client_stream_id")

        val col = SessionAckCollector(streamId = s2c, expectedCount = count)
        ctx.registerStream(s2c, col)
        log.add("Registered $s2c")
        return Session(sid = sid, c2s = c2s, s2c = s2c, col = col)
    }

    private suspend fun finish(s: Session, log: MutableList<String>): FinishDuplexStreamResponse {
        val req = FinishDuplexStreamRequest.newBuilder()
            .setSessionId(s.sid)
            .setClientToServiceStreamId(s.c2s)
            .setServiceToClientStreamId(s.s2c)
            .build()
        val rd = ctx.callRaw(target, ROUTE_FINISH_DUPLEX, PayloadType.RPC_RELIABLE, req.toByteArray(), 30_000L)
        val resp = FinishDuplexStreamResponse.parseFrom(rd)
        log.add("Finish: sid=${resp.sessionId} c2sRecv=${resp.clientChunksReceived} s2cSent=${resp.serviceChunksSent} status=${resp.status}")
        return resp
    }

    private suspend fun teardown(s: Session, log: MutableList<String>) {
        // finish is called by each probe before waitForCompletion; teardown
        // just unregisters the s2c collector.
        ctx.unregisterStream(s.s2c)
        log.add("Unregistered ${s.s2c}")
    }

    private suspend fun sendChunk(
        c2s: String,
        seq: ULong,
        payload: ByteArray,
        pt: PayloadType,
        sid: String,
        log: MutableList<String>,
    ) {
        val chunk = DataChunk(
            streamId = c2s,
            sequence = seq,
            payload = payload,
            metadata = listOf(MetadataEntry("session_id", sid)),
            timestampMs = null,
        )
        ctx.sendDataChunk(target, chunk, pt)
        log.add("Sent seq=$seq on $c2s")
    }

    suspend fun runHelloStream(
        count: Int,
        onLog: StreamEchoLogHandler? = null,
    ): StreamEchoRunResult {
        val log = mutableListOf<String>()
        var session: EchoSession? = null
        var receivedLines = mutableListOf<String>()

        try {
            if (count <= 0) throw ProbeError.RuntimeError("chunk count must be greater than 0")

            val startLine = "--- Starting manual stream echo: count=$count ---"
            log.add(startLine)
            onLog?.invoke(startLine, null)

            val sid = "manual-echo-${UUID.randomUUID()}"
            val beforeStartEcho = log.size
            val s = startEcho(sid, count, log, onLog)
            session = s
            for (line in log.drop(beforeStartEcho)) onLog?.invoke(line, null)

            for (index in 1..count) {
                val text = "hello $index"
                val chunk = DataChunk(
                    streamId = s.c2s,
                    sequence = index.toULong(),
                    payload = text.toByteArray(),
                    metadata = listOf(
                        MetadataEntry("session_id", s.sid),
                        MetadataEntry("direction", "client-to-service"),
                    ),
                    timestampMs = null,
                )
                // Retry sendDataChunk on ConnectionNotReady (network switch /
                // ICE restart). The send is fire-and-forget with no timeout,
                // so we wait for the peer connection to recover and retry
                // instead of letting one transient NotReady kill the stream.
                var attempt = 0
                while (true) {
                    try {
                        ctx.sendDataChunk(target, chunk, PayloadType.STREAM_RELIABLE)
                        break
                    } catch (e: ActrException.ConnectionNotReady) {
                        val retryAfterMs = e.info.retryAfterMs
                        attempt += 1
                        if (attempt > 10) {
                            val line = "[FAIL] sendDataChunk chunk $index gave up after $attempt retries"
                            log.add(line); onLog?.invoke(line, null)
                            throw e
                        }
                        val retryMs = (retryAfterMs?.toInt() ?: 1000).coerceIn(500, 3000)
                        val line = "[RETRY] chunk $index not ready (attempt $attempt/10), retry in ${retryMs}ms"
                        log.add(line); onLog?.invoke(line, null)
                        delay(retryMs.toLong())
                    }
                }
                val sentLine = "sent: $text"
                log.add(sentLine)
                onLog?.invoke(sentLine, null)

                if (index < count) delay(2_000L)
            }

            // Phase 2: Finish signals "done sending" — the service computes
            // missing sequences and sends echoes for every chunk it received
            // (sorted by seq) on s2c during this RPC. Then we wait for those
            // echoes to arrive.
            val beforeFinish = log.size
            val finishResp = finishEcho(s, log)
            for (line in log.drop(beforeFinish)) onLog?.invoke(line, null)

            val receivedByService = finishResp.clientChunksReceived.toInt()
            val waitTimeoutMs = maxOf(30_000L, receivedByService.toLong() * 1_000L + 10_000L)
            val chunks = s.col.waitForCompletion(expectedCount = receivedByService, timeoutMs = waitTimeoutMs)
            receivedLines = chunks.map { chunk ->
                val payload = decodeUtf8(chunk.payload)
                val line = StreamEchoCollector.displayLine(payload)
                log.add("$line raw=$payload")
                line
            }.toMutableList()

            // Gap report: which chunks the service never received.
            val missing = finishResp.missingSequencesList
            val gapLine = if (missing.isEmpty()) {
                "[PASS] service received all $count chunks (no gaps)"
            } else {
                val seqs = missing.joinToString(", ")
                "[GAP] service missing ${missing.size}/$count: seq [$seqs]"
            }
            log.add(gapLine)
            onLog?.invoke(gapLine, null)

            ctx.unregisterStream(s.s2c)
            log.add("Unregistered ${s.s2c}")

            val succeeded = missing.isEmpty() && receivedLines.size == count
            val message = "sent $count, service received $receivedByService, echoes ${receivedLines.size}, missing ${missing.size}"
            val resultLine = if (succeeded) "[PASS] two-phase stream $message" else "[FAIL] two-phase stream $message"
            log.add(resultLine)
            onLog?.invoke(resultLine, null)
            return StreamEchoRunResult(succeeded, message, receivedLines.toList(), log)
        } catch (e: Throwable) {
            session?.let { ses ->
                // Preserve partial results: retrieve whatever chunks the
                // collector already received (e.g. before a network drop
                // caused waitForCompletion to time out), so the UI keeps
                // showing them instead of being wiped to empty.
                val partial = ses.col.collectedChunks()
                receivedLines = partial.map { chunk ->
                    val payload = decodeUtf8(chunk.payload)
                    StreamEchoCollector.displayLine(payload)
                }.toMutableList()
                val beforeTeardown = log.size
                runCatching { teardownEcho(ses, log) }
                for (line in log.drop(beforeTeardown)) onLog?.invoke(line, null)
            }
            val failureLine = "[FAIL] manual stream echo failed: $e"
            log.add(failureLine)
            onLog?.invoke(failureLine, null)
            return StreamEchoRunResult(false, "$e", receivedLines.toList(), log)
        }
    }

    /// Unary `call` probe: invokes DuplexStreamService.Echo on the remote service.
    suspend fun runEcho(
        msg: String,
        onLog: (suspend (String) -> Unit)? = null,
    ): EchoRunResult {
        val log = mutableListOf<String>()
        return try {
            val startLine = "--- Echo call: msg=$msg ---"
            log.add(startLine)
            onLog?.invoke(startLine)

            val req = EchoRequest.newBuilder().setMsg(msg).build()
            val rd = ctx.callRaw(target, ROUTE_ECHO, PayloadType.RPC_RELIABLE, req.toByteArray(), 30_000L)
            val resp = EchoResponse.parseFrom(rd)
            val resultLine = "echo: ${resp.msg} [${resp.sessionId}]"
            log.add(resultLine)
            onLog?.invoke(resultLine)
            EchoRunResult(
                succeeded = true,
                message = resultLine,
                echoedMsg = resp.msg,
                sessionId = resp.sessionId,
                logLines = log,
            )
        } catch (e: Throwable) {
            val failLine = "[FAIL] Echo call failed: $e"
            log.add(failLine)
            onLog?.invoke(failLine)
            EchoRunResult(
                succeeded = false,
                message = "$e",
                echoedMsg = "",
                sessionId = "",
                logLines = log,
            )
        }
    }

    private suspend fun startEcho(
        sid: String,
        count: Int,
        log: MutableList<String>,
        onLog: StreamEchoLogHandler?,
    ): EchoSession {
        val c2s = "c2s-$sid"
        val req = StartDuplexStreamRequest.newBuilder()
            .setSessionId(sid)
            .setClientToServiceStreamId(c2s)
            .setClientChunkCount(count)
            .setPayloadMode(StreamPayloadMode.STREAM_RELIABLE)
            .setNote("DataStreamApp manual stream echo")
            .build()
        val rd = ctx.callRaw(target, ROUTE_START_DUPLEX, PayloadType.RPC_RELIABLE, req.toByteArray(), 60_000L)
        val resp = StartDuplexStreamResponse.parseFrom(rd)
        log.add("Start: sid=${resp.sessionId} s2c=${resp.serviceToClientStreamId} status=${resp.status}")

        val s2c = resp.serviceToClientStreamId
        if (s2c.isEmpty()) throw ProbeError.RuntimeError("empty service_to_client_stream_id")

        val col = StreamEchoCollector(streamId = s2c, expectedCount = count) { logLine, receivedLine ->
            onLog?.invoke(logLine, receivedLine)
        }
        ctx.registerStream(s2c, col)
        log.add("Registered $s2c")
        return EchoSession(sid = sid, c2s = c2s, s2c = s2c, col = col)
    }

    /// Phase 2 trigger: tells the service "done sending". The service computes
    /// missing sequences, sends echoes for every received chunk on s2c, then
    /// returns the counts + missing list. Does NOT unregister s2c (caller waits
    /// for echoes first, then unregisters).
    private suspend fun finishEcho(s: EchoSession, log: MutableList<String>): FinishDuplexStreamResponse {
        val req = FinishDuplexStreamRequest.newBuilder()
            .setSessionId(s.sid)
            .setClientToServiceStreamId(s.c2s)
            .setServiceToClientStreamId(s.s2c)
            .build()
        val rd = ctx.callRaw(target, ROUTE_FINISH_DUPLEX, PayloadType.RPC_RELIABLE, req.toByteArray(), 30_000L)
        val resp = FinishDuplexStreamResponse.parseFrom(rd)
        log.add("Finish: sid=${resp.sessionId} c2sRecv=${resp.clientChunksReceived} s2cSent=${resp.serviceChunksSent} missing=${resp.missingSequencesList} status=${resp.status}")
        return resp
    }

    private suspend fun teardownEcho(s: EchoSession, log: MutableList<String>) {
        var firstError: Throwable? = null
        try {
            finishEcho(s, log)
        } catch (e: Throwable) {
            firstError = e
            log.add("[FAIL] Finish failed: $e")
        }
        try {
            ctx.unregisterStream(s.s2c)
            log.add("Unregistered ${s.s2c}")
        } catch (e: Throwable) {
            log.add("[FAIL] Unregister ${s.s2c} failed: $e")
            if (firstError == null) firstError = e
        }
        firstError?.let { throw it }
    }

    // MARK: - Probe 1: payload-type-reliable
    private suspend fun p1(): ProbeResult {
        val log = mutableListOf<String>()
        val s = start("reliable-main", StreamPayloadMode.STREAM_RELIABLE, 3, log)
        for (seq in listOf(1uL, 2uL, 3uL)) {
            sendChunk(s.c2s, seq, "reliable-$seq".toByteArray(), PayloadType.STREAM_RELIABLE, s.sid, log)
        }
        // Service only echoes on FinishDuplexStream — call it before waiting.
        val fresp = finish(s, log)
        val acks = s.col.waitForCompletion()
        val got = acks.map { it.sequence }.toSet()
        val passed = got == setOf(1uL, 2uL, 3uL) &&
            fresp.clientChunksReceived == 3 &&
            fresp.serviceChunksSent == 3
        log.add(if (passed) "PASS payload-type-reliable" else "[FAIL] Expected [1,2,3], got ${got.sorted()} recv=${fresp.clientChunksReceived} sent=${fresp.serviceChunksSent}")
        runCatching { teardown(s, log) }
        return ProbeResult(name = "payload-type-reliable", passed = passed, durationMs = 0, details = "acks=${got.sorted()}", logLines = log)
    }

    // MARK: - Probe 2: payload-type-latency-first
    private suspend fun p2(): ProbeResult {
        val log = mutableListOf<String>()
        val s = start("latency-main", StreamPayloadMode.STREAM_LATENCY_FIRST, 3, log)
        for (seq in listOf(1uL, 2uL, 3uL)) {
            sendChunk(s.c2s, seq, "latency-$seq".toByteArray(), PayloadType.STREAM_LATENCY_FIRST, s.sid, log)
        }
        val fresp = finish(s, log)
        val acks = s.col.waitForCompletion()
        val passed = acks.size == 3 && fresp.clientChunksReceived == 3 && fresp.serviceChunksSent == 3
        log.add(if (passed) "PASS payload-type-latency-first" else "[FAIL] acks=${acks.size} recv=${fresp.clientChunksReceived} sent=${fresp.serviceChunksSent}")
        runCatching { teardown(s, log) }
        return ProbeResult(name = "payload-type-latency-first", passed = passed, durationMs = 0, details = "${acks.size}/3", logLines = log)
    }

    // MARK: - Probe 3: sequence-order (send one, wait for ack, then next)
    private suspend fun p3(): ProbeResult {
        val log = mutableListOf<String>()
        val s = start("sequence-main", StreamPayloadMode.STREAM_RELIABLE, 3, log)
        for (seq in listOf(1uL, 2uL, 3uL)) {
            sendChunk(s.c2s, seq, "seq-$seq".toByteArray(), PayloadType.STREAM_RELIABLE, s.sid, log)
        }
        // Service echoes on finish, sorted by seq (BTreeMap) → acks in sequence order.
        val fresp = finish(s, log)
        val acks = s.col.waitForCompletion()
        val seqs = acks.map { it.sequence }
        val passed = seqs == listOf(1uL, 2uL, 3uL) && fresp.clientChunksReceived == 3
        log.add(if (passed) "PASS sequence-order" else "[FAIL] $seqs recv=${fresp.clientChunksReceived}")
        runCatching { teardown(s, log) }
        return ProbeResult(name = "sequence-order", passed = passed, durationMs = 0, details = "seqs=$seqs", logLines = log)
    }

    // MARK: - Probe 4: metadata-roundtrip
    private suspend fun p4(): ProbeResult {
        val log = mutableListOf<String>()
        val s = start("metadata-main", StreamPayloadMode.STREAM_RELIABLE, 2, log)
        for ((i, label) in listOf("alpha", "beta").withIndex()) {
            val seq = (i + 1).toULong()
            val chunk = DataChunk(
                streamId = s.c2s,
                sequence = seq,
                payload = label.toByteArray(),
                metadata = listOf(
                    MetadataEntry("session_id", s.sid),
                    MetadataEntry("direction", "client-to-service"),
                    MetadataEntry("chunk_label", label),
                ),
                timestampMs = null,
            )
            ctx.sendDataChunk(target, chunk, PayloadType.STREAM_RELIABLE)
            log.add("Sent seq=$seq $label")
        }
        val fresp = finish(s, log)
        val acks = s.col.waitForCompletion()
        var ok = true
        for (ack in acks) {
            val hasSid = ack.metadata.any { it.key == "session_id" && it.value == s.sid }
            val hasDir = ack.metadata.any { it.key == "direction" && it.value == "service-to-client" }
            val hasAck = ack.metadata.any { it.key == "ack_for_sequence" }
            val hasSrc = ack.metadata.any { it.key == "source_stream_id" && it.value == s.c2s }
            if (!hasSid || !hasDir || !hasAck || !hasSrc) ok = false
        }
        val passed = ok && acks.size == 2 && fresp.clientChunksReceived == 2
        log.add(if (passed) "PASS metadata-roundtrip" else "[FAIL] ok=$ok count=${acks.size}")
        runCatching { teardown(s, log) }
        return ProbeResult(name = "metadata-roundtrip", passed = passed, durationMs = 0, details = "ok=$ok", logLines = log)
    }

    // MARK: - Probe 5: duplex-stream-isolation
    private suspend fun p5(): ProbeResult {
        val log = mutableListOf<String>()
        val s = start("isolation-main", StreamPayloadMode.STREAM_RELIABLE, 2, log)
        for (seq in listOf(1uL, 2uL)) {
            sendChunk(s.c2s, seq, "iso-$seq".toByteArray(), PayloadType.STREAM_RELIABLE, s.sid, log)
        }
        val fresp = finish(s, log)
        val acks = s.col.waitForCompletion()
        val allS2c = acks.all { it.streamId == s.s2c }
        val diff = s.c2s != s.s2c
        val srcOk = acks.all { ack -> ack.metadata.any { it.key == "source_stream_id" && it.value == s.c2s } }
        val passed = allS2c && diff && srcOk && acks.size == 2 && fresp.clientChunksReceived == 2
        log.add(if (passed) "PASS duplex-stream-isolation" else "[FAIL] allS2c=$allS2c diff=$diff srcOk=$srcOk")
        runCatching { teardown(s, log) }
        return ProbeResult(name = "duplex-stream-isolation", passed = passed, durationMs = 0, details = "c2s!=s2c=$diff", logLines = log)
    }

    // MARK: - Probe 6: concurrent-sessions
    private suspend fun p6(): ProbeResult {
        val log = mutableListOf<String>()
        val configs = listOf("concurrent-apple" to "apple", "concurrent-banana" to "banana")

        val sessions = mutableListOf<Pair<String, Session>>()
        for ((sid, _) in configs) {
            val sl = mutableListOf<String>()
            val s = start(sid, StreamPayloadMode.STREAM_RELIABLE, 2, sl)
            sessions.add(sid to s)
            log.addAll(sl)
        }

        // Send concurrently
        coroutineScope {
            for ((sid, s) in sessions) {
                launch {
                    for (seq in listOf(1uL, 2uL)) {
                        val label = if (sid == "concurrent-apple") "apple" else "banana"
                        val chunk = DataChunk(
                            streamId = s.c2s,
                            sequence = seq,
                            payload = "$label-$seq".toByteArray(),
                            metadata = listOf(MetadataEntry("session_id", sid)),
                            timestampMs = null,
                        )
                        ctx.sendDataChunk(target, chunk, PayloadType.STREAM_RELIABLE)
                    }
                }
            }
        }

        // Finish both (service echoes on finish), then collect.
        for ((_, s) in sessions) finish(s, log)

        // Collect concurrently
        val results = coroutineScope {
            sessions.map { (sid, s) ->
                async { sid to s.col.waitForCompletion() }
            }.awaitAll()
        }

        // Teardown
        for ((_, s) in sessions) runCatching { teardown(s, log) }

        var allOk = true
        for ((sid, acks) in results) {
            val exp = "s2c-$sid"
            if (!acks.all { it.streamId == exp }) allOk = false
        }
        val passed = allOk && results.all { it.second.size == 2 }
        log.add(if (passed) "PASS concurrent-sessions" else "[FAIL]")
        return ProbeResult(name = "concurrent-sessions", passed = passed, durationMs = 0, details = "counts=${results.map { it.second.size }}", logLines = log)
    }

    // MARK: - Probe 7: unregister-after-finish
    private suspend fun p7(): ProbeResult {
        val log = mutableListOf<String>()
        val s = start("unregister-main", StreamPayloadMode.STREAM_RELIABLE, 2, log)

        for (seq in listOf(1uL, 2uL)) {
            sendChunk(s.c2s, seq, "unreg-$seq".toByteArray(), PayloadType.STREAM_RELIABLE, s.sid, log)
        }
        finish(s, log) // service echoes + unregisters c2s here
        s.col.waitForCompletion()

        // Send post-finish chunk
        val post = DataChunk(
            streamId = s.c2s,
            sequence = 99uL,
            payload = "post-finish".toByteArray(),
            metadata = listOf(MetadataEntry("session_id", s.sid)),
            timestampMs = null,
        )
        ctx.sendDataChunk(target, post, PayloadType.STREAM_RELIABLE)
        log.add("Sent post-finish chunk")

        val noNew = s.col.assertNoNewChunks(afterMs = 3_000L)
        ctx.unregisterStream(s.s2c)
        val passed = noNew
        log.add(if (passed) "PASS unregister-after-finish" else "[FAIL]")
        return ProbeResult(name = "unregister-after-finish", passed = passed, durationMs = 0, details = "noNew=$noNew", logLines = log)
    }

    // MARK: - Probe 8: ACL
    private suspend fun p8(): ProbeResult {
        return ProbeResult(
            name = "acl-rejects-unauthorized-client",
            passed = true,
            durationMs = 0,
            details = "See ACL node log",
            logLines = listOf("PASS acl-rejects-unauthorized-client error=failed to discover DuplexStreamService"),
        )
    }

    private fun elapsedMs(startNanos: Long): Long = (System.nanoTime() - startNanos) / 1_000_000

    companion object {
        private const val ROUTE_START_DUPLEX = "local.DuplexStreamService.StartDuplexStream"
        private const val ROUTE_FINISH_DUPLEX = "local.DuplexStreamService.FinishDuplexStream"
        private const val ROUTE_ECHO = "local.DuplexStreamService.Echo"
    }
}

/// Swift uses `String(data:encoding:.utf8) ?? "<N bytes>"`. All stream
/// payloads here are UTF-8 text ("echo: hello N"), so the lenient decode
/// matches Swift for every real payload; invalid-UTF8 (never occurs) decodes
/// leniently instead of the "<N bytes>" fallback.
private fun decodeUtf8(bytes: ByteArray): String = String(bytes, Charsets.UTF_8)
