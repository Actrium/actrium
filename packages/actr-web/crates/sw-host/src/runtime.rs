//! Client Runtime for Service Worker
//!
//! Service Worker client runtime implemented according to the documented architecture:
//! - State path: `Mailbox` + `MailboxProcessor` for reliable message handling
//! - Fast path: direct callbacks for stream/media traffic with low latency
//! - WebRTC signaling relay coordinated through the DOM side
//!
//! # Message Flow
//!
//! ## Sending Requests (DOM -> Remote)
//! ```text
//! DOM → handle_dom_control → WORKLOAD (WasmWorkload dispatch)
//!     → local route: workload.dispatch(route_key, payload, ctx) → response
//!     → remote route: ctx.call_raw() → Gate → WebRTC DataChannel
//! ```
//!
//! ## Receiving Responses/Requests (Remote -> SW)
//! ```text
//! WebRTC → handle_fast_path → InboundPacketDispatcher.dispatch()
//!   -> RPC response: match the pending request directly and return to the DOM
//!   -> RPC request: `Mailbox` -> `MailboxProcessor` -> actor handling
//! ```

#[path = "relay_security.rs"]
mod relay_security;
pub use relay_security::{handle_relay_record, prepare_relay_connection, start_relay_handshake};

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use actr_mailbox_web::{IndexedDbMailbox, Mailbox, MessageRecord};
use actr_protocol::prost::Message as ProstMessage;
use actr_protocol::{
    AIdCredential, Acl, AclRule, ActrId, ActrToSignaling, ActrType, Direction, Ping,
    RegisterAuthMode, RegisterRequest, RenewCredentialRequest, RoleNegotiation,
    RouteCandidatesRequest, RpcEnvelope, SIGNALING_ENVELOPE_VERSION, ServiceAvailabilityState,
    SignalingEnvelope, acl_rule, actr_relay, actr_to_signaling, renew_credential_response,
    route_candidates_request, session_description, signaling_envelope, signaling_to_actr,
};
use actr_protocol::{IceCandidate, IceRestartRejected, SessionDescription, ice_restart_rejected};
use actr_web_common::{
    ExponentialBackoff, MessageFormat, PayloadType, RenewError, WebIssuerClient,
};
use bytes::Bytes;
use futures::StreamExt;
use futures::channel::{mpsc, oneshot};
use futures::lock::Mutex;
use gloo_timers::future::TimeoutFuture;
use js_sys::{Object, Reflect};
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{BinaryType, CloseEvent, MessageEvent, MessagePort, WebSocket};

use actr_platform_traits::{PlatformProvider, StorageNamespace};
use actr_platform_web::WebPlatformProvider;
use sha2::{Digest, Sha256};

use crate::context::RuntimeContext;
use crate::inbound::{InboundPacketDispatcher, MailboxProcessor, Scheduler};
use crate::outbound::Gate;
use crate::web_context::RuntimeBridge;

/// Milliseconds from `performance.now()`: monotonic per RFC-0419, unlike
/// `Date.now()`, which jumps when the wall clock changes (NTP, manual set).
///
/// Fails explicitly if the Performance API is unavailable: falling back to
/// wall time would allow a clock adjustment to fire the connection timeout
/// early or stall it indefinitely.
fn monotonic_now_ms() -> Result<f64, JsValue> {
    js_sys::global()
        .unchecked_into::<web_sys::WorkerGlobalScope>()
        .performance()
        .map(|p| p.now())
        .ok_or_else(|| {
            JsValue::from_str(
                "Performance API unavailable; cannot enforce a monotonic WebSocket connect timeout",
            )
        })
}

type StreamHandler = Rc<RefCell<Box<dyn FnMut(Bytes)>>>;

#[derive(Serialize)]
struct RpcError {
    code: i32,
    message: String,
}

#[derive(Serialize)]
struct RpcResponsePayload {
    request_id: String,
    #[serde(with = "serde_wasm_bindgen::preserve")]
    data: JsValue,
    error: Option<RpcError>,
}

#[derive(Serialize)]
struct WebRtcCommandPayload {
    action: String,
    #[serde(rename = "peerId")]
    peer_id: String,
    #[serde(with = "serde_wasm_bindgen::preserve")]
    payload: JsValue,
}

#[derive(Serialize)]
struct SendDataPayload {
    #[serde(rename = "channelId")]
    channel_id: u32,
    #[serde(with = "serde_wasm_bindgen::preserve")]
    data: JsValue,
}

#[derive(Serialize)]
struct SwMessage<T> {
    #[serde(rename = "type")]
    msg_type: &'static str,
    payload: T,
}

#[derive(Clone)]
struct SignalingClient {
    ws: WebSocket,
    pending: Rc<Mutex<HashMap<String, oneshot::Sender<SignalingEnvelope>>>>,
    /// Sender side of inbound channel; stored so we can close it on disconnect.
    inbound_tx: Rc<RefCell<Option<mpsc::UnboundedSender<SignalingEnvelope>>>>,
    inbound_rx: Rc<Mutex<mpsc::UnboundedReceiver<SignalingEnvelope>>>,
    envelope_counter: Rc<std::cell::Cell<u64>>,
    _onmessage: Rc<Closure<dyn FnMut(MessageEvent)>>,
    _onclose: Rc<Closure<dyn FnMut(CloseEvent)>>,
}

impl SignalingClient {
    async fn connect(url: &str) -> Result<Self, JsValue> {
        let ws = WebSocket::new(url)?;
        ws.set_binary_type(BinaryType::Arraybuffer);

        let pending: Rc<Mutex<HashMap<String, oneshot::Sender<SignalingEnvelope>>>> =
            Rc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = mpsc::unbounded();
        let inbound_rx = Rc::new(Mutex::new(rx));
        let envelope_counter = Rc::new(std::cell::Cell::new(0));

        let pending_clone = Rc::clone(&pending);
        let tx_clone = tx.clone();
        let onmessage = Closure::wrap(Box::new(move |event: MessageEvent| {
            let data = match event.data().dyn_into::<js_sys::ArrayBuffer>() {
                Ok(buf) => {
                    let array = js_sys::Uint8Array::new(&buf);
                    let mut bytes = vec![0u8; array.length() as usize];
                    array.copy_to(&mut bytes);
                    bytes
                }
                Err(_) => return,
            };

            let envelope = match SignalingEnvelope::decode(&data[..]) {
                Ok(env) => env,
                Err(e) => {
                    log::error!("[Signaling] Failed to decode envelope: {:?}", e);
                    return;
                }
            };

            let pending_clone = Rc::clone(&pending_clone);
            let tx_clone = tx_clone.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if let Some(reply_for) = envelope.reply_for.clone() {
                    if let Some(tx) = pending_clone.lock().await.remove(&reply_for) {
                        let _ = tx.send(envelope);
                        return;
                    }
                }
                let _ = tx_clone.unbounded_send(envelope);
            });
        }) as Box<dyn FnMut(MessageEvent)>);

        ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));

        // Close the inbound channel when the WebSocket closes so that
        // recv_inbound() returns None and the relay loop terminates.
        let inbound_tx: Rc<RefCell<Option<mpsc::UnboundedSender<SignalingEnvelope>>>> =
            Rc::new(RefCell::new(Some(tx)));
        let inbound_tx_for_close = Rc::clone(&inbound_tx);
        let onclose = Closure::wrap(Box::new(move |_event: CloseEvent| {
            log::info!("[Signaling] WebSocket closed, closing inbound channel");
            if let Some(tx) = inbound_tx_for_close.borrow_mut().take() {
                tx.close_channel();
            }
        }) as Box<dyn FnMut(CloseEvent)>);
        ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));

        // Wait for open (max 15s)
        let ws_clone = ws.clone();
        let open_future = async move {
            let start = monotonic_now_ms()?;
            loop {
                if ws_clone.ready_state() == WebSocket::OPEN {
                    return Ok(());
                }
                if ws_clone.ready_state() == WebSocket::CLOSED {
                    return Err(JsValue::from_str("WebSocket closed"));
                }
                if monotonic_now_ms()? - start > 15000.0 {
                    return Err(JsValue::from_str("WebSocket connect timeout"));
                }
                TimeoutFuture::new(10).await;
            }
        };
        open_future.await?;

        Ok(Self {
            ws,
            pending,
            inbound_tx,
            inbound_rx,
            envelope_counter,
            _onmessage: Rc::new(onmessage),
            _onclose: Rc::new(onclose),
        })
    }

    /// Connect to signaling server with retry and exponential backoff.
    ///
    /// Mirrors the `connect_with_retries` logic from the native `actr` runtime:
    /// - If `reconnect_enabled` is false, attempts a single connection.
    /// - Otherwise, retries up to `max_attempts` times with exponential backoff.
    async fn connect_with_retries(
        url: &str,
        reconnect_cfg: &ReconnectConfig,
    ) -> Result<Self, JsValue> {
        if !reconnect_cfg.enabled {
            return Self::connect(url).await;
        }

        let mut backoff =
            ExponentialBackoff::new(reconnect_cfg.initial_delay_ms, reconnect_cfg.max_delay_ms)
                .with_multiplier(reconnect_cfg.backoff_multiplier)
                .with_jitter(0.1);

        let mut attempt: u32 = 0;

        loop {
            attempt += 1;

            match Self::connect(url).await {
                Ok(client) => {
                    if attempt > 1 {
                        log::info!("[Signaling] Connected after {} attempts", attempt);
                    }
                    return Ok(client);
                }
                Err(e) => {
                    log::warn!(
                        "[Signaling] Connect attempt {}/{} failed: {:?}",
                        attempt,
                        reconnect_cfg.max_attempts,
                        e
                    );

                    if attempt >= reconnect_cfg.max_attempts {
                        log::error!(
                            "[Signaling] Connect failed after {} attempts, giving up",
                            attempt
                        );
                        return Err(e);
                    }

                    let delay = backoff.next_delay();
                    let delay_ms = delay.as_millis() as u32;
                    log::info!("[Signaling] Retry connect after {}ms", delay_ms);
                    TimeoutFuture::new(delay_ms).await;
                }
            }
        }
    }

    fn next_envelope_id(&self) -> String {
        let next = self.envelope_counter.get() + 1;
        self.envelope_counter.set(next);
        format!("sw-env-{}-{}", next, js_sys::Date::now() as u64)
    }

    async fn send_envelope(&self, mut envelope: SignalingEnvelope) -> Result<(), JsValue> {
        if envelope.envelope_id.is_empty() {
            envelope.envelope_id = self.next_envelope_id();
        }
        if envelope.envelope_version == 0 {
            envelope.envelope_version = SIGNALING_ENVELOPE_VERSION;
        }

        let bytes = envelope.encode_to_vec();
        self.ws.send_with_u8_array(&bytes)?;
        Ok(())
    }

    async fn send_request(
        &self,
        envelope: SignalingEnvelope,
    ) -> Result<SignalingEnvelope, JsValue> {
        let mut envelope = envelope;
        if envelope.envelope_id.is_empty() {
            envelope.envelope_id = self.next_envelope_id();
        }
        let reply_for = envelope.envelope_id.clone();
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(reply_for, tx);
        self.send_envelope(envelope).await?;

        rx.await
            .map_err(|_| JsValue::from_str("signaling reply channel closed"))
    }

    async fn recv_inbound(&self) -> Option<SignalingEnvelope> {
        self.inbound_rx.lock().await.next().await
    }

    /// Close the underlying WebSocket connection.
    ///
    /// This triggers cleanup on the signaling host side (which removes
    /// the actor from the ServiceRegistry) and causes background loops
    /// (heartbeat, relay) to naturally terminate.
    fn close(&self) {
        let _ = self.ws.close();
        // Also close the inbound channel immediately so recv_inbound()
        // returns None without waiting for the async onclose event.
        if let Some(tx) = self.inbound_tx.borrow_mut().take() {
            tx.close_channel();
        }
    }
}

/// Reconnection configuration for the initial signaling connection.
///
/// Mirrors `ReconnectConfig` from the native `actr` runtime.
#[derive(Debug, Clone, Deserialize)]
struct ReconnectConfig {
    /// Whether automatic reconnection is enabled.
    #[serde(default = "ReconnectConfig::default_enabled")]
    enabled: bool,

    /// Maximum number of connection attempts.
    #[serde(default = "ReconnectConfig::default_max_attempts")]
    max_attempts: u32,

    /// Initial retry delay in milliseconds.
    #[serde(default = "ReconnectConfig::default_initial_delay_ms")]
    initial_delay_ms: u64,

    /// Maximum retry delay in milliseconds.
    #[serde(default = "ReconnectConfig::default_max_delay_ms")]
    max_delay_ms: u64,

    /// Backoff multiplier factor.
    #[serde(default = "ReconnectConfig::default_backoff_multiplier")]
    backoff_multiplier: f64,
}

impl ReconnectConfig {
    fn default_enabled() -> bool {
        true
    }
    fn default_max_attempts() -> u32 {
        10
    }
    fn default_initial_delay_ms() -> u64 {
        1000
    }
    fn default_max_delay_ms() -> u64 {
        60000
    }
    fn default_backoff_multiplier() -> f64 {
        2.0
    }
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_attempts: 10,
            initial_delay_ms: 1000, // 1s
            max_delay_ms: 60000,    // 60s
            backoff_multiplier: 2.0,
        }
    }
}

#[derive(Deserialize)]
struct SwConfig {
    issuer_endpoint: String,
    signaling_url: String,
    /// Realm name `<realm_id>@<domain>`.
    realm: String,
    /// Import an already authorized device key once; subsequent starts restore it.
    #[serde(default)]
    realm_client_key: Option<actr_protocol::realm_membership::RealmClientKeyFile>,
    client_actr_type: String,
    #[serde(default)]
    target_actr_type: Option<String>,
    service_fingerprint: String,
    /// ACL: list of actr_types (e.g. "acme:echo-client-app") that are allowed
    /// to discover and communicate with this actor. Sent in RegisterRequest.
    #[serde(default)]
    acl_allow_types: Vec<String>,
    /// Reconnection configuration for the initial signaling WebSocket connection.
    /// If omitted, defaults are used (enabled, 10 attempts, 1s–60s exponential backoff).
    #[serde(default)]
    reconnect_config: ReconnectConfig,
}

#[derive(Deserialize)]
struct DomRpcCall {
    action: String,
    request_id: String,
    request: DomRpcRequest,
}

#[derive(Deserialize)]
struct DomRpcRequest {
    route_key: String,
    payload: Vec<u8>,
    timeout: Option<u32>,
}

#[derive(Deserialize)]
struct DomWebRtcEvent {
    #[serde(rename = "eventType")]
    event_type: String,
    data: serde_json::Value,
}

#[derive(Deserialize)]
struct LocalDescriptionEvent {
    #[allow(dead_code)]
    #[serde(rename = "peerId")]
    peer_id: String,
    sdp: SdpInit,
    #[serde(rename = "sdpExchangeId")]
    sdp_exchange_id: Option<String>,
}

#[derive(Deserialize)]
struct SdpInit {
    #[serde(rename = "type")]
    sdp_type: String,
    sdp: String,
}

#[derive(Deserialize)]
struct IceCandidateEvent {
    #[allow(dead_code)]
    #[serde(rename = "peerId")]
    peer_id: String,
    candidate: IceCandidateInit,
}

#[derive(Deserialize)]
struct IceCandidateInit {
    candidate: String,
    #[serde(rename = "sdpMid")]
    sdp_mid: Option<String>,
    #[serde(rename = "sdpMLineIndex")]
    sdp_mline_index: Option<u32>,
    #[serde(rename = "usernameFragment")]
    username_fragment: Option<String>,
}

#[derive(Deserialize)]
struct DataChannelEvent {
    #[serde(rename = "peerId")]
    peer_id: String,
    #[serde(rename = "channelId")]
    channel_id: u32,
}

#[derive(Deserialize)]
struct FastPathPayload {
    #[serde(rename = "streamId")]
    stream_id: String,
    data: Vec<u8>,
}

/// Distinguishes between DOM-originated and handler-internal pending RPCs.
///
/// - `Dom`: response must be sent back to the DOM as a `control_response` message.
/// - `Internal`: response is consumed by the HostGate oneshot only (handler-initiated remote calls).
#[derive(Clone, Debug)]
pub enum PendingRpcTarget {
    Dom,
    Internal,
}

/// ICE restart configuration constants
const ICE_RESTART_MAX_RETRIES: u32 = 2;
const ICE_RESTART_TIMEOUT_MS: u32 = 3000;
const ICE_RESTART_INITIAL_BACKOFF_MS: u32 = 2000;
const ICE_RESTART_MAX_BACKOFF_MS: u32 = 5000;

fn is_correlated_restart_rejection(
    is_offerer: bool,
    restart_inflight: bool,
    pending_exchange_id: Option<&str>,
    rejection: &IceRestartRejected,
) -> bool {
    is_offerer
        && restart_inflight
        && rejection.reason == Some(ice_restart_rejected::Reason::NoActiveSession as i32)
        && pending_exchange_id == Some(rejection.sdp_exchange_id.as_str())
}

#[cfg(test)]
mod ice_restart_rejection_tests {
    use super::*;

    fn rejection(exchange_id: &str) -> IceRestartRejected {
        IceRestartRejected {
            sdp_exchange_id: exchange_id.to_string(),
            reason: Some(ice_restart_rejected::Reason::NoActiveSession as i32),
        }
    }

    #[test]
    fn accepts_only_current_offerer_restart_rejection() {
        let current = rejection("current");
        assert!(is_correlated_restart_rejection(
            true,
            true,
            Some("current"),
            &current
        ));
        assert!(!is_correlated_restart_rejection(
            true,
            true,
            Some("stale"),
            &current
        ));
        assert!(!is_correlated_restart_rejection(
            false,
            true,
            Some("current"),
            &current
        ));
        assert!(!is_correlated_restart_rejection(
            true,
            false,
            Some("current"),
            &current
        ));
    }
}

/// P2P initial connection retry configuration.
///
/// Mirrors the retry strategy from the native `actr` runtime's
/// `create_connection_inner` (3 retries, 5s→15s backoff, 30s overall).
const P2P_CONNECTION_MAX_RETRIES: u32 = 3;
const P2P_RETRY_INITIAL_DELAY_MS: u64 = 3000;
const P2P_RETRY_MAX_DELAY_MS: u64 = 15000;

struct SwRuntime {
    /// Unique client identifier (one per browser tab)
    client_id: String,
    issuer: WebIssuerClient,
    signaling_url: String,
    reconnect_config: ReconnectConfig,
    realm: String,
    client_actr_type: ActrType,
    target_actr_type: Option<ActrType>,
    service_fingerprint: String,
    acl: Option<Acl>,
    signaling: SignalingClient,
    actor_id: Option<ActrId>,
    credential: Option<AIdCredential>,
    /// TURN credential from Issuer registration (time-limited HMAC)
    turn_credential: Option<actr_protocol::TurnCredential>,
    /// Platform provider for crypto and KV storage
    platform: WebPlatformProvider,
    /// Per-target-type cache of the most recently discovered `ActrId`.
    /// Avoids running Issuer discovery on every legacy raw RPC call.
    /// Entries are invalidated when the corresponding peer connection fails or closes.
    discovered_targets: HashMap<String, ActrId>,
    dom_port: Option<MessagePort>,
    pending_rpcs: HashMap<String, PendingRpcTarget>,
    known_peers: HashSet<String>,
    open_channels: HashMap<String, HashSet<u32>>,
    pending_channel_data: HashMap<String, Vec<Vec<u8>>>,
    role_negotiated: HashSet<String>,
    role_assignments: HashMap<String, bool>,
    pending_local_sdp_exchanges: HashMap<String, String>,
    pending_remote_sdp_exchanges: HashMap<String, String>,
    /// ICE restart: tracks whether an ICE restart is in-flight for each peer
    ice_restart_inflight: HashMap<String, bool>,
    /// ICE restart: retry attempt count per peer
    ice_restart_attempts: HashMap<String, u32>,
    /// ICE restart: tracks the connection state for each peer
    peer_connection_states: HashMap<String, String>,
    /// The DOM connection whose state was most recently observed for each peer.
    peer_connection_ids: HashMap<String, String>,
}

