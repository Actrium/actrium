//! Host-bridge integration tests for actr-cabi.
//!
//! Exercises the process-global stream sink and reverse-RPC bridge:
//! - `actr_register_stream_sink` + `forward_chunk_to_host` field fidelity
//! - `ask_host` round-trip via `actr_host_respond_reverse_rpc` (success,
//!   host-failure status, deadline and cancellation cleanup, no sink)
//! - `actr_host_respond_reverse_rpc` input validation (-22 / -10)
//!
//! The sinks are process-global, so every test that touches them holds
//! `BRIDGE` for its duration and clears both sinks before releasing it.

#![allow(clippy::missing_panics_doc)] // not a library

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::Duration;

use actr_protocol::{ActrError, ActrId, DataChunk, MetadataEntry};
use bytes::Bytes;

/// Serializes tests that touch the process-global sinks.
static BRIDGE: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, PartialEq, Eq)]
struct CapturedChunk {
    stream_id: String,
    payload: Vec<u8>,
    metadata: Vec<(String, String)>,
    sequence: u64,
    flags: u8,
    timestamp_ms: i64,
    sender: String,
}

static CAPTURED: Mutex<Vec<CapturedChunk>> = Mutex::new(Vec::new());
static LAST_REQUEST_ID: AtomicU64 = AtomicU64::new(0);
static EMPTY_METADATA_PTR_WAS_NULL: AtomicBool = AtomicBool::new(false);
static IMMEDIATE_RESPONSE_STATUS: AtomicI32 = AtomicI32::new(i32::MIN);

struct BlockingSinkState {
    entered: bool,
    release: bool,
}

static BLOCKING_SINK: (Mutex<BlockingSinkState>, Condvar) = (
    Mutex::new(BlockingSinkState {
        entered: false,
        release: false,
    }),
    Condvar::new(),
);

unsafe extern "C" fn capture_stream_sink(
    stream_id_ptr: *const u8,
    stream_id_len: usize,
    payload_ptr: *const u8,
    payload_len: usize,
    metadata_ptr: *const actr_cabi::StreamMetadataEntry,
    metadata_len: usize,
    sequence: u64,
    flags: u8,
    timestamp_ms: i64,
    sender_ptr: *const u8,
    sender_len: usize,
    _user_data: *mut c_void,
) {
    // SAFETY: actr-cabi guarantees these pointers are readable for the
    // duration of the callback; copy immediately.
    let stream_id = String::from_utf8_lossy(unsafe {
        std::slice::from_raw_parts(stream_id_ptr, stream_id_len)
    })
    .into_owned();
    let payload = unsafe { std::slice::from_raw_parts(payload_ptr, payload_len) }.to_vec();
    let metadata = unsafe { std::slice::from_raw_parts(metadata_ptr, metadata_len) }
        .iter()
        .map(|entry| {
            let key = String::from_utf8_lossy(unsafe {
                std::slice::from_raw_parts(entry.key_ptr, entry.key_len)
            })
            .into_owned();
            let value = String::from_utf8_lossy(unsafe {
                std::slice::from_raw_parts(entry.value_ptr, entry.value_len)
            })
            .into_owned();
            (key, value)
        })
        .collect();
    let sender =
        String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(sender_ptr, sender_len) })
            .into_owned();
    CAPTURED.lock().unwrap().push(CapturedChunk {
        stream_id,
        payload,
        metadata,
        sequence,
        flags,
        timestamp_ms,
        sender,
    });
}

fn chunk(timestamp_ms: Option<i64>) -> DataChunk {
    DataChunk {
        stream_id: "stream-1".to_string(),
        sequence: 7,
        payload: Bytes::from_static(b"hello chunk"),
        metadata: vec![MetadataEntry {
            key: "content-type".to_string(),
            value: "application/octet-stream".to_string(),
        }],
        timestamp_ms,
        end_of_stream: None,
    }
}

