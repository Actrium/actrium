//! Actor Identity Service (Issuer) - ActrId registration and credential issuance.
//!
//! # Overview
//!
//! Issuer is the core Actrix identity service. It allocates ActrId serial numbers,
//! issues signed AIdCredential tokens, generates time-limited TURN credentials,
//! and returns renewal tokens for `/issuer/renew`.
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────┐
//! │   Client     │
//! └──────┬───────┘
//!        │ POST /issuer/register (protobuf)
//!        ▼
//! ┌──────────────────────────────────────────┐
//! │  Issuer Service                             │
//! │  ┌────────────┐      ┌────────────────┐ │
//! │  │  Handlers  │─────▶│  AIdIssuer     │ │
//! │  └────────────┘      └────────┬───────┘ │
//! │                               │         │
//! │  ┌──────────────────┐  ┌─────▼──────┐  │
//! │  │ SN Generator     │  │ KeyStorage │  │
//! │  │ (Snowflake)      │  │ (SQLite)   │  │
//! │  └──────────────────┘  └────────────┘  │
//! └──────────────┬───────────────────────────┘
//!                │ Signer Client
//!                ▼
//!         ┌─────────────┐
//!         │ Signer Service  │ (key generation)
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
//! - **Fetch**: On startup, load the cached key from local SQLite; if expired or too close to expiry, fetch from Signer
//! - **Refresh**: Background task checks every 10 minutes; refreshes keys expiring within 10 minutes
//! - **Minting deadline**: Never mint with a key at or within 10 seconds of `expires_at`, because signaling drops the key at that instant
//! - **Tolerance**: Signer key-material retention after expiry is read-only and does not extend the Issuer minting window
//!
//! # Usage Example
//!
//! ```no_run
//! use issuer::create_issuer_router;
//! use platform::config::{IssuerServiceConfig, ActrixConfig};
//! use tokio_util::sync::CancellationToken;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Create configuration
//! let global_config = ActrixConfig::default();
//! let issuer_config = IssuerServiceConfig::default();
//! let cancel = CancellationToken::new();
//!
//! // Create Issuer router
//! let router = create_issuer_router(&issuer_config, &global_config, cancel).await?;
//!
//! // Integrate into the main HTTP service
//! // let app = Router::new().nest("/issuer", router);
//! # Ok(())
//! # }
//! ```
//!
//! # Security Considerations
//!
//! - **Renewal token rotation**: renewal tokens are stored as hashes and rotated on use.
//! - **Encrypted transport**: tokens are encrypted with ECIES; only services holding the private key can decrypt them
//! - **Serial number uniqueness**: the Snowflake algorithm guarantees global uniqueness in distributed environments
//! - **Key rotation**: old keys remain verifiable during the tolerance window.
//!
//! # Configuration Options
//!
//! See [`platform::config::IssuerServiceConfig`] for full configuration details.
#![deny(clippy::disallowed_macros)]

pub mod handlers;
// The attempt journal backs the /renew and /reissue attempt_id contract; the
// handlers adopt it in a follow-up change.
#[allow(dead_code)]
mod idempotency;
pub mod issuer;
pub mod ratelimit;
pub mod renewal;
pub mod signer_client_wrapper;
mod sn;
mod storage;

pub use issuer::{AIdIssuer, IssuerConfig, KeyCacheInfo};

use crate::handlers::{IssuerState, create_router};
use crate::signer_client_wrapper::create_signer_client;
use anyhow::{Context, Result};
use axum::Router;
use platform::config::IssuerServiceConfig;
use platform::monitoring::ServiceCounters;
use std::sync::Arc;

