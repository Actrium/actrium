//! Proof of possession for an Actor's long-term Ed25519 key.
//!
//! The proof is deliberately independent of both the package Publisher key and
//! the Realm client key. Its signing input is stable across protobuf runtimes:
//! length-prefixed identity fields plus the SHA-256 digest of the canonical
//! `RegisterRequest` encoding with `actor_key_proof` cleared.

use crate::{ActrType, RegisterRequest};
use ed25519_dalek::{Signature, Signer as _, SigningKey, Verifier as _, VerifyingKey};
use prost::Message as _;
use prost::bytes::Bytes;
use sha2::{Digest as _, Sha256};
use thiserror::Error;

const ACTOR_KEY_PROOF_DOMAIN: &[u8] = b"actr.actor-key-proof.v1\0";

/// Operation whose credential request is authorized by an Actor-key proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorKeyProofOperation {
    Register,
    Reissue,
}

impl ActorKeyProofOperation {
    fn as_bytes(self) -> &'static [u8] {
        match self {
            Self::Register => b"REGISTER",
            Self::Reissue => b"REISSUE",
        }
    }
}

/// Actor-key proof construction or verification failed.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ActorKeyProofError {
    #[error("missing Actor public key")]
    MissingPublicKey,
    #[error("Actor public key must be 32 bytes, got {0}")]
    InvalidPublicKeyLength(usize),
    #[error("invalid Actor Ed25519 public key: {0}")]
    InvalidPublicKey(String),
    #[error("missing Actor key proof")]
    MissingProof,
    #[error("Actor key proof must be 64 bytes, got {0}")]
    InvalidProofLength(usize),
    #[error("invalid Actor key proof signature")]
    InvalidSignature,
}

fn append_field(message: &mut Vec<u8>, value: &[u8]) {
    message.extend_from_slice(&(value.len() as u64).to_be_bytes());
    message.extend_from_slice(value);
}

fn append_actr_type(message: &mut Vec<u8>, actr_type: &ActrType) {
    append_field(message, actr_type.publisher.as_bytes());
    append_field(message, actr_type.name.as_bytes());
    append_field(message, actr_type.version.as_bytes());
}

/// Bytes an Actor signs to prove possession of its long-term private key.
///
/// The digest covers every logical registration field, including ACL and
/// service metadata. Operation, Realm, Actor type and Actor public key are also
/// explicit fields, so their binding does not rely only on protobuf field
/// numbering or on the outer Realm client proof.
pub fn actor_key_proof_message(
    request: &RegisterRequest,
    operation: ActorKeyProofOperation,
) -> Result<Vec<u8>, ActorKeyProofError> {
    let actor_public_key = request
        .actor_public_key
        .as_ref()
        .ok_or(ActorKeyProofError::MissingPublicKey)?;
    if actor_public_key.len() != 32 {
        return Err(ActorKeyProofError::InvalidPublicKeyLength(
            actor_public_key.len(),
        ));
    }
    let actor_public_key: [u8; 32] = actor_public_key
        .as_ref()
        .try_into()
        .expect("length checked above");
    VerifyingKey::from_bytes(&actor_public_key)
        .map_err(|error| ActorKeyProofError::InvalidPublicKey(error.to_string()))?;

    let mut canonical_request = request.clone();
    canonical_request.actor_key_proof = None;
    let request_digest = Sha256::digest(canonical_request.encode_to_vec());

    let mut message = Vec::with_capacity(256);
    message.extend_from_slice(ACTOR_KEY_PROOF_DOMAIN);
    append_field(&mut message, operation.as_bytes());
    append_field(&mut message, request.realm.as_bytes());
    append_actr_type(&mut message, &request.actr_type);
    append_field(&mut message, &actor_public_key);
    message.extend_from_slice(&request_digest);
    Ok(message)
}

/// Set `actor_public_key` and sign the registration proof with `signing_key`.
///
/// This helper never reads or changes `signing_public_key`; package Publisher
/// authentication remains independent.
pub fn sign_actor_key_proof(
    request: &mut RegisterRequest,
    operation: ActorKeyProofOperation,
    signing_key: &SigningKey,
) {
    request.actor_public_key = Some(Bytes::copy_from_slice(
        signing_key.verifying_key().as_bytes(),
    ));
    request.actor_key_proof = None;
    let message = actor_key_proof_message(request, operation)
        .expect("a generated Ed25519 verifying key is valid");
    request.actor_key_proof = Some(Bytes::copy_from_slice(
        &signing_key.sign(&message).to_bytes(),
    ));
}