fn chunk_without_metadata() -> DataChunk {
    DataChunk {
        metadata: Vec::new(),
        ..chunk(None)
    }
}

fn clear_sinks() {
    // SAFETY: null sinks are the documented clear path.
    unsafe {
        let _ = actr_cabi::actr_register_stream_sink(None, std::ptr::null_mut());
        let _ = actr_cabi::actr_register_reverse_rpc_sink(None, std::ptr::null_mut());
    }
}

fn current_thread_rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
}

fn lock_bridge() -> std::sync::MutexGuard<'static, ()> {
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(clear_sinks);
    BRIDGE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A valid ActrId (ActrId::default() has an empty Realm name).
fn sender_id() -> ActrId {
    ActrId::from_string_repr("1a2b3c.test@example.com").expect("valid test actor id")
}

// ── stream sink ────────────────────────────────────────────────────────────

#[test]
fn stream_sink_marks_graceful_end_of_stream() {
    let _bridge = lock_bridge();
    clear_sinks();
    CAPTURED.lock().unwrap().clear();

    // SAFETY: capture_stream_sink is a valid function pointer.
    let rc = unsafe {
        actr_cabi::actr_register_stream_sink(Some(capture_stream_sink), std::ptr::null_mut())
    };
    assert_eq!(rc, 0);

    let sender = sender_id();
    let expected_sender = sender.to_string_repr();
    // The end-of-stream marker is a control signal: empty payload, the
    // marker's own sequence and sender, and the flag bit set.
    let mut marker = chunk(None);
    marker.payload = Bytes::new();
    marker.end_of_stream = Some(true);
    assert!(actr_cabi::forward_chunk_to_host(&marker, &sender));

    let captured = CAPTURED.lock().unwrap();
    assert_eq!(captured.len(), 1);
    let captured = &captured[0];
    assert_eq!(captured.flags, actr_cabi::ACTR_CHUNK_FLAG_END_OF_STREAM);
    assert!(captured.payload.is_empty());
    assert_eq!(captured.sequence, 7);
    assert_eq!(captured.sender, expected_sender);
}

#[test]
fn stream_sink_forwards_all_chunk_fields() {
    let _bridge = lock_bridge();
    clear_sinks();
    CAPTURED.lock().unwrap().clear();

    // SAFETY: capture_stream_sink is a valid function pointer.
    let rc = unsafe {
        actr_cabi::actr_register_stream_sink(Some(capture_stream_sink), std::ptr::null_mut())
    };
    assert_eq!(rc, 0);

    let sender = sender_id();
    let expected_sender = sender.to_string_repr();
    assert!(actr_cabi::forward_chunk_to_host(
        &chunk(Some(1234)),
        &sender
    ));

    let captured = CAPTURED.lock().unwrap();
    assert_eq!(
        *captured,
        vec![CapturedChunk {
            stream_id: "stream-1".to_string(),
            payload: b"hello chunk".to_vec(),
            metadata: vec![(
                "content-type".to_string(),
                "application/octet-stream".to_string()
            )],
            sequence: 7,
            flags: actr_cabi::ACTR_CHUNK_FLAG_HAS_TIMESTAMP,
            timestamp_ms: 1234,
            sender: expected_sender,
        }]
    );
}

unsafe extern "C" fn blocking_stream_sink(
    _stream_id_ptr: *const u8,
    _stream_id_len: usize,
    _payload_ptr: *const u8,
    _payload_len: usize,
    _metadata_ptr: *const actr_cabi::StreamMetadataEntry,
    _metadata_len: usize,
    _sequence: u64,
    _flags: u8,
    _timestamp_ms: i64,
    _sender_ptr: *const u8,
    _sender_len: usize,
    _user_data: *mut c_void,
) {
    let (lock, ready) = &BLOCKING_SINK;
    let mut state = lock.lock().unwrap();
    state.entered = true;
    ready.notify_all();
    while !state.release {
        state = ready.wait(state).unwrap();
    }
}

