//! Actrix Signaling Server - based on protobuf SignalingEnvelope
//!
//! Fully based on the protobuf protocol, using WebSocket Binary message transport
//!
//! # Feature Overview
//!
//! ## Core Features Implemented
//!
//! ### Basic Signaling Flow
//! - ✅ Actor unregistration (`UnregisterRequest`)
//! - ✅ Heartbeat mechanism (`Ping` / `Pong`)
//! - ✅ WebRTC signaling relay (`ActrRelay` - ICE / SDP)
//!
//! ### Extended Features
//! - ✅ Service discovery (`DiscoveryRequest` / `DiscoveryResponse`)
//! - ✅ Load-balanced routing (`RouteCandidatesRequest` / `RouteCandidatesResponse`)
//!   - Multi-factor ranking: power reserve, mailbox backlog, geographic distance, client stickiness
//!   - Exact match priority strategy
//! - ✅ Presence subscription (`SubscribeActrUpRequest` / `ActrUpEvent`)
//! - ❌ Credential refresh has been migrated to AIS HTTP
//! - ✅ Load metric storage (`handle_ping()` - stored in ServiceRegistry for load balancing)
//!
//! ## Pending Features (optional enhancements)
//!
//! 1. **Credential validation** (optional security enhancement)
//!    - `handle_actr_to_server()` - validate credential in Actor messages
//!    - `handle_actr_relay()` - validate credential in relay messages
//!
//! 2. **ServiceSpec and ACL persistence** (optional access control)
//!    - `handle_register_request()` - persist service specs and access control rules
//!    - Used for fine-grained inter-service access control

use actr_protocol::{
    AIdCredential, ActrId, ActrRelay, ActrToSignaling, ActrType, ErrorResponse, PeerToSignaling,
    Ping, Pong, Realm, RegisterResponse, RoleAssignment, RoleNegotiation,
    SIGNALING_ENVELOPE_VERSION, SignalingEnvelope, SignalingToActr, actr_relay, actr_to_signaling,
    peer_to_signaling, register_response, signaling_envelope, signaling_to_actr,
};
use futures_util::{SinkExt, StreamExt};
use platform::aid::credential::validator::AIdCredentialValidator;
use platform::realm::Realm as RealmEntity;
use prost::Message as ProstMessage;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info_span;
use uuid::Uuid;

// Axum WebSocket
use axum::extract::ws::{Message as WsMessage, WebSocket};

use crate::load_balancer::LoadBalancer;
use crate::presence::PresenceManager;
use crate::service_registry::ServiceRegistry;
#[cfg(feature = "opentelemetry")]
use crate::trace::{extract_trace_context, inject_trace_context};
use tracing::Instrument;
#[cfg(feature = "opentelemetry")]
use tracing::instrument;

fn type_key(actor_type: &ActrType) -> String {
    format!(
        "{}:{}:{}",
        actor_type.manufacturer, actor_type.name, actor_type.version
    )
}

/// Signaling server state
#[derive(Debug)]
pub struct SignalingServer {
    /// Connected clients
    pub clients: Arc<RwLock<HashMap<String, ClientConnection>>>,
    /// Index for looking up client_id by ActorId
    pub actor_id_index: Arc<RwLock<HashMap<ActrId, String>>>,
    /// Service registry
    pub service_registry: Arc<RwLock<ServiceRegistry>>,
    /// Presence subscription manager
    pub presence_manager: Arc<RwLock<PresenceManager>>,
    /// Connection rate limiter
    pub connection_rate_limiter: Option<Arc<crate::ratelimit::ConnectionRateLimiter>>,
    /// Message rate limiter
    pub message_rate_limiter: Option<Arc<crate::ratelimit::MessageRateLimiter>>,
}

/// Client connection info
#[derive(Debug)]
pub struct ClientConnection {
    pub id: String,
    pub actor_id: Option<ActrId>,
    pub credential: Option<AIdCredential>,
    pub direct_sender: tokio::sync::mpsc::UnboundedSender<WsMessage>,
    pub client_ip: Option<std::net::IpAddr>,
    /// WebRTC role: "answer" or None (defaults to offer)
    pub webrtc_role: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CleanupMode {
    ConnectionClosed,
    ExplicitUnregister,
}

struct StaleClient {
    id: String,
    direct_sender: tokio::sync::mpsc::UnboundedSender<WsMessage>,
}

/// Signaling server handle - used to operate the server in async tasks
#[derive(Debug, Clone)]
pub struct SignalingServerHandle {
    pub clients: Arc<RwLock<HashMap<String, ClientConnection>>>,
    pub actor_id_index: Arc<RwLock<HashMap<ActrId, String>>>,
    pub service_registry: Arc<RwLock<ServiceRegistry>>,
    pub presence_manager: Arc<RwLock<PresenceManager>>,
    pub connection_rate_limiter: Option<Arc<crate::ratelimit::ConnectionRateLimiter>>,
    pub message_rate_limiter: Option<Arc<crate::ratelimit::MessageRateLimiter>>,
}
impl SignalingServerHandle {
    /// Create a SignalingEnvelope
    #[cfg_attr(
        feature = "opentelemetry",
        instrument(level = "debug", skip_all, fields(reply_for))
    )]
    fn create_envelope(
        &self,
        flow: signaling_envelope::Flow,
        reply_for: Option<&str>,
    ) -> SignalingEnvelope {
        #[allow(unused_mut)]
        let mut envelope = SignalingEnvelope {
            envelope_version: SIGNALING_ENVELOPE_VERSION,
            envelope_id: Uuid::new_v4().to_string(),
            reply_for: reply_for.map(|id| id.to_string()),
            traceparent: None,
            tracestate: None,
            flow: Some(flow),
        };
        platform::recording::debug!(
            "Created envelope: envelope_id={}, reply_for={reply_for:?}",
            envelope.envelope_id,
        );
        envelope
    }

    #[cfg_attr(feature = "opentelemetry", instrument(level = "debug", skip_all))]
    fn create_new_envelope(&self, flow: signaling_envelope::Flow) -> SignalingEnvelope {
        self.create_envelope(flow, None)
    }
}

impl Default for SignalingServer {
    fn default() -> Self {
        Self::new()
    }
}

impl SignalingServer {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(RwLock::new(HashMap::new())),
            actor_id_index: Arc::new(RwLock::new(HashMap::new())),
            service_registry: Arc::new(RwLock::new(ServiceRegistry::new())),
            presence_manager: Arc::new(RwLock::new(PresenceManager::new())),
            connection_rate_limiter: None, // Initialized in axum_router based on config
            message_rate_limiter: None,    // Initialized in axum_router based on config
        }
    }
}

