//! SDK-side, mutually authenticated Noise XX sessions. The relay receives only
//! handshake records and ciphertext; private keys never leave the endpoints.
//! Certificates and issuer keys must come from a separately trusted AIS.

use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use snow::{HandshakeState, TransportState};
use subtle::ConstantTimeEq;
use zeroize::{Zeroize, Zeroizing};

pub mod actor;
pub use actor::{
    ACTOR_CHANNEL_AUDIENCE, ACTOR_CHANNEL_CERTIFICATE_DOMAIN, ActorChannelCertificate,
    ActorChannelClaims, ActorChannelEnrollment, ActorChannelIdentity, ActorChannelSession,
    ActorIdentityKey, VerifiedActorChannel, actor_channel_signing_bytes,
    verify_actor_channel_certificate,
};

pub const PATTERN: &str = "Noise_XX_25519_ChaChaPoly_SHA256";
pub const CERTIFICATE_DOMAIN: &str = "ACTR-RELAY-IDENTITY-V2";
pub const CERTIFICATE_TTL: u64 = 300;
pub const MAX_APPLICATION_BYTES: usize = 8 * 1024 * 1024 + 5;
pub const MAX_RECORD_BYTES: usize = 65535 + 13;
const MAGIC: &[u8; 8] = b"ACTRE2E1";
const HEADER: usize = 13;
#[cfg(not(test))]
const REKEY_RECORDS: u64 = 1 << 20;
// Keep the same rekey transition reachable in short adversarial unit tests.
#[cfg(test)]
const REKEY_RECORDS: u64 = 16;
const FRAGMENT_BYTES: usize = 65535 - 16 - 9;
const CONFIRM_INITIATOR: &[u8] = b"\0ACTR-RELAY-CONFIRM-V1:I";
const CONFIRM_RESPONDER: &[u8] = b"\0ACTR-RELAY-CONFIRM-V1:R";
const ADMISSION_DOMAIN: &[u8] = b"ACTR-RELAY-ADMISSION-V1\0";

/// Reserve this many bytes before constructing all records for an application
/// message. Caller-side admission bounds concurrent encryption across peers.
pub fn encrypted_bytes(application_bytes: usize) -> Result<usize> {
    if application_bytes > MAX_APPLICATION_BYTES {
        return Err(SecurityError("application message too large"));
    }
    Ok(application_bytes + application_bytes.div_ceil(FRAGMENT_BYTES).max(1) * (HEADER + 9 + 16))
}