impl SwRuntime {
    async fn new(client_id: String, config: SwConfig) -> Result<Self, JsValue> {
        // Build ACL from config
        let acl = if config.acl_allow_types.is_empty() {
            None
        } else {
            let rules: Vec<AclRule> = config
                .acl_allow_types
                .iter()
                .map(|type_str| {
                    let actr_type = ActrType::from_string_repr(type_str).map_err(|e| {
                        JsValue::from_str(&format!("Invalid ACL type '{}': {}", type_str, e))
                    })?;
                    Ok(AclRule {
                        permission: acl_rule::Permission::Allow as i32,
                        from_type: actr_type,
                        // Actrix's DB-backed ACL lookup currently matches on an explicit
                        // `source_realm` equality. Using `any_realm` would not match and
                        // would default-deny discovery.
                        source_realm: Some(acl_rule::SourceRealm::Realm(config.realm.clone())),
                    })
                })
                .collect::<Result<Vec<_>, JsValue>>()?;
            Some(Acl { rules })
        };

        let client_actr_type = ActrType::from_string_repr(&config.client_actr_type)
            .map_err(|e| JsValue::from_str(&format!("Invalid client actr_type: {e}")))?;
        let platform = WebPlatformProvider::new(client_actr_type.to_string_repr());

        // Step 1: Obtain credential via Issuer HTTP registration
        //   Try to restore persisted credentials first; if expired or missing,
        //   perform a fresh Issuer HTTP registration.
        //
        // TD-004: partition the namespace by `client_id` so each tab performs
        // an independent Issuer registration and receives its own `actor_id`.
        let cred_kv_ns = Self::build_cred_kv_namespace(&client_actr_type, &client_id);
        let issuer = Self::load_issuer_client(&config, &platform).await?;
        let (actor_id, credential, turn_credential) = Self::obtain_credential_from_issuer(
            &issuer,
            &client_actr_type,
            &config.realm,
            &acl,
            &platform,
            &cred_kv_ns,
        )
        .await?;

        // Step 2: Build signaling URL with credential query params
        let signaling_url_with_cred = Self::build_signaling_url_with_identity_static(
            &config.signaling_url,
            &actor_id,
            &credential,
        );

        // Step 3: Connect signaling WebSocket with credential in URL
        let signaling = SignalingClient::connect_with_retries(
            &signaling_url_with_cred,
            &config.reconnect_config,
        )
        .await?;

        Ok(Self {
            client_id,
            issuer,
            signaling_url: config.signaling_url,
            reconnect_config: config.reconnect_config,
            realm: config.realm,
            client_actr_type,
            target_actr_type: config
                .target_actr_type
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .map(ActrType::from_string_repr)
                .transpose()
                .map_err(|e| JsValue::from_str(&format!("Invalid target actr_type: {e}")))?,
            service_fingerprint: config.service_fingerprint,
            acl,
            signaling,
            actor_id: Some(actor_id),
            credential: Some(credential),
            turn_credential,
            platform,
            discovered_targets: HashMap::new(),
            dom_port: None,
            pending_rpcs: HashMap::new(),
            known_peers: HashSet::new(),
            open_channels: HashMap::new(),
            pending_channel_data: HashMap::new(),
            role_negotiated: HashSet::new(),
            role_assignments: HashMap::new(),
            pending_local_sdp_exchanges: HashMap::new(),
            pending_remote_sdp_exchanges: HashMap::new(),
            ice_restart_inflight: HashMap::new(),
            ice_restart_attempts: HashMap::new(),
            peer_connection_states: HashMap::new(),
            peer_connection_ids: HashMap::new(),
        })
    }

    async fn load_issuer_client(
        config: &SwConfig,
        platform: &WebPlatformProvider,
    ) -> Result<WebIssuerClient, JsValue> {
        use actr_web_common::issuer_client::WebRealmClientKey;
        let scope = serde_json::to_vec(&(
            "actr-web-realm-client-key/v1",
            &config.realm,
            config.issuer_endpoint.trim_end_matches('/'),
        ))
        .map_err(|_| JsValue::from_str("Cannot encode Realm key scope"))?;
        let namespace = StorageNamespace::actr_v1(Sha256::digest(scope).into());
        let store = platform
            .secret_store(&namespace)
            .await
            .map_err(|e| JsValue::from_str(&format!("Cannot open Realm key store: {e}")))?;
        let text = match &config.realm_client_key {
            Some(file) => serde_json::to_string(file)
                .map_err(|_| JsValue::from_str("Cannot encode Realm client key"))?,
            None => {
                let bytes = store.get("realm_client_key").await
                    .map_err(|e| JsValue::from_str(&format!("Cannot load Realm client key: {e}")))?
                    .ok_or_else(|| JsValue::from_str(
                        "Import an authorized Realm client key with runtimeConfig.realm_client_key before starting this browser Host"
                    ))?;
                String::from_utf8(bytes)
                    .map_err(|_| JsValue::from_str("Stored Realm client key is invalid"))?
            }
        };
        let key = WebRealmClientKey::from_key_file(&config.issuer_endpoint, &config.realm, &text)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        if config.realm_client_key.is_some() {
            store
                .set("realm_client_key", text.as_bytes())
                .await
                .map_err(|e| JsValue::from_str(&format!("Cannot persist Realm client key: {e}")))?;
        }
        Ok(WebIssuerClient::new(&config.issuer_endpoint, key))
    }

    /// Register with Issuer via HTTP and obtain credential.
    ///
    /// Web always uses Linked registration (realm authorization). Credential
    /// renewal is handled by the Web Credential Manager via POST /issuer/renew.
    async fn register_via_issuer(&mut self) -> Result<(), JsValue> {
        let (actor_id, credential, turn_credential) = Self::obtain_credential_from_issuer(
            &self.issuer,
            &self.client_actr_type,
            &self.realm,
            &self.acl,
            &self.platform,
            &self.cred_kv_namespace(),
        )
        .await?;
        self.actor_id = Some(actor_id);
        self.credential = Some(credential);
        self.turn_credential = turn_credential;
        Ok(())
    }

    /// Obtain a credential from Issuer via HTTP.
    ///
    /// 1. Perform Linked registration (no manifest for web clients)
    /// 2. Persist credential and actor_id to IndexedDB
    /// 3. Return (actor_id, credential, turn_credential)
    async fn obtain_credential_from_issuer(
        issuer: &WebIssuerClient,
        client_actr_type: &ActrType,
        realm: &str,
        acl: &Option<Acl>,
        platform: &WebPlatformProvider,
        cred_kv_ns: &StorageNamespace,
    ) -> Result<(ActrId, AIdCredential, Option<actr_protocol::TurnCredential>), JsValue> {
        // Try to restore persisted credential if still valid
        if let Some((actor_id, credential)) =
            Self::try_restore_credential_static(platform, cred_kv_ns).await?
        {
            log::info!(
                "[SW] credentials restored from IndexedDB (actor_id={})",
                actor_id
            );
            if issuer.accepts_credential(&credential) && actor_id.realm == realm {
                let turn_credential =
                    Self::try_restore_turn_credential_static(platform, cred_kv_ns).await?;
                return Ok((actor_id, credential, turn_credential));
            }
            Self::clear_persisted_identity_static(platform, cred_kv_ns).await?;
        }

        if let Some((actor_id, credential, turn_credential)) =
            Self::try_renew_persisted_credential_static(issuer, platform, cred_kv_ns).await?
        {
            log::info!(
                "[SW] credentials renewed from persisted identity lease (actor_id={})",
                actor_id
            );
            return Ok((actor_id, credential, turn_credential));
        }

        // Web registration always uses Linked auth (realm authorization).
        // Legacy PSK renewal path removed — credential renewal now goes
        // through the Web Credential Manager via POST /issuer/renew.
        let request = RegisterRequest {
            actr_type: client_actr_type.clone(),
            realm: realm.to_string(),
            service_spec: None,
            acl: acl.clone(),
            service: None,
            ws_address: None,
            manifest_raw: None,
            publisher_signature: None,
            target: None,
            signing_public_key: None,
            auth_mode: Some(RegisterAuthMode::Linked as i32),
        };

        log::info!(
            "[SW] register via Issuer HTTP (linked): actr_type={}",
            client_actr_type
        );

        let response = issuer
            .register_linked(request)
            .await
            .map_err(|e| JsValue::from_str(&format!("Issuer registration failed: {e}")))?;

        match response.result {
            Some(actr_protocol::register_response::Result::Success(ok)) => {
                let actor_id = ok.actr_id.clone();
                let credential = ok.credential.clone();
                let turn_credential = Some(ok.turn_credential.clone());

                log::info!("[SW] Issuer registration success: actr_id={}", actor_id);

                // Persist credential and actor_id
                Self::persist_credentials_static(platform, cred_kv_ns, &actor_id, &credential)
                    .await?;

                // Persist TurnCredential
                if let Some(ref tc) = turn_credential {
                    Self::persist_turn_credential_static(platform, cred_kv_ns, tc).await?;
                }

                if let (Some(token), Some(expires_at)) = (
                    ok.renewal_token.as_ref(),
                    ok.renewal_token_expires_at.as_ref(),
                ) {
                    Self::persist_renewal_token_static(
                        platform,
                        cred_kv_ns,
                        token.as_ref(),
                        expires_at.seconds as u64,
                    )
                    .await?;
                }

                Ok((actor_id, credential, turn_credential))
            }
            Some(actr_protocol::register_response::Result::Error(err)) => {
                log::warn!("[SW] Issuer register error: {}", err.message);
                Err(JsValue::from_str(&format!(
                    "Issuer register failed: {}",
                    err.message
                )))
            }
            None => Err(JsValue::from_str("Issuer register response missing result")),
        }
    }

    /// Build signaling URL with credential query params for authenticated WS connection.
    fn build_signaling_url_with_identity(&self) -> String {
        let actor_id = self.actor_id.as_ref().expect("actor_id must be set");
        let credential = self.credential.as_ref().expect("credential must be set");
        Self::build_signaling_url_with_identity_static(&self.signaling_url, actor_id, credential)
    }

    /// Build signaling URL with identity params (static version, no &self needed).
    fn build_signaling_url_with_identity_static(
        base_url: &str,
        actor_id: &ActrId,
        credential: &AIdCredential,
    ) -> String {
        let actor_str = actor_id.to_string_repr();
        let claims_b64 = bytes_to_base64(&credential.claims);
        let sig_b64 = bytes_to_base64(&credential.signature);

        let separator = if base_url.contains('?') { "&" } else { "?" };
        format!(
            "{base_url}{separator}actor_id={}&key_id={}&claims={}&signature={}",
            js_encode_uri_component(&actor_str),
            credential.key_id,
            js_encode_uri_component(&claims_b64),
            js_encode_uri_component(&sig_b64),
        )
    }

    /// Persist credential and actor_id to IndexedDB (static version).
    async fn persist_credentials_static(
        platform: &WebPlatformProvider,
        cred_kv_ns: &StorageNamespace,
        actor_id: &ActrId,
        credential: &AIdCredential,
    ) -> Result<(), JsValue> {
        let kv = platform
            .secret_store(cred_kv_ns)
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to open KV store: {e}")))?;

        let cred_bytes = credential.encode_to_vec();
        kv.set("credential", &cred_bytes)
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to persist credential: {e}")))?;

        let id_str = actor_id.to_string_repr();
        kv.set("actor_id", id_str.as_bytes())
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to persist actor_id: {e}")))?;

        log::info!("[SW] credentials persisted to IndexedDB");
        Ok(())
    }

    /// Persist TurnCredential to IndexedDB (static version).
    async fn persist_turn_credential_static(
        platform: &WebPlatformProvider,
        cred_kv_ns: &StorageNamespace,
        turn_credential: &actr_protocol::TurnCredential,
    ) -> Result<(), JsValue> {
        let kv = platform
            .secret_store(cred_kv_ns)
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to open KV store: {e}")))?;

        let tc_bytes = turn_credential.encode_to_vec();
        kv.set("turn_credential", &tc_bytes)
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to persist TurnCredential: {e}")))?;

        log::info!("[SW] TurnCredential persisted to IndexedDB");
        Ok(())
    }

    async fn persist_renewal_token_static(
        platform: &WebPlatformProvider,
        cred_kv_ns: &StorageNamespace,
        renewal_token: &[u8],
        expires_at: u64,
    ) -> Result<(), JsValue> {
        let kv = platform
            .secret_store(cred_kv_ns)
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to open KV store: {e}")))?;

        kv.set("renewal_token", renewal_token)
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to persist renewal token: {e}")))?;
        kv.set("renewal_token_expires_at", &expires_at.to_le_bytes())
            .await
            .map_err(|e| {
                JsValue::from_str(&format!("Failed to persist renewal token expiry: {e}"))
            })?;

        Ok(())
    }

    async fn clear_persisted_identity_static(
        platform: &WebPlatformProvider,
        cred_kv_ns: &StorageNamespace,
    ) -> Result<(), JsValue> {
        let kv = platform
            .secret_store(cred_kv_ns)
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to open KV store: {e}")))?;

        for key in [
            "credential",
            "actor_id",
            "turn_credential",
            "renewal_token",
            "renewal_token_expires_at",
            "renewal_attempt_id",
            "renewal_attempt_actor_id",
            "renewal_attempt_token_hash",
        ] {
            let _ = kv.delete(key).await;
        }

        Ok(())
    }

    async fn try_renew_persisted_credential_static(
        issuer: &WebIssuerClient,
        platform: &WebPlatformProvider,
        cred_kv_ns: &StorageNamespace,
    ) -> Result<Option<(ActrId, AIdCredential, Option<actr_protocol::TurnCredential>)>, JsValue>
    {
        let kv = platform
            .secret_store(cred_kv_ns)
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to open KV store: {e}")))?;

        let credential = kv
            .get("credential")
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to read credential: {e}")))?;
        if !credential.as_ref().is_some_and(|bytes| {
            AIdCredential::decode(bytes.as_slice())
                .is_ok_and(|credential| issuer.accepts_credential(&credential))
        }) {
            Self::clear_persisted_identity_static(platform, cred_kv_ns).await?;
            return Ok(None);
        }

        let Some(id_bytes) = kv
            .get("actor_id")
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to read actor_id: {e}")))?
        else {
            return Ok(None);
        };
        let Some(token) = kv
            .get("renewal_token")
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to read renewal token: {e}")))?
        else {
            return Ok(None);
        };
        let Some(expires_bytes) = kv
            .get("renewal_token_expires_at")
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to read renewal token expiry: {e}")))?
        else {
            return Ok(None);
        };

        if expires_bytes.len() != 8 {
            return Ok(None);
        }
        let expires_at = u64::from_le_bytes(expires_bytes.as_slice().try_into().unwrap());
        let now_secs = (js_sys::Date::now() / 1000.0) as u64;
        // The issuer TTL and rotation window are configurable; the persisted
        // token has no authoritative issuance-time bound.
        if now_secs >= expires_at {
            let _ = kv.delete("renewal_token").await;
            let _ = kv.delete("renewal_token_expires_at").await;
            return Ok(None);
        }

        let id_str = String::from_utf8(id_bytes)
            .map_err(|e| JsValue::from_str(&format!("Invalid actor_id UTF-8: {e}")))?;
        let actor_id = ActrId::from_string_repr(&id_str)
            .map_err(|e| JsValue::from_str(&format!("Failed to parse actor_id: {e}")))?;

        // Persist the exact attempt identity across outer Service Worker
        // recovery rounds (and worker restarts). It is replaced only when the
        // actor/token binding changes or Issuer returns a definitive result.
        let token_hash = Sha256::digest(&token).to_vec();
        let stored_attempt_id = kv
            .get("renewal_attempt_id")
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to read renewal attempt: {e}")))?;
        let stored_attempt_actor = kv.get("renewal_attempt_actor_id").await.map_err(|e| {
            JsValue::from_str(&format!("Failed to read renewal attempt actor: {e}"))
        })?;
        let stored_attempt_token_hash =
            kv.get("renewal_attempt_token_hash").await.map_err(|e| {
                JsValue::from_str(&format!("Failed to read renewal attempt token: {e}"))
            })?;
        let attempt_id = match (
            stored_attempt_id,
            stored_attempt_actor,
            stored_attempt_token_hash,
        ) {
            (Some(attempt), Some(actor), Some(hash))
                if attempt.len() == 16 && actor == id_str.as_bytes() && hash == token_hash =>
            {
                Bytes::from(attempt)
            }
            _ => {
                let attempt = Bytes::copy_from_slice(uuid::Uuid::new_v4().as_bytes());
                kv.set("renewal_attempt_id", attempt.as_ref())
                    .await
                    .map_err(|e| {
                        JsValue::from_str(&format!("Failed to persist renewal attempt: {e}"))
                    })?;
                kv.set("renewal_attempt_actor_id", id_str.as_bytes())
                    .await
                    .map_err(|e| {
                        JsValue::from_str(&format!("Failed to persist renewal attempt actor: {e}"))
                    })?;
                kv.set("renewal_attempt_token_hash", &token_hash)
                    .await
                    .map_err(|e| {
                        JsValue::from_str(&format!("Failed to persist renewal attempt token: {e}"))
                    })?;
                attempt
            }
        };

        let response = match issuer
            .renew_credential(RenewCredentialRequest {
                actr_id: actor_id.clone(),
                renewal_token: token.into(),
                attempt_id,
            })
            .await
        {
            Ok(response) => response,
            Err(RenewError::TokenRejected | RenewError::InvalidRequest(_)) => {
                log::warn!(
                    "[SW] Issuer renew rejected persisted identity; clearing local identity"
                );
                Self::clear_persisted_identity_static(platform, cred_kv_ns).await?;
                return Ok(None);
            }
            Err(
                RenewError::AttemptExpired | RenewError::AttemptConflict | RenewError::Protocol(_),
            ) => {
                for key in [
                    "renewal_attempt_id",
                    "renewal_attempt_actor_id",
                    "renewal_attempt_token_hash",
                ] {
                    let _ = kv.delete(key).await;
                }
                return Err(JsValue::from_str(
                    "Issuer renewal attempt must be restarted with a new attempt ID",
                ));
            }
            Err(e) => return Err(JsValue::from_str(&format!("Issuer renewal failed: {e}"))),
        };

        let ok = match response.result {
            Some(renew_credential_response::Result::Success(ok)) => ok,
            Some(renew_credential_response::Result::Error(err)) => {
                log::warn!("[SW] Issuer renew error: {}", err.message);
                if matches!(err.code, 400 | 401) {
                    Self::clear_persisted_identity_static(platform, cred_kv_ns).await?;
                }
                return Ok(None);
            }
            None => {
                Self::clear_persisted_identity_static(platform, cred_kv_ns).await?;
                return Ok(None);
            }
        };

        if ok.actr_id != actor_id {
            return Err(JsValue::from_str(
                "Issuer renew returned a different ActrId",
            ));
        }

        let credential = ok.credential.clone();
        let turn_credential = Some(ok.turn_credential.clone());
        Self::persist_credentials_static(platform, cred_kv_ns, &actor_id, &credential).await?;
        if let Some(ref tc) = turn_credential {
            Self::persist_turn_credential_static(platform, cred_kv_ns, tc).await?;
        }
        if let (Some(token), Some(expires_at)) = (
            ok.renewal_token.as_ref(),
            ok.renewal_token_expires_at.as_ref(),
        ) {
            Self::persist_renewal_token_static(
                platform,
                cred_kv_ns,
                token.as_ref(),
                expires_at.seconds as u64,
            )
            .await?;
        }
        for key in [
            "renewal_attempt_id",
            "renewal_attempt_actor_id",
            "renewal_attempt_token_hash",
        ] {
            let _ = kv.delete(key).await;
        }

        Ok(Some((actor_id, credential, turn_credential)))
    }