/// Handle WebSocket connection
pub async fn handle_websocket_connection(
    websocket: WebSocket,
    server: SignalingServerHandle,
    client_ip: Option<std::net::IpAddr>,
    url_identity: Option<(ActrId, AIdCredential)>,
    webrtc_role: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let client_id = Uuid::new_v4().to_string();
    platform::recording::info!(
        "🔗 New WebSocket client connection: {} (IP: {:?})",
        client_id,
        client_ip
    );

    // Split read and write streams
    let (mut ws_sender, mut ws_receiver) = websocket.split();

    // Create a dedicated send channel for peer-to-peer messages
    let (direct_tx, mut direct_rx) = tokio::sync::mpsc::unbounded_channel();
    let control_tx = direct_tx.clone();

    // Register client (including dedicated sender)
    let (actor_id, credential) = match url_identity {
        Some(identity) => (Some(identity.0), Some(identity.1)),
        None => (None, None),
    };
    let actor_id_for_registry = actor_id.clone();
    let stale_clients = {
        let mut clients_guard = server.clients.write().await;

        // Remove existing connections for the same actor (to avoid stale mappings), and send a WS Close frame to let the old connection exit cleanly.
        let stale_clients = if let Some(ref aid) = actor_id {
            let stale_clients = remove_stale_actor_clients(&mut clients_guard, aid);

            // Synchronously update actor_id_index: acquire actor_id_index write lock while holding clients write lock
            // Maintain lock order: clients -> actor_id_index
            let mut actor_index = server.actor_id_index.write().await;
            actor_index.insert(aid.clone(), client_id.clone());

            stale_clients
        } else {
            Vec::new()
        };

        clients_guard.insert(
            client_id.clone(),
            ClientConnection {
                id: client_id.clone(),
                actor_id,
                credential,
                direct_sender: direct_tx,
                client_ip,
                webrtc_role: webrtc_role.clone(),
            },
        );

        stale_clients
    };

    close_stale_clients(stale_clients, &server).await;

    // Register actor in service registry for discovery/routing (only when URL identity is provided)
    if let Some(actor_id_for_registry) = actor_id_for_registry {
        {
            // Load pending service_spec and ws_address from DB (written by AIS during registration)
            let (service_spec, ws_address) = {
                let db = platform::storage::db::get_database();
                let pool = db.get_pool();
                let row: Option<(Option<Vec<u8>>, Option<String>)> = sqlx::query_as(
                    "SELECT service_spec_blob, ws_address FROM pending_registration \
                     WHERE serial_number = ? AND realm_id = ?",
                )
                .bind(actor_id_for_registry.serial_number as i64)
                .bind(actor_id_for_registry.realm.realm_id as i64)
                .fetch_optional(pool)
                .await
                .ok()
                .flatten();
                match row {
                    Some((blob, ws)) => (
                        blob.and_then(|b| actr_protocol::ServiceSpec::decode(b.as_slice()).ok()),
                        ws,
                    ),
                    None => (None, None),
                }
            };

            let service_name = format!(
                "{}:{}",
                actor_id_for_registry.r#type.manufacturer, actor_id_for_registry.r#type.name
            );
            let mut registry = server.service_registry.write().await;
            if let Err(e) = registry.register_service_full(
                actor_id_for_registry.clone(),
                service_name,
                vec![],
                None,
                service_spec,
                None,
                ws_address,
            ) {
                platform::recording::warn!("Failed to register actor in service registry: {}", e);
            }
        }
        // Notify presence subscribers about the new actor
        {
            let actor_id_for_presence = actor_id_for_registry.clone();
            let presence = server.presence_manager.read().await;
            let subscribers = presence
                .get_subscribers_with_acl(&actor_id_for_presence)
                .await;
            drop(presence);

            for subscriber_id in &subscribers {
                // Find the client_id for this subscriber
                let sub_client_id = {
                    let idx = server.actor_id_index.read().await;
                    idx.get(subscriber_id).cloned()
                };
                if let Some(sub_cid) = sub_client_id {
                    let event = actr_protocol::ActrUpEvent {
                        actor_id: actor_id_for_presence.clone(),
                    };
                    let flow =
                        signaling_envelope::Flow::ServerToActr(actr_protocol::SignalingToActr {
                            target: subscriber_id.clone(),
                            payload: Some(actr_protocol::signaling_to_actr::Payload::ActrUpEvent(
                                event,
                            )),
                        });
                    let envelope = server.create_envelope(flow, None);
                    if let Err(e) = send_envelope_to_client(&sub_cid, envelope, &server).await {
                        platform::recording::warn!(
                            "Failed to send ActrUp notification to {}: {}",
                            subscriber_id.to_string_repr(),
                            e
                        );
                    }
                }
            }
        }
    }

    // Task for handling client messages
    let server_for_receive = server.clone();
    let client_id_for_receive = client_id.clone();

    let receive_task = tokio::spawn(async move {
        while let Some(msg) = ws_receiver.next().await {
            match msg {
                Ok(WsMessage::Binary(data)) => {
                    if let Err(e) =
                        handle_client_envelope(&data, &client_id_for_receive, &server_for_receive)
                            .await
                    {
                        platform::recording::error!("Error handling client signaling: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    platform::recording::error!("WebSocket error: {}", e);
                    break;
                }
                Ok(message) => {
                    if !handle_websocket_control_message(
                        message,
                        &control_tx,
                        &client_id_for_receive,
                    ) {
                        break;
                    }
                }
            }
        }

        // Clean up client
        cleanup_client(
            &client_id_for_receive,
            &server_for_receive,
            CleanupMode::ConnectionClosed,
        )
        .await;
    });

    // Task for sending messages
    let mut send_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                // Handle peer-to-peer messages
                msg = direct_rx.recv() => {
                    match msg {
                        Some(message) => {
                            let should_close = matches!(message, WsMessage::Close(_));
                            if ws_sender.send(message).await.is_err() {
                                break;
                            }
                            if should_close {
                                break;
                            }
                        }
                        None => break,
                    }
                }
            }
        }
    });
    let mut receive_task = receive_task;

    // Wait for either task to complete
    tokio::select! {
        result = &mut receive_task => {
            if let Err(e) = result {
                platform::recording::warn!(
                    "WebSocket receive task join error for client {}: {}",
                    client_id,
                    e
                );
            }
            send_task.abort();
            let _ = send_task.await;
        },
        result = &mut send_task => {
            if let Err(e) = result {
                platform::recording::warn!(
                    "WebSocket send task join error for client {}: {}",
                    client_id,
                    e
                );
            }
            receive_task.abort();
            let _ = receive_task.await;
        },
    }

    // Clean up client connection
    cleanup_client(&client_id, &server, CleanupMode::ConnectionClosed).await;
    platform::recording::info!("🔌 Client {} disconnected", client_id);

    Ok(())
}

fn remove_stale_actor_clients(
    clients: &mut HashMap<String, ClientConnection>,
    actor_id: &ActrId,
) -> Vec<StaleClient> {
    let stale_client_ids = clients
        .iter()
        .filter_map(|(cid, conn)| (conn.actor_id.as_ref() == Some(actor_id)).then_some(cid.clone()))
        .collect::<Vec<_>>();

    stale_client_ids
        .into_iter()
        .filter_map(|cid| {
            clients.remove(&cid).map(|client| StaleClient {
                id: cid,
                direct_sender: client.direct_sender,
            })
        })
        .collect()
}

async fn close_stale_clients(stale_clients: Vec<StaleClient>, server: &SignalingServerHandle) {
    for stale_client in stale_clients {
        let close_frame = axum::extract::ws::CloseFrame {
            code: axum::extract::ws::close_code::POLICY,
            reason: "displaced by new connection".into(),
        };

        if stale_client
            .direct_sender
            .send(WsMessage::Close(Some(close_frame)))
            .is_err()
        {
            platform::recording::debug!(
                "Stale WebSocket client {} send task already closed",
                stale_client.id
            );
        }

        platform::recording::info!(
            "🧹 Closing stale WebSocket client {} for actor reconnect",
            stale_client.id
        );

        if let Some(ref limiter) = server.message_rate_limiter {
            limiter.remove_connection(&stale_client.id).await;
        }
    }
}

fn handle_websocket_control_message(
    message: WsMessage,
    control_tx: &tokio::sync::mpsc::UnboundedSender<WsMessage>,
    client_id: &str,
) -> bool {
    match message {
        WsMessage::Ping(payload) => {
            platform::recording::debug!("Received WebSocket Ping, replying Pong: {}", client_id);
            if control_tx.send(WsMessage::Pong(payload)).is_err() {
                platform::recording::warn!(
                    "WebSocket Pong send queue closed for client {}",
                    client_id
                );
                return false;
            }
            true
        }
        WsMessage::Pong(_) => {
            platform::recording::debug!("Received WebSocket Pong: {}", client_id);
            true
        }
        WsMessage::Close(_) => {
            platform::recording::info!("Client {} initiated disconnect", client_id);
            false
        }
        WsMessage::Text(_) => {
            platform::recording::warn!("Received Text WebSocket message, ignoring: {}", client_id);
            true
        }
        _ => {
            platform::recording::warn!(
                "Received unsupported non-Binary WebSocket message, ignoring: {}",
                client_id
            );
            true
        }
    }
}

/// Handle a SignalingEnvelope sent by the client
async fn handle_client_envelope(
    data: &[u8],
    client_id: &str,
    server: &SignalingServerHandle,
) -> Result<(), Box<dyn std::error::Error>> {
    // Check message rate limit
    if let Some(ref limiter) = server.message_rate_limiter
        && let Err(e) = limiter.check_message(client_id).await
    {
        platform::recording::warn!(
            "🚫 Connection {} message rate limit triggered: {}",
            client_id,
            e
        );
        // Send error response
        let error_response = ErrorResponse {
            code: 429,
            message: e,
        };
        let error_envelope =
            server.create_new_envelope(signaling_envelope::Flow::EnvelopeError(error_response));
        send_envelope_to_client(client_id, error_envelope, server).await?;
        return Ok(());
    }

    // Decode protobuf message
    let envelope = SignalingEnvelope::decode(data)?;

    #[cfg(feature = "opentelemetry")]
    let remote_context = extract_trace_context(&envelope);

    let span = info_span!(
        "signaling.handle_envelope",
        envelope_id = %envelope.envelope_id,
        client_id = %client_id
    );
    #[cfg(feature = "opentelemetry")]
    {
        use tracing_opentelemetry::OpenTelemetrySpanExt;
        let _ = span.set_parent(remote_context.clone());
    }

    async move {
        platform::recording::debug!(
            "📨 Received signaling message envelope_id={}",
            envelope.envelope_id
        );

        // Handle message based on flow direction
        match envelope.flow {
            Some(signaling_envelope::Flow::PeerToServer(peer_to_server)) => {
                handle_peer_to_server(peer_to_server, client_id, server, &envelope.envelope_id)
                    .await
            }
            Some(signaling_envelope::Flow::ActrToServer(actr_to_server)) => {
                handle_actr_to_server(actr_to_server, client_id, server, &envelope.envelope_id)
                    .await
            }
            Some(signaling_envelope::Flow::ActrRelay(ref relay)) => {
                #[cfg(feature = "opentelemetry")]
                {
                    handle_actr_relay(
                        relay.clone(),
                        client_id,
                        server,
                        &envelope.envelope_id,
                        remote_context,
                    )
                    .await
                }
                #[cfg(not(feature = "opentelemetry"))]
                {
                    handle_actr_relay(relay.clone(), client_id, server, &envelope.envelope_id).await
                }
            }
            Some(signaling_envelope::Flow::EnvelopeError(error)) => {
                platform::recording::error!(
                    "Received envelope error: code={}, message={}",
                    error.code,
                    error.message
                );
                Ok(())
            }
            _ => {
                platform::recording::warn!("Unknown signaling flow direction");
                Ok(())
            }
        }
    }
    .instrument(span)
    .await
}