#[test]
fn clearing_stream_sink_waits_for_in_flight_callback() {
    let _bridge = lock_bridge();
    clear_sinks();
    {
        let (lock, _) = &BLOCKING_SINK;
        *lock.lock().unwrap() = BlockingSinkState {
            entered: false,
            release: false,
        };
    }
    // SAFETY: valid callback with no user data.
    unsafe {
        actr_cabi::actr_register_stream_sink(Some(blocking_stream_sink), std::ptr::null_mut())
    };

    let sender = sender_id();
    let forwarder =
        std::thread::spawn(move || actr_cabi::forward_chunk_to_host(&chunk(None), &sender));
    {
        let (lock, ready) = &BLOCKING_SINK;
        let mut state = lock.lock().unwrap();
        while !state.entered {
            state = ready.wait(state).unwrap();
        }
    }

    let (cleared_tx, cleared_rx) = std::sync::mpsc::channel();
    let clearer = std::thread::spawn(move || {
        // SAFETY: null is the documented clear path.
        let rc = unsafe { actr_cabi::actr_register_stream_sink(None, std::ptr::null_mut()) };
        cleared_tx.send(rc).unwrap();
    });
    assert!(cleared_rx.recv_timeout(Duration::from_millis(20)).is_err());

    {
        let (lock, ready) = &BLOCKING_SINK;
        lock.lock().unwrap().release = true;
        ready.notify_all();
    }
    assert!(forwarder.join().unwrap());
    assert_eq!(cleared_rx.recv_timeout(Duration::from_secs(1)).unwrap(), 0);
    clearer.join().unwrap();
}

#[test]
fn stream_sink_without_timestamp_clears_flag() {
    let _bridge = lock_bridge();
    clear_sinks();
    CAPTURED.lock().unwrap().clear();

    // SAFETY: valid function pointer.
    unsafe {
        actr_cabi::actr_register_stream_sink(Some(capture_stream_sink), std::ptr::null_mut())
    };
    assert!(actr_cabi::forward_chunk_to_host(&chunk(None), &sender_id()));

    let captured = CAPTURED.lock().unwrap();
    let first = captured.first().expect("chunk captured");
    assert_eq!(first.flags, 0);
    assert_eq!(first.timestamp_ms, 0);
}

unsafe extern "C" fn empty_metadata_sink(
    _stream_id_ptr: *const u8,
    _stream_id_len: usize,
    _payload_ptr: *const u8,
    _payload_len: usize,
    metadata_ptr: *const actr_cabi::StreamMetadataEntry,
    metadata_len: usize,
    _sequence: u64,
    _flags: u8,
    _timestamp_ms: i64,
    _sender_ptr: *const u8,
    _sender_len: usize,
    _user_data: *mut c_void,
) {
    EMPTY_METADATA_PTR_WAS_NULL.store(
        metadata_len == 0 && metadata_ptr.is_null(),
        Ordering::SeqCst,
    );
}

#[test]
fn stream_sink_uses_null_metadata_pointer_for_empty_metadata() {
    let _bridge = lock_bridge();
    clear_sinks();
    EMPTY_METADATA_PTR_WAS_NULL.store(false, Ordering::SeqCst);

    // SAFETY: valid callback with no user data.
    assert_eq!(
        unsafe {
            actr_cabi::actr_register_stream_sink(Some(empty_metadata_sink), std::ptr::null_mut())
        },
        0
    );
    assert!(actr_cabi::forward_chunk_to_host(
        &chunk_without_metadata(),
        &sender_id()
    ));
    assert!(EMPTY_METADATA_PTR_WAS_NULL.load(Ordering::SeqCst));
}

#[test]
fn chunk_without_sink_is_dropped_not_fatal() {
    let _bridge = lock_bridge();
    clear_sinks();

    assert!(!actr_cabi::forward_chunk_to_host(
        &chunk(None),
        &sender_id()
    ));
}

