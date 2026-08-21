//! RFC T1 Controller tests.
//!
//! These tests run the real Controller task with a recording effect port,
//! semantic fact injector, Snapshot observer, and per-fixture manual clock.
//! They never create a SessionFactory, SessionOps, or PeerConnection.

use std::sync::Arc;
use std::time::Duration;

use actr_protocol::{ActrId, ActrType, Realm};
use tokio::sync::mpsc;
use tokio::time::timeout;
use webrtc::error::Error as RtcError;
use webrtc::peer_connection::{
    RTCConfiguration, RTCPeerConnectionState, RTCSdpType, RTCSessionDescription,
};

use super::controller::{RtcPeerController, SharedNodeDeps};
use super::effect_port::{RtcEffectIdentity, TaggedRtcPeerEffect, TestEffectSink};
use super::handle::RtcPeerControllerHandle;
use super::policy_clock::{ManualPolicyClock, PolicyClock};
use super::reducer::{
    EffectCompletion, LaneEvent, MAX_STATS_SAMPLE_ATTEMPTS, RtcConfigurationRefreshError,
    RtcPeerEffect, SignalingMessage,
};
use crate::wire::webrtc::v020::events::{PeerDriverEvent, PeerEventMailbox};
use crate::wire::webrtc::v020::recovery::{NETWORK_RECOVERY_TIMEOUT, RtcNetworkEvent};
use crate::wire::webrtc::v020::status::{PeerTransportRetirement, RtcPeerSnapshot};

const WAIT: Duration = Duration::from_secs(5);

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
    let mut description = RTCSessionDescription::default();
    description.sdp_type = sdp_type;
    description.sdp = format!("fake-{sdp_type:?}");
    description
}

fn desc_with_ufrag(sdp_type: RTCSdpType, ufrag: &str) -> RTCSessionDescription {
    let mut description = RTCSessionDescription::default();
    description.sdp_type = sdp_type;
    description.sdp = format!("v=0\r\na=ice-ufrag:{ufrag}\r\na=ice-pwd:test-password\r\n");
    description
}

/// Controlled semantic-fact input for the real Controller task. Raw WebRTC
/// objects are intentionally absent from this T1 seam.
struct FactInjector {
    mailbox: PeerEventMailbox,
    lane_tx: mpsc::Sender<LaneEvent>,
}

impl FactInjector {
    fn driver(&self, event: PeerDriverEvent) {
        self.mailbox.try_publish(event);
    }

    async fn lane(&self, event: LaneEvent) {
        self.lane_tx
            .send(event)
            .await
            .expect("controller lane fact");
    }
}

/// Stable T1 fixture defined by the RFC. Every field is owned by this test
/// instance, including the clock and effect recorder.
struct ControllerTestHarness {
    handle: RtcPeerControllerHandle,
    effects: Arc<TestEffectSink>,
    facts: FactInjector,
    clock: Arc<ManualPolicyClock>,
    signaling_rx: mpsc::Receiver<(ActrId, SignalingMessage)>,
    supervisor: crate::wire::webrtc::v020::recovery::RtcRecoverySupervisor,
    retirement_rx: tokio::sync::broadcast::Receiver<PeerTransportRetirement>,
    task: tokio::task::JoinHandle<()>,
}