/// Handle PeerToSignaling flow (pre-registration)
#[cfg_attr(feature = "opentelemetry", instrument(level = "debug", skip_all, fields(client_id, envelope_id = request_envelope_id)))]
async fn handle_peer_to_server(
    peer_to_server: PeerToSignaling,
    client_id: &str,
    server: &SignalingServerHandle,
    request_envelope_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    match peer_to_server.payload {
        Some(peer_to_signaling::Payload::RegisterRequest(_)) => {
            // Registration has been migrated to AIS HTTP; signaling no longer relays registration requests
            platform::recording::warn!(
                "⚠️  RegisterRequest rejected: use /ais/register HTTP endpoint instead"
            );
            send_register_error(
                client_id,
                410,
                "Registration via signaling is no longer supported; use /ais/register HTTP endpoint",
                server,
                request_envelope_id,
            )
            .await?;
        }
        None => {
            platform::recording::warn!("PeerToSignaling message missing payload");
        }
    }
    Ok(())
}

/// Send registration error response
#[cfg_attr(feature = "opentelemetry", instrument(level = "debug", skip_all, fields(client_id, envelope_id = request_envelope_id)))]
async fn send_register_error(
    client_id: &str,
    code: u32,
    message: &str,
    server: &SignalingServerHandle,
    request_envelope_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let error_response = ErrorResponse {
        code,
        message: message.to_string(),
    };

    let response = RegisterResponse {
        result: Some(register_response::Result::Error(error_response)),
    };

    // Create temporary ActrId (for response)
    let temp_actor_id = ActrId {
        realm: Realm { realm_id: 0 },
        serial_number: 0,
        r#type: ActrType {
            manufacturer: "temp".to_string(),
            name: "temp".to_string(),
            version: "1.0.0".to_string(),
        },
    };

    let flow = signaling_envelope::Flow::ServerToActr(SignalingToActr {
        target: temp_actor_id,
        payload: Some(signaling_to_actr::Payload::RegisterResponse(response)),
    });

    let response_envelope = server.create_envelope(flow, Some(request_envelope_id));

    send_envelope_to_client(client_id, response_envelope, server).await?;

    Ok(())
}

/// Handle ActrToSignaling flow (after registration)
#[cfg_attr(feature = "opentelemetry", instrument(level = "debug", skip_all, fields(client_id, envelope_id = request_envelope_id, actr_id = %actr_to_server.source.to_string_repr())))]
async fn handle_actr_to_server(
    actr_to_server: ActrToSignaling,
    client_id: &str,
    server: &SignalingServerHandle,
    request_envelope_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let source = actr_to_server.source.clone();

    platform::recording::info!(
        "📬 Processing message from Actor {}",
        source.to_string_repr()
    );

    // Validate that Realm exists, is not expired, and is in normal state
    let realm_id = source.realm.realm_id;
    if let Err(e) = RealmEntity::validate_realm(realm_id).await {
        let (code, msg) = RealmEntity::map_validation_error(e);
        platform::recording::warn!(
            "⚠️  Actor {} realm validation failed: {}",
            source.to_string_repr(),
            msg
        );
        send_error_response(
            client_id,
            &source,
            code,
            &format!("Realm validation failed: {msg}"),
            server,
            Some(request_envelope_id),
        )
        .await?;
        return Ok(());
    }

    // Validate credential and get tolerance period status
    let in_tolerance_period = match AIdCredentialValidator::check(
        &actr_to_server.credential,
        source.realm.realm_id,
    )
    .await
    {
        Ok((_claims, in_tolerance)) => in_tolerance,
        Err(e) => {
            platform::recording::warn!(
                "⚠️  Actor {} credential validation failed: {}",
                source.to_string_repr(),
                e
            );
            // Send error response
            send_error_response(
                client_id,
                &source,
                401,
                &format!("Credential validation failed: {e}"),
                server,
                Some(request_envelope_id),
            )
            .await?;
            return Ok(());
        }
    };

    match actr_to_server.payload {
        Some(actr_to_signaling::Payload::Ping(ping)) => {
            handle_ping(
                source,
                ping,
                client_id,
                server,
                request_envelope_id,
                in_tolerance_period,
            )
            .await?;
        }
        Some(actr_to_signaling::Payload::UnregisterRequest(req)) => {
            handle_unregister(source, req, client_id, server, request_envelope_id).await?;
        }
        Some(actr_to_signaling::Payload::DiscoveryRequest(req)) => {
            handle_discovery_request(source, req, client_id, server, request_envelope_id).await?;
        }
        Some(actr_to_signaling::Payload::RouteCandidatesRequest(req)) => {
            handle_route_candidates_request(source, req, client_id, server, request_envelope_id)
                .await?;
        }
        Some(actr_to_signaling::Payload::GetServiceSpecRequest(req)) => {
            handle_get_service_spec_request(source, req, client_id, server, request_envelope_id)
                .await?;
        }
        Some(actr_to_signaling::Payload::SubscribeActrUpRequest(req)) => {
            handle_subscribe_actr_up(source, req, client_id, server, request_envelope_id).await?;
        }
        Some(actr_to_signaling::Payload::UnsubscribeActrUpRequest(req)) => {
            handle_unsubscribe_actr_up(source, req, client_id, server, request_envelope_id).await?;
        }
        Some(actr_to_signaling::Payload::GetSigningKeyRequest(req)) => {
            handle_get_signing_key_request(source, req, client_id, server, request_envelope_id)
                .await?;
        }
        Some(actr_to_signaling::Payload::Error(error)) => {
            platform::recording::error!(
                "Received client error report (Actor {}): code={}, message={}",
                source.to_string_repr(),
                error.code,
                error.message
            );
        }
        None => {
            platform::recording::warn!("ActrToSignaling message missing payload");
        }
    }

    Ok(())
}

/// Handle heartbeat (Ping)
async fn handle_ping(
    source: ActrId,
    ping: Ping,
    client_id: &str,
    server: &SignalingServerHandle,
    request_envelope_id: &str,
    in_tolerance_period: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    platform::recording::info!(
        "💓 Received Actor {} heartbeat: availability={}, power_reserve={:.2}, mailbox_backlog={:.2}, sticky_clients={}{}",
        source.to_string_repr(),
        ping.availability,
        ping.power_reserve,
        ping.mailbox_backlog,
        ping.sticky_client_ids.len(),
        if in_tolerance_period {
            " [⚠️ Key in tolerance period]"
        } else {
            ""
        }
    );

    // Store load metrics in ServiceRegistry
    let mut registry = server.service_registry.write().await;
    if let Err(e) = registry.update_load_metrics(
        &source,
        ping.availability,
        ping.power_reserve,
        ping.mailbox_backlog,
    ) {
        platform::recording::warn!(
            "Failed to update load metrics for Actor {}: {}, attempting to restore service from database",
            source.to_string_repr(),
            e
        );

        // Attempt to restore service from database
        match registry.restore_service_from_storage(&source).await {
            Ok(true) => {
                platform::recording::info!(
                    "✅ Successfully restored service registration for Actor {} from database",
                    source.to_string_repr()
                );

                // Retry load metrics update after restoration
                if let Err(e2) = registry.update_load_metrics(
                    &source,
                    ping.availability,
                    ping.power_reserve,
                    ping.mailbox_backlog,
                ) {
                    platform::recording::error!(
                        "❌ Still unable to update load metrics for Actor {} after database restoration: {}",
                        source.to_string_repr(),
                        e2
                    );
                } else {
                    platform::recording::info!(
                        "✅ Load metrics updated successfully after service restoration for Actor {}",
                        source.to_string_repr()
                    );
                }
            }
            Ok(false) => {
                platform::recording::warn!(
                    "⚠️  Service info for Actor {} not found in database (may have expired or was never registered)",
                    source.to_string_repr()
                );
                // TODO: optional - add a warning in Pong response to prompt client to re-register
            }
            Err(e) => {
                platform::recording::error!(
                    "❌ Failed to restore service for Actor {} from database: {}",
                    source.to_string_repr(),
                    e
                );
            }
        }
    }
    drop(registry);

    // Create Pong response
    let mut pong = Pong {
        // Wall-clock seconds used as an opaque echo value only. This is not a
        // sequence number: clock adjustments can make it repeat or go
        // backwards, so consumers must not rely on it for ordering.
        seq: chrono::Utc::now().timestamp() as u64,
        suggest_interval_secs: Some(30),
        credential_warning: None,
    };

    // If credential key is in tolerance period, add a warning
    if in_tolerance_period {
        platform::recording::warn!(
            "⚠️  Actor {} credential key is in tolerance period",
            source.to_string_repr()
        );
        pong.credential_warning = Some(actr_protocol::CredentialWarning {
            r#type: actr_protocol::credential_warning::WarningType::KeyInTolerancePeriod as i32,
            message:
                "Your credential key is in tolerance period. Please update your credential soon."
                    .to_string(),
        });
    }

    let flow = signaling_envelope::Flow::ServerToActr(SignalingToActr {
        target: source,
        payload: Some(signaling_to_actr::Payload::Pong(pong)),
    });

    let response_envelope = server.create_envelope(flow, Some(request_envelope_id));

    send_envelope_to_client(client_id, response_envelope, server).await?;

    Ok(())
}