    /// Try to restore a valid TurnCredential from IndexedDB (static version).
    async fn try_restore_turn_credential_static(
        platform: &WebPlatformProvider,
        cred_kv_ns: &StorageNamespace,
    ) -> Result<Option<actr_protocol::TurnCredential>, JsValue> {
        let kv = platform
            .secret_store(cred_kv_ns)
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to open KV store: {e}")))?;

        let Some(tc_bytes) = kv
            .get("turn_credential")
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to read TurnCredential: {e}")))?
        else {
            return Ok(None);
        };

        let tc = actr_protocol::TurnCredential::decode(&*tc_bytes).map_err(|e| {
            JsValue::from_str(&format!("Failed to decode persisted TurnCredential: {e}"))
        })?;

        // The issuer TTL is configurable and the credential carries no
        // authoritative issuance-time bound.
        let now_secs = (js_sys::Date::now() / 1000.0) as u64;
        if tc.expires_at <= now_secs {
            log::info!(
                "[SW] persisted TurnCredential expired (expires_at={}, now={})",
                tc.expires_at,
                now_secs
            );
            let _ = kv.delete("turn_credential").await;
            return Ok(None);
        }

        log::info!("[SW] TurnCredential restored from IndexedDB");
        Ok(Some(tc))
    }

    /// Send TurnCredential to the DOM so the WebRTC coordinator can
    /// include TURN credentials in ICE server configuration.
    fn send_turn_credential_to_dom(
        &self,
        tc: &actr_protocol::TurnCredential,
    ) -> Result<(), JsValue> {
        #[derive(Serialize)]
        struct TurnCredentialPayload {
            username: String,
            password: String,
        }

        let payload = TurnCredentialPayload {
            username: tc.username.clone(),
            password: tc.password.clone(),
        };

        let msg = SwMessage {
            msg_type: "update_turn_credential",
            payload,
        };

        let js_value = serde_wasm_bindgen::to_value(&msg).map_err(|e| {
            JsValue::from_str(&format!(
                "Failed to serialize TurnCredential message: {}",
                e
            ))
        })?;

        log::info!("[SW] Sending TurnCredential to DOM");
        self.send_dom_message(&js_value)
    }

    /// Try to restore valid credentials from IndexedDB (static version).
    async fn try_restore_credential_static(
        platform: &WebPlatformProvider,
        cred_kv_ns: &StorageNamespace,
    ) -> Result<Option<(ActrId, AIdCredential)>, JsValue> {
        let kv = platform
            .secret_store(cred_kv_ns)
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to open KV store: {e}")))?;

        let Some(cred_bytes) = kv
            .get("credential")
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to read credential: {e}")))?
        else {
            return Ok(None);
        };

        let Some(id_bytes) = kv
            .get("actor_id")
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to read actor_id: {e}")))?
        else {
            return Ok(None);
        };

        let credential = AIdCredential::decode(&*cred_bytes).map_err(|e| {
            JsValue::from_str(&format!("Failed to decode persisted credential: {e}"))
        })?;

        let claims = actr_protocol::IdentityClaims::decode(&*credential.claims)
            .map_err(|e| JsValue::from_str(&format!("Failed to decode identity claims: {e}")))?;

        let now_secs = (js_sys::Date::now() / 1000.0) as u64;
        // The issuer TTL is configurable and claims do not carry an issued-at
        // value, so only the signed absolute expiry can be evaluated here.
        if claims.expires_at <= now_secs {
            log::info!(
                "[SW] persisted credential expired (expires_at={}, now={})",
                claims.expires_at,
                now_secs
            );
            // Keep the expired credential as the binding of the renewal lease.
            return Ok(None);
        }

        let id_str = String::from_utf8(id_bytes)
            .map_err(|e| JsValue::from_str(&format!("Invalid actor_id UTF-8: {e}")))?;
        let actor_id = ActrId::from_string_repr(&id_str)
            .map_err(|e| JsValue::from_str(&format!("Failed to parse actor_id: {e}")))?;

        Ok(Some((actor_id, credential)))
    }

    /// KV namespace for persisting credentials across SW restarts.
    ///
    /// TD-004 fix: the namespace is keyed by `{client_actr_type}_{client_id}`.
    /// Rationale: within a same-origin Service Worker, multiple client pages
    /// share the SW instance. If the namespace only included `actr_type`, a
    /// second client page would restore the first client's credential from
    /// IndexedDB, both WS sessions would land on the same `actor_id`, and the
    /// mock-actrix registry rebind would steal the relay binding from the
    /// original client. Partitioning by `client_id` guarantees every client
    /// page performs its own Issuer registration and persists its own credential.
    fn cred_kv_namespace(&self) -> StorageNamespace {
        Self::build_cred_kv_namespace(&self.client_actr_type, &self.client_id)
    }

    /// Build a credential KV namespace for a given `(actr_type, client_id)` pair.
    ///
    /// Exposed as a static helper so that `SwRuntime::new()` can build the
    /// namespace before `self` exists without duplicating the encoding.
    ///
    /// The identity fields and `client_id` are hashed into the opaque namespace
    /// using a domain-separated, tagged, length-prefixed encoding, so distinct
    /// field splits never collide — e.g. `("acme:Echo:1_a", "b")` vs
    /// `("acme/Echo@1", "a_b")` would map to the same string under a
    /// delimiter-joined encoding. Per-tab partitioning is preserved while the raw
    /// identity stays out of any storage-location string.
    fn build_cred_kv_namespace(client_actr_type: &ActrType, client_id: &str) -> StorageNamespace {
        const DOMAIN: &[u8] = b"actr-web-cred-namespace\0v1";
        fn field(h: &mut Sha256, tag: u8, bytes: &[u8]) {
            h.update([tag]);
            h.update((bytes.len() as u32).to_be_bytes());
            h.update(bytes);
        }
        let mut h = Sha256::new();
        h.update(DOMAIN);
        field(&mut h, 1, client_actr_type.publisher.as_bytes());
        field(&mut h, 2, client_actr_type.name.as_bytes());
        field(&mut h, 3, client_actr_type.version.as_bytes());
        field(&mut h, 4, client_id.as_bytes());
        let digest: [u8; 32] = h.finalize().into();
        StorageNamespace::actr_v1(digest)
    }

    /// Reconnect the signaling WebSocket after detecting a dead connection.
    ///
    /// New flow: obtain credential via Issuer HTTP first, then connect signaling
    /// with credential in URL.
    async fn reconnect_signaling(&mut self) -> Result<(), JsValue> {
        log::info!(
            "[SW] [{}] Reconnecting signaling WebSocket...",
            self.client_id
        );

        // 1. Close old signaling (idempotent — may already be closed).
        self.signaling.close();

        // 2. Clear stale peer / target state so the next RPC forces
        //    a fresh discovery & WebRTC handshake.
        self.clear_peer_state_for_signaling_reconnect();

        // 3. Obtain credential: try restore from IndexedDB, else re-register via Issuer HTTP.
        let cred_kv_ns = self.cred_kv_namespace();
        match Self::try_restore_credential_static(&self.platform, &cred_kv_ns).await {
            Ok(Some((actor_id, credential))) => {
                log::info!("[SW] restored credentials from IndexedDB, skipping Issuer re-register");
                self.actor_id = Some(actor_id);
                self.credential = Some(credential);
                // Also try to restore TurnCredential
                if let Ok(tc) =
                    Self::try_restore_turn_credential_static(&self.platform, &cred_kv_ns).await
                {
                    self.turn_credential = tc;
                }
            }
            _ => {
                self.register_via_issuer().await?;
            }
        }

        // Send updated TurnCredential to DOM for new peer connections
        if let Some(ref tc) = self.turn_credential {
            if let Err(e) = self.send_turn_credential_to_dom(tc) {
                log::warn!(
                    "[SW] Failed to send TurnCredential to DOM on reconnect: {:?}",
                    e
                );
            }
        }

        // 4. Build signaling URL with credential and connect.
        self.connect_signaling_with_current_identity().await?;

        log::info!(
            "[SW] [{}] Signaling reconnected (Issuer HTTP credential)",
            self.client_id
        );
        Ok(())
    }

    fn clear_peer_state_for_signaling_reconnect(&mut self) {
        self.discovered_targets.clear();
        self.known_peers.clear();
        self.open_channels.clear();
        self.pending_channel_data.clear();
        self.role_negotiated.clear();
        self.role_assignments.clear();
        self.pending_local_sdp_exchanges.clear();
        self.pending_remote_sdp_exchanges.clear();
        self.ice_restart_inflight.clear();
        self.ice_restart_attempts.clear();
        self.peer_connection_states.clear();
    }

    async fn connect_signaling_with_current_identity(&mut self) -> Result<(), JsValue> {
        let url_with_cred = self.build_signaling_url_with_identity();
        self.signaling =
            SignalingClient::connect_with_retries(&url_with_cred, &self.reconnect_config).await?;
        Ok(())
    }

    async fn renew_current_credential(&mut self) -> Result<(), JsValue> {
        let cred_kv_ns = self.cred_kv_namespace();

        let Some((actor_id, credential, turn_credential)) =
            Self::try_renew_persisted_credential_static(&self.issuer, &self.platform, &cred_kv_ns)
                .await?
        else {
            log::warn!("[SW] renewal token unavailable; falling back to Issuer registration");
            self.register_via_issuer().await?;
            if let Some(ref tc) = self.turn_credential {
                self.send_turn_credential_to_dom(tc)?;
            }
            self.signaling.close();
            self.clear_peer_state_for_signaling_reconnect();
            self.connect_signaling_with_current_identity().await?;
            log::info!(
                "[SW] [{}] signaling reconnected after Issuer re-registration",
                self.client_id
            );
            return Ok(());
        };

        self.actor_id = Some(actor_id);
        self.credential = Some(credential);
        self.turn_credential = turn_credential;
        if let Some(ref tc) = self.turn_credential {
            self.send_turn_credential_to_dom(tc)?;
        }

        Ok(())
    }

    /// Send a signaling heartbeat (Ping) to keep the connection alive
    /// and the service registration active.
    async fn send_heartbeat(&self) -> Result<(), JsValue> {
        let actor_id = self
            .actor_id
            .clone()
            .ok_or_else(|| JsValue::from_str("Actor not registered"))?;
        let credential = self
            .credential
            .clone()
            .ok_or_else(|| JsValue::from_str("Missing credential"))?;

        let ping = Ping {
            availability: ServiceAvailabilityState::Full as i32,
            power_reserve: 0.01,
            mailbox_backlog: 0.0,
            sticky_client_ids: vec![],
        };
        let envelope = SignalingEnvelope {
            envelope_version: SIGNALING_ENVELOPE_VERSION,
            envelope_id: self.signaling.next_envelope_id(),
            reply_for: None,
            traceparent: None,
            tracestate: None,
            flow: Some(signaling_envelope::Flow::ActrToServer(ActrToSignaling {
                source: actor_id,
                credential,
                payload: Some(actr_to_signaling::Payload::Ping(ping)),
            })),
        };
        let response = self.signaling.send_request(envelope).await?;
        match response.flow {
            Some(signaling_envelope::Flow::ServerToActr(msg)) => match msg.payload {
                Some(signaling_to_actr::Payload::Pong(_)) => {}
                Some(signaling_to_actr::Payload::Error(err)) if err.code == 401 => {
                    return Err(JsValue::from_str("signaling credential expired"));
                }
                Some(signaling_to_actr::Payload::Error(err)) => {
                    return Err(JsValue::from_str(&format!(
                        "signaling heartbeat error {}: {}",
                        err.code, err.message
                    )));
                }
                _ => {
                    return Err(JsValue::from_str(
                        "signaling heartbeat response was not Pong",
                    ));
                }
            },
            _ => {
                return Err(JsValue::from_str(
                    "invalid signaling heartbeat response flow",
                ));
            }
        }
        log::debug!("[SW] heartbeat sent");
        Ok(())
    }

    /// Discover a peer of `self.target_actr_type` via the Issuer route-candidates API.
    ///
    /// Returns a cached `ActrId` if a previous discovery for the same target type
    /// succeeded and the associated peer has not been torn down. Otherwise issues
    /// a fresh `RouteCandidatesRequest`.
    ///
    /// Intended for the active / initiator role: callers that need to open an
    /// outbound connection. Passive actors never call this path; they learn the
    /// remote peer identity from inbound signaling envelopes (`relay.source`).
    async fn discover_target(&mut self) -> Result<ActrId, JsValue> {
        let target_type = self.target_actr_type.clone().ok_or_else(|| {
            JsValue::from_str(
                "No default target_actr_type is configured; resolve a dependency alias and discover that ActrType instead",
            )
        })?;
        let target_type_key = target_type.to_string_repr();
        if let Some(target) = self.discovered_targets.get(&target_type_key).cloned() {
            return Ok(target);
        }
        let actor_id = self
            .actor_id
            .clone()
            .ok_or_else(|| JsValue::from_str("Actor not registered"))?;
        let credential = self
            .credential
            .clone()
            .ok_or_else(|| JsValue::from_str("Missing credential"))?;

        log::info!("[SW] route_candidates: request target_type={}", target_type);
        let criteria = route_candidates_request::NodeSelectionCriteria {
            candidate_count: 1,
            ranking_factors: vec![route_candidates_request::node_selection_criteria::NodeRankingFactor::MaximumPowerReserve as i32],
            minimal_dependency_requirement: None,
            minimal_health_requirement: None,
        };
        let request = RouteCandidatesRequest {
            target_type,
            criteria: Some(criteria),
            client_location: None,
            client_fingerprint: self.service_fingerprint.clone(),
        };

        let envelope = SignalingEnvelope {
            envelope_version: SIGNALING_ENVELOPE_VERSION,
            envelope_id: self.signaling.next_envelope_id(),
            reply_for: None,
            traceparent: None,
            tracestate: None,
            flow: Some(signaling_envelope::Flow::ActrToServer(ActrToSignaling {
                source: actor_id,
                credential,
                payload: Some(
                    actr_protocol::actr_to_signaling::Payload::RouteCandidatesRequest(request),
                ),
            })),
        };

        let response = self.signaling.send_request(envelope).await?;
        log::info!("[SW] route_candidates: got response");
        let route_response = match response.flow {
            Some(signaling_envelope::Flow::ServerToActr(server_to_actr)) => {
                match server_to_actr.payload {
                    Some(signaling_to_actr::Payload::RouteCandidatesResponse(resp)) => resp,
                    _ => {
                        return Err(JsValue::from_str(
                            "Unexpected signaling route response payload",
                        ));
                    }
                }
            }
            _ => {
                return Err(JsValue::from_str(
                    "Unexpected signaling route response flow",
                ));
            }
        };

        match route_response.result {
            Some(actr_protocol::route_candidates_response::Result::Success(ok)) => {
                let target = ok
                    .candidates
                    .first()
                    .cloned()
                    .ok_or_else(|| JsValue::from_str("No candidates"))?;
                log::info!("[SW] route_candidates: selected {}", target);
                self.discovered_targets
                    .insert(target_type_key, target.clone());
                Ok(target)
            }
            Some(actr_protocol::route_candidates_response::Result::Error(err)) => {
                log::warn!("[SW] route_candidates: error {}", err.message);
                Err(JsValue::from_str(&format!(
                    "Route candidates error: {}",
                    err.message
                )))
            }
            None => Err(JsValue::from_str("Route candidates missing result")),
        }
    }

    /// Drop any `discovered_targets` entry whose resolved `ActrId` matches
    /// the given peer identifier. Used when a peer connection fails/closes,
    /// so the next outbound RPC issues a fresh Issuer discovery rather than
    /// re-using the dead `ActrId`.
    fn invalidate_discovered_target(&mut self, peer_id: &str) {
        let before = self.discovered_targets.len();
        self.discovered_targets
            .retain(|_, actr_id| actr_id.to_string_repr() != peer_id);
        if self.discovered_targets.len() != before {
            log::info!(
                "[SW] discovered_targets: invalidated entry for peer={}",
                peer_id
            );
        }
    }

    /// Discover a target actor using an explicit `ActrType`.
    ///
    /// Similar to `discover_target`, but allows the caller to provide the
    /// target type instead of relying on the configured default.
    async fn discover_target_for_type(
        &mut self,
        target_type: &ActrType,
    ) -> Result<ActrId, JsValue> {
        let actor_id = self
            .actor_id
            .clone()
            .ok_or_else(|| JsValue::from_str("Actor not registered"))?;
        let credential = self
            .credential
            .clone()
            .ok_or_else(|| JsValue::from_str("Missing credential"))?;

        log::info!(
            "[SW] discover_target_for_type: request target_type={}",
            target_type
        );
        let criteria = route_candidates_request::NodeSelectionCriteria {
            candidate_count: 1,
            ranking_factors: vec![
                route_candidates_request::node_selection_criteria::NodeRankingFactor::ExactMatchFirst as i32,
                route_candidates_request::node_selection_criteria::NodeRankingFactor::MaximumPowerReserve as i32,
            ],
            minimal_dependency_requirement: None,
            minimal_health_requirement: None,
        };
        let request = RouteCandidatesRequest {
            target_type: target_type.clone(),
            criteria: Some(criteria),
            client_location: None,
            client_fingerprint: crate::dependency_aliases::fingerprint_for(target_type)
                .unwrap_or_default(),
        };

        let envelope = SignalingEnvelope {
            envelope_version: SIGNALING_ENVELOPE_VERSION,
            envelope_id: self.signaling.next_envelope_id(),
            reply_for: None,
            traceparent: None,
            tracestate: None,
            flow: Some(signaling_envelope::Flow::ActrToServer(ActrToSignaling {
                source: actor_id,
                credential,
                payload: Some(
                    actr_protocol::actr_to_signaling::Payload::RouteCandidatesRequest(request),
                ),
            })),
        };

        let response = self.signaling.send_request(envelope).await?;
        let route_response = match response.flow {
            Some(signaling_envelope::Flow::ServerToActr(server_to_actr)) => {
                match server_to_actr.payload {
                    Some(signaling_to_actr::Payload::RouteCandidatesResponse(resp)) => resp,
                    _ => {
                        return Err(JsValue::from_str(
                            "Unexpected signaling route response payload",
                        ));
                    }
                }
            }
            _ => {
                return Err(JsValue::from_str(
                    "Unexpected signaling route response flow",
                ));
            }
        };

        match route_response.result {
            Some(actr_protocol::route_candidates_response::Result::Success(ok)) => {
                let target = ok
                    .candidates
                    .first()
                    .cloned()
                    .ok_or_else(|| JsValue::from_str("No candidates"))?;
                log::info!("[SW] discover_target_for_type: selected {}", target);
                Ok(target)
            }
            Some(actr_protocol::route_candidates_response::Result::Error(err)) => Err(
                JsValue::from_str(&format!("Route candidates error: {}", err.message)),
            ),
            None => Err(JsValue::from_str("Route candidates missing result")),
        }
    }

