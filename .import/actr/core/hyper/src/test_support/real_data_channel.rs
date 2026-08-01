use super::VNetPair;
use crate::transport::{ConnectionEvent, DataChannelStateSignal, DataLane};
use crate::wire::webrtc::WebRtcInboundMessage;
use crate::wire::webrtc::connection::WebRtcConnection;
use actr_protocol::PayloadType;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use webrtc::api::APIBuilder;
use webrtc::api::setting_engine::SettingEngine;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::configuration::RTCConfiguration;

/// An unsignaled pair of real WebRTC peers with a production DataChannel lane.
///
/// Integration tests can arm waiters against the exact signal stored by the
/// lane, then drive the installed `on_open` and `on_close` callbacks through a
/// real loopback transition.
pub struct RealDataChannelPair {
    offer: WebRtcConnection,
    answer: WebRtcConnection,
    offer_pc: Arc<RTCPeerConnection>,
    answer_pc: Arc<RTCPeerConnection>,
    lane: Arc<dyn DataLane>,
    state_signal: DataChannelStateSignal,
    _inbound: mpsc::Receiver<WebRtcInboundMessage>,
    events: broadcast::Receiver<ConnectionEvent>,
    vnet: VNetPair,
}

impl RealDataChannelPair {
    pub async fn new() -> Self {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let vnet = VNetPair::new()
            .await
            .expect("real DataChannel VNet should be created");

        let mut offer_settings = SettingEngine::default();
        offer_settings.set_vnet(Some(vnet.net_offerer.clone()));
        let offer_api = APIBuilder::new()
            .with_setting_engine(offer_settings)
            .build();

        let mut answer_settings = SettingEngine::default();
        answer_settings.set_vnet(Some(vnet.net_answerer.clone()));
        let answer_api = APIBuilder::new()
            .with_setting_engine(answer_settings)
            .build();

        let offer_pc = Arc::new(
            offer_api
                .new_peer_connection(RTCConfiguration::default())
                .await
                .expect("offer peer connection should be created"),
        );
        let answer_pc = Arc::new(
            answer_api
                .new_peer_connection(RTCConfiguration::default())
                .await
                .expect("answer peer connection should be created"),
        );
        let (offer_events, _) = broadcast::channel(16);
        let (answer_events, _) = broadcast::channel(16);
        let offer = WebRtcConnection::new(
            super::make_actor_id(901),
            offer_pc.clone(),
            offer_events.clone(),
        );
        let answer =
            WebRtcConnection::new(super::make_actor_id(902), answer_pc.clone(), answer_events);

        let (inbound_tx, inbound) = mpsc::channel(512);
        let answer_for_channel = answer.clone();
        answer_pc.on_data_channel(Box::new(move |channel| {
            let answer = answer_for_channel.clone();
            let inbound_tx = inbound_tx.clone();
            Box::pin(async move {
                answer
                    .register_received_data_channel(channel, PayloadType::RpcReliable, inbound_tx)
                    .await
                    .expect("received test DataChannel should register");
            })
        }));

        let lane = offer
            .get_lane(PayloadType::RpcReliable)
            .await
            .expect("offer DataChannel lane should be created before signaling");
        let state_signal = offer
            .data_channel_state_signal_for_test(PayloadType::RpcReliable)
            .await
            .expect("production lane should expose its state signal");
        let events = offer_events.subscribe();

        Self {
            offer,
            answer,
            offer_pc,
            answer_pc,
            lane,
            state_signal,
            _inbound: inbound,
            events,
            vnet,
        }
    }

    pub fn lane(&self) -> Arc<dyn DataLane> {
        self.lane.clone()
    }

    pub fn state_signal(&self) -> DataChannelStateSignal {
        self.state_signal.clone()
    }

    pub async fn open(&mut self) {
        signal_pair(&self.offer_pc, &self.answer_pc).await;
        self.wait_for_data_channel_event(true).await;
    }

    pub async fn close_data_channel(&mut self) {
        let channel = self.offer.data_channels().await[PayloadType::RpcReliable as usize]
            .clone()
            .expect("offer DataChannel should still be cached before close");
        channel
            .close()
            .await
            .expect("real DataChannel should close");
        self.wait_for_data_channel_event(false).await;
    }

    /// Close the offering connection through the production teardown path
    /// while its DataChannel is still Connecting, so no `on_open`/`on_close`
    /// callback can fire before teardown replaces the handlers with no-ops.
    pub async fn close_offer_connection(&self) {
        self.offer
            .close_immediately()
            .await
            .expect("offer connection teardown should succeed");
    }

    /// Attempt to create a lane on the offering connection, whatever state
    /// it is in. Used to prove registration is refused after teardown.
    pub async fn offer_get_lane(
        &self,
        payload_type: PayloadType,
    ) -> crate::transport::NetworkResult<Arc<dyn DataLane>> {
        self.offer.get_lane(payload_type).await
    }

    /// Register a freshly created incoming channel on the offering
    /// connection, returning the channel so the test can observe what the
    /// registration gate did with it.
    ///
    /// The fresh channel is minted on the ANSWER peer connection: after
    /// `close_offer_connection` the closed offer PC refuses to create
    /// channels, while the registration gate under test only inspects the
    /// session flag and the channel object itself.
    pub async fn offer_register_fresh_channel(
        &self,
        payload_type: PayloadType,
    ) -> (
        Arc<webrtc::data_channel::RTCDataChannel>,
        crate::transport::NetworkResult<Arc<dyn DataLane>>,
    ) {
        let channel = self.mint_incoming_channel(payload_type).await;
        let result = self
            .offer_register_channel(channel.clone(), payload_type)
            .await;
        (channel, result)
    }

