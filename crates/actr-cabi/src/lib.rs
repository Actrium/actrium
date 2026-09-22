//! actr-cabi — C ABI primitive layer for cross-platform actr consumers.
//!
//! Exposes the actr node typestate chain (`from_config → link → register →
//! start → ActrRef::call`) as `extern "C"` functions with a callback-based
//! async model, so Swift/C# hosts that cannot use UniFFI can drive a
//! Rust-written workload in-process. See `actr-cabi.h` for the C surface.

mod error;
mod handle;
mod host_sink;
mod observer;
mod registry;
mod runtime;

pub use inventory;
pub use registry::{BundleEntry, LinkFn, WorkloadEntry, bundle_count, bundle_id, find_workload};
pub use runtime::{init_runtime, shutdown_runtime};
// `export_workload!` and `export_bundle!` are `#[macro_export]` → available at
// crate root as `actr_cabi::export_workload!` / `actr_cabi::export_bundle!`,
// not under `registry::`.

use std::ffi::{c_char, c_void};
use std::path::Path;

use actr_framework::{Context, Dest};
use actr_protocol::{ActrType, PayloadType};
use bytes::Bytes;

pub use crate::error::{
    DELIVERY_UNCERTAIN, HANDLE_INVALID, INVALID_UTF8, NULL_POINTER, PANIC, REQUEST_ID_UNKNOWN,
    RT_INIT_FAILED, RT_NOT_INITIALIZED, SUCCESS, WORKLOAD_NOT_FOUND,
};
use crate::error::{alloc_bytes, alloc_cstring, free_bytes, free_cstring};
use crate::handle::{
    box_actr_ref, box_attached, box_init, box_registered, box_workload, clone_actr_ref,
    drop_actr_ref, drop_handle, take_attached, take_init, take_link_inputs, take_registered,
};
pub use crate::host_sink::{
    ACTR_CHUNK_FLAG_HAS_TIMESTAMP, ReverseRpcCallback, StreamMetadataEntry, StreamSinkCallback,
    ask_host, forward_chunk_to_host, pending_ask_count,
};
use crate::observer::{CabiObserver, ObserverCallbacks};

/// Generic framework/runtime error not covered by a specific code.
const GENERIC_ERROR: i32 = -50;

fn actr_ref_call_error_status(error: &actr_protocol::ActrError) -> i32 {
    match error {
        // A workload can return TimedOut after executing an operation. The ABI
        // has no provenance distinguishing that reply from pre-send expiry.
        actr_protocol::ActrError::TimedOut | actr_protocol::ActrError::DeliveryUncertain { .. } => {
            DELIVERY_UNCERTAIN
        }
        _ => GENERIC_ERROR,
    }
}

// ── C callback typedefs ─────────────────────────────────────────────────────

/// Completion callbacks run synchronously on the calling/runtime thread. A
/// callback may re-enter actr-cabi, but it must not unwind across the C ABI
/// boundary and must copy borrowed buffers before returning.
pub type ActrVoidCallback =
    extern "C" fn(status: i32, error_ptr: *const c_char, error_len: usize, user_data: *mut c_void);

pub type ActrHandleCallback = extern "C" fn(
    status: i32,
    result_handle: *mut c_void,
    error_ptr: *const c_char,
    error_len: usize,
    user_data: *mut c_void,
);

pub type ActrCallCallback = extern "C" fn(
    status: i32,
    resp_ptr: *const u8,
    resp_len: usize,
    error_ptr: *const c_char,
    error_len: usize,
    user_data: *mut c_void,
);

pub type ActrDiscoverCallback = extern "C" fn(
    status: i32,
    actor_id_ptr: *const c_char,
    actor_id_len: usize,
    error_ptr: *const c_char,
    error_len: usize,
    user_data: *mut c_void,
);

// ── helpers ─────────────────────────────────────────────────────────────────

/// SAFETY: `ptr` must be valid for `len` bytes or null.
unsafe fn decode_str(ptr: *const c_char, len: usize) -> Result<String, i32> {
    if ptr.is_null() {
        return Err(NULL_POINTER);
    }
    // SAFETY: the caller guarantees that a non-null `ptr` is readable for
    // `len` bytes. `c_char` and `u8` have the same size and alignment.
    let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), len) };
    std::str::from_utf8(bytes)
        .map(String::from)
        .map_err(|_| INVALID_UTF8)
}

fn catch_unwind_i32(f: impl FnOnce() -> i32) -> i32 {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(code) => code,
        Err(_) => PANIC,
    }
}

fn catch_unwind_ptr(f: impl FnOnce() -> *mut c_void) -> *mut c_void {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(ptr) => ptr,
        Err(_) => {
            tracing::error!("panic at pointer-returning actr-cabi boundary");
            std::ptr::null_mut()
        }
    }
}

fn catch_unwind_void(f: impl FnOnce()) {
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_err() {
        tracing::error!("panic at void actr-cabi boundary");
    }
}

fn input_error(code: i32, field: &str) -> String {
    match code {
        NULL_POINTER => format!("{field} pointer is null"),
        INVALID_UTF8 => format!("{field} is not valid UTF-8"),
        _ => format!("invalid {field}"),
    }
}

/// Fire an error variant of `ActrHandleCallback` with a one-shot NUL buffer.
fn emit_handle_error<E: std::fmt::Display>(
    code: i32,
    err: E,
    callback: ActrHandleCallback,
    user_data: *mut c_void,
) {
    let msg = err.to_string();
    let msg_len = msg.len();
    let ptr = alloc_cstring(&msg);
    let reported_len = if ptr.is_null() { 0 } else { msg_len };
    callback(
        code,
        std::ptr::null_mut(),
        ptr as *const c_char,
        reported_len,
        user_data,
    );
    if !ptr.is_null() {
        free_cstring(ptr, msg_len + 1);
    }
}