    fn send_dom_message(&self, msg: &JsValue) -> Result<(), JsValue> {
        let port = self
            .dom_port
            .as_ref()
            .ok_or_else(|| JsValue::from_str("DOM port not set"))?;
        port.post_message(msg)?;
        Ok(())
    }

    fn send_webrtc_command(
        &self,
        action: &str,
        peer_id: &str,
        payload: JsValue,
    ) -> Result<(), JsValue> {
        log::info!("[SW] webrtc_command: {} peer={}", action, peer_id);

        let response = SwMessage {
            msg_type: "webrtc_command",
            payload: WebRtcCommandPayload {
                action: action.to_string(),
                peer_id: peer_id.to_string(),
                payload,
            },
        };

        let msg_js_value = serde_wasm_bindgen::to_value(&response).map_err(|e| {
            JsValue::from_str(&format!("Failed to serialize WebRTC command: {}", e))
        })?;

        self.send_dom_message(&msg_js_value)
    }

    async fn send_ice_restart_rejected(
        &self,
        target: &ActrId,
        sdp_exchange_id: &str,
    ) -> Result<(), JsValue> {
        let actor_id = self
            .actor_id
            .clone()
            .ok_or_else(|| JsValue::from_str("Actor not registered"))?;
        let credential = self
            .credential
            .clone()
            .ok_or_else(|| JsValue::from_str("Missing credential"))?;
        let relay = actr_protocol::ActrRelay {
            source: actor_id,
            credential,
            target: target.clone(),
            delivery_context: None,
            source_type: None,
            payload: Some(actr_relay::Payload::IceRestartRejected(
                IceRestartRejected {
                    sdp_exchange_id: sdp_exchange_id.to_string(),
                    reason: Some(ice_restart_rejected::Reason::NoActiveSession as i32),
                },
            )),
        };
        let envelope = SignalingEnvelope {
            envelope_version: SIGNALING_ENVELOPE_VERSION,
            envelope_id: self.signaling.next_envelope_id(),
            reply_for: None,
            traceparent: None,
            tracestate: None,
            flow: Some(signaling_envelope::Flow::ActrRelay(relay)),
        };
        self.signaling.send_envelope(envelope).await
    }

    async fn send_role_negotiation(&self, target: &ActrId) -> Result<(), JsValue> {
        let actor_id = self
            .actor_id
            .clone()
            .ok_or_else(|| JsValue::from_str("Actor not registered"))?;
        let credential = self
            .credential
            .clone()
            .ok_or_else(|| JsValue::from_str("Missing credential"))?;
        log::info!("[SW] role_negotiation: from={} to={}", actor_id, target);
        let payload = actr_relay::Payload::RoleNegotiation(RoleNegotiation {
            from: actor_id.clone(),
            to: target.clone(),
        });
        let relay = actr_protocol::ActrRelay {
            source: actor_id,
            credential,
            target: target.clone(),
            delivery_context: None,
            source_type: None,
            payload: Some(payload),
        };
        let envelope = SignalingEnvelope {
            envelope_version: SIGNALING_ENVELOPE_VERSION,
            envelope_id: self.signaling.next_envelope_id(),
            reply_for: None,
            traceparent: None,
            tracestate: None,
            flow: Some(signaling_envelope::Flow::ActrRelay(relay)),
        };
        self.signaling.send_envelope(envelope).await
    }

    async fn ensure_peer(&mut self) -> Result<String, JsValue> {
        let target = self.discover_target().await?;
        let peer_id = target.to_string_repr();
        request_relay_peer(&self.client_id, &target);

        if self.peer_requires_reconnect(&peer_id) {
            log::warn!(
                "[SW] ensure_peer: reconnecting stale peer={} state={:?}",
                peer_id,
                self.peer_connection_states.get(&peer_id)
            );
            self.reset_peer_for_reconnect(&peer_id);
        }

        if !self.known_peers.contains(&peer_id) {
            self.send_webrtc_command("create_peer", &peer_id, JsValue::NULL)?;
            self.known_peers.insert(peer_id.clone());
        }
        if !self.role_negotiated.contains(&peer_id) {
            self.send_role_negotiation(&target).await?;
            self.role_negotiated.insert(peer_id.clone());
        }
        Ok(peer_id)
    }

    /// Ensure a peer connection is initiated, with retry and exponential backoff.
    ///
    /// Mirrors the `create_connection_inner` retry logic from the native `actr`
    /// runtime: retries `ensure_peer()` up to `P2P_CONNECTION_MAX_RETRIES` times
    /// with exponential backoff. This handles transient failures such as:
    /// - Target actor not yet registered ("No candidates")
    /// - Signaling network errors during role negotiation
    async fn ensure_peer_with_retry(&mut self) -> Result<String, JsValue> {
        let mut backoff =
            ExponentialBackoff::new(P2P_RETRY_INITIAL_DELAY_MS, P2P_RETRY_MAX_DELAY_MS)
                .with_multiplier(2.0)
                .with_jitter(0.1);

        let mut attempt: u32 = 0;

        loop {
            attempt += 1;

            match self.ensure_peer().await {
                Ok(peer_id) => {
                    if attempt > 1 {
                        log::info!("[SW] P2P peer established after {} attempts", attempt);
                    }
                    return Ok(peer_id);
                }
                Err(e) => {
                    log::warn!(
                        "[SW] P2P connection attempt {}/{} failed: {:?}",
                        attempt,
                        P2P_CONNECTION_MAX_RETRIES,
                        e
                    );

                    if attempt >= P2P_CONNECTION_MAX_RETRIES {
                        log::error!(
                            "[SW] P2P connection failed after {} attempts, giving up",
                            attempt
                        );
                        return Err(e);
                    }

                    let delay = backoff.next_delay();
                    let delay_ms = delay.as_millis() as u32;
                    log::info!("[SW] Retry P2P connection after {}ms", delay_ms);
                    TimeoutFuture::new(delay_ms).await;
                }
            }
        }
    }

    /// Discover the target actor with retry and exponential backoff.
    ///
    /// This is a standalone retry wrapper for `discover_target()`, used by
    /// callers that need discovery but not the full `ensure_peer()` flow.
    /// Handles the common case where the server actor hasn't registered yet.
    async fn discover_target_with_retry(&mut self) -> Result<ActrId, JsValue> {
        let mut backoff =
            ExponentialBackoff::new(P2P_RETRY_INITIAL_DELAY_MS, P2P_RETRY_MAX_DELAY_MS)
                .with_multiplier(2.0)
                .with_jitter(0.1);

        let mut attempt: u32 = 0;

        loop {
            attempt += 1;

            match self.discover_target().await {
                Ok(target) => {
                    if attempt > 1 {
                        log::info!("[SW] Target discovered after {} attempts", attempt);
                    }
                    return Ok(target);
                }
                Err(e) => {
                    log::warn!(
                        "[SW] Target discovery attempt {}/{} failed: {:?}",
                        attempt,
                        P2P_CONNECTION_MAX_RETRIES,
                        e
                    );

                    if attempt >= P2P_CONNECTION_MAX_RETRIES {
                        log::error!(
                            "[SW] Target discovery failed after {} attempts, giving up",
                            attempt
                        );
                        return Err(e);
                    }

                    let delay = backoff.next_delay();
                    let delay_ms = delay.as_millis() as u32;
                    log::info!("[SW] Retry target discovery after {}ms", delay_ms);
                    TimeoutFuture::new(delay_ms).await;
                }
            }
        }
    }

    /// Register a pending RPC (callable from RuntimeContext for internal calls)
    fn register_pending_rpc(&mut self, request_id: String, target: PendingRpcTarget) {
        self.pending_rpcs.insert(request_id, target);
    }

    /// Handle an RPC response received from a remote peer.
    ///
    /// Documented flow:
    /// `remote response -> handle_fast_path -> handle_rpc_response`
    /// `-> System.handle_remote_response -> HostGate.handle_response -> DOM response`
    fn handle_rpc_response(
        &mut self,
        envelope: RpcEnvelope,
        peer_id: &str,
        channel_id: u32,
        stream_id: &str,
    ) -> Result<(), JsValue> {
        let request_id = envelope.request_id.clone();

        // Check whether this response belongs to a request we sent.
        let Some(rpc_target) = self.pending_rpcs.remove(&request_id) else {
            log::warn!(
                "[SW] rpc.orphan_response_dropped: envelope marked Response has no pending request request_id={} peer={} channel={} stream_id={} route_key={}; dropping (late reply or mislabeled request)",
                request_id,
                peer_id,
                channel_id,
                stream_id,
                envelope.route_key,
            );
            return Ok(());
        };

        log::info!(
            "[SW] handle_rpc_response: request_id={} target={:?}",
            request_id,
            rpc_target
        );

        // Extract the payload for downstream handling.
        let payload_bytes = envelope
            .payload
            .as_ref()
            .map(|p| p.to_vec())
            .unwrap_or_default();

        // Let `System` process the response.
        // This triggers `HostGate.handle_response()`.
        CLIENTS.with(|cell| {
            if let Some(ctx) = cell.borrow().get(&self.client_id) {
                ctx.system
                    .handle_remote_response(&request_id, Bytes::from(payload_bytes.clone()));
            } else {
                log::warn!(
                    "[SW] Client context not found for client_id={}",
                    self.client_id
                );
            }
        });

        // Only send to DOM for DOM-originated requests
        match rpc_target {
            PendingRpcTarget::Dom => {
                let js_payload: JsValue = if payload_bytes.is_empty() {
                    JsValue::NULL
                } else {
                    js_sys::Uint8Array::from(payload_bytes.as_slice()).into()
                };

                let response = SwMessage {
                    msg_type: "control_response",
                    payload: RpcResponsePayload {
                        request_id,
                        data: js_payload,
                        error: envelope.error.map(|e| RpcError {
                            code: e.code as i32,
                            message: e.message,
                        }),
                    },
                };

                let msg_js_value = serde_wasm_bindgen::to_value(&response).map_err(|e| {
                    JsValue::from_str(&format!("Failed to serialize RPC response: {}", e))
                })?;

                self.send_dom_message(&msg_js_value)?;
            }
            PendingRpcTarget::Internal => {
                // Internal (handler-initiated) RPCs: response handled via System/HostGate only
                log::debug!(
                    "[SW] Internal RPC response handled: request_id={}",
                    request_id
                );
            }
        }

        Ok(())
    }

    #[allow(dead_code)]
    async fn handle_inbound_signaling(&mut self) -> Result<(), JsValue> {
        while let Some(env) = self.signaling.recv_inbound().await {
            if let Some(signaling_envelope::Flow::ActrRelay(relay)) = env.flow {
                self.handle_actr_relay(relay).await?;
            }
        }
        Ok(())
    }

    fn new_sdp_exchange_id(&self) -> String {
        self.signaling.next_envelope_id()
    }

