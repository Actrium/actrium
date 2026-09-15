//! `PeerConnectionFactory` (RFC §"PeerConnectionFactory").
//!
//! Every `build()` creates a **fresh** `PeerConnectionBuilder`, configuration,
//! and socket set — a consumed builder is never reused (RFC: "Factory MUST
//! create a new Builder, MediaEngine, interceptor registry, EventHandler, and
//! socket on every build"). The factory owns codec/interceptor/config/socket/
//! runtime policy and returns an object-safe `Arc<dyn PeerConnection>`; it does
//! **not** do signaling, hooks, controller bookkeeping, or business receive
//! loops.
//!
//! Stage 6: the factory registers the default codec set (VP8/H264/OPUS via
//! `MediaEngine::register_default_codecs`) and the standard interceptor
//! registry on every build — webrtc 0.20 auto-registers neither, and media
//! tracks cannot negotiate without them.

#[cfg(feature = "test-utils")]
use std::net::SocketAddr;
use std::sync::Arc;

#[cfg(feature = "test-utils")]
use actr_protocol::ActrId;
use actr_protocol::TurnCredential;
use futures_util::FutureExt;
use webrtc::error::Error as RtcError;
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, RTCBundlePolicy, RTCConfiguration,
    RTCConfigurationBuilder, RTCIceServer, RTCRtcpMuxPolicy, SettingEngine, SettingEngineBuilder,
};

use super::address::{LocalAddressProvider, LocalAddressResolver};
use super::event_handler::WebRtcEventHandler;
use super::runtime::RtcRuntimeConfig;

/// Supplies the latest AIS-issued TURN credential when a fresh
/// `PeerConnection` configuration is materialized.
///
/// The URL/policy configuration is process-stable, while the HMAC TURN
/// credential rotates with the actor credential. Keeping the provider behind
/// this small interface prevents the Factory from freezing the bootstrap
/// credential into every later PeerConnection generation.
#[async_trait::async_trait]
pub(crate) trait TurnCredentialProvider: Send + Sync {
    async fn current_turn_credential(&self) -> Option<TurnCredential>;
}

#[async_trait::async_trait]
impl TurnCredentialProvider for crate::lifecycle::CredentialState {
    async fn current_turn_credential(&self) -> Option<TurnCredential> {
        self.turn_credential().await
    }
}

/// Builds independent v0.20 `PeerConnection`s sharing process-level policy.
pub(crate) struct PeerConnectionFactory {
    runtime_config: RtcRuntimeConfig,
    /// Concrete providers are retained for deterministic tests and platform
    /// adapters. Production desktop construction leaves this unset and uses
    /// dual-stack wildcard binds so webrtc-rs re-expands live interfaces on
    /// every initial bind and ICE-restart rebind.
    address_provider: Option<Arc<dyn LocalAddressProvider>>,
    ice_servers: Vec<RTCIceServer>,
    configuration: Option<RTCConfiguration>,
    turn_credential_provider: Option<Arc<dyn TurnCredentialProvider>>,
    setting_engine: SettingEngine,
    #[cfg(feature = "test-utils")]
    directed_faults: Option<(
        ActrId,
        crate::wire::webrtc::test_runtime::DirectedRtcFaultRegistry,
    )>,
}

impl PeerConnectionFactory {
    pub(crate) fn new(
        runtime_config: RtcRuntimeConfig,
        address_provider: Arc<dyn LocalAddressProvider>,
        ice_servers: Vec<RTCIceServer>,
    ) -> Self {
        let setting_engine = default_setting_engine(&runtime_config);
        Self {
            runtime_config,
            address_provider: Some(address_provider),
            ice_servers,
            configuration: None,
            turn_credential_provider: None,
            setting_engine,
            #[cfg(feature = "test-utils")]
            directed_faults: None,
        }
    }

