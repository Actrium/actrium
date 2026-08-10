//! Global Tokio runtime for actr-cabi.
//!
//! Mirrors `core/framework/src/guest/dynclib/runtime.rs` but simplified: the
//! runtime is process-scoped (actr-cabi runs in the host process, not a
//! dlopen/dlclose image), so we cache the runtime's initialization result and
//! do not support
//! re-initialization. `shutdown_runtime` is a no-op MVP — the runtime lives
//! until process exit.

use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::OnceLock;

use futures_util::FutureExt;
use tokio::runtime::Runtime;

use crate::error::{PANIC, RT_INIT_FAILED, RT_NOT_INITIALIZED, SUCCESS};

static GLOBAL_RT: OnceLock<Result<Runtime, String>> = OnceLock::new();

/// Initialize the process-global multi-thread Tokio runtime.
///
/// Must be called once before any async `extern "C"` operation. Safe to call
/// multiple times — only the first call installs the runtime.
pub fn init_runtime() -> i32 {
    match GLOBAL_RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("actr-cabi")
            .build()
            .map_err(|error| error.to_string())
    }) {
        Ok(_) => SUCCESS,
        Err(error) => {
            tracing::error!(%error, "failed to initialize actr-cabi runtime");
            RT_INIT_FAILED
        }
    }
}

/// MVP no-op: the runtime lives until process exit.
///
/// A future revision may switch to `Mutex<Option<Runtime>>` (like the dynclib
/// runtime) to support teardown + re-init, but actr-cabi hosts (UI apps) do
/// not reload the library.
pub fn shutdown_runtime() -> i32 {
    SUCCESS
}

/// Spawn a future on the global runtime, guaranteeing the FFI caller's
/// completion callback always fires.
///
/// If the runtime is not initialized, `on_fail` is invoked synchronously with
/// `RT_NOT_INITIALIZED`. If the future panics, `on_fail` is invoked with
/// [`PANIC`](crate::error::PANIC). This prevents a consumer's
/// `TaskCompletionSource`/continuation from hanging forever when the spawned
/// task cannot deliver a result.
pub fn spawn_guarded<F>(future: F, on_fail: impl FnOnce(i32) + Send + 'static) -> i32
where
    F: Future<Output = ()> + Send + 'static,
{
    match GLOBAL_RT.get() {
        Some(Ok(rt)) => {
            rt.spawn(async move {
                if AssertUnwindSafe(future).catch_unwind().await.is_err() {
                    on_fail(PANIC);
                }
            });
            SUCCESS
        }
        None => {
            on_fail(RT_NOT_INITIALIZED);
            RT_NOT_INITIALIZED
        }
        Some(Err(_)) => {
            on_fail(RT_INIT_FAILED);
            RT_INIT_FAILED
        }
    }
}

/// Block the current thread on a future using the global runtime.
///
/// Returns `None` if the runtime is not initialized.
#[allow(dead_code)]
pub fn block_on<F: std::future::Future>(future: F) -> Option<F::Output> {
    GLOBAL_RT
        .get()
        .and_then(|result| result.as_ref().ok())
        .map(|rt| rt.block_on(future))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_is_idempotent() {
        let first = init_runtime();
        let second = init_runtime();
        assert_eq!(first, SUCCESS);
        assert_eq!(second, SUCCESS);
        assert!(spawn_guarded(async {}, |_| {}).await_success());
    }

    // Helper trait to make the spawn assertion readable.
    trait AwaitSuccess {
        fn await_success(&self) -> bool;
    }
    impl AwaitSuccess for i32 {
        fn await_success(&self) -> bool {
            *self == SUCCESS
        }
    }
}