fn emit_call_error<E: std::fmt::Display>(
    code: i32,
    err: E,
    callback: ActrCallCallback,
    user_data: *mut c_void,
) {
    let msg = err.to_string();
    let msg_len = msg.len();
    let ptr = alloc_cstring(&msg);
    let reported_len = if ptr.is_null() { 0 } else { msg_len };
    callback(
        code,
        std::ptr::null(),
        0,
        ptr as *const c_char,
        reported_len,
        user_data,
    );
    if !ptr.is_null() {
        free_cstring(ptr, msg_len + 1);
    }
}

fn emit_discover_error<E: std::fmt::Display>(
    code: i32,
    err: E,
    callback: ActrDiscoverCallback,
    user_data: *mut c_void,
) {
    let msg = err.to_string();
    let msg_len = msg.len();
    let ptr = alloc_cstring(&msg);
    let reported_len = if ptr.is_null() { 0 } else { msg_len };
    callback(
        code,
        std::ptr::null(),
        0,
        ptr as *const c_char,
        reported_len,
        user_data,
    );
    if !ptr.is_null() {
        free_cstring(ptr, msg_len + 1);
    }
}

fn emit_void_error<E: std::fmt::Display>(
    code: i32,
    err: E,
    callback: ActrVoidCallback,
    user_data: *mut c_void,
) {
    let msg = err.to_string();
    let msg_len = msg.len();
    let ptr = alloc_cstring(&msg);
    let reported_len = if ptr.is_null() { 0 } else { msg_len };
    callback(code, ptr as *const c_char, reported_len, user_data);
    if !ptr.is_null() {
        free_cstring(ptr, msg_len + 1);
    }
}

fn with_handle_callback(
    callback: Option<ActrHandleCallback>,
    user_data: *mut c_void,
    f: impl FnOnce(ActrHandleCallback) -> i32,
) -> i32 {
    let Some(callback) = callback else {
        return NULL_POINTER;
    };
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(callback))) {
        Ok(code) => code,
        Err(_) => {
            emit_handle_error(PANIC, "panic at actr-cabi boundary", callback, user_data);
            PANIC
        }
    }
}

fn with_call_callback(
    callback: Option<ActrCallCallback>,
    user_data: *mut c_void,
    f: impl FnOnce(ActrCallCallback) -> i32,
) -> i32 {
    let Some(callback) = callback else {
        return NULL_POINTER;
    };
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(callback))) {
        Ok(code) => code,
        Err(_) => {
            emit_call_error(PANIC, "panic at actr-cabi boundary", callback, user_data);
            PANIC
        }
    }
}

fn with_discover_callback(
    callback: Option<ActrDiscoverCallback>,
    user_data: *mut c_void,
    f: impl FnOnce(ActrDiscoverCallback) -> i32,
) -> i32 {
    let Some(callback) = callback else {
        return NULL_POINTER;
    };
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(callback))) {
        Ok(code) => code,
        Err(_) => {
            emit_discover_error(PANIC, "panic at actr-cabi boundary", callback, user_data);
            PANIC
        }
    }
}

fn with_void_callback(
    callback: Option<ActrVoidCallback>,
    user_data: *mut c_void,
    f: impl FnOnce(ActrVoidCallback) -> i32,
) -> i32 {
    let Some(callback) = callback else {
        return NULL_POINTER;
    };
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(callback))) {
        Ok(code) => code,
        Err(_) => {
            emit_void_error(PANIC, "panic at actr-cabi boundary", callback, user_data);
            PANIC
        }
    }
}

// ── runtime ─────────────────────────────────────────────────────────────────

/// Initialize the process-global Tokio runtime.
///
/// # Safety
///
/// Must be called once before any async `extern "C"` operation. Safe to call
/// multiple times — only the first call installs the runtime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn actr_init_runtime() -> i32 {
    catch_unwind_i32(runtime::init_runtime)
}

/// Shut down the process-global runtime (currently a no-op MVP).
///
/// # Safety
///
/// The runtime lives until process exit; calling this is always safe.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn actr_shutdown_runtime() -> i32 {
    catch_unwind_i32(runtime::shutdown_runtime)
}

// ── native introspection ────────────────────────────────────────────────────
//
// Static, infallible queries a host calls before driving the typestate chain
// to confirm the loaded `actr_cabi` matches the managed binding's expectations
// (ABI version + actr version) and to surface which workload bundle was
// linked. Unlike callback buffers, the returned pointers point into static
// memory and must NOT be freed.

/// The actr-cabi ABI version. Bumped only on incompatible changes to the C
/// surface (callback signatures, handle semantics, memory ownership). Hosts
/// compare this against their compiled-in expectation before any other call.
pub const ABI_VERSION: u32 = 1;

/// The actr workspace version this `actr_cabi` was built from
/// (`CARGO_PKG_VERSION`). Hosts compare this against the managed package
/// version to detect a stale or mismatched native library.
pub const ACTR_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Return the ABI version. Infallible — no allocation, no decode.
///
/// SAFETY: panic-free by construction (reads a `const`). Unlike the callback
/// extern fns above, no `catch_unwind` is needed; if a future edit adds a
/// panic-capable operation (tracing, allocation, formatting), wrap it.
#[unsafe(no_mangle)]
pub extern "C" fn actr_cabi_abi_version() -> u32 {
    ABI_VERSION
}

/// Return a pointer to the static actr version string.
///
/// Writes the byte length to `out_len` when non-null. The returned pointer
/// points into static memory and is valid for the process lifetime — the
/// caller must NOT free it (unlike `alloc_cstring` callback buffers).
///
/// SAFETY: panic-free by construction (`&'static str` pointer + optional raw
/// write to a caller-provided `usize`). See `actr_cabi_abi_version` for why
/// no `catch_unwind` is used.
///
/// # Safety
///
/// `out_len` may be null; if non-null it must point to a writable `usize`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn actr_cabi_actr_version(out_len: *mut usize) -> *const c_char {
    if !out_len.is_null() {
        // SAFETY: the caller guarantees a non-null `out_len` is writable.
        unsafe { *out_len = ACTR_VERSION.len() };
    }
    ACTR_VERSION.as_ptr().cast::<c_char>()
}

