//! `V020SignalingTransport` — adapts the production 0.14 `WebSocketSignalingClient`
//! to the v0.20 [`SignalingTransport`] trait.
//!
//! The v0.20 `SignalingRouter` sends/receives [`SignalingMessage`]s; this
//! adapter converts them to/from the protobuf `ActrRelay` envelope format the
//! signaling server speaks, preserving wire compatibility with the 0.14 stack.
//!
//! Conversion table (v0.20 → ActrRelay payload):
//! * `Offer { desc, exchange_id }` → correlated `SessionDescription { type: OFFER, sdp }`
//! * `Answer { desc, exchange_id }` → correlated `SessionDescription { type: ANSWER, sdp }`
//! * `Candidate(init)` → `IceCandidate { candidate, sdp_mid, ... }`
//! * `RoleNegotiation` → `RoleNegotiation { from, to, realm_id }`
//! * `RoleAssignment { is_offerer }` → `RoleAssignment { is_offerer, .. }`
//!
//! The reverse mapping applies on receive. Non-`ActrRelay` envelopes (heartbeat,
//! registration, route candidates) are not forwarded to the v0.20 router — they
//! belong to the 0.14 signaling client's own lifecycle.

use std::sync::Arc;

use actr_protocol::{
    ActrId, RelayDeliveryContext, RelayDeliveryStatus, SIGNALING_ENVELOPE_VERSION,
    SignalingEnvelope, actr_relay, session_description, signaling_envelope, signaling_to_actr,
};
use async_trait::async_trait;

use webrtc::peer_connection::{RTCIceCandidateInit, RTCSdpType, RTCSessionDescription};

use crate::lifecycle::CredentialState;
use crate::wire::webrtc::signaling::SignalingClient;
use crate::wire::webrtc::v020::peer::reducer::SignalingMessage;

use super::router::SignalingTransport;
use super::{RelayAttemptIdentity, SharedLocalId, SignalingDispatch, SignalingInbound};

/// Production [`SignalingTransport`] that sends/receives v0.20 signaling
/// messages over the 0.14 `WebSocketSignalingClient` WebSocket.
///
/// The signaling `receive_envelope` is single-consumer, so the 0.14 and v0.20
/// stacks cannot run simultaneously — the node-level exclusive switch (W5)
/// ensures only one backend is constructed. The wire format (`ActrRelay`
/// protobuf) is version-agnostic: the SDP string and candidate string are
/// opaque to the transport, so v0.20 messages reuse the same envelope.
pub(crate) struct V020SignalingTransport {
    signaling_client: Arc<dyn SignalingClient>,
    /// Shared local identity, updated by hard rebind (CF-1); every outbound
    /// envelope stamps the *current* value.
    local_id: SharedLocalId,
    credential_state: CredentialState,
}

impl V020SignalingTransport {
    pub(crate) fn new(
        signaling_client: Arc<dyn SignalingClient>,
        local_id: SharedLocalId,
        credential_state: CredentialState,
    ) -> Self {
        Self {
            signaling_client,
            local_id,
            credential_state,
        }
    }