    /// Mint a fresh DataChannel on the ANSWER peer connection. See
    /// [`Self::offer_register_fresh_channel`] for why the answer side mints.
    pub async fn mint_incoming_channel(
        &self,
        payload_type: PayloadType,
    ) -> Arc<webrtc::data_channel::RTCDataChannel> {
        self.answer_pc
            .create_data_channel(payload_type.as_str_name(), None)
            .await
            .expect("fresh data channel should be created")
    }

    /// Register an incoming channel on the offering connection.
    pub async fn offer_register_channel(
        &self,
        channel: Arc<webrtc::data_channel::RTCDataChannel>,
        payload_type: PayloadType,
    ) -> crate::transport::NetworkResult<Arc<dyn DataLane>> {
        let (tx, _rx) = mpsc::channel(8);
        self.offer
            .register_received_data_channel(channel, payload_type, tx)
            .await
    }

    /// Spawn an incoming-channel registration on the offering connection,
    /// so a test can interleave other operations while it is in flight.
    pub fn spawn_offer_register(
        &self,
        channel: Arc<webrtc::data_channel::RTCDataChannel>,
        payload_type: PayloadType,
    ) -> tokio::task::JoinHandle<crate::transport::NetworkResult<Arc<dyn DataLane>>> {
        let offer = self.offer.clone();
        tokio::spawn(async move {
            let (tx, _rx) = mpsc::channel(8);
            offer
                .register_received_data_channel(channel, payload_type, tx)
                .await
        })
    }

    /// Pause the next received-channel registration on the offering
    /// connection immediately before its first cache commit.
    pub fn set_offer_registration_commit_barrier(
        &self,
        payload_type: PayloadType,
        barrier: Arc<tokio::sync::Barrier>,
    ) {
        self.offer
            .set_registration_commit_barrier_for_test(payload_type, barrier);
    }

    /// The offering connection's cached DataChannel for one payload type.
    pub async fn offer_data_channel(
        &self,
        payload_type: PayloadType,
    ) -> Option<Arc<webrtc::data_channel::RTCDataChannel>> {
        self.offer.data_channels().await[payload_type as usize].clone()
    }

    /// The state signal stored by the offering connection's lane for one
    /// payload type, if a lane exists.
    pub async fn offer_state_signal(
        &self,
        payload_type: PayloadType,
    ) -> Option<DataChannelStateSignal> {
        self.offer
            .data_channel_state_signal_for_test(payload_type)
            .await
    }

    pub async fn shutdown(self) {
        let (offer_result, answer_result) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(
                self.offer.close_immediately(),
                self.answer.close_immediately()
            )
        })
        .await
        .expect("real peer-pair cleanup should remain bounded");
        offer_result.expect("offer cleanup should succeed");
        answer_result.expect("answer cleanup should succeed");
        tokio::time::timeout(Duration::from_secs(5), async {
            self.vnet.wan.lock().await.stop().await
        })
        .await
        .expect("test VNet shutdown should remain bounded")
        .expect("test VNet router should stop cleanly");
    }

    async fn wait_for_data_channel_event(&mut self, opened: bool) {
        let expected_session_id = self.offer.session_id();
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let event = self
                    .events
                    .recv()
                    .await
                    .expect("real DataChannel event stream should remain open");
                let matches_transition = if opened {
                    matches!(
                        event,
                        ConnectionEvent::DataChannelOpened {
                            session_id,
                            payload_type: PayloadType::RpcReliable,
                            ..
                        } if session_id == expected_session_id
                    )
                } else {
                    matches!(
                        event,
                        ConnectionEvent::DataChannelClosed {
                            session_id,
                            payload_type: PayloadType::RpcReliable,
                            ..
                        } if session_id == expected_session_id
                    )
                };
                if matches_transition {
                    return;
                }
            }
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "real DataChannel {} callback should fire",
                if opened { "open" } else { "close" }
            )
        });
    }
}

async fn signal_pair(offer: &RTCPeerConnection, answer: &RTCPeerConnection) {
    let offer_description = offer
        .create_offer(None)
        .await
        .expect("test offer should be created");
    let mut offer_gathered = offer.gathering_complete_promise().await;
    offer
        .set_local_description(offer_description)
        .await
        .expect("test offer should become local description");
    tokio::time::timeout(Duration::from_secs(10), offer_gathered.recv())
        .await
        .expect("offer ICE gathering should complete");
    answer
        .set_remote_description(
            offer
                .local_description()
                .await
                .expect("offer local description should exist"),
        )
        .await
        .expect("answer should accept remote offer");

    let answer_description = answer
        .create_answer(None)
        .await
        .expect("test answer should be created");
    let mut answer_gathered = answer.gathering_complete_promise().await;
    answer
        .set_local_description(answer_description)
        .await
        .expect("test answer should become local description");
    tokio::time::timeout(Duration::from_secs(10), answer_gathered.recv())
        .await
        .expect("answer ICE gathering should complete");
    offer
        .set_remote_description(
            answer
                .local_description()
                .await
                .expect("answer local description should exist"),
        )
        .await
        .expect("offer should accept remote answer");
}