impl ControllerTestHarness {
    fn spawn(serial: u64) -> Self {
        let peer_id = test_actor_id(serial);
        let effects = TestEffectSink::new(peer_id.clone());
        let effect_port: Arc<dyn super::effect_port::RtcPeerEffectPort> = effects.clone();
        let clock = Arc::new(ManualPolicyClock::default());
        let policy_clock: Arc<dyn PolicyClock> = clock.clone();
        let (signaling_tx, signaling_rx) = mpsc::channel(16);
        let supervisor = crate::wire::webrtc::v020::recovery::RtcRecoverySupervisor::new();
        supervisor.set_signaling_generation(Some(0));
        let (peer_transport_retired, retirement_rx) = tokio::sync::broadcast::channel(8);
        let deps = SharedNodeDeps {
            inbound: Arc::new(crate::wire::webrtc::v020::inbound::InboundRouter::discard()),
            build_throttle: Arc::new(crate::wire::webrtc::v020::recovery::BuildThrottle::new(
                crate::wire::webrtc::v020::recovery::DEFAULT_MAX_CONCURRENT_BUILDS,
                Duration::ZERO,
            )),
            media_registry: None,
            buffer_config: crate::wire::webrtc::v020::config::RtcBufferConfig::default(),
            #[cfg(feature = "test-utils")]
            driver_event_observer: None,
            controller_lifecycle_id: 1,
            peer_generation_invalidated: None,
            peer_sendability_changed: None,
            peer_transport_retired: Some(peer_transport_retired),
            peer_snapshot_events: None,
        };
        let (handle, lane_tx, mailbox, task) = RtcPeerController::spawn_with_effect_port(
            peer_id,
            1,
            effect_port,
            signaling_tx,
            supervisor.subscribe(),
            supervisor.subscribe_signaling_generation(),
            deps,
            policy_clock,
        );
        Self {
            handle,
            effects,
            facts: FactInjector { mailbox, lane_tx },
            clock,
            signaling_rx,
            supervisor,
            retirement_rx,
            task,
        }
    }

    async fn next_effect(&self) -> TaggedRtcPeerEffect {
        timeout(WAIT, self.effects.next_effect())
            .await
            .expect("timed out waiting for T1 effect")
            .expect("T1 effect channel closed")
    }

    async fn expect_role_negotiation(&mut self) {
        let (_, message) = timeout(WAIT, self.signaling_rx.recv())
            .await
            .expect("role negotiation timeout")
            .expect("signaling channel closed");
        assert!(matches!(message, SignalingMessage::RoleNegotiation));
    }

    async fn shutdown(self) {
        let Self {
            handle,
            effects,
            facts: _,
            clock: _,
            signaling_rx: _,
            supervisor: _,
            retirement_rx: _,
            mut task,
        } = self;
        handle.close().await.expect("close T1 controller");

        timeout(WAIT, async {
            loop {
                tokio::select! {
                    result = &mut task => {
                        result.expect("T1 controller task panicked");
                        break;
                    }
                    operation = effects.next_effect() => {
                        let operation = operation.expect("T1 effect channel closed before teardown");
                        effects.complete(shutdown_completion(&operation)).await
                            .expect("complete T1 teardown effect");
                    }
                }
            }
        })
        .await
        .expect("T1 controller teardown timed out");
        assert_eq!(effects.close_all_calls(), 1, "effect teardown called once");
    }
}