    /// Convert a v0.20 `SignalingMessage` to an `actr_relay::Payload` for
    /// delivery to `to`.
    fn message_to_payload(&self, to: &ActrId, message: &SignalingMessage) -> actr_relay::Payload {
        match message {
            SignalingMessage::Offer { desc, exchange_id } => {
                actr_relay::Payload::SessionDescription(actr_protocol::SessionDescription {
                    r#type: session_description::Type::Offer as i32,
                    sdp: desc.sdp.clone(),
                    sdp_exchange_id: exchange_id.clone(),
                })
            }
            SignalingMessage::IceRestartOffer { desc, exchange_id } => {
                actr_relay::Payload::SessionDescription(actr_protocol::SessionDescription {
                    r#type: session_description::Type::IceRestartOffer as i32,
                    sdp: desc.sdp.clone(),
                    sdp_exchange_id: Some(exchange_id.clone()),
                })
            }
            SignalingMessage::IceRestartRequest {
                request_id,
                attempt,
            } => actr_relay::Payload::IceRestartRequest(actr_protocol::IceRestartRequest {
                reason: Some("v020_recovery".to_string()),
                request_id: Some(request_id.clone()),
                attempt: Some(*attempt),
            }),
            SignalingMessage::Answer { desc, exchange_id } => {
                actr_relay::Payload::SessionDescription(actr_protocol::SessionDescription {
                    r#type: session_description::Type::Answer as i32,
                    sdp: desc.sdp.clone(),
                    sdp_exchange_id: exchange_id.clone(),
                })
            }
            SignalingMessage::Candidate(init) => {
                actr_relay::Payload::IceCandidate(actr_protocol::IceCandidate {
                    candidate: init.candidate.clone(),
                    sdp_mid: init.sdp_mid.clone(),
                    sdp_mline_index: init.sdp_mline_index.map(|i| i as u32),
                    username_fragment: init.username_fragment.clone(),
                })
            }
            SignalingMessage::RoleNegotiation => {
                let local_id = self.local_id.get();
                actr_relay::Payload::RoleNegotiation(actr_protocol::RoleNegotiation {
                    realm_id: local_id.realm.realm_id,
                    from: local_id,
                    to: to.clone(),
                })
            }
            SignalingMessage::RoleAssignment { is_offerer } => {
                actr_relay::Payload::RoleAssignment(actr_protocol::RoleAssignment {
                    is_offerer: *is_offerer,
                    remote_fixed: None,
                })
            }
            SignalingMessage::IceRestartRejected {
                exchange_id,
                reason,
            } => actr_relay::Payload::IceRestartRejected(actr_protocol::IceRestartRejected {
                sdp_exchange_id: exchange_id.clone(),
                reason: Some(*reason),
            }),
        }
    }

    /// Convert an inbound `actr_relay::Payload` to a v0.20 `SignalingMessage`.
    /// Returns `None` for payloads outside the v0.20 router contract.
    fn payload_to_message(payload: actr_relay::Payload) -> Option<SignalingMessage> {
        match payload {
            actr_relay::Payload::SessionDescription(sd) => {
                let wire_type = sd.r#type();
                let sdp_type = match wire_type {
                    session_description::Type::Offer
                    | session_description::Type::RenegotiationOffer
                    | session_description::Type::IceRestartOffer => RTCSdpType::Offer,
                    session_description::Type::Answer => RTCSdpType::Answer,
                };
                let mut desc = RTCSessionDescription::default();
                desc.sdp_type = sdp_type;
                desc.sdp = sd.sdp;
                Some(if wire_type == session_description::Type::IceRestartOffer {
                    SignalingMessage::IceRestartOffer {
                        desc: Box::new(desc),
                        exchange_id: sd.sdp_exchange_id.unwrap_or_default(),
                    }
                } else {
                    match sdp_type {
                        RTCSdpType::Offer => SignalingMessage::Offer {
                            desc: Box::new(desc),
                            exchange_id: sd.sdp_exchange_id,
                        },
                        _ => SignalingMessage::Answer {
                            desc: Box::new(desc),
                            exchange_id: sd.sdp_exchange_id,
                        },
                    }
                })
            }
            actr_relay::Payload::IceCandidate(ice) => {
                Some(SignalingMessage::Candidate(RTCIceCandidateInit {
                    candidate: ice.candidate,
                    sdp_mid: ice.sdp_mid,
                    sdp_mline_index: ice.sdp_mline_index.map(|i| i as u16),
                    username_fragment: ice.username_fragment,
                    url: None,
                }))
            }
            actr_relay::Payload::RoleNegotiation(_) => Some(SignalingMessage::RoleNegotiation),
            actr_relay::Payload::RoleAssignment(assign) => Some(SignalingMessage::RoleAssignment {
                is_offerer: assign.is_offerer,
            }),
            actr_relay::Payload::IceRestartRequest(request) => {
                Some(SignalingMessage::IceRestartRequest {
                    request_id: request.request_id.unwrap_or_default(),
                    attempt: request.attempt.unwrap_or(1),
                })
            }
            actr_relay::Payload::IceRestartRejected(rejection) => {
                Some(SignalingMessage::IceRestartRejected {
                    exchange_id: rejection.sdp_exchange_id,
                    reason: rejection.reason.unwrap_or_default(),
                })
            }
            _ => None,
        }
    }
}

