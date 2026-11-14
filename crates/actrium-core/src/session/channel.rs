//! Mutual-authentication channel types and handshake messages.
//!
//! See `rfcs/actr/0563-secure-actor-channels.md`.
//!
//! # Architecture
//!
//! ```text
//! Actr Host (actr-hyper)          Actrium Core (this crate)
//! ┌─────────────────────────┐    ┌────────────────────────────┐
//! │ WireHandle / DataLane   │    │ VerifiedChannel            │
//! │ Noise XX (relay-security)│───→│ ActorChannelCertificate     │
//! │ SDP / DTLS / ICE        │    │ ChannelState               │
//! └─────────────────────────┘    │ HandshakeStep1..4          │
//!                                └────────────────────────────┘
//! ```
//!
//! The Actr Host drives the state machine and owns physical connections.
//! This crate defines the shared types that both the Host and the session
//! layer agree on. No transport or crypto primitives are defined here.

use super::MemberRole;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// VerifiedChannel — the result of successful mutual authentication
// ---------------------------------------------------------------------------

/// An authenticated channel between two Actors.
///
/// Produced by the Actr Host after all handshake steps pass. Consumed by the
/// Attach handler to read authoritative peer identity without re-verifying
/// credentials.
///
/// This struct is NOT a credential or bearer token. It is a runtime context
/// that proves the channel was established between two specific Actors with
/// specific keys and fresh nonces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedChannel {
    /// Unique identifier for this channel generation.
    pub channel_id: String,

    /// SHA-256 of the canonical handshake transcript. Both sides compute
    /// the same value and include it in every Attach request.
    pub channel_transcript_digest: Vec<u8>,

    /// Peer identity (verified during handshake).
    pub peer_actor_id: String,
    pub peer_actor_type: String,
    pub peer_member: String,
    pub peer_client_key: String,
    pub peer_role: MemberRole,
    pub peer_actor_public_key: Vec<u8>,

    /// Channel expiry derived from peer credential.
    pub peer_credential_expires_at_epoch_secs: u64,

    /// Local runtime instance (for stale-session detection on restart).
    pub local_runtime_instance_id: String,
    /// Peer runtime instance.
    pub peer_runtime_instance_id: String,

    /// Negotiated transport and security parameters.
    pub negotiation_digest: Vec<u8>,
}

impl VerifiedChannel {
    /// Returns `true` if the channel has not yet expired.
    pub fn is_valid_at(&self, now_epoch_secs: u64) -> bool {
        self.peer_credential_expires_at_epoch_secs > now_epoch_secs
    }
}

// ---------------------------------------------------------------------------
// Channel state machine
// ---------------------------------------------------------------------------

/// States for a physical secure channel. The Actr Host drives transitions.
///
/// Only `Verified` allows application data. `LegacyBypass` is a temporary
/// compatibility mode that MUST be removed before production use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelState {
    /// Pre-handshake. No data allowed.
    New,
    /// Handshake in progress. No application data.
    Handshaking,
    /// Mutual authentication and key confirmation complete. Ready for Attach.
    Verified,
    /// Symmetric rekey in progress (same identity, same state).
    Rekeying,
    /// Re-authenticating after credential or ActorStateDigest change.
    Reauthenticating,
    /// Terminal: transport closed or handshake failed.
    Closed,
    /// Terminal: authentication failure.
    Failed,
    /// Temporary transport-only mode. MUST be removed in production.
    LegacyBypass,
}

impl ChannelState {
    /// Whether application data (Attach, RPC) may be sent or received.
    pub fn is_sendable(&self) -> bool {
        matches!(self, ChannelState::Verified)
    }

    /// Whether this is a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, ChannelState::Closed | ChannelState::Failed)
    }

    /// Whether a state transition is legal.
    pub fn can_transition_to(&self, next: ChannelState) -> bool {
        use ChannelState::*;
        matches!(
            (self, next),
            (New, Handshaking)
                | (New, Closed)
                | (New, Failed)
                | (Handshaking, Verified)
                | (Handshaking, Closed)
                | (Handshaking, Failed)
                | (Verified, Rekeying)
                | (Verified, Reauthenticating)
                | (Verified, Closed)
                | (Rekeying, Verified)
                | (Rekeying, Closed)
                | (Reauthenticating, Verified)
                | (Reauthenticating, Closed)
                | (LegacyBypass, Handshaking)
                | (LegacyBypass, Closed)
                | (LegacyBypass, Failed)
        )
    }
}

// ---------------------------------------------------------------------------
// ActorChannelCertificate — issuer-signed binding
// ---------------------------------------------------------------------------

