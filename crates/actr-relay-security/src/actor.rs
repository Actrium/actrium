//! Mutually authenticated Front–Service Actor channels.
//!
//! The Noise XX handshake authenticates issuer-signed Actor certificates. Once
//! Noise finishes, each Actor additionally signs the exact Noise handshake hash
//! and the channel binding with its independent Ed25519 key. Application data
//! and the verified context remain unavailable until both that proof and the
//! existing bidirectional Noise key confirmation have completed.

use super::{
    IdentityKey, IssuerKey, Received, Result, SecureSession, SecurityError, TransportState, encrypt,
};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

pub const ACTOR_CHANNEL_CERTIFICATE_DOMAIN: &str = "ACTR-ACTOR-CHANNEL-CERTIFICATE-V1";
pub const ACTOR_CHANNEL_AUDIENCE: &str = "actr.actor.channel";
const ACTOR_PROOF_DOMAIN: &[u8] = b"ACTR-ACTOR-CHANNEL-PROOF-V1\0";
const ACTOR_TRANSCRIPT_DOMAIN: &[u8] = b"ACTR-ACTOR-CHANNEL-TRANSCRIPT-V1\0";
const ACTOR_PROLOGUE_DOMAIN: &[u8] = b"ACTR-ACTOR-CHANNEL-PROLOGUE-V1\0";
const ACTOR_PROOF_PREFIX: &[u8] = b"\0ACTR-ACTOR-CHANNEL-PROOF-V1\0";

/// Host-provided binding for a single Actor channel enrollment. The Realm
/// client proof covers the enclosing IdentityRequest body, so the Issuer can
/// bind these values into the certificate without a second management token.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorChannelEnrollment {
    pub policy_digest: [u8; 32],
    pub actor_state_digest: [u8; 32],
    pub runtime_instance_id: String,
    /// Canonical protobuf encoding of the Actor's current `AIdCredential`.
    /// The Issuer verifies these bytes instead of trusting the supplied digest.
    pub actor_credential: Vec<u8>,
    pub actor_credential_digest: [u8; 32],
}

/// Digest of the exact credential bytes presented during Actor enrollment.
pub fn actor_credential_digest(credential: &[u8]) -> [u8; 32] {
    Sha256::digest(credential).into()
}

/// Claims signed by the realm Issuer for an Actor channel endpoint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorChannelClaims {
    pub realm: String,
    pub actor_id: String,
    pub actor_type: String,
    pub member: String,
    pub client_key: String,
    pub role: String,
    pub actor_public_key: [u8; 32],
    pub noise_public_key: [u8; 32],
    pub actor_credential_digest: [u8; 32],
    /// Policy and Actor state captured by the same credential issuance and
    /// subsequently bound into the channel proof and verified peer context.
    pub policy_digest: [u8; 32],
    pub actor_state_digest: [u8; 32],
    pub issuer: String,
    pub provider: String,
    pub audience: String,
    pub expires_at: u64,
    /// A fresh value per runtime incarnation, not a reused logical ActorId.
    pub runtime_instance_id: String,
}

/// An issuer-signed certificate. The raw claim bytes are signed exactly as
/// transmitted; verification never reserializes attacker-controlled claims.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorChannelCertificate {
    pub key_id: u32,
    pub claims: Vec<u8>,
    pub signature: Vec<u8>,
}

impl ActorChannelCertificate {
    /// Issue a certificate with an Issuer Ed25519 private key.
    pub fn issue(claims: &ActorChannelClaims, key_id: u32, issuer: &SigningKey) -> Result<Self> {
        let claims = serde_json::to_vec(claims)
            .map_err(|_| SecurityError("invalid Actor certificate claims"))?;
        let signature = issuer
            .sign(&actor_channel_signing_bytes_from_raw(&claims))
            .to_bytes()
            .to_vec();
        Ok(Self {
            key_id,
            claims,
            signature,
        })
    }
}