    /// Build sessions from a complete v0.20 configuration. This preserves
    /// policies such as relay-only ICE that cannot be represented by an ICE
    /// server list alone.
    pub(crate) fn new_with_configuration(
        runtime_config: RtcRuntimeConfig,
        address_provider: Arc<dyn LocalAddressProvider>,
        configuration: RTCConfiguration,
    ) -> Self {
        let setting_engine = default_setting_engine(&runtime_config);
        Self {
            runtime_config,
            address_provider: Some(address_provider),
            ice_servers: Vec::new(),
            configuration: Some(configuration),
            turn_credential_provider: None,
            setting_engine,
            #[cfg(feature = "test-utils")]
            directed_faults: None,
        }
    }

    /// Build sessions with v0.20's dual-stack wildcard bind plan.
    ///
    /// The webrtc-rs driver expands wildcard addresses against the live
    /// interface set at the point of each bind/rebind. This is the production
    /// desktop path: no cached local-address provider or polling monitor is
    /// needed to make an ICE restart see a newly selected interface.
    pub(crate) fn new_with_configuration_and_wildcard_bind(
        runtime_config: RtcRuntimeConfig,
        configuration: RTCConfiguration,
    ) -> Self {
        let setting_engine = default_setting_engine(&runtime_config);
        Self {
            runtime_config,
            address_provider: None,
            ice_servers: Vec::new(),
            configuration: Some(configuration),
            turn_credential_provider: None,
            setting_engine,
            #[cfg(feature = "test-utils")]
            directed_faults: None,
        }
    }

    /// Install the test-only generation-scoped Runtime wrapper. The scope is
    /// completed at build time from the handler's remote peer and generation.
    #[cfg(feature = "test-utils")]
    pub(crate) fn with_directed_faults(
        mut self,
        local_id: ActrId,
        registry: crate::wire::webrtc::test_runtime::DirectedRtcFaultRegistry,
    ) -> Self {
        self.directed_faults = Some((local_id, registry));
        self
    }

    /// Read AIS-issued TURN credentials at build time instead of freezing the
    /// credential that happened to exist when the node started.
    pub(crate) fn with_turn_credential_provider(
        mut self,
        provider: Arc<dyn TurnCredentialProvider>,
    ) -> Self {
        self.turn_credential_provider = Some(provider);
        self
    }

    /// Apply the v0.20-native ICE/transport settings for every fresh
    /// generation. The engine is cloned per build; a consumed Builder is never
    /// reused.
    pub(crate) fn with_setting_engine(mut self, setting_engine: SettingEngine) -> Self {
        self.setting_engine = setting_engine;
        self
    }

    /// Materialize the complete configuration for a new generation.
    ///
    /// TURN URLs from `actr.toml` intentionally carry no static password: AIS
    /// returns a time-limited HMAC credential during registration/renewal. The
    /// v0.20 builder validates TURN credentials synchronously, so an empty
    /// username/password must be rejected here with the native typed error
    /// instead of becoming a generic, silent `Idle` transition.
    pub(crate) async fn current_configuration(&self) -> Result<RTCConfiguration, RtcError> {
        let base = self.configuration.clone().unwrap_or_else(|| {
            RTCConfigurationBuilder::default()
                .with_ice_servers(self.ice_servers.clone())
                // Pin every v0.20 configuration knob that affects transport
                // shape instead of relying on crate defaults drifting later.
                .with_bundle_policy(RTCBundlePolicy::Balanced)
                .with_rtcp_mux_policy(RTCRtcpMuxPolicy::Require)
                .with_ice_candidate_pool_size(0)
                .build()
        });

        let has_turn = base
            .ice_servers()
            .iter()
            .any(|server| server.urls.iter().any(|url| is_turn_url(url)));
        if !has_turn {
            return Ok(base);
        }

        let dynamic_credential = match &self.turn_credential_provider {
            Some(provider) => provider.current_turn_credential().await,
            None => None,
        };
        let mut ice_servers = base.ice_servers().to_vec();
        for server in &mut ice_servers {
            if !server.urls.iter().any(|url| is_turn_url(url)) {
                continue;
            }
            if let Some(credential) = &dynamic_credential {
                server.username.clone_from(&credential.username);
                server.credential.clone_from(&credential.password);
            }
            if server.username.is_empty() || server.credential.is_empty() {
                return Err(RtcError::ErrNoTurnCredentials);
            }
        }

        Ok(RTCConfigurationBuilder::default()
            .with_ice_servers(ice_servers)
            .with_ice_transport_policy(base.ice_transport_policy())
            .with_bundle_policy(base.bundle_policy())
            .with_rtcp_mux_policy(base.rtcp_mux_policy())
            .with_peer_identitys(base.peer_identity().to_owned())
            .with_certificates(base.certificates().to_vec())
            .with_ice_candidate_pool_size(base.ice_candidate_pool_size())
            .build())
    }