async fn drive_offerer_to_sendable(h: &mut ControllerTestHarness) {
    h.handle.connect().await.expect("connect T1 controller");
    h.expect_role_negotiation().await;
    h.handle
        .apply_role_assignment(true)
        .await
        .expect("assign offerer role");

    let build = h.next_effect().await;
    let build_operation = build.identity.operation_id;
    assert!(matches!(
        build.effect,
        RtcPeerEffect::BuildSession { generation: 1, .. }
    ));
    h.effects
        .complete(EffectCompletion::BuildSession {
            generation: 1,
            operation_id: build_operation,
            result: Ok(()),
        })
        .await
        .expect("complete T1 build");

    let offer = h.next_effect().await;
    let offer_operation = offer.identity.operation_id;
    assert!(matches!(
        offer.effect,
        RtcPeerEffect::CreateOffer { generation: 1, .. }
    ));
    h.effects
        .complete(EffectCompletion::CreateOffer {
            generation: 1,
            operation_id: offer_operation,
            result: Ok(Box::new(desc(RTCSdpType::Offer))),
        })
        .await
        .expect("complete T1 create offer");

    let local = h.next_effect().await;
    let local_operation = local.identity.operation_id;
    assert!(matches!(
        local.effect,
        RtcPeerEffect::SetLocalDescription { generation: 1, .. }
    ));
    h.effects
        .complete(EffectCompletion::SetLocalDescription {
            generation: 1,
            operation_id: local_operation,
            result: Ok(()),
        })
        .await
        .expect("complete T1 local description");
    let (_, signaling) = timeout(WAIT, h.signaling_rx.recv())
        .await
        .expect("T1 offer timeout")
        .expect("T1 signaling channel closed");
    assert!(matches!(signaling, SignalingMessage::Offer { .. }));

    h.handle
        .apply_remote_description(desc(RTCSdpType::Answer))
        .await
        .expect("apply T1 answer");
    let remote = h.next_effect().await;
    let remote_operation = remote.identity.operation_id;
    assert!(matches!(
        remote.effect,
        RtcPeerEffect::SetRemoteDescription { generation: 1, .. }
    ));
    h.effects
        .complete(EffectCompletion::SetRemoteDescription {
            generation: 1,
            operation_id: remote_operation,
            result: Ok(()),
        })
        .await
        .expect("complete T1 remote description");

    h.facts.driver(PeerDriverEvent::PeerConnectionStateChanged {
        generation: 1,
        state: RTCPeerConnectionState::Connected,
    });
    h.facts
        .lane(LaneEvent::Opened {
            generation: 1,
            lane: crate::wire::webrtc::v020::status::LaneKind::RpcReliable,
        })
        .await;
    await_snapshot(&h.handle, "T1 sendable", |snapshot| snapshot.sendable).await;

    // The empty T1 report deliberately cannot resolve a selected pair. Settle
    // the complete bounded retry sequence so callers never inherit an
    // unrelated GetStats operation from this setup helper.
    for _ in 0..MAX_STATS_SAMPLE_ATTEMPTS {
        let stats = h.next_effect().await;
        let stats_operation = stats.identity.operation_id;
        assert!(matches!(
            stats.effect,
            RtcPeerEffect::GetStats { generation: 1, .. }
        ));
        h.effects
            .complete(EffectCompletion::GetStats {
                generation: 1,
                operation_id: stats_operation,
                report: Default::default(),
            })
            .await
            .expect("complete T1 stats");
    }
}

async fn drive_answerer_to_sendable(h: &mut ControllerTestHarness) {
    h.handle
        .apply_remote_description(desc_with_ufrag(RTCSdpType::Offer, "remote-initial"))
        .await
        .expect("apply T1 initial offer");

    let build = h.next_effect().await;
    let build_operation = build.identity.operation_id;
    assert!(matches!(
        build.effect,
        RtcPeerEffect::BuildSession { generation: 1, .. }
    ));
    h.effects
        .complete(EffectCompletion::BuildSession {
            generation: 1,
            operation_id: build_operation,
            result: Ok(()),
        })
        .await
        .expect("complete T1 answerer build");

    let remote = h.next_effect().await;
    let remote_operation = remote.identity.operation_id;
    assert!(matches!(
        remote.effect,
        RtcPeerEffect::SetRemoteDescription { generation: 1, .. }
    ));
    h.effects
        .complete(EffectCompletion::SetRemoteDescription {
            generation: 1,
            operation_id: remote_operation,
            result: Ok(()),
        })
        .await
        .expect("complete T1 answerer remote description");

    let answer = h.next_effect().await;
    let answer_operation = answer.identity.operation_id;
    assert!(matches!(
        answer.effect,
        RtcPeerEffect::CreateAnswer { generation: 1, .. }
    ));
    h.effects
        .complete(EffectCompletion::CreateAnswer {
            generation: 1,
            operation_id: answer_operation,
            result: Ok(Box::new(desc_with_ufrag(
                RTCSdpType::Answer,
                "local-initial",
            ))),
        })
        .await
        .expect("complete T1 create answer");

    let local = h.next_effect().await;
    let local_operation = local.identity.operation_id;
    assert!(matches!(
        local.effect,
        RtcPeerEffect::SetLocalDescription { generation: 1, .. }
    ));
    h.effects
        .complete(EffectCompletion::SetLocalDescription {
            generation: 1,
            operation_id: local_operation,
            result: Ok(()),
        })
        .await
        .expect("complete T1 answerer local description");
    let (_, signaling) = timeout(WAIT, h.signaling_rx.recv())
        .await
        .expect("T1 answer timeout")
        .expect("T1 signaling channel closed");
    assert!(matches!(signaling, SignalingMessage::Answer { .. }));

    h.facts.driver(PeerDriverEvent::PeerConnectionStateChanged {
        generation: 1,
        state: RTCPeerConnectionState::Connected,
    });
    h.facts
        .lane(LaneEvent::Opened {
            generation: 1,
            lane: crate::wire::webrtc::v020::status::LaneKind::RpcReliable,
        })
        .await;
    await_snapshot(&h.handle, "T1 answerer sendable", |snapshot| {
        snapshot.sendable
    })
    .await;

    for _ in 0..MAX_STATS_SAMPLE_ATTEMPTS {
        let stats = h.next_effect().await;
        let stats_operation = stats.identity.operation_id;
        assert!(matches!(
            stats.effect,
            RtcPeerEffect::GetStats { generation: 1, .. }
        ));
        h.effects
            .complete(EffectCompletion::GetStats {
                generation: 1,
                operation_id: stats_operation,
                report: Default::default(),
            })
            .await
            .expect("complete T1 answerer stats");
    }
}