/// Return a pointer to the static bundle ID registered by the workload
/// cdylib via `export_bundle!`, or null if none was registered. When null,
/// `out_len` (if non-null) is set to 0 so the host never reads stale bytes.
///
/// The returned pointer is static and must NOT be freed.
///
/// SAFETY: panic-free by construction (`inventory::iter` walks a static
/// `AtomicPtr` list — no allocation, no mutex). See `actr_cabi_abi_version`
/// for why no `catch_unwind` is used.
///
/// # Safety
///
/// `out_len` may be null; if non-null it must point to a writable `usize`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn actr_cabi_bundle_id(out_len: *mut usize) -> *const c_char {
    match bundle_id() {
        Some(id) => {
            if !out_len.is_null() {
                // SAFETY: the caller guarantees a non-null `out_len` is writable.
                unsafe { *out_len = id.len() };
            }
            id.as_ptr().cast::<c_char>()
        }
        None => {
            if !out_len.is_null() {
                // SAFETY: the caller guarantees a non-null `out_len` is writable.
                unsafe { *out_len = 0 };
            }
            std::ptr::null()
        }
    }
}

/// Return the number of distinct bundle IDs registered. Should be 0 or 1;
/// a value greater than 1 means multiple workload cdylibs were linked into
/// the process, violating the single-provider contract.
///
/// SAFETY: panic-free by construction (`inventory::iter().count()` on a
/// static list). See `actr_cabi_abi_version` for why no `catch_unwind` is used.
#[unsafe(no_mangle)]
pub extern "C" fn actr_cabi_bundle_count() -> u32 {
    bundle_count() as u32
}

// ── node: from_config ───────────────────────────────────────────────────────

/// Create a `Node<Init>` from a TOML config file path. Async — result is
/// delivered via `callback`.
///
/// # Safety
///
/// `path_ptr` must point to `path_len` valid UTF-8 bytes (or be null with
/// `path_len == 0`). `callback` must be a valid function pointer.
/// `user_data` is passed verbatim and must be safe to send across threads.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn actr_node_from_config(
    path_ptr: *const c_char,
    path_len: usize,
    callback: Option<ActrHandleCallback>,
    user_data: *mut c_void,
) -> i32 {
    with_handle_callback(callback, user_data, |callback| {
        // SAFETY: the function's contract requires `path_ptr` to be readable
        // for `path_len` bytes whenever it is non-null.
        let path = match unsafe { decode_str(path_ptr, path_len) } {
            Ok(path) => path,
            Err(code) => {
                emit_handle_error(code, input_error(code, "path"), callback, user_data);
                return code;
            }
        };
        // `*mut c_void` is not `Send`; carry it as `usize` across the spawn.
        let ud = user_data as usize;
        runtime::spawn_guarded(
            async move {
                match actr_hyper::Node::from_config_file(Path::new(&path)).await {
                    Ok(node) => {
                        let h = box_init(node);
                        callback(SUCCESS, h, std::ptr::null(), 0, ud as *mut c_void);
                    }
                    Err(e) => emit_handle_error(GENERIC_ERROR, e, callback, ud as *mut c_void),
                }
            },
            move |code| {
                emit_handle_error(code, "actr-cabi task failed", callback, ud as *mut c_void)
            },
        )
    })
}

// ── node: with_actor_type (synchronous) ─────────────────────────────────────

/// Override a linked node's actor identity before attaching a workload.
///
/// Returns a replacement `ActrNodeInitHandle`, or null when the input actor
/// type is invalid or the supplied handle is stale or has the wrong state. The
/// actor type must be `publisher/program@version` and satisfies the protocol
/// identity validation rules.
///
/// # Safety
///
/// `init_handle` must be a valid `ActrNodeInitHandle` that has not been
/// consumed. `actor_type_ptr` must point to `actor_type_len` valid UTF-8 bytes
/// (or be null with `actor_type_len == 0`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn actr_node_with_actor_type(
    init_handle: *mut c_void,
    actor_type_ptr: *const c_char,
    actor_type_len: usize,
) -> *mut c_void {
    catch_unwind_ptr(|| {
        // SAFETY: the function's contract requires `actor_type_ptr` to be
        // readable for `actor_type_len` bytes whenever it is non-null.
        let actor_type_str = match unsafe { decode_str(actor_type_ptr, actor_type_len) } {
            Ok(actor_type) => actor_type,
            Err(_) => return std::ptr::null_mut(),
        };
        let actor_type = match ActrType::from_string_repr(&actor_type_str) {
            Ok(actor_type) => actor_type,
            Err(_) => return std::ptr::null_mut(),
        };
        let node = match take_init(init_handle) {
            Some(node) => node,
            None => return std::ptr::null_mut(),
        };
        box_init(node.with_actor_type(actor_type))
    })
}

// ── workload: create ────────────────────────────────────────────────────────

/// Look up a registered workload by name. Returns an opaque `WorkloadHandle`
/// or null if not found. Synchronous (registry lookup is link-time static).
///
/// # Safety
///
/// `name_ptr` must point to `name_len` valid UTF-8 bytes (or be null with
/// `name_len == 0`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn actr_workload_create(
    name_ptr: *const c_char,
    name_len: usize,
) -> *mut c_void {
    catch_unwind_ptr(|| {
        // SAFETY: the function's contract requires `name_ptr` to be readable
        // for `name_len` bytes whenever it is non-null.
        let name = match unsafe { decode_str(name_ptr, name_len) } {
            Ok(name) => name,
            Err(_) => return std::ptr::null_mut(),
        };
        match find_workload(&name) {
            Some(entry) => box_workload(entry.link_fn),
            None => std::ptr::null_mut(),
        }
    })
}

