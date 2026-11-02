//! TURN server implementation
//!
//! Provides TURN relay server functionality for NAT traversal and network relay.
#![deny(clippy::disallowed_macros)]

// TURN server implementation modules
mod authenticator;
pub mod error;
mod inactive_realms;
mod relay_filter;

// Re-export types for convenience
pub use authenticator::Authenticator;
pub use error::{ErrorSeverity, TurnError};
pub use inactive_realms::{InactiveRealms, WatchGuard};
pub use relay_filter::is_forbidden_relay_peer;

use std::net::IpAddr;
use std::str::FromStr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use turn_crate::auth::AuthHandler;
use turn_crate::relay::relay_range::*;
use turn_crate::server::config::*;
use turn_crate::server::*;
use webrtc_util::vnet::net::*;

// Create and initialize the TURN server
pub async fn create_turn_server(
    socket: Arc<UdpSocket>,
    advertised_ip: &str,
    realm: &str,
    relay_port_range: (u16, u16),
    auth_handler: Arc<dyn AuthHandler + Send + Sync>,
    // When false (production default), the relay drops datagrams to loopback,
    // private, link-local and cloud-metadata peers to stop the relay being used
    // as an SSRF primitive. Set true only for loopback tests/fixtures that
    // legitimately relay between local addresses.
    allow_private_relay_peers: bool,
) -> error::Result<Server> {
    create_turn_server_inner(
        socket,
        advertised_ip,
        realm,
        relay_port_range,
        auth_handler,
        allow_private_relay_peers,
        None,
    )
    .await
}

/// Create the production server with observations of actual allocation closes.
/// The receiver must be drained by the test; notifications never initiate cleanup.
#[cfg(feature = "test-utils")]
pub async fn create_turn_server_with_close_notify(
    socket: Arc<UdpSocket>,
    advertised_ip: &str,
    realm: &str,
    relay_port_range: (u16, u16),
    auth_handler: Arc<dyn AuthHandler + Send + Sync>,
    allow_private_relay_peers: bool,
    alloc_close_notify: tokio::sync::mpsc::Sender<turn_crate::allocation::AllocationInfo>,
) -> error::Result<Server> {
    create_turn_server_inner(
        socket,
        advertised_ip,
        realm,
        relay_port_range,
        auth_handler,
        allow_private_relay_peers,
        Some(alloc_close_notify),
    )
    .await
}

async fn create_turn_server_inner(
    socket: Arc<UdpSocket>,
    advertised_ip: &str,
    realm: &str,
    relay_port_range: (u16, u16),
    auth_handler: Arc<dyn AuthHandler + Send + Sync>,
    allow_private_relay_peers: bool,
    alloc_close_notify: Option<tokio::sync::mpsc::Sender<turn_crate::allocation::AllocationInfo>>,
) -> error::Result<Server> {
    platform::recording::info!("Creating TURN server with advertised IP: {}", advertised_ip);

    // Get the local address of the socket
    let local_addr = match socket.local_addr() {
        Ok(addr) => addr.ip().to_string(),
        Err(e) => {
            platform::recording::error!("Failed to get local address from socket: {}", e);
            // Fall back to 0.0.0.0 if we can't get the actual address
            "0.0.0.0".to_string()
        }
    };

    platform::recording::info!(
        "TURN server will use local address: {} and advertised IP: {}",
        local_addr,
        advertised_ip
    );

    // Parse advertised IP
    let relay_ip = match IpAddr::from_str(advertised_ip) {
        Ok(ip) => ip,
        Err(e) => {
            let err_msg = format!("Invalid advertised IP address: {e}");
            platform::recording::error!("{}", err_msg);
            return Err(TurnError::Configuration {
                field: "advertised_ip".to_string(),
                value: advertised_ip.to_string(),
            });
        }
    };

    let (min_port, max_port) = relay_port_range;
    platform::recording::info!(
        "TURN relay allocation range configured: {}-{}",
        min_port,
        max_port
    );

    let base_generator = RelayAddressGeneratorRanges {
        relay_address: relay_ip,
        min_port,
        max_port,
        max_retries: 10,
        address: local_addr,
        net: Arc::new(Net::new(None)),
    };
    // Wrap the range generator so every allocated relay conn drops datagrams to
    // forbidden peers (loopback / private / link-local / cloud metadata),
    // preventing the relay from becoming an SSRF primitive. Upstream `turn` has
    // no peer-filtering hook. Loopback tests/fixtures opt out explicitly.
    let relay_addr_generator: Box<dyn turn_crate::relay::RelayAddressGenerator + Send + Sync> =
        if allow_private_relay_peers {
            Box::new(base_generator)
        } else {
            Box::new(relay_filter::FilteringRelayGenerator {
                inner: base_generator,
            })
        };

    let server_config = ServerConfig {
        conn_configs: vec![ConnConfig {
            conn: socket,
            relay_addr_generator,
        }],
        realm: realm.to_string(),
        auth_handler,
        channel_bind_timeout: std::time::Duration::from_secs(600), // 10 minutes
        alloc_close_notify,
    };

    // Create the actual server instance
    let server = match Server::new(server_config).await {
        Ok(server) => server,
        Err(e) => {
            let err_msg = format!("Failed to create TURN server: {e}");
            platform::recording::error!("{}", err_msg);
            return Err(TurnError::ServerStartFailed { reason: err_msg });
        }
    };

    platform::recording::info!("TURN server created successfully (includes STUN functionality)");
    Ok(server)
}