fn shutdown_completion(operation: &TaggedRtcPeerEffect) -> EffectCompletion {
    let error = || RtcError::Other("T1 fixture shutdown".into());
    match &operation.effect {
        RtcPeerEffect::BuildSession {
            generation,
            operation_id,
        } => EffectCompletion::BuildSession {
            generation: *generation,
            operation_id: *operation_id,
            result: Err(error()),
        },
        RtcPeerEffect::CreateOffer {
            generation,
            operation_id,
        } => EffectCompletion::CreateOffer {
            generation: *generation,
            operation_id: *operation_id,
            result: Err(error()),
        },
        RtcPeerEffect::CreateAnswer {
            generation,
            operation_id,
        } => EffectCompletion::CreateAnswer {
            generation: *generation,
            operation_id: *operation_id,
            result: Err(error()),
        },
        RtcPeerEffect::SetLocalDescription {
            generation,
            operation_id,
            ..
        } => EffectCompletion::SetLocalDescription {
            generation: *generation,
            operation_id: *operation_id,
            result: Err(error()),
        },
        RtcPeerEffect::SetRemoteDescription {
            generation,
            operation_id,
            ..
        } => EffectCompletion::SetRemoteDescription {
            generation: *generation,
            operation_id: *operation_id,
            result: Err(error()),
        },
        RtcPeerEffect::AddIceCandidate {
            generation,
            operation_id,
            ice_generation,
            ..
        } => EffectCompletion::AddIceCandidate {
            generation: *generation,
            operation_id: *operation_id,
            ice_generation: *ice_generation,
            result: Err(error()),
        },
        RtcPeerEffect::RestartIce {
            generation,
            operation_id,
            ice_generation,
        } => EffectCompletion::RestartIce {
            generation: *generation,
            operation_id: *operation_id,
            ice_generation: *ice_generation,
            result: Err(error()),
        },
        RtcPeerEffect::RefreshConfiguration {
            generation,
            operation_id,
            ..
        } => EffectCompletion::RefreshConfiguration {
            generation: *generation,
            operation_id: *operation_id,
            result: Err(error()),
        },
        RtcPeerEffect::GetStats {
            generation,
            operation_id,
        } => EffectCompletion::GetStats {
            generation: *generation,
            operation_id: *operation_id,
            report: Default::default(),
        },
        RtcPeerEffect::CloseSession {
            generation,
            operation_id,
        } => EffectCompletion::CloseSession {
            generation: *generation,
            operation_id: *operation_id,
            result: Ok(()),
        },
        RtcPeerEffect::CreateLane { .. } | RtcPeerEffect::SendSignaling { .. } => {
            panic!("non-blocking effect reached TestEffectSink")
        }
    }
}

