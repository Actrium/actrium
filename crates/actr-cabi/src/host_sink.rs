//! Host-side sink bridging workload data to C callbacks.
//!
//! Two process-global channels let a C#/.NET host receive data that arrives
//! *after* the one-shot `actr_ref_call` completes:
//!
//! - **Stream sink** — `actr_register_stream_sink` stores a C callback;
//!   `actr_stream_listen` registers a framework stream handler on the node's
//!   context and forwards every inbound [`DataChunk`] to that callback.
//! - **Reverse-RPC sink** — a Rust workload dispatcher calls [`ask_host`]
//!   with an inbound route it wants the host (e.g. a UI asking the user a
//!   question) to answer; the host receives the request through its
//!   registered callback and replies via `actr_host_respond_reverse_rpc`.
//!
//! Both sinks are process-global because actr-cabi has a single-provider
//! contract: exactly one `actr_cabi` cdylib (and in practice one node) per
//! process. Callbacks fire synchronously on actr-cabi runtime worker threads
//! and must not panic or unwind: a panic inside an `extern "C"` callback
//! aborts the process under extern-C unwind semantics, before any Rust handler
//! could observe it. Every buffer passed to a callback is borrowed from
//! Rust-owned memory and is valid only for the duration of the callback.
//! Replacing or clearing a sink waits for its in-flight callbacks so the old
//! user data can be released once registration returns. A sink must not
//! replace or clear either sink from inside a callback; that operation returns
//! `GENERIC_ERROR` instead of deadlocking.

use std::cell::Cell;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock, RwLock};
use std::time::{Duration, Instant};

use actr_protocol::{ActorResult, ActrError, ActrId, DataChunk};
use bytes::Bytes;
use tokio::sync::oneshot;

use crate::error::SUCCESS;

// ── C callback types ────────────────────────────────────────────────────────

/// A borrowed metadata key/value pair passed to [`StreamSinkCallback`].
/// The entry and both byte buffers are valid only for the callback duration.
#[repr(C)]
pub struct StreamMetadataEntry {
    pub key_ptr: *const u8,
    pub key_len: usize,
    pub value_ptr: *const u8,
    pub value_len: usize,
}

/// Stream-chunk sink signature. `chunk_flags` bit 0 (`0x01`) indicates that
/// `timestamp_ms` is set; other bits are reserved and must be ignored.
/// All pointers are valid only for the duration of the callback.
pub type StreamSinkCallback = unsafe extern "C" fn(
    stream_id_ptr: *const u8,
    stream_id_len: usize,
    payload_ptr: *const u8,
    payload_len: usize,
    metadata_ptr: *const StreamMetadataEntry,
    metadata_len: usize,
    sequence: u64,
    chunk_flags: u8,
    timestamp_ms: i64,
    sender_id_ptr: *const u8,
    sender_id_len: usize,
    user_data: *mut c_void,
);

/// Reverse-RPC sink signature. The host must eventually answer `request_id`
/// via `actr_host_respond_reverse_rpc`. Buffers are valid only for the
/// duration of the callback.
pub type ReverseRpcCallback = unsafe extern "C" fn(
    request_id: u64,
    route_ptr: *const u8,
    route_len: usize,
    payload_ptr: *const u8,
    payload_len: usize,
    user_data: *mut c_void,
);

/// Registered stream sink (C function pointer + verbatim user data).
struct StreamSink {
    callback: StreamSinkCallback,
    user_data: *mut c_void,
}

/// Registered reverse-RPC sink.
struct ReverseSink {
    callback: ReverseRpcCallback,
    user_data: *mut c_void,
}

// SAFETY: sinks are installed only via the unsafe C entry points, whose
// contract requires user_data to remain valid and safe for concurrent
// runtime-thread access until the sink is replaced or cleared. The function
// pointers are themselves Send + Sync.
unsafe impl Send for StreamSink {}
unsafe impl Sync for StreamSink {}
unsafe impl Send for ReverseSink {}
unsafe impl Sync for ReverseSink {}

static STREAM_SINK: RwLock<Option<StreamSink>> = RwLock::new(None);
static REVERSE_SINK: RwLock<Option<ReverseSink>> = RwLock::new(None);

struct PendingAsk {
    sender: oneshot::Sender<Result<Bytes, i32>>,
    deadline: Instant,
}

