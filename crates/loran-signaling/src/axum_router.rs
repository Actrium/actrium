//! Axum Router integration
//!
//! Provides an Axum Router adapter for SignalingServer

#[cfg(test)]
mod revocation_tests;

use crate::server::SignalingServer;
use actr_protocol::{AIdCredential, ActrId, ActrType, IdentityClaims};
use anyhow::{Context as _, Result};
use axum::{
    Router,
    extract::{
        ConnectInfo, Query, State,
        ws::{WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
    routing::get,
};
use base64::Engine as _;
use platform::aid::credential::validator::AIdCredentialValidator;
use platform::config::ActrixConfig;
use platform::monitoring::ServiceCounters;
use platform::realm::Realm as RealmEntity;
use prost::Message as _;
use std::net::SocketAddr;
use std::sync::Arc;
use std::{collections::HashMap, str::FromStr};

/// Signaling Server state (for Axum State)
#[derive(Clone)]
pub struct SignalingState {
    pub server: Arc<SignalingServer>,
    /// Service-level counters for metrics collection.
    pub counters: Option<Arc<ServiceCounters>>,
    /// When true, require actor_id/token query params and validate credentials
    /// before upgrading the WebSocket connection. Production routers set this
    /// to true; the bare `create_signaling_router()` helper leaves it false.
    pub require_url_auth: bool,
}

/// Create a Signaling Axum Router
///
/// Returns a Router that can be mounted onto the main HTTP server
pub async fn create_signaling_router() -> Result<Router> {
    platform::recording::info!("Creating Signaling Axum router");

    let server = SignalingServer::new();
    let state = SignalingState {
        server: Arc::new(server),
        counters: None,
        require_url_auth: false,
    };

    let router = Router::new()
        .route("/ws", get(websocket_handler))
        .with_state(state);

    platform::recording::info!("Signaling Axum router created successfully");
    Ok(router)
}

/// Create a Signaling Axum Router (with config)
///
/// Initializes AIdCredentialValidator and returns a mountable Router
pub async fn create_signaling_router_with_config(
    config: &ActrixConfig,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<Router> {
    create_signaling_router_with_config_and_counters(config, cancel, None).await
}

/// Create a Signaling Axum Router with config and optional service counters.
pub async fn create_signaling_router_with_config_and_counters(
    config: &ActrixConfig,
    cancel: tokio_util::sync::CancellationToken,
    counters: Option<Arc<ServiceCounters>>,
) -> Result<Router> {
    platform::recording::info!("Creating Signaling Axum router with config");

    // Initialize AIdCredentialValidator (Ed25519 mode, using local key_cache DB)
    AIdCredentialValidator::init(&config.sqlite_path)
        .await
        .map_err(|e| {
            platform::recording::error!("Failed to initialize AIdCredentialValidator: {}", e);
            anyhow::anyhow!("AIdCredentialValidator initialization failed: {e}")
        })?;
    platform::recording::info!("✅ AIdCredentialValidator (Ed25519) initialized");

    // Create SignalingServer (apply the egress queue bound from config, issue #351)
    let egress_config = config
        .services
        .signaling
        .as_ref()
        .map(|sc| sc.server.egress.clone())
        .unwrap_or_default();
    let mut server = SignalingServer::with_egress_config(egress_config);
    server.provider_id = config.authority.provider_id().map_err(anyhow::Error::msg)?;

    // Initialize ActorRegistry persistent cache (for restart recovery)
    let cache_ttl_secs = crate::actor_registry_storage::DEFAULT_SERVICE_TTL_SECS;

    if !config.sqlite_path.exists() {
        std::fs::create_dir_all(&config.sqlite_path).with_context(|| {
            format!(
                "Failed to create SQLite data directory: {}",
                config.sqlite_path.display()
            )
        })?;
    }
    let cache_db_file = config.sqlite_path.join("signaling_cache.db");

    match crate::actor_registry_storage::ActorRegistryStorage::new(
        &cache_db_file,
        Some(cache_ttl_secs),
    )
    .await
    {
        Ok(storage) => {
            let storage_arc = Arc::new(storage);
            platform::recording::info!(
                "✅ ActorRegistry cache initialized at: {}",
                cache_db_file.display()
            );

            // Set storage on ActorRegistry
            {
                let mut registry = server.actor_registry.write().await;
                registry.set_storage(storage_arc.clone());

                // Restore service list from cache
                match registry.restore_from_storage().await {
                    Ok(count) => {
                        if count > 0 {
                            platform::recording::info!("✅ Restored {} services from cache", count);
                        }
                    }
                    Err(e) => {
                        platform::recording::warn!(
                            "⚠️  Failed to restore services from cache: {}",
                            e
                        );
                    }
                }
            }

            // Start periodic cleanup task (clean expired data every 5 minutes)
            let storage_for_cleanup = storage_arc.clone();
            let cancel_for_storage = cancel.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(300)); // 5 minutes
                loop {
                    tokio::select! {
                        _ = cancel_for_storage.cancelled() => break,
                        _ = interval.tick() => {}
                    }
                    // Clean up expired service registrations
                    match storage_for_cleanup.cleanup_expired().await {
                        Ok(deleted) => {
                            if deleted > 0 {
                                platform::recording::info!(
                                    "🧹 Cleaned up {} expired services from cache",
                                    deleted
                                );
                            }
                        }
                        Err(e) => {
                            platform::recording::error!(
                                "Failed to cleanup expired services: {:?}",
                                e
                            );
                        }
                    }
                }
                platform::recording::debug!("Signaling storage cleanup task cancelled");
            });
        }
        Err(e) => {
            platform::recording::warn!("⚠️  Failed to initialize ActorRegistry cache: {:?}", e);
            platform::recording::warn!(
                "    Service discovery will work but won't survive restarts"
            );
        }
    }

    // Start the periodic cleanup task for the ActorRegistry memory table (cleanup expired services, avoid stale connections)
    {
        let registry_for_cleanup = server.actor_registry.clone();
        let cancel_for_registry = cancel.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(
                crate::actor_registry::CLEANUP_INTERVAL_SECS,
            ));
            loop {
                tokio::select! {
                    _ = cancel_for_registry.cancelled() => break,
                    _ = interval.tick() => {}
                }
                let mut registry = registry_for_cleanup.write().await;
                registry.cleanup_expired_services();
            }
            platform::recording::debug!("Signaling registry cleanup task cancelled");
        });
    }

    // Initialize rate limiter (if config exists)
    if let Some(signaling_config) = &config.services.signaling {
        let rate_limit_config = &signaling_config.server.rate_limit;

        // Initialize connection rate limiter
        if rate_limit_config.connection.enabled {
            platform::recording::info!(
                "Initializing connection rate limiter: {}/min, burst: {}, max concurrent: {}/IP",
                rate_limit_config.connection.per_minute,
                rate_limit_config.connection.burst_size,
                rate_limit_config.connection.max_concurrent_per_ip
            );
            server.connection_rate_limiter = Some(Arc::new(
                crate::ratelimit::ConnectionRateLimiter::new(rate_limit_config.connection.clone()),
            ));
            platform::recording::info!("✅ Connection rate limiter initialized");
        } else {
            platform::recording::info!("⚠️  Connection rate limiting is disabled");
        }

        // Initialize message rate limiter
        if rate_limit_config.message.enabled {
            platform::recording::info!(
                "Initializing message rate limiter: {}/sec, burst: {}",
                rate_limit_config.message.per_second,
                rate_limit_config.message.burst_size
            );
            server.message_rate_limiter = Some(Arc::new(
                crate::ratelimit::MessageRateLimiter::new(rate_limit_config.message.clone()),
            ));
            platform::recording::info!("✅ Message rate limiter initialized");
        } else {
            platform::recording::info!("⚠️  Message rate limiting is disabled");
        }
    }

    // Create Router
    let state = SignalingState {
        server: Arc::new(server),
        counters,
        require_url_auth: true,
    };
    tokio::spawn(crate::server::follow_realm_retirements(
        state.server.handle(),
        cancel.clone(),
    ));

    let router = Router::new()
        .route("/ws", get(websocket_handler))
        .with_state(state);

    platform::recording::info!("Signaling Axum router created successfully");
    Ok(router)
}