    async fn handle_actr_relay(&mut self, relay: actr_protocol::ActrRelay) -> Result<(), JsValue> {
        let peer_id = relay.source.to_string_repr();
        match relay.payload {
            Some(actr_relay::Payload::SessionDescription(sd)) => {
                log::info!(
                    "[SW] relay: session_description from={} type={}",
                    peer_id,
                    sd.r#type
                );

                // Check if this is an ICE restart offer (type=3)
                let is_ice_restart = sd.r#type == session_description::Type::IceRestartOffer as i32;
                // Reject only when we cannot answer the restart at all: the peer
                // is unknown, or its PeerConnection is already closed. `failed`
                // is deliberately NOT rejected — an ICE-restart answer is how a
                // failed connection recovers, so it falls through to the normal
                // offer/answer path below.
                let peer_is_closed = matches!(
                    self.peer_connection_states
                        .get(&peer_id)
                        .map(String::as_str),
                    Some("closed")
                );
                if is_ice_restart && (!self.known_peers.contains(&peer_id) || peer_is_closed) {
                    let Some(exchange_id) = sd.sdp_exchange_id.as_deref() else {
                        log::warn!(
                            "[SW] Ignoring ICE restart offer from={} without sdp_exchange_id",
                            peer_id
                        );
                        return Ok(());
                    };
                    log::warn!(
                        "[SW] Rejecting ICE restart offer from={} exchange_id={}: no active peer session",
                        peer_id,
                        exchange_id
                    );
                    self.send_ice_restart_rejected(&relay.source, exchange_id)
                        .await?;
                    return Ok(());
                }

                let sdp_type = match sd.r#type {
                    0 => "offer",  // OFFER
                    1 => "answer", // ANSWER
                    2 => "offer",  // RENEGOTIATION_OFFER (treated as offer)
                    3 => "offer",  // ICE_RESTART_OFFER (treated as offer for WebRTC API)
                    _ => "offer",
                };

                let remote_sdp_exchange_id = if sd.r#type
                    == session_description::Type::Answer as i32
                {
                    let Some(exchange_id) = sd.sdp_exchange_id.as_deref() else {
                        log::warn!(
                            "[SW] Ignoring SDP answer from={} without sdp_exchange_id",
                            peer_id
                        );
                        return Ok(());
                    };
                    match self.pending_local_sdp_exchanges.get(&peer_id) {
                        Some(current) if current == exchange_id => {
                            self.pending_local_sdp_exchanges.remove(&peer_id);
                        }
                        Some(current) => {
                            log::warn!(
                                "[SW] Ignoring stale SDP answer from={} exchange_id={} current={}",
                                peer_id,
                                exchange_id,
                                current
                            );
                            return Ok(());
                        }
                        None => {
                            log::warn!(
                                "[SW] Ignoring SDP answer from={} with no pending local exchange",
                                peer_id
                            );
                            return Ok(());
                        }
                    }
                    None
                } else if sdp_type == "offer" {
                    let Some(exchange_id) = sd.sdp_exchange_id.clone() else {
                        log::warn!(
                            "[SW] Ignoring SDP offer from={} without sdp_exchange_id",
                            peer_id
                        );
                        return Ok(());
                    };
                    if let Some(previous_exchange_id) = self
                        .pending_remote_sdp_exchanges
                        .insert(peer_id.clone(), exchange_id.clone())
                    {
                        if previous_exchange_id != exchange_id {
                            log::warn!(
                                "[SW] Replacing pending remote SDP exchange for peer={} previous={} current={}",
                                peer_id,
                                previous_exchange_id,
                                exchange_id
                            );
                        }
                    }
                    Some(exchange_id)
                } else {
                    None
                };

                // Ensure peer exists before setting remote description
                if !self.known_peers.contains(&peer_id) {
                    self.send_webrtc_command("create_peer", &peer_id, JsValue::NULL)?;
                    self.known_peers.insert(peer_id.clone());
                }

                let sdp_obj = Object::new();
                Reflect::set(
                    &sdp_obj,
                    &JsValue::from_str("type"),
                    &JsValue::from_str(sdp_type),
                )?;
                Reflect::set(
                    &sdp_obj,
                    &JsValue::from_str("sdp"),
                    &JsValue::from_str(&sd.sdp),
                )?;

                let payload = Object::new();
                Reflect::set(&payload, &JsValue::from_str("sdp"), &sdp_obj)?;
                if let Some(exchange_id) = remote_sdp_exchange_id.as_deref() {
                    Reflect::set(
                        &payload,
                        &JsValue::from_str("sdpExchangeId"),
                        &JsValue::from_str(exchange_id),
                    )?;
                }

                self.send_webrtc_command("set_remote_description", &peer_id, payload.into())?;

                if sdp_type == "offer" {
                    if is_ice_restart {
                        log::info!(
                            "[SW] ICE restart offer received from={}, creating answer",
                            peer_id
                        );
                    }
                    let answer_payload = Object::new();
                    if let Some(exchange_id) = remote_sdp_exchange_id.as_deref() {
                        Reflect::set(
                            &answer_payload,
                            &JsValue::from_str("sdpExchangeId"),
                            &JsValue::from_str(exchange_id),
                        )?;
                    }
                    self.send_webrtc_command("create_answer", &peer_id, answer_payload.into())?;
                }

                // If we received an answer during ICE restart, mark restart as complete
                if sd.r#type == session_description::Type::Answer as i32 {
                    if self.ice_restart_inflight.get(&peer_id) == Some(&true) {
                        log::info!(
                            "[SW] ICE restart: received answer from={}, waiting for datachannel reopen",
                            peer_id
                        );
                        // The restart will be fully complete when datachannel re-opens
                        // (handled in datachannel_open event)
                    }
                }
            }
            Some(actr_relay::Payload::IceCandidate(ice)) => {
                log::info!("[SW] relay: ice_candidate from={}", peer_id);
                let candidate = Object::new();
                Reflect::set(
                    &candidate,
                    &JsValue::from_str("candidate"),
                    &JsValue::from_str(&ice.candidate),
                )?;
                if let Some(mid) = ice.sdp_mid {
                    Reflect::set(
                        &candidate,
                        &JsValue::from_str("sdpMid"),
                        &JsValue::from_str(&mid),
                    )?;
                }
                if let Some(mline) = ice.sdp_mline_index {
                    Reflect::set(
                        &candidate,
                        &JsValue::from_str("sdpMLineIndex"),
                        &JsValue::from_f64(mline as f64),
                    )?;
                }
                if let Some(ufrag) = ice.username_fragment {
                    Reflect::set(
                        &candidate,
                        &JsValue::from_str("usernameFragment"),
                        &JsValue::from_str(&ufrag),
                    )?;
                }

                let payload = Object::new();
                Reflect::set(&payload, &JsValue::from_str("candidate"), &candidate)?;

                self.send_webrtc_command("add_ice_candidate", &peer_id, payload.into())?;
            }
            Some(actr_relay::Payload::RoleAssignment(assign)) => {
                // For RoleAssignment, relay.source may be our own ID (echoed back
                // by the signaling server). Determine the actual remote peer:
                // if source == self, use relay.target; otherwise use relay.source.
                let is_self = self
                    .actor_id
                    .as_ref()
                    .map(|id| id.to_string_repr() == peer_id)
                    .unwrap_or(false);
                let remote_peer_id = if is_self {
                    relay.target.to_string_repr()
                } else {
                    peer_id.clone()
                };

                log::info!(
                    "[SW] relay: role_assignment remote_peer={} is_offerer={}",
                    remote_peer_id,
                    assign.is_offerer
                );
                self.role_assignments
                    .insert(remote_peer_id.clone(), assign.is_offerer);
                if !self.known_peers.contains(&remote_peer_id) {
                    self.send_webrtc_command("create_peer", &remote_peer_id, JsValue::NULL)?;
                    self.known_peers.insert(remote_peer_id.clone());
                }
                if assign.is_offerer {
                    self.send_webrtc_command("create_offer", &remote_peer_id, JsValue::NULL)?;
                }
            }
            Some(actr_relay::Payload::IceRestartRejected(rejection)) => {
                let is_offerer = self
                    .role_assignments
                    .get(&peer_id)
                    .copied()
                    .unwrap_or(false);
                let restart_inflight = self.ice_restart_inflight.get(&peer_id) == Some(&true);
                let pending_exchange_id = self
                    .pending_local_sdp_exchanges
                    .get(&peer_id)
                    .map(String::as_str);
                if !is_correlated_restart_rejection(
                    is_offerer,
                    restart_inflight,
                    pending_exchange_id,
                    &rejection,
                ) {
                    log::warn!(
                        "[SW] Ignoring stale or role-invalid ICE restart rejection from={} exchange_id={} reason={:?}",
                        peer_id,
                        rejection.sdp_exchange_id,
                        rejection.reason
                    );
                    return Ok(());
                }

                log::warn!(
                    "[SW] ICE restart rejected by peer={} exchange_id={}: rebuilding on next request",
                    peer_id,
                    rejection.sdp_exchange_id
                );
                self.handle_stale_peer_failure(
                    &peer_id,
                    "ice_restart_rejected",
                    "remote peer has no active session",
                )
                .await?;
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_webrtc_event(&mut self, event: DomWebRtcEvent) -> Result<(), JsValue> {
        let actor_id = self
            .actor_id
            .clone()
            .ok_or_else(|| JsValue::from_str("Actor not registered"))?;
        let credential = self
            .credential
            .clone()
            .ok_or_else(|| JsValue::from_str("Missing credential"))?;

        log::info!("[SW] webrtc_event: {}", event.event_type);

        // Resolve the signaling target from the `peerId` embedded in every DOM
        // webrtc event. This identity is always known by the time a local SDP
        // or ICE candidate fires because the SW drove the `create_peer` command
        // (for initiators) or learned it from an inbound relay (for acceptors).
        let resolve_target = |event_data: &serde_json::Value| -> Result<ActrId, JsValue> {
            if let Some(peer_id_str) = event_data.get("peerId").and_then(|v| v.as_str()) {
                ActrId::from_string_repr(peer_id_str).map_err(|e| {
                    JsValue::from_str(&format!("Invalid peerId '{}': {}", peer_id_str, e))
                })
            } else {
                Err(JsValue::from_str("Missing peerId in event data"))
            }
        };

        match event.event_type.as_str() {
            "local_description" => {
                let data: LocalDescriptionEvent = serde_json::from_value(event.data.clone())
                    .map_err(|e| JsValue::from_str(&e.to_string()))?;

                let target = resolve_target(&event.data)?;

                let sd_type = match data.sdp.sdp_type.as_str() {
                    "offer" => session_description::Type::Offer as i32,
                    "answer" => session_description::Type::Answer as i32,
                    _ => session_description::Type::Offer as i32,
                };
                let sdp_exchange_id = if sd_type == session_description::Type::Answer as i32 {
                    let Some(exchange_id) = data.sdp_exchange_id.clone() else {
                        log::warn!(
                            "[SW] Not sending SDP answer to peer={} without sdp_exchange_id",
                            data.peer_id
                        );
                        return Ok(());
                    };
                    match self.pending_remote_sdp_exchanges.get(&data.peer_id) {
                        Some(current) if current == &exchange_id => {
                            self.pending_remote_sdp_exchanges.remove(&data.peer_id);
                        }
                        Some(current) => {
                            log::warn!(
                                "[SW] Not sending stale SDP answer to peer={} exchange_id={} current={}",
                                data.peer_id,
                                exchange_id,
                                current
                            );
                            return Ok(());
                        }
                        None => {
                            log::warn!(
                                "[SW] Not sending SDP answer to peer={} without pending remote exchange",
                                data.peer_id
                            );
                            return Ok(());
                        }
                    }
                    exchange_id
                } else {
                    let exchange_id = self.new_sdp_exchange_id();
                    self.pending_local_sdp_exchanges
                        .insert(data.peer_id.clone(), exchange_id.clone());
                    exchange_id
                };
                let sd = SessionDescription {
                    r#type: sd_type,
                    sdp: data.sdp.sdp,
                    sdp_exchange_id: Some(sdp_exchange_id),
                };
                let relay = actr_protocol::ActrRelay {
                    source: actor_id,
                    credential,
                    target,
                    delivery_context: None,
                    source_type: None,
                    payload: Some(actr_relay::Payload::SessionDescription(sd)),
                };
                let envelope = SignalingEnvelope {
                    envelope_version: SIGNALING_ENVELOPE_VERSION,
                    envelope_id: self.signaling.next_envelope_id(),
                    reply_for: None,
                    traceparent: None,
                    tracestate: None,
                    flow: Some(signaling_envelope::Flow::ActrRelay(relay)),
                };
                self.signaling.send_envelope(envelope).await?;
            }
            "ice_candidate" => {
                let data: IceCandidateEvent = serde_json::from_value(event.data.clone())
                    .map_err(|e| JsValue::from_str(&e.to_string()))?;

                let target = resolve_target(&event.data)?;

                let ice = IceCandidate {
                    candidate: data.candidate.candidate,
                    sdp_mid: data.candidate.sdp_mid,
                    sdp_mline_index: data.candidate.sdp_mline_index,
                    username_fragment: data.candidate.username_fragment,
                };
                let relay = actr_protocol::ActrRelay {
                    source: actor_id,
                    credential,
                    target,
                    delivery_context: None,
                    source_type: None,
                    payload: Some(actr_relay::Payload::IceCandidate(ice)),
                };
                let envelope = SignalingEnvelope {
                    envelope_version: SIGNALING_ENVELOPE_VERSION,
                    envelope_id: self.signaling.next_envelope_id(),
                    reply_for: None,
                    traceparent: None,
                    tracestate: None,
                    flow: Some(signaling_envelope::Flow::ActrRelay(relay)),
                };
                self.signaling.send_envelope(envelope).await?;
            }
            "datachannel_open" => {
                let data: DataChannelEvent = serde_json::from_value(event.data)
                    .map_err(|e| JsValue::from_str(&e.to_string()))?;
                log::info!(
                    "[SW] datachannel_open: peer={} channel_id={}",
                    data.peer_id,
                    data.channel_id
                );
                self.mark_channel_open(&data.peer_id, data.channel_id);
                self.flush_pending_data(&data.peer_id, data.channel_id)?;

                // ICE restart completion: when datachannel re-opens after restart,
                // mark the restart as completed and reset the attempt counter.
                if self.ice_restart_inflight.get(&data.peer_id) == Some(&true) {
                    log::info!(
                        "[SW] ICE restart completed: peer={} (datachannel re-opened)",
                        data.peer_id
                    );
                    self.ice_restart_inflight
                        .insert(data.peer_id.clone(), false);
                    self.ice_restart_attempts.remove(&data.peer_id);
                }
            }
            "datachannel_close" => {
                let data: DataChannelEvent = serde_json::from_value(event.data)
                    .map_err(|e| JsValue::from_str(&e.to_string()))?;
                log::info!(
                    "[SW] datachannel_close: peer={} channel_id={}",
                    data.peer_id,
                    data.channel_id
                );
                // Remove the channel from open set
                if let Some(channels) = self.open_channels.get_mut(&data.peer_id) {
                    channels.remove(&data.channel_id);
                }
            }
            "connection_state_changed" => {
                #[derive(Deserialize)]
                struct ConnectionStateEvent {
                    #[serde(rename = "peerId")]
                    peer_id: String,
                    #[serde(rename = "connectionId")]
                    connection_id: String,
                    state: String,
                }
                let data: ConnectionStateEvent = serde_json::from_value(event.data)
                    .map_err(|e| JsValue::from_str(&e.to_string()))?;

                log::info!(
                    "[SW] connection_state_changed: peer={} state={}",
                    data.peer_id,
                    data.state
                );

                self.peer_connection_states
                    .insert(data.peer_id.clone(), data.state.clone());
                self.peer_connection_ids
                    .insert(data.peer_id.clone(), data.connection_id);

                match data.state.as_str() {
                    "disconnected" => {
                        // Treat all previously opened lanes as stale once the
                        // peer enters disconnected. Some browsers do not emit
                        // per-channel close events promptly during refresh/
                        // reload churn, so relying on `datachannel_close`
                        // alone leaves `open_channels` falsely marked open and
                        // prevents the next outbound RPC from forcing a clean
                        // reconnect.
                        self.open_channels.remove(&data.peer_id);
                        if let Some(ctx) =
                            CLIENTS.with(|cell| cell.borrow().get(&self.client_id).map(Rc::clone))
                        {
                            let dest = actr_web_common::Dest::Peer(data.peer_id.clone());
                            if let Err(e) = retire_rtc_path(&ctx, &dest, true).await {
                                log::warn!(
                                    "[SW] disconnected: failed to close transport for peer={} error={}",
                                    data.peer_id,
                                    e
                                );
                            }
                        }

                        // Only the offerer should initiate ICE restart
                        let is_offerer = self
                            .role_assignments
                            .get(&data.peer_id)
                            .copied()
                            .unwrap_or(false);

                        if is_offerer {
                            self.trigger_ice_restart(&data.peer_id)?;
                        } else {
                            log::info!(
                                "[SW] ICE restart: not offerer for peer={}, waiting for remote restart",
                                data.peer_id
                            );
                        }
                    }
                    "failed" => {
                        // "failed" means the connection is irrecoverable via ICE restart.
                        // Immediately clean up all peer state and force re-discovery.
                        log::warn!(
                            "[SW] connection_state_changed: peer={} FAILED — cleaning up immediately",
                            data.peer_id
                        );
                        if let Some(ctx) =
                            CLIENTS.with(|cell| cell.borrow().get(&self.client_id).map(Rc::clone))
                        {
                            let dest = actr_web_common::Dest::Peer(data.peer_id.clone());
                            if let Err(e) = retire_rtc_path(&ctx, &dest, false).await {
                                log::warn!(
                                    "[SW] failed: failed to close transport for peer={} error={}",
                                    data.peer_id,
                                    e
                                );
                            }
                        }
                        self.send_webrtc_command("close_peer", &data.peer_id, JsValue::NULL)?;
                        self.ice_restart_inflight.remove(&data.peer_id);
                        self.ice_restart_attempts.remove(&data.peer_id);
                        self.known_peers.remove(&data.peer_id);
                        self.open_channels.remove(&data.peer_id);
                        self.pending_channel_data.remove(&data.peer_id);
                        self.role_negotiated.remove(&data.peer_id);
                        self.role_assignments.remove(&data.peer_id);
                        self.pending_local_sdp_exchanges.remove(&data.peer_id);
                        self.pending_remote_sdp_exchanges.remove(&data.peer_id);
                        self.peer_connection_states.remove(&data.peer_id);
                        // Invalidate any discovered-target cache entry that resolved
                        // to this peer, so the next outbound RPC runs a fresh Issuer
                        // discovery instead of re-using a broken ActrId.
                        self.invalidate_discovered_target(&data.peer_id);
                        // Pending RPCs keep their own deadlines while relay may still connect.
                    }
                    "connected" => {
                        if let Some(ctx) =
                            CLIENTS.with(|cell| cell.borrow().get(&self.client_id).map(Rc::clone))
                        {
                            let dest = actr_web_common::Dest::Peer(data.peer_id.clone());
                            if let Some(transport) = ctx.transport_manager.existing_transport(&dest)
                            {
                                transport.rtc_connected();
                            }
                        }
                        // Connection recovered, clear any restart state
                        if self.ice_restart_inflight.get(&data.peer_id) == Some(&true) {
                            log::info!(
                                "[SW] ICE restart: connection recovered for peer={}",
                                data.peer_id
                            );
                            self.ice_restart_inflight
                                .insert(data.peer_id.clone(), false);
                            self.ice_restart_attempts.remove(&data.peer_id);
                        }
                    }
                    "closed" => {
                        // Peer is fully closed, clean up all associated state
                        if let Some(ctx) =
                            CLIENTS.with(|cell| cell.borrow().get(&self.client_id).map(Rc::clone))
                        {
                            let dest = actr_web_common::Dest::Peer(data.peer_id.clone());
                            if let Err(e) = retire_rtc_path(&ctx, &dest, false).await {
                                log::warn!(
                                    "[SW] closed: failed to close transport for peer={} error={}",
                                    data.peer_id,
                                    e
                                );
                            }
                        }
                        self.ice_restart_inflight.remove(&data.peer_id);
                        self.ice_restart_attempts.remove(&data.peer_id);
                        self.known_peers.remove(&data.peer_id);
                        self.open_channels.remove(&data.peer_id);
                        self.pending_channel_data.remove(&data.peer_id);
                        self.role_negotiated.remove(&data.peer_id);
                        self.role_assignments.remove(&data.peer_id);
                        self.pending_local_sdp_exchanges.remove(&data.peer_id);
                        self.pending_remote_sdp_exchanges.remove(&data.peer_id);
                        self.peer_connection_states.remove(&data.peer_id);
                        // Force re-discovery on next outbound RPC targeting this peer.
                        self.invalidate_discovered_target(&data.peer_id);
                    }
                    _ => {}
                }
            }
            "peer_connection_closed" => {
                #[derive(Deserialize)]
                struct PeerClosedEvent {
                    #[serde(rename = "peerId")]
                    peer_id: String,
                    #[serde(rename = "connectionId")]
                    connection_id: String,
                }
                let data: PeerClosedEvent = serde_json::from_value(event.data)
                    .map_err(|e| JsValue::from_str(&e.to_string()))?;
                if self.peer_connection_ids.get(&data.peer_id) == Some(&data.connection_id) {
                    self.peer_connection_ids.remove(&data.peer_id);
                    if let Some(ctx) =
                        CLIENTS.with(|cell| cell.borrow().get(&self.client_id).map(Rc::clone))
                    {
                        let dest = actr_web_common::Dest::Peer(data.peer_id);
                        if let Some(transport) = ctx.transport_manager.existing_transport(&dest) {
                            transport
                                .rtc_physical_closed(&data.connection_id)
                                .await
                                .map_err(|e| JsValue::from_str(&e.to_string()))?;
                        }
                    }
                }
            }
            "ice_restart_local_description" => {
                // ICE restart offer generated by DOM, send to remote peer
                let data: LocalDescriptionEvent = serde_json::from_value(event.data.clone())
                    .map_err(|e| JsValue::from_str(&e.to_string()))?;

                let target = resolve_target(&event.data)?;

                log::info!("[SW] Sending ICE restart offer to peer={}", target);

                let sdp_exchange_id = self.new_sdp_exchange_id();
                self.pending_local_sdp_exchanges
                    .insert(data.peer_id.clone(), sdp_exchange_id.clone());

                // Use ICE_RESTART_OFFER type (3) for the session description
                let sd = SessionDescription {
                    r#type: session_description::Type::IceRestartOffer as i32,
                    sdp: data.sdp.sdp,
                    sdp_exchange_id: Some(sdp_exchange_id),
                };
                let relay = actr_protocol::ActrRelay {
                    source: actor_id,
                    credential,
                    target,
                    delivery_context: None,
                    source_type: None,
                    payload: Some(actr_relay::Payload::SessionDescription(sd)),
                };
                let envelope = SignalingEnvelope {
                    envelope_version: SIGNALING_ENVELOPE_VERSION,
                    envelope_id: self.signaling.next_envelope_id(),
                    reply_for: None,
                    traceparent: None,
                    tracestate: None,
                    flow: Some(signaling_envelope::Flow::ActrRelay(relay)),
                };
                self.signaling.send_envelope(envelope).await?;
            }
            "command_error" => {
                #[derive(Deserialize)]
                struct CommandErrorEvent {
                    #[serde(rename = "peerId")]
                    peer_id: String,
                    action: String,
                    error: String,
                }
                if let Ok(data) = serde_json::from_value::<CommandErrorEvent>(event.data) {
                    log::warn!(
                        "[SW] command_error: peer={} action={} error={}",
                        data.peer_id,
                        data.action,
                        data.error
                    );
                    let should_reset_peer = data.error.contains("not found")
                        || data.error.starts_with("datachannel_not_open:");
                    if should_reset_peer {
                        self.handle_stale_peer_failure(&data.peer_id, &data.action, &data.error)
                            .await?;
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Handle fast-path data.
    ///
    /// Architectural split:
    /// - Explicit RPC responses -> handled directly (or orphan-dropped) on the fast path
    /// - Inbound RPC requests -> `Mailbox` -> `MailboxProcessor` (state path, about 30-40 ms)
    fn handle_fast_path(&mut self, payload: FastPathPayload) -> Result<(), JsValue> {
        let (peer_id, channel_id) = parse_peer_and_channel(&payload.stream_id);

        if matches!(channel_id, 2 | 3) {
            return self.handle_data_chunk(payload);
        }

        let envelope = RpcEnvelope::decode(&payload.data[..])
            .map_err(|e| JsValue::from_str(&format!("Failed to decode RpcEnvelope: {e}")))?;

        if channel_id == 1
            && let Some(nonce) = actr_protocol::transport_control::acknowledgement_nonce(&envelope)
        {
            CLIENTS.with(|cell| {
                if let Some(context) = cell.borrow().get(&self.client_id) {
                    context
                        .transport_manager
                        .handover
                        .acknowledge(&peer_id, nonce);
                }
            });
            return Ok(());
        }

        let Some(direction) =
            direction_for_routing(&envelope, &peer_id, channel_id, &payload.stream_id)
        else {
            return Ok(());
        };

        if matches!(direction, Direction::Response) {
            // Fast path: process the response directly.
            log::debug!(
                "[SW] Fast Path: response for request_id={}",
                envelope.request_id
            );
            self.handle_rpc_response(envelope, &peer_id, channel_id, &payload.stream_id)
        } else {
            // State path: route through `Dispatcher -> Mailbox -> MailboxProcessor -> Scheduler`.
            // By design, all inbound RPC requests must be persisted in the mailbox
            // and processed through serialized scheduling.
            log::info!(
                "[SW] State Path: request route_key={} request_id={} → Mailbox",
                envelope.route_key,
                envelope.request_id
            );

            // Pass `stream_id` as `from` so `MailboxProcessor` knows the response target.
            let stream_id_bytes = payload.stream_id.as_bytes().to_vec();
            let payload_type = match channel_id {
                1 => PayloadType::RpcSignal,
                _ => PayloadType::RpcReliable,
            };
            let message = MessageFormat::new(payload_type, Bytes::from(payload.data));

            CLIENTS.with(|cell| {
                if let Some(ctx) = cell.borrow().get(&self.client_id) {
                    let dispatcher = Rc::clone(&ctx.dispatcher);
                    wasm_bindgen_futures::spawn_local(async move {
                        if let Err(e) = dispatcher.dispatch(stream_id_bytes, message).await {
                            log::error!("[SW] Dispatcher enqueue error: {}", e);
                        }
                    });
                } else {
                    log::warn!(
                        "[SW] No dispatcher available for client_id={}",
                        self.client_id
                    );
                }
            });

            Ok(())
        }
    }

    fn handle_data_chunk(&mut self, payload: FastPathPayload) -> Result<(), JsValue> {
        let (peer, channel) = parse_peer_and_channel(&payload.stream_id);
        if channel == 2
            && let Ok(chunk) = actr_protocol::DataChunk::decode(payload.data.as_slice())
            && let Some(nonce) = actr_protocol::transport_control::barrier_nonce(&chunk)
        {
            let context = CLIENTS.with(|cell| cell.borrow().get(&self.client_id).map(Rc::clone));
            if let Some(context) = context {
                // Earlier synchronous stream callbacks have returned before
                // this ordered pump observes the barrier. Never hold the
                // runtime lock while awaiting acknowledgement submission.
                let bytes =
                    actr_protocol::transport_control::acknowledgement(nonce).encode_to_vec();
                wasm_bindgen_futures::spawn_local(async move {
                    let dest = actr_web_common::Dest::Peer(peer);
                    if let Some(transport) = context.transport_manager.existing_transport(&dest) {
                        let _ = futures::future::select(
                            Box::pin(transport.send(PayloadType::RpcSignal, &bytes)),
                            Box::pin(gloo_timers::future::TimeoutFuture::new(1000)),
                        )
                        .await;
                    }
                });
            }
            return Ok(());
        }
        let (logical_stream_id, data) = decode_stream_payload(&payload.data)?;

        let handler = CLIENTS.with(|cell| {
            cell.borrow().get(&self.client_id).and_then(|ctx| {
                ctx.stream_handlers
                    .borrow()
                    .get(&logical_stream_id)
                    .cloned()
            })
        });

        if let Some(handler) = handler {
            log::info!(
                "[SW] Fast Path stream dispatch: stream_id={} bytes={}",
                logical_stream_id,
                data.len()
            );
            (handler.borrow_mut())(data);
        } else {
            log::warn!(
                "[SW] No stream handler registered: stream_id={} peer_stream={}",
                logical_stream_id,
                payload.stream_id
            );
        }

        Ok(())
    }

    #[allow(dead_code)]
    fn is_channel_open(&self, peer_id: &str, channel_id: u32) -> bool {
        self.open_channels
            .get(peer_id)
            .map(|channels| channels.contains(&channel_id))
            .unwrap_or(false)
    }

    fn mark_channel_open(&mut self, peer_id: &str, channel_id: u32) {
        self.open_channels
            .entry(peer_id.to_string())
            .or_default()
            .insert(channel_id);
    }

    fn peer_requires_reconnect(&self, peer_id: &str) -> bool {
        matches!(
            self.peer_connection_states.get(peer_id).map(String::as_str),
            Some("disconnected" | "failed" | "closed")
        )
    }

    fn reset_peer_for_reconnect(&mut self, peer_id: &str) {
        let _ = self.send_webrtc_command("close_peer", peer_id, JsValue::NULL);
        self.known_peers.remove(peer_id);
        self.open_channels.remove(peer_id);
        self.pending_channel_data.remove(peer_id);
        self.role_negotiated.remove(peer_id);
        self.role_assignments.remove(peer_id);
        self.pending_local_sdp_exchanges.remove(peer_id);
        self.pending_remote_sdp_exchanges.remove(peer_id);
        self.peer_connection_states.remove(peer_id);
        self.ice_restart_inflight.remove(peer_id);
        self.ice_restart_attempts.remove(peer_id);
    }

    async fn handle_stale_peer_failure(
        &mut self,
        peer_id: &str,
        action: &str,
        error: &str,
    ) -> Result<(), JsValue> {
        log::warn!(
            "[SW] stale peer send failure: peer={} action={} error={}",
            peer_id,
            action,
            error
        );

        if let Some(ctx) = CLIENTS.with(|cell| cell.borrow().get(&self.client_id).map(Rc::clone)) {
            let dest = actr_web_common::Dest::Peer(peer_id.to_string());
            if let Err(e) = retire_rtc_path(&ctx, &dest, false).await {
                log::warn!(
                    "[SW] stale peer cleanup: failed to close transport for peer={} error={}",
                    peer_id,
                    e
                );
            }
        }

        self.reset_peer_for_reconnect(peer_id);
        self.invalidate_discovered_target(peer_id);
        Ok(())
    }

    #[allow(dead_code)]
    fn queue_channel_data(&mut self, peer_id: &str, data: Vec<u8>) {
        self.pending_channel_data
            .entry(peer_id.to_string())
            .or_default()
            .push(data);
    }

    fn flush_pending_data(&mut self, peer_id: &str, channel_id: u32) -> Result<(), JsValue> {
        if channel_id != 0 {
            return Ok(());
        }
        let Some(pending) = self.pending_channel_data.remove(peer_id) else {
            return Ok(());
        };
        for data in pending {
            self.send_channel_data(peer_id, channel_id, &data)?;
        }
        Ok(())
    }

    fn send_channel_data(
        &self,
        peer_id: &str,
        channel_id: u32,
        data: &[u8],
    ) -> Result<(), JsValue> {
        log::info!(
            "[SW] send_channel_data: peer={} channel_id={} bytes={}",
            peer_id,
            channel_id,
            data.len()
        );
        let js_data: JsValue = js_sys::Uint8Array::from(data).into();

        let payload = SendDataPayload {
            channel_id,
            data: js_data,
        };

        let payload_js = serde_wasm_bindgen::to_value(&payload).map_err(|e| {
            JsValue::from_str(&format!("Failed to serialize send_data payload: {}", e))
        })?;

        self.send_webrtc_command("send_data", peer_id, payload_js)
    }

    /// Trigger an ICE restart for the given peer.
    ///
    /// Only the offerer side should call this. Implements deduplication:
    /// - Skips if an ICE restart is already in-flight
    /// - Skips if max retries exceeded (will close peer instead)
    fn trigger_ice_restart(&mut self, peer_id: &str) -> Result<(), JsValue> {
        // Dedup: skip if already in-flight
        if self.ice_restart_inflight.get(peer_id) == Some(&true) {
            log::info!(
                "[SW] ICE restart: already in-flight for peer={}, skipping",
                peer_id
            );
            return Ok(());
        }

        // Check retry count
        let attempts = self
            .ice_restart_attempts
            .entry(peer_id.to_string())
            .or_insert(0);
        if *attempts >= ICE_RESTART_MAX_RETRIES {
            log::warn!(
                "[SW] ICE restart: max retries ({}) exceeded for peer={}, closing connection",
                ICE_RESTART_MAX_RETRIES,
                peer_id
            );
            // Clean up: close the peer connection
            self.send_webrtc_command("close_peer", peer_id, JsValue::NULL)?;
            self.known_peers.remove(peer_id);
            self.open_channels.remove(peer_id);
            self.ice_restart_inflight.remove(peer_id);
            self.ice_restart_attempts.remove(peer_id);
            self.peer_connection_states.remove(peer_id);
            self.role_negotiated.remove(peer_id);
            self.role_assignments.remove(peer_id);
            self.pending_local_sdp_exchanges.remove(peer_id);
            self.pending_remote_sdp_exchanges.remove(peer_id);
            // Force re-discovery on next outbound RPC targeting this peer.
            self.invalidate_discovered_target(peer_id);
            // A failed RTC path does not decide the outcome of a concurrent relay dial.
            return Ok(());
        }

        *attempts += 1;
        let attempt = *attempts;
        self.ice_restart_inflight.insert(peer_id.to_string(), true);

        log::info!(
            "[SW] ICE restart: initiating for peer={} (attempt {}/{})",
            peer_id,
            attempt,
            ICE_RESTART_MAX_RETRIES
        );

        // Send create_ice_restart_offer command to DOM
        self.send_webrtc_command("create_ice_restart_offer", peer_id, JsValue::NULL)?;

        // Schedule a timeout to check if restart completed
        let peer_id_owned = peer_id.to_string();
        let runtime = CLIENTS.with(|cell| {
            cell.borrow()
                .get(&self.client_id)
                .map(|ctx| Rc::clone(&ctx.runtime))
        });
        if let Some(runtime) = runtime {
            wasm_bindgen_futures::spawn_local(async move {
                // Wait for the restart timeout
                TimeoutFuture::new(ICE_RESTART_TIMEOUT_MS).await;

                let mut rt = runtime.lock().await;
                // Check if restart is still in-flight (i.e., not completed)
                if rt.ice_restart_inflight.get(&peer_id_owned) == Some(&true) {
                    log::warn!(
                        "[SW] ICE restart: timeout for peer={}, will retry",
                        peer_id_owned
                    );
                    // Mark as not in-flight so next trigger can proceed
                    rt.ice_restart_inflight.insert(peer_id_owned.clone(), false);

                    // Check current connection state - if still disconnected/failed, retry
                    let state = rt
                        .peer_connection_states
                        .get(&peer_id_owned)
                        .cloned()
                        .unwrap_or_default();
                    if state == "disconnected" || state == "failed" {
                        // Calculate backoff delay
                        let attempt = rt
                            .ice_restart_attempts
                            .get(&peer_id_owned)
                            .copied()
                            .unwrap_or(0);
                        let backoff_ms = std::cmp::min(
                            ICE_RESTART_INITIAL_BACKOFF_MS * (1 << (attempt.saturating_sub(1))),
                            ICE_RESTART_MAX_BACKOFF_MS,
                        );
                        log::info!(
                            "[SW] ICE restart: scheduling retry in {}ms for peer={}",
                            backoff_ms,
                            peer_id_owned
                        );

                        // Drop the lock before sleeping
                        drop(rt);
                        TimeoutFuture::new(backoff_ms).await;

                        let mut rt = runtime.lock().await;
                        // Re-check state after backoff
                        let state = rt
                            .peer_connection_states
                            .get(&peer_id_owned)
                            .cloned()
                            .unwrap_or_default();
                        if state == "disconnected" || state == "failed" {
                            let _ = rt.trigger_ice_restart(&peer_id_owned);
                        }
                    }
                }
            });
        }

        Ok(())
    }
}

use crate::workload::WasmWorkload;

/// Per-client context stored in the SW.
/// Each browser tab gets its own independent client context with
/// its own signaling connection, actor registration, and WebRTC state.
struct ClientContext {
    runtime: Rc<Mutex<SwRuntime>>,
    system: Rc<crate::System>,
    dispatcher: Rc<InboundPacketDispatcher>,
    /// `PeerGate`, the cross-node transport adapter.
    peer_gate: Arc<crate::outbound::PeerGate>,
    /// `PeerTransport`, which manages one `DestTransport` per destination.
    transport_manager: Arc<crate::transport::PeerTransport>,
    /// `DataChunk` callback registry, isolated per browser tab.
    stream_handlers: Rc<RefCell<HashMap<String, StreamHandler>>>,
    relay_channels: RefCell<HashMap<String, String>>,
    relay_security: relay_security::Manager,
    rtc_paths: RefCell<HashMap<String, Option<bool>>>,
}

/// `SwRuntimeBridge`, the `RuntimeBridge` implementation.
///
/// Connects `RuntimeContext` with `SwRuntime`, `System`, and `PeerGate`,
/// providing the lower-level capabilities behind `ctx.call_raw()` and
/// `ctx.discover()` inside handlers.
struct SwRuntimeBridge {
    runtime: Rc<Mutex<SwRuntime>>,
    peer_gate: Arc<crate::outbound::PeerGate>,
    client_id: String,
}

#[async_trait::async_trait(?Send)]
impl RuntimeBridge for SwRuntimeBridge {
    fn register_pending_rpc(&self, request_id: String) {
        // Register synchronously via `futures::lock::Mutex::try_lock`.
        // In single-threaded WASM, lock contention here indicates a bug.
        if let Some(mut rt) = self.runtime.try_lock() {
            rt.register_pending_rpc(request_id, PendingRpcTarget::Internal);
        } else {
            log::error!("[SwRuntimeBridge] Failed to lock runtime for pending RPC registration");
        }
    }

    fn unregister_pending_rpc(&self, request_id: &str) {
        if let Some(mut rt) = self.runtime.try_lock() {
            rt.pending_rpcs.remove(request_id);
        } else {
            log::error!("[SwRuntimeBridge] Failed to lock runtime for pending RPC cleanup");
        }
    }

    async fn discover_target(&self, target_type: &ActrType) -> actr_protocol::ActorResult<ActrId> {
        let mut rt = self.runtime.lock().await;
        rt.discover_target_for_type(target_type)
            .await
            .map_err(|e| actr_protocol::ActrError::Unavailable(format!("Discover failed: {:?}", e)))
    }

    async fn ensure_connection(&self, target_id: &ActrId) -> actr_protocol::ActorResult<()> {
        let peer_id = target_id.to_string_repr();
        request_relay_peer(&self.client_id, target_id);
        self.peer_gate.register_actor(
            target_id.clone(),
            actr_web_common::Dest::Peer(peer_id.clone()),
        );
        let runtime = self.runtime.clone();
        let target = target_id.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let mut runtime = runtime.lock().await;
            if runtime.peer_requires_reconnect(&peer_id) {
                runtime.reset_peer_for_reconnect(&peer_id);
            }
            if !runtime.known_peers.contains(&peer_id) {
                if runtime
                    .send_webrtc_command("create_peer", &peer_id, JsValue::NULL)
                    .is_ok()
                {
                    runtime.known_peers.insert(peer_id.clone());
                }
            }
            if !runtime.role_negotiated.contains(&peer_id) {
                match runtime.send_role_negotiation(&target).await {
                    Ok(()) => {
                        runtime.role_negotiated.insert(peer_id);
                    }
                    Err(error) => log::debug!("[SW] WebRTC negotiation pending: {:?}", error),
                }
            }
        });
        Ok(())
    }

    fn register_stream_handler(
        &self,
        stream_id: String,
        callback: Box<dyn FnMut(Bytes) + 'static>,
    ) -> actr_protocol::ActorResult<()> {
        CLIENTS.with(|cell| {
            let map = cell.borrow();
            let Some(ctx) = map.get(&self.client_id) else {
                return Err(actr_protocol::ActrError::Unavailable(format!(
                    "Client context not found: {}",
                    self.client_id
                )));
            };

            ctx.stream_handlers
                .borrow_mut()
                .insert(stream_id.clone(), Rc::new(RefCell::new(callback)));

            log::info!(
                "[SW] Stream handler registered: client_id={} stream_id={}",
                self.client_id,
                stream_id
            );
            Ok(())
        })
    }

    fn unregister_stream_handler(&self, stream_id: &str) -> actr_protocol::ActorResult<()> {
        CLIENTS.with(|cell| {
            let map = cell.borrow();
            let Some(ctx) = map.get(&self.client_id) else {
                return Err(actr_protocol::ActrError::Unavailable(format!(
                    "Client context not found: {}",
                    self.client_id
                )));
            };

            ctx.stream_handlers.borrow_mut().remove(stream_id);
            log::info!(
                "[SW] Stream handler unregistered: client_id={} stream_id={}",
                self.client_id,
                stream_id
            );
            Ok(())
        })
    }
}

fn parse_peer_and_channel(stream_id: &str) -> (String, u32) {
    if let Some(last_colon) = stream_id.rfind(':') {
        let peer = &stream_id[..last_colon];
        let channel = stream_id[last_colon + 1..].parse::<u32>().unwrap_or(0);
        (peer.to_string(), channel)
    } else {
        (stream_id.to_string(), 0)
    }
}

fn direction_for_routing(
    envelope: &RpcEnvelope,
    peer_id: &str,
    channel_id: u32,
    stream_id: &str,
) -> Option<Direction> {
    match envelope.direction {
        Some(raw) => match Direction::try_from(raw) {
            Ok(direction @ (Direction::Request | Direction::Response | Direction::Tell)) => {
                Some(direction)
            }
            Ok(Direction::Unspecified) => {
                log::warn!(
                    "[SW] rpc.invalid_direction_dropped: RpcEnvelope.direction is Unspecified request_id={} peer={} channel={} stream_id={} route_key={} direction={}; dropping",
                    envelope.request_id,
                    peer_id,
                    channel_id,
                    stream_id,
                    envelope.route_key,
                    raw
                );
                None
            }
            Err(_) => {
                log::warn!(
                    "[SW] rpc.invalid_direction_dropped: unknown RpcEnvelope.direction request_id={} peer={} channel={} stream_id={} route_key={} direction={}; dropping",
                    envelope.request_id,
                    peer_id,
                    channel_id,
                    stream_id,
                    envelope.route_key,
                    raw
                );
                None
            }
        },
        None => {
            log::warn!(
                "[SW] rpc.invalid_direction_dropped: missing RpcEnvelope.direction request_id={} peer={} channel={} stream_id={} route_key={}; dropping",
                envelope.request_id,
                peer_id,
                channel_id,
                stream_id,
                envelope.route_key
            );
            None
        }
    }
}

fn decode_stream_payload(data: &[u8]) -> Result<(String, Bytes), JsValue> {
    if let Ok(chunk) = actr_protocol::DataChunk::decode(data) {
        if !chunk.stream_id.is_empty() {
            return Ok((chunk.stream_id, chunk.payload));
        }
    }
    // Receive compatibility for the earlier browser-only prefix format.
    if data.len() < 4 {
        return Err(JsValue::from_str(
            "Invalid DataChunk payload: missing stream_id length",
        ));
    }

    let stream_id_len = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let payload_start = 4usize
        .checked_add(stream_id_len)
        .filter(|offset| *offset <= data.len())
        .ok_or_else(|| JsValue::from_str("Invalid DataChunk payload: truncated stream_id"))?;

    let logical_stream_id = String::from_utf8(data[4..payload_start].to_vec())
        .map_err(|e| JsValue::from_str(&format!("Invalid DataChunk stream_id: {}", e)))?;

    Ok((
        logical_stream_id,
        Bytes::copy_from_slice(&data[payload_start..]),
    ))
}

thread_local! {
    /// Map of client_id → ClientContext for multi-tab support.
    /// Each browser tab registers with a unique client_id.
    static CLIENTS: RefCell<HashMap<String, Rc<ClientContext>>> = RefCell::new(HashMap::new());
    static GLOBAL_INITIALIZED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static WORKLOAD: RefCell<Option<WasmWorkload>> = const { RefCell::new(None) };
}

/// Register a WASM workload with the SW runtime.
///
/// The workload dispatches RPC requests to business logic through its handler.
///
/// # Example
/// ```ignore
/// use actr_sw_host::{WasmWorkload, register_workload};
///
/// let workload = WasmWorkload::new(Rc::new(|route_key, bytes, timeout_ms, ctx| {
///     Box::pin(async move {
///         match route_key {
///             "echo.EchoService.Echo" => handle_echo(bytes, ctx).await,
///             _ => Err(format!("Unknown route: {}", route_key)),
///         }
///     })
/// }));
/// register_workload(workload);
/// ```
pub fn register_workload(workload: WasmWorkload) {
    WORKLOAD.with(|cell| {
        *cell.borrow_mut() = Some(workload);
    });
    log::info!("[SW] Workload registered");
}

#[wasm_bindgen]
pub fn init_global() -> Result<(), JsValue> {
    let first_init = GLOBAL_INITIALIZED.with(|cell| {
        if cell.get() {
            false
        } else {
            cell.set(true);
            true
        }
    });

    if first_init {
        // Install the panic hook so panic details reach the console.
        console_error_panic_hook::set_once();

        // Initialize logging.
        wasm_logger::init(wasm_logger::Config::default());

        // Initialize lifecycle management.
        let lifecycle = crate::SwLifecycleManager::new();
        if let Err(e) = lifecycle.init() {
            log::error!("Failed to initialize lifecycle manager: {:?}", e);
        }
        log::info!("[SW] Global initialization complete");
    }

    Ok(())
}

/// Register a new client (browser tab) with the SW runtime.
///
/// Each call creates an independent runtime with its own signaling connection,
/// actor registration, and WebRTC state. This enables multiple browser tabs
/// to work simultaneously without interfering with each other.
/// Returns the actual imported or restored Realm client key ID after success.
#[wasm_bindgen]
pub async fn register_client(
    client_id: String,
    config: JsValue,
    port: MessagePort,
) -> Result<String, JsValue> {
    // Ensure global init
    init_global()?;

    log::info!("[SW] register_client: client_id={}", client_id);

    let config: SwConfig = serde_wasm_bindgen::from_value(config)?;
    log::info!(
        "[SW] SwConfig parsed: client_id={} acl_allow_types={:?}",
        client_id,
        config.acl_allow_types,
    );
    let mut runtime = SwRuntime::new(client_id.clone(), config).await?;
    let client_key_id = runtime.issuer.client_key_id().to_owned();

    // Set DOM port
    runtime.dom_port = Some(port);

    // Send TURN credential to DOM so the WebRTC coordinator can use it
    // for ICE server authentication. This must happen before any create_peer
    // commands.
    if let Some(ref tc) = runtime.turn_credential {
        if let Err(e) = runtime.send_turn_credential_to_dom(tc) {
            log::warn!("[SW] Failed to send TurnCredential to DOM: {:?}", e);
        }
    }

    // Fetch `actor_id` for the `System`.
    let actor_id = runtime.actor_id.clone();

    let runtime = Rc::new(Mutex::new(runtime));

    // ==================== State Path Initialization ====================
    // Documented architecture: `InboundDispatcher -> Mailbox -> MailboxProcessor -> Scheduler -> Actor`
    log::info!("[SW] [{}] Initializing State Path components...", client_id);

    // 1. Create the mailbox (`IndexedDB`).
    let mailbox: Rc<dyn Mailbox> = Rc::new(
        IndexedDbMailbox::new()
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to create Mailbox: {}", e)))?,
    );

    // 2. Create the `MailboxProcessor` and its event-driven notifier.
    let (mut processor, notifier) = MailboxProcessor::new(mailbox.clone(), 10);

    // 3. Create the `InboundPacketDispatcher` with the shared IndexedDB backend and notifier.
    let mailbox_arc: Arc<dyn Mailbox> = Arc::from(IndexedDbMailbox::new().await.map_err(|e| {
        JsValue::from_str(&format!("Failed to create Mailbox for dispatcher: {}", e))
    })?);
    let dispatcher =
        Rc::new(InboundPacketDispatcher::new(mailbox_arc.clone()).with_notifier(notifier));

    // 4. Create the `Scheduler`, which serializes execution per actor.
    let scheduler = Scheduler::new();

    // 5. Install the scheduler's actor-processing callback.
    //    Documented flow: `Scheduler -> actor business logic -> response`
    let runtime_for_scheduler = Rc::clone(&runtime);
    let client_id_for_scheduler = client_id.clone();
    scheduler.set_handler(Rc::new(move |record: MessageRecord| {
        let runtime = Rc::clone(&runtime_for_scheduler);
        let client_id = client_id_for_scheduler.clone();
        Box::pin(async move {
            // Decode the message. Every mailbox entry here is an inbound RPC request.
            let envelope = RpcEnvelope::decode(&record.payload[..])
                .map_err(|e| actr_web_common::WebError::Protocol(format!("Failed to decode RpcEnvelope: {}", e)))?;

            log::info!(
                "[Scheduler] Processing RPC request: request_id={}, route_key={}",
                envelope.request_id,
                envelope.route_key
            );

            // Fetch the registered workload.
            let workload = WORKLOAD.with(|cell| cell.borrow().clone());

            if let Some(workload) = workload {
                let route_key = envelope.route_key.clone();
                let request_id = envelope.request_id.clone();
                // Fire-and-forget is signalled ONLY by the explicit
                // Direction::Tell label; receivers must not infer tell-ness
                // from timeout_ms == 0 (see package.proto).
                let is_tell = envelope.direction == Some(Direction::Tell as i32);
                let request_bytes = envelope
                    .payload
                    .as_ref()
                    .map(|p| p.to_vec())
                    .unwrap_or_default();
                let stream_id = String::from_utf8(record.from.clone()).unwrap_or_default();
                let caller_id = envelope
                    .metadata
                    .iter()
                    .find(|entry| entry.key == "sender_actr_id")
                    .and_then(|entry| ActrId::from_string_repr(&entry.value).ok());
                let (peer_id, _channel_id) = parse_peer_and_channel(&stream_id);

                log::info!(
                    "[Scheduler] Dispatching to handler: route_key={} request_id={} is_tell={}",
                    route_key,
                    request_id,
                    is_tell
                );

                // Build RuntimeContext for the handler (outbound peer gate)
                // Look up system/peer_gate from CLIENTS (initialized after scheduler setup)
                let (outgate, bridge) = CLIENTS.with(|cell| {
                    let map = cell.borrow();
                    if let Some(ctx) = map.get(&client_id) {
                        if let Some(caller_id) = caller_id.clone() {
                            ctx.peer_gate.register_actor(
                                caller_id,
                                actr_web_common::Dest::Peer(peer_id.clone()),
                            );
                        }
                        let outgate = ctx.system.outgate().unwrap_or_else(|| {
                            Gate::host(Arc::clone(ctx.system.host_gate()))
                        });
                        let bridge: Rc<dyn RuntimeBridge> = Rc::new(SwRuntimeBridge {
                            runtime: Rc::clone(&runtime),
                            peer_gate: Arc::clone(&ctx.peer_gate),
                            client_id: client_id.clone(),
                        });
                        (outgate, bridge)
                    } else {
                        // Fallback: Host gate with no bridge
                        log::error!("[Scheduler] Client context not found for {}", client_id);
                        #[allow(clippy::arc_with_non_send_sync)]
                        let gate = Gate::host(Arc::new(crate::outbound::HostGate::new()));
                        #[allow(clippy::arc_with_non_send_sync)]
                        let bridge: Rc<dyn RuntimeBridge> = Rc::new(SwRuntimeBridge {
                            runtime: Rc::clone(&runtime),
                            peer_gate: Arc::new(crate::outbound::PeerGate::new(
                                Arc::new(crate::transport::PeerTransport::new(
                                    client_id.clone(),
                                    Arc::new(crate::transport::WebWireBuilder::new()),
                                )),
                            )),
                            client_id: client_id.clone(),
                        });
                        (gate, bridge)
                    }
                });
                let actor_id = {
                    let rt = runtime.lock().await;
                    rt.actor_id.clone().unwrap_or_default()
                };
                let handler_ctx: Rc<RuntimeContext> = Rc::new(
                    RuntimeContext::new(
                        actor_id,
                        caller_id,
                        envelope.traceparent.clone().unwrap_or_default(),
                        envelope.tracestate.clone().unwrap_or_default(),
                        request_id.clone(),
                        outgate,
                    )
                    .with_bridge(bridge),
                );

                // Execute the actor business logic via workload dispatch.
                let result = workload
                    .dispatch(
                        &route_key,
                        &request_bytes,
                        envelope.timeout_ms,
                        handler_ctx,
                    )
                    .await;

                // `tell()` is fire-and-forget, so no response is built or sent.
                if is_tell {
                    match result {
                        Ok(_) => log::debug!(
                            "[Scheduler] Tell handled successfully: request_id={}",
                            request_id
                        ),
                        Err(err) => log::error!(
                            "[Scheduler] Tell handler error (no response sent): request_id={} error={}",
                            request_id,
                            err
                        ),
                    }
                } else {
                    // `call()` uses request-response semantics, so build and send a response.
                    let response_envelope = match result {
                        Ok(response_bytes) => {
                            log::info!(
                                "[Scheduler] Service handler success: request_id={} response_len={}",
                                request_id,
                                response_bytes.len()
                            );
                            RpcEnvelope {
                                route_key: route_key.clone(),
                                payload: Some(Bytes::from(response_bytes)),
                                error: None,
                                direction: Some(Direction::Response as i32),
                                traceparent: None,
                                tracestate: None,
                                request_id: request_id.clone(),
                                metadata: vec![],
                                timeout_ms: 0,
                            }
                        }
                        Err(err) => {
                            log::error!(
                                "[Scheduler] Service handler error: request_id={} error={}",
                                request_id,
                                err
                            );
                            RpcEnvelope {
                                route_key: route_key.clone(),
                                payload: None,
                                error: Some(actr_protocol::ErrorResponse {
                                    code: 500,
                                    message: err,
                                }),
                                direction: Some(Direction::Response as i32),
                                traceparent: None,
                                tracestate: None,
                                request_id: request_id.clone(),
                                metadata: vec![],
                                timeout_ms: 0,
                            }
                        }
                    };

                    let data = response_envelope.encode_to_vec();

                    // Parse the sender's `peer_id` and `channel_id` from `stream_id`.
                    let (peer_id, channel_id) = if let Some(last_colon) = stream_id.rfind(':') {
                        let peer = &stream_id[..last_colon];
                        let ch = stream_id[last_colon + 1..].parse::<u32>().unwrap_or(0);
                        (peer.to_string(), ch)
                    } else {
                        (stream_id.clone(), 0u32)
                    };

                    log::info!(
                        "[Scheduler] Sending RPC response: request_id={} peer={} channel={} bytes={}",
                        request_id,
                        peer_id,
                        channel_id,
                        data.len()
                    );

                    // Use the same ready-path selection as requests. Keep both
                    // receive paths alive while direct connectivity is promoted.
                    let context = CLIENTS.with(|cell| cell.borrow().get(&client_id).map(Rc::clone));
                    if let Some(context) = context {
                        let payload_type = if channel_id == 1 { PayloadType::RpcSignal } else { PayloadType::RpcReliable };
                        if let Err(error) = context.transport_manager.send(&actr_web_common::Dest::Peer(peer_id), payload_type, &data).await {
                            log::error!("[Scheduler] RPC response send failed: request_id={} error={}", request_id, error);
                        }
                    }
                }
            } else {
                log::warn!(
                    "[Scheduler] No workload registered for incoming RPC request: route_key={}",
                    envelope.route_key
                );
            }

            Ok(())
        })
    }));

    // 6. Install the `MailboxProcessor` callback.
    //    Documented flow: `MailboxProcessor -> Scheduler` into the actor's serialized queue.
    let scheduler_for_processor = scheduler.clone();
    let local_actor_id_for_processor = actor_id.clone().unwrap_or_else(|| {
        log::warn!("[SW] actor_id not set after register, using default for Scheduler");
        ActrId::default()
    });
    processor.set_handler(Rc::new(move |record: MessageRecord| {
        let sched = scheduler_for_processor.clone();
        let aid = local_actor_id_for_processor.clone();
        Box::pin(async move {
            sched.schedule(aid, record);
            Ok(())
        })
    }));

    // 7. Start the scheduler and mailbox processor.
    scheduler.start();
    processor.start();

    // ==================== System Initialization ====================

    let system = Rc::new(crate::System::new());
    if let Some(actor_id) = actor_id {
        system.set_local_actor_id(actor_id);
    }

    // Build the full transport stack: `PeerGate -> PeerTransport -> DestTransport -> WirePool`.
    #[allow(clippy::arc_with_non_send_sync)]
    let wire_builder = Arc::new(crate::transport::WebWireBuilder::new());
    #[allow(clippy::arc_with_non_send_sync)]
    let transport_manager = Arc::new(crate::transport::PeerTransport::new(
        client_id.clone(),
        wire_builder,
    ));
    #[allow(clippy::arc_with_non_send_sync)]
    let peer_gate = Arc::new(crate::outbound::PeerGate::new(Arc::clone(
        &transport_manager,
    )));
    system.set_outgate(crate::outbound::Gate::peer(Arc::clone(&peer_gate)));

    system.init_message_handler();

    // ==================== Store ClientContext ====================

    let client_context = Rc::new(ClientContext {
        runtime: Rc::clone(&runtime),
        system: Rc::clone(&system),
        dispatcher: Rc::clone(&dispatcher),
        peer_gate: Arc::clone(&peer_gate),
        transport_manager: Arc::clone(&transport_manager),
        stream_handlers: Rc::new(RefCell::new(HashMap::new())),
        relay_channels: RefCell::new(HashMap::new()),
        relay_security: relay_security::Manager::default(),
        rtc_paths: RefCell::new(HashMap::new()),
    });

    CLIENTS.with(|cell| {
        cell.borrow_mut().insert(client_id.clone(), client_context);
    });

    // ==================== Background Loops ====================

    let runtime_for_loop = Rc::clone(&runtime);
    let signaling = runtime_for_loop.lock().await.signaling.clone();
    let client_id_for_loop = client_id.clone();
    wasm_bindgen_futures::spawn_local(async move {
        while let Some(env) = signaling.recv_inbound().await {
            if let Some(signaling_envelope::Flow::ActrRelay(relay)) = env.flow {
                let mut runtime = runtime_for_loop.lock().await;
                let _ = runtime.handle_actr_relay(relay).await;
            }
        }
        log::info!("[SW] [{}] Signaling relay loop ended", client_id_for_loop);
    });

    let runtime_for_heartbeat = Rc::clone(&runtime);
    let client_id_for_heartbeat = client_id.clone();
    wasm_bindgen_futures::spawn_local(async move {
        let mut consecutive_failures: u32 = 0;
        loop {
            TimeoutFuture::new(25_000).await;

            // ---- try heartbeat ----
            let hb_result = {
                let rt = runtime_for_heartbeat.lock().await;
                rt.send_heartbeat().await
            };

            if hb_result.is_ok() {
                consecutive_failures = 0;
                continue;
            }

            let heartbeat_error = hb_result.unwrap_err();
            let credential_expired =
                format!("{heartbeat_error:?}").contains("signaling credential expired");
            if credential_expired {
                let renew_result = {
                    let mut rt = runtime_for_heartbeat.lock().await;
                    rt.renew_current_credential().await
                };
                match renew_result {
                    Ok(()) => {
                        log::info!(
                            "[SW] [{}] Credential renewed after heartbeat 401",
                            client_id_for_heartbeat
                        );
                        consecutive_failures = 0;
                        continue;
                    }
                    Err(err) => {
                        log::warn!(
                            "[SW] [{}] Credential renewal after heartbeat 401 failed: {:?}",
                            client_id_for_heartbeat,
                            err
                        );
                    }
                }
            }

            // ---- heartbeat failed ----
            consecutive_failures += 1;
            log::warn!(
                "[SW] [{}] Heartbeat failed ({}/3): {:?}",
                client_id_for_heartbeat,
                consecutive_failures,
                heartbeat_error
            );

            if consecutive_failures < 3 {
                continue;
            }

            // ---- 3 consecutive failures → reconnect ----
            log::info!(
                "[SW] [{}] Attempting signaling reconnection...",
                client_id_for_heartbeat
            );

            let reconnect_result = {
                let mut rt = runtime_for_heartbeat.lock().await;
                rt.reconnect_signaling().await
            };

            match reconnect_result {
                Ok(_) => {
                    log::info!(
                        "[SW] [{}] Signaling reconnected, spawning new relay loop",
                        client_id_for_heartbeat
                    );
                    consecutive_failures = 0;

                    // Spawn a fresh relay loop for the new signaling client.
                    let new_signaling = {
                        let rt = runtime_for_heartbeat.lock().await;
                        rt.signaling.clone()
                    };
                    let runtime_for_new_relay = Rc::clone(&runtime_for_heartbeat);
                    let cid = client_id_for_heartbeat.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        while let Some(env) = new_signaling.recv_inbound().await {
                            if let Some(signaling_envelope::Flow::ActrRelay(relay)) = env.flow {
                                let mut runtime = runtime_for_new_relay.lock().await;
                                let _ = runtime.handle_actr_relay(relay).await;
                            }
                        }
                        log::info!("[SW] [{}] Signaling relay loop ended (post-reconnect)", cid);
                    });
                }
                Err(e) => {
                    log::error!(
                        "[SW] [{}] Signaling reconnection failed: {:?}, stopping heartbeat",
                        client_id_for_heartbeat,
                        e
                    );
                    break;
                }
            }
        }
        log::info!("[SW] [{}] Heartbeat loop ended", client_id_for_heartbeat);
    });

    log::info!(
        "[SW] [{}] Client registration complete (State Path enabled)",
        client_id
    );
    Ok(client_key_id)
}

/// Unregister a client (browser tab) from the SW runtime.
///
/// Closes the signaling WebSocket (so the signaling server removes
/// the actor from its ServiceRegistry) and removes the client context.
/// Background tasks (signaling relay, heartbeat) will naturally stop
/// when the signaling connection drops.
#[wasm_bindgen]
pub async fn unregister_client(client_id: String) {
    if let Ok(function) =
        js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("__actrRelayStop"))
    {
        if let Some(function) = function.dyn_ref::<js_sys::Function>() {
            let _ = function.call1(&JsValue::NULL, &JsValue::from_str(&client_id));
        }
    }
    log::info!("[SW] unregister_client: client_id={}", client_id);
    let ctx = CLIENTS.with(|cell| cell.borrow_mut().remove(&client_id));
    if let Some(ctx) = ctx {
        ctx.stream_handlers.borrow_mut().clear();
        let rt = ctx.runtime.lock().await;
        rt.signaling.close();
        log::info!(
            "[SW] unregister_client: signaling WebSocket closed for client_id={}",
            client_id
        );
    }
}

/// Handle an RPC control request originating from the DOM side.
///
/// Message flow in unified-dispatcher mode:
/// - With `WORKLOAD`: `DOM -> workload.dispatch(route_key, payload, ctx) -> response`
///   - Local route: the workload processes locally and may call remote targets via `ctx.call_raw()`
///   - Remote route: the workload forwards to a remote actor via `ctx.call_raw()`
/// - Without `WORKLOAD`: `DOM -> HostGate -> Gate -> WebRTC`
#[wasm_bindgen]
pub async fn handle_dom_control(client_id: String, payload: JsValue) -> Result<(), JsValue> {
    let call: DomRpcCall = serde_wasm_bindgen::from_value(payload)?;

    if call.action != "rpc_call" {
        return Ok(());
    }

    // Fetch the client context.
    let ctx = CLIENTS.with(|cell| cell.borrow().get(&client_id).map(Rc::clone));

    let Some(ctx) = ctx else {
        log::error!(
            "[SW] handle_dom_control: client not found client_id={}",
            client_id
        );
        return Err(JsValue::from_str("Client not registered"));
    };

    let system = &ctx.system;
    let runtime = &ctx.runtime;

    let route_key = call.request.route_key.clone();
    let payload_bytes = call.request.payload.clone();
    let request_id = call.request_id.clone();
    let timeout_ms = call.request.timeout.unwrap_or(30000);
    // Sender-side contract (#254): a REQUEST must carry a positive deadline.
    crate::outbound::validate_rpc_timeout_ms(timeout_ms as i64)
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    // Check if a workload is registered
    let workload = WORKLOAD.with(|cell| cell.borrow().clone());

    if let Some(workload) = workload {
        // ========== Workload dispatch path (local + remote handler) ==========
        log::info!(
            "[SW] handle_dom_control: client_id={} route_key={} request_id={} (via handler)",
            client_id,
            route_key,
            request_id
        );

        // Build RuntimeContext for the handler
        let actor_id = {
            let rt = runtime.lock().await;
            rt.actor_id.clone().unwrap_or_default()
        };
        // Use the peer gate directly for handler-initiated outbound traffic.
        // `call_raw()` still registers pending RPCs through the bridge, while
        // `send_data_chunk()` must bypass HostGate so it uses the stream lane
        // instead of being wrapped as an RPC envelope.
        let outgate = Gate::peer(Arc::clone(&ctx.peer_gate));
        let bridge: Rc<dyn RuntimeBridge> = Rc::new(SwRuntimeBridge {
            runtime: Rc::clone(runtime),
            peer_gate: Arc::clone(&ctx.peer_gate),
            client_id: client_id.clone(),
        });
        let handler_ctx: Rc<RuntimeContext> = Rc::new(
            RuntimeContext::new(
                actor_id,
                None,
                String::new(),
                String::new(),
                request_id.clone(),
                outgate,
            )
            .with_bridge(bridge),
        );

        // Spawn workload dispatch
        let runtime_for_response = Rc::clone(runtime);
        wasm_bindgen_futures::spawn_local(async move {
            let result = workload
                .dispatch(&route_key, &payload_bytes, timeout_ms as i64, handler_ctx)
                .await;

            // Send response back to DOM as control_response
            let rt = runtime_for_response.lock().await;
            match result {
                Ok(response_bytes) => {
                    log::info!(
                        "[SW] workload_dom_response: client_id={} request_id={} ok len={}",
                        rt.client_id,
                        request_id,
                        response_bytes.len()
                    );
                    let js_payload: JsValue =
                        js_sys::Uint8Array::from(response_bytes.as_slice()).into();
                    let response = SwMessage {
                        msg_type: "control_response",
                        payload: RpcResponsePayload {
                            request_id: request_id.clone(),
                            data: js_payload,
                            error: None,
                        },
                    };
                    if let Ok(msg) = serde_wasm_bindgen::to_value(&response) {
                        let _ = rt.send_dom_message(&msg);
                    }
                }
                Err(err) => {
                    log::error!(
                        "[SW] workload_dom_response: client_id={} request_id={} err={}",
                        rt.client_id,
                        request_id,
                        err
                    );
                    log::error!(
                        "[SW] Handler error: request_id={} error={}",
                        request_id,
                        err
                    );
                    let response = SwMessage {
                        msg_type: "control_response",
                        payload: RpcResponsePayload {
                            request_id: request_id.clone(),
                            data: JsValue::NULL,
                            error: Some(RpcError {
                                code: 500,
                                message: err,
                            }),
                        },
                    };
                    if let Ok(msg) = serde_wasm_bindgen::to_value(&response) {
                        let _ = rt.send_dom_message(&msg);
                    }
                }
            }
        });
    } else {
        // ========== Legacy path: direct remote forwarding (backward compatible) ==========
        let envelope = RpcEnvelope {
            route_key: route_key.clone(),
            payload: Some(Bytes::from(payload_bytes)),
            error: None,
            direction: Some(Direction::Request as i32),
            traceparent: None,
            tracestate: None,
            request_id: request_id.clone(),
            metadata: vec![],
            timeout_ms: timeout_ms as i64,
        };

        log::info!(
            "[SW] handle_dom_control: client_id={} route_key={} request_id={} (legacy remote path)",
            client_id,
            route_key,
            request_id
        );

        // Discover the target for this RPC. `discover_target_with_retry`
        // consults the per-type cache first and only hits Issuer on miss, so
        // steady-state calls remain cheap while transparently re-discovering
        // after a peer tear-down invalidates the cache.
        let target_id = {
            let mut rt = runtime.lock().await;
            rt.discover_target_with_retry().await.map_err(|e| {
                log::error!("[SW] Failed to discover target: {:?}", e);
                e
            })?
        };

        // Ensure the P2P connection exists and register the `ActrId -> Dest` mapping.
        {
            let mut rt = runtime.lock().await;
            let peer_id = rt.ensure_peer_with_retry().await.map_err(|e| {
                log::error!("[SW] Failed to ensure peer: {:?}", e);
                e
            })?;
            ctx.peer_gate
                .register_actor(target_id.clone(), actr_web_common::Dest::Peer(peer_id));
            rt.pending_rpcs
                .insert(request_id.clone(), PendingRpcTarget::Dom);
        }

        // Send the request through `HostGate`.
        let host_gate = Arc::clone(system.host_gate());

        wasm_bindgen_futures::spawn_local({
            let request_id = request_id.clone();
            let runtime = Rc::clone(runtime);
            async move {
                match host_gate.send_request(&target_id, envelope).await {
                    Ok(_response) => {
                        log::info!(
                            "[SW] HostGate response received for request_id={}",
                            request_id
                        );
                    }
                    Err(e) => {
                        log::error!("[SW] HostGate send_request failed: {:?}", e);
                        let mut runtime = runtime.lock().await;
                        if runtime.pending_rpcs.remove(&request_id).is_some() {
                            let response = SwMessage {
                                msg_type: "control_response",
                                payload: RpcResponsePayload {
                                    request_id,
                                    data: JsValue::NULL,
                                    error: Some(RpcError {
                                        code: 503,
                                        message: e.to_string(),
                                    }),
                                },
                            };
                            if let Ok(message) = serde_wasm_bindgen::to_value(&response) {
                                let _ = runtime.send_dom_message(&message);
                            }
                        }
                    }
                }
            }
        });
    }

    Ok(())
}

#[wasm_bindgen]
pub async fn handle_dom_webrtc_event(client_id: String, payload: JsValue) -> Result<(), JsValue> {
    let event: DomWebRtcEvent = serde_wasm_bindgen::from_value(payload)?;
    if event.event_type == "network_path_changed" {
        let context = CLIENTS.with(|cell| cell.borrow().get(&client_id).map(Rc::clone));
        if let Some(context) = context {
            if let Some(peer) = event.data.get("peerId").and_then(|value| value.as_str()) {
                let relayed = event.data.get("relayed").and_then(|value| value.as_bool());
                context
                    .rtc_paths
                    .borrow_mut()
                    .insert(peer.to_owned(), relayed);
                let dest = actr_web_common::Dest::Peer(peer.to_owned());
                if let Some(transport) = context.transport_manager.existing_transport(&dest) {
                    if let Some(handle) = transport
                        .wire_pool()
                        .get_connection(actr_web_common::ConnType::WebRTC)
                        .await
                    {
                        if let Some(rtc) = handle.as_webrtc() {
                            rtc.set_network_path(relayed);
                        }
                    }
                }
            }
        }
        return Ok(());
    }

    let runtime = CLIENTS.with(|cell| {
        cell.borrow()
            .get(&client_id)
            .map(|ctx| Rc::clone(&ctx.runtime))
    });
    if let Some(runtime) = runtime {
        wasm_bindgen_futures::spawn_local(async move {
            let mut rt = runtime.lock().await;
            let _ = rt.handle_webrtc_event(event).await;
        });
    } else {
        log::warn!(
            "[SW] handle_dom_webrtc_event: client not found client_id={}",
            client_id
        );
    }
    Ok(())
}

/// Register a dedicated DataChannel `MessagePort` received from the DOM side.
///
/// After the DOM creates the DataChannel bridge:
/// 1. DOM: `port1 <-> DataChannel` for bidirectional forwarding
/// 2. DOM: transfers `port2` to the SW via a transferable object
/// 3. SW: this function receives `port2`, builds `WebRtcConnection`, and injects it into `WirePool`
///
/// After injection, `DestTransport` is awakened through `ReadyWatcher`, and
/// subsequent outbound traffic is sent zero-copy through `DataLane::PostMessage(port)`.
#[wasm_bindgen]
pub async fn register_datachannel_port(
    client_id: String,
    peer_id: String,
    connection_id: String,
    port: MessagePort,
) -> Result<(), JsValue> {
    log::info!(
        "[SW] register_datachannel_port: client_id={} peer_id={}",
        client_id,
        peer_id
    );

    let ctx = CLIENTS.with(|cell| cell.borrow().get(&client_id).map(Rc::clone));

    let Some(ctx) = ctx else {
        log::error!(
            "[SW] register_datachannel_port: client not found client_id={}",
            client_id
        );
        return Err(JsValue::from_str("Client not registered"));
    };

    // Create the `WebRtcConnection` and attach the `MessagePort`.
    let mut rtc_conn = crate::transport::WebRtcConnection::new(peer_id.clone());
    rtc_conn.set_connection_id(connection_id);
    rtc_conn.set_network_path(ctx.rtc_paths.borrow().get(&peer_id).copied().flatten());
    rtc_conn.set_datachannel_port(port);

    let wire_handle = crate::transport::WireHandle::WebRTC(rtc_conn);
    let dest = actr_web_common::Dest::Peer(peer_id.clone());

    // Inject it into the corresponding `WirePool` owned by `PeerTransport`.
    // If the `DestTransport` does not exist yet, an empty one is created automatically.
    ctx.transport_manager
        .inject_connection(&dest, wire_handle)
        .await
        .map_err(|e| {
            JsValue::from_str(&format!(
                "Failed to inject connection for peer {}: {}",
                peer_id, e
            ))
        })?;

    log::info!(
        "[SW] register_datachannel_port: injected WebRTC connection for peer_id={}",
        peer_id
    );
    Ok(())
}

#[wasm_bindgen]
pub fn handle_dom_fast_path(client_id: String, payload: JsValue) -> Result<(), JsValue> {
    let data: FastPathPayload = serde_wasm_bindgen::from_value(payload)?;
    let runtime = CLIENTS.with(|cell| {
        cell.borrow()
            .get(&client_id)
            .map(|ctx| Rc::clone(&ctx.runtime))
    });
    if let Some(runtime) = runtime {
        wasm_bindgen_futures::spawn_local(async move {
            let mut rt = runtime.lock().await;
            let _ = rt.handle_fast_path(data);
        });
    } else {
        log::warn!(
            "[SW] handle_dom_fast_path: client not found client_id={}",
            client_id
        );
    }
    Ok(())
}

fn request_relay_peer(client_id: &str, peer: &ActrId) {
    if let Ok(function) =
        js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("__actrRelayEnsure"))
    {
        if let Some(function) = function.dyn_ref::<js_sys::Function>() {
            let _ = function.call2(
                &JsValue::NULL,
                &JsValue::from_str(client_id),
                &JsValue::from_str(&peer.to_string_repr()),
            );
        }
    }
}

