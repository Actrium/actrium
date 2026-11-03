//! Creates independent relay and WebRTC candidates without pre-connecting.
//! WirePool races both and retains successful paths for fallback.

use super::peer_transport::WireBuilder;
use super::wire_handle::WireHandle;
use super::{Dest, NetworkError, NetworkResult};
use crate::wire::relay::RelayClient;
use async_trait::async_trait;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub struct DefaultWireBuilderConfig {
    /// Enables the outbound WebSocket relay candidate. No peer listener or
    /// advertised application endpoint is required.
    pub enable_websocket: bool,
}

impl Default for DefaultWireBuilderConfig {
    fn default() -> Self {
        Self {
            enable_websocket: true,
        }
    }
}

pub struct DefaultWireBuilder {
    rtc_manager: Option<Arc<crate::wire::webrtc::rtc::manager::RtcConnectionManager>>,
    relay_client: Option<Arc<RelayClient>>,
    config: DefaultWireBuilderConfig,
}

impl DefaultWireBuilder {
    pub fn new(config: DefaultWireBuilderConfig) -> Self {
        Self {
            rtc_manager: None,
            relay_client: None,
            config,
        }
    }

    pub(crate) fn with_rtc_manager(
        mut self,
        manager: Arc<crate::wire::webrtc::rtc::manager::RtcConnectionManager>,
    ) -> Self {
        self.rtc_manager = Some(manager);
        self
    }

    pub(crate) fn with_relay(mut self, client: Arc<RelayClient>) -> Self {
        self.relay_client = Some(client);
        self
    }
}

#[async_trait]
impl WireBuilder for DefaultWireBuilder {
    async fn create_connections(&self, dest: &Dest) -> NetworkResult<Vec<Arc<dyn WireHandle>>> {
        self.create_connections_with_cancel(dest, None).await
    }

    async fn existing_connections(&self, dest: &Dest) -> NetworkResult<Vec<Arc<dyn WireHandle>>> {
        let mut wires = Vec::new();
        if let (Some(client), Some(peer)) = (&self.relay_client, dest.as_peer_id()) {
            if let Some(wire) = client.existing_wire(peer).await {
                wires.push(wire);
            }
        }
        // RTC controllers own the physical inbound/outbound PeerConnection
        // independently of PeerTransport. ExistingOnly response traffic may
        // materialize a local DestTransport around that already-sendable
        // Session; it must not dial or negotiate a new connection.
        if let Some(manager) = self.rtc_manager.as_ref()
            && let Some(peer_id) = dest.as_peer_id()
            && let Some(controller) = manager.get_controller(peer_id).await
            && controller.snapshot().sendable
        {
            let handle = crate::wire::webrtc::rtc::wire_handle::RtcWireHandle::from_existing(
                manager.clone(),
                peer_id.clone(),
                controller,
            );
            wires.push(Arc::new(handle) as Arc<dyn WireHandle>);
        }
        Ok(wires)
    }

    async fn create_connections_with_cancel(
        &self,
        dest: &Dest,
        cancel: Option<CancellationToken>,
    ) -> NetworkResult<Vec<Arc<dyn WireHandle>>> {
        if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
            return Err(NetworkError::ConnectionClosed(
                "Connection creation cancelled".into(),
            ));
        }
        let mut connections: Vec<Arc<dyn WireHandle>> = Vec::new();
        if let Some(peer) = dest.as_peer_id() {
            if self.config.enable_websocket {
                if let Some(client) = &self.relay_client {
                    connections.push(client.wire(peer.clone()));
                }
            }
            if let Some(manager) = &self.rtc_manager {
                connections.push(Arc::new(
                    crate::wire::webrtc::rtc::wire_handle::RtcWireHandle::new(
                        manager.clone(),
                        peer.clone(),
                    ),
                ));
            }
        }
        Ok(connections)
    }
}

#[cfg(test)]
#[path = "wire_builder_tests.rs"]
mod tests;