/// Canonical bytes signed by the Issuer for Actor channel claims.
pub fn actor_channel_signing_bytes(claims: &ActorChannelClaims) -> Result<Vec<u8>> {
    let claims = serde_json::to_vec(claims)
        .map_err(|_| SecurityError("invalid Actor certificate claims"))?;
    Ok(actor_channel_signing_bytes_from_raw(&claims))
}

fn actor_channel_signing_bytes_from_raw(claims: &[u8]) -> Vec<u8> {
    let mut bytes = ACTOR_CHANNEL_CERTIFICATE_DOMAIN.as_bytes().to_vec();
    bytes.push(0);
    bytes.extend_from_slice(claims);
    bytes
}

/// Verify the Issuer signature and every endpoint binding before trusting any
/// claims. `expected_noise_public_key` must come from Noise's remote static key,
/// never from handshake plaintext.
pub fn verify_actor_channel_certificate(
    certificate: &ActorChannelCertificate,
    issuer: &IssuerKey,
    expected_realm: &str,
    expected_actor_id: &str,
    expected_noise_public_key: &[u8],
    now: u64,
) -> Result<ActorChannelClaims> {
    if certificate.key_id != issuer.key_id || issuer.expires_at <= now {
        return Err(SecurityError("untrusted or expired Actor issuer"));
    }
    let issuer_key = VerifyingKey::from_bytes(&issuer.public_key)
        .map_err(|_| SecurityError("invalid Actor issuer key"))?;
    let signature = Signature::from_slice(&certificate.signature)
        .map_err(|_| SecurityError("invalid Actor certificate signature"))?;
    issuer_key
        .verify_strict(
            &actor_channel_signing_bytes_from_raw(&certificate.claims),
            &signature,
        )
        .map_err(|_| SecurityError("invalid Actor certificate signature"))?;
    let claims: ActorChannelClaims = serde_json::from_slice(&certificate.claims)
        .map_err(|_| SecurityError("invalid Actor certificate claims"))?;
    VerifyingKey::from_bytes(&claims.actor_public_key)
        .map_err(|_| SecurityError("invalid Actor public key"))?;
    if claims.realm != expected_realm
        || claims.actor_id != expected_actor_id
        || claims.audience != ACTOR_CHANNEL_AUDIENCE
        || claims.noise_public_key.as_slice() != expected_noise_public_key
        || claims.actor_public_key == [0; 32]
        || claims.noise_public_key == [0; 32]
        || claims.actor_credential_digest == [0; 32]
        || claims.policy_digest == [0; 32]
        || claims.actor_state_digest == [0; 32]
        || claims.expires_at <= now
        || [
            &claims.realm,
            &claims.actor_id,
            &claims.actor_type,
            &claims.member,
            &claims.client_key,
            &claims.role,
            &claims.issuer,
            &claims.provider,
            &claims.runtime_instance_id,
        ]
        .iter()
        .any(|value| value.is_empty())
    {
        return Err(SecurityError(
            "Actor certificate identity, audience, key or expiry mismatch",
        ));
    }
    Ok(claims)
}

/// An Actor's long-term Ed25519 signing key, independent of its Noise key.
pub struct ActorIdentityKey {
    private: Zeroizing<[u8; 32]>,
    pub public: [u8; 32],
}

impl ActorIdentityKey {
    pub fn generate() -> Result<Self> {
        let mut private = Zeroizing::new([0; 32]);
        getrandom::getrandom(private.as_mut())
            .map_err(|_| SecurityError("Actor signing randomness unavailable"))?;
        Ok(Self::from_private(private))
    }

    pub fn from_bytes(private: [u8; 32]) -> Self {
        Self::from_private(Zeroizing::new(private))
    }

    fn from_private(private: Zeroizing<[u8; 32]>) -> Self {
        let public = SigningKey::from_bytes(&private).verifying_key().to_bytes();
        Self { private, public }
    }
}

/// A locally certified Actor with separate Ed25519 and X25519 private keys.
pub struct ActorChannelIdentity {
    noise: IdentityKey,
    actor: ActorIdentityKey,
    certificate: ActorChannelCertificate,
    issuer: IssuerKey,
    claims: ActorChannelClaims,
    expires_at: u64,
}