// NOTE: there is deliberately no "panicking sink is contained" test — a
// panic inside an extern "C" callback aborts the process before any Rust
// handler can observe it (extern-C unwind semantics), so containment is not
// a testable property. The contract lives in the header docs: callbacks
// must not panic or unwind.

// ── reverse RPC ────────────────────────────────────────────────────────────

/// Sink that answers asynchronously from a fresh thread after a short delay,
/// exercising the cross-thread resolve path C# hosts use.
unsafe extern "C" fn responding_reverse_sink(
    request_id: u64,
    _route_ptr: *const u8,
    _route_len: usize,
    _payload_ptr: *const u8,
    _payload_len: usize,
    _user_data: *mut c_void,
) {
    LAST_REQUEST_ID.store(request_id, Ordering::SeqCst);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(10));
        let answer = b"42";
        // SAFETY: answer outlives the synchronous call.
        unsafe {
            actr_cabi::actr_host_respond_reverse_rpc(request_id, 0, answer.as_ptr(), answer.len())
        };
    });
}

/// Sink that answers with a non-zero status.
unsafe extern "C" fn failing_reverse_sink(
    request_id: u64,
    _route_ptr: *const u8,
    _route_len: usize,
    _payload_ptr: *const u8,
    _payload_len: usize,
    _user_data: *mut c_void,
) {
    LAST_REQUEST_ID.store(request_id, Ordering::SeqCst);
    std::thread::spawn(move || {
        // SAFETY: empty payload with a null pointer is the documented
        // empty-payload encoding.
        unsafe { actr_cabi::actr_host_respond_reverse_rpc(request_id, -7, std::ptr::null(), 0) };
    });
}

/// Answers immediately, then keeps the callback active past the request
/// deadline. The response was valid when accepted and must win over the
/// callback-return timing.
unsafe extern "C" fn immediate_response_then_delays_reverse_sink(
    request_id: u64,
    _route_ptr: *const u8,
    _route_len: usize,
    _payload_ptr: *const u8,
    _payload_len: usize,
    _user_data: *mut c_void,
) {
    let answer = b"accepted-before-deadline";
    // SAFETY: answer remains valid for the duration of this synchronous call.
    let status = unsafe {
        actr_cabi::actr_host_respond_reverse_rpc(request_id, 0, answer.as_ptr(), answer.len())
    };
    IMMEDIATE_RESPONSE_STATUS.store(status, Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(20));
}

/// Sink that never answers.
unsafe extern "C" fn silent_reverse_sink(
    _request_id: u64,
    _route_ptr: *const u8,
    _route_len: usize,
    _payload_ptr: *const u8,
    _payload_len: usize,
    _user_data: *mut c_void,
) {
    LAST_REQUEST_ID.store(_request_id, Ordering::SeqCst);
}

unsafe extern "C" fn blocking_reverse_sink(
    request_id: u64,
    _route_ptr: *const u8,
    _route_len: usize,
    _payload_ptr: *const u8,
    _payload_len: usize,
    _user_data: *mut c_void,
) {
    LAST_REQUEST_ID.store(request_id, Ordering::SeqCst);
    let (lock, ready) = &BLOCKING_SINK;
    let mut state = lock.lock().unwrap();
    state.entered = true;
    ready.notify_all();
    while !state.release {
        state = ready.wait(state).unwrap();
    }
}

