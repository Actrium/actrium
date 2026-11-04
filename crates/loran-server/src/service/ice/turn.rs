//! TURN service implementation

use crate::service::IceService;
use anyhow::Result;
use async_trait::async_trait;
use platform::config::LoranConfig;
use platform::monitoring::ServiceCounters;
use platform::status::services::ServiceState;
use platform::{ServiceInfo, ServiceType};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::net::UdpSocket;
use url::Url;

/// TURN service implementation
#[derive(Debug, Clone)]
pub struct TurnService {
    info: ServiceInfo,
    config: LoranConfig,
    socket: Option<Arc<UdpSocket>>,
    /// Service-level counters for metrics collection.
    pub counters: Option<Arc<ServiceCounters>>,
}

impl TurnService {
    pub fn new(config: LoranConfig) -> Self {
        Self {
            info: ServiceInfo::new(
                "TURN Server",
                ServiceType::Turn,
                Some("TURN server with built-in STUN support for WebRTC connectivity".to_string()),
                &config,
            ),
            config,
            socket: None,
            counters: None,
        }
    }

    /// Attach service-level counters.
    pub fn with_counters(mut self, counters: Arc<ServiceCounters>) -> Self {
        self.counters = Some(counters);
        self
    }
}

#[async_trait]
impl IceService for TurnService {
    fn info(&self) -> &ServiceInfo {
        &self.info
    }

    fn info_mut(&mut self) -> &mut ServiceInfo {
        &mut self.info
    }

    async fn start(
        &mut self,
        mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
        oneshot_tx: tokio::sync::oneshot::Sender<ServiceInfo>,
    ) -> Result<()> {
        let ice_bind = &self.config.bind.ice;
        let ip = ice_bind.ip.parse::<IpAddr>().map_err(|e| {
            anyhow::anyhow!(
                "Invalid bind.ice.ip '{}': {} (expected IPv4/IPv6 literal)",
                ice_bind.ip,
                e
            )
        })?;
        let addr = SocketAddr::new(ip, ice_bind.port);

        platform::recording::info!("Starting TURN service on {}", addr);

        // Bind UDP socket
        let socket = match UdpSocket::bind(addr).await {
            Ok(socket) => {
                platform::recording::info!("TURN service listening on: {}", addr);
                Arc::new(socket)
            }
            Err(e) => {
                let error_msg = format!("Failed to bind TURN service to {addr}: {e}");
                self.info.set_error(&error_msg);
                return Err(anyhow::anyhow!(error_msg));
            }
        };

        self.socket = Some(socket.clone());

        // Realm-scoped credentials need revocation state; static and external
        // REST credentials have no Actor binding to watch or revoke.
        let realm = self.config.turn.realm.clone();
        let inactive_realms = self
            .config
            .uses_realm_turn_auth()
            .then(turn::InactiveRealms::default);
        let _realm_watch = match &inactive_realms {
            Some(inactive) => Some(inactive.watch().await),
            None => None,
        };
        let auth_handler: Arc<dyn turn_crate::auth::AuthHandler + Send + Sync> =
            if !self.config.turn.users.is_empty() {
                Arc::new(turn::StaticAuthenticator::new(
                    self.config.turn.users.clone(),
                )?)
            } else if let Some(inactive) = &inactive_realms {
                Arc::new(
                    turn::Authenticator::new(
                        self.config.turn.turn_secret.clone(),
                        self.config
                            .authority
                            .provider_id()
                            .map_err(anyhow::Error::msg)?,
                    )
                    .map_err(|e| anyhow::anyhow!("Failed to create TURN authenticator: {e}"))?
                    .with_inactive_realms(inactive.clone()),
                )
            } else {
                Arc::new(
                    turn::RestAuthenticator::new(self.config.turn.turn_secret.clone())
                        .map_err(|e| anyhow::anyhow!("Failed to create TURN authenticator: {e}"))?,
                )
            };
        let relay_port_range = self
            .config
            .turn
            .relay_port_bounds()
            .map_err(anyhow::Error::msg)?;
        #[cfg(test)]
        let allow_private_relay_peers = std::env::var("ACTR_PACKAGE_RUNTIME_FIXTURE").as_deref()
            == Ok("1")
            && std::env::var_os("ACTR_REALM_E2E_DOCUMENTS").is_some();
        #[cfg(not(test))]
        let allow_private_relay_peers = false;
        let turn_server = match turn::create_turn_server(
            socket.clone(),
            &self.config.bind.ice.advertised_ip,
            &realm,
            relay_port_range,
            auth_handler,
            allow_private_relay_peers,
        )
        .await
        {
            Ok(server) => {
                let url = Url::parse(&format!(
                    "turn:{}:{}?transport=udp",
                    ice_bind.advertised_ip, ice_bind.advertised_port
                ))?;
                self.info.set_running(url);
                // Attach counters to ServiceInfo and record TURN allocation
                if let Some(ref ctr) = self.counters {
                    self.info.set_counters(ctr.clone());
                    ctr.inc_conns();
                    ctr.record_request(true, 0.0).await;
                }
                oneshot_tx
                    .send(self.info.clone())
                    .map_err(|e| anyhow::anyhow!("Failed to send TURN service info: {e:?}"))?;
                platform::recording::info!("TURN service started successfully");
                server
            }
            Err(e) => {
                let error_msg = format!("Failed to start TURN service: {e}");
                self.info.set_error(&error_msg);
                return Err(anyhow::anyhow!(error_msg));
            }
        };

        // A Realm that stops being served, or a disabled Actor, loses its
        // allocations too, not only their refreshes.
        let turn_server = Arc::new(turn_server);
        let closer = inactive_realms.map(|inactive_realms| {
            let server = turn_server.clone();
            tokio::spawn(async move {
                loop {
                    inactive_realms.deactivated().await;
                    match turn::close_unauthorized_allocations(&server, &inactive_realms).await {
                        Ok(0) => {}
                        Ok(closed) => platform::recording::info!(
                            "Closed the TURN allocations of {} credentials of inactive Realms or Actors",
                            closed
                        ),
                        Err(e) => platform::recording::warn!(
                            "Failed to close TURN allocations of inactive Realms or Actors: {}",
                            e
                        ),
                    }
                }
            })
        });

        // Wait for shutdown signal
        let _ = shutdown_rx.recv().await;
        platform::recording::info!("TURN service received shutdown signal");
        if let Some(closer) = closer {
            closer.abort();
        }

        // Shut down TURN server
        if let Err(e) = turn::shutdown_turn_server(&turn_server).await {
            platform::recording::error!("Error shutting down TURN server: {}", e);
        }

        self.stop().await?;
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        platform::recording::info!("Stopping TURN service");

        // Decrement active connection on stop
        if let Some(ref ctr) = self.counters {
            ctr.dec_conns();
        }

        self.socket = None;
        self.info.status = ServiceState::Unknown;

        platform::recording::info!("TURN service stopped");
        Ok(())
    }
}