// ── node: link ──────────────────────────────────────────────────────────────

/// Link a `Node<Init>` with a workload, producing `Node<Attached>`. Async —
/// result is delivered via `callback`.
///
/// # Safety
///
/// `init_handle` must be a valid `ActrNodeInitHandle` returned by
/// `actr_node_from_config` and not yet consumed. `workload_handle` must be
/// a valid `WorkloadHandle` from `actr_workload_create`. Both handles are
/// consumed (invalidated) by this call. `callback` must be a valid function
/// pointer. `user_data` is passed verbatim and must be safe to send across
/// threads.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn actr_node_link(
    init_handle: *mut c_void,
    workload_handle: *mut c_void,
    callback: Option<ActrHandleCallback>,
    user_data: *mut c_void,
) -> i32 {
    with_handle_callback(callback, user_data, |callback| {
        let (node, link_fn) = match take_link_inputs(init_handle, workload_handle) {
            Some(inputs) => inputs,
            None => {
                emit_handle_error(
                    HANDLE_INVALID,
                    "invalid init or workload handle",
                    callback,
                    user_data,
                );
                return HANDLE_INVALID;
            }
        };
        let ud = user_data as usize;
        runtime::spawn_guarded(
            async move {
                match link_fn(node).await {
                    Ok(attached) => {
                        let h = box_attached(attached);
                        callback(SUCCESS, h, std::ptr::null(), 0, ud as *mut c_void);
                    }
                    Err(e) => emit_handle_error(GENERIC_ERROR, e, callback, ud as *mut c_void),
                }
            },
            move |code| {
                emit_handle_error(code, "actr-cabi task failed", callback, ud as *mut c_void)
            },
        )
    })
}

// ── node: with_observer (synchronous) ───────────────────────────────────────

/// Returns a new `ActrNodeAttachedHandle` with the observer chained, or null
/// on invalid input.
///
/// # Safety
///
/// `attached_handle` must be a valid `ActrNodeAttachedHandle` not yet
/// consumed. The `callbacks` struct must contain valid function pointers
/// (or null for unused slots). `callbacks.user_data` must be safe to send
/// across threads and must outlive all observer invocations.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn actr_node_with_observer(
    attached_handle: *mut c_void,
    callbacks: ObserverCallbacks,
) -> *mut c_void {
    catch_unwind_ptr(|| {
        let node = match take_attached(attached_handle) {
            Some(node) => node,
            None => return std::ptr::null_mut(),
        };
        let observer = CabiObserver { callbacks };
        let new_node = node.with_hook_observer(observer);
        box_attached(new_node)
    })
}

// ── node: register ──────────────────────────────────────────────────────────

/// Register the node with the Issuer endpoint. Async — result via `callback`.
///
/// # Safety
///
/// `attached_handle` must be a valid `ActrNodeAttachedHandle` not yet
/// consumed (it is consumed by this call). `issuer_ptr` may point to `issuer_len`
/// valid UTF-8 bytes, or be null to use the node's configured Issuer endpoint.
/// `callback` must be a valid function pointer. `user_data` is passed
/// verbatim and must be safe to send across threads.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn actr_node_register(
    attached_handle: *mut c_void,
    issuer_ptr: *const c_char,
    issuer_len: usize,
    callback: Option<ActrHandleCallback>,
    user_data: *mut c_void,
) -> i32 {
    with_handle_callback(callback, user_data, |callback| {
        let node = match take_attached(attached_handle) {
            Some(node) => node,
            None => {
                emit_handle_error(
                    HANDLE_INVALID,
                    "invalid attached handle",
                    callback,
                    user_data,
                );
                return HANDLE_INVALID;
            }
        };
        // If the caller passes no issuer_endpoint (nil), extract it from the node
        // runtime config — mirrors libactr's `attached.issuer_endpoint()` path.
        let issuer = if issuer_ptr.is_null() {
            if issuer_len != 0 {
                emit_handle_error(
                    NULL_POINTER,
                    "Issuer pointer is null with a non-zero length",
                    callback,
                    user_data,
                );
                return NULL_POINTER;
            }
            node.issuer_endpoint().to_string()
        } else {
            // SAFETY: the function's contract requires a non-null `issuer_ptr` to
            // be readable for `issuer_len` bytes.
            match unsafe { decode_str(issuer_ptr, issuer_len) } {
                Ok(issuer) => issuer,
                Err(code) => {
                    emit_handle_error(
                        code,
                        input_error(code, "Issuer endpoint"),
                        callback,
                        user_data,
                    );
                    return code;
                }
            }
        };
        let ud = user_data as usize;
        runtime::spawn_guarded(
            async move {
                match node.register(&issuer).await {
                    Ok(registered) => {
                        let h = box_registered(registered);
                        callback(SUCCESS, h, std::ptr::null(), 0, ud as *mut c_void);
                    }
                    Err(e) => emit_handle_error(GENERIC_ERROR, e, callback, ud as *mut c_void),
                }
            },
            move |code| {
                emit_handle_error(code, "actr-cabi task failed", callback, ud as *mut c_void)
            },
        )
    })
}

// ── node: start ──────────────────────────────────────────────────────────────

