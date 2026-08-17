//! WebRTC negotiator
//!
//! Responsible for WebRTC Connect's Offer/Answer protocol quotient

use crate::lifecycle::CredentialState;
use crate::transport::NetworkResult;
use crate::wire::webrtc::adapter::{
    RTCConfiguration, RTCConfigurationBuilder, RTCIceCandidateInit, RTCIceServer,
    RTCIceTransportPolicy, RTCPeerConnection, RTCSessionDescription, SettingEngine,
};
use actr_protocol::{IceCandidate, TurnCredential};

use std::sync::Arc;
#[cfg(any(test, feature = "test-utils"))]
use std::sync::atomic::AtomicBool;
#[cfg(feature = "test-utils")]
use webrtc_util::vnet::net::Net;

// Re-export types from actr-config
pub use actr_config::{IceTransportPolicy, WebRtcConfig};

/// Keep the sans-I/O drivers off the application runtime. The pool is lazy,
/// process-global, and bounded: fewer than eight connections create only one
/// reactor thread each; additional connections share the existing threads.
/// Eight slots also keep the five-round abrupt-mobile regression isolated
/// even if an old v0.20 driver is temporarily non-cooperative while timing out.
const WEBRTC_REACTOR_POOL_SIZE: usize = 8;

/// Wildcard bind used for ICE-restart recovery. The v0.20.3 driver expands a
/// wildcard into current concrete interface candidates on every bind, so an
/// ICE restart can follow a mobile network handover without reusing a stale
/// concrete address.
///
/// Keeping the configured form in the driver also lets each rebind choose a
/// fresh ephemeral port rather than reusing the socket being replaced.
const ICE_RESTART_WILDCARD_ADDR: &str = "0.0.0.0:0";

/// WebRTC negotiator
#[derive(Clone)]
pub(crate) struct WebRtcNegotiator {
    /// Base WebRTC configuration (URLs + policy)
    config: WebRtcConfig,
    /// Latest credential state (updated on register/renew, contains TurnCredential)
    credential_state: CredentialState,
    /// Optional virtual network for integration testing.
    /// When set, RTCPeerConnection will use this VNet instead of real OS networking.
    #[cfg(feature = "test-utils")]
    vnet: Option<Arc<Net>>,
    /// Shared packet gate used to preserve outage simulation with the v0.20
    /// builder, which owns its UDP sockets and cannot consume the legacy VNet.
    #[cfg(feature = "test-utils")]
    packet_blocked: Option<Arc<AtomicBool>>,
    /// Per-instance assertion seam proving production construction selects
    /// Runtime::spawn_reactor rather than the ambient Runtime::spawn path.
    #[cfg(test)]
    reactor_spawn_probe: Arc<AtomicBool>,
}

impl WebRtcNegotiator {
    pub(crate) fn uses_turn(&self) -> bool {
        self.config.ice_servers.iter().any(|server| {
            server
                .urls
                .iter()
                .any(|url| url.starts_with("turn:") || url.starts_with("turns:"))
        })
    }

