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
    #[cfg(any(test, feature = "test-utils"))]
    Fixture(Arc<actr_relay_security::ActorChannelIdentity>),
}

impl ActorChannelCredentials {
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn fixture(id: &ActrId) -> Self {
        use actr_relay_security::{
            ActorChannelCertificate, ActorChannelClaims, ActorChannelIdentity, ActorIdentityKey,
            IdentityKey, IssuerKey,
        };
        let issuer = ed25519_dalek::SigningKey::from_bytes(&[42; 32]);
        let now = crate::wire::relay::now_secs();
        let expires_at = now + 3600;
        let noise = IdentityKey::generate().expect("fixture Noise key");
        let actor = ActorIdentityKey::generate().expect("fixture Actor key");
        let claims = ActorChannelClaims {
            realm: id.realm.clone(),
            actor_id: id.to_string_repr(),
            actor_type: "test/Peer@1.0.0".into(),
            member: "fixture".into(),
            client_key: "fixture".into(),
            role: "editor".into(),
            actor_public_key: actor.public,
            noise_public_key: noise.public,
            actor_credential_digest: [1; 32],
            policy_digest: [2; 32],
            actor_state_digest: [3; 32],
            issuer: id.realm.clone(),
            provider: "fixture".into(),
            audience: actr_relay_security::ACTOR_CHANNEL_AUDIENCE.into(),
            expires_at,
            runtime_instance_id: uuid::Uuid::new_v4().to_string(),
        };
        let certificate =
            ActorChannelCertificate::issue(&claims, 1, &issuer).expect("fixture Actor certificate");
        let issuer = IssuerKey {
            key_id: 1,
            public_key: issuer.verifying_key().to_bytes(),
            expires_at,
        };
        let identity = ActorChannelIdentity::certify(
            noise,
            actor,
            certificate,
            issuer,
            &id.realm,
            &claims.actor_id,
            now,
        )
        .expect("fixture channel identity");
        Self::Fixture(Arc::new(identity))
    }

    async fn identity(&self) -> NetworkResult<Arc<actr_relay_security::ActorChannelIdentity>> {
        match self {
            Self::Enrollment(provider) => provider.actor_channel_identity().await,
            #[cfg(any(test, feature = "test-utils"))]
            Self::Fixture(identity) => Ok(identity.clone()),
        }
    }

    async fn issuer(&self, id: u32) -> NetworkResult<actr_relay_security::IssuerKey> {
        match self {
            Self::Enrollment(provider) => provider.issuer(id).await,
            #[cfg(any(test, feature = "test-utils"))]
            Self::Fixture(identity) if identity.issuer().key_id == id => {
                Ok(identity.issuer().clone())
            }
            #[cfg(any(test, feature = "test-utils"))]
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
