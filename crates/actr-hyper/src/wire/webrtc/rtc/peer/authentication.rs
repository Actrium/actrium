use std::sync::Arc;

use actr_protocol::ActrId;
use actr_relay_security::{ActorChannelSession, RTC_AUTH_FRAME_PREFIX, signing_key_id};
use bytes::BytesMut;

use super::{adapter::LaneDataChannel, effects::SessionOps};
use crate::transport::{NetworkError, NetworkResult, SecureCell, SecureChannelState};
use crate::wire::relay::{RelayClient, now_secs, secure_error, verified_peer_context};

#[derive(Clone)]
pub(crate) enum ActorChannelCredentials {
    Enrollment(Arc<RelayClient>),
    #[cfg(feature = "test-utils")]
    Fixture(Arc<actr_relay_security::ActorChannelIdentity>),
}

impl ActorChannelCredentials {
    async fn identity(&self) -> NetworkResult<Arc<actr_relay_security::ActorChannelIdentity>> {
        match self {
            Self::Enrollment(provider) => provider.actor_channel_identity().await,
            #[cfg(feature = "test-utils")]
            Self::Fixture(identity) => Ok(identity.clone()),
        }
    }

    async fn issuer(&self, id: u32) -> NetworkResult<actr_relay_security::IssuerKey> {
        match self {
            Self::Enrollment(provider) => provider.issuer(id).await,
            #[cfg(feature = "test-utils")]
            Self::Fixture(identity) if identity.issuer().key_id == id => {
                Ok(identity.issuer().clone())
            }
            #[cfg(feature = "test-utils")]
            Self::Fixture(_) => Err(NetworkError::ConnectionError(
                "unrecognized fixture Issuer".into(),
            )),
        }
    }
}

pub(super) struct RtcAuthentication {
    provider: ActorChannelCredentials,
    ops: Arc<dyn SessionOps>,
    peer: ActrId,
    generation: u64,
    exchange: Option<(ActrId, ActorChannelSession)>,
}

impl RtcAuthentication {
    pub(super) fn new(
        provider: ActorChannelCredentials,
        ops: Arc<dyn SessionOps>,
        peer: ActrId,
        generation: u64,
    ) -> Self {
        Self {
            provider,
            ops,
            peer,
            generation,
            exchange: None,
        }
    }

    pub(super) async fn start(
        &mut self,
        channel: &Arc<dyn LaneDataChannel>,
        secure: &SecureCell,
    ) -> NetworkResult<()> {
        if secure.is_io_sendable() {
            return Ok(());
        }
        if !secure.set_state_validated(SecureChannelState::New, SecureChannelState::Handshaking) {
            return Err(NetworkError::SecureChannelNotReady(
                "WebRTC Actor exchange already started".into(),
            ));
        }
        let (local_fingerprint, peer_fingerprint) = self
            .ops
            .dtls_fingerprints()
            .await
            .map_err(|error| NetworkError::ConnectionError(error.to_string()))?;
        let identity = self.provider.identity().await?;
        let local = ActrId::from_string_repr(&identity.claims().actor_id)
            .map_err(|error| NetworkError::ConnectionError(error.to_string()))?;
        if local.realm != self.peer.realm {
            return Err(NetworkError::ConnectionError(
                "WebRTC Actor realm mismatch".into(),
            ));
        }
        let mut exchange = ActorChannelSession::new_rtc(
            &identity,
            &self.peer.to_string_repr(),
            &local_fingerprint,
            &peer_fingerprint,
            now_secs(),
        )
        .map_err(secure_error)?;
        if let Some(first) = exchange.start().map_err(secure_error)? {
            Self::send(channel, &first).await?;
        }
        self.exchange = Some((local, exchange));
        Ok(())
    }

    pub(super) async fn receive(
        &mut self,
        record: &[u8],
        channel: &Arc<dyn LaneDataChannel>,
        secure: &SecureCell,
    ) -> NetworkResult<()> {
        let (local, exchange) = self.exchange.as_mut().ok_or_else(|| {
            NetworkError::ConnectionError("unexpected WebRTC Actor authentication record".into())
        })?;
        let issuer = match signing_key_id(record).map_err(secure_error)? {
            Some(id) => Some(self.provider.issuer(id).await?),
            None => None,
        };
        let received = exchange
            .receive(record, issuer.as_ref(), now_secs())
            .map_err(secure_error)?;
        for reply in received.outbound {
            Self::send(channel, &reply).await?;
        }
        if received.became_ready {
            let verified = exchange.verified().cloned().ok_or_else(|| {
                NetworkError::ConnectionError("missing verified WebRTC Actor context".into())
            })?;
            let binding = verified.channel_id.clone();
            let mut context = verified_peer_context(
                local.clone(),
                self.peer.clone(),
                &binding,
                self.generation,
                verified,
            )?;
            // Each completed exchange has fresh Noise transcript material.
            context.channel_id = format!(
                "rtc:{}",
                uuid::Uuid::from_bytes(
                    context.transcript_digest[..16]
                        .try_into()
                        .expect("SHA-256 transcript")
                )
            );
            if !secure.publish_verified(SecureChannelState::Handshaking, context) {
                return Err(NetworkError::SecureChannelNotReady(
                    "WebRTC Actor generation was retired".into(),
                ));
            }
            self.exchange = None;
        }
        Ok(())
    }

    async fn send(channel: &Arc<dyn LaneDataChannel>, record: &[u8]) -> NetworkResult<()> {
        let mut frame = BytesMut::with_capacity(RTC_AUTH_FRAME_PREFIX.len() + record.len());
        frame.extend_from_slice(RTC_AUTH_FRAME_PREFIX);
        frame.extend_from_slice(record);
        channel
            .send(frame)
            .await
            .map(|_| ())
            .map_err(|error| NetworkError::SendError(error.to_string()))
    }
}