#[async_trait]
impl SignalingTransport for V020SignalingTransport {
    async fn send(&self, to: ActrId, message: SignalingMessage) {
        self.send_dispatch(SignalingDispatch::untracked(to, message))
            .await;
    }

    async fn send_dispatch(&self, dispatch: SignalingDispatch) {
        let request_delivery_result = dispatch.requests_delivery_result();
        let SignalingDispatch {
            peer: to,
            message,
            attempt,
        } = dispatch;
        let target_repr = to.to_string_repr();
        let payload = self.message_to_payload(&to, &message);
        let credential = self.credential_state.credential().await;
        let relay = actr_protocol::ActrRelay {
            source: self.local_id.get(),
            credential,
            target: to,
            delivery_context: request_delivery_result.then(|| {
                let attempt = attempt.expect("delivery-result request has an RTC attempt identity");
                RelayDeliveryContext {
                    controller_lifecycle_id: attempt.controller_lifecycle_id,
                    transport_epoch: attempt.transport_epoch,
                    pc_generation: attempt.pc_generation,
                    negotiation_epoch: Some(attempt.negotiation_epoch),
                }
            }),
            payload: Some(payload),
        };
        let envelope = SignalingEnvelope {
            envelope_version: SIGNALING_ENVELOPE_VERSION,
            envelope_id: uuid::Uuid::new_v4().to_string(),
            reply_for: None,
            traceparent: None,
            tracestate: None,
            flow: Some(signaling_envelope::Flow::ActrRelay(relay)),
        };
        if let Err(e) = self.signaling_client.send_envelope(envelope).await {
            tracing::warn!(
                target = "v020-signaling",
                peer = %target_repr,
                error = %e,
                "failed to send v0.20 signaling envelope"
            );
        }
    }

    async fn recv(&self) -> Option<(ActrId, SignalingMessage)> {
        match self.recv_event().await {
            Some(SignalingInbound::PeerMessage { from, message }) => Some((from, message)),
            Some(SignalingInbound::TargetUnavailable { .. }) | None => None,
        }
    }