#[test]
fn clearing_reverse_sink_waits_for_in_flight_callback() {
    let _bridge = lock_bridge();
    clear_sinks();
    LAST_REQUEST_ID.store(0, Ordering::SeqCst);
    {
        let (lock, _) = &BLOCKING_SINK;
        *lock.lock().unwrap() = BlockingSinkState {
            entered: false,
            release: false,
        };
    }
    // SAFETY: valid callback with no user data.
    unsafe {
        actr_cabi::actr_register_reverse_rpc_sink(Some(blocking_reverse_sink), std::ptr::null_mut())
    };

    let asker = std::thread::spawn(|| {
        current_thread_rt().block_on(actr_cabi::ask_host("ask.Host/Block", Bytes::new(), 1_000))
    });
    {
        let (lock, ready) = &BLOCKING_SINK;
        let mut state = lock.lock().unwrap();
        while !state.entered {
            state = ready.wait(state).unwrap();
        }
    }

    let (cleared_tx, cleared_rx) = std::sync::mpsc::channel();
    let clearer = std::thread::spawn(move || {
        // SAFETY: null is the documented clear path.
        let rc = unsafe { actr_cabi::actr_register_reverse_rpc_sink(None, std::ptr::null_mut()) };
        cleared_tx.send(rc).unwrap();
    });
    assert!(cleared_rx.recv_timeout(Duration::from_millis(20)).is_err());

    {
        let (lock, ready) = &BLOCKING_SINK;
        lock.lock().unwrap().release = true;
        ready.notify_all();
    }
    assert_eq!(cleared_rx.recv_timeout(Duration::from_secs(1)).unwrap(), 0);
    clearer.join().unwrap();

    let request_id = LAST_REQUEST_ID.load(Ordering::SeqCst);
    assert_ne!(request_id, 0);
    let answer = b"released";
    // SAFETY: the callback installed this exact pending request id.
    assert_eq!(
        unsafe {
            actr_cabi::actr_host_respond_reverse_rpc(request_id, 0, answer.as_ptr(), answer.len())
        },
        0
    );
    assert_eq!(
        asker.join().unwrap().expect("host answer"),
        Bytes::from_static(b"released")
    );
}

#[test]
fn ask_host_roundtrip_resolves_with_host_payload() {
    let _bridge = lock_bridge();
    clear_sinks();
    LAST_REQUEST_ID.store(0, Ordering::SeqCst);

    // SAFETY: valid function pointer.
    unsafe {
        actr_cabi::actr_register_reverse_rpc_sink(
            Some(responding_reverse_sink),
            std::ptr::null_mut(),
        )
    };

    let rt = current_thread_rt();
    let answer = rt
        .block_on(actr_cabi::ask_host(
            "ask.AskService/AskUserQuestion",
            Bytes::from_static(b"{\"q\":\"ping\"}"),
            5_000,
        ))
        .expect("host should answer");
    assert_eq!(&answer[..], b"42");
    assert_eq!(actr_cabi::pending_ask_count(), 0);

    // The original, already-consumed id cannot be answered twice.
    let request_id = LAST_REQUEST_ID.load(Ordering::SeqCst);
    assert_ne!(request_id, 0);
    // SAFETY: this exact request has already been resolved.
    let duplicate =
        unsafe { actr_cabi::actr_host_respond_reverse_rpc(request_id, 0, std::ptr::null(), 0) };
    assert_eq!(duplicate, actr_cabi::REQUEST_ID_UNKNOWN);
}

#[test]
fn accepted_synchronous_response_wins_after_callback_returns_past_deadline() {
    let _bridge = lock_bridge();
    clear_sinks();
    IMMEDIATE_RESPONSE_STATUS.store(i32::MIN, Ordering::SeqCst);

    // SAFETY: valid callback with no user data.
    assert_eq!(
        unsafe {
            actr_cabi::actr_register_reverse_rpc_sink(
                Some(immediate_response_then_delays_reverse_sink),
                std::ptr::null_mut(),
            )
        },
        0
    );

    let answer = current_thread_rt()
        .block_on(actr_cabi::ask_host("ask.Host/Immediate", Bytes::new(), 1))
        .expect("response was accepted before the deadline");
    assert_eq!(IMMEDIATE_RESPONSE_STATUS.load(Ordering::SeqCst), 0);
    assert_eq!(&answer[..], b"accepted-before-deadline");
    assert_eq!(actr_cabi::pending_ask_count(), 0);
}