async fn await_snapshot(
    handle: &RtcPeerControllerHandle,
    what: &str,
    predicate: impl Fn(&RtcPeerSnapshot) -> bool,
) -> RtcPeerSnapshot {
    let mut snapshots = handle.subscribe();
    timeout(WAIT, async move {
        loop {
            let snapshot = snapshots.borrow().clone();
            if predicate(&snapshot) {
                return snapshot;
            }
            snapshots.changed().await.expect("snapshot channel closed");
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {what}"))
}

#[tokio::test]
async fn authenticates_peer_generation_and_operation_without_peer_connection() {
    let mut h = ControllerTestHarness::spawn(700);
    h.handle.connect().await.expect("connect T1 controller");
    h.expect_role_negotiation().await;
    h.handle
        .apply_role_assignment(true)
        .await
        .expect("assign offerer role");

    let build = h.next_effect().await;
    let build_identity = build.identity.clone();
    assert_eq!(build_identity.peer_id, test_actor_id(700));
    assert_eq!(build_identity.pc_generation, 1);
    assert_eq!(build_identity.ice_generation, None);
    assert!(matches!(
        build.effect,
        RtcPeerEffect::BuildSession {
            generation: 1,
            operation_id
        } if operation_id == build_identity.operation_id
    ));
    let before_stale_completion = h.handle.snapshot();

    // Same peer/generation/kind, but a never-issued operation id: the envelope
    // is internally consistent, so rejection is proven at reducer identity.
    let wrong_operation_id = build_identity.operation_id + 100;
    h.effects
        .inject(
            RtcEffectIdentity {
                peer_id: build_identity.peer_id.clone(),
                pc_generation: 1,
                operation_id: wrong_operation_id,
                ice_generation: None,
            },
            EffectCompletion::BuildSession {
                generation: 1,
                operation_id: wrong_operation_id,
                result: Ok(()),
            },
        )
        .await
        .expect("inject stale completion");
    tokio::task::yield_now().await;
    assert_eq!(
        h.effects.pending_count(),
        1,
        "real operation remains pending"
    );
    assert_eq!(
        h.handle.snapshot(),
        before_stale_completion,
        "wrong operation completion must not publish a state change"
    );

    // The authentic completion remains accepted and advances to CreateOffer,
    // proving the malformed completion did not consume the pending identity.
    h.effects
        .complete(EffectCompletion::BuildSession {
            generation: 1,
            operation_id: build_identity.operation_id,
            result: Ok(()),
        })
        .await
        .expect("complete authentic build");
    let offer = h.next_effect().await;
    assert_eq!(offer.identity.peer_id, build_identity.peer_id);
    assert_eq!(offer.identity.pc_generation, 1);
    assert!(offer.identity.operation_id > build_identity.operation_id);
    assert!(matches!(offer.effect, RtcPeerEffect::CreateOffer { .. }));

    h.effects
        .complete(shutdown_completion(&offer))
        .await
        .expect("settle offer before teardown");
    h.shutdown().await;
}

#[tokio::test]
async fn manual_policy_clock_drives_exact_role_negotiation_deadline() {
    let mut h = ControllerTestHarness::spawn(701);
    h.handle.connect().await.expect("connect T1 controller");
    h.expect_role_negotiation().await;

    h.clock.advance(Duration::from_secs(4));
    tokio::task::yield_now().await;
    assert_ne!(
        h.handle.snapshot().status,
        actr_framework::WebRtcPeerStatus::Idle,
        "deadline must not fire early"
    );

    h.clock.advance(Duration::from_secs(1));
    let snapshot = await_snapshot(&h.handle, "role deadline", |snapshot| {
        snapshot.status == actr_framework::WebRtcPeerStatus::Idle
            && snapshot.unavailable_reason
                == Some(actr_framework::WebRtcUnavailableReason::NegotiationFailed)
    })
    .await;
    assert!(!snapshot.sendable);
    h.shutdown().await;
}

#[tokio::test]
async fn fact_injector_drives_semantic_state_without_raw_webrtc_objects() {
    let h = ControllerTestHarness::spawn(702);
    h.facts
        .driver(PeerDriverEvent::NegotiationNeeded { generation: 99 });
    h.facts
        .lane(LaneEvent::Opened {
            generation: 99,
            lane: crate::wire::webrtc::v020::status::LaneKind::RpcReliable,
        })
        .await;
    tokio::task::yield_now().await;
    assert_eq!(
        h.handle.snapshot(),
        RtcPeerSnapshot::idle(test_actor_id(702))
    );
    h.shutdown().await;
}

#[tokio::test]
async fn refresh_configuration_ack_waits_for_exact_authenticated_completion() {
    let mut h = ControllerTestHarness::spawn(703);
    drive_offerer_to_sendable(&mut h).await;

    let handle = h.handle.clone();
    let refresh = tokio::spawn(async move {
        handle
            .refresh_configuration(RTCConfiguration::default())
            .await
    });
    let effect = h.next_effect().await;
    let identity = effect.identity.clone();
    assert!(matches!(
        effect.effect,
        RtcPeerEffect::RefreshConfiguration { generation: 1, .. }
    ));
    assert!(
        !refresh.is_finished(),
        "enqueue is not a refresh acknowledgement"
    );

    let wrong_generation = identity.pc_generation + 1;
    h.effects
        .inject(
            RtcEffectIdentity {
                peer_id: identity.peer_id.clone(),
                pc_generation: wrong_generation,
                operation_id: identity.operation_id,
                ice_generation: None,
            },
            EffectCompletion::RefreshConfiguration {
                generation: wrong_generation,
                operation_id: identity.operation_id,
                result: Ok(()),
            },
        )
        .await
        .expect("inject wrong-generation refresh completion");
    tokio::task::yield_now().await;
    assert!(
        !refresh.is_finished(),
        "wrong generation must not consume the pending acknowledgement"
    );

    h.effects
        .complete(EffectCompletion::RefreshConfiguration {
            generation: identity.pc_generation,
            operation_id: identity.operation_id,
            result: Ok(()),
        })
        .await
        .expect("complete authentic refresh");
    assert_eq!(
        timeout(WAIT, refresh)
            .await
            .expect("refresh acknowledgement timeout")
            .expect("refresh task panicked"),
        Ok(())
    );
    h.shutdown().await;
}

#[tokio::test]
async fn refresh_configuration_reports_not_ready_and_rejection() {
    let idle = ControllerTestHarness::spawn(704);
    assert_eq!(
        idle.handle
            .refresh_configuration(RTCConfiguration::default())
            .await,
        Err(RtcConfigurationRefreshError::ConnectionNotReady)
    );
    idle.shutdown().await;

    let mut h = ControllerTestHarness::spawn(705);
    drive_offerer_to_sendable(&mut h).await;
    let handle = h.handle.clone();
    let refresh = tokio::spawn(async move {
        handle
            .refresh_configuration(RTCConfiguration::default())
            .await
    });
    let effect = h.next_effect().await;
    let identity = effect.identity.clone();
    assert!(matches!(
        effect.effect,
        RtcPeerEffect::RefreshConfiguration { .. }
    ));
    h.effects
        .complete(EffectCompletion::RefreshConfiguration {
            generation: identity.pc_generation,
            operation_id: identity.operation_id,
            result: Err(RtcError::Other("TURN refresh rejected".into())),
        })
        .await
        .expect("complete rejected refresh");
    let result = timeout(WAIT, refresh)
        .await
        .expect("rejected refresh acknowledgement timeout")
        .expect("refresh task panicked");
    assert!(matches!(
        result,
        Err(RtcConfigurationRefreshError::Rejected(message))
            if message.contains("TURN refresh rejected")
    ));

    let replacement = h.next_effect().await;
    assert!(matches!(
        replacement.effect,
        RtcPeerEffect::BuildSession { generation: 2, .. }
    ));
    h.effects
        .complete(shutdown_completion(&replacement))
        .await
        .expect("settle replacement build before teardown");
    h.shutdown().await;
}

#[tokio::test]
async fn manual_policy_clock_enforces_one_non_extendable_total_recovery_budget() {
    let mut h = ControllerTestHarness::spawn(706);
    drive_offerer_to_sendable(&mut h).await;

    h.supervisor.publish(RtcNetworkEvent::NetworkUnavailable);
    await_snapshot(&h.handle, "T1 recovery entry", |snapshot| {
        snapshot.status == actr_framework::WebRtcPeerStatus::Recovering && !snapshot.sendable
    })
    .await;

    h.clock
        .advance(NETWORK_RECOVERY_TIMEOUT - Duration::from_millis(1));
    tokio::task::yield_now().await;
    assert_eq!(
        h.handle.snapshot().status,
        actr_framework::WebRtcPeerStatus::Recovering,
        "total recovery budget must not fire early"
    );

    h.clock.advance(Duration::from_millis(1));
    let exhausted = await_snapshot(&h.handle, "T1 total recovery exhaustion", |snapshot| {
        snapshot.status == actr_framework::WebRtcPeerStatus::Idle
            && snapshot.unavailable_reason
                == Some(actr_framework::WebRtcUnavailableReason::RecoveryExhausted)
    })
    .await;
    assert!(!exhausted.sendable);
    assert_eq!(exhausted.transport_epoch, 1);

    let retirement = timeout(WAIT, h.retirement_rx.recv())
        .await
        .expect("terminal transport-retirement timeout")
        .expect("terminal transport-retirement channel closed");
    assert_eq!(retirement.lifecycle.peer_id, test_actor_id(706));
    assert_eq!(retirement.lifecycle.controller_lifecycle_id, 1);
    assert_eq!(retirement.lifecycle.transport_epoch, 1);
    assert_eq!(
        retirement.reason,
        actr_framework::WebRtcUnavailableReason::RecoveryExhausted
    );

    let close = h.next_effect().await;
    assert!(matches!(
        close.effect,
        RtcPeerEffect::CloseSession { generation: 1, .. }
    ));
    h.effects
        .complete(shutdown_completion(&close))
        .await
        .expect("complete exhausted-session close");
    h.shutdown().await;
}

#[tokio::test]
async fn fresh_remote_offer_cancels_the_previous_recovery_budget() {
    let mut h = ControllerTestHarness::spawn(707);
    drive_answerer_to_sendable(&mut h).await;

    h.supervisor.publish(RtcNetworkEvent::NetworkUnavailable);
    await_snapshot(&h.handle, "T1 answerer recovery entry", |snapshot| {
        snapshot.status == actr_framework::WebRtcPeerStatus::Recovering && !snapshot.sendable
    })
    .await;
    h.supervisor.publish(RtcNetworkEvent::NetworkAvailable);
    // Request-only answerer recovery: the restart request goes straight to
    // signaling; the local restart happens implicitly when the remote restart
    // offer is applied, so no RestartIce effect is dispatched here.
    let (_, recovery_message) = timeout(WAIT, h.signaling_rx.recv())
        .await
        .expect("T1 restart request timeout")
        .expect("T1 signaling channel closed");
    assert!(matches!(
        recovery_message,
        SignalingMessage::IceRestartRequest { .. }
    ));

    h.handle
        .apply_remote_description(desc_with_ufrag(RTCSdpType::Offer, "remote-fresh"))
        .await
        .expect("apply T1 fresh remote offer");
    let close = h.next_effect().await;
    let replacement = h.next_effect().await;
    assert!(matches!(
        close.effect,
        RtcPeerEffect::CloseSession { generation: 1, .. }
    ));
    assert!(matches!(
        replacement.effect,
        RtcPeerEffect::BuildSession { generation: 2, .. }
    ));
    await_snapshot(&h.handle, "T1 fresh transport attempt", |snapshot| {
        snapshot.transport_epoch == 2
            && snapshot.status == actr_framework::WebRtcPeerStatus::Connecting
    })
    .await;

    h.clock.advance(NETWORK_RECOVERY_TIMEOUT);
    tokio::task::yield_now().await;
    let snapshot = h.handle.snapshot();
    assert_eq!(snapshot.transport_epoch, 2);
    assert_eq!(
        snapshot.status,
        actr_framework::WebRtcPeerStatus::Connecting,
        "the old recovery timer must not terminate the fresh Offer flight"
    );
    assert!(matches!(
        h.retirement_rx.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));

    h.effects
        .complete(EffectCompletion::CloseSession {
            generation: close.identity.pc_generation,
            operation_id: close.identity.operation_id,
            result: Ok(()),
        })
        .await
        .expect("complete T1 old-session close");
    h.effects
        .complete(EffectCompletion::BuildSession {
            generation: replacement.identity.pc_generation,
            operation_id: replacement.identity.operation_id,
            result: Err(RtcError::Other("settle T1 replacement".into())),
        })
        .await
        .expect("settle T1 replacement build");
    h.shutdown().await;
}

#[tokio::test]
async fn failed_hard_replacement_retries_on_the_attempt_deadline() {
    let mut h = ControllerTestHarness::spawn(708);
    drive_offerer_to_sendable(&mut h).await;

    h.supervisor.publish(RtcNetworkEvent::LocalAddressChanged);
    let failed_replacement = h.next_effect().await;
    assert!(matches!(
        failed_replacement.effect,
        RtcPeerEffect::BuildSession { generation: 2, .. }
    ));
    h.effects
        .complete(EffectCompletion::BuildSession {
            generation: failed_replacement.identity.pc_generation,
            operation_id: failed_replacement.identity.operation_id,
            result: Err(RtcError::Other("transient replacement failure".into())),
        })
        .await
        .expect("complete failed hard replacement");

    let close = h.next_effect().await;
    assert!(matches!(
        close.effect,
        RtcPeerEffect::CloseSession { generation: 1, .. }
    ));
    h.effects
        .complete(EffectCompletion::CloseSession {
            generation: close.identity.pc_generation,
            operation_id: close.identity.operation_id,
            result: Ok(()),
        })
        .await
        .expect("complete retired active session close");
    await_snapshot(&h.handle, "T1 replacement retry wait", |snapshot| {
        snapshot.status == actr_framework::WebRtcPeerStatus::Recovering
            && snapshot.pc_generation == 0
    })
    .await;

    h.clock.advance(Duration::from_secs(6));
    let retry = h.next_effect().await;
    assert!(matches!(
        retry.effect,
        RtcPeerEffect::BuildSession { generation: 3, .. }
    ));
    h.effects
        .complete(EffectCompletion::BuildSession {
            generation: retry.identity.pc_generation,
            operation_id: retry.identity.operation_id,
            result: Err(RtcError::Other("settle replacement retry".into())),
        })
        .await
        .expect("settle replacement retry");
    h.shutdown().await;
}

#[tokio::test]
async fn hung_hard_replacement_negotiation_is_retired_and_retried() {
    let mut h = ControllerTestHarness::spawn(709);
    drive_offerer_to_sendable(&mut h).await;

    h.supervisor.publish(RtcNetworkEvent::LocalAddressChanged);
    let build = h.next_effect().await;
    assert!(matches!(
        build.effect,
        RtcPeerEffect::BuildSession { generation: 2, .. }
    ));
    h.effects
        .complete(EffectCompletion::BuildSession {
            generation: build.identity.pc_generation,
            operation_id: build.identity.operation_id,
            result: Ok(()),
        })
        .await
        .expect("complete hard replacement build");
    let hung_offer = h.next_effect().await;
    assert!(matches!(
        hung_offer.effect,
        RtcPeerEffect::CreateOffer { generation: 2, .. }
    ));

    h.clock.advance(Duration::from_secs(6));
    let close = h.next_effect().await;
    let retry = h.next_effect().await;
    assert!(matches!(
        close.effect,
        RtcPeerEffect::CloseSession { generation: 2, .. }
    ));
    assert!(matches!(
        retry.effect,
        RtcPeerEffect::BuildSession { generation: 3, .. }
    ));

    h.effects
        .complete(shutdown_completion(&hung_offer))
        .await
        .expect("complete stale replacement offer");
    h.effects
        .complete(EffectCompletion::CloseSession {
            generation: close.identity.pc_generation,
            operation_id: close.identity.operation_id,
            result: Ok(()),
        })
        .await
        .expect("complete failed replacement close");
    h.effects
        .complete(EffectCompletion::BuildSession {
            generation: retry.identity.pc_generation,
            operation_id: retry.identity.operation_id,
            result: Err(RtcError::Other("settle hard retry".into())),
        })
        .await
        .expect("settle hard retry");
    h.shutdown().await;
}
