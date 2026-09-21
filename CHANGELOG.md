# Changelog

Release notes are published on the
[GitHub Releases page](https://github.com/Actrium/actrium/releases).

## Unreleased

### Breaking: stream-first DataChunk APIs (#86)

The callback-first DataChunk APIs are replaced by stream-first ones across
all SDK surfaces. `DataChunk` (the wire item) is unchanged.

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

Behavior changes to note:

- Only one active subscription is allowed per `stream_id` within one
  actor/context instance; a duplicate returns `InvalidArgument` instead of
  replacing the consumer.
- Explicit consumer close (or language-level cancellation) unregisters
  immediately, drops buffered items, and wakes pending readers — an
  intentional cancellation exception to the graceful internal
  unregister/drain behavior.
- The FFI `ActrError::DeliveryUncertain` field `cause` is renamed
  `cause_detail` (Kotlin `Throwable.cause` clash); wire format unchanged.
- On `wasm32-wasip2`, guest code only runs while an invocation is active:
  stream consumers must be polled inside active invocations (or spawned with
  `wit_bindgen::spawn_local` on the serial host path); the WIT
  `on-data-chunk` export remains internal plumbing feeding the bounded
  subscription queue.

