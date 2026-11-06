# DataChunk API Example

This example demonstrates the DataChunk API for streaming application data between Actors.

## Overview

The DataChunk API provides a Fast Path for streaming non-media data (file transfers, game state updates, custom protocols) between Actors without going through the RPC envelope mechanism.

**Key Concepts:**
- **DataChunk**: Protocol buffer message for streaming data chunks
- **Stream ID**: Identifies a specific data stream (e.g., "file-transfer")
- **Sequence Number**: Receiver-side reassembly key — chunks of one stream are dispatched in send order, but gaps are possible (`StreamLatencyFirst` drops under pressure, reconnects can lose in-flight reliable chunks), so reassemble and detect gaps by sequence rather than assuming contiguous arrival
- **Fast Path**: Bypasses RPC envelope for lower latency

## Architecture

```
Sender (datastream.Sender)
  └─ SenderWorkload
      └─ ctx.data_chunk_sink(&dest, stream_id, StreamReliable)
           └─ sink.send(chunk)
                │
                ├─ RuntimeContext (send path)
                ├─ Gate::send_data_chunk()
                └─ PeerGate::send_data_chunk()
                     │
                     └─ PeerTransport::send()
                          └─ WebRTC DataChannel / WebSocket

Receiver (datastream.Receiver)
  └─ ReceiverWorkload
      └─ ctx.data_chunks("file-transfer", options)
           └─ DataChunkRegistry (bounded per-stream queue)
                │
                └─ InboundPacketDispatcher::dispatch()
                     └─ stream.next() yields DataChunkEvent
```

## Components

### Sender
- **sender/src/main.rs**: Entry point, builds `ActrNode` and starts it
- **sender/src/sender_workload.rs**: Workload that sends 10 data chunks
- **sender/src/generated/**: Auto-generated code from proto files (via `actr gen`)

### Receiver
- **receiver/src/main.rs**: Entry point, builds `ActrNode` and starts it
- **receiver/src/receiver_workload.rs**: Workload that subscribes to inbound chunks via `data_chunks`
- **receiver/src/generated/**: Auto-generated code from proto files (via `actr gen`)

## Running the Example

### Prerequisites

1. **Loran (Signaling Server)** must be running. The `start.sh` script will automatically start it, or you can run it manually:
   ```bash
   loran --config loran-config.toml
   ```

2. **Code Generation**: The `start.sh` script automatically generates code using `actr gen`. The sender/receiver crates also run `actr gen` in `build.rs` during `cargo build`/`cargo run`—`actr` must be available in `PATH`; otherwise build will fail. To generate manually:
    ```bash
    cd data-stream/sender
    actr gen --input=proto --output=src/generated --clean --no-scaffold
   
   cd ../receiver
   actr gen --input=proto --output=src/generated --clean
   ```
   
   Note: Sender uses `--no-scaffold` because it doesn't need the user code template (uses `sender_workload.rs` instead).

### Option 1: Use start script (Recommended)

From the workspace root:
```bash
bash data-stream/start.sh
```

This will:
- Generate code using `actr gen`
- Build and start loran (signaling server)
- Build binaries
- Start receiver and sender automatically

### Option 2: Manual Start (For debugging)

From the workspace root:

Terminal 1 - Start Receiver:
```bash
RUST_LOG=info cargo run --bin data-stream-receiver
```

Terminal 2 - Start Sender (after receiver is ready):
```bash
RUST_LOG=info cargo run --bin data-stream-sender
```

You can also use the `-p` flag (equivalent):
```bash
cargo run -p data-stream-receiver
cargo run -p data-stream-sender
```

## Expected Output

### Receiver Output:
```
🚀 DataChunk Receiver starting
✅ ActrNode created
✅ ActrNode started!
🎉 Receiver ready to receive data streams
📦 Received chunk #1: stream_id=file-transfer, sequence=0, size=67 bytes
📄 Content preview: Chunk #0: Hello from DataChunk API! This is chunk number 0.
📦 Received chunk #2: stream_id=file-transfer, sequence=1, size=67 bytes
...
📊 Final statistics:
   Total chunks received: 10
   Total bytes received: 670
```

### Sender Output:
```
🚀 DataChunk Sender starting
✅ ActrNode created
✅ ActrNode started!
📤 SenderWorkload will start sending after 3 seconds...
📤 Sending chunk #0 (sequence=0, size=67 bytes)
✅ Chunk #0 sent successfully
📤 Sending chunk #1 (sequence=1, size=67 bytes)
✅ Chunk #1 sent successfully
...
✅ All chunks sent!
```

## Key API Methods

### Sender Side

```rust
use actr_protocol::DataChunk;
use actr_framework::{Context, Dest};
use futures_util::SinkExt;

// Create a sink bound to the destination, stream ID, and lane
let dest = Dest::Peer(receiver_id);
let mut sink = ctx
    .data_chunk_sink(&dest, "file-transfer".to_string(), PayloadType::StreamReliable)
    .await?;

// Send chunks; each chunk's stream_id must match the sink's
let chunk = DataChunk {
    stream_id: "file-transfer".to_string(),
    sequence: 0,
    payload: bytes::Bytes::from("Hello!"),
    metadata: vec![],
    timestamp_ms: Some(chrono::Utc::now().timestamp_millis()),
};
sink.send(chunk).await?;

// Flush and close when done
sink.close().await?;
```

`StreamReliable` waits for a usable WebRTC/WebSocket route inside the
framework and consumes one bounded send deadline. Do not retain the chunk for a
connection Hook or replay it from a fallback timer. A successful call means the
local transport accepted the chunk; it does not acknowledge remote processing.
If the result is an uncertain-delivery error, stop automatic replay and resolve
the outcome with an application ACK, checksum, or idempotency key.

### Receiver Side

```rust
use futures_util::StreamExt;

// Subscribe to a stream (single active subscription per stream_id)
let mut chunks = ctx
    .data_chunks("file-transfer".to_string(), Default::default())
    .await?;

while let Some(event) = chunks.next().await {
    let event = event?;
    println!(
        "Received chunk: sequence={}, size={}",
        event.chunk.sequence,
        event.chunk.payload.len()
    );
}

// Or close explicitly (drop also cancels the subscription)
chunks.close().await;
```

### Receiver-side rules

Same-stream chunks are delivered in send order, but gaps are possible:
`StreamLatencyFirst` drops chunks under pressure, and a reconnect can lose
in-flight reliable chunks. Use `DataChunk.sequence` as the authoritative
reassembly key and detect gaps explicitly instead of assuming contiguous
arrival.

## Use Cases

- **File Transfer**: Stream large files in chunks
- **Game State Sync**: Send game state updates at high frequency
- **Custom Protocols**: Implement your own streaming protocols
- **Log Streaming**: Stream application logs to monitoring services
- **Sensor Data**: Stream IoT sensor data

## Comparison with RPC

| Feature | RPC (call/tell) | DataChunk |
|---------|----------------|------------|
| Use Case | Request-response | Streaming data |
| Overhead | Higher (RpcEnvelope) | Lower (direct protobuf) |
| Ordering | Single request | Sequence numbers |
| Consumption | Per-message handler | Pull-based chunk stream |
| Best For | Commands, queries | Bulk data, high frequency |

## Next Steps

1. Try modifying chunk size and frequency
2. Implement proper file transfer with error handling
3. Add flow control and backpressure
4. Experiment with StreamLatencyFirst vs StreamReliable PayloadTypes