    /// Build a fresh v0.20 `PeerConnection` with `handler` pre-injected.
    ///
    /// Test/platform providers may request one UDP socket per concrete address
    /// (ephemeral port). Production uses dual-stack wildcard inputs and lets
    /// webrtc-rs expand them against the live interfaces at bind time. An
    /// unsatisfiable concrete bind still surfaces here as an error (fail-fast),
    /// not a silent fallback.
    pub(crate) async fn build(
        &self,
        handler: Arc<WebRtcEventHandler>,
    ) -> Result<Arc<dyn PeerConnection>, RtcError> {
        self.runtime_config.validate().map_err(|error| {
            RtcError::Other(format!("invalid RTC resource configuration: {error}"))
        })?;
        let bind_ips = match &self.address_provider {
            Some(provider) => provider
                .current_addresses_sync()
                .map_err(|e| RtcError::Other(format!("local address provider failed: {e}")))?,
            None => vec![
                std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
                std::net::IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED),
            ],
        };
        if bind_ips.is_empty() {
            return Err(RtcError::Other(
                "local address provider returned no bindable addresses".to_string(),
            ));
        }
        // Production supplies dual-stack wildcards, not a cached interface
        // list. Upstream expands them on every bind/rebind. Explicit test
        // providers still pin concrete addresses. The runtime observation
        // (`bound_socket_addresses`) is the authoritative evidence of what
        // actually got bound.
        #[cfg(feature = "test-utils")]
        let udp_addrs: Vec<SocketAddr> = bind_ips
            .into_iter()
            .map(|ip| SocketAddr::new(ip, 0))
            .collect();
        let address_resolver = match &self.address_provider {
            Some(provider) => LocalAddressResolver::new(provider.clone()),
            None => LocalAddressResolver::wildcard(),
        };

        let configuration = self.current_configuration().await?;

        // S6-7b: register the default codec set and interceptor registry.
        // webrtc 0.20 does NOT auto-register either (the builder defaults to
        // an empty MediaEngine + NoopInterceptor), so media tracks cannot
        // negotiate codecs and RTCP feedback (NACK/PLI/reports) is inert
        // without this. Mirrors the canonical rtc 0.20 setup.
        let mut media_engine = webrtc::peer_connection::MediaEngine::default();
        media_engine
            .register_default_codecs()
            .map_err(|e| RtcError::Other(format!("default codec registration failed: {e}")))?;
        let interceptor_registry = webrtc::peer_connection::register_default_interceptors(
            webrtc::peer_connection::Registry::new(),
            &mut media_engine,
        )
        .map_err(|e| RtcError::Other(format!("default interceptor registration failed: {e}")))?;