    async fn recv_event(&self) -> Option<SignalingInbound> {
        // The inbound queue can close or error while a replacement queue is
        // being installed (`WebSocketSignalingClient::disconnect()` swaps the
        // bounded inbound queue). Yielding alone spun the loop hot on that
        // edge; a bounded exponential backoff (50ms doubling to a 1s cap,
        // reset by any received envelope) keeps the sole consumer alive
        // across signaling reconnects without burning a core.
        let mut retry_backoff = std::time::Duration::from_millis(50);
        loop {
            let envelope = match self.signaling_client.receive_envelope().await {
                Ok(Some(envelope)) => envelope,
                Ok(None) | Err(_) => {
                    // `WebSocketSignalingClient::disconnect()` replaces its
                    // bounded inbound queue. The sole consumer first observes
                    // the old queue closing, then must attach to the new queue
                    // and remain alive across signaling reconnects. Treating
                    // this edge as end-of-stream permanently killed the v0.20
                    // SignalingRouter after every mobile cleanup.
                    tracing::debug!(
                        target = "v020-signaling",
                        backoff_ms = retry_backoff.as_millis() as u64,
                        "signaling inbound generation ended; backing off before retrying receive"
                    );
                    crate::timer::sleep(
                        crate::timer::ids::SIGNALING_INBOUND_RETRY_BACKOFF,
                        retry_backoff,
                    )
                    .await;
                    retry_backoff = (retry_backoff * 2).min(std::time::Duration::from_secs(1));
                    continue;
                }
            };
            retry_backoff = std::time::Duration::from_millis(50);
            let Some(flow) = envelope.flow else {
                tracing::warn!(
                    target = "v020-signaling",
                    "dropping signaling envelope without a flow"
                );
                continue;
            };
            match flow {
                signaling_envelope::Flow::ActrRelay(relay) => {
                    let source = relay.source.clone();
                    let target = relay.target.clone();
                    if let Some(payload) = relay.payload
                        && let Some(message) = Self::payload_to_message(payload)
                    {
                        let peer = if matches!(&message, SignalingMessage::RoleAssignment { .. })
                            && source == self.local_id.get()
                        {
                            target
                        } else {
                            source
                        };
                        return Some(SignalingInbound::PeerMessage {
                            from: peer,
                            message,
                        });
                    }
                }
                signaling_envelope::Flow::ServerToActr(server_to_actr) => {
                    if let Some(signaling_to_actr::Payload::RelayDeliveryResult(result)) =
                        server_to_actr.payload
                    {
                        let status = match RelayDeliveryStatus::try_from(result.status) {
                            Ok(status) => status,
                            Err(unknown) => {
                                // An unknown future status must not read as a
                                // confirmed target-unavailable fact, and must
                                // be loud enough to diagnose a protocol skew.
                                tracing::warn!(
                                    target = "v020-signaling",
                                    peer = %result.target.to_string_repr(),
                                    raw_status = %unknown,
                                    "unknown relay delivery status from Actrix; treating as rejected"
                                );
                                RelayDeliveryStatus::RelayDeliveryRejected
                            }
                        };
                        match status {
                            RelayDeliveryStatus::RelayDeliveryTargetUnavailable => {
                                return Some(SignalingInbound::TargetUnavailable {
                                    target: result.target,
                                    attempt: RelayAttemptIdentity {
                                        controller_lifecycle_id: result
                                            .context
                                            .controller_lifecycle_id,
                                        transport_epoch: result.context.transport_epoch,
                                        pc_generation: result.context.pc_generation,
                                        // A pre-correlation sender omits the
                                        // epoch; zero never matches a live
                                        // gen-0 fence (every probe emission
                                        // bumps the epoch past zero first).
                                        negotiation_epoch: result
                                            .context
                                            .negotiation_epoch
                                            .unwrap_or(0),
                                    },
                                });
                            }
                            RelayDeliveryStatus::RelayDeliveryRejected => tracing::warn!(
                                target = "v020-signaling",
                                peer = %result.target.to_string_repr(),
                                "Actrix rejected RTC signaling relay without declaring the target unavailable"
                            ),
                            RelayDeliveryStatus::RelayDeliveryAccepted => {}
                        }
                    }
                }
                _ => {}
            }
            // Non-ActrRelay envelopes (heartbeat, registration, etc.) are
            // consumed by the 0.14 stack; skip them for the v0.20 router.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    fn test_id(serial: u64) -> ActrId {
        ActrId {
            realm: actr_protocol::Realm { realm_id: 1 },
            serial_number: serial,
            r#type: actr_protocol::ActrType {
                manufacturer: "acme".to_string(),
                name: "node".to_string(),
                version: "1.0.0".to_string(),
            },
        }
    }

    /// Build a transport without a real signaling client (for conversion-only
    /// tests). The client is never called.
    fn conversion_only_transport() -> V020SignalingTransport {
        // Use a disconnected client; conversion methods don't touch it.
        let client: Arc<dyn SignalingClient> = Arc::new(ScriptedReceiveClient::default());
        let credential = actr_protocol::AIdCredential {
            key_id: 0,
            claims: bytes::Bytes::new(),
            signature: bytes::Bytes::new(),
        };
        V020SignalingTransport::new(
            client,
            SharedLocalId::new(test_id(1)),
            CredentialState::new(credential, None, None),
        )
    }

    /// Minimal signaling client whose receive results model successive inbound
    /// queue generations. An outer `Some` is one scripted call; its inner value
    /// is the `receive_envelope` result (`None` means the old queue closed).
    #[derive(Default)]
    struct ScriptedReceiveClient {
        receives: Mutex<VecDeque<Option<SignalingEnvelope>>>,
        sent: Mutex<Vec<SignalingEnvelope>>,
    }

