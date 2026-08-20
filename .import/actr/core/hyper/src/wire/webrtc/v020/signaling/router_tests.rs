//! SignalingRouter tests: routing by remote ActrId, answerer creation on
//! unknown offer, and role-assignment dispatch.

use std::sync::Arc;

use actr_protocol::{ActrId, ActrType, Realm};
use async_trait::async_trait;
use tokio::sync::mpsc;

use webrtc::error::Error as RtcError;
use webrtc::peer_connection::{
    RTCConfiguration, RTCIceCandidateInit, RTCSdpType, RTCSessionDescription,
};

use crate::wire::webrtc::v020::event_handler::WebRtcEventHandler;
use crate::wire::webrtc::v020::manager::RtcConnectionManager;
use crate::wire::webrtc::v020::peer::effects::{SessionFactory, SessionOps};
use crate::wire::webrtc::v020::peer::reducer::SignalingMessage;

use super::router::SignalingRouter;

fn test_actor_id(serial_number: u64) -> ActrId {
    ActrId {
        realm: Realm { realm_id: 1 },
        serial_number,
        r#type: ActrType {
            manufacturer: "acme".to_string(),
            name: "node".to_string(),
            version: "1.0.0".to_string(),
        },
    }
}

fn desc(sdp_type: RTCSdpType) -> RTCSessionDescription {
    let mut d = RTCSessionDescription::default();
    d.sdp_type = sdp_type;
    d.sdp = format!("fake-{sdp_type:?}");
    d
}

struct FakeOps;

#[async_trait]
impl SessionOps for FakeOps {
    async fn create_offer(&self) -> Result<RTCSessionDescription, RtcError> {
        Ok(desc(RTCSdpType::Offer))
    }
    async fn create_answer(&self) -> Result<RTCSessionDescription, RtcError> {
        Ok(desc(RTCSdpType::Answer))
    }
    async fn set_local_description(&self, _d: RTCSessionDescription) -> Result<(), RtcError> {
        Ok(())
    }
    async fn set_remote_description(&self, _d: RTCSessionDescription) -> Result<(), RtcError> {
        Ok(())
    }
    async fn add_ice_candidate(&self, _c: RTCIceCandidateInit) -> Result<(), RtcError> {
        Ok(())
    }
    async fn restart_ice(&self) -> Result<(), RtcError> {
        Ok(())
    }
    async fn set_configuration(&self, _config: RTCConfiguration) -> Result<(), RtcError> {
        Ok(())
    }
    async fn create_data_channel(
        &self,
        label: &str,
        _init: webrtc::data_channel::RTCDataChannelInit,
    ) -> Result<Arc<dyn webrtc::data_channel::DataChannel>, RtcError> {
        Ok(crate::wire::webrtc::v020::peer::effects::test_support::fake_data_channel(label))
    }
    async fn add_track(
        &self,
        _track: Arc<dyn webrtc::media_stream::track_local::TrackLocal>,
    ) -> Result<Arc<dyn webrtc::rtp_transceiver::RtpSender>, RtcError> {
        Err(RtcError::Other("no media in router tests".into()))
    }
    async fn remove_track(
        &self,
        _sender: &Arc<dyn webrtc::rtp_transceiver::RtpSender>,
    ) -> Result<(), RtcError> {
        Ok(())
    }
    async fn get_senders(&self) -> Vec<Arc<dyn webrtc::rtp_transceiver::RtpSender>> {
        Vec::new()
    }
    async fn get_stats(&self) -> webrtc::peer_connection::RTCStatsReport {
        Default::default()
    }
    async fn close(&self) -> Result<(), RtcError> {
        Ok(())
    }
}

struct AutoFactory;

#[async_trait]
impl SessionFactory for AutoFactory {
    async fn build(
        &self,
        _handler: Arc<WebRtcEventHandler>,
    ) -> Result<Arc<dyn SessionOps>, RtcError> {
        Ok(Arc::new(FakeOps))
    }
}

fn router() -> (
    Arc<RtcConnectionManager>,
    SignalingRouter,
    mpsc::Receiver<(ActrId, SignalingMessage)>,
) {
    let (signaling_tx, signaling_rx) = mpsc::channel(16);
    let manager = Arc::new(RtcConnectionManager::new(
        Arc::new(AutoFactory),
        signaling_tx,
    ));
    manager.supervisor().set_signaling_generation(Some(0));
    let local_id = test_actor_id(100);
    let router = SignalingRouter::new(
        manager.clone(),
        crate::wire::webrtc::v020::signaling::SharedLocalId::new(local_id),
    );
    (manager, router, signaling_rx)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unknown_offer_creates_answerer_controller() {
    let (manager, router, _rx) = router();
    let peer = test_actor_id(1);
    assert!(manager.get_controller(&peer).await.is_none());

    router
        .route_inbound(
            peer.clone(),
            SignalingMessage::Offer {
                desc: Box::new(desc(RTCSdpType::Offer)),
                exchange_id: Some("offer-1".to_string()),
            },
        )
        .await;

    assert!(
        manager.get_controller(&peer).await.is_some(),
        "an unknown offer must create an answerer controller"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn answer_and_candidate_for_unknown_peer_are_dropped_safely() {
    let (manager, router, _rx) = router();
    let peer = test_actor_id(2);

    // These must not create a controller nor panic — just dropped.
    router
        .route_inbound(
            peer.clone(),
            SignalingMessage::Answer {
                desc: Box::new(desc(RTCSdpType::Answer)),
                exchange_id: Some("answer-1".to_string()),
            },
        )
        .await;
    router
        .route_inbound(
            peer.clone(),
            SignalingMessage::Candidate(RTCIceCandidateInit {
                candidate: "candidate:1 1 udp 1 192.0.2.1 9 typ host".to_string(),
                sdp_mid: None,
                sdp_mline_index: None,
                username_fragment: None,
                url: None,
            }),
        )
        .await;

    assert!(manager.get_controller(&peer).await.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn role_assignment_routes_to_controller() {
    let (manager, router, _rx) = router();
    let peer = test_actor_id(3);

    // An unsolicited assignment (the other peer negotiated) creates the
    // controller so it can record its role.
    router
        .route_inbound(
            peer.clone(),
            SignalingMessage::RoleAssignment { is_offerer: false },
        )
        .await;

    let handle = manager
        .get_controller(&peer)
        .await
        .expect("controller created for unsolicited role assignment");
    // The answerer role is recorded (snapshot is still Idle but the controller
    // exists and is not sendable).
    assert!(!handle.snapshot().sendable);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn outbound_message_is_forwarded_by_run_loop() {
    let (manager, _router, mut signaling_rx) = router();

    // A controller that connects sends a RoleNegotiation on the shared outbound
    // channel the router consumes.
    manager.connect(test_actor_id(4)).await;
    let (to, message) =
        tokio::time::timeout(std::time::Duration::from_secs(5), signaling_rx.recv())
            .await
            .expect("timed out waiting for outbound role negotiation")
            .expect("outbound channel closed");
    assert_eq!(to, test_actor_id(4));
    assert!(matches!(message, SignalingMessage::RoleNegotiation));
}
