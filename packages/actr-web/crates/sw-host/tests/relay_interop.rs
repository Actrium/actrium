//! Real browser/native interoperability through the canonical worker relay.
//! Run with tests/e2e/relay-runtime.mjs --interop and error-fixture.
use actr_protocol::ActrId;
use actr_sw_host::{RuntimeContext, WasmWorkload, WebContext};
use bytes::Bytes;
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{JsCast, prelude::*};
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_service_worker);

#[wasm_bindgen(inline_js = r#"
import * as adapter from '/adapter.js';
import * as runtime from '/wasm-bindgen-test';
function fixtureBase() {
  return location.origin;
}
export async function fixture_info() {
  return (await fetch(fixtureBase() + '/fixture')).json();
}
export async function relay_start(client, port) {
  adapter.start(runtime, client, port);
}
export async function relay_disconnect() {
  const response = await fetch(fixtureBase() + '/disconnect', { method: 'POST' });
  if (!response.ok) throw new Error('Fixture disconnect failed');
}
export function relay_stop(client) { adapter.stop(client); }
export function relay_security_observations() { return adapter.security_observations(); }
"#)]
extern "C" {
    fn fixture_info() -> js_sys::Promise;
    fn relay_start(client: &str, port: &web_sys::MessagePort) -> js_sys::Promise;
    fn relay_disconnect() -> js_sys::Promise;
    fn relay_stop(client: &str);
    fn relay_security_observations() -> JsValue;
}

async fn until(ready: impl Fn() -> bool) {
    for _ in 0..1500 {
        if ready() {
            return;
        }
        gloo_timers::future::TimeoutFuture::new(10).await;
    }
    assert!(ready(), "interoperability event deadline exceeded");
}