/// Handle unregistration
#[cfg_attr(feature = "opentelemetry", instrument(level = "debug", skip_all, fields(client_id, envelope_id = request_envelope_id)))]
async fn handle_unregister(
    source: ActrId,
    req: actr_protocol::UnregisterRequest,
    client_id: &str,
    server: &SignalingServerHandle,
    request_envelope_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    platform::recording::info!(
        "👋 Actor {} unregistered: reason={:?}",
        source.to_string_repr(),
        req.reason.as_deref().unwrap_or("not provided")
    );

    // Send UnregisterResponse
    let response = actr_protocol::UnregisterResponse {
        result: Some(actr_protocol::unregister_response::Result::Success(
            actr_protocol::unregister_response::UnregisterOk {},
        )),
    };

    let flow = signaling_envelope::Flow::ServerToActr(SignalingToActr {
        target: source,
        payload: Some(signaling_to_actr::Payload::UnregisterResponse(response)),
    });

    let response_envelope = server.create_envelope(flow, Some(request_envelope_id));
    send_envelope_to_client(client_id, response_envelope, server).await?;

    // Clean up client connection
    cleanup_client(client_id, server, CleanupMode::ExplicitUnregister).await;

    Ok(())
}

/// Resolve client_id via actor_id_index, keeping the index in sync with clients
#[allow(dead_code)]
async fn resolve_client_id_by_actor_id(
    actor_id: &ActrId,
    server: &SignalingServerHandle,
) -> Result<String, String> {
    let clients_guard = server.clients.read().await;
    let index_guard = server.actor_id_index.read().await;
    let client_id = index_guard.get(actor_id).cloned();

    let client_id = match client_id {
        Some(id) => id,
        None => {
            platform::recording::warn!(
                "⚠️  Actor {} missing client_id index, may not be registered yet or already cleaned up",
                actor_id.to_string_repr()
            );
            return Err("client_id not found for actor_id".into());
        }
    };

    if !clients_guard.contains_key(&client_id) {
        platform::recording::warn!(
            "⚠️  Actor {} index points to non-existent client {}, index may be stale",
            actor_id.to_string_repr(),
            client_id
        );
        return Err("actor_id_index stale for actor_id".into());
    }

    Ok(client_id)
}

#[derive(Debug, PartialEq, Eq)]
enum RelaySourceConnectionStatus {
    Current,
    ClientMissing,
    ClientActorMismatch,
    ActorIndexMissing,
    Displaced { current_client_id: String },
}

async fn relay_source_connection_status(
    client_id: &str,
    source: &ActrId,
    server: &SignalingServerHandle,
) -> RelaySourceConnectionStatus {
    let clients_guard = server.clients.read().await;
    let Some(client) = clients_guard.get(client_id) else {
        return RelaySourceConnectionStatus::ClientMissing;
    };

    if client.actor_id.as_ref() != Some(source) {
        return RelaySourceConnectionStatus::ClientActorMismatch;
    }

    let actor_index = server.actor_id_index.read().await;
    match actor_index.get(source) {
        Some(current_client_id) if current_client_id == client_id => {
            RelaySourceConnectionStatus::Current
        }
        Some(current_client_id) => RelaySourceConnectionStatus::Displaced {
            current_client_id: current_client_id.clone(),
        },
        None => RelaySourceConnectionStatus::ActorIndexMissing,
    }
}

