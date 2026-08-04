//! AId Credential verifier
//!
//! Responsible for verifying the Ed25519 signature of AId Credential and decoding IdentityClaims.
//! Compared to the old ECIES decryption approach, the new scheme uses asymmetric signature verification:
//! - claims are plaintext proto bytes, signed by the AIS Ed25519 private key
//! - The verifier only needs the corresponding verifying key (public key) to complete verification

use super::error::AidError;
use actr_protocol::prost::Message as ProstMessage;
use actr_protocol::{AIdCredential, IdentityClaims};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};

/// AId Credential verifier
///
/// Provides pure-function Ed25519 signature verification with no internal state.
/// The verifying_key is provided by the caller, typically from the signing_pubkey in the AIS registration response or the local key cache.
pub struct AIdCredentialVerifier;

impl AIdCredentialVerifier {
    /// Verify the Ed25519 signature of AIdCredential and return IdentityClaims
    ///
    /// # Verification flow
    /// 1. Parse 64-byte Ed25519 signature from credential.signature
    /// 2. Verify signature against credential.claims (plaintext proto bytes) using verifying_key
    /// 3. Proto decode IdentityClaims
    /// 4. Check if expires_at has expired
    ///
    /// # Arguments
    /// * `credential` - AIdCredential to verify, with fields key_id / claims / signature
    /// * `verifying_key` - Ed25519 public key provided by the caller (from AIS registration response or key cache)
    ///
    /// # Returns
    /// * `Ok(IdentityClaims)` - Signature is valid and not expired, returns decoded identity claims
    /// * `Err(AidError)` - Invalid signature, malformed, or token expired
    pub fn verify(
        credential: &AIdCredential,
        verifying_key: &VerifyingKey,
    ) -> Result<IdentityClaims, AidError> {
        crate::recording::debug!(
            "Verifying AIdCredential Ed25519 signature: key_id={}, claims_len={}",
            credential.key_id,
            credential.claims.len()
        );

        // 1. Parse signature (must be exactly 64 bytes)
        let sig_bytes: [u8; 64] = credential.signature[..].try_into().map_err(|_| {
            AidError::InvalidSignature(format!(
                "Invalid signature length: expected 64 bytes, got {} bytes",
                credential.signature.len()
            ))
        })?;
        let signature = Signature::from_bytes(&sig_bytes);

        // 2. Verify signature (claims are the signed plaintext bytes)
        verifying_key
            .verify(&credential.claims[..], &signature)
            .map_err(|e| {
                AidError::InvalidSignature(format!("Ed25519 signature verification failed: {e}"))
            })?;

        // 3. Proto decode IdentityClaims
        let claims = IdentityClaims::decode(&credential.claims[..]).map_err(|e| {
            AidError::DecodeFailure(format!("IdentityClaims proto decode failed: {e}"))
        })?;

        // 4. Check expiration
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if claims.expires_at <= now {
            return Err(AidError::Expired);
        }

        crate::recording::debug!(
            "AIdCredential verification passed: key_id={}, actor_id={}, expires_at={}",
            credential.key_id,
            claims.actor_id,
            claims.expires_at
        );

        Ok(claims)
    }
}