/// Create the Issuer router, following the project's HttpRouterService architecture
pub async fn create_issuer_router(
    config: &IssuerServiceConfig,
    global_config: &platform::config::ActrixConfig,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<Router> {
    create_issuer_router_with_counters(config, global_config, cancel, None).await
}

/// Create Issuer router with optional service counters for metrics collection.
pub async fn create_issuer_router_with_counters(
    config: &IssuerServiceConfig,
    global_config: &platform::config::ActrixConfig,
    cancel: tokio_util::sync::CancellationToken,
    counters: Option<Arc<ServiceCounters>>,
) -> Result<Router> {
    let state = create_issuer_state(config, global_config, cancel, counters).await?;
    Ok(create_router_from_state(state))
}

/// Create the Issuer service state (issuer + idempotency store).
///
/// The returned [`IssuerState`] owns the issuer inside an `Arc`; the control
/// plane can grab `state.issuer` to trigger authenticated key rotation on the
/// same issuer that backs the public `/issuer` router built from this state.
pub async fn create_issuer_state(
    config: &IssuerServiceConfig,
    global_config: &platform::config::ActrixConfig,
    cancel: tokio_util::sync::CancellationToken,
    counters: Option<Arc<ServiceCounters>>,
) -> Result<IssuerState> {
    platform::recording::info!("Creating Issuer state with config");

    // Get Signer client config
    let signer_client_config = config
        .get_signer_client_config(global_config)
        .context("Failed to get Signer client config. Ensure Signer is enabled or issuer.dependencies.signer is configured.")?;

    // Create Signer gRPC client
    let signer_client =
        create_signer_client(&signer_client_config, &global_config.actrix_shared_key)
            .await
            .context("Failed to create Signer gRPC client")?;

    platform::recording::info!("Signer gRPC client created successfully");

    // Create Issuer config
    if config.server.renewal_token_secret.is_empty() {
        anyhow::bail!("services.issuer.server.renewal_token_secret is required");
    }
    let renewal_token_secret = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        &config.server.renewal_token_secret,
    )
    .context("Failed to decode services.issuer.server.renewal_token_secret from base64")?;
    if renewal_token_secret.len() < 32 {
        anyhow::bail!(
            "services.issuer.server.renewal_token_secret must decode to at least 32 bytes, got {} byte(s)",
            renewal_token_secret.len()
        );
    }

    let key_storage_file = global_config.sqlite_path.join(ISSUER_KEY_STORAGE_FILE);
    migrate_legacy_key_storage(&global_config.sqlite_path, &key_storage_file)?;

    let issuer_config = IssuerConfig {
        worker_id: config.server.worker_id,
        token_ttl_secs: config.server.token_ttl_secs,
        signaling_heartbeat_interval_secs: config.server.signaling_heartbeat_interval_secs,
        key_refresh_interval_secs: 3600, // 1 hour
        key_storage_file,
        enable_periodic_rotation: false, // disabled by default; can be enabled via config
        key_rotation_interval_secs: 86400, // 24 hours
        turn_secret: global_config.turn.turn_secret.clone(),
        provider_id: global_config
            .authority
            .provider_id()
            .map_err(anyhow::Error::msg)?,
        sqlite_path: global_config.sqlite_path.clone(),
        renewal_token_ttl_secs: config.server.renewal_token_ttl_secs,
        renewal_rotation_window_secs: config.server.renewal_rotation_window_secs,
        renewal_token_secret,
    };

    // Create AId token issuer
    let issuer = AIdIssuer::new(signer_client, issuer_config, cancel)
        .await
        .context("Failed to create Issuer issuer")?;

    let state = if let Some(ctr) = counters {
        IssuerState::new(issuer).with_counters(ctr)
    } else {
        IssuerState::new(issuer)
    };

    platform::recording::info!("Issuer state created successfully");
    Ok(state)
}

/// File name of the issuer signing-key store under `sqlite_path`.
pub const ISSUER_KEY_STORAGE_FILE: &str = "issuer_keys.db";

/// File name used for the issuer signing-key store before the historical
/// service name was retired.
const LEGACY_KEY_STORAGE_FILE: &str = "ais_keys.db";

/// Moves a key store created under the retired file name to its current
/// name so existing signing keys survive the rename.
fn migrate_legacy_key_storage(
    sqlite_path: &std::path::Path,
    key_storage_file: &std::path::Path,
) -> anyhow::Result<()> {
    let legacy = sqlite_path.join(LEGACY_KEY_STORAGE_FILE);
    if legacy.exists() && !key_storage_file.exists() {
        std::fs::rename(&legacy, key_storage_file).with_context(|| {
            format!(
                "Failed to move legacy issuer key store {} to {}",
                legacy.display(),
                key_storage_file.display()
            )
        })?;
        platform::recording::info!(
            "Moved legacy issuer key store to {}",
            key_storage_file.display()
        );
    }
    Ok(())
}

/// Build the public `/issuer` router from an existing [`IssuerState`].
pub fn create_router_from_state(state: IssuerState) -> Router {
    create_router(state)
}

#[cfg(test)]
mod tests {

    // Note: full integration tests require the Signer service to be running
    // Only basic unit tests here
    // Actual testing is done via HTTP endpoints after the main program starts
}