/// Verify the request's Actor-key proof with its declared Actor public key.
pub fn verify_actor_key_proof(
    request: &RegisterRequest,
    operation: ActorKeyProofOperation,
) -> Result<(), ActorKeyProofError> {
    let message = actor_key_proof_message(request, operation)?;
    let public_key = request
        .actor_public_key
        .as_ref()
        .expect("actor_key_proof_message checked presence");
    let public_key: &[u8; 32] = public_key
        .as_ref()
        .try_into()
        .expect("actor_key_proof_message checked length");
    let verifying_key = VerifyingKey::from_bytes(public_key)
        .map_err(|error| ActorKeyProofError::InvalidPublicKey(error.to_string()))?;
    let proof = request
        .actor_key_proof
        .as_ref()
        .ok_or(ActorKeyProofError::MissingProof)?;
    if proof.len() != 64 {
        return Err(ActorKeyProofError::InvalidProofLength(proof.len()));
    }
    let signature = Signature::from_slice(proof)
        .map_err(|_| ActorKeyProofError::InvalidProofLength(proof.len()))?;
    verifying_key
        .verify(&message, &signature)
        .map_err(|_| ActorKeyProofError::InvalidSignature)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> RegisterRequest {
        RegisterRequest {
            actr_type: ActrType {
                publisher: "example.com".into(),
                name: "echo".into(),
                version: "1.2.3".into(),
            },
            realm: "actors@example.com".into(),
            service_spec: None,
            acl: None,
            service: Some("Echo:fingerprint".into()),
            ws_address: None,
            manifest_raw: Some(Bytes::from_static(b"manifest")),
            publisher_signature: Some(Bytes::from_static(b"publisher-signature")),
            target: Some("wasm32-wasip1".into()),
            auth_mode: None,
            signing_public_key: Some(Bytes::from_static(&[7; 32])),
            actor_public_key: None,
            actor_key_proof: None,
        }
    }

    #[test]
    fn proof_verifies_and_does_not_reuse_publisher_key() {
        let actor_key = SigningKey::from_bytes(&[11; 32]);
        let mut request = request();
        let publisher_key = request.signing_public_key.clone();
        sign_actor_key_proof(&mut request, ActorKeyProofOperation::Register, &actor_key);

        verify_actor_key_proof(&request, ActorKeyProofOperation::Register).unwrap();
        assert_eq!(request.signing_public_key, publisher_key);
        assert_ne!(request.actor_public_key, request.signing_public_key);
    }

    #[test]
    fn proof_rejects_request_operation_and_key_substitution() {
        let actor_key = SigningKey::from_bytes(&[11; 32]);
        let other_key = SigningKey::from_bytes(&[12; 32]);
        let mut request = request();
        sign_actor_key_proof(&mut request, ActorKeyProofOperation::Register, &actor_key);

        assert_eq!(
            verify_actor_key_proof(&request, ActorKeyProofOperation::Reissue),
            Err(ActorKeyProofError::InvalidSignature)
        );
        let mut changed = request.clone();
        changed.realm = "other@example.com".into();
        assert_eq!(
            verify_actor_key_proof(&changed, ActorKeyProofOperation::Register),
            Err(ActorKeyProofError::InvalidSignature)
        );
        let mut changed = request.clone();
        changed.actr_type.name = "other".into();
        assert_eq!(
            verify_actor_key_proof(&changed, ActorKeyProofOperation::Register),
            Err(ActorKeyProofError::InvalidSignature)
        );
        let mut changed = request.clone();
        changed.manifest_raw = Some(Bytes::from_static(b"other-manifest"));
        assert_eq!(
            verify_actor_key_proof(&changed, ActorKeyProofOperation::Register),
            Err(ActorKeyProofError::InvalidSignature)
        );
        let mut changed = request;
        changed.actor_public_key =
            Some(Bytes::copy_from_slice(other_key.verifying_key().as_bytes()));
        assert_eq!(
            verify_actor_key_proof(&changed, ActorKeyProofOperation::Register),
            Err(ActorKeyProofError::InvalidSignature)
        );
    }

    #[test]
    fn missing_and_malformed_proofs_are_rejected() {
        let mut request = request();
        assert_eq!(
            verify_actor_key_proof(&request, ActorKeyProofOperation::Register),
            Err(ActorKeyProofError::MissingPublicKey)
        );
        request.actor_public_key = Some(Bytes::from_static(&[0; 31]));
        assert_eq!(
            verify_actor_key_proof(&request, ActorKeyProofOperation::Register),
            Err(ActorKeyProofError::InvalidPublicKeyLength(31))
        );

        let actor_key = SigningKey::from_bytes(&[11; 32]);
        request.actor_public_key =
            Some(Bytes::copy_from_slice(actor_key.verifying_key().as_bytes()));
        assert_eq!(
            verify_actor_key_proof(&request, ActorKeyProofOperation::Register),
            Err(ActorKeyProofError::MissingProof)
        );
        request.actor_key_proof = Some(Bytes::from_static(&[0; 63]));
        assert_eq!(
            verify_actor_key_proof(&request, ActorKeyProofOperation::Register),
            Err(ActorKeyProofError::InvalidProofLength(63))
        );
    }
}