#[wasm_bindgen_test]
async fn browser_native_rpc_streams_and_reconnect_without_rtc_or_peer_listeners() {
    #[derive(serde::Deserialize)]
    struct Fixture {
        port: u16,
        actor_id: String,
    }
    let fixture: Fixture =
        serde_wasm_bindgen::from_value(JsFuture::from(fixture_info()).await.unwrap()).unwrap();
    let peer = ActrId::from_string_repr(&fixture.actor_id).unwrap();
    let client = "relay-browser-interop";
    let captured = Rc::new(RefCell::new(None::<Rc<RuntimeContext>>));
    let context = captured.clone();
    actr_sw_host::register_workload(WasmWorkload::new(Rc::new(move |route, body, _, ctx| {
        *context.borrow_mut() = Some(ctx);
        let result = match route {
            "browser.Setup" | "browser.Echo" => Ok(body.to_vec()),
            other => Err(format!("Unknown route: {other}")),
        };
        Box::pin(async move { result })
    })));
    let ports = web_sys::MessageChannel::new().unwrap();
    let events = Rc::new(RefCell::new(Vec::<bool>::new()));
    let received = events.clone();
    let on_message = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
        let payload = js_sys::Reflect::get(&event.data(), &"payload".into()).unwrap();
        if js_sys::Reflect::get(&payload, &"eventType".into())
            .ok()
            .and_then(|value| value.as_string())
            .as_deref()
            == Some("relay_state_changed")
        {
            let data = js_sys::Reflect::get(&payload, &"data".into()).unwrap();
            received.borrow_mut().push(
                js_sys::Reflect::get(&data, &"connected".into())
                    .unwrap()
                    .as_bool()
                    .unwrap(),
            );
        }
    }) as Box<dyn FnMut(web_sys::MessageEvent)>);
    ports
        .port2()
        .set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    ports.port2().start();
    let issuer_endpoint = format!("http://127.0.0.1:{}/issuer", fixture.port);
    let client_key = actr_protocol::realm_membership::RealmClientKeyFile::new(
        "test@example.com",
        "provider.example",
        &issuer_endpoint,
        &ed25519_dalek::SigningKey::from_bytes(&[27; 32]),
    );
    let config = js_sys::JSON::parse(
        &serde_json::json!({
            "issuer_endpoint": issuer_endpoint,
            "signaling_url": format!("ws://127.0.0.1:{}/signaling/ws", fixture.port),
            "realm": "test@example.com",
            "realm_client_key": client_key,
            "client_actr_type": "test/RelayBrowser@0.2.0",
            "service_fingerprint": "",
        })
        .to_string(),
    )
    .unwrap();
    actr_sw_host::register_client(client.into(), config, ports.port1())
        .await
        .unwrap();
    // The real WASM bootstrap must produce an ordinary object for the worker.
    let info = actr_sw_host::relay_connection_info(client.into())
        .await
        .unwrap();
    assert!(
        js_sys::Reflect::get(&info, &"signalingUrl".into())
            .unwrap()
            .as_string()
            .unwrap()
            .starts_with("ws://127.0.0.1:")
    );
    JsFuture::from(relay_start(client, &ports.port1()))
        .await
        .unwrap();
    let setup = js_sys::JSON::parse(
        r#"{"action":"rpc_call","request_id":"setup","request":{"route_key":"browser.Setup","payload":[],"timeout":5000}}"#,
    )
    .unwrap();
    actr_sw_host::handle_dom_control(client.into(), setup)
        .await
        .unwrap();
    until(|| captured.borrow().is_some()).await;
    let ctx = captured.borrow().as_ref().unwrap().clone();
    let payload = vec![0xA5; 64 * 1024];
    let started = js_sys::Date::now();
    assert_eq!(
        ctx.call_raw(&peer, "fixture.Fixture.Echo", &payload, 5000)
            .await
            .unwrap(),
        payload
    );
    let elapsed = js_sys::Date::now() - started;
    console_log!("First peer-acknowledged relay RPC: {elapsed:.1}ms (RTC forwarding disabled)");
    until(|| events.borrow().iter().any(|connected| *connected)).await;

    let chunks = Rc::new(RefCell::new(Vec::<Bytes>::new()));
    let received = chunks.clone();
    ctx.register_stream(
        "relay-interop".into(),
        Box::new(move |bytes| {
            received.borrow_mut().push(bytes);
        }),
    )
    .await
    .unwrap();
    // Native calls back into the browser, then sends both stream lane types.
    assert_eq!(
        ctx.call_raw(&peer, "fixture.Fixture.ProbePeer", &payload, 5000)
            .await
            .unwrap(),
        payload
    );
    until(|| chunks.borrow().len() == 2).await;
    assert!(
        chunks
            .borrow()
            .iter()
            .all(|chunk| chunk.as_ref() == payload)
    );
    // Exercise the public browser bytes API. Both native stream lane types
    // were received above; outbound browser streams use the reliable lane.
    let reliable = Bytes::from(vec![0xB6; 64 * 1024]);
    ctx.send_data_chunk(&peer, "relay-interop", reliable.clone())
        .await
        .unwrap();
    until(|| chunks.borrow().len() == 3).await;
    assert_eq!(chunks.borrow()[2], reliable);
    JsFuture::from(relay_disconnect()).await.unwrap();
    until(|| {
        events
            .borrow()
            .iter()
            .filter(|connected| **connected)
            .count()
            >= 2
    })
    .await;
    assert!(events.borrow().contains(&false));
    assert_eq!(
        ctx.call_raw(&peer, "fixture.Fixture.ProbePeer", b"after reconnect", 5000)
            .await
            .unwrap(),
        b"after reconnect"
    );
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Observed {
        sent_records: u32,
        received_records: u32,
        scoped_hellos: u32,
        invalid_hellos: u32,
        plaintext_leaks: u32,
        invalid_records: u32,
    }
    let observed: Observed = serde_wasm_bindgen::from_value(relay_security_observations()).unwrap();
    assert!(observed.sent_records > 5 && observed.received_records > 5);
    assert!(observed.scoped_hellos >= 2);
    assert_eq!(
        observed.invalid_hellos, 0,
        "Only relay-scoped access credentials may be exposed"
    );
    assert_eq!(
        observed.invalid_records, 0,
        "All relay data must use encrypted record framing"
    );
    assert_eq!(
        observed.plaintext_leaks, 0,
        "Known application markers must not appear on relay wire"
    );
    relay_stop(client);
    actr_sw_host::unregister_client(client.into()).await;
    ports.port2().set_onmessage(None);
}