impl ActorChannelIdentity {
    pub fn certify(
        noise: IdentityKey,
        actor: ActorIdentityKey,
        certificate: ActorChannelCertificate,
        issuer: IssuerKey,
        realm: &str,
        actor_id: &str,
        now: u64,
    ) -> Result<Self> {
        let claims = verify_actor_channel_certificate(
            &certificate,
            &issuer,
            realm,
            actor_id,
            &noise.public,
            now,
        )?;
        if claims.actor_public_key != actor.public {
            return Err(SecurityError("Actor signing key mismatch"));
        }
        let expires_at = claims.expires_at.min(issuer.expires_at);
        Ok(Self {
            noise,
            actor,
            certificate,
            issuer,
            claims,
            expires_at,
        })
    }

    pub fn claims(&self) -> &ActorChannelClaims {
        &self.claims
    }

    pub fn certificate(&self) -> &ActorChannelCertificate {
        &self.certificate
    }

    pub fn issuer(&self) -> &IssuerKey {
        &self.issuer
    }

    pub fn noise_public_key(&self) -> &[u8; 32] {
        &self.noise.public
    }

    pub fn actor_public_key(&self) -> &[u8; 32] {
        &self.actor.public
    }

    pub fn expires_at(&self) -> u64 {
        self.expires_at
    }
}

/// Authenticated peer context, available only after the complete Actor proof
/// and mutual key-confirmation state machine reaches ready.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedActorChannel {
    pub channel_id: String,
    pub transcript_digest: [u8; 32],
    pub peer_claims: ActorChannelClaims,
    pub peer_actor_public_key: [u8; 32],
    pub local_runtime_instance_id: String,
    pub peer_runtime_instance_id: String,
    pub expires_at: u64,
}

impl VerifiedActorChannel {
    pub fn is_valid_at(&self, now: u64) -> bool {
        now < self.expires_at
    }
}

/// A real encrypted Actor channel. Front is always the Noise initiator and
/// Service is always the responder; lexical ActorId ordering is not used.
pub struct ActorChannelSession {
    session: SecureSession,
}

impl ActorChannelSession {
    pub fn new_front(
        local: &ActorChannelIdentity,
        target_service_actor_id: &str,
        channel_id: &str,
        now: u64,
    ) -> Result<Self> {
        Self::new(
            local,
            target_service_actor_id,
            target_service_actor_id,
            channel_id,
            true,
            now,
        )
    }

    pub fn new_service(
        local: &ActorChannelIdentity,
        front_actor_id: &str,
        channel_id: &str,
        now: u64,
    ) -> Result<Self> {
        let service_actor_id = local.claims.actor_id.clone();
        Self::new(
            local,
            front_actor_id,
            &service_actor_id,
            channel_id,
            false,
            now,
        )
    }

    fn new(
        local: &ActorChannelIdentity,
        peer_actor_id: &str,
        target_service_actor_id: &str,
        channel_id: &str,
        initiator: bool,
        now: u64,
    ) -> Result<Self> {
        if local.expires_at <= now
            || local.claims.actor_id == peer_actor_id
            || peer_actor_id.is_empty()
            || target_service_actor_id.is_empty()
            || channel_id.is_empty()
        {
            return Err(SecurityError("invalid Actor channel identity"));
        }
        let (front_actor_id, service_actor_id) = if initiator {
            (local.claims.actor_id.clone(), peer_actor_id.to_owned())
        } else {
            (peer_actor_id.to_owned(), local.claims.actor_id.clone())
        };
        if target_service_actor_id != service_actor_id {
            return Err(SecurityError("target Service ActorId mismatch"));
        }
        let mut prologue = ACTOR_PROLOGUE_DOMAIN.to_vec();
        for value in [
            ACTOR_CHANNEL_AUDIENCE,
            local.claims.realm.as_str(),
            front_actor_id.as_str(),
            service_actor_id.as_str(),
            target_service_actor_id,
            channel_id,
        ] {
            push_string(&mut prologue, value);
        }
        let certificate = serde_json::to_vec(&local.certificate)
            .map_err(|_| SecurityError("invalid local Actor certificate"))?;
        let mut session = SecureSession::with_handshake(
            &local.noise.private,
            certificate,
            local.certificate.key_id,
            peer_actor_id,
            local.expires_at,
            initiator,
            prologue,
        )?;
        session.actor = Some(ActorSessionState {
            actor_private: Zeroizing::new(*local.actor.private),
            local_claims: local.claims.clone(),
            peer_claims: None,
            realm: local.claims.realm.clone(),
            front_actor_id,
            service_actor_id,
            target_service_actor_id: target_service_actor_id.to_owned(),
            channel_id: channel_id.to_owned(),
            handshake_hash: None,
            transcript_digest: None,
            peer_proof_verified: false,
            local_proof_sent: false,
            verified: None,
        });
        Ok(Self { session })
    }

