//! WebSocket-based test signaling server
//!
//! Thin wrapper around `actr_mock_actrix::MockActrixServer` with
//! backward-compatible API for existing integration tests.

pub use actr_mock_actrix::MockActrixServer;

/// Controllable test signaling server with real WebSocket.
///
/// This is a compatibility wrapper around [`MockActrixServer`] that
/// preserves the original `TestSignalingServer` API used by hyper integration
/// tests.
pub struct TestSignalingServer {
    inner: Option<MockActrixServer>,
    external_url: Option<String>,
}

impl TestSignalingServer {
    /// Start the test server on a random available port.
    pub async fn start() -> anyhow::Result<Self> {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        Ok(Self {
            inner: Some(MockActrixServer::start().await?),
            external_url: None,
        })
    }

    pub async fn start_with_relay() -> anyhow::Result<Self> {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        Ok(Self {
            inner: Some(MockActrixServer::start_with_relay().await?),
            external_url: None,
        })
    }

    pub fn relay_connection_count(&self) -> usize {
        self.controlled().relay_connection_count()
    }

    pub fn set_relay_discovery_enabled(&self, enabled: bool) {
        self.controlled().set_relay_discovery_enabled(enabled);
    }

    pub fn relay_discovery_requests(&self) -> u32 {
        self.controlled().relay_discovery_requests()
    }

    pub fn disconnect_relay_connections(&self) {
        self.controlled().disconnect_relay_connections();
    }

    pub fn pause_relay_enrollment(&self) {
        self.controlled().pause_relay_enrollment();
    }

    /// Start the test server on the specified port.
    pub async fn start_on_port(port: u16) -> anyhow::Result<Self> {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        Ok(Self {
            inner: Some(MockActrixServer::start_on_port(port).await?),
            external_url: None,
        })
    }

    /// Represent a signaling endpoint owned by another fixture (for example
    /// [`crate::test_support::ActrixTestServices`]). No mock server is started;
    /// protocol-specific controls are intentionally unavailable.
    pub fn external(url: impl Into<String>) -> Self {
        Self {
            inner: None,
            external_url: Some(url.into()),
        }
    }

    fn controlled(&self) -> &MockActrixServer {
        self.inner
            .as_ref()
            .expect("signaling fault control requires the compatibility mock backend")
    }

    pub fn url(&self) -> String {
        self.external_url
            .clone()
            .unwrap_or_else(|| self.controlled().url())
    }

    pub fn port(&self) -> u16 {
        self.inner.as_ref().map_or_else(
            || {
                url::Url::parse(self.external_url.as_deref().expect("external URL"))
                    .expect("valid external signaling URL")
                    .port_or_known_default()
                    .expect("external signaling URL has a port")
            },
            MockActrixServer::port,
        )
    }

    pub async fn shutdown(&mut self) {
        if let Some(inner) = &mut self.inner {
            inner.shutdown().await;
        }
    }

    pub fn pause_forwarding(&self) {
        self.controlled().pause_forwarding();
    }

    pub fn relay_frame_count(&self) -> u32 {
        self.controlled().relay_frame_count()
    }

    pub fn resume_forwarding(&self) {
        self.controlled().resume_forwarding();
    }

    pub fn drop_next_ice_restart_requests(&self, count: u32) {
        self.controlled().drop_next_ice_restart_requests(count);
    }

    pub fn blackhole_websocket_io(&self) {
        self.controlled().blackhole_websocket_io();
    }

    pub fn restore_websocket_io(&self) {
        self.controlled().restore_websocket_io();
    }

    pub fn drop_next_ice_candidates(&self, count: u32) {
        self.controlled().drop_next_ice_candidates(count);
    }

    pub fn drop_next_ice_candidates_for(&self, count: u32, duration: std::time::Duration) {
        self.controlled()
            .drop_next_ice_candidates_for(count, duration);
    }

    pub fn delay_ice_candidates_for(&self, duration: std::time::Duration) {
        self.controlled().delay_ice_candidates_for(duration);
    }

    pub fn delayed_ice_candidate_count(&self) -> u32 {
        self.controlled().delayed_ice_candidate_count()
    }

    pub fn drop_next_offers(&self, count: u32) {
        self.controlled().drop_next_offers(count);
    }

    pub fn message_count(&self) -> u32 {
        self.controlled().message_count()
    }

    pub async fn received_messages(&self) -> Vec<actr_protocol::SignalingEnvelope> {
        self.controlled().received_messages().await
    }

    pub fn get_ice_restart_count(&self) -> u32 {
        self.controlled().ice_restart_count()
    }

    pub fn get_ice_restart_request_count(&self) -> u32 {
        self.controlled().ice_restart_request_count()
    }

    pub fn get_connection_count(&self) -> u32 {
        self.controlled().connection_count()
    }

    pub fn get_disconnection_count(&self) -> u32 {
        self.controlled().disconnection_count()
    }

    pub fn reset_counters(&self) {
        self.controlled().reset_counters();
    }

    pub fn is_running(&self) -> bool {
        self.inner.as_ref().is_none_or(MockActrixServer::is_running)
    }
}