        let runtime = self.runtime_config.runtime();
        #[cfg(feature = "test-utils")]
        let runtime = match &self.directed_faults {
            Some((local_id, registry)) => registry.scoped_runtime(
                runtime,
                local_id.clone(),
                handler.peer_id().clone(),
                handler.generation(),
                udp_addrs.clone(),
            ),
            None => runtime,
        };
        // Enable upstream webrtc-rs/webrtc#868 ICE-restart rebind for every
        // fresh v0.20 generation: after a mobile network handover, ICE restart
        // discards stale local candidates and re-binds fresh sockets instead of
        // wedging on the dead transport. 0.20.3 keeps this opt-in (off by
        // default); v0.20 enables it uniformly here so production and test
        // harnesses share one behavior. The driver invokes the resolver again
        // on every restart rebind; production wildcard expansion therefore
        // happens at the actual bind/rebind point, while concrete test
        // providers are queried afresh.
        let setting_engine = self.setting_engine.clone();
        let builder = PeerConnectionBuilder::<LocalAddressResolver>::new()
            .with_configuration(configuration)
            .with_runtime(runtime)
            .with_handler(handler)
            .with_udp_addrs(vec![address_resolver])
            .with_setting_engine(setting_engine)
            .with_data_channel_send_buffer_limit(
                self.runtime_config.data_channel_send_buffer_limit(),
            )
            .with_media_engine(media_engine)
            .with_interceptor_registry(interceptor_registry);
        let mut builder = builder;
        if self.runtime_config.dedicated_reactor() {
            builder =
                builder.with_dedicated_reactor_pool_size(self.runtime_config.reactor_pool_size());
        }

        // The builder's async state includes the full ICE/DTLS/SCTP setup.
        // Erase it at this boundary as well so callers that add request-level
        // deadline state do not inline another large future into their stack.
        let pc = builder.build().boxed().await?;
        Ok(Arc::new(pc))
    }
}

fn default_setting_engine(runtime_config: &RtcRuntimeConfig) -> SettingEngine {
    SettingEngineBuilder::new()
        .with_discard_local_candidates_during_ice_restart(true)
        .with_sctp_max_receive_buffer_size(runtime_config.sctp_receive_buffer_size())
        .build()
}

fn is_turn_url(url: &str) -> bool {
    url.split_once(':').is_some_and(|(scheme, _)| {
        scheme.eq_ignore_ascii_case("turn") || scheme.eq_ignore_ascii_case("turns")
    })
}

#[async_trait::async_trait]
impl super::peer::effects::SessionFactory for PeerConnectionFactory {
    /// The production `SessionFactory` (used from Stage 3): builds the real
    /// connection and wraps it in `RealSessionOps`.
    async fn build(
        &self,
        handler: Arc<WebRtcEventHandler>,
    ) -> Result<Arc<dyn super::peer::effects::SessionOps>, RtcError> {
        let pc = PeerConnectionFactory::build(self, handler).await?;
        Ok(Arc::new(super::peer::effects::RealSessionOps::new(pc)))
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};
    use std::sync::Arc;
    use std::time::Duration;

    use actr_protocol::{AIdCredential, TurnCredential};
    use tokio::time::timeout;
    use webrtc::data_channel::RTCDataChannelInit;
    use webrtc::peer_connection::{
        RTCConfigurationBuilder, RTCIceGatheringState, RTCIceServer, RTCIceTransportPolicy,
    };

    use super::*;
    use crate::lifecycle::CredentialState;
    use crate::wire::webrtc::v020::address::DesktopLocalAddressProvider;
    use crate::wire::webrtc::v020::event_handler::WebRtcEventHandler;
    use crate::wire::webrtc::v020::events::peer_event_mailbox;