/// Await mailbox admission before releasing the JS relay receive budget.
/// Responses and streams retain the existing fast dispatch path.
#[wasm_bindgen]
pub async fn handle_relay_data(client_id: String, payload: JsValue) -> Result<(), JsValue> {
    let payload: FastPathPayload = serde_wasm_bindgen::from_value(payload)?;
    let ctx = CLIENTS
        .with(|cell| cell.borrow().get(&client_id).map(Rc::clone))
        .ok_or_else(|| JsValue::from_str("Client not registered"))?;
    let (peer, lane) = parse_peer_and_channel(&payload.stream_id);
    if !matches!(lane, 2 | 3) {
        let envelope = RpcEnvelope::decode(payload.data.as_slice())
            .map_err(|_| JsValue::from_str("Invalid relay RPC envelope"))?;
        if lane == 1 && actr_protocol::transport_control::acknowledgement_nonce(&envelope).is_some()
        {
            return ctx.runtime.lock().await.handle_fast_path(payload);
        }
        let direction = direction_for_routing(&envelope, &peer, lane, &payload.stream_id);
        if matches!(direction, Some(Direction::Request | Direction::Tell)) {
            let payload_type = if lane == 1 {
                PayloadType::RpcSignal
            } else {
                PayloadType::RpcReliable
            };
            return ctx
                .dispatcher
                .dispatch(
                    payload.stream_id.as_bytes().to_vec(),
                    MessageFormat::new(payload_type, Bytes::from(payload.data)),
                )
                .await
                .map_err(|error| JsValue::from_str(&error.to_string()));
        }
    }
    ctx.runtime.lock().await.handle_fast_path(payload)
}