/// The Realm and Actor (text form) of a scoped TURN username
/// (`<expires>:realm=<realm>:provider=…:actor=<actor_id>`)
fn username_scope(username: &str) -> Option<(&str, &str)> {
    let (_, scoped) = username.split_once(":realm=")?;
    let (realm, rest) = scoped.split_once(":provider=")?;
    let (_, actor) = rest.split_once(":actor=")?;
    Some((realm, actor))
}

/// Close the allocations whose credential belongs to a Realm and Actor
/// that `closed(realm, actor)` selects; returns how many usernames were
/// closed. Credentials are refused once a Realm is not served or an Actor
/// is disabled; this also ends the allocations they already have.
pub async fn close_allocations(
    server: &Server,
    closed: impl Fn(&str, &str) -> bool,
) -> error::Result<usize> {
    close_allocations_matching(server, |username| {
        username_scope(username).is_some_and(|(realm, actor)| closed(realm, actor))
    })
    .await
}

/// Close allocations whose issuing device is no longer authorized, including
/// after a recovery skips over the transient disabled-Actor state.
pub async fn close_unauthorized_allocations(
    server: &Server,
    state: &InactiveRealms,
) -> error::Result<usize> {
    close_allocations_matching(server, |username| {
        let Some((realm, actor)) = username_scope(username) else {
            return true;
        };
        let key = username
            .split_once(":client_key=")
            .and_then(|(_, rest)| rest.split_once(":actor="))
            .map(|(key, _)| key);
        state.contains(realm)
            || state.contains_actor(actor)
            || key.is_none_or(|key| !state.matches_binding(actor, key))
    })
    .await
}

async fn close_allocations_matching(
    server: &Server,
    closed: impl Fn(&str) -> bool,
) -> error::Result<usize> {
    let infos =
        server
            .get_allocations_info(None)
            .await
            .map_err(|e| TurnError::AllocationFailed {
                reason: format!("list allocations: {e}"),
            })?;
    let mut usernames: Vec<String> = infos
        .into_values()
        .map(|info| info.username)
        .filter(|username| closed(username))
        .collect();
    usernames.sort();
    usernames.dedup();
    for username in &usernames {
        server
            .delete_allocations_by_username(username.clone())
            .await
            .map_err(|e| TurnError::AllocationFailed {
                reason: format!("close allocations: {e}"),
            })?;
    }
    Ok(usernames.len())
}

// Shutdown the TURN server
pub async fn shutdown_turn_server(server: &Server) -> error::Result<()> {
    platform::recording::info!("Shutting down TURN server");

    if let Err(e) = server.close().await {
        platform::recording::error!("Error while closing TURN server: {e}");
        return Err(TurnError::ServerShutdownFailed {
            reason: format!("Failed to close TURN server: {e}"),
        });
    }

    platform::recording::info!("TURN server has been shut down");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use tokio::net::UdpSocket;
    use turn_crate::auth::AuthHandler;

    // Mock auth handler for testing
    struct MockAuthHandler;

    impl AuthHandler for MockAuthHandler {
        fn auth_handle(
            &self,
            _username: &str,
            _realm: &str,
            _src_addr: SocketAddr,
        ) -> Result<Vec<u8>, turn_crate::Error> {
            Ok(vec![0u8; 16]) // Return dummy hash
        }
    }

    #[tokio::test]
    async fn test_create_turn_server() -> anyhow::Result<()> {
        // Create a UDP socket for testing
        let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await?);
        let auth_handler: Arc<dyn AuthHandler + Send + Sync> = Arc::new(MockAuthHandler);

        // Test server creation
        let server = create_turn_server(
            socket,
            "127.0.0.1",
            "test.realm",
            (49_152, 65_535),
            auth_handler,
            true,
        )
        .await?;

        // Test server shutdown
        shutdown_turn_server(&server).await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_invalid_public_ip() {
        let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let auth_handler: Arc<dyn AuthHandler + Send + Sync> = Arc::new(MockAuthHandler);

        // Test with invalid IP
        let result = create_turn_server(
            socket,
            "invalid.ip.address",
            "test.realm",
            (49_152, 65_535),
            auth_handler,
            true,
        )
        .await;

        assert!(result.is_err());
    }
}