/// WebSocket upgrade handler
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<SignalingState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let client_ip = addr.ip();

    // Check connection rate limit
    if let Some(ref limiter) = state.server.connection_rate_limiter
        && let Err(e) = limiter.check_connection(client_ip).await
    {
        platform::recording::warn!("🚫 IP {} connection rate limit triggered: {}", client_ip, e);
        if let Some(ref ctr) = state.counters {
            ctr.record_request(false, 0.0).await;
        }
        return axum::http::StatusCode::TOO_MANY_REQUESTS.into_response();
    }

    // URL-based credential validation (production path)
    let url_identity = if state.require_url_auth {
        let (actor_id, credential) = match parse_url_identity(&params) {
            Ok(identity) => identity,
            Err(e) => {
                platform::recording::warn!(
                    "🚫 WebSocket auth params missing/invalid: ip={} reason={}",
                    client_ip,
                    e
                );
                return axum::http::StatusCode::UNAUTHORIZED.into_response();
            }
        };

        let (actor_type, client_key) = match AIdCredentialValidator::check(
            &credential,
            &actor_id.realm,
            &state.server.provider_id,
            "actr.signaling",
        )
        .await
        {
            Ok((claims, _in_tolerance)) => {
                let expected_actor_id = actor_id.to_string_repr();
                if claims.actor_id != expected_actor_id {
                    platform::recording::warn!(
                        "🚫 WebSocket credential does not match actor_id: ip={} claim_actor_id={} query_actor_id={}",
                        client_ip,
                        claims.actor_id,
                        expected_actor_id
                    );
                    return axum::http::StatusCode::UNAUTHORIZED.into_response();
                }
                (claims.actor_type, claims.client_key)
            }
            Err(e) => {
                platform::recording::warn!(
                    "🚫 WebSocket credential verification failed: ip={} error={}",
                    client_ip,
                    e
                );
                return axum::http::StatusCode::UNAUTHORIZED.into_response();
            }
        };

        // Only a Realm that is served accepts connections.
        if let Err(e) = RealmEntity::validate_realm(&actor_id.realm).await {
            let (code, message) = RealmEntity::map_validation_error(e);
            platform::recording::warn!(
                "🚫 WebSocket Realm refused: ip={} reason={}",
                client_ip,
                message
            );
            return axum::http::StatusCode::from_u16(code as u16)
                .unwrap_or(axum::http::StatusCode::FORBIDDEN)
                .into_response();
        }

        // A disabled Actor (revoked, or its Realm client key revoked) does
        // not connect again, nor does a credential of a key a reissue moved
        // the Actor from.
        match platform::realm::membership::credential_refused(&actor_id, &client_key).await {
            Ok(false) => {}
            Ok(true) => {
                platform::recording::warn!(
                    "🚫 WebSocket Actor refused: ip={} actor={} is disabled or its credential key was replaced",
                    client_ip,
                    actor_id.to_string_repr()
                );
                return axum::http::StatusCode::FORBIDDEN.into_response();
            }
            Err(e) => {
                platform::recording::warn!("WebSocket Actor state lookup failed: {}", e);
                return axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response();
            }
        }

        Some((actor_id, actor_type, credential))
    } else {
        // The bare router is an in-process integration fixture: it skips
        // credential validation, but still binds a syntactically valid URL
        // identity (with the Actor type read from the unverified claims) so
        // the production relay path can address the connection.
        // Production always uses `require_url_auth=true` above.
        parse_url_identity(&params)
            .ok()
            .and_then(|(actor_id, credential)| {
                let claims = IdentityClaims::decode(credential.claims.as_ref()).ok()?;
                Some((actor_id, claims.actor_type, credential))
            })
    };

    let webrtc_role = params.get("webrtc_role").cloned();
    if let Some(ref role) = webrtc_role {
        platform::recording::info!("🎭 WebRTC role: {}", role);
    }

    // Record successful WebSocket upgrade request
    if let Some(ref ctr) = state.counters {
        ctr.record_request(true, 0.0).await;
    }

    // Cap frame/message sizes at the transport boundary so a client cannot
    // force Axum to allocate a many-megabyte buffer before the post-read size
    // check rejects it (issue #351). Axum defaults are 16 MiB frame / 64 MiB
    // message; both are bounded to `max_frame_bytes` here. The post-read
    // check in the `handle_websocket` receive loop remains as defense in depth.
    let max_frame_bytes = state.server.egress_config.max_frame_bytes as usize;
    ws.max_frame_size(max_frame_bytes)
        .max_message_size(max_frame_bytes)
        .on_upgrade(move |socket| {
            handle_websocket(socket, state, client_ip, url_identity, webrtc_role)
        })
}