    fn test_actor_id(serial_number: u64) -> actr_protocol::ActrId {
        actr_protocol::ActrId {
            realm: actr_protocol::Realm { realm_id: 1 },
            serial_number,
            r#type: actr_protocol::ActrType {
                manufacturer: "acme".to_string(),
                name: "node".to_string(),
                version: "1.0.0".to_string(),
            },
        }
    }

    async fn wait_for_gathering_complete(
        events: &crate::wire::webrtc::v020::events::PeerEventMailboxReceiver,
        previous_seq: u64,
    ) {
        timeout(Duration::from_secs(10), async {
            loop {
                let tick = events.latest_state();
                if tick.seq > previous_seq
                    && tick.gathering_state == Some(RTCIceGatheringState::Complete)
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("ICE gathering did not complete");
    }

    fn relay_configuration() -> RTCConfiguration {
        RTCConfigurationBuilder::default()
            .with_ice_servers(vec![RTCIceServer {
                urls: vec!["turn:127.0.0.1:3478".to_string()],
                username: String::new(),
                credential: String::new(),
            }])
            .with_ice_transport_policy(RTCIceTransportPolicy::Relay)
            .build()
    }

    fn turn_credential(username: &str, password: &str) -> TurnCredential {
        TurnCredential {
            username: username.to_string(),
            password: password.to_string(),
            expires_at: 1,
        }
    }

    fn factory(configuration: RTCConfiguration) -> PeerConnectionFactory {
        PeerConnectionFactory::new_with_configuration(
            RtcRuntimeConfig::desktop_default(),
            Arc::new(DesktopLocalAddressProvider::with_override(vec![
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            ])),
            configuration,
        )
    }

    #[tokio::test]
    async fn turn_configuration_without_credentials_returns_typed_error() {
        let error = factory(relay_configuration())
            .current_configuration()
            .await
            .expect_err("TURN without credentials must fail before PeerConnection build");
        assert_eq!(error, RtcError::ErrNoTurnCredentials);
    }

    #[tokio::test]
    async fn current_ais_turn_credential_is_injected_for_each_build() {
        let state = CredentialState::new(
            AIdCredential::default(),
            None,
            Some(turn_credential("first-user", "first-password")),
        );
        let factory =
            factory(relay_configuration()).with_turn_credential_provider(Arc::new(state.clone()));

        let initial = factory.current_configuration().await.unwrap();
        assert_eq!(initial.ice_servers()[0].username, "first-user");
        assert_eq!(initial.ice_servers()[0].credential, "first-password");

        state
            .update_for_test(
                AIdCredential::default(),
                None,
                Some(turn_credential("renewed-user", "renewed-password")),
            )
            .await;

        let renewed = factory.current_configuration().await.unwrap();
        assert_eq!(renewed.ice_servers()[0].username, "renewed-user");
        assert_eq!(renewed.ice_servers()[0].credential, "renewed-password");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn wildcard_factory_builds_and_rebinds_on_ice_restart() {
        let factory = PeerConnectionFactory::new_with_configuration_and_wildcard_bind(
            RtcRuntimeConfig::desktop_default(),
            RTCConfigurationBuilder::default().build(),
        );
        let (mailbox, events) = peer_event_mailbox();
        let handler = Arc::new(WebRtcEventHandler::new(test_actor_id(3), 1, mailbox));
        let pc = factory.build(handler).await.expect("build wildcard PC");
        pc.create_data_channel("wildcard", Some(RTCDataChannelInit::default()))
            .await
            .expect("create channel to trigger host gathering");

        let first_offer = pc.create_offer(None).await.expect("create first offer");
        pc.set_local_description(first_offer)
            .await
            .expect("set first wildcard offer");
        wait_for_gathering_complete(&events, 0).await;
        let first_sdp = pc
            .local_description()
            .await
            .expect("first local description")
            .sdp;
        assert!(
            first_sdp.contains("a=candidate:"),
            "wildcard bind must gather host candidates"
        );

        let restart_state_seq = events.latest_state().seq;
        pc.restart_ice().await.expect("arm ICE restart");
        let restart_offer = pc.create_offer(None).await.expect("create restart offer");
        pc.set_local_description(restart_offer)
            .await
            .expect("wildcard ICE restart rebind must succeed");
        wait_for_gathering_complete(&events, restart_state_seq).await;
        let second_sdp = pc
            .local_description()
            .await
            .expect("restart local description")
            .sdp;
        assert!(
            second_sdp.contains("a=candidate:"),
            "wildcard rebind must gather fresh host candidates"
        );
        assert_ne!(
            first_sdp, second_sdp,
            "ICE restart must produce a new offer"
        );
        pc.close().await.expect("close wildcard PC");
    }
}