struct PendingRequests(HashMap<u64, PendingAsk>);

static PENDING_REQUESTS: OnceLock<Mutex<PendingRequests>> = OnceLock::new();
static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    /// Replacing or clearing a sink from that sink's own callback would need
    /// to wait for itself to return. Reject it rather than deadlocking.
    static IN_SINK_CALLBACK: Cell<u32> = const { Cell::new(0) };
}

struct SinkCallbackScope;

impl SinkCallbackScope {
    fn enter() -> Self {
        IN_SINK_CALLBACK.with(|depth| depth.set(depth.get() + 1));
        Self
    }

    fn active() -> bool {
        IN_SINK_CALLBACK.with(|depth| depth.get() != 0)
    }
}

impl Drop for SinkCallbackScope {
    fn drop(&mut self) {
        IN_SINK_CALLBACK.with(|depth| depth.set(depth.get() - 1));
    }
}

fn pending_requests() -> MutexGuard<'static, PendingRequests> {
    PENDING_REQUESTS
        .get_or_init(|| Mutex::new(PendingRequests(HashMap::new())))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Removes the pending entry when an `ask_host` future is cancelled before a
/// host response or deadline resolves it. A caller is allowed to drop its
/// future at any await point; retaining the sender until a host eventually
/// responds would otherwise leak the request indefinitely.
struct PendingAskGuard {
    request_id: u64,
}

impl Drop for PendingAskGuard {
    fn drop(&mut self) {
        pending_requests().0.remove(&self.request_id);
    }
}

fn insert_pending_ask(sender: oneshot::Sender<Result<Bytes, i32>>, deadline: Instant) -> u64 {
    let mut pending = pending_requests();
    loop {
        // Request id zero is reserved as the initial / unset value in common
        // hosts, so skip it if the atomic counter ever wraps. Never overwrite
        // a live request if the counter does wrap in a long-running process.
        let request_id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        if request_id == 0 {
            continue;
        }
        if let Entry::Vacant(entry) = pending.0.entry(request_id) {
            entry.insert(PendingAsk { sender, deadline });
            return request_id;
        }
    }
}

fn map_host_response(result: Result<Bytes, i32>) -> ActorResult<Bytes> {
    match result {
        Ok(bytes) => Ok(bytes),
        Err(host_status) => Err(ActrError::Unavailable(format!(
            "host reverse-RPC callback failed with status {host_status}"
        ))),
    }
}

/// Resolve the race between the task's deadline and a host response. The
/// responder sends while holding the pending map lock, so after this removes
/// the entry a later `try_recv` distinguishes an already accepted response
/// from a response that lost the deadline race.
fn finish_timed_out_ask(
    request_id: u64,
    receiver: &mut oneshot::Receiver<Result<Bytes, i32>>,
) -> ActorResult<Bytes> {
    let timeout_cleanup_won = pending_requests().0.remove(&request_id).is_some();
    match receiver.try_recv() {
        Ok(result) => map_host_response(result),
        // Removing the entry drops its sender, so a closed receiver means the
        // timeout cleanup won rather than that the host bridge failed.
        Err(tokio::sync::oneshot::error::TryRecvError::Closed) if timeout_cleanup_won => {
            Err(ActrError::TimedOut)
        }
        Err(tokio::sync::oneshot::error::TryRecvError::Closed) => Err(ActrError::Unavailable(
            "host reverse-RPC bridge dropped the pending request".into(),
        )),
        Err(tokio::sync::oneshot::error::TryRecvError::Empty) => Err(ActrError::TimedOut),
    }
}

// ── sink registration (called from the C entries) ───────────────────────────

/// Install or clear (None) the process-global stream sink. This blocks until
/// existing callbacks finish, unless called from a sink callback itself.
pub fn set_stream_sink(
    sink: Option<StreamSinkCallback>,
    user_data: *mut c_void,
) -> Result<(), i32> {
    if SinkCallbackScope::active() {
        return Err(crate::GENERIC_ERROR);
    }
    let mut guard = STREAM_SINK.write().unwrap_or_else(|e| e.into_inner());
    *guard = sink.map(|callback| StreamSink {
        callback,
        user_data,
    });
    Ok(())
}