    ///
    /// # Arguments
    /// - `config`: WebRTC configuration
    /// - `credential_state`: shared credential state containing TurnCredential
    pub fn new(config: WebRtcConfig, credential_state: CredentialState) -> Self {
        Self {
            config,
            credential_state,
            #[cfg(feature = "test-utils")]
            vnet: None,
            #[cfg(feature = "test-utils")]
            packet_blocked: None,
            #[cfg(test)]
            reactor_spawn_probe: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Record the legacy virtual network used by the integration harness.
    ///
    /// webrtc-rs v0.20 no longer accepts this socket provider. The companion
    /// packet-blocked flag installs a runtime wrapper around the builder-owned
    /// UDP sockets and supplies the actual outage behavior.
    ///
    /// # Arguments
    /// - `vnet`: The virtual network instance (from `webrtc_util::vnet::net::Net`)
    #[cfg(feature = "test-utils")]
    pub fn set_vnet(&mut self, vnet: Arc<Net>) {
        tracing::info!("🌐 Legacy VNet test context attached to WebRtcNegotiator");
        self.vnet = Some(vnet);
    }

    #[cfg(feature = "test-utils")]
    pub fn set_test_packet_blocked(&mut self, blocked: Arc<AtomicBool>) {
        self.packet_blocked = Some(blocked);
    }

    fn build_rtc_configuration(
        &self,
        turn_credential: Option<&TurnCredential>,
    ) -> RTCConfiguration {
        let ice_servers: Vec<RTCIceServer> = self
            .config
            .ice_servers
            .iter()
            .map(|server| {
                let is_turn = server
                    .urls
                    .iter()
                    .any(|url| url.starts_with("turn:") || url.starts_with("turns:"));

                if is_turn {
                    match turn_credential {
                        Some(credential) => RTCIceServer {
                            urls: server.urls.clone(),
                            username: credential.username.clone(),
                            credential: credential.password.clone(),
                        },
                        None => {
                            tracing::warn!(
                                server_url = %server.urls.first().cloned().unwrap_or_default(),
                                "TURN credential is not ready; authentication may fail"
                            );
                            RTCIceServer {
                                urls: server.urls.clone(),
                                username: String::new(),
                                credential: String::new(),
                            }
                        }
                    }
                } else {
                    RTCIceServer {
                        urls: server.urls.clone(),
                        username: server.username.clone().unwrap_or_default(),
                        credential: server.credential.clone().unwrap_or_default(),
                    }
                }
            })
            .collect();

        if ice_servers.is_empty() {
            tracing::info!("🌐 No ICE servers configured; proceeding without STUN/TURN servers");
        } else {
            tracing::info!(
                ice_server_count = ice_servers.len(),
                "🌐 ICE servers configured"
            );
        }

        let ice_transport_policy = match self.config.ice_transport_policy {
            IceTransportPolicy::All => RTCIceTransportPolicy::All,
            IceTransportPolicy::Relay => RTCIceTransportPolicy::Relay,
        };

        RTCConfigurationBuilder::new()
            .with_ice_servers(ice_servers)
            .with_ice_transport_policy(ice_transport_policy)
            .build()
    }

    pub(crate) async fn current_rtc_configuration(&self) -> (RTCConfiguration, u64) {
        let (turn_credential, revision) = self.credential_state.turn_snapshot().await;
        (
            self.build_rtc_configuration(turn_credential.as_ref()),
            revision,
        )
    }

    /// Create RTCPeerConnection
    ///
    /// # Arguments
    /// - `is_answerer`: true if this node is the answerer (passive side), false if offerer (default)
    /// - `remote_fixed`: true if remote peer has fixed network configuration
    ///   (informational only since the webrtc-rs 0.20 migration — NAT 1:1
    ///   mappings are rejected at startup, so it no longer selects a mapping
    ///   strategy)
    ///
    /// # Returns
    /// newCreate's PeerConnection
    #[cfg_attr(
        feature = "opentelemetry",
        tracing::instrument(level = "info", skip(self), fields(ice_servers = self.config.ice_servers.len(), is_answerer, remote_fixed))
    )]
    pub async fn create_peer_connection(
        &self,
        is_answerer: bool,
        remote_fixed: bool,
    ) -> NetworkResult<RTCPeerConnection> {
        use rtc::rtp_transceiver::rtp_sender::{RTCRtpCodec, RTCRtpCodecParameters, RtpCodecKind};
        use webrtc::peer_connection::{
            MediaEngine, PeerConnection, PeerConnectionBuilder, Registry,
            register_default_interceptors,
        };

        // Create MediaEngine and register codecs
        let mut media_engine = MediaEngine::default();

        // Register VP8 video codec
        media_engine.register_codec(
            RTCRtpCodecParameters {
                rtp_codec: RTCRtpCodec {
                    mime_type: "video/VP8".to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 96,
            },
            RtpCodecKind::Video,
        )?;

        // Register H264 video codec
        media_engine.register_codec(
            RTCRtpCodecParameters {
                rtp_codec: RTCRtpCodec {
                    mime_type: "video/H264".to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line:
                        "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f"
                            .to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 102,
            },
            RtpCodecKind::Video,
        )?;

        // Register OPUS audio codec
        media_engine.register_codec(
            RTCRtpCodecParameters {
                rtp_codec: RTCRtpCodec {
                    mime_type: "audio/opus".to_owned(),
                    clock_rate: 48000,
                    channels: 2,
                    sdp_fmtp_line: "minptime=10;useinbandfec=1".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 111,
            },
            RtpCodecKind::Audio,
        )?;

        let (rtc_config, turn_revision) = self.current_rtc_configuration().await;

        // Create SettingEngine with role-based configuration
        let mut setting_engine = SettingEngine::default();

        // Inject VNet if configured (test-utils only)
        #[cfg(feature = "test-utils")]
        match (&self.vnet, &self.packet_blocked) {
            (Some(_), Some(_)) => {
                tracing::info!("webrtc-rs 0.20 test packet filter enabled over its UDP socket")
            }
            (Some(_), None) => tracing::warn!(
                "webrtc-rs 0.20 does not expose VNet socket injection; no packet filter was provided"
            ),
            _ => {}
        }

        // Apply ICE candidate acceptance wait times (for both Offerer and Answerer)
        self.apply_ice_wait_times(&mut setting_engine);

        // Enable upstream webrtc-rs/webrtc#868: rebind UDP sockets on ICE restart so
        // a peer stalled after ~10s background (Android) recovers instead of wedging.
        // Opt-in flag added in webrtc-rs/rtc#168.
        setting_engine.set_discard_local_candidates_during_ice_restart(true);

        // Apply advanced parameters (UDP ports) only for Answerer
        if is_answerer {
            tracing::info!(
                remote_fixed,
                "🎭 Applying advanced WebRTC parameters (Answerer mode)"
            );
            self.apply_answerer_config();
        } else {
            tracing::info!("🎭 Using default WebRTC configuration (Offerer mode)");
        }

        // The default registry is a deeply nested generic interceptor chain. Keeping
        // that type inside the async builder makes the PeerConnection construction
        // future large enough to overflow small runtime/test thread stacks. Erase the
        // chain before it crosses the async boundary.
        let registry = register_default_interceptors(Registry::new(), &mut media_engine)?.boxed();
        let events = RTCPeerConnection::events();
        let udp_addrs = self.select_udp_bind_addrs();
        let peer_connection_builder = PeerConnectionBuilder::new()
            .with_configuration(rtc_config)
            .with_media_engine(media_engine)
            .with_setting_engine(setting_engine)
            .with_interceptor_registry(registry)
            .with_handler(events.clone())
            // v0.20 drives every connection in one sans-I/O task. A stale
            // TURN/ICE timeout loop must not starve actr signaling, heartbeat,
            // or peer reaping on the ambient Tokio runtime.
            .with_dedicated_reactor_thread(true)
            .with_reactor_pool_size(WEBRTC_REACTOR_POOL_SIZE)
            .with_udp_addrs(udp_addrs);
        #[cfg(test)]
        let peer_connection_builder = peer_connection_builder.with_runtime(
            crate::wire::webrtc::test_runtime::reactor_probe_runtime(Arc::clone(
                &self.reactor_spawn_probe,
            )),
        );
        #[cfg(feature = "test-utils")]
        let peer_connection_builder = match &self.packet_blocked {
            Some(blocked) => peer_connection_builder.with_runtime(
                crate::wire::webrtc::test_runtime::packet_filter_runtime(blocked.clone()),
            ),
            None => peer_connection_builder,
        };
        let peer_connection = peer_connection_builder.build().await?;
        let peer_connection = RTCPeerConnection::new(
            Arc::new(peer_connection) as Arc<dyn PeerConnection>,
            events,
            turn_revision,
        );

        peer_connection.on_ice_connection_state_change(Box::new(move |state| {
            tracing::info!("🔄 ICE Connection State Changed: {:?}", state);
            Box::pin(async move {})
        }));

        peer_connection.on_ice_gathering_state_change(Box::new(move |state| {
            tracing::info!("🔄 ICE Gathering State Changed: {:?}", state);
            Box::pin(async move {})
        }));

        tracing::info!("✅ Create RTCPeerConnection with VP8, H264, OPUS codecs");

        Ok(peer_connection)
    }

    /// Apply ICE candidate acceptance wait times (for both Offerer and Answerer)
    fn apply_ice_wait_times(&self, setting_engine: &mut SettingEngine) {
        use rtc::ice::network_type::NetworkType;
        use std::time::Duration;

        let advanced = &self.config.advanced;

        crate::timer::register_external(
            crate::timer::ids::EXTERNAL_ICE_CANDIDATE_SELECTION,
            Duration::from_millis(
                advanced
                    .ice_host_acceptance_min_wait
                    .max(advanced.ice_srflx_acceptance_min_wait)
                    .max(advanced.ice_prflx_acceptance_min_wait)
                    .max(advanced.ice_relay_acceptance_min_wait),
            ),
        );

        // Limit local ICE gathering to IPv4 UDP candidates. On local/dev setups
        // with localhost STUN/TURN, unusable UDP6 probes can stall gathering and
        // make DataChannel readiness flaky.
        setting_engine.set_network_types(vec![NetworkType::Udp4]);

        setting_engine.set_host_acceptance_min_wait(Some(Duration::from_millis(
            advanced.ice_host_acceptance_min_wait,
        )));
        setting_engine.set_srflx_acceptance_min_wait(Some(Duration::from_millis(
            advanced.ice_srflx_acceptance_min_wait,
        )));
        setting_engine.set_prflx_acceptance_min_wait(Some(Duration::from_millis(
            advanced.ice_prflx_acceptance_min_wait,
        )));
        setting_engine.set_relay_acceptance_min_wait(Some(Duration::from_millis(
            advanced.ice_relay_acceptance_min_wait,
        )));

        tracing::info!(
            "🔧 ICE wait times: host={}ms, srflx={}ms, prflx={}ms, relay={}ms",
            advanced.ice_host_acceptance_min_wait,
            advanced.ice_srflx_acceptance_min_wait,
            advanced.ice_prflx_acceptance_min_wait,
            advanced.ice_relay_acceptance_min_wait
        );
    }

    /// Log the Answerer-side fixed-port policy.
    ///
    /// No NAT 1:1 mapping is applied here: rtc 0.20 stores `nat_1to1_ips` on
    /// the setting engine without ever reading it back, so `public_ips` is
    /// rejected at startup instead (see `WebRtcAdvancedConfig::validate`)
    /// rather than silently advertising internal addresses.
    fn apply_answerer_config(&self) {
        let advanced = &self.config.advanced;

        if let Some((min, max)) = advanced.udp_ports {
            tracing::warn!(
                min,
                max,
                wildcard_bind = ICE_RESTART_WILDCARD_ADDR,
                "Configured UDP port range is ignored while wildcard ICE-restart recovery is enabled"
            );
        } else {
            tracing::info!(
                wildcard_bind = ICE_RESTART_WILDCARD_ADDR,
                "🔧 Using wildcard UDP bind for ICE-restart recovery"
            );
        }
    }

    fn select_udp_bind_addrs(&self) -> Vec<String> {
        // Keep the wildcard unresolved so webrtc-rs can enumerate the current
        // interfaces and choose fresh ephemeral ports on every ICE restart.
        vec![ICE_RESTART_WILDCARD_ADDR.to_owned()]
    }

    /// Create Offer (Trickle ICE mode)
    ///
    /// # Arguments
    /// - `peer_connection`: PeerConnection
    ///
    /// # Returns
    /// Offer SDP string (ICE candidates sent separately via on_ice_candidate callback)
    #[cfg_attr(
        feature = "opentelemetry",
        tracing::instrument(level = "info", skip_all)
    )]
    pub async fn create_offer(&self, peer_connection: &RTCPeerConnection) -> NetworkResult<String> {
        // Note: Negotiated DataChannel should be created BEFORE calling this method
        // to trigger ICE gathering (done in coordinator.rs)

        // Create Offer
        let offer = peer_connection.create_offer(None).await?;
        let offer_sdp = offer.sdp.clone();

        // Set local Description (this triggers ICE gathering)
        peer_connection.set_local_description(offer).await?;

        // DO NOT wait for ICE gathering - this is Trickle ICE
        // ICE candidates will be sent via on_ice_candidate callback

        tracing::info!(
            "✅ Create Offer (SDP length: {}, Trickle ICE mode)",
            offer_sdp.len()
        );

        Ok(offer_sdp)
    }

    /// Create ICE restart Offer (offerer side)
    pub async fn create_ice_restart_offer(
        &self,
        peer_connection: &RTCPeerConnection,
    ) -> NetworkResult<String> {
        peer_connection.restart_ice().await?;
        let offer = peer_connection.create_offer(None).await?;
        let offer_sdp = offer.sdp.clone();

        peer_connection.set_local_description(offer).await?;

        tracing::info!(
            "✅ Create ICE Restart Offer (SDP length: {}, Trickle ICE mode)",
            offer_sdp.len()
        );

        Ok(offer_sdp)
    }

    /// Handle Answer
    ///
    /// # Arguments
    /// - `peer_connection`: PeerConnection
    /// - `answer_sdp`: Answer SDP string
    #[cfg_attr(
        feature = "opentelemetry",
        tracing::instrument(level = "info", skip_all, fields(answer_len = answer_sdp.len()))
    )]
    pub async fn handle_answer(
        &self,
        peer_connection: &RTCPeerConnection,
        answer_sdp: String,
    ) -> NetworkResult<()> {
        // Setremote Description
        let answer = RTCSessionDescription::answer(answer_sdp)?;
        peer_connection.set_remote_description(answer).await?;

        tracing::info!("✅ Handle Answer");

        Ok(())
    }

    /// Create Answer (passive side, Trickle ICE mode)
    ///
    /// # Arguments
    /// - `peer_connection`: PeerConnection
    /// - `offer_sdp`: Offer SDP string
    ///
    /// # Returns
    /// Answer SDP string (ICE candidates sent separately)
    #[cfg_attr(
        feature = "opentelemetry",
        tracing::instrument(level = "info", skip_all)
    )]
    pub async fn create_answer(
        &self,
        peer_connection: &RTCPeerConnection,
        offer_sdp: String,
    ) -> NetworkResult<String> {
        // Set remote Description (Offer)
        let offer = RTCSessionDescription::offer(offer_sdp)?;
        peer_connection.set_remote_description(offer).await?;

        // Create Answer
        let answer = peer_connection.create_answer(None).await?;
        let answer_sdp = answer.sdp.clone();

        // Set local Description (triggers ICE gathering)
        peer_connection.set_local_description(answer).await?;

        // DO NOT wait for ICE gathering - Trickle ICE mode
        // ICE candidates will be sent via on_ice_candidate callback

        tracing::info!(
            "✅ Create Answer (SDP length: {}, Trickle ICE mode)",
            answer_sdp.len()
        );

        Ok(answer_sdp)
    }

    /// add ICE Candidate
    ///
    /// # Arguments
    /// - `peer_connection`: PeerConnection
    /// - `candidate`: ICE Candidate and its ICE-generation metadata
    #[cfg_attr(
        feature = "opentelemetry",
        tracing::instrument(
            level = "trace",
            skip_all,
            fields(candidate_len = candidate.candidate.len())
        )
    )]
    pub async fn add_ice_candidate(
        &self,
        peer_connection: &RTCPeerConnection,
        candidate: IceCandidate,
    ) -> NetworkResult<()> {
        let sdp_mline_index = candidate
            .sdp_mline_index
            .map(u16::try_from)
            .transpose()
            .map_err(|_| {
                crate::transport::NetworkError::InvalidArgument(
                    "ICE candidate sdp_mline_index exceeds u16".to_string(),
                )
            })?;
        let ice_candidate = RTCIceCandidateInit {
            candidate: candidate.candidate,
            sdp_mid: candidate.sdp_mid,
            sdp_mline_index,
            username_fragment: candidate.username_fragment,
            url: None,
        };

        peer_connection.add_ice_candidate(ice_candidate).await?;

        tracing::trace!("✅ add ICE Candidate");

        Ok(())
    }
}

#[cfg(test)]
#[path = "negotiator_tests.rs"]
mod tests;
