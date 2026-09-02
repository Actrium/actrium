//! E2E — a **generated** Python V2 guest through the resident
//! `Store::run_concurrent` path (#430).
//!
//! The M5 suite (`wasm_open_concurrency.rs`) proves the conflict-key
//! scheduler + interleaved runner properties against a purpose-built Rust
//! fixture. This suite proves the SAME properties against a guest that went
//! through the real pipeline a user's Python workload goes through:
//! `actr gen -l python` → typed dispatcher → `actr-workload componentize` →
//! wasm32-wasip2 Component Model module (world `actr-workload-guest-v2`).
//!
//! Probes mirror the Rust fixture contract:
//!
//! * `concurrency.ProbeService.InflightProbe` — per-instance in-flight
//!   counter, parked at the host-gated `test/gate` crossing, replies with the
//!   peak concurrency this instance observed (`ProbeResponse.max_seen`).
//! * `concurrency.ProbeService.BoomAfterAwait` — parks at `test/double_impl`
//!   and then raises, fault-injecting a guest trap with siblings in flight.
//!
//! The counters are module-level globals in `workload.py`: in a Python wasm
//! component they live in the linear memory, so a rebuilt instance starts
//! from zero — the same basis-agnostic semantics as the Rust fixture's
//! per-instance fields.
//!
//! Slow e2e: building the component needs python3 + the `actr` CLI. Run with:
//!
//! ```text
//! ACTR_PYTHON_CONCURRENCY_E2E=1 \
//! cargo test -p actr-hyper --features wasm-engine,test-utils \
//!     --test e2e_python_generated_concurrency -- --ignored --test-threads=1
//! ```

#![cfg(all(feature = "wasm-engine", feature = "test-utils"))]

use std::process::Command;
use std::sync::{Arc, OnceLock};

use tokio::sync::Mutex;

use actr_framework::Bytes;
use actr_hyper::config::WasmRuntimeLimits;
use actr_hyper::test_support::concurrency::{
    await_dispatch, caller, gate_bridge, spawn_dispatch, wait_entered,
};
use actr_hyper::test_support::instantiate_wasm_workload;
use actr_hyper::wasm::WasmHost;
use actr_hyper::{ConflictKeySpec, KeySource};

/// Route constants for the generated probe service
/// (`{package}.{service}.{method}`, see `proto_model.rs`).
const PROBE: &str = "concurrency.ProbeService.InflightProbe";
const BOOM: &str = "concurrency.ProbeService.BoomAfterAwait";
const OUT_WASM: &str = "dist/concurrency-probe-python-0.2.0-wasm32-wasip2.wasm";

fn workload_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("actr-hyper lives under the workspace root")
        .parent()
        .expect("workspace root")
        .join("examples/actr/python/concurrency-probe-workload")
}

fn python_e2e_or_skip() -> bool {
    if std::env::var("ACTR_PYTHON_CONCURRENCY_E2E").ok().as_deref() != Some("1") {
        eprintln!("skipping: set ACTR_PYTHON_CONCURRENCY_E2E=1 to run this e2e");
        return false;
    }
    true
}

/// Build the component once per test binary (venv + codegen + componentize).
fn build_component() -> std::path::PathBuf {
    let dir = workload_dir();
    let out = Command::new("bash")
        .arg("build.sh")
        .current_dir(&dir)
        .output()
        .expect("failed to spawn build.sh");
    assert!(
        out.status.success(),
        "build.sh failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let component = dir.join(OUT_WASM);
    assert!(component.exists(), "component missing at {component:?}");
    component
}

fn component_path() -> &'static std::path::Path {
    static PATH: OnceLock<std::path::PathBuf> = OnceLock::new();
    PATH.get_or_init(build_component)
}

fn python_host() -> &'static WasmHost {
    static HOST: OnceLock<WasmHost> = OnceLock::new();
    HOST.get_or_init(|| {
        let bytes = std::fs::read(component_path()).expect("read python component");
        // CPython eagerly initializes its interpreter + heap when the component
        // is instantiated (same situation as the Go runtime in
        // `native_workload_smoke.rs`). Keep the production default conservative
        // while granting this known runtime enough bounded startup/entry fuel.
        let limits = WasmRuntimeLimits {
            fuel_per_invocation: 100_000_000_000,
            ..Default::default()
        };
        WasmHost::compile_with_limits(&bytes, &limits).expect("compile python component")
    })
}