/// WebSocket connection handler
async fn handle_websocket(
    socket: WebSocket,
    state: SignalingState,
    client_ip: std::net::IpAddr,
    url_identity: Option<(ActrId, ActrType, AIdCredential)>,
    webrtc_role: Option<String>,
) {
    platform::recording::info!("📡 New WebSocket connection: IP={}", client_ip);

    // Increment service-level active connections counter
    if let Some(ref ctr) = state.counters {
        ctr.inc_conns();
    }

    // Increment connection count
    if let Some(ref limiter) = state.server.connection_rate_limiter {
        limiter.increment_connection(client_ip).await;
    }

    let server_handle = state.server.handle();

    // Call SignalingServer's WebSocket handler function
    if let Err(e) = crate::server::handle_websocket_connection_inner(
        socket,
        server_handle,
        Some(client_ip),
        url_identity,
        webrtc_role,
        state.require_url_auth,
    )
    .await
    {
        platform::recording::error!("WebSocket connection error: {}", e);
    }

    // Decrement service-level active connections counter
    if let Some(ref ctr) = state.counters {
        ctr.dec_conns();
    }

    // Decrement connection count
    if let Some(ref limiter) = state.server.connection_rate_limiter {
        limiter.decrement_connection(client_ip).await;
    }
}

fn parse_url_identity(
    params: &HashMap<String, String>,
) -> std::result::Result<(ActrId, AIdCredential), String> {
    let actor_str = params
        .get("actor_id")
        .ok_or_else(|| "missing actor_id".to_string())?;
    let actor_id =
        ActrId::from_string_repr(actor_str).map_err(|e| format!("invalid actor_id: {e}"))?;

    let key_id = params
        .get("key_id")
        .ok_or_else(|| "missing key_id".to_string())
        .and_then(|s| u32::from_str(s).map_err(|e| format!("invalid key_id: {e}")))?;

    let claims_b64 = params
        .get("claims")
        .ok_or_else(|| "missing claims".to_string())?;
    let claims = base64::engine::general_purpose::STANDARD
        .decode(claims_b64)
        .map_err(|e| format!("invalid claims base64: {e}"))?;

    let signature_b64 = params
        .get("signature")
        .ok_or_else(|| "missing signature".to_string())?;
    let signature = base64::engine::general_purpose::STANDARD
        .decode(signature_b64)
        .map_err(|e| format!("invalid signature base64: {e}"))?;

    Ok((
        actor_id,
        AIdCredential {
            key_id,
            claims: claims.into(),
            signature: signature.into(),
        },
    ))
}
