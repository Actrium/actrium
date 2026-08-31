//! WASM workload execution engine (feature = "wasm-engine").
//!
//! Backed by the wasmtime 46 Component Model + wit-bindgen against the
//! `actr:workload@0.6.0` async world; see `core/framework/wit-v2/actr-workload.wit`
//! for the contract. The 0.1.0 synchronous world is retired.

pub(crate) mod component_bindings_v2;
mod error;
mod host;
mod host_v2;
mod runtime_limits;

pub use error::WasmError;
pub use host::WasmHost;
pub(crate) use host_v2::WasmWorkloadV2;
#[cfg(feature = "test-utils")]
pub(crate) use runtime_limits::{
    QuotaPermit as TestWasmQuotaPermit, acquire_instantiate as test_acquire_instantiate_quota,
    instantiate_quota_denial_snapshot as test_instantiate_quota_denial_snapshot,
    quota_release_snapshot as test_quota_release_snapshot,
    wait_for_instantiate_quota_denial_after as test_wait_for_instantiate_quota_denial_after,
    wait_for_quota_release_after as test_wait_for_quota_release_after,
};
pub use runtime_limits::{WasmRuntimeStats, wasm_runtime_stats};