/// Install or clear (None) the process-global reverse-RPC sink. This blocks
/// until existing callbacks finish, unless called from a sink callback itself.
pub fn set_reverse_sink(
    sink: Option<ReverseRpcCallback>,
    user_data: *mut c_void,
) -> Result<(), i32> {
    if SinkCallbackScope::active() {
        return Err(crate::GENERIC_ERROR);
    }
    let mut guard = REVERSE_SINK.write().unwrap_or_else(|e| e.into_inner());
    *guard = sink.map(|callback| ReverseSink {
        callback,
        user_data,
    });
    Ok(())
}

// ── stream forwarding ───────────────────────────────────────────────────────

/// `ACTR_CHUNK_FLAG_HAS_TIMESTAMP`: `chunk_flags` bit set when the producer
/// annotated the chunk with `timestamp_ms`.
pub const ACTR_CHUNK_FLAG_HAS_TIMESTAMP: u8 = 0x01;

/// Forward one inbound chunk to the registered host stream sink.
///
/// Returns `true` when a sink was registered (the callback was fired),
/// `false` when no sink is registered and the chunk was dropped. Dropping is
/// not an error: a host that never registered a sink has no stream consumer
/// to notify. The callback must not panic — see the module docs.
pub fn forward_chunk_to_host(chunk: &DataChunk, sender: &ActrId) -> bool {
    // Keep the read lock for the whole callback. A clear or replacement takes
    // the write lock, so when its C entry point returns the old user_data is
    // no longer reachable by an in-flight native callback.
    let guard = STREAM_SINK.read().unwrap_or_else(|e| e.into_inner());
    let Some(sink) = guard.as_ref() else {
        tracing::trace!(
            stream_id = %chunk.stream_id,
            sequence = chunk.sequence,
            "dropping DataChunk: no host stream sink registered"
        );
        return false;
    };

    let stream_id = chunk.stream_id.as_bytes();
    let payload = &chunk.payload[..];
    let metadata: Vec<StreamMetadataEntry> = chunk
        .metadata
        .iter()
        .map(|entry| StreamMetadataEntry {
            key_ptr: entry.key.as_ptr(),
            key_len: entry.key.len(),
            value_ptr: entry.value.as_ptr(),
            value_len: entry.value.len(),
        })
        .collect();
    let sender_repr = sender.to_string_repr();
    let sender_bytes = sender_repr.as_bytes();
    let flags = if chunk.timestamp_ms.is_some() {
        ACTR_CHUNK_FLAG_HAS_TIMESTAMP
    } else {
        0
    };
    let timestamp_ms = chunk.timestamp_ms.unwrap_or(0);

    // SAFETY: the sink contract requires the host callback to treat every
    // borrowed pointer as valid only for the duration of this call. The
    // slices above are borrowed from values that outlive the call.
    unsafe {
        let _scope = SinkCallbackScope::enter();
        (sink.callback)(
            stream_id.as_ptr(),
            stream_id.len(),
            payload.as_ptr(),
            payload.len(),
            if metadata.is_empty() {
                std::ptr::null()
            } else {
                metadata.as_ptr()
            },
            metadata.len(),
            chunk.sequence,
            flags,
            timestamp_ms,
            sender_bytes.as_ptr(),
            sender_bytes.len(),
            sink.user_data,
        )
    }
    true
}

// ── reverse RPC ─────────────────────────────────────────────────────────────

