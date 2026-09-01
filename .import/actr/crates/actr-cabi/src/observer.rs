//! Host-side observer bridging actr hook events to C callbacks.
//!
//! `CabiObserver` implements `actr_framework::Workload` with a no-op
//! dispatcher (its `dispatch` is never called — observers only exist for their
//! hook overrides). The C side passes an `ObserverCallbacks` table via
//! `actr_node_with_observer`; when a hook fires, the matching `extern "C" fn`
//! is invoked with `user_data`.
//!
//! MVP scope: signaling + error hooks. WebRTC / credential / mailbox hooks
//! inherit the `Workload` trait defaults (tracing-only) and can be added
//! later without breaking the C ABI.

use std::ffi::c_void;

use actr_framework::{Context, MessageDispatcher, Workload};
use actr_protocol::{ActorResult, RpcEnvelope};
use async_trait::async_trait;
use bytes::Bytes;

/// C callback table. `user_data` is passed verbatim to every callback.
///
/// `#[repr(C)]` so the layout is stable across the FFI boundary.
/// The pointed-to context and callback functions must remain valid until the
/// owning node/`ActrRef` is released. Callbacks run synchronously on runtime
/// worker threads, may re-enter actr-cabi (no handle-registry lock is held),
/// and must not unwind across the C ABI boundary.
#[repr(C)]
pub struct ObserverCallbacks {
    pub user_data: *mut c_void,
    pub on_signaling_connecting: Option<extern "C" fn(*mut c_void)>,
    pub on_signaling_connected: Option<extern "C" fn(*mut c_void)>,
    pub on_signaling_disconnected: Option<extern "C" fn(*mut c_void)>,
    pub on_error: Option<extern "C" fn(*const u8, usize, *mut c_void)>,
}

/// Rust-side observer wrapping the C callback table.
pub struct CabiObserver {
    pub callbacks: ObserverCallbacks,
}

// SAFETY: constructing `CabiObserver` is confined to the unsafe C entry point,
// whose contract requires `user_data` to remain valid and safe for concurrent
// runtime-thread access. Callback fields are immutable `extern "C"` function
// pointers, which are themselves Send + Sync.
unsafe impl Send for CabiObserver {}
unsafe impl Sync for CabiObserver {}

/// Dispatcher that is never invoked — observers only carry hooks.
pub struct NoopDispatcher;

#[async_trait]
impl MessageDispatcher for NoopDispatcher {
    type Workload = CabiObserver;
    async fn dispatch<C: Context>(_: &Self::Workload, _: RpcEnvelope, _: &C) -> ActorResult<Bytes> {
        Err(actr_protocol::ActrError::NotImplemented(
            "CabiObserver has no dispatch; it is hook-only".into(),
        ))
    }
}

#[async_trait]
impl Workload for CabiObserver {
    type Dispatcher = NoopDispatcher;

    async fn on_signaling_connecting<C: Context>(&self, _: Option<&C>) {
        if let Some(cb) = self.callbacks.on_signaling_connecting {
            cb(self.callbacks.user_data);
        }
    }

    async fn on_signaling_connected<C: Context>(&self, _: Option<&C>) {
        if let Some(cb) = self.callbacks.on_signaling_connected {
            cb(self.callbacks.user_data);
        }
    }

    async fn on_signaling_disconnected<C: Context>(&self, _: &C) {
        if let Some(cb) = self.callbacks.on_signaling_disconnected {
            cb(self.callbacks.user_data);
        }
    }

    async fn on_error<C: Context>(
        &self,
        _: &C,
        event: &actr_framework::ErrorEvent,
    ) -> ActorResult<()> {
        if let Some(cb) = self.callbacks.on_error {
            let msg = format!("{event:?}");
            let (ptr, len) = crate::error::alloc_bytes(msg.as_bytes());
            cb(ptr, len, self.callbacks.user_data);
            crate::error::free_bytes(ptr, len);
        }
        Ok(())
    }
}