async fn retire_rtc_path(
    ctx: &ClientContext,
    dest: &actr_web_common::Dest,
    recoverable: bool,
) -> actr_web_common::WebResult<()> {
    if let Some(transport) = ctx.transport_manager.existing_transport(dest) {
        if recoverable {
            transport.rtc_disconnected().await?;
        } else {
            transport.rtc_terminated().await?;
        }
    }
    Ok(())
}

/// Internal worker-only bootstrap. Credentials stay in the worker and are
/// never posted to the DOM or appended to the relay URL.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RelayConnectionInfo {
    endpoint: String,
    token: String,
    signaling_url: String,
    expires_at: f64,
    security_epoch: String,
}

#[wasm_bindgen]
pub async fn relay_connection_info(client_id: String) -> Result<JsValue, JsValue> {
    let ctx = CLIENTS
        .with(|cell| cell.borrow().get(&client_id).map(Rc::clone))
        .ok_or_else(|| JsValue::from_str("Client not registered"))?;
    let (secure_expiry, security_epoch) = relay_security::ensure_identity(&ctx).await?;
    let rt = ctx.runtime.lock().await;
    let identity = rt
        .actor_id
        .as_ref()
        .ok_or_else(|| JsValue::from_str("Client identity not ready"))?;
    let credential = rt
        .credential
        .as_ref()
        .ok_or_else(|| JsValue::from_str("Client credential not ready"))?;
    let claims = actr_protocol::IdentityClaims::decode(credential.claims.as_ref())
        .map_err(|_| JsValue::from_str("Invalid credential claims"))?;
    serde_wasm_bindgen::to_value(&RelayConnectionInfo {
        endpoint: identity.to_string_repr(),
        token: relay_security::access_token(&ctx)?,
        signaling_url: rt.signaling_url.clone(),
        expires_at: (claims.expires_at.min(secure_expiry) as f64) * 1000.0,
        security_epoch,
    })
    .map_err(JsValue::from)
}