/// Ask the host to service an inbound route and await its answer.
///
/// Intended for Rust workload dispatchers linked into the actr_cabi cdylib:
/// when an inbound envelope should be answered by the embedding host (e.g. an
/// AskUserQuestion round-trip into UI), the dispatcher calls this with the
/// route key and payload. The registered host callback receives
/// `request_id` + route + payload and must eventually call
/// `actr_host_respond_reverse_rpc`; `ask_host` resolves with the host's
/// payload (or error) or `ActrError::TimedOut` after `timeout_ms`.
///
/// Errors: `InvalidArgument` for a non-positive timeout, `Unavailable`
/// when no reverse-RPC sink is registered, the host answered with a failure
/// status, or the pending entry was dropped; `TimedOut` when the host does
/// not answer in time.
pub async fn ask_host(route: &str, payload: Bytes, timeout_ms: i64) -> ActorResult<Bytes> {
    if timeout_ms <= 0 {
        return Err(ActrError::InvalidArgument(format!(
            "ask_host timeout_ms must be > 0, got {timeout_ms}"
        )));
    }
    let timeout = Duration::from_millis(timeout_ms as u64);
    let Some(deadline) = Instant::now().checked_add(timeout) else {
        return Err(ActrError::InvalidArgument(
            "ask_host timeout_ms is too large for this platform".into(),
        ));
    };

    let (tx, rx) = oneshot::channel::<Result<Bytes, i32>>();
    // Insert before firing so a synchronous host response can never race the
    // map entry. The guard also removes it if this async operation is dropped.
    let request_id = insert_pending_ask(tx, deadline);
    let _pending_guard = PendingAskGuard { request_id };

    let route_bytes = route.as_bytes();
    let payload_slice = &payload[..];
    {
        // Holding the read lock prevents clear/replace from returning while
        // this callback could still observe its old user_data.
        let guard = REVERSE_SINK.read().unwrap_or_else(|e| e.into_inner());
        let Some(sink) = guard.as_ref() else {
            return Err(ActrError::Unavailable(
                "no reverse-RPC sink registered; cannot ask the host".into(),
            ));
        };
        // SAFETY: same borrowed-buffer contract as the stream sink.
        unsafe {
            let _scope = SinkCallbackScope::enter();
            (sink.callback)(
                request_id,
                route_bytes.as_ptr(),
                route_bytes.len(),
                payload_slice.as_ptr(),
                payload_slice.len(),
                sink.user_data,
            )
        }
    }

    // A synchronous host response can have been accepted before its deadline
    // even if the callback itself returned after that deadline. Consume it
    // before checking remaining time so that valid response is not lost.
    let mut rx = rx;
    match rx.try_recv() {
        Ok(result) => return map_host_response(result),
        Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
            return Err(ActrError::Unavailable(
                "host reverse-RPC bridge dropped the pending request".into(),
            ));
        }
        Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
    }

    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return finish_timed_out_ask(request_id, &mut rx);
    }

    match tokio::time::timeout(remaining, &mut rx).await {
        // Host answered successfully.
        Ok(Ok(result)) => map_host_response(result),
        // Sender dropped after an external cancellation of the asker.
        Ok(Err(_)) => Err(ActrError::Unavailable(
            "host reverse-RPC bridge dropped the pending request".into(),
        )),
        // The timeout future can win scheduling even after a response was
        // accepted just before the deadline. Prefer that accepted response;
        // `host_respond` rejects responses that arrive after the deadline.
        Err(_) => finish_timed_out_ask(request_id, &mut rx),
    }
}

/// Deliver the host's answer for a pending `ask_host` request.
///
/// `status == SUCCESS` resolves the awaiting dispatcher with `payload`;
/// any other status resolves it with `ActrError::Unavailable`. Returns
/// `Err(REQUEST_ID_UNKNOWN)` when `request_id` is unknown, already
/// answered, or timed out.
pub fn host_respond(request_id: u64, status: i32, payload: &[u8]) -> Result<(), i32> {
    let result = if status == SUCCESS {
        Ok(Bytes::copy_from_slice(payload))
    } else {
        Err(status)
    };
    {
        let mut pending = pending_requests();
        let Some(ask) = pending.0.remove(&request_id) else {
            return Err(crate::error::REQUEST_ID_UNKNOWN);
        };
        if Instant::now() >= ask.deadline {
            return Err(crate::error::REQUEST_ID_UNKNOWN);
        }
        // Send before releasing the map lock. This makes an accepted answer
        // observable before a concurrent deadline cleanup can decide timeout.
        // A send error merely means the asker was cancelled after we won the
        // map race, so this response is still consumed exactly once.
        let _ = ask.sender.send(result);
    }
    Ok(())
}

/// Number of requests still awaiting a host answer (diagnostic / tests).
pub fn pending_ask_count() -> usize {
    pending_requests().0.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    // The C entry points exercise these paths end-to-end in
    // tests/host_sink.rs; unit tests here only cover pure helpers.

    #[test]
    fn chunk_flag_bit_is_stable() {
        assert_eq!(ACTR_CHUNK_FLAG_HAS_TIMESTAMP, 0x01);
    }
}