    #[async_trait]
    impl SignalingClient for ScriptedReceiveClient {
        async fn connect(&self) -> crate::transport::NetworkResult<()> {
            Ok(())
        }
        async fn disconnect(&self) -> crate::transport::NetworkResult<()> {
            Ok(())
        }
        async fn send_register_request(
            &self,
            _: actr_protocol::RegisterRequest,
        ) -> crate::transport::NetworkResult<actr_protocol::RegisterResponse> {
            unimplemented!()
        }
        async fn send_unregister_request(
            &self,
            _: ActrId,
            _: actr_protocol::AIdCredential,
            _: Option<String>,
        ) -> crate::transport::NetworkResult<actr_protocol::UnregisterResponse> {
            unimplemented!()
        }
        async fn send_heartbeat(
            &self,
            _: ActrId,
            _: actr_protocol::AIdCredential,
            _: actr_protocol::ServiceAvailabilityState,
            _: f32,
            _: f32,
        ) -> crate::transport::NetworkResult<actr_protocol::Pong> {
            unimplemented!()
        }
        async fn send_route_candidates_request(
            &self,
            _: ActrId,
            _: actr_protocol::AIdCredential,
            _: actr_protocol::RouteCandidatesRequest,
        ) -> crate::transport::NetworkResult<actr_protocol::RouteCandidatesResponse> {
            unimplemented!()
        }
        async fn get_signing_key(
            &self,
            _: ActrId,
            _: actr_protocol::AIdCredential,
            _: u32,
        ) -> crate::transport::NetworkResult<(u32, Vec<u8>)> {
            unimplemented!()
        }
        async fn send_envelope(
            &self,
            envelope: SignalingEnvelope,
        ) -> crate::transport::NetworkResult<()> {
            self.sent.lock().expect("scripted send lock").push(envelope);
            Ok(())
        }
        async fn receive_envelope(
            &self,
        ) -> crate::transport::NetworkResult<Option<SignalingEnvelope>> {
            let scripted = self
                .receives
                .lock()
                .expect("scripted receive lock")
                .pop_front();
            match scripted {
                Some(result) => Ok(result),
                None => std::future::pending().await,
            }
        }
        fn is_connected(&self) -> bool {
            false
        }
        fn get_stats(&self) -> crate::wire::webrtc::signaling::SignalingStats {
            crate::wire::webrtc::signaling::SignalingStats::default()
        }
        fn subscribe_events(
            &self,
        ) -> tokio::sync::broadcast::Receiver<crate::wire::webrtc::signaling::SignalingEvent>
        {
            let (tx, rx) = tokio::sync::broadcast::channel(1);
            drop(tx);
            rx
        }
        async fn set_actor_id(&self, _: ActrId) {}
        async fn set_credential_state(&self, _: CredentialState) {}
        async fn clear_identity(&self) {}
    }

    #[tokio::test]
    async fn receive_survives_inbound_queue_replacement() {
        let local = test_id(1);
        let remote = test_id(2);
        let envelope = SignalingEnvelope {
            envelope_version: SIGNALING_ENVELOPE_VERSION,
            envelope_id: "after-reconnect".to_string(),
            reply_for: None,
            traceparent: None,
            tracestate: None,
            flow: Some(signaling_envelope::Flow::ActrRelay(
                actr_protocol::ActrRelay {
                    source: remote.clone(),
                    credential: actr_protocol::AIdCredential {
                        key_id: 0,
                        claims: bytes::Bytes::new(),
                        signature: bytes::Bytes::new(),
                    },
                    target: local.clone(),
                    delivery_context: None,
                    payload: Some(actr_relay::Payload::SessionDescription(
                        actr_protocol::SessionDescription {
                            r#type: session_description::Type::Offer as i32,
                            sdp: "v=0\r\n...".to_string(),
                            sdp_exchange_id: None,
                        },
                    )),
                },
            )),
        };
        let client: Arc<dyn SignalingClient> = Arc::new(ScriptedReceiveClient {
            receives: Mutex::new(VecDeque::from([None, Some(envelope)])),
            sent: Mutex::new(Vec::new()),
        });
        let transport = V020SignalingTransport::new(
            client,
            SharedLocalId::new(local),
            CredentialState::new(
                actr_protocol::AIdCredential {
                    key_id: 0,
                    claims: bytes::Bytes::new(),
                    signature: bytes::Bytes::new(),
                },
                None,
                None,
            ),
        );

        let (peer, message) = crate::timer::timeout(
            crate::timer::ids::SIGNALING_CONNECTED_WAIT,
            std::time::Duration::from_secs(1),
            SignalingTransport::recv(&transport),
        )
        .await
        .expect("transport stopped after the old inbound queue closed")
        .expect("replacement inbound queue did not produce a message");

        assert_eq!(peer, remote);
        assert!(matches!(message, SignalingMessage::Offer { .. }));
    }