/// Declares every generated probe route as keyed by the sender, so a test
/// controls concurrency purely through which `caller` it passes.
fn python_probe_spec() -> ConflictKeySpec {
    ConflictKeySpec::builder()
        .method(PROBE, KeySource::Sender)
        .method(BOOM, KeySource::Sender)
        .build()
        .expect("build python conflict-key spec")
}

/// Decode `ProbeResponse { uint32 max_seen = 1; }` — field tag `0x08`
/// followed by the varint value.
fn decode_max_seen(reply: &Bytes) -> u32 {
    assert!(
        reply.len() >= 2 && reply[0] == 0x08,
        "unexpected ProbeResponse encoding: {reply:02x?}"
    );
    let mut value: u64 = 0;
    let mut shift = 0;
    for &byte in &reply[1..] {
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    u32::try_from(value).expect("max_seen fits u32")
}

/// The runtime quota counters are process-wide; serialize this binary's
/// scenarios so unrelated quota-release wakeups never disturb a rebuild
/// assertion (same discipline as the M5 suite).
fn quota_isolation() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

// ── Facet 1 — distinct keys truly interleave (MAX_SEEN >= 2) ─────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore] // Slow e2e: builds the Python component (venv + codegen + componentize).
async fn generated_python_interleave_distinct_keys_reaches_max_seen_2() {
    if !python_e2e_or_skip() {
        return;
    }
    let _quota_isolation = quota_isolation().lock().await;
    let wl = instantiate_wasm_workload(python_host())
        .await
        .expect("instantiate python component");
    let dispatcher = Arc::new(wl.into_concurrent_dispatcher(python_probe_spec(), 8, 256, None));
    let (bridge, mut ctl) = gate_bridge();

    // Two distinct callers -> distinct conflict keys -> eligible to run at once.
    let d1 = spawn_dispatch(&dispatcher, PROBE, vec![], caller(1), &bridge);
    let d2 = spawn_dispatch(&dispatcher, PROBE, vec![], caller(2), &bridge);

    // Both must be suspended inside the ONE instance before either is released.
    wait_entered(&mut ctl.gate_entered, 2).await;
    ctl.gate_release.add_permits(2);

    let m1 = decode_max_seen(&await_dispatch(d1, "distinct d1").await.expect("d1 ok"));
    let m2 = decode_max_seen(&await_dispatch(d2, "distinct d2").await.expect("d2 ok"));

    assert!(
        m1.max(m2) >= 2,
        "distinct-key dispatches must interleave inside one generated python \
         instance (MAX_SEEN>=2), got {m1} and {m2}"
    );
    dispatcher.shutdown().await;
}

// ── Facet 2 — same key stays strictly serial (MAX_SEEN == 1) ─────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore] // Slow e2e: builds the Python component (venv + codegen + componentize).
async fn generated_python_same_key_stays_serial_max_seen_1() {
    if !python_e2e_or_skip() {
        return;
    }
    let _quota_isolation = quota_isolation().lock().await;
    let wl = instantiate_wasm_workload(python_host())
        .await
        .expect("instantiate python component");
    let dispatcher = Arc::new(wl.into_concurrent_dispatcher(python_probe_spec(), 8, 256, None));
    let (bridge, mut ctl) = gate_bridge();

    // SAME caller -> same conflict key -> the scheduler must serialize them.
    let a = spawn_dispatch(&dispatcher, PROBE, vec![], caller(7), &bridge);
    let b = spawn_dispatch(&dispatcher, PROBE, vec![], caller(7), &bridge);

    // Release strictly by the guest's real entry order (never by spawn order):
    // the scheduler is free to admit either submission first, and the second
    // can only enter after the first reply frees the key.
    for _ in 0..2 {
        wait_entered(&mut ctl.gate_entered, 1).await;
        ctl.gate_release.add_permits(1);
    }
    let first = decode_max_seen(&await_dispatch(a, "same-key A").await.expect("A ok"));
    let second = decode_max_seen(&await_dispatch(b, "same-key B").await.expect("B ok"));

    // Never overlapped => the shared in-flight counter never exceeded 1.
    assert_eq!(first, 1, "same-key dispatch A must never overlap a sibling");
    assert_eq!(
        second, 1,
        "same-key dispatch B must never overlap a sibling"
    );
    dispatcher.shutdown().await;
}

// ── Facet 3 — concurrency is capped at the scheduler budget C ────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore] // Slow e2e: builds the Python component (venv + codegen + componentize).
async fn generated_python_concurrency_capped_at_budget() {
    if !python_e2e_or_skip() {
        return;
    }
    let _quota_isolation = quota_isolation().lock().await;
    const C: usize = 3;
    let wl = instantiate_wasm_workload(python_host())
        .await
        .expect("instantiate python component");
    let dispatcher = Arc::new(wl.into_concurrent_dispatcher(python_probe_spec(), C, 256, None));
    let (bridge, mut ctl) = gate_bridge();

    // C+2 distinct keys all want to run; only C may be in flight at once.
    let mut handles = Vec::new();
    for i in 0..(C as u64 + 2) {
        handles.push(spawn_dispatch(
            &dispatcher,
            PROBE,
            vec![],
            caller(100 + i),
            &bridge,
        ));
    }

    // Exactly C reach the guest and park; the extra 2 wait for a budget slot.
    wait_entered(&mut ctl.gate_entered, C).await;
    ctl.gate_release.add_permits(C + 2);

    let mut peak = 0u32;
    for h in handles {
        peak = peak.max(decode_max_seen(
            &await_dispatch(h, "budget dispatch")
                .await
                .expect("dispatch ok"),
        ));
    }
    assert_eq!(
        peak, C as u32,
        "peak in-flight must equal the budget C={C} (never exceed it)"
    );
    dispatcher.shutdown().await;
}

// ── Facets 4 & 5 — an in-flight trap fails ALL siblings and rebuilds ─────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore] // Slow e2e: builds the Python component (venv + codegen + componentize).
async fn generated_python_inflight_trap_fails_all_siblings_then_rebuilds() {
    if !python_e2e_or_skip() {
        return;
    }
    let _quota_isolation = quota_isolation().lock().await;
    const SIBLINGS: u64 = 3;
    let wl = instantiate_wasm_workload(python_host())
        .await
        .expect("instantiate python component");
    let dispatcher = Arc::new(wl.into_concurrent_dispatcher(python_probe_spec(), 8, 256, None));
    let (bridge, mut ctl) = gate_bridge();

    // N-1 probe siblings park at test/gate (held), and one boom dispatch parks
    // at test/double_impl. All distinct keys, all in flight in the ONE region.
    let mut siblings = Vec::new();
    for i in 0..SIBLINGS {
        siblings.push(spawn_dispatch(
            &dispatcher,
            PROBE,
            vec![],
            caller(200 + i),
            &bridge,
        ));
    }
    let boom = spawn_dispatch(&dispatcher, BOOM, vec![], caller(299), &bridge);

    // Wait until every sibling AND boom are suspended inside the instance.
    wait_entered(&mut ctl.gate_entered, SIBLINGS as usize).await;
    wait_entered(&mut ctl.impl_entered, 1).await;

    // Release ONLY boom's host import: it returns, the guest raises after the
    // await, and the whole run_concurrent region collapses — taking every
    // in-flight sibling down with it.
    ctl.impl_release.add_permits(1);

    let boom_res = await_dispatch(boom, "boom").await;
    assert!(boom_res.is_err(), "the trapping dispatch itself must fail");
    for (i, h) in siblings.into_iter().enumerate() {
        let res = await_dispatch(h, "trap sibling").await;
        assert!(
            res.is_err(),
            "sibling {i} must fail when a co-resident dispatch traps (whole-region teardown)"
        );
        let msg = format!("{:?}", res.unwrap_err()).to_lowercase();
        assert!(
            msg.contains("trap") || msg.contains("unavailable") || msg.contains("instance"),
            "sibling {i} must fail with a retryable trap-class error, got: {msg}"
        );
    }

    // The instance must rebuild: a fresh dispatch succeeds AND reports
    // MAX_SEEN==1, which can only be true on a fresh linear memory (the
    // pre-trap in-flight count was SIBLINGS and never decremented on the
    // torn-down region).
    ctl.gate_release.add_permits(1); // let the recovery probe pass straight through
    let recovered = dispatcher
        .dispatch(PROBE, vec![], caller(777), &bridge)
        .await
        .expect("a dispatch after the trap must succeed on the rebuilt instance");
    assert_eq!(
        decode_max_seen(&recovered),
        1,
        "post-trap probe must see MAX_SEEN==1 (fresh linear memory ⇒ rebuild + cleared globals)"
    );
    dispatcher.shutdown().await;
}
