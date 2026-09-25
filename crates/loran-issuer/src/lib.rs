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
//! Client ── POST /issuer/register (protobuf) ──▶ Handlers ──▶ AIdIssuer
//!                                                               │
//!                              Snowflake serial numbers ◀───────┤
//!                                                               ▼
//!                                             Signer (issuer:<realm> keys)
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
//! ## Signing Keys
//!
//! Credentials are signed by the Signer with the Realm's current issuer key
//! (the first is `issuer:<realm>`, created on first use). The keys rotate as
//! `realm-profile.md` section 6.1 describes and are retired with the Realm.
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
//!
//! # Configuration Options
//!
//! See [`platform::config::IssuerServiceConfig`] for full configuration details.
#![deny(clippy::disallowed_macros)]

pub mod handlers;
mod idempotency;
pub mod issuer;
pub mod ratelimit;
pub mod realm_management;
pub mod realm_profile;
pub mod renewal;
pub mod signer_client_wrapper;
mod sn;

pub use issuer::{AIdIssuer, IssuerConfig};

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
/// plane shares `state.issuer`, the same issuer that backs the public
/// `/issuer` router built from this state.
pub async fn create_issuer_state(
    config: &IssuerServiceConfig,
    global_config: &platform::config::ActrixConfig,
    cancel: tokio_util::sync::CancellationToken,
    counters: Option<Arc<ServiceCounters>>,
) -> Result<IssuerState> {
    platform::recording::info!("Creating Issuer state with config");
    remove_retired_key_store(&global_config.sqlite_path);

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

    let issuer_config = IssuerConfig {
        worker_id: config.server.worker_id,
        token_ttl_secs: config.server.token_ttl_secs,
        signaling_heartbeat_interval_secs: config.server.signaling_heartbeat_interval_secs,
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

/// Delete `issuer_keys.db` (earlier `ais_keys.db`), the store of the removed
/// global Issuer key, if a data directory still has it.
fn remove_retired_key_store(data_dir: &std::path::Path) {
    for file in ["issuer_keys.db", "ais_keys.db"]
        .into_iter()
        .flat_map(|name| ["", "-wal", "-shm"].map(|suffix| format!("{name}{suffix}")))
    {
        let path = data_dir.join(file);
        match std::fs::remove_file(&path) {
            Ok(()) => platform::recording::info!(
                "Removed {}, left by the retired global Issuer key",
                path.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                platform::recording::warn!("Failed to remove {}: {}", path.display(), error)
            }
        }
    }
}

/// Build the public `/issuer` router from an existing [`IssuerState`].
pub fn create_router_from_state(state: IssuerState) -> Router {
    create_router(state)
}

#[cfg(test)]
mod tests {
    #[test]
    fn removes_the_retired_key_store() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "issuer_keys.db",
            "issuer_keys.db-wal",
            "ais_keys.db",
            "actrix.db",
        ] {
            std::fs::write(dir.path().join(name), b"x").unwrap();
        }
        super::remove_retired_key_store(dir.path());
        assert!(!dir.path().join("issuer_keys.db").exists());
        assert!(!dir.path().join("issuer_keys.db-wal").exists());
        assert!(!dir.path().join("ais_keys.db").exists());
        assert!(dir.path().join("actrix.db").exists());
        // Nothing left to remove is fine.
        super::remove_retired_key_store(dir.path());
    }
}