/// Issuer-signed certificate binding an Actor identity to a Noise static key.
///
/// This certificate is produced by the Realm Issuer (loran-issuer) and
/// carried during the handshake. It binds:
///
/// ```text
/// ActorId ↔ Actor signing public key ↔ Noise static public key
/// ```
///
/// The peer verifies this certificate using the Issuer key from the accepted
/// Realm Profile, never from a signaling-supplied key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorChannelCertificate {
    /// Realm this certificate is valid for.
    pub realm: String,
    /// Actor identity.
    pub actor_id: String,
    pub actor_type: String,
    /// Member/client/role claims from the Issuer.
    pub member: String,
    pub client_key: String,
    pub role: MemberRole,

    /// Actor's long-term Ed25519 signing public key.
    pub actor_public_key: Vec<u8>,
    /// Noise static public key (X25519).
    pub noise_public_key: Vec<u8>,

    /// SHA-256 of the Actor credential this certificate was issued against.
    pub actor_credential_digest: Vec<u8>,

    /// Issuer information.
    pub issuer: String,
    pub provider: String,
    /// Expected audience: "actr.actor.channel".
    pub audience: String,

    /// Expiry (Unix epoch seconds).
    pub expires_at_epoch_secs: u64,

    /// Ed25519 signature over the canonical certificate claims.
    pub signature: Vec<u8>,
}

impl ActorChannelCertificate {
    /// Verify that this certificate is valid for the expected parameters.
    ///
    /// The caller must verify the `signature` field against the Issuer key
    /// from the accepted Realm Profile. This method checks structural
    /// consistency and expiry only.
    pub fn is_consistent(
        &self,
        expected_realm: &str,
        expected_actor_id: &str,
        now_epoch_secs: u64,
    ) -> bool {
        self.realm == expected_realm
            && self.actor_id == expected_actor_id
            && self.expires_at_epoch_secs > now_epoch_secs
            && !self.actor_public_key.is_empty()
            && !self.noise_public_key.is_empty()
    }
}

// ---------------------------------------------------------------------------
// PeerBinding — transport-specific signed binding
// ---------------------------------------------------------------------------

/// Signed binding between Actor identity and transport-level key material.
///
/// For WebRTC, this binds the Actor's DTLS fingerprint to its ActorId.
/// For WebSocket, it binds the ephemeral DH public key.
///
/// The `signature` is produced by the Actor's long-term signing key and
/// verified using the public key from the `ActorChannelCertificate`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerBinding {
    pub source_actor_id: String,
    pub target_actor_id: String,
    pub exchange_id: u64,
    pub nonce: Vec<u8>,
    /// Transport-specific key material (DTLS fingerprint, ephemeral DH key, etc.).
    pub transport_key_material: Vec<u8>,
    /// Transport kind discriminator.
    pub transport_kind: TransportKind,
    /// Ed25519 signature over the canonical binding.
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportKind {
    WebRtc,
    WebSocket,
    Relay,
}

// ---------------------------------------------------------------------------
// Handshake messages (four-step mutual authentication)
// ---------------------------------------------------------------------------

/// Step 1: A → B. Front initiates handshake.
///
/// A sends its negotiation parameters, credential, and nonce. B cannot yet
/// trust A because A has not responded to B's challenge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeStep1 {
    /// Negotiation parameters (transport, version, SDP/DTLS fingerprint, etc.).
    pub negotiation: NegotiationParams,
    /// A's Actor credential.
    pub credential: ActorChannelCertificate,
    /// A's ephemeral public key for this handshake.
    pub ephemeral_public_key: Vec<u8>,
    /// A's fresh nonce.
    pub nonce: Vec<u8>,
    /// Exchange identifier for this handshake.
    pub exchange_id: u64,
}

/// Step 2: B → A. Service responds to Front.
///
/// B validates A's negotiation and actorKeyProof, then responds with its own
/// credential, nonce, and a challenge response over A's nonce.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeStep2 {
    pub negotiation: NegotiationParams,
    pub credential: ActorChannelCertificate,
    pub ephemeral_public_key: Vec<u8>,
    pub nonce: Vec<u8>,
    /// Signature over (A's nonce + B's nonce + exchange_id) proving B holds
    /// its Actor private key and has seen A's challenge.
    pub challenge_response: Vec<u8>,
    pub exchange_id: u64,
}

/// Step 3: A → B. Front responds to Service's challenge and confirms keys.
///
/// A signs B's nonce and computes a key-confirmation MAC over the transcript.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeStep3 {
    /// Signature over (A's nonce + B's nonce + exchange_id) proving A holds
    /// its Actor private key and has seen B's challenge.
    pub challenge_response: Vec<u8>,
    /// MAC(frontKey, "actr-channel-confirm:v1:A" || channelTranscriptDigest)
    pub key_confirmation: Vec<u8>,
    pub exchange_id: u64,
}

