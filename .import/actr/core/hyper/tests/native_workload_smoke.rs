//! Runtime smoke tests for the C and Go V2 workload examples.
//!
//! The components are built by the `c_go_workloads` CI job and passed through
//! environment variables. These tests are ignored in the regular Rust suite
//! because the language-specific toolchains are intentionally not installed
//! there.

#![cfg(feature = "wasm-engine")]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use actr_hyper::config::WasmRuntimeLimits;
use actr_hyper::test_support::instantiate_wasm_workload;
use actr_hyper::wasm::WasmHost;
use actr_hyper::workload::{HostAbiFn, HostOperationResult, InvocationContext};
use actr_protocol::{
    ActrId, ActrType, Direction, Realm, RpcEnvelope, prost::Message as ProstMessage,
};

fn invocation_context(language: &str) -> InvocationContext {
    InvocationContext {
        self_id: ActrId {
            realm: Realm { realm_id: 1 },
            serial_number: 1,
            r#type: ActrType {
                manufacturer: "actr-ci".to_string(),
                name: format!("{language}-echo-workload"),
                version: "0.1.0".to_string(),
            },
        },
        caller_id: None,
        request_id: format!("native-{language}-smoke"),
    }
}

fn envelope(language: &str, payload: Vec<u8>) -> Vec<u8> {
    RpcEnvelope {
        route_key: "smoke/echo".to_string(),
        payload: Some(payload.into()),
        request_id: format!("native-{language}-smoke"),
        direction: Some(Direction::Request as i32),
        ..Default::default()
    }
    .encode_to_vec()
}

async fn assert_echo_component(env_name: &str, language: &str) {
    let component_path = std::env::var(env_name)
        .unwrap_or_else(|_| panic!("{env_name} must point to the built {language} component"));
    let component = std::fs::read(&component_path)
        .unwrap_or_else(|error| panic!("read {language} component `{component_path}`: {error}"));
    let mut limits = WasmRuntimeLimits::default();
    if language == "go" {
        // Go eagerly initializes its runtime heap when the component is
        // instantiated. Keep the production default conservative while
        // granting this known runtime enough bounded startup/entry fuel.
        // The default five-second invocation timeout remains in force and
        // catches a stock Go toolchain deadlocking in the async scheduler.
        limits.fuel_per_invocation = 100_000_000;
    }
    let host = WasmHost::compile_with_limits(&component, &limits)
        .unwrap_or_else(|error| panic!("compile {language} component: {error}"));
    let mut workload = instantiate_wasm_workload(&host)
        .await
        .unwrap_or_else(|error| panic!("instantiate {language} component: {error}"));

    workload
        .call_on_start()
        .await
        .unwrap_or_else(|error| panic!("run {language} on-start: {error}"));

    let host_calls = Arc::new(AtomicU64::new(0));
    let host_calls_for_bridge = Arc::clone(&host_calls);
    let bridge: HostAbiFn = Arc::new(move |_| {
        host_calls_for_bridge.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { HostOperationResult::AbiError(-1) })
    });
    let payload = format!("hello-from-{language}").into_bytes();
    let reply = workload
        .handle(
            &envelope(language, payload.clone()),
            invocation_context(language),
            &bridge,
        )
        .await
        .unwrap_or_else(|error| panic!("dispatch through {language} component: {error}"));

    let mut expected = b"echo: ".to_vec();
    expected.extend_from_slice(&payload);
    assert_eq!(
        reply, expected,
        "{language} component returned wrong payload"
    );
    assert_eq!(
        host_calls.load(Ordering::SeqCst),
        0,
        "{language} echo unexpectedly called a host import"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the C workload component built by the native workload CI job"]
async fn c_workload_echoes_through_actr_host() {
    assert_echo_component("ACTR_C_ECHO_COMPONENT", "c").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the Go workload component built by the native workload CI job"]
async fn go_workload_echoes_through_actr_host() {
    assert_echo_component("ACTR_GO_ECHO_COMPONENT", "go").await;
}