#[wasm_bindgen]
pub async fn relay_peer_identity(client_id: String, endpoint: String) -> Result<String, JsValue> {
    let peer = ActrId::from_string_repr(&endpoint)
        .map_err(|_| JsValue::from_str("Invalid relay endpoint"))?;
    if peer.to_string_repr() != endpoint {
        return Err(JsValue::from_str("Noncanonical relay endpoint"));
    }
    let ctx = CLIENTS
        .with(|cell| cell.borrow().get(&client_id).map(Rc::clone))
        .ok_or_else(|| JsValue::from_str("Client not registered"))?;
    let rt = ctx.runtime.lock().await;
    if rt
        .actor_id
        .as_ref()
        .is_none_or(|local| local.realm != peer.realm)
    {
        return Err(JsValue::from_str("Relay peer realm mismatch"));
    }
    Ok(peer.to_string_repr())
}

#[wasm_bindgen]
pub async fn register_relay_connection(
    client_id: String,
    endpoint: String,
    channel: String,
    send: js_sys::Function,
) -> Result<String, JsValue> {
    let peer = relay_peer_identity(client_id.clone(), endpoint).await?;
    let ctx = CLIENTS
        .with(|cell| cell.borrow().get(&client_id).map(Rc::clone))
        .ok_or_else(|| JsValue::from_str("Client not registered"))?;
    if !relay_security::authenticated(&ctx, &channel, &peer) {
        return Err(JsValue::from_str(
            "Relay channel is not mutually authenticated",
        ));
    }
    let connection = crate::transport::RelayConnection::new(send);
    ctx.relay_channels
        .borrow_mut()
        .insert(peer.clone(), channel);
    ctx.transport_manager
        .inject_connection(
            &actr_web_common::Dest::Peer(peer.clone()),
            crate::transport::WireHandle::Relay(connection),
        )
        .await
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    Ok(peer)
}

#[wasm_bindgen]
pub async fn relay_channel_closed(
    client_id: String,
    peer: String,
    channel: String,
) -> Result<(), JsValue> {
    let ctx = CLIENTS.with(|cell| cell.borrow().get(&client_id).map(Rc::clone));
    if let Some(ctx) = ctx {
        relay_security::remove(&ctx, &channel);
        let current = ctx.relay_channels.borrow().get(&peer) == Some(&channel);
        if current {
            ctx.relay_channels.borrow_mut().remove(&peer);
            if let Some(transport) = ctx
                .transport_manager
                .existing_transport(&actr_web_common::Dest::Peer(peer))
            {
                let pool = transport.wire_pool();
                if let Some(connection) = pool
                    .get_connection(actr_web_common::ConnType::WebSocket)
                    .await
                {
                    connection
                        .close()
                        .await
                        .map_err(|error| JsValue::from_str(&error.to_string()))?;
                }
                pool.mark_connection_failed(actr_web_common::ConnType::WebSocket);
            }
        }
    }
    Ok(())
}

/// Encode raw bytes to standard Base64 string.
fn bytes_to_base64(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((triple >> 18) & 0x3F) as usize] as char);
        out.push(ALPHABET[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// URL-encode a string using JS `encodeURIComponent`.
fn js_encode_uri_component(s: &str) -> String {
    js_sys::encode_uri_component(s).into()
}

#[cfg(test)]
mod cred_ns_tests {
    use super::*;

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test::wasm_bindgen_test]
    async fn authorized_realm_key_survives_host_restart_and_is_scoped() {
        use actr_protocol::IdentityClaims;
        use actr_protocol::realm_membership::RealmClientKeyFile;
        let root = format!("realm-key-test-{}", uuid::Uuid::new_v4());
        let file = RealmClientKeyFile::new(
            "browser@example.com",
            "provider.example",
            "https://provider.example/issuer",
            &ed25519_dalek::SigningKey::from_bytes(&[57; 32]),
        );
        let mut config: SwConfig = serde_json::from_value(serde_json::json!({
            "issuer_endpoint": file.issuer,
            "signaling_url": "wss://provider.example/signaling/ws",
            "realm": file.realm,
            "client_actr_type": "example.com/browser@1.0.0",
            "service_fingerprint": "test",
            "realm_client_key": file,
        }))
        .unwrap();
        let credential = AIdCredential {
            claims: IdentityClaims {
                realm: file.realm.clone(),
                provider: file.provider_id.clone(),
                client_key: file.key_id.clone(),
                ..Default::default()
            }
            .encode_to_vec()
            .into(),
            ..Default::default()
        };
        let imported = SwRuntime::load_issuer_client(&config, &WebPlatformProvider::new(&root))
            .await
            .unwrap();
        assert_eq!(imported.client_key_id(), file.key_id);
        assert!(imported.accepts_credential(&credential));
        drop(imported);
        config.realm_client_key = None;
        // Recreate the provider: restoration must come from IndexedDB, not memory.
        let restored = SwRuntime::load_issuer_client(&config, &WebPlatformProvider::new(&root))
            .await
            .unwrap();
        assert_eq!(restored.client_key_id(), file.key_id);
        assert!(restored.accepts_credential(&credential));
        assert!(
            SwRuntime::load_issuer_client(
                &config,
                &WebPlatformProvider::new(format!("{root}-other"))
            )
            .await
            .is_err()
        );
        config.realm = "other@example.com".into();
        assert!(
            SwRuntime::load_issuer_client(&config, &WebPlatformProvider::new(&root))
                .await
                .is_err()
        );
        config.realm = file.realm.clone();
        config.issuer_endpoint = "https://other.example/issuer".into();
        assert!(
            SwRuntime::load_issuer_client(&config, &WebPlatformProvider::new(&root))
                .await
                .is_err()
        );
        config.realm_client_key = Some(file);
        assert!(
            SwRuntime::load_issuer_client(&config, &WebPlatformProvider::new(&root))
                .await
                .is_err()
        );
    }

    fn ty(m: &str, n: &str, v: &str) -> ActrType {
        ActrType {
            publisher: m.to_string(),
            name: n.to_string(),
            version: v.to_string(),
        }
    }

    #[test]
    fn cred_kv_namespace_avoids_delimiter_ambiguity() {
        // ("acme:Echo:1_a", "b") vs ("acme/Echo@1", "a_b") produce the SAME string
        // under a delimiter-joined encoding; the length-prefixed encoding must
        // keep them distinct.
        let a = SwRuntime::build_cred_kv_namespace(&ty("acme", "Echo", "1_a"), "b");
        let b = SwRuntime::build_cred_kv_namespace(&ty("acme", "Echo", "1"), "a_b");
        assert_ne!(a, b);
    }

    #[test]
    fn cred_kv_namespace_partitions_by_client_id_and_is_deterministic() {
        let a = SwRuntime::build_cred_kv_namespace(&ty("acme", "Echo", "1.0.0"), "tab1");
        let b = SwRuntime::build_cred_kv_namespace(&ty("acme", "Echo", "1.0.0"), "tab2");
        assert_ne!(a, b);
        let a2 = SwRuntime::build_cred_kv_namespace(&ty("acme", "Echo", "1.0.0"), "tab1");
        assert_eq!(a, a2);
    }
}

#[cfg(test)]
mod stream_codec_tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn native_data_chunk_wire_vector_delivers_the_same_payload() {
        // DataChunk { stream_id: "s", sequence: 7, payload: [0, 255, 42] }.
        let encoded = [0x0a, 1, b's', 0x10, 7, 0x1a, 3, 0, 255, 42];
        let (stream, payload) = decode_stream_payload(&encoded).unwrap();
        assert_eq!(stream, "s");
        assert_eq!(payload.as_ref(), &[0, 255, 42]);
    }

    #[wasm_bindgen_test]
    fn legacy_browser_stream_prefix_remains_receivable() {
        let encoded = [0, 0, 0, 1, b's', 0, 255, 42];
        let (stream, payload) = decode_stream_payload(&encoded).unwrap();
        assert_eq!(stream, "s");
        assert_eq!(payload.as_ref(), &[0, 255, 42]);
    }
}
