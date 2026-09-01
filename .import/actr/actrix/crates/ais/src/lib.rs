//! Actor Identity Service (AIS) - ActrId registration and credential issuance.
//!
//! # Overview
//!
//! AIS is the core Actrix identity service. It allocates ActrId serial numbers,
//! issues signed AIdCredential tokens, generates time-limited TURN credentials,
//! and returns renewal tokens for `/ais/renew`.
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────┐
//! │   Client     │
//! └──────┬───────┘
//!        │ POST /ais/register (protobuf)
//!        ▼
//! ┌──────────────────────────────────────────┐
//! │  AIS Service                             │
//! │  ┌────────────┐      ┌────────────────┐ │
//! │  │  Handlers  │─────▶│  AIdIssuer     │ │
//! │  └────────────┘      └────────┬───────┘ │
//! │                               │         │
//! │  ┌──────────────────┐  ┌─────▼──────┐  │
//! │  │ SN Generator     │  │ KeyStorage │  │
//! │  │ (Snowflake)      │  │ (SQLite)   │  │
//! │  └──────────────────┘  └────────────┘  │
//! └──────────────┬───────────────────────────┘
//!                │ KS Client
//!                ▼
//!         ┌─────────────┐
//!         │ KS Service  │ (key generation)
//!         └─────────────┘
//! ```
//!
//! # Core Flow
//!
//! ## Registration
//!
//! 1. Receive a protobuf `RegisterRequest`.
//! 2. Allocate a 54-bit serial number with Snowflake.
//! 3. Sign identity claims through Signer and build an AIdCredential.
//! 4. Generate a time-limited TURN credential.
//! 5. Generate and persist a renewal token hash.
//! 6. Return `RegisterResponse` with ActrId, access credential, TURN credential,
//!    and renewal token.
//!
//! ## Key Management
//!
//! - **Fetch**: On startup, load the cached key from local SQLite; if expired, fetch from Signer
//! - **Refresh**: Background task checks every 10 minutes; refreshes keys expiring within 10 minutes
//! - **Tolerance**: Keys remain usable for 24 hours after expiry (avoids clock skew disruptions)
//!
//! # Usage Example
//!
//! ```no_run
//! use ais::create_ais_router;
//! use platform::config::{AisConfig, ActrixConfig};
//! use tokio_util::sync::CancellationToken;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Create configuration
//! let global_config = ActrixConfig::default();
//! let ais_config = AisConfig::default();
//! let cancel = CancellationToken::new();
//!
//! // Create the AIS router
//! let router = create_ais_router(&ais_config, &global_config, cancel).await?;
//!
//! // Integrate into the main HTTP service
//! // let app = Router::new().nest("/ais", router);
//! # Ok(())
//! # }
//! ```
//!
//! # Security Considerations
//!
//! - **Renewal token rotation**: renewal tokens are stored as hashes and rotated on use.
//! - **Encrypted transport**: Tokens are encrypted using ECIES; only services holding the private key can decrypt them
//! - **Serial number uniqueness**: The Snowflake algorithm guarantees global uniqueness in distributed environments
//! - **Key rotation**: old keys remain verifiable during the tolerance window.
//!
//! # Configuration Options
//!
//! See [`platform::config::AisConfig`] for the full configuration reference.
#![deny(clippy::disallowed_macros)]

pub mod handlers;
mod idempotency;
pub mod issuer;
pub mod ratelimit;
pub mod renewal;
pub mod signer_client_wrapper;
mod sn;
mod storage;

pub use issuer::{AIdIssuer, IssuerConfig, KeyCacheInfo};

use crate::handlers::{AISState, create_router};
use crate::signer_client_wrapper::create_signer_client;
use anyhow::{Context, Result};
use axum::Router;
use platform::config::AisConfig;
use platform::monitoring::ServiceCounters;
use std::sync::Arc;

/// Create the AIS router, following the project's HttpRouterService architecture
pub async fn create_ais_router(
    config: &AisConfig,
    global_config: &platform::config::ActrixConfig,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<Router> {
    create_ais_router_with_counters(config, global_config, cancel, None).await
}

/// Create AIS router with optional service counters for metrics collection.
pub async fn create_ais_router_with_counters(
    config: &AisConfig,
    global_config: &platform::config::ActrixConfig,
    cancel: tokio_util::sync::CancellationToken,
    counters: Option<Arc<ServiceCounters>>,
) -> Result<Router> {
    let state = create_ais_state(config, global_config, cancel, counters).await?;
    Ok(create_router_from_state(state))
}

/// Create the AIS service state (issuer + idempotency store).
///
/// The returned [`AISState`] owns the issuer inside an `Arc`; the control
/// plane can grab `state.issuer` to trigger authenticated key rotation on the
/// same issuer that backs the public `/ais` router built from this state.
pub async fn create_ais_state(
    config: &AisConfig,
    global_config: &platform::config::ActrixConfig,
    cancel: tokio_util::sync::CancellationToken,
    counters: Option<Arc<ServiceCounters>>,
) -> Result<AISState> {
    platform::recording::info!("Creating AIS state with config");

    // Get the Signer client configuration
    let signer_client_config = config
        .get_signer_client_config(global_config)
        .context("Failed to get Signer client config. Ensure Signer is enabled or ais.dependencies.signer is configured.")?;

    // Create the KS gRPC client
    let signer_client =
        create_signer_client(&signer_client_config, &global_config.actrix_shared_key)
            .await
            .context("Failed to create Signer gRPC client")?;

    platform::recording::info!("Signer gRPC client created successfully");

    // Create the Issuer configuration
    if config.server.renewal_token_secret.is_empty() {
        anyhow::bail!("services.ais.server.renewal_token_secret is required");
    }
    let renewal_token_secret = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        &config.server.renewal_token_secret,
    )
    .context("Failed to decode services.ais.server.renewal_token_secret from base64")?;
    if renewal_token_secret.len() < 32 {
        anyhow::bail!(
            "services.ais.server.renewal_token_secret must decode to at least 32 bytes, got {} byte(s)",
            renewal_token_secret.len()
        );
    }

    let issuer_config = IssuerConfig {
        token_ttl_secs: config.server.token_ttl_secs,
        signaling_heartbeat_interval_secs: config.server.signaling_heartbeat_interval_secs,
        key_refresh_interval_secs: 3600, // 1 hour
        key_storage_file: global_config.sqlite_path.join("ais_keys.db"),
        enable_periodic_rotation: false, // disabled by default, can be enabled via config
        key_rotation_interval_secs: 86400, // 24 hours
        turn_secret: global_config.turn.turn_secret.clone(),
        sqlite_path: global_config.sqlite_path.clone(),
        renewal_token_ttl_secs: config.server.renewal_token_ttl_secs,
        renewal_rotation_window_secs: config.server.renewal_rotation_window_secs,
        renewal_token_secret,
    };

    // Create the AId Token issuer
    let issuer = AIdIssuer::new(signer_client, issuer_config, cancel)
        .await
        .context("Failed to create AIS issuer")?;

    let state = if let Some(ctr) = counters {
        AISState::new(issuer).with_counters(ctr)
    } else {
        AISState::new(issuer)
    };

    platform::recording::info!("AIS state created successfully");
    Ok(state)
}

/// Build the public `/ais` router from an existing [`AISState`].
pub fn create_router_from_state(state: AISState) -> Router {
    create_router(state)
}

#[cfg(test)]
mod tests {

    // Note: Full integration tests require a running KS service.
    // Only basic unit tests here.
    // Actual testing is performed through HTTP endpoints after the main program starts.
}