#[test]
fn ask_host_failure_status_maps_to_unavailable() {
    let _bridge = lock_bridge();
    clear_sinks();

    // SAFETY: valid function pointer.
    unsafe {
        actr_cabi::actr_register_reverse_rpc_sink(Some(failing_reverse_sink), std::ptr::null_mut())
    };

    let rt = current_thread_rt();
    let err = rt
        .block_on(actr_cabi::ask_host("ask.Host/Fail", Bytes::new(), 5_000))
        .expect_err("host reported failure");
    match err {
        ActrError::Unavailable(msg) => assert!(msg.contains("-7"), "status carried: {msg}"),
        other => panic!("expected Unavailable, got {other:?}"),
    }
}

#[test]
fn ask_host_timeout_cleans_pending_entry() {
    let _bridge = lock_bridge();
    clear_sinks();
    LAST_REQUEST_ID.store(0, Ordering::SeqCst);

    // SAFETY: valid function pointer.
    unsafe {
        actr_cabi::actr_register_reverse_rpc_sink(Some(silent_reverse_sink), std::ptr::null_mut())
    };

    let rt = current_thread_rt();
    let err = rt
        .block_on(actr_cabi::ask_host("ask.Host/Silent", Bytes::new(), 50))
        .expect_err("host never answers");
    assert!(matches!(err, ActrError::TimedOut), "got {err:?}");
    assert_eq!(
        actr_cabi::pending_ask_count(),
        0,
        "timeout must drop the entry"
    );

    // A late response for this exact timed-out request is rejected.
    let request_id = LAST_REQUEST_ID.load(Ordering::SeqCst);
    assert_ne!(request_id, 0, "sink observed the request id");
    // SAFETY: this exact id timed out above.
    let rc =
        unsafe { actr_cabi::actr_host_respond_reverse_rpc(request_id, 0, std::ptr::null(), 0) };
    assert_eq!(rc, actr_cabi::REQUEST_ID_UNKNOWN);
}

#[test]
fn dropping_ask_host_cleans_pending_entry() {
    let _bridge = lock_bridge();
    clear_sinks();
    LAST_REQUEST_ID.store(0, Ordering::SeqCst);

    // SAFETY: valid callback with no user data.
    assert_eq!(
        unsafe {
            actr_cabi::actr_register_reverse_rpc_sink(
                Some(silent_reverse_sink),
                std::ptr::null_mut(),
            )
        },
        0
    );

    let runtime = current_thread_rt();
    runtime.block_on(async {
        let task = tokio::spawn(actr_cabi::ask_host(
            "ask.Host/Cancelled",
            Bytes::new(),
            60_000,
        ));
        tokio::time::timeout(Duration::from_secs(1), async {
            while LAST_REQUEST_ID.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("ask_host did not reach the reverse sink");
        task.abort();
        let _ = task.await;
    });
    assert_eq!(actr_cabi::pending_ask_count(), 0);
}

#[test]
fn ask_host_without_sink_is_unavailable() {
    let _bridge = lock_bridge();
    clear_sinks();

    let rt = current_thread_rt();
    let err = rt
        .block_on(actr_cabi::ask_host("ask.Host/None", Bytes::new(), 1_000))
        .expect_err("no sink registered");
    assert!(matches!(err, ActrError::Unavailable(_)), "got {err:?}");
}

#[test]
fn ask_host_rejects_non_positive_timeout() {
    let _bridge = lock_bridge();
    clear_sinks();

    let rt = current_thread_rt();
    let err = rt
        .block_on(actr_cabi::ask_host("ask.Host/Bad", Bytes::new(), 0))
        .expect_err("invalid timeout");
    assert!(matches!(err, ActrError::InvalidArgument(_)), "got {err:?}");
}

// ── respond input validation ───────────────────────────────────────────────

#[test]
fn respond_rejects_null_payload_with_nonzero_length() {
    // SAFETY: null pointer with non-zero length is the documented -10 path;
    // validation happens before any lookup.
    let rc = unsafe { actr_cabi::actr_host_respond_reverse_rpc(1, 0, std::ptr::null(), 5) };
    assert_eq!(rc, actr_cabi::NULL_POINTER);
}
