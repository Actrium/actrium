# Changelog

Release notes are published on the
[GitHub Releases page](https://github.com/Actrium/actrium/releases).

## Unreleased

### Breaking: stream-first DataChunk APIs (#86)

The callback-first DataChunk APIs are replaced by stream-first ones across
all SDK surfaces. `DataChunk` (the wire item) gains an optional
`end_of_stream` field: a sink sends the in-band marker on `close`, and the
receiving stream yields every chunk queued before it, then ends (`None` /
completion / `nil`, per language). On `StreamLatencyFirst` lanes the marker
can be dropped like any other chunk, so applications on lossy lanes should
still back stream completion with a timeout. Receiver-side close/drop stays
an abort that discards buffered chunks.

- **Rust**: `Context::register_stream` / `unregister_stream` /
  `send_data_chunk` are removed. Use `Context::data_chunks(stream_id,
  options)` (a `futures_core::Stream` of `DataChunkEvent`) and
  `Context::data_chunk_sink(target, stream_id, payload_type)` (a
  `futures_sink::Sink<DataChunk>`).
- **FFI (UniFFI)**: `DataChunkCallback` and the callback-style
  `ContextBridge` methods are removed. Use the opaque
  `DataChunkStreamBridge` (`next` / `close`) and `DataChunkSinkBridge`
  (`send` / `close`) handles. On Kotlin, the bridge `close` methods are
  named `closeAsync` (UniFFI's `AutoCloseable` claims `close()`), and the
  ergonomic Flow adapter is `DataChunkStreamBridge.asFlow()` from the DSL.
- **Swift**: `DataChunkCallback` / `registerStream` / `unregisterStream` /
  `sendDataChunk` are removed from `ActrContext`. Use
  `dataChunks(streamId:capacity:)` (a pull-based `DataChunkSequence`
  `AsyncSequence`) and `dataChunkSink(target:streamId:payloadType:)`.
- **TypeScript**: `registerStream` / `unregisterStream` / `sendDataChunk` /
  `StreamCallback` are removed from `@actrium/actr-workload`. Use
  `dataChunks(streamId, { capacity })` (single-consumer
  `AsyncIterable<DataChunkEvent>`) and `dataChunkSink(target, streamId,
  payloadType)`.
- **Component Model ABI**: the WIT world moves to `actr:workload@0.8.0`. The
  `data-chunk` record gains `end-of-stream`, and `register-stream` takes an
  optional `capacity` so the guest's queue bound applies to the host-side
  subscription too. Packages built against older worlds are rejected at load
  with a rebuild hint.
- **Abort vs graceful end**: sinks gain `abort()` (Rust / FFI / Swift /
  Kotlin DSL), which seals the sink WITHOUT emitting the end-of-stream
  marker — the right failure-path tool; a graceful `close()` on a broken
  sink would otherwise report an end the peer should not trust.
- **Marker validation**: an `end_of_stream` chunk carrying a payload is a
  protocol violation — senders reject it (`InvalidArgument`), and a
  receiving stream surfaces one terminal `DecodeFailure` error instead of a
  clean end.
- **Marker sender inspection**: `DataChunkStream::end_marker`, the FFI
  `endMarker()` / `endedByMarker()`, and the Swift `DataChunkSequence`
  equivalents expose the terminating marker and its
  transport-authenticated sender, so apps that pin a stream to one producer
  can detect a foreign terminator.
- **C ABI**: `actr-cabi.h` publishes `ACTR_CHUNK_FLAG_END_OF_STREAM` (0x02)
  on the stream sink callback, and ActrSharp's `ActrStreamChunk` gains
  `EndOfStream`. `actr_stream_listen` no longer keeps the `ActrRef` alive
  for the listener's lifetime.

Behavior changes to note:

- Only one active subscription is allowed per `stream_id` within one
  actor/context instance; a duplicate returns `InvalidArgument` instead of
  replacing the consumer.
- Explicit consumer close (or language-level cancellation) unregisters
  immediately, drops buffered items, and wakes pending readers — an
  intentional cancellation exception to the graceful internal
  unregister/drain behavior.
- The FFI `ActrError::DeliveryUncertain` field `cause` is renamed
  `cause_detail` (Kotlin `Throwable.cause` would clash with the generated
  property). The rename shows up in the generated Kotlin *and* Swift
  bindings; the handwritten Swift `Actr` module maps it back to `cause`, so
  the public Swift API is unchanged. Wire format unchanged.
- On `wasm32-wasip2`, guest code only runs while an invocation is active:
  stream consumers must be polled inside active invocations (or spawned with
  `wit_bindgen::spawn_local` on the serial host path); the WIT
  `on-data-chunk` export remains internal plumbing feeding the bounded
  subscription queue.