    pub fn start(&mut self) -> Result<Option<Vec<u8>>> {
        self.session.start()
    }

    pub fn receive(
        &mut self,
        frame: &[u8],
        issuer: Option<&IssuerKey>,
        now: u64,
    ) -> Result<Received> {
        self.session.receive(frame, issuer, now)
    }

    pub fn send(&mut self, application: &[u8], now: u64) -> Result<Vec<Vec<u8>>> {
        self.session.send(application, now)
    }

    pub fn close(&mut self) {
        self.session.close();
    }

    /// Consume the wrapper when embedding into an existing SecureSession-based
    /// transport such as the Hyper relay client.
    pub fn into_inner(self) -> SecureSession {
        self.session
    }

    pub fn is_ready(&self) -> bool {
        self.session.is_ready()
    }

    pub fn verified(&self) -> Option<&VerifiedActorChannel> {
        if !self.is_ready() {
            return None;
        }
        self.session.actor.as_ref()?.verified.as_ref()
    }

    pub fn expires_at(&self) -> u64 {
        self.session.expires_at()
    }

    pub fn buffered_bytes(&self) -> usize {
        self.session.buffered_bytes()
    }

    #[cfg(test)]
    fn actor_state_mut(&mut self) -> &mut ActorSessionState {
        self.session.actor.as_mut().unwrap()
    }
}

pub(super) struct ActorSessionState {
    actor_private: Zeroizing<[u8; 32]>,
    local_claims: ActorChannelClaims,
    peer_claims: Option<ActorChannelClaims>,
    realm: String,
    front_actor_id: String,
    service_actor_id: String,
    target_service_actor_id: String,
    channel_id: String,
    handshake_hash: Option<[u8; 32]>,
    transcript_digest: Option<[u8; 32]>,
    peer_proof_verified: bool,
    local_proof_sent: bool,
    verified: Option<VerifiedActorChannel>,
}

impl ActorSessionState {
    pub(super) fn verified_context(&self) -> Option<&VerifiedActorChannel> {
        self.verified.as_ref()
    }

    pub(super) fn realm(&self) -> &str {
        &self.realm
    }

    pub(super) fn set_peer_claims(&mut self, claims: ActorChannelClaims) {
        self.peer_claims = Some(claims);
    }

    pub(super) fn finish_handshake(&mut self, handshake_hash: &[u8]) -> Result<()> {
        let handshake_hash: [u8; 32] = handshake_hash
            .try_into()
            .map_err(|_| SecurityError("invalid Noise handshake hash"))?;
        let proof = self.proof_bytes(&handshake_hash)?;
        self.transcript_digest = Some(
            Sha256::new()
                .chain_update(ACTOR_TRANSCRIPT_DOMAIN)
                .chain_update(&proof)
                .finalize()
                .into(),
        );
        self.handshake_hash = Some(handshake_hash);
        Ok(())
    }

    pub(super) fn create_local_proof(&mut self) -> Result<Vec<u8>> {
        let handshake_hash = self
            .handshake_hash
            .ok_or(SecurityError("Noise handshake hash unavailable"))?;
        let proof = self.proof_bytes(&handshake_hash)?;
        let signature = SigningKey::from_bytes(&self.actor_private).sign(&proof);
        let mut record = ACTOR_PROOF_PREFIX.to_vec();
        record.extend_from_slice(&signature.to_bytes());
        self.local_proof_sent = true;
        Ok(record)
    }