/// Start the registered node, producing an `ActrRefHandle`. Async — result
/// via `callback`.
///
/// # Safety
///
/// `registered_handle` must be a valid `ActrNodeRegisteredHandle` not yet
/// consumed (it is consumed by this call). `callback` must be a valid
/// function pointer. `user_data` is passed verbatim and must be safe to
/// send across threads.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn actr_node_start(
    registered_handle: *mut c_void,
    callback: Option<ActrHandleCallback>,
    user_data: *mut c_void,
) -> i32 {
    with_handle_callback(callback, user_data, |callback| {
        let node = match take_registered(registered_handle) {
            Some(node) => node,
            None => {
                emit_handle_error(
                    HANDLE_INVALID,
                    "invalid registered handle",
                    callback,
                    user_data,
                );
                return HANDLE_INVALID;
            }
        };
        let ud = user_data as usize;
        runtime::spawn_guarded(
            async move {
                match node.start().await {
                    Ok(actr_ref) => {
                        let h = box_actr_ref(actr_ref);
                        callback(SUCCESS, h, std::ptr::null(), 0, ud as *mut c_void);
                    }
                    Err(e) => emit_handle_error(GENERIC_ERROR, e, callback, ud as *mut c_void),
                }
            },
            move |code| {
                emit_handle_error(code, "actr-cabi task failed", callback, ud as *mut c_void)
            },
        )
    })
}

// ── ActrRef: call ────────────────────────────────────────────────────────────

/// Invoke an RPC on the actr workload. Async — response via `callback`.
///
/// # Safety
///
/// `ref_handle` must be a valid `ActrRefHandle` (it is cloned, not
/// consumed). `route_key_ptr` must point to `route_key_len` valid UTF-8
/// bytes. `payload_ptr` may be null with `payload_len == 0` for an empty
/// payload, or point to `payload_len` bytes. `callback` must be a valid
/// function pointer. `user_data` is passed verbatim and must be safe to
/// send across threads.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn actr_ref_call(
    ref_handle: *mut c_void,
    route_key_ptr: *const c_char,
    route_key_len: usize,
    payload_ptr: *const u8,
    payload_len: usize,
    timeout_ms: i64,
    callback: Option<ActrCallCallback>,
    user_data: *mut c_void,
) -> i32 {
    with_call_callback(callback, user_data, |callback| {
        let actr_ref = match clone_actr_ref(ref_handle) {
            Some(actr_ref) => actr_ref,
            None => {
                emit_call_error(
                    HANDLE_INVALID,
                    "invalid actr ref handle",
                    callback,
                    user_data,
                );
                return HANDLE_INVALID;
            }
        };
        // SAFETY: the function's contract requires `route_key_ptr` to be
        // readable for `route_key_len` bytes whenever it is non-null.
        let route_key = match unsafe { decode_str(route_key_ptr, route_key_len) } {
            Ok(route_key) => route_key,
            Err(code) => {
                emit_call_error(code, input_error(code, "route key"), callback, user_data);
                return code;
            }
        };
        let payload = if payload_ptr.is_null() {
            if payload_len != 0 {
                emit_call_error(
                    NULL_POINTER,
                    "payload pointer is null with a non-zero length",
                    callback,
                    user_data,
                );
                return NULL_POINTER;
            }
            Bytes::new()
        } else {
            // SAFETY: the function's contract requires a non-null
            // `payload_ptr` to be readable for `payload_len` bytes.
            let payload = unsafe { std::slice::from_raw_parts(payload_ptr, payload_len) };
            Bytes::copy_from_slice(payload)
        };
        let ud = user_data as usize;
        runtime::spawn_guarded(
            async move {
                let ctx = actr_ref.app_context().await;
                match ctx
                    .call_raw(
                        &Dest::Workload,
                        route_key,
                        PayloadType::RpcReliable,
                        payload,
                        timeout_ms,
                    )
                    .await
                {
                    Ok(resp) => {
                        let (ptr, len) = alloc_bytes(&resp);
                        if !resp.is_empty() && ptr.is_null() {
                            emit_call_error(
                                GENERIC_ERROR,
                                "failed to allocate response buffer",
                                callback,
                                ud as *mut c_void,
                            );
                        } else {
                            callback(SUCCESS, ptr, len, std::ptr::null(), 0, ud as *mut c_void);
                        }
                        if !ptr.is_null() {
                            free_bytes(ptr, len);
                        }
                    }
                    Err(e) => emit_call_error(
                        actr_ref_call_error_status(&e),
                        e,
                        callback,
                        ud as *mut c_void,
                    ),
                }
            },
            move |code| emit_call_error(code, "actr-cabi task failed", callback, ud as *mut c_void),
        )
    })
}

// ── ActrRef: discover ────────────────────────────────────────────────────────

/// Discover route candidates for a given actr type. Async — result via
/// `callback`.
///
/// # Safety
///
/// `ref_handle` must be a valid `ActrRefHandle` (it is cloned, not
/// consumed). `type_ptr` must point to `type_len` valid UTF-8 bytes
/// representing a valid `ActrType` string repr. `callback` must be a
/// valid function pointer. `user_data` is passed verbatim and must be safe
/// to send across threads.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn actr_ref_discover(
    ref_handle: *mut c_void,
    type_ptr: *const c_char,
    type_len: usize,
    callback: Option<ActrDiscoverCallback>,
    user_data: *mut c_void,
) -> i32 {
    with_discover_callback(callback, user_data, |callback| {
        let actr_ref = match clone_actr_ref(ref_handle) {
            Some(actr_ref) => actr_ref,
            None => {
                emit_discover_error(
                    HANDLE_INVALID,
                    "invalid actr ref handle",
                    callback,
                    user_data,
                );
                return HANDLE_INVALID;
            }
        };
        // SAFETY: the function's contract requires `type_ptr` to be readable
        // for `type_len` bytes whenever it is non-null.
        let type_str = match unsafe { decode_str(type_ptr, type_len) } {
            Ok(type_str) => type_str,
            Err(code) => {
                emit_discover_error(code, input_error(code, "actr type"), callback, user_data);
                return code;
            }
        };
        let ud = user_data as usize;
        runtime::spawn_guarded(
            async move {
                let actr_type = match ActrType::from_string_repr(&type_str) {
                    Ok(t) => t,
                    Err(e) => {
                        emit_discover_error(GENERIC_ERROR, e, callback, ud as *mut c_void);
                        return;
                    }
                };
                match actr_ref.discover_route_candidates(&actr_type, 1).await {
                    Ok(ids) => {
                        let Some(id) = ids.first() else {
                            emit_discover_error(
                                GENERIC_ERROR,
                                "no route candidates found",
                                callback,
                                ud as *mut c_void,
                            );
                            return;
                        };
                        let s = id.to_string_repr();
                        let msg_len = s.len();
                        let ptr = alloc_cstring(&s);
                        if ptr.is_null() {
                            emit_discover_error(
                                GENERIC_ERROR,
                                "failed to allocate actor ID buffer",
                                callback,
                                ud as *mut c_void,
                            );
                        } else {
                            callback(
                                SUCCESS,
                                ptr as *const c_char,
                                msg_len,
                                std::ptr::null(),
                                0,
                                ud as *mut c_void,
                            );
                        }
                        if !ptr.is_null() {
                            free_cstring(ptr, msg_len + 1);
                        }
                    }
                    Err(e) => emit_discover_error(GENERIC_ERROR, e, callback, ud as *mut c_void),
                }
            },
            move |code| {
                emit_discover_error(code, "actr-cabi task failed", callback, ud as *mut c_void)
            },
        )
    })
}