    #[tokio::test]
    async fn critical_relay_carries_attempt_identity() {
        let client = Arc::new(ScriptedReceiveClient::default());
        let transport = V020SignalingTransport::new(
            client.clone(),
            SharedLocalId::new(test_id(1)),
            CredentialState::new(actr_protocol::AIdCredential::default(), None, None),
        );
        let dispatch = SignalingDispatch::tracked(
            test_id(2),
            7,
            11,
            13,
            29,
            SignalingMessage::RoleNegotiation,
        );

        SignalingTransport::send_dispatch(&transport, dispatch).await;

        let sent = client.sent.lock().expect("scripted send lock");
        let relay = match sent[0].flow.as_ref() {
            Some(signaling_envelope::Flow::ActrRelay(relay)) => relay,
            other => panic!("expected ActrRelay, got {other:?}"),
        };
        let context = relay
            .delivery_context
            .as_ref()
            .expect("critical relay requests a delivery result");
        assert_eq!(context.controller_lifecycle_id, 7);
        assert_eq!(context.transport_epoch, 11);
        assert_eq!(context.pc_generation, 13);
        assert_eq!(context.negotiation_epoch, Some(29));
    }

    #[tokio::test]
    async fn target_unavailable_result_preserves_attempt_identity() {
        let local = test_id(1);
        let target = test_id(2);
        let envelope = SignalingEnvelope {
            envelope_version: SIGNALING_ENVELOPE_VERSION,
            envelope_id: "target-unavailable".to_string(),
            reply_for: Some("critical-relay".to_string()),
            traceparent: None,
            tracestate: None,
            flow: Some(signaling_envelope::Flow::ServerToActr(
                actr_protocol::SignalingToActr {
                    target: local.clone(),
                    payload: Some(signaling_to_actr::Payload::RelayDeliveryResult(
                        actr_protocol::RelayDeliveryResult {
                            target: target.clone(),
                            context: RelayDeliveryContext {
                                controller_lifecycle_id: 17,
                                transport_epoch: 19,
                                pc_generation: 23,
                                negotiation_epoch: Some(29),
                            },
                            status: RelayDeliveryStatus::RelayDeliveryTargetUnavailable as i32,
                        },
                    )),
                },
            )),
        };
        let client: Arc<dyn SignalingClient> = Arc::new(ScriptedReceiveClient {
            receives: Mutex::new(VecDeque::from([Some(envelope)])),
            sent: Mutex::new(Vec::new()),
        });
        let transport = V020SignalingTransport::new(
            client,
            SharedLocalId::new(local),
            CredentialState::new(actr_protocol::AIdCredential::default(), None, None),
        );

        let event = SignalingTransport::recv_event(&transport)
            .await
            .expect("target-unavailable event");
        match event {
            SignalingInbound::TargetUnavailable {
                target: actual,
                attempt,
            } => {
                assert_eq!(actual, target);
                assert_eq!(attempt.controller_lifecycle_id, 17);
                assert_eq!(attempt.transport_epoch, 19);
                assert_eq!(attempt.pc_generation, 23);
                assert_eq!(attempt.negotiation_epoch, 29);
            }
            other => panic!("expected target unavailable, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn unknown_relay_delivery_status_falls_back_to_rejection_without_stranding_the_loop() {
        let local = test_id(1);
        let target = test_id(2);
        let envelope = |status: i32, envelope_id: &str| SignalingEnvelope {
            envelope_version: SIGNALING_ENVELOPE_VERSION,
            envelope_id: envelope_id.to_string(),
            reply_for: None,
            traceparent: None,
            tracestate: None,
            flow: Some(signaling_envelope::Flow::ServerToActr(
                actr_protocol::SignalingToActr {
                    target: local.clone(),
                    payload: Some(signaling_to_actr::Payload::RelayDeliveryResult(
                        actr_protocol::RelayDeliveryResult {
                            target: target.clone(),
                            context: RelayDeliveryContext {
                                controller_lifecycle_id: 17,
                                transport_epoch: 19,
                                pc_generation: 23,
                                negotiation_epoch: Some(29),
                            },
                            status,
                        },
                    )),
                },
            )),
        };
        // The unknown status must be swallowed as an ordinary rejection
        // (never a confirmed unavailability), leaving the loop to serve the
        // next result — here a real target-unavailable fact.
        let client: Arc<dyn SignalingClient> = Arc::new(ScriptedReceiveClient {
            receives: Mutex::new(VecDeque::from([
                Some(envelope(99, "unknown-status")),
                Some(envelope(
                    RelayDeliveryStatus::RelayDeliveryTargetUnavailable as i32,
                    "target-unavailable",
                )),
            ])),
            sent: Mutex::new(Vec::new()),
        });
        let transport = V020SignalingTransport::new(
            client,
            SharedLocalId::new(local),
            CredentialState::new(actr_protocol::AIdCredential::default(), None, None),
        );

        let event = SignalingTransport::recv_event(&transport)
            .await
            .expect("subsequent target-unavailable event");
        match event {
            SignalingInbound::TargetUnavailable { target: actual, .. } => {
                assert_eq!(actual, target);
            }
            other => panic!("expected target unavailable, got {other:?}"),
        }
    }

    #[test]
    fn offer_maps_to_session_description_offer() {
        let t = conversion_only_transport();
        let mut desc = RTCSessionDescription::default();
        desc.sdp_type = RTCSdpType::Offer;
        desc.sdp = "v=0\r\n...".to_string();
        let msg = SignalingMessage::Offer {
            desc: Box::new(desc),
            exchange_id: Some("offer-exchange".to_string()),
        };
        let payload = t.message_to_payload(&test_id(2), &msg);
        match payload {
            actr_relay::Payload::SessionDescription(sd) => {
                assert_eq!(sd.r#type(), session_description::Type::Offer);
                assert_eq!(sd.sdp, "v=0\r\n...");
                assert_eq!(sd.sdp_exchange_id.as_deref(), Some("offer-exchange"));
            }
            other => panic!("expected SessionDescription, got {other:?}"),
        }
    }

    #[test]
    fn answer_maps_to_session_description_answer() {
        let t = conversion_only_transport();
        let mut desc = RTCSessionDescription::default();
        desc.sdp_type = RTCSdpType::Answer;
        desc.sdp = "v=0\r\n...".to_string();
        let msg = SignalingMessage::Answer {
            desc: Box::new(desc),
            exchange_id: Some("answer-exchange".to_string()),
        };
        let payload = t.message_to_payload(&test_id(2), &msg);
        match payload {
            actr_relay::Payload::SessionDescription(sd) => {
                assert_eq!(sd.r#type(), session_description::Type::Answer);
                assert_eq!(sd.sdp_exchange_id.as_deref(), Some("answer-exchange"));
            }
            other => panic!("expected SessionDescription, got {other:?}"),
        }
    }

    #[test]
    fn candidate_maps_to_ice_candidate() {
        let t = conversion_only_transport();
        let init = RTCIceCandidateInit {
            candidate: "candidate:1 ...".to_string(),
            sdp_mid: Some("0".to_string()),
            sdp_mline_index: Some(0),
            username_fragment: Some("ufrag".to_string()),
            url: None,
        };
        let msg = SignalingMessage::Candidate(init);
        let payload = t.message_to_payload(&test_id(2), &msg);
        match payload {
            actr_relay::Payload::IceCandidate(ice) => {
                assert_eq!(ice.candidate, "candidate:1 ...");
                assert_eq!(ice.sdp_mid, Some("0".to_string()));
                assert_eq!(ice.username_fragment, Some("ufrag".to_string()));
            }
            other => panic!("expected IceCandidate, got {other:?}"),
        }
    }

    #[test]
    fn role_negotiation_carries_from_and_to() {
        let t = conversion_only_transport();
        let msg = SignalingMessage::RoleNegotiation;
        let payload = t.message_to_payload(&test_id(2), &msg);
        match payload {
            actr_relay::Payload::RoleNegotiation(rn) => {
                assert_eq!(rn.from, test_id(1));
                assert_eq!(rn.to, test_id(2));
            }
            other => panic!("expected RoleNegotiation, got {other:?}"),
        }
    }

    /// CF-1: after a hard rebind adopts a fresh ActrId, outbound envelopes
    /// must stamp the new identity, not the construction-time one.
    #[test]
    fn role_negotiation_stamps_current_identity_after_rebind() {
        let t = conversion_only_transport();
        t.local_id.set(test_id(99));
        let msg = SignalingMessage::RoleNegotiation;
        let payload = t.message_to_payload(&test_id(2), &msg);
        match payload {
            actr_relay::Payload::RoleNegotiation(rn) => {
                assert_eq!(rn.from, test_id(99), "stamps the post-rebind identity");
                assert_eq!(rn.to, test_id(2));
            }
            other => panic!("expected RoleNegotiation, got {other:?}"),
        }
    }

    #[test]
    fn role_assignment_round_trips() {
        let t = conversion_only_transport();
        let msg = SignalingMessage::RoleAssignment { is_offerer: true };
        let payload = t.message_to_payload(&test_id(2), &msg);
        let back = V020SignalingTransport::payload_to_message(payload).unwrap();
        match back {
            SignalingMessage::RoleAssignment { is_offerer } => assert!(is_offerer),
            other => panic!("expected RoleAssignment, got {other:?}"),
        }
    }

    #[test]
    fn inbound_offer_produces_offer_message() {
        let sd = actr_protocol::SessionDescription {
            r#type: session_description::Type::Offer as i32,
            sdp: "v=0\r\n...".to_string(),
            sdp_exchange_id: Some("inbound-offer".to_string()),
        };
        let payload = actr_relay::Payload::SessionDescription(sd);
        let msg = V020SignalingTransport::payload_to_message(payload).unwrap();
        assert!(matches!(
            msg,
            SignalingMessage::Offer {
                exchange_id: Some(exchange_id),
                ..
            } if exchange_id == "inbound-offer"
        ));
    }

    #[test]
    fn inbound_renegotiation_offer_produces_offer_message() {
        let sd = actr_protocol::SessionDescription {
            r#type: session_description::Type::RenegotiationOffer as i32,
            sdp: "v=0\r\n...".to_string(),
            sdp_exchange_id: None,
        };
        let payload = actr_relay::Payload::SessionDescription(sd);
        let msg = V020SignalingTransport::payload_to_message(payload).unwrap();
        assert!(matches!(msg, SignalingMessage::Offer { .. }));
    }

    #[test]
    fn ice_restart_request_round_trips() {
        let payload = actr_relay::Payload::IceRestartRequest(actr_protocol::IceRestartRequest {
            reason: Some("test".to_string()),
            request_id: Some("request-1".to_string()),
            attempt: Some(2),
        });
        assert!(matches!(
            V020SignalingTransport::payload_to_message(payload),
            Some(SignalingMessage::IceRestartRequest { request_id, attempt })
                if request_id == "request-1" && attempt == 2
        ));
    }
}