    pub(super) fn has_peer_proof(&self) -> bool {
        self.peer_proof_verified
    }

    pub(super) fn verify_peer_proof(&mut self, record: &[u8]) -> Result<()> {
        let signature_bytes = record
            .strip_prefix(ACTOR_PROOF_PREFIX)
            .ok_or(SecurityError("missing Actor channel proof"))?;
        let signature = Signature::from_slice(signature_bytes)
            .map_err(|_| SecurityError("invalid Actor channel proof"))?;
        let handshake_hash = self
            .handshake_hash
            .ok_or(SecurityError("Noise handshake hash unavailable"))?;
        let proof = self.proof_bytes(&handshake_hash)?;
        let peer = self
            .peer_claims
            .as_ref()
            .ok_or(SecurityError("verified peer Actor claims unavailable"))?;
        let key = VerifyingKey::from_bytes(&peer.actor_public_key)
            .map_err(|_| SecurityError("invalid peer Actor public key"))?;
        key.verify_strict(&proof, &signature)
            .map_err(|_| SecurityError("invalid Actor channel proof"))?;
        self.peer_proof_verified = true;
        Ok(())
    }

    pub(super) fn can_be_ready(&self) -> bool {
        self.peer_proof_verified && self.local_proof_sent
    }

    pub(super) fn mark_ready(&mut self, expires_at: u64) -> Result<()> {
        if !self.can_be_ready() {
            return Err(SecurityError("incomplete Actor channel authentication"));
        }
        let peer_claims = self
            .peer_claims
            .clone()
            .ok_or(SecurityError("verified peer Actor claims unavailable"))?;
        self.verified = Some(VerifiedActorChannel {
            channel_id: self.channel_id.clone(),
            transcript_digest: self
                .transcript_digest
                .ok_or(SecurityError("Actor transcript unavailable"))?,
            peer_actor_public_key: peer_claims.actor_public_key,
            local_runtime_instance_id: self.local_claims.runtime_instance_id.clone(),
            peer_runtime_instance_id: peer_claims.runtime_instance_id.clone(),
            peer_claims,
            expires_at,
        });
        Ok(())
    }

    fn proof_bytes(&self, handshake_hash: &[u8; 32]) -> Result<Vec<u8>> {
        let peer = self
            .peer_claims
            .as_ref()
            .ok_or(SecurityError("verified peer Actor claims unavailable"))?;
        let (front_credential_digest, service_credential_digest) =
            if self.local_claims.actor_id == self.front_actor_id {
                (
                    &self.local_claims.actor_credential_digest,
                    &peer.actor_credential_digest,
                )
            } else {
                (
                    &peer.actor_credential_digest,
                    &self.local_claims.actor_credential_digest,
                )
            };
        let mut bytes = ACTOR_PROOF_DOMAIN.to_vec();
        bytes.extend_from_slice(handshake_hash);
        for value in [
            self.realm.as_str(),
            self.front_actor_id.as_str(),
            self.service_actor_id.as_str(),
            self.target_service_actor_id.as_str(),
            self.channel_id.as_str(),
        ] {
            push_string(&mut bytes, value);
        }
        bytes.extend_from_slice(front_credential_digest);
        bytes.extend_from_slice(service_credential_digest);
        Ok(bytes)
    }
}

impl SecureSession {
    pub(super) fn actor_local_proof(&mut self, transport: &mut TransportState) -> Result<Vec<u8>> {
        let proof = self
            .actor
            .as_mut()
            .ok_or(SecurityError("Actor authentication not configured"))?
            .create_local_proof()?;
        encrypt(transport, &proof)
    }
}

fn push_string(bytes: &mut Vec<u8>, value: &str) {
    bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
    bytes.extend_from_slice(value.as_bytes());
}

#[cfg(test)]
mod tests;