// ── Host bridge: stream sink + reverse RPC ──────────────────────────────────

/// Install (or clear, with a null sink) the process-global host stream-chunk
/// sink. `actr_stream_listen` forwards every inbound DataChunk of a listened
/// stream to this callback.
///
/// Callback contract (mirrors the observer hooks): it fires synchronously on
/// actr-cabi runtime worker threads, may re-enter actr-cabi, must not unwind,
/// and every buffer it receives is valid only for the duration of the call —
/// copy before returning. `chunk_flags` bit `0x01` indicates that
/// `timestamp_ms` is set. `sender_id` is the sending actor's
/// `to_string_repr()` form. Metadata entries are forwarded as borrowed key /
/// value pairs in the callback's `metadata_ptr` array.
///
/// # Safety
///
/// `sink` must be a valid function pointer (or null) and `user_data` must
/// be safe to send across threads. Replacing or clearing waits for in-flight
/// callbacks, so it is safe to release old `user_data` once this function
/// returns. Calling either sink registration entry point from a sink callback
/// returns `GENERIC_ERROR` to avoid self-deadlock.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn actr_register_stream_sink(
    sink: Option<host_sink::StreamSinkCallback>,
    user_data: *mut c_void,
) -> i32 {
    catch_unwind_i32(|| match host_sink::set_stream_sink(sink, user_data) {
        Ok(()) => SUCCESS,
        Err(code) => code,
    })
}

/// Install (or clear, with a null sink) the process-global reverse-RPC sink.
///
/// A Rust workload dispatcher calls `actr_cabi::ask_host(route, payload,
/// timeout_ms)` for an inbound request the embedding host should answer; the
/// host receives it through this callback and must eventually answer the
/// carried `request_id` via `actr_host_respond_reverse_rpc`. Same callback
/// contract as `actr_register_stream_sink` (synchronous, no unwinding,
/// borrowed buffers).
///
/// # Safety
///
/// Same lifetime and in-callback registration restrictions as
/// `actr_register_stream_sink`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn actr_register_reverse_rpc_sink(
    sink: Option<host_sink::ReverseRpcCallback>,
    user_data: *mut c_void,
) -> i32 {
    catch_unwind_i32(|| match host_sink::set_reverse_sink(sink, user_data) {
        Ok(()) => SUCCESS,
        Err(code) => code,
    })
}

/// Deliver the host's answer for a reverse-RPC request.
///
/// `status == 0` resolves the awaiting `ask_host` caller with `payload`;
/// any other status resolves it with an `Unavailable` error carrying the
/// status. Returns `REQUEST_ID_UNKNOWN` (-22) when `request_id` is
/// unknown, already answered, or past its `ask_host` timeout. Synchronous —
/// safe to call from any thread, including from inside the reverse-RPC sink
/// callback.
///
/// # Safety
///
/// `payload_ptr` may be null with `payload_len == 0` for an empty payload,
/// or point to `payload_len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn actr_host_respond_reverse_rpc(
    request_id: u64,
    status: i32,
    payload_ptr: *const u8,
    payload_len: usize,
) -> i32 {
    catch_unwind_i32(|| {
        let payload: &[u8] = if payload_ptr.is_null() {
            if payload_len != 0 {
                return NULL_POINTER;
            }
            &[]
        } else {
            // SAFETY: the function's contract requires a non-null
            // `payload_ptr` to be readable for `payload_len` bytes.
            unsafe { std::slice::from_raw_parts(payload_ptr, payload_len) }
        };
        match host_sink::host_respond(request_id, status, payload) {
            Ok(()) => SUCCESS,
            Err(code) => code,
        }
    })
}

/// Handle to one active stream listener task.
///
/// `done` flips to `true` once the listener has closed the stream and
/// unregistered it from the runtime, so `actr_stream_unlisten` can report
/// SUCCESS only after actual teardown (listen/unlisten/re-listen flows cannot
/// race the runtime's duplicate-subscription rejection).
struct StreamListener {
    cancel: tokio_util::sync::CancellationToken,
    done: tokio::sync::watch::Receiver<bool>,
}

type StreamListenerMap = std::collections::HashMap<(usize, String), std::sync::Arc<StreamListener>>;

/// Cancellation handles of active stream listeners, keyed by
/// `(ref_handle, stream_id)`. `actr_stream_listen` inserts one entry per
/// subscription; `actr_stream_unlisten` cancels it and waits for teardown.
fn stream_listeners() -> &'static std::sync::Mutex<StreamListenerMap> {
    static LISTENERS: std::sync::OnceLock<std::sync::Mutex<StreamListenerMap>> =
        std::sync::OnceLock::new();
    LISTENERS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