/// Step 4: B → A. Service confirms keys.
///
/// B verifies A's challenge response and key confirmation, then sends its
/// own key-confirmation MAC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeStep4 {
    /// MAC(serviceKey, "actr-channel-confirm:v1:B" || channelTranscriptDigest)
    pub key_confirmation: Vec<u8>,
    pub exchange_id: u64,
}

/// Negotiation parameters carried in steps 1 and 2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NegotiationParams {
    pub protocol_version: u32,
    pub transport_kind: TransportKind,
    pub cipher_suite: String,
    /// SHA-256 of canonical transport-specific negotiation (SDP, WS headers, etc.).
    pub negotiation_digest: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Handshake transcript
// ---------------------------------------------------------------------------

/// Canonical transcript for computing `channelTranscriptDigest`.
///
/// Both sides independently construct this from the handshake messages and
/// compute SHA-256. The resulting digest is included in Attach requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeTranscript {
    pub protocol_version: u32,
    pub source_actor_id: String,
    pub target_actor_id: String,
    pub exchange_id: u64,
    pub source_nonce: Vec<u8>,
    pub target_nonce: Vec<u8>,
    pub source_ephemeral_public_key: Vec<u8>,
    pub target_ephemeral_public_key: Vec<u8>,
    pub source_credential_digest: Vec<u8>,
    pub target_credential_digest: Vec<u8>,
    pub source_negotiation_digest: Vec<u8>,
    pub target_negotiation_digest: Vec<u8>,
    pub target_service_actor_id: String,
    pub transport_kind: TransportKind,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_state_sendable_only_when_verified() {
        assert!(!ChannelState::New.is_sendable());
        assert!(!ChannelState::Handshaking.is_sendable());
        assert!(ChannelState::Verified.is_sendable());
        assert!(!ChannelState::Rekeying.is_sendable());
        assert!(!ChannelState::Closed.is_sendable());
        assert!(!ChannelState::Failed.is_sendable());
        assert!(!ChannelState::LegacyBypass.is_sendable());
    }

    #[test]
    fn channel_state_terminal() {
        assert!(!ChannelState::New.is_terminal());
        assert!(ChannelState::Closed.is_terminal());
        assert!(ChannelState::Failed.is_terminal());
        assert!(!ChannelState::Verified.is_terminal());
    }

    #[test]
    fn channel_state_valid_transitions() {
        assert!(ChannelState::New.can_transition_to(ChannelState::Handshaking));
        assert!(ChannelState::Handshaking.can_transition_to(ChannelState::Verified));
        assert!(ChannelState::Verified.can_transition_to(ChannelState::Rekeying));
        assert!(ChannelState::Verified.can_transition_to(ChannelState::Reauthenticating));
        assert!(ChannelState::Rekeying.can_transition_to(ChannelState::Verified));
        assert!(ChannelState::LegacyBypass.can_transition_to(ChannelState::Handshaking));
        // Invalid transitions
        assert!(!ChannelState::New.can_transition_to(ChannelState::Verified));
        assert!(!ChannelState::Closed.can_transition_to(ChannelState::Handshaking));
        assert!(!ChannelState::Failed.can_transition_to(ChannelState::Verified));
    }

    #[test]
    fn verified_channel_expiry() {
        let channel = VerifiedChannel {
            channel_id: "ch1".into(),
            channel_transcript_digest: vec![0; 32],
            peer_actor_id: "actor1".into(),
            peer_actor_type: "front".into(),
            peer_member: "member1".into(),
            peer_client_key: "key1".into(),
            peer_role: MemberRole::Owner,
            peer_actor_public_key: vec![1; 32],
            peer_credential_expires_at_epoch_secs: 1000,
            local_runtime_instance_id: "local1".into(),
            peer_runtime_instance_id: "peer1".into(),
            negotiation_digest: vec![0; 32],
        };
        assert!(channel.is_valid_at(999));
        assert!(!channel.is_valid_at(1000));
        assert!(!channel.is_valid_at(1001));
    }

    #[test]
    fn actor_channel_certificate_consistency() {
        let cert = ActorChannelCertificate {
            realm: "realm1".into(),
            actor_id: "actor1".into(),
            actor_type: "front".into(),
            member: "m1".into(),
            client_key: "ck1".into(),
            role: MemberRole::Editor,
            actor_public_key: vec![1; 32],
            noise_public_key: vec![2; 32],
            actor_credential_digest: vec![0; 32],
            issuer: "issuer1".into(),
            provider: "provider1".into(),
            audience: "actr.actor.channel".into(),
            expires_at_epoch_secs: 2000,
            signature: vec![0; 64],
        };
        assert!(cert.is_consistent("realm1", "actor1", 1000));
        assert!(!cert.is_consistent("realm1", "actor1", 2000));
        assert!(!cert.is_consistent("realm2", "actor1", 1000));
        assert!(!cert.is_consistent("realm1", "actor2", 1000));
    }
}