#[derive(Debug, thiserror::Error)]
#[error("relay secure session rejected: {0}")]
pub struct SecurityError(pub &'static str);
type Result<T> = std::result::Result<T, SecurityError>;

// No Debug implementations: requests contain private bearer proofs.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityRequest {
    pub endpoint: String,
    pub renewal_token: Vec<u8>,
    pub public_key: [u8; 32],
    /// A commitment to a private relay admission secret, not the secret itself.
    pub admission_commitment: [u8; 32],
    /// Optional Actor channel enrollment. Legacy relay-only clients omit it.
    #[serde(default)]
    pub actor: Option<actor::ActorChannelEnrollment>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityClaims {
    pub domain: String,
    pub endpoint: String,
    pub actor_type: String,
    pub client_key: String,
    pub public_key: [u8; 32],
    pub admission_commitment: [u8; 32],
    pub expires_at: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Certificate {
    pub key_id: u32,
    pub claims: Vec<u8>,
    pub signature: Vec<u8>,
}

/// Private relay admission material. Peers receive only the certificate.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelayCredential {
    pub certificate: Certificate,
    pub admission_secret: [u8; 32],
}

impl RelayCredential {
    /// Check the secret against claims already authenticated by the issuer.
    /// This does not verify the certificate signature or expiry.
    pub fn verify_admission(&self, claims: &IdentityClaims) -> Result<()> {
        let commitment = admission_commitment(&self.admission_secret);
        if bool::from(commitment.ct_eq(&claims.admission_commitment)) {
            Ok(())
        } else {
            Err(SecurityError("relay admission secret mismatch"))
        }
    }
}

impl Drop for RelayCredential {
    fn drop(&mut self) {
        self.admission_secret.zeroize();
    }
}

fn admission_commitment(secret: &[u8; 32]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(ADMISSION_DOMAIN);
    digest.update(secret);
    digest.finalize().into()
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssuerKey {
    pub key_id: u32,
    pub public_key: [u8; 32],
    /// The issuer key is trusted only until this local verification lease ends.
    pub expires_at: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssuedIdentity {
    pub certificate: Certificate,
    pub issuer: IssuerKey,
    /// Present only when the enrollment requested an Actor channel certificate.
    #[serde(default)]
    pub actor_certificate: Option<actor::ActorChannelCertificate>,
}

/// Exact bytes signed by AIS, with domain separation from all other AIS uses.
pub fn signing_bytes(claims: &[u8]) -> Vec<u8> {
    let mut bytes = CERTIFICATE_DOMAIN.as_bytes().to_vec();
    bytes.push(0);
    bytes.extend_from_slice(claims);
    bytes
}

/// Verify a certificate against a currently trusted issuer key. A caller that
/// retains the result must bound its use by both the certificate expiry and
/// the issuer key's verification lease.
pub fn verify_certificate(
    certificate: &Certificate,
    issuer: &IssuerKey,
    endpoint: &str,
    public_key: &[u8],
    now: u64,
) -> Result<IdentityClaims> {
    if certificate.key_id != issuer.key_id || issuer.expires_at <= now {
        return Err(SecurityError("untrusted or expired issuer"));
    }
    let key = VerifyingKey::from_bytes(&issuer.public_key)
        .map_err(|_| SecurityError("invalid issuer key"))?;
    let signature = Signature::from_slice(&certificate.signature)
        .map_err(|_| SecurityError("invalid certificate signature"))?;
    key.verify_strict(&signing_bytes(&certificate.claims), &signature)
        .map_err(|_| SecurityError("invalid certificate signature"))?;
    let claims: IdentityClaims = serde_json::from_slice(&certificate.claims)
        .map_err(|_| SecurityError("invalid certificate claims"))?;
    if claims.domain != CERTIFICATE_DOMAIN
        || claims.endpoint != endpoint
        || claims.actor_type.is_empty()
        || claims.client_key.is_empty()
        || claims.public_key.as_slice() != public_key
        || claims.public_key == [0; 32]
        || claims.admission_commitment == [0; 32]
        || claims.expires_at <= now
    {
        return Err(SecurityError(
            "certificate identity, key or expiry mismatch",
        ));
    }
    Ok(claims)
}

struct OsRandom;
impl snow::types::Random for OsRandom {
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> std::result::Result<(), snow::Error> {
        getrandom::getrandom(dest).map_err(|_| snow::Error::Rng)
    }
}
struct Resolver;
impl snow::resolvers::CryptoResolver for Resolver {
    fn resolve_rng(&self) -> Option<Box<dyn snow::types::Random>> {
        Some(Box::new(OsRandom))
    }
    fn resolve_dh(&self, choice: &snow::params::DHChoice) -> Option<Box<dyn snow::types::Dh>> {
        snow::resolvers::DefaultResolver.resolve_dh(choice)
    }
    fn resolve_hash(
        &self,
        choice: &snow::params::HashChoice,
    ) -> Option<Box<dyn snow::types::Hash>> {
        snow::resolvers::DefaultResolver.resolve_hash(choice)
    }
    fn resolve_cipher(
        &self,
        choice: &snow::params::CipherChoice,
    ) -> Option<Box<dyn snow::types::Cipher>> {
        snow::resolvers::DefaultResolver.resolve_cipher(choice)
    }
}
fn builder() -> snow::Builder<'static> {
    snow::Builder::with_resolver(
        PATTERN.parse().expect("fixed Noise pattern"),
        Box::new(Resolver),
    )
}
fn noise<T>(result: std::result::Result<T, snow::Error>) -> Result<T> {
    result.map_err(|_| SecurityError("Noise authentication or state failure"))
}

#[derive(Clone)]
pub struct IdentityKey {
    private: Zeroizing<Vec<u8>>,
    pub public: [u8; 32],
    admission_secret: Zeroizing<[u8; 32]>,
}
impl IdentityKey {
    pub fn generate() -> Result<Self> {
        let pair = noise(builder().generate_keypair())?;
        let mut admission_secret = Zeroizing::new([0; 32]);
        getrandom::getrandom(admission_secret.as_mut())
            .map_err(|_| SecurityError("relay admission randomness unavailable"))?;
        Ok(Self {
            private: Zeroizing::new(pair.private),
            public: pair
                .public
                .try_into()
                .map_err(|_| SecurityError("invalid Noise key"))?,
            admission_secret,
        })
    }

    pub fn admission_commitment(&self) -> [u8; 32] {
        admission_commitment(&self.admission_secret)
    }

    pub fn relay_credential(&self, certificate: Certificate) -> RelayCredential {
        RelayCredential {
            certificate,
            admission_secret: *self.admission_secret,
        }
    }
    pub fn certify(
        self,
        issued: IssuedIdentity,
        endpoint: &str,
        now: u64,
    ) -> Result<LocalIdentity> {
        let claims = verify_certificate(
            &issued.certificate,
            &issued.issuer,
            endpoint,
            &self.public,
            now,
        )?;
        if !bool::from(
            self.admission_commitment()
                .ct_eq(&claims.admission_commitment),
        ) {
            return Err(SecurityError("relay admission commitment mismatch"));
        }
        Ok(LocalIdentity {
            key: self,
            expires_at: claims.expires_at.min(issued.issuer.expires_at),
            issued,
            endpoint: endpoint.into(),
        })
    }
}
pub struct LocalIdentity {
    key: IdentityKey,
    pub issued: IssuedIdentity,
    pub endpoint: String,
    pub expires_at: u64,
}

impl LocalIdentity {
    pub fn relay_credential(&self) -> RelayCredential {
        self.key.relay_credential(self.issued.certificate.clone())
    }
}

enum State {
    Handshake(Box<HandshakeState>),
    Transport(Box<TransportState>),
}
#[derive(Default)]
pub struct Received {
    pub outbound: Vec<Vec<u8>>,
    pub application: Option<Vec<u8>>,
    pub became_ready: bool,
}

pub struct SecureSession {
    state: Option<State>,
    actor: Option<actor::ActorSessionState>,
    local_certificate: Vec<u8>,
    local_key_id: u32,
    peer: String,
    peer_actor_type: Option<String>,
    initiator: bool,
    started: bool,
    ready: bool,
    expires_at: u64,
    partial: Vec<u8>,
    partial_total: Option<usize>,
}

/// Only a key identifier is exposed in the untrusted framing. Resolve it from
/// the configured trusted AIS. Certificate verification binds it to the key.
pub fn signing_key_id(frame: &[u8]) -> Result<Option<u32>> {
    if frame.len() <= HEADER || frame.len() > MAX_RECORD_BYTES || &frame[..8] != MAGIC {
        return Err(SecurityError("invalid encrypted record"));
    }
    let id = u32::from_be_bytes(frame[9..13].try_into().unwrap());
    match (frame[8], id) {
        (1, 0) | (2, 0) => Ok(None),
        (1, _) => Ok(Some(id)),
        _ => Err(SecurityError("invalid encrypted record kind")),
    }
}
fn wrap(kind: u8, key_id: u32, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(HEADER + payload.len());
    frame.extend_from_slice(MAGIC);
    frame.push(kind);
    frame.extend_from_slice(&key_id.to_be_bytes());
    frame.extend_from_slice(payload);
    frame
}

impl SecureSession {
    pub fn close(&mut self) {
        self.state = None;
        self.ready = false;
        self.partial.clear();
        self.partial_total = None;
    }
    pub fn new(local: &LocalIdentity, peer: &str, channel: u64, now: u64) -> Result<Self> {
        if local.endpoint == peer || local.expires_at <= now || channel == 0 {
            return Err(SecurityError("invalid secure channel identity"));
        }
        let initiator = local.endpoint.as_str() < peer;
        let (first, second) = if initiator {
            (local.endpoint.as_str(), peer)
        } else {
            (peer, local.endpoint.as_str())
        };
        let mut prologue = b"ACTR-RELAY-E2E-V1\0".to_vec();
        for endpoint in [first, second] {
            prologue.extend_from_slice(&(endpoint.len() as u32).to_be_bytes());
            prologue.extend_from_slice(endpoint.as_bytes());
        }
        prologue.extend_from_slice(&channel.to_be_bytes());
        let certificate = serde_json::to_vec(&local.issued.certificate)
            .map_err(|_| SecurityError("invalid local certificate"))?;
        Self::with_handshake(
            &local.key.private,
            certificate,
            local.issued.certificate.key_id,
            peer,
            local.expires_at,
            initiator,
            prologue,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn with_handshake(
        private_key: &[u8],
        local_certificate: Vec<u8>,
        local_key_id: u32,
        peer: &str,
        expires_at: u64,
        initiator: bool,
        prologue: Vec<u8>,
    ) -> Result<Self> {
        let b = noise(builder().local_private_key(private_key))?;
        let b = noise(b.prologue(&prologue))?;
        let state = if initiator {
            noise(b.build_initiator())?
        } else {
            noise(b.build_responder())?
        };
        Ok(Self {
            state: Some(State::Handshake(Box::new(state))),
            actor: None,
            local_certificate,
            local_key_id,
            peer: peer.into(),
            peer_actor_type: None,
            initiator,
            started: false,
            ready: false,
            expires_at,
            partial: Vec::new(),
            partial_total: None,
        })
    }
    pub fn is_ready(&self) -> bool {
        self.ready && self.state.is_some()
    }
    pub fn peer_actor_type(&self) -> Option<&str> {
        self.peer_actor_type.as_deref()
    }
    /// Authenticated Actor context, available only after both the Actor proof
    /// and bidirectional Noise key confirmation have completed.
    pub fn verified_actor(&self) -> Option<&actor::VerifiedActorChannel> {
        if !self.is_ready() {
            return None;
        }
        self.actor.as_ref()?.verified_context()
    }
    pub fn expires_at(&self) -> u64 {
        self.expires_at
    }
    pub fn buffered_bytes(&self) -> usize {
        self.partial.len()
    }
    pub fn start(&mut self) -> Result<Option<Vec<u8>>> {
        if self.started {
            return Err(SecurityError("handshake already started"));
        }
        self.started = true;
        if !self.initiator {
            return Ok(None);
        }
        let Some(State::Handshake(state)) = self.state.as_mut() else {
            return Err(SecurityError("session closed"));
        };
        let mut bytes = vec![0; 65535];
        let len = noise(state.write_message(&[], &mut bytes))?;
        Ok(Some(wrap(1, 0, &bytes[..len])))
    }
    pub fn receive(
        &mut self,
        frame: &[u8],
        issuer: Option<&IssuerKey>,
        now: u64,
    ) -> Result<Received> {
        let state = self.state.take().ok_or(SecurityError("session closed"))?;
        match self.receive_inner(state, frame, issuer, now) {
            Ok((state, received)) => {
                self.state = Some(state);
                Ok(received)
            }
            Err(error) => {
                self.ready = false;
                self.partial.clear();
                self.partial_total = None;
                Err(error)
            }
        }
    }
    fn receive_inner(
        &mut self,
        state: State,
        frame: &[u8],
        issuer: Option<&IssuerKey>,
        now: u64,
    ) -> Result<(State, Received)> {
        let id = signing_key_id(frame)?;
        if now >= self.expires_at || !self.started {
            return Err(SecurityError("session expired or not started"));
        }
        let mut received = Received::default();
        let mut plain = vec![0; 65535];
        match state {
            State::Handshake(mut handshake) => {
                if frame[8] != 1 {
                    return Err(SecurityError("application before handshake"));
                }
                let len = noise(handshake.read_message(&frame[HEADER..], &mut plain))?;
                let remote = handshake.get_remote_static();
                if let Some(remote) = remote {
                    let (actor_type, certificate_expiry) = if let Some(actor_state) =
                        self.actor.as_mut()
                    {
                        let certificate: actor::ActorChannelCertificate =
                            serde_json::from_slice(&plain[..len])
                                .map_err(|_| SecurityError("missing peer Actor certificate"))?;
                        if id != Some(certificate.key_id) {
                            return Err(SecurityError("issuer identifier mismatch"));
                        }
                        let issuer = issuer.ok_or(SecurityError("missing trusted Actor issuer"))?;
                        let claims = actor::verify_actor_channel_certificate(
                            &certificate,
                            issuer,
                            actor_state.realm(),
                            &self.peer,
                            remote,
                            now,
                        )?;
                        let actor_type = claims.actor_type.clone();
                        let expires_at = claims.expires_at;
                        actor_state.set_peer_claims(claims);
                        (actor_type, expires_at)
                    } else {
                        let certificate: Certificate = serde_json::from_slice(&plain[..len])
                            .map_err(|_| SecurityError("missing peer certificate"))?;
                        if id != Some(certificate.key_id) {
                            return Err(SecurityError("issuer identifier mismatch"));
                        }
                        let issuer = issuer.ok_or(SecurityError("missing trusted issuer"))?;
                        let claims =
                            verify_certificate(&certificate, issuer, &self.peer, remote, now)?;
                        (claims.actor_type, claims.expires_at)
                    };
                    self.peer_actor_type = Some(actor_type);
                    // A certificate may outlive a locally cached verification
                    // lease, but the session must not outlive either one.
                    self.expires_at = self
                        .expires_at
                        .min(certificate_expiry)
                        .min(issuer.unwrap().expires_at);
                } else if len != 0 || id.is_some() {
                    return Err(SecurityError("unexpected first handshake payload"));
                }
                if !handshake.is_handshake_finished() {
                    let mut out = vec![0; 65535];
                    let len = noise(handshake.write_message(&self.local_certificate, &mut out))?;
                    received
                        .outbound
                        .push(wrap(1, self.local_key_id, &out[..len]));
                }
                if handshake.is_handshake_finished() {
                    if let Some(actor) = self.actor.as_mut() {
                        actor.finish_handshake(handshake.get_handshake_hash())?;
                    }
                    let mut transport = Box::new(noise(handshake.into_transport_mode())?);
                    if !self.initiator {
                        if self.actor.is_some() {
                            received
                                .outbound
                                .push(self.actor_local_proof(&mut transport)?);
                        }
                        received
                            .outbound
                            .push(encrypt(&mut transport, CONFIRM_RESPONDER)?);
                    }
                    Ok((State::Transport(transport), received))
                } else {
                    Ok((State::Handshake(handshake), received))
                }
            }
            State::Transport(mut transport) => {
                if frame[8] != 2 {
                    return Err(SecurityError("handshake replay"));
                }
                let len = noise(transport.read_message(&frame[HEADER..], &mut plain))?;
                if transport.receiving_nonce().is_multiple_of(REKEY_RECORDS) {
                    transport.rekey_incoming();
                }
                let plain = &plain[..len];
                if !self.ready {
                    if let Some(actor) = self.actor.as_mut() {
                        if !actor.has_peer_proof() {
                            actor.verify_peer_proof(plain)?;
                            return Ok((State::Transport(transport), received));
                        }
                    }
                    let expected = if self.initiator {
                        CONFIRM_RESPONDER
                    } else {
                        CONFIRM_INITIATOR
                    };
                    if plain != expected {
                        return Err(SecurityError("missing mutual key confirmation"));
                    }
                    if self.initiator {
                        if self.actor.is_some() {
                            received
                                .outbound
                                .push(self.actor_local_proof(&mut transport)?);
                        }
                        received
                            .outbound
                            .push(encrypt(&mut transport, CONFIRM_INITIATOR)?);
                    }
                    if let Some(actor) = self.actor.as_mut() {
                        if !actor.can_be_ready() {
                            return Err(SecurityError("incomplete Actor channel authentication"));
                        }
                        actor.mark_ready(self.expires_at)?;
                    }
                    self.ready = true;
                    received.became_ready = true;
                } else {
                    if plain.len() < 9 || plain[0] != 1 {
                        return Err(SecurityError("invalid application record"));
                    }
                    let total = u32::from_be_bytes(plain[1..5].try_into().unwrap()) as usize;
                    let offset = u32::from_be_bytes(plain[5..9].try_into().unwrap()) as usize;
                    if total > MAX_APPLICATION_BYTES
                        || offset != self.partial.len()
                        || self.partial_total.is_some_and(|expected| expected != total)
                        || plain.len() - 9 > total.saturating_sub(offset)
                        || (plain.len() == 9 && total != 0)
                    {
                        return Err(SecurityError("invalid application fragmentation"));
                    }
                    self.partial_total = Some(total);
                    self.partial.extend_from_slice(&plain[9..]);
                    if self.partial.len() == total {
                        received.application = Some(std::mem::take(&mut self.partial));
                        self.partial_total = None;
                    }
                }
                Ok((State::Transport(transport), received))
            }
        }
    }
    pub fn send(&mut self, application: &[u8], now: u64) -> Result<Vec<Vec<u8>>> {
        if !self.is_ready() || now >= self.expires_at {
            return Err(SecurityError("session not authenticated or expired"));
        }
        encrypted_bytes(application.len())?;
        let Some(State::Transport(transport)) = self.state.as_mut() else {
            return Err(SecurityError("session closed"));
        };
        let mut records = Vec::new();
        for (index, part) in application.chunks(FRAGMENT_BYTES).enumerate() {
            let mut plain = Vec::with_capacity(part.len() + 9);
            plain.push(1);
            plain.extend_from_slice(&(application.len() as u32).to_be_bytes());
            plain.extend_from_slice(&((index * FRAGMENT_BYTES) as u32).to_be_bytes());
            plain.extend_from_slice(part);
            records.push(encrypt(transport, &plain)?);
        }
        if application.is_empty() {
            records.push(encrypt(transport, &[1, 0, 0, 0, 0, 0, 0, 0, 0])?);
        }
        Ok(records)
    }
}
fn encrypt(state: &mut TransportState, plain: &[u8]) -> Result<Vec<u8>> {
    let mut bytes = vec![0; plain.len() + 16];
    let len = noise(state.write_message(plain, &mut bytes))?;
    if state.sending_nonce().is_multiple_of(REKEY_RECORDS) {
        state.rekey_outgoing();
    }
    Ok(wrap(2, 0, &bytes[..len]))
}

#[cfg(test)]
mod tests;