async fn stop_stream_listener(ref_key: usize, stream_id: String) {
    // Keep the entry discoverable until the listener itself finishes teardown.
    // Every concurrent unlisten must wait for the same completion signal.
    let listener = stream_listeners()
        .lock()
        .unwrap()
        .get(&(ref_key, stream_id))
        .cloned();
    if let Some(listener) = listener {
        listener.cancel.cancel();
        let mut done = listener.done.clone();
        while !*done.borrow() {
            if done.changed().await.is_err() {
                break;
            }
        }
    }
}

/// Subscribe the node to one inbound data stream and forward every chunk to
/// the registered host stream sink. Async — registration result via
/// `callback`. One listener per `stream_id`: a second `listen` for the same
/// id fails with an error through `callback` (duplicate subscriptions are
/// rejected by the runtime).
///
/// Chunks arriving before a sink is registered are dropped (traced), not
/// buffered. Only chunks whose `stream_id` exactly equals this argument reach
/// this listener; unregistered stream ids are never forwarded by this API.
///
/// # Safety
///
/// `ref_handle` must be a valid `ActrRefHandle` (it is cloned, not
/// consumed). `stream_id_ptr` must point to `stream_id_len` valid UTF-8
/// bytes. `callback` must be a valid function pointer. `user_data` is passed
/// verbatim and must be safe to send across threads.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn actr_stream_listen(
    ref_handle: *mut c_void,
    stream_id_ptr: *const c_char,
    stream_id_len: usize,
    callback: Option<ActrVoidCallback>,
    user_data: *mut c_void,
) -> i32 {
    with_void_callback(callback, user_data, |callback| {
        let actr_ref = match clone_actr_ref(ref_handle) {
            Some(actr_ref) => actr_ref,
            None => {
                emit_void_error(
                    HANDLE_INVALID,
                    "invalid actr ref handle",
                    callback,
                    user_data,
                );
                return HANDLE_INVALID;
            }
        };
        // SAFETY: the function's contract requires `stream_id_ptr` to be
        // readable for `stream_id_len` bytes whenever it is non-null.
        let stream_id = match unsafe { decode_str(stream_id_ptr, stream_id_len) } {
            Ok(stream_id) => stream_id,
            Err(code) => {
                emit_void_error(code, input_error(code, "stream id"), callback, user_data);
                return code;
            }
        };
        let ud = user_data as usize;
        let ref_key = ref_handle as usize;
        runtime::spawn_guarded(
            async move {
                use futures_util::StreamExt;

                let ctx = actr_ref.app_context().await;
                let mut stream = match ctx.data_chunks(stream_id.clone(), Default::default()).await
                {
                    Ok(stream) => stream,
                    Err(e) => {
                        emit_void_error(GENERIC_ERROR, e, callback, ud as *mut c_void);
                        return;
                    }
                };
                let (done_tx, done_rx) = tokio::sync::watch::channel(false);
                let entry = std::sync::Arc::new(StreamListener {
                    cancel: tokio_util::sync::CancellationToken::new(),
                    done: done_rx,
                });
                stream_listeners()
                    .lock()
                    .unwrap()
                    .insert((ref_key, stream_id.clone()), entry.clone());
                let cancel = entry.cancel.clone();
                callback(SUCCESS, std::ptr::null(), 0, ud as *mut c_void);
                // Forward until unlistened, the stream ends, or the node
                // shuts down (runtime teardown terminates the stream).
                loop {
                    tokio::select! {
                        biased;
                        _ = cancel.cancelled() => break,
                        item = stream.next() => match item {
                            Some(Ok(event)) => {
                                host_sink::forward_chunk_to_host(&event.chunk, &event.sender);
                            }
                            Some(Err(_)) | None => break,
                        },
                    }
                }
                stream.close().await;
                // Remove only our own entry: a re-listen during close may
                // already have installed a newer listener for this stream.
                let mut listeners = stream_listeners().lock().unwrap();
                if listeners
                    .get(&(ref_key, stream_id.clone()))
                    .is_some_and(|current| std::sync::Arc::ptr_eq(current, &entry))
                {
                    listeners.remove(&(ref_key, stream_id));
                }
                drop(listeners);
                let _ = done_tx.send(true);
            },
            move |code| emit_void_error(code, "actr-cabi task failed", callback, ud as *mut c_void),
        )
    })
}

/// Stop forwarding one stream. Async — result via `callback`. Unlistening a
/// stream that was never listened is an idempotent runtime no-op and completes
/// successfully through `callback`. Unlistening drops chunks buffered for the
/// listener without delivering them.
///
/// # Safety
///
/// Same contract as `actr_stream_listen`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn actr_stream_unlisten(
    ref_handle: *mut c_void,
    stream_id_ptr: *const c_char,
    stream_id_len: usize,
    callback: Option<ActrVoidCallback>,
    user_data: *mut c_void,
) -> i32 {
    with_void_callback(callback, user_data, |callback| {
        let actr_ref = match clone_actr_ref(ref_handle) {
            Some(actr_ref) => actr_ref,
            None => {
                emit_void_error(
                    HANDLE_INVALID,
                    "invalid actr ref handle",
                    callback,
                    user_data,
                );
                return HANDLE_INVALID;
            }
        };
        drop(actr_ref);
        // SAFETY: the function's contract requires `stream_id_ptr` to be
        // readable for `stream_id_len` bytes whenever it is non-null.
        let stream_id = match unsafe { decode_str(stream_id_ptr, stream_id_len) } {
            Ok(stream_id) => stream_id,
            Err(code) => {
                emit_void_error(code, input_error(code, "stream id"), callback, user_data);
                return code;
            }
        };
        let ud = user_data as usize;
        let ref_key = ref_handle as usize;
        runtime::spawn_guarded(
            async move {
                stop_stream_listener(ref_key, stream_id).await;
                callback(SUCCESS, std::ptr::null(), 0, ud as *mut c_void);
            },
            move |code| emit_void_error(code, "actr-cabi task failed", callback, ud as *mut c_void),
        )
    })
}