/// Handle ActrRelay (WebRTC signaling relay)
#[cfg_attr(feature = "opentelemetry", instrument(level = "debug", skip_all, fields(client_id, envelope_id = request_envelope_id, actr_id = %relay.source.to_string_repr(), target_id = %relay.target.to_string_repr())))]
async fn handle_actr_relay(
    relay: ActrRelay,
    client_id: &str,
    server: &SignalingServerHandle,
    request_envelope_id: &str,
    #[cfg(feature = "opentelemetry")] remote_context: opentelemetry::Context,
) -> Result<(), Box<dyn std::error::Error>> {
    let source = relay.source.clone();
    let target = &relay.target;
    // Validate source Actor realm (exists, not expired, and in normal state)
    let realm_id = source.realm.realm_id;
    if let Err(e) = RealmEntity::validate_realm(realm_id).await {
        let (code, msg) = RealmEntity::map_validation_error(e);
        platform::recording::warn!(
            "⚠️  Actor {} realm validation failed: {}",
            source.to_string_repr(),
            msg
        );
        send_error_response(
            client_id,
            &source,
            code,
            &format!("Realm validation failed: {msg}"),
            server,
            Some(request_envelope_id),
        )
        .await?;
        return Ok(());
    }

    platform::recording::info!(
        "🔀 Relay signaling: {} -> {}",
        source.to_string_repr(),
        target.to_string_repr()
    );

    platform::recording::debug!("handle_actr_relay: relay={:?}", relay);

    // ACL check: can source relay to target?
    use platform::realm::acl::ActorAcl;
    let source_realm = source.realm.realm_id;
    let target_realm = target.realm.realm_id;

    // Unified ACL check: same-realm / cross-realm both go through the same rule query
    let source_type = type_key(&source.r#type);
    let target_type = type_key(&target.r#type);

    let can_relay = ActorAcl::can_discover(source_realm, target_realm, &source_type, &target_type)
        .await
        .unwrap_or(false);

    if !can_relay {
        platform::recording::warn!(
            "⚠️  ACL denied relay: {} -> {}",
            source.to_string_repr(),
            target.to_string_repr()
        );
        send_error_response(
            client_id,
            &source,
            403,
            "ACL policy denies relay to target actor",
            server,
            Some(request_envelope_id),
        )
        .await?;
        return Ok(());
    }

    // Validate credential and retain claims for identity verification below.
    let claims = match AIdCredentialValidator::check(&relay.credential, source.realm.realm_id).await
    {
        Ok((claims, _)) => claims,
        Err(e) => {
            platform::recording::warn!(
                "Actor {} credential validation failed: {}",
                source.to_string_repr(),
                e
            );
            send_error_response(
                client_id,
                &source,
                401,
                &format!("Credential validation failed: {e}"),
                server,
                Some(request_envelope_id),
            )
            .await?;
            return Ok(());
        }
    };

    // Verify that the actor_id bound to the credential matches relay.source,
    // preventing clients from forging their source identity.
    let source_repr = source.to_string_repr();
    if claims.actor_id != source_repr {
        platform::recording::warn!(
            "relay.source does not match credential actor_id: source={}, credential={}",
            source_repr,
            claims.actor_id
        );
        send_error_response(
            client_id,
            &source,
            403,
            "Source identity mismatch: relay.source does not match credential",
            server,
            Some(request_envelope_id),
        )
        .await?;
        return Ok(());
    }

    let connection_status = relay_source_connection_status(client_id, &source, server).await;
    if connection_status != RelaySourceConnectionStatus::Current {
        platform::recording::warn!(
            "Dropping ActrRelay from non-current WebSocket: client_id={}, source={}, status={:?}",
            client_id,
            source.to_string_repr(),
            connection_status
        );
        return Ok(());
    }

    // Role negotiation: server decides offerer/answerer and notifies both parties
    if let Some(actr_relay::Payload::RoleNegotiation(RoleNegotiation { from, to, .. })) =
        relay.payload.clone()
    {
        let clients_guard = server.clients.read().await;

        // Use determine_webrtc_role function to determine role
        let is_offerer = determine_webrtc_role(&from, &to, &clients_guard);

        drop(clients_guard);

        // Send RoleAssignment to from
        let new_relay = ActrRelay {
            // source: peer actor, target: receiver of this assignment
            source: from.clone(),
            credential: relay.credential.clone(),
            target: to.clone(),
            payload: Some(actr_relay::Payload::RoleAssignment(RoleAssignment {
                is_offerer,
                remote_fixed: None,
            })),
        };
        send_role_assignment(
            &from,
            server,
            new_relay.clone(),
            #[cfg(feature = "opentelemetry")]
            remote_context.clone(),
        )
        .await?;

        // Send RoleAssignment to to
        let new_relay = ActrRelay {
            // source: peer actor, target: receiver of this assignment
            source: from.clone(),
            credential: relay.credential.clone(),
            target: to.clone(),
            payload: Some(actr_relay::Payload::RoleAssignment(RoleAssignment {
                is_offerer: !is_offerer,
                remote_fixed: None,
            })),
        };

        send_role_assignment(
            &to,
            server,
            new_relay,
            #[cfg(feature = "opentelemetry")]
            remote_context,
        )
        .await?;

        return Ok(());
    }

    // Find target client and forward other relay messages
    let clients_guard = server.clients.read().await;
    let target_client_id = clients_guard.iter().find_map(|(id, client)| {
        client.actor_id.as_ref().and_then(|actor_id| {
            if actor_id.realm.realm_id == target.realm.realm_id
                && actor_id.serial_number == target.serial_number
            {
                Some(id.clone())
            } else {
                None
            }
        })
    });

    if let Some(target_client_id) = target_client_id {
        // Reconstruct envelope and forward
        let flow = signaling_envelope::Flow::ActrRelay(relay);
        #[allow(unused_mut)]
        let mut forward_envelope = server.create_new_envelope(flow);

        // Inject the original trace context into the forwarded envelope to ensure end-to-end tracing
        #[cfg(feature = "opentelemetry")]
        inject_trace_context(&remote_context, &mut forward_envelope);
        send_envelope_to_client(&target_client_id, forward_envelope, server).await?;

        platform::recording::info!("✅ Signaling relay successful");
    } else {
        platform::recording::warn!("⚠️ Target Actor {} not found", target.to_string_repr());
    }

    Ok(())
}

// Compute ActorId key for sorting, ensuring role assignment is deterministic
fn actor_order_key(id: &ActrId) -> (u32, u64, String, String) {
    (
        id.realm.realm_id,
        id.serial_number,
        id.r#type.manufacturer.clone(),
        id.r#type.name.clone(),
    )
}

/// Determine whether the initiator is the offerer based on both parties role preferences and ActorId
///
/// # Role determination rules:
/// 1. If one party explicitly requests to be "answer" and the other does not, satisfy that request
/// 2. If both parties have the same preference (both want "answer" or neither specifies), fall back to ActrId static sorting logic
///
/// # Arguments
/// * `from` - initiator ActorId
/// * `to` - receiver ActorId
/// * `clients` - client connection map, used to look up role preferences
///
/// # Returns
/// * `true` - initiator should be offerer
/// * `false` - initiator should be answerer
fn determine_webrtc_role(
    from: &ActrId,
    to: &ActrId,
    clients: &HashMap<String, ClientConnection>,
) -> bool {
    // Look up both parties role preferences from client connections
    let from_role = clients
        .values()
        .find(|c| c.actor_id.as_ref() == Some(from))
        .and_then(|c| c.webrtc_role.as_deref());

    let to_role = clients
        .values()
        .find(|c| c.actor_id.as_ref() == Some(to))
        .and_then(|c| c.webrtc_role.as_deref());

    let is_offerer = if from_role == Some("answer") && to_role != Some("answer") {
        false
    } else if to_role == Some("answer") && from_role != Some("answer") {
        true
    } else {
        // Other cases (both prefer answer or both have no preference), use ActorId reverse ordering
        actor_order_key(from) > actor_order_key(to)
    };

    platform::recording::info!(
        "⚖️ Role negotiation complete: {} (role={:?}) -> {} (role={:?}), is_offerer={}",
        from.to_string_repr(),
        from_role,
        to.to_string_repr(),
        to_role,
        is_offerer
    );

    is_offerer
}

#[cfg_attr(feature = "opentelemetry", instrument(level = "debug", skip_all))]
async fn send_role_assignment(
    target_actor: &ActrId,
    server: &SignalingServerHandle,
    relay: ActrRelay,
    #[cfg(feature = "opentelemetry")] remote_context: opentelemetry::Context,
) -> Result<(), Box<dyn std::error::Error>> {
    let flow = signaling_envelope::Flow::ActrRelay(relay);
    #[allow(unused_mut)]
    let mut envelope = server.create_new_envelope(flow);

    #[cfg(feature = "opentelemetry")]
    inject_trace_context(&remote_context, &mut envelope);

    let mut buf = Vec::new();
    envelope.encode(&mut buf)?;

    let clients_guard = server.clients.read().await;
    if let Some(client) = clients_guard.values().find(|client| {
        client.actor_id.as_ref().is_some_and(|id| {
            id.realm.realm_id == target_actor.realm.realm_id
                && id.serial_number == target_actor.serial_number
        })
    }) {
        platform::recording::debug!(
            "send_role_assignment: sending envelope to client {:?}",
            client.actor_id
        );
        client
            .direct_sender
            .send(WsMessage::Binary(buf.into()))
            .map_err(|e| e.into())
    } else {
        platform::recording::warn!(
            "⚠️ send_role_assignment: target Actor {} not found",
            target_actor.to_string_repr()
        );
        Ok(())
    }
}

/// Send SignalingEnvelope to client
#[cfg_attr(feature = "opentelemetry", instrument(level = "debug", skip_all, fields(client_id, envelope_id = envelope.envelope_id)))]
async fn send_envelope_to_client(
    client_id: &str,
    #[allow(unused_mut)] mut envelope: SignalingEnvelope,
    server: &SignalingServerHandle,
) -> Result<(), Box<dyn std::error::Error>> {
    let clients_guard = server.clients.read().await;

    if let Some(client) = clients_guard.get(client_id) {
        #[cfg(feature = "opentelemetry")]
        {
            use tracing_opentelemetry::OpenTelemetrySpanExt;
            let context = tracing::Span::current().context();
            inject_trace_context(&context, &mut envelope);
        }

        // Encode protobuf
        let mut buf = Vec::new();
        envelope.encode(&mut buf)?;

        // Send Binary message
        match client.direct_sender.send(WsMessage::Binary(buf.into())) {
            Ok(_) => {
                platform::recording::info!("✅ Successfully sent envelope to client {}", client_id);
                Ok(())
            }
            Err(e) => {
                platform::recording::error!("❌ Send failed: {}", e);
                Err(format!("Send failed: {e}").into())
            }
        }
    } else {
        platform::recording::warn!("⚠️ Client {} not found", client_id);
        Err(format!("Client {client_id} not found").into())
    }
}

/// Clean up client connection
async fn cleanup_client(client_id: &str, server: &SignalingServerHandle, mode: CleanupMode) {
    let (removed_client, actor_cleanup) = {
        let mut clients_guard = server.clients.write().await;
        let removed_client = clients_guard.remove(client_id);

        let actor_cleanup = removed_client
            .as_ref()
            .and_then(|client| client.actor_id.clone())
            .map(|actor_id| {
                let replacement_client_id = clients_guard
                    .iter()
                    .find(|(id, client)| {
                        id.as_str() != client_id && client.actor_id.as_ref() == Some(&actor_id)
                    })
                    .map(|(id, _)| id.clone());

                (actor_id, replacement_client_id)
            });

        if let Some((actor_id, replacement_client_id)) = actor_cleanup.as_ref() {
            let mut actor_index = server.actor_id_index.write().await;
            match actor_index.get(actor_id).cloned() {
                Some(mapped_client) if mapped_client == client_id => {
                    if let Some(replacement_client_id) = replacement_client_id {
                        actor_index.insert(actor_id.clone(), replacement_client_id.clone());
                        platform::recording::info!(
                            "🔁 Actor {} switched to new client {}, preserving index",
                            actor_id.to_string_repr(),
                            replacement_client_id
                        );
                    } else {
                        actor_index.remove(actor_id);
                    }
                }
                Some(mapped_client) => {
                    platform::recording::warn!(
                        "⚠️  Actor {} index points to another client {}, preserving index",
                        actor_id.to_string_repr(),
                        mapped_client
                    );
                }
                None => {
                    if let Some(replacement_client_id) = replacement_client_id {
                        actor_index.insert(actor_id.clone(), replacement_client_id.clone());
                        platform::recording::warn!(
                            "⚠️  Actor {} index missing during cleanup, fixed to new client {}",
                            actor_id.to_string_repr(),
                            replacement_client_id
                        );
                    } else {
                        platform::recording::warn!(
                            "⚠️  Actor {} no index entry found during cleanup",
                            actor_id.to_string_repr()
                        );
                    }
                }
            }
        }

        (removed_client, actor_cleanup)
    };

    if let Some(_client) = removed_client {
        if let Some((actor_id, replacement_client_id)) = actor_cleanup {
            platform::recording::info!(
                "🧹 Cleaning up Actor {} connection",
                actor_id.to_string_repr()
            );

            if let Some(replacement_client_id) = replacement_client_id {
                platform::recording::info!(
                    "⏭️ Actor {} already has new connection {}, skipping service unregistration",
                    actor_id.to_string_repr(),
                    replacement_client_id
                );
            } else {
                let mut registry = server.service_registry.write().await;
                match mode {
                    CleanupMode::ConnectionClosed => {
                        registry.unregister_actor_memory_only(&actor_id);
                    }
                    CleanupMode::ExplicitUnregister => {
                        registry.unregister_actor(&actor_id);
                    }
                }
            }
        }

        // Remove message rate limiter
        if let Some(ref limiter) = server.message_rate_limiter {
            limiter.remove_connection(client_id).await;
        }
    }
}

/// Handle service discovery request
#[cfg_attr(feature = "opentelemetry", instrument(level = "debug", skip_all, fields(client_id, envelope_id = request_envelope_id)))]
async fn handle_discovery_request(
    source: ActrId,
    req: actr_protocol::DiscoveryRequest,
    client_id: &str,
    server: &SignalingServerHandle,
    request_envelope_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    platform::recording::info!(
        "🔍 Processing Discovery request from Actor {}: manufacturer={:?}, limit={}",
        source.to_string_repr(),
        req.manufacturer.as_deref().unwrap_or("*"),
        req.limit.unwrap_or(64)
    );

    // Query all services from ServiceRegistry
    let registry = server.service_registry.read().await;
    let services = registry.discover_all(req.manufacturer.as_deref());
    let total_count = services.len(); // Save count before moving

    // Apply ACL filtering (if ACL is enabled)
    use platform::realm::acl::ActorAcl;
    let source_realm = source.realm.realm_id;
    let source_type = type_key(&source.r#type);

    let mut acl_filtered_services = Vec::new();

    // ACL always enabled: filter services based on ACL rules
    for service in services {
        let target_realm = service.actor_id.realm.realm_id;
        let target_type = type_key(&service.actor_id.r#type);

        match ActorAcl::can_discover(source_realm, target_realm, &source_type, &target_type).await {
            Ok(true) => acl_filtered_services.push(service),
            Ok(false) => {
                platform::recording::debug!(
                    "ACL denied discovery: {} (realm {}) cannot discover {} (realm {})",
                    source.to_string_repr(),
                    source_realm,
                    service.actor_id.to_string_repr(),
                    target_realm
                );
            }
            Err(e) => {
                platform::recording::warn!(
                    "ACL check failed for {} (realm {}) -> {} (realm {}): {}",
                    source.to_string_repr(),
                    source_realm,
                    service.actor_id.to_string_repr(),
                    target_realm,
                    e
                );
            }
        }
    }
    platform::recording::info!(
        "ACL filtering: {} -> {} services",
        total_count,
        acl_filtered_services.len()
    );

    // Aggregate services by ActrType (using HashMap for deduplication)
    use std::collections::HashMap;
    let mut type_map: HashMap<String, actr_protocol::discovery_response::TypeEntry> =
        HashMap::new();

    for service in acl_filtered_services {
        let type_key = type_key(&service.actor_id.r#type);

        // If this type has not been added yet, create a new entry
        type_map.entry(type_key).or_insert_with(|| {
            let (fingerprint, description, published_at, tags) = service
                .service_spec
                .as_ref()
                .map(|spec| {
                    (
                        spec.fingerprint.clone(),
                        spec.description.clone(),
                        spec.published_at,
                        spec.tags.clone(),
                    )
                })
                .unwrap_or_else(|| ("unknown".to_string(), None, None, Vec::new()));

            actr_protocol::discovery_response::TypeEntry {
                actr_type: service.actor_id.r#type.clone(),
                name: service.service_name.clone(),
                description,
                service_fingerprint: fingerprint,
                published_at,
                tags,
            }
        });
    }

    // Convert to Vec and apply limit
    let mut entries: Vec<_> = type_map.into_values().collect();
    let limit = req.limit.unwrap_or(64) as usize;
    entries.truncate(limit);

    drop(registry);

    platform::recording::info!(
        "✅ Returned {} service types for Actor {}",
        source.to_string_repr(),
        entries.len()
    );

    let response = actr_protocol::DiscoveryResponse {
        result: Some(actr_protocol::discovery_response::Result::Success(
            actr_protocol::discovery_response::DiscoveryOk { entries },
        )),
    };

    let flow = signaling_envelope::Flow::ServerToActr(SignalingToActr {
        target: source,
        payload: Some(signaling_to_actr::Payload::DiscoveryResponse(response)),
    });

    let response_envelope = server.create_envelope(flow, Some(request_envelope_id));
    send_envelope_to_client(client_id, response_envelope, server).await?;

    Ok(())
}

/// Handle route candidates request (load balancing)
#[cfg_attr(feature = "opentelemetry", instrument(level = "debug", skip_all, fields(client_id, envelope_id = request_envelope_id)))]
async fn handle_route_candidates_request(
    source: ActrId,
    req: actr_protocol::RouteCandidatesRequest,
    client_id: &str,
    server: &SignalingServerHandle,
    request_envelope_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    platform::recording::info!(
        "🎯 Processing RouteCandidates request from Actor {}: target_type={}/{}",
        source.to_string_repr(),
        req.target_type.manufacturer,
        req.target_type.name,
    );

    // Query all instances matching target_type from ServiceRegistry
    let registry = server.service_registry.read().await;
    let candidates_raw = registry.find_by_actr_type(&req.target_type);
    drop(registry);

    // Filter out ghost candidates with no active WebSocket connection.
    // ServiceRegistry may briefly retain offline entries due to network disruptions,
    // Cross-checking against the clients map eliminates them.
    let connected_actor_ids: std::collections::HashSet<ActrId> = {
        let clients = server.clients.read().await;
        clients
            .values()
            .filter_map(|c| c.actor_id.clone())
            .collect()
    };
    let candidates: Vec<_> = candidates_raw
        .into_iter()
        .filter(|c| {
            let connected = connected_actor_ids.contains(&c.actor_id);
            if !connected {
                platform::recording::warn!(
                    "🚫 Filtered ghost candidate: Actor {} (no active connection)",
                    c.actor_id.to_string_repr()
                );
            }
            connected
        })
        .collect();

    let total_candidates = candidates.len();

    if candidates.is_empty() {
        platform::recording::info!(
            "⚠️  No service instances found for type {}/{}",
            req.target_type.manufacturer,
            req.target_type.name
        );
    } else {
        platform::recording::info!(
            "📋 Found {} candidate instances for type {}/{}",
            total_candidates,
            req.target_type.manufacturer,
            req.target_type.name
        );
    }

    // Apply ACL filtering
    use platform::realm::acl::ActorAcl;
    let source_realm = source.realm.realm_id;
    let source_type = type_key(&source.r#type);

    let mut acl_filtered_candidates = Vec::new();
    for candidate in candidates {
        let target_realm = candidate.actor_id.realm.realm_id;
        let target_type = type_key(&candidate.actor_id.r#type);

        match ActorAcl::can_discover(source_realm, target_realm, &source_type, &target_type).await {
            Ok(true) => acl_filtered_candidates.push(candidate),
            Ok(false) => {
                platform::recording::debug!(
                    "ACL denied route candidate: {} (realm {}) cannot access {} (realm {})",
                    source.to_string_repr(),
                    source_realm,
                    candidate.actor_id.to_string_repr(),
                    target_realm
                );
            }
            Err(e) => {
                platform::recording::warn!(
                    "ACL check failed for {} (realm {}) -> {} (realm {}): {}",
                    source.to_string_repr(),
                    source_realm,
                    candidate.actor_id.to_string_repr(),
                    target_realm,
                    e
                );
            }
        }
    }
    platform::recording::info!(
        "ACL filtering for route candidates: {} -> {} candidates",
        total_candidates,
        acl_filtered_candidates.len()
    );

    // Extract client location from request (if provided)
    let client_location = req.client_location.as_ref().and_then(|loc| {
        if let (Some(lat), Some(lon)) = (loc.latitude, loc.longitude) {
            Some((lat, lon))
        } else {
            None
        }
    });

    // Extract ws_address (rank_candidates moves candidates, collect beforehand)
    let ws_address_map: Vec<(ActrId, Option<String>)> = acl_filtered_candidates
        .iter()
        .map(|c| (c.actor_id.clone(), c.ws_address.clone()))
        .collect();

    // Ranking: ACL filtered, same ActrType means protocol compatible, apply ranking_factors in order
    // is_exact_match flag handled internally by LB via client_fingerprint
    let ranked_actor_ids = LoadBalancer::rank_candidates(
        &acl_filtered_candidates,
        req.criteria.as_ref(),
        req.client_fingerprint.trim(),
        Some(client_id),
        client_location,
    );

    platform::recording::info!(
        "✅ Returned {} candidates for Actor {}",
        source.to_string_repr(),
        ranked_actor_ids.len(),
    );

    let ws_address_map: Vec<actr_protocol::WsAddressEntry> = ranked_actor_ids
        .iter()
        .filter_map(|id| {
            ws_address_map.iter().find(|(a, _)| a == id).map(|(_, ws)| {
                actr_protocol::WsAddressEntry {
                    candidate_id: id.clone(),
                    ws_address: ws.clone(),
                }
            })
        })
        .collect();

    let response = actr_protocol::RouteCandidatesResponse {
        result: Some(actr_protocol::route_candidates_response::Result::Success(
            actr_protocol::route_candidates_response::RouteCandidatesOk {
                candidates: ranked_actor_ids,
                ws_address_map,
            },
        )),
    };

    let flow = signaling_envelope::Flow::ServerToActr(SignalingToActr {
        target: source,
        payload: Some(signaling_to_actr::Payload::RouteCandidatesResponse(
            response,
        )),
    });

    let response_envelope = server.create_envelope(flow, Some(request_envelope_id));
    send_envelope_to_client(client_id, response_envelope, server).await?;

    Ok(())
}

/// Handle GetServiceSpec request
#[cfg_attr(feature = "opentelemetry", instrument(level = "debug", skip_all, fields(client_id, envelope_id = request_envelope_id)))]
async fn handle_get_service_spec_request(
    source: ActrId,
    req: actr_protocol::GetServiceSpecRequest,
    client_id: &str,
    server: &SignalingServerHandle,
    request_envelope_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let service_name = req.name.as_str();
    platform::recording::info!(
        "Handle GetServiceSpec request for Actor {} name={}",
        source.to_string_repr(),
        service_name
    );

    // Find matching ServiceSpec in ServiceRegistry by spec name
    let result = server
        .service_registry
        .read()
        .await
        .discover_by_spec_name(service_name)
        .into_iter()
        .find_map(|service| service.service_spec.clone())
        .map(actr_protocol::get_service_spec_response::Result::Success)
        .unwrap_or_else(|| {
            actr_protocol::get_service_spec_response::Result::Error(ErrorResponse {
                code: 404,
                message: format!("Service specification not found for name={service_name}"),
            })
        });

    let response = actr_protocol::GetServiceSpecResponse {
        result: Some(result),
    };

    let flow = signaling_envelope::Flow::ServerToActr(SignalingToActr {
        target: source,
        payload: Some(signaling_to_actr::Payload::GetServiceSpecResponse(response)),
    });

    let response_envelope = server.create_envelope(flow, Some(request_envelope_id));
    send_envelope_to_client(client_id, response_envelope, server).await?;

    Ok(())
}

/// Handle GetSigningKey request — proxies key lookup via local KeyCache
async fn handle_get_signing_key_request(
    source: ActrId,
    req: actr_protocol::GetSigningKeyRequest,
    client_id: &str,
    server: &SignalingServerHandle,
    request_envelope_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let key_id = req.key_id;
    platform::recording::debug!(
        "Actor {} requested signing key: key_id={}",
        source.to_string_repr(),
        key_id
    );

    let response = match AIdCredentialValidator::get_key_bytes(key_id).await {
        Ok(Some(pubkey_bytes)) => actr_protocol::GetSigningKeyResponse {
            key_id,
            pubkey: pubkey_bytes.into(),
        },
        Ok(None) => {
            platform::recording::warn!(
                "GetSigningKeyRequest: key_id={} not found in cache",
                key_id
            );
            send_error_response(
                client_id,
                &source,
                404,
                &format!("signing key not found: key_id={key_id}"),
                server,
                Some(request_envelope_id),
            )
            .await?;
            return Ok(());
        }
        Err(e) => {
            platform::recording::error!(
                "GetSigningKeyRequest: key_id={} lookup error: {}",
                key_id,
                e
            );
            send_error_response(
                client_id,
                &source,
                500,
                "internal error looking up signing key",
                server,
                Some(request_envelope_id),
            )
            .await?;
            return Ok(());
        }
    };

    let flow = signaling_envelope::Flow::ServerToActr(SignalingToActr {
        target: source,
        payload: Some(signaling_to_actr::Payload::GetSigningKeyResponse(response)),
    });
    let envelope = server.create_envelope(flow, Some(request_envelope_id));
    send_envelope_to_client(client_id, envelope, server).await?;

    Ok(())
}

/// Handle subscribe to Actor up events
#[cfg_attr(feature = "opentelemetry", instrument(level = "debug", skip_all, fields(client_id, envelope_id = request_envelope_id)))]
async fn handle_subscribe_actr_up(
    source: ActrId,
    req: actr_protocol::SubscribeActrUpRequest,
    client_id: &str,
    server: &SignalingServerHandle,
    request_envelope_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    platform::recording::info!(
        "📢 Actor {} subscribed to service up events: target_type={}/{}",
        source.to_string_repr(),
        req.target_type.manufacturer,
        req.target_type.name
    );

    // Add subscription to PresenceManager
    let mut presence = server.presence_manager.write().await;
    presence.subscribe(source.clone(), req.target_type);
    drop(presence);

    let response = actr_protocol::SubscribeActrUpResponse {
        result: Some(actr_protocol::subscribe_actr_up_response::Result::Success(
            actr_protocol::subscribe_actr_up_response::SubscribeOk {},
        )),
    };

    let flow = signaling_envelope::Flow::ServerToActr(SignalingToActr {
        target: source,
        payload: Some(signaling_to_actr::Payload::SubscribeActrUpResponse(
            response,
        )),
    });

    let response_envelope = server.create_envelope(flow, Some(request_envelope_id));
    send_envelope_to_client(client_id, response_envelope, server).await?;

    Ok(())
}

/// Handle unsubscribe from Actor up events
#[cfg_attr(feature = "opentelemetry", tracing::instrument(level = "debug", skip_all, fields(client_id, envelope_id = request_envelope_id)))]
async fn handle_unsubscribe_actr_up(
    source: ActrId,
    req: actr_protocol::UnsubscribeActrUpRequest,
    client_id: &str,
    server: &SignalingServerHandle,
    request_envelope_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    platform::recording::info!(
        "🔕 Actor {} unsubscribed from service up events: target_type={}/{}",
        source.to_string_repr(),
        req.target_type.manufacturer,
        req.target_type.name
    );

    // Remove subscription from PresenceManager
    let mut presence = server.presence_manager.write().await;
    let removed = presence.unsubscribe(&source, &req.target_type);
    drop(presence);

    if !removed {
        platform::recording::warn!(
            "Actor {} was not subscribed to {}/{}",
            source.to_string_repr(),
            req.target_type.manufacturer,
            req.target_type.name
        );
    }

    let response = actr_protocol::UnsubscribeActrUpResponse {
        result: Some(
            actr_protocol::unsubscribe_actr_up_response::Result::Success(
                actr_protocol::unsubscribe_actr_up_response::UnsubscribeOk {},
            ),
        ),
    };

    let flow = signaling_envelope::Flow::ServerToActr(SignalingToActr {
        target: source,
        payload: Some(signaling_to_actr::Payload::UnsubscribeActrUpResponse(
            response,
        )),
    });

    let response_envelope = server.create_envelope(flow, Some(request_envelope_id));
    send_envelope_to_client(client_id, response_envelope, server).await?;

    Ok(())
}

/// Send generic error response
#[cfg_attr(feature = "opentelemetry", tracing::instrument(level = "debug", skip_all, fields(client_id, reply_for = ?reply_for, target = ?target)))]
async fn send_error_response(
    client_id: &str,
    target: &ActrId,
    code: u32,
    message: &str,
    server: &SignalingServerHandle,
    reply_for: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let error_response = ErrorResponse {
        code,
        message: message.to_string(),
    };

    let flow = signaling_envelope::Flow::ServerToActr(SignalingToActr {
        target: target.clone(),
        payload: Some(signaling_to_actr::Payload::Error(error_response)),
    });

    let response_envelope = server.create_envelope(flow, reply_for);
    send_envelope_to_client(client_id, response_envelope, server).await?;

    Ok(())
}

// Main function removed - SignalingServer can now be instantiated and started from other modules

#[cfg(test)]
mod tests {
    use super::*;
    use actr_protocol::{ActrType, Realm};
    use std::collections::HashMap;

    /// Create test ActrId
    fn create_test_actr_id(serial: u64) -> ActrId {
        ActrId {
            serial_number: serial,
            realm: Realm { realm_id: 1001 },
            r#type: ActrType {
                manufacturer: "test".to_string(),
                name: "device".to_string(),
                version: "1.0.0".to_string(),
            },
        }
    }

    /// Create test ClientConnection
    fn create_test_client(actor_id: ActrId, webrtc_role: Option<String>) -> ClientConnection {
        create_test_client_with_id(&uuid::Uuid::new_v4().to_string(), actor_id, webrtc_role)
    }

    fn create_test_client_with_id(
        id: &str,
        actor_id: ActrId,
        webrtc_role: Option<String>,
    ) -> ClientConnection {
        ClientConnection {
            id: id.to_string(),
            actor_id: Some(actor_id),
            credential: None,
            direct_sender: tokio::sync::mpsc::unbounded_channel().0,
            client_ip: None,
            webrtc_role,
        }
    }

    fn create_test_server_handle() -> SignalingServerHandle {
        let server = SignalingServer::new();
        SignalingServerHandle {
            clients: server.clients.clone(),
            actor_id_index: server.actor_id_index.clone(),
            service_registry: server.service_registry.clone(),
            presence_manager: server.presence_manager.clone(),
            connection_rate_limiter: server.connection_rate_limiter.clone(),
            message_rate_limiter: server.message_rate_limiter.clone(),
        }
    }

    async fn register_test_service(server: &SignalingServerHandle, actor_id: &ActrId) {
        server
            .service_registry
            .write()
            .await
            .register_service(
                actor_id.clone(),
                "test_service".to_string(),
                vec!["TestMessage".to_string()],
                None,
            )
            .unwrap();
    }

    #[test]
    fn test_websocket_ping_enqueues_pong() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        assert!(handle_websocket_control_message(
            WsMessage::Ping(Vec::from("probe").into()),
            &tx,
            "client",
        ));

        match rx.try_recv().expect("pong should be queued") {
            WsMessage::Pong(payload) => assert_eq!(payload.as_ref(), b"probe"),
            other => panic!("expected Pong, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_reconnect_closes_removed_stale_client() {
        let server = create_test_server_handle();
        let actor_id = create_test_actr_id(1);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        let mut clients = HashMap::new();
        clients.insert(
            "old-client".to_string(),
            ClientConnection {
                id: "old-client".to_string(),
                actor_id: Some(actor_id.clone()),
                credential: None,
                direct_sender: tx,
                client_ip: None,
                webrtc_role: None,
            },
        );

        let stale_clients = remove_stale_actor_clients(&mut clients, &actor_id);
        assert!(
            !clients.contains_key("old-client"),
            "stale client should be removed from routing map"
        );

        close_stale_clients(stale_clients, &server).await;

        assert!(
            matches!(rx.try_recv(), Ok(WsMessage::Close(_))),
            "stale client should receive a WebSocket Close frame"
        );
    }

    #[tokio::test]
    async fn test_cleanup_stale_connection_keeps_replacement_service() {
        let server = create_test_server_handle();
        let actor_id = create_test_actr_id(1);
        register_test_service(&server, &actor_id).await;

        {
            let mut clients = server.clients.write().await;
            clients.insert(
                "old-client".to_string(),
                create_test_client_with_id("old-client", actor_id.clone(), None),
            );
            clients.insert(
                "new-client".to_string(),
                create_test_client_with_id("new-client", actor_id.clone(), None),
            );
        }
        server
            .actor_id_index
            .write()
            .await
            .insert(actor_id.clone(), "new-client".to_string());

        cleanup_client("old-client", &server, CleanupMode::ConnectionClosed).await;

        assert!(
            server.clients.read().await.contains_key("new-client"),
            "replacement client should remain connected"
        );
        assert_eq!(
            server.actor_id_index.read().await.get(&actor_id).cloned(),
            Some("new-client".to_string())
        );
        assert_eq!(
            server
                .service_registry
                .read()
                .await
                .discover_by_service_name("test_service")
                .len(),
            1,
            "stale connection cleanup must not unregister a live replacement"
        );
    }

    #[tokio::test]
    async fn test_connection_cleanup_removes_service_from_memory_only() {
        let server = create_test_server_handle();
        let actor_id = create_test_actr_id(1);
        register_test_service(&server, &actor_id).await;

        server.clients.write().await.insert(
            "client".to_string(),
            create_test_client_with_id("client", actor_id.clone(), None),
        );
        server
            .actor_id_index
            .write()
            .await
            .insert(actor_id.clone(), "client".to_string());

        cleanup_client("client", &server, CleanupMode::ConnectionClosed).await;

        assert_eq!(
            server
                .service_registry
                .read()
                .await
                .discover_by_service_name("test_service")
                .len(),
            0,
            "connection cleanup should remove offline services from memory"
        );
        assert!(
            !server.actor_id_index.read().await.contains_key(&actor_id),
            "connection cleanup should clear the old actor index"
        );
    }

    #[tokio::test]
    async fn test_relay_source_connection_status_allows_current_client() {
        let server = create_test_server_handle();
        let actor_id = create_test_actr_id(1);

        server.clients.write().await.insert(
            "client".to_string(),
            create_test_client_with_id("client", actor_id.clone(), None),
        );
        server
            .actor_id_index
            .write()
            .await
            .insert(actor_id.clone(), "client".to_string());

        assert_eq!(
            relay_source_connection_status("client", &actor_id, &server).await,
            RelaySourceConnectionStatus::Current
        );
    }

    #[tokio::test]
    async fn test_relay_source_connection_status_rejects_displaced_client() {
        let server = create_test_server_handle();
        let actor_id = create_test_actr_id(1);

        {
            let mut clients = server.clients.write().await;
            clients.insert(
                "old-client".to_string(),
                create_test_client_with_id("old-client", actor_id.clone(), None),
            );
            clients.insert(
                "new-client".to_string(),
                create_test_client_with_id("new-client", actor_id.clone(), None),
            );
        }
        server
            .actor_id_index
            .write()
            .await
            .insert(actor_id.clone(), "new-client".to_string());

        assert_eq!(
            relay_source_connection_status("old-client", &actor_id, &server).await,
            RelaySourceConnectionStatus::Displaced {
                current_client_id: "new-client".to_string()
            }
        );
    }

    #[tokio::test]
    async fn test_relay_source_connection_status_rejects_actor_mismatch() {
        let server = create_test_server_handle();
        let client_actor = create_test_actr_id(1);
        let relay_source = create_test_actr_id(2);

        server.clients.write().await.insert(
            "client".to_string(),
            create_test_client_with_id("client", client_actor, None),
        );
        server
            .actor_id_index
            .write()
            .await
            .insert(relay_source.clone(), "client".to_string());

        assert_eq!(
            relay_source_connection_status("client", &relay_source, &server).await,
            RelaySourceConnectionStatus::ClientActorMismatch
        );
    }

    #[test]
    fn test_determine_webrtc_role() {
        // Test data: (from_serial, to_serial, from_role, to_role, expected_is_offerer, description)
        let test_cases = vec![
            (
                2,
                1,
                None,
                None,
                true,
                "Neither has preference, use ActorId reverse order",
            ),
            (
                2,
                1,
                None,
                Some("answer"),
                true,
                "Receiver prefers answer, initiator should be offerer",
            ),
            (
                2,
                1,
                Some("answer"),
                None,
                false,
                "Initiator prefers answer, initiator should be answerer",
            ),
            (
                2,
                1,
                Some("answer"),
                Some("answer"),
                true,
                "Both prefer answer, fall back to ActorId reverse order",
            ),
            (
                1,
                2,
                None,
                None,
                false,
                "serial 1 < serial 2, initiator should be answerer",
            ),
            (
                1,
                2,
                Some("answer"),
                Some("answer"),
                false,
                "Both prefer answer, serial 1 should be answerer",
            ),
        ];

        for (from_serial, to_serial, from_role, to_role, expected, desc) in test_cases {
            let from = create_test_actr_id(from_serial);
            let to = create_test_actr_id(to_serial);

            // Create clients map
            let mut clients = HashMap::new();
            clients.insert(
                format!("client_{from_serial}"),
                create_test_client(from.clone(), from_role.map(|s| s.to_string())),
            );
            clients.insert(
                format!("client_{to_serial}"),
                create_test_client(to.clone(), to_role.map(|s| s.to_string())),
            );

            let is_offerer = determine_webrtc_role(&from, &to, &clients);

            assert_eq!(
                is_offerer, expected,
                "Test failed: {desc} (from={from_serial}, to={to_serial}, from_role={from_role:?}, to_role={to_role:?})"
            );
        }
    }
}