#[cfg(test)]
mod stream_listener_tests {
    use super::*;

    #[tokio::test]
    async fn concurrent_unlisten_waits_for_listener_teardown() {
        let key = (usize::MAX, "concurrent-unlisten".to_owned());
        let (done_tx, done_rx) = tokio::sync::watch::channel(false);
        let entry = std::sync::Arc::new(StreamListener {
            cancel: tokio_util::sync::CancellationToken::new(),
            done: done_rx,
        });
        stream_listeners()
            .lock()
            .unwrap()
            .insert(key.clone(), entry.clone());
        let mut first = Box::pin(stop_stream_listener(key.0, key.1.clone()));
        assert!(futures_util::poll!(first.as_mut()).is_pending());
        assert!(entry.cancel.is_cancelled());
        let mut second = Box::pin(stop_stream_listener(key.0, key.1.clone()));
        assert!(futures_util::poll!(second.as_mut()).is_pending());

        // The listener removes its entry only after closing the subscription.
        stream_listeners().lock().unwrap().remove(&key);
        done_tx.send(true).unwrap();
        first.await;
        second.await;
    }
}
// ── ActrRef: shutdown ────────────────────────────────────────────────────────

/// Shut down the actr node (synchronous). The handle remains valid for
/// `actr_release_actr_ref`.
///
/// # Safety
///
/// `ref_handle` must be a valid `ActrRefHandle` (it is cloned, not
/// consumed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn actr_ref_shutdown(ref_handle: *mut c_void) -> i32 {
    catch_unwind_i32(|| {
        let actr_ref = match clone_actr_ref(ref_handle) {
            Some(r) => r,
            None => return HANDLE_INVALID,
        };
        actr_ref.shutdown();
        drop(actr_ref);
        SUCCESS
    })
}

/// Wait asynchronously for the actr node to finish shutting down. Async —
/// completion via `callback`.
///
/// # Safety
///
/// `ref_handle` must be a valid `ActrRefHandle` (it is cloned, not
/// consumed). `callback` must be a valid function pointer. `user_data` is
/// passed verbatim and must be safe to send across threads.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn actr_ref_wait_shutdown(
    ref_handle: *mut c_void,
    callback: Option<ActrVoidCallback>,
    user_data: *mut c_void,
) -> i32 {
    with_void_callback(callback, user_data, |callback| {
        let actr_ref = match clone_actr_ref(ref_handle) {
            Some(actr_ref) => actr_ref,
            None => {
                emit_void_error(
                    HANDLE_INVALID,
                    "invalid actr ref handle",
                    callback,
                    user_data,
                );
                return HANDLE_INVALID;
            }
        };
        let ud = user_data as usize;
        runtime::spawn_guarded(
            async move {
                actr_ref.wait_for_shutdown().await;
                callback(SUCCESS, std::ptr::null(), 0, ud as *mut c_void);
            },
            move |code| emit_void_error(code, "actr-cabi task failed", callback, ud as *mut c_void),
        )
    })
}

// ── memory management ────────────────────────────────────────────────────────

/// Free a byte buffer allocated by actr-cabi.
///
/// # Safety
///
/// `ptr` must have been allocated by `alloc_bytes` (or a function returning
/// the same allocation scheme) with exactly `len` bytes, and not already
/// freed. Passing null/0 is a safe no-op.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn actr_free_bytes(ptr: *mut u8, len: usize) {
    catch_unwind_void(|| free_bytes(ptr, len));
}

/// Free a C string allocated by actr-cabi.
///
/// `len` is the content length in bytes WITHOUT the NUL terminator — the
/// same value the crate reported to the callback (e.g. `error_len`,
/// `actor_id_len`). It matches `alloc_cstring`, which allocates
/// `len + 1` bytes (content + implicit NUL); this function reconstructs
/// the full allocation as `len + 1` before deallocating.
///
/// # Safety
///
/// `ptr` must have been allocated by `alloc_cstring` with exactly `len`
/// content bytes, and not already freed. Passing null is a safe no-op.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn actr_free_string(ptr: *mut c_char, len: usize) {
    catch_unwind_void(|| {
        if ptr.is_null() {
            return;
        }
        let Some(allocation_len) = len.checked_add(1) else {
            return;
        };
        free_cstring(ptr.cast::<u8>(), allocation_len);
    });
}

/// Release any opaque handle and its correctly typed inner value.
///
/// # Safety
///
/// `handle` may be null, stale, already consumed, or already released; those
/// cases are safe no-ops. Non-null values are treated only as opaque integer
/// tokens and are never dereferenced.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn actr_release_handle(handle: *mut c_void) {
    catch_unwind_void(|| {
        drop_handle(handle);
    });
}

/// Release an `ActrRefHandle`.
///
/// # Safety
///
/// `handle` may be null, stale, already released, or the wrong handle type;
/// those cases are safe no-ops. In-flight operations own an `ActrRef` clone, so
/// releasing the token concurrently cannot make those operations dangle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn actr_release_actr_ref(handle: *mut c_void) {
    catch_unwind_void(|| {
        drop_actr_ref(handle);
    });
}

#[cfg(test)]
mod call_error_status_tests {
    use super::*;

    #[test]
    fn untracked_call_deadlines_have_uncertain_delivery_status() {
        assert_eq!(
            actr_ref_call_error_status(&actr_protocol::ActrError::TimedOut),
            DELIVERY_UNCERTAIN
        );
        assert_eq!(
            actr_ref_call_error_status(&actr_protocol::ActrError::DeliveryUncertain {
                operation_id: "op-1".into(),
                cause: "send was submitted".into(),
            }),
            DELIVERY_UNCERTAIN
        );
        assert_eq!(
            actr_ref_call_error_status(&actr_protocol::ActrError::Unavailable("offline".into())),
            GENERIC_ERROR
        );
    }
}
