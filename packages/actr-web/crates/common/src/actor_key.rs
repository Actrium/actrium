//! Independent, long-term Actor Ed25519 identity for browser workloads.
//!
//! The private key belongs in an origin's persistent IndexedDB secret store.
//! It is neither the Realm client key nor the package Publisher signing key.

use actr_protocol::actor_key_proof::{
    ActorKeyProofOperation, sign_actor_key_proof, verify_actor_key_proof,
};
use actr_protocol::prost::Message as _;
use actr_protocol::{AIdCredential, IdentityClaims, RegisterRequest};
use ed25519_dalek::SigningKey;

use crate::error::{WebError, WebResult};

/// Key within the Service Worker's persistent IndexedDB Actor-key namespace.
pub const ACTOR_SIGNING_KEY_STORAGE_KEY: &str = "actor_signing_key";

/// A browser Actor's persistent signing identity.
pub struct WebActorKey {
    signing_key: SigningKey,
}

impl WebActorKey {
    /// Generate a new key with the browser's cryptographically secure RNG.
    /// Callers must persist it before registration and reuse it thereafter.
    pub fn generate() -> WebResult<Self> {
        let mut private = [0u8; 32];
        getrandom::getrandom(&mut private).map_err(|error| {
            WebError::Protocol(format!("Actor signing randomness unavailable: {error}"))
        })?;
        Ok(Self::from_private(private))
    }

    /// Restore a key previously read from durable storage.
    pub fn from_bytes(bytes: &[u8]) -> WebResult<Self> {
        let private: [u8; 32] = bytes.try_into().map_err(|_| {
            WebError::Protocol(format!(
                "persisted Actor private key must be 32 bytes, got {}",
                bytes.len()
            ))
        })?;
        Ok(Self::from_private(private))
    }

    fn from_private(private: [u8; 32]) -> Self {
        Self {
            signing_key: SigningKey::from_bytes(&private),
        }
    }

    /// Raw private-key bytes for the IndexedDB secret store. Never log these.
    pub fn private_key_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    /// Attach and locally verify a REGISTER proof. Publisher authentication is
    /// deliberately left untouched, including `signing_public_key`.
    pub fn sign_register_request(&self, request: &mut RegisterRequest) -> WebResult<()> {
        sign_actor_key_proof(request, ActorKeyProofOperation::Register, &self.signing_key);
        verify_actor_key_proof(request, ActorKeyProofOperation::Register)
            .map_err(|error| WebError::Protocol(format!("local Actor proof rejected: {error}")))
    }

    /// Require an Issuer credential to be bound to this exact Actor key.
    pub fn accepts_credential(&self, credential: &AIdCredential) -> bool {
        IdentityClaims::decode(credential.claims.as_ref()).is_ok_and(|claims| {
            claims.actor_public_key.as_deref() == Some(self.public_key().as_slice())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actr_protocol::{ActrType, RegisterAuthMode};

    fn request() -> RegisterRequest {
        RegisterRequest {
            actr_type: ActrType {
                publisher: "example.com".into(),
                name: "browser".into(),
                version: "1.0.0".into(),
            },
            realm: "actors@example.com".into(),
            signing_public_key: Some(vec![91; 32].into()),
            auth_mode: Some(RegisterAuthMode::Linked as i32),
            ..Default::default()
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn request_proof_is_valid_and_independent_of_publisher_key() {
        let actor = WebActorKey::from_bytes(&[23; 32]).unwrap();
        let mut request = request();
        let publisher_key = request.signing_public_key.clone();
        actor.sign_register_request(&mut request).unwrap();

        assert_eq!(
            request.actor_public_key.as_deref(),
            Some(actor.public_key().as_slice())
        );
        assert_eq!(
            request.actor_key_proof.as_ref().map(|proof| proof.len()),
            Some(64)
        );
        assert_eq!(request.signing_public_key, publisher_key);
        assert_ne!(request.actor_public_key, request.signing_public_key);
        assert!(verify_actor_key_proof(&request, ActorKeyProofOperation::Register).is_ok());

        request.realm.push_str(".changed");
        assert!(verify_actor_key_proof(&request, ActorKeyProofOperation::Register).is_err());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn restored_key_round_trip_and_malformed_key_behavior() {
        let actor = WebActorKey::from_bytes(&[37; 32]).unwrap();
        let restored = WebActorKey::from_bytes(&actor.private_key_bytes()).unwrap();
        assert_eq!(restored.public_key(), actor.public_key());
        assert!(WebActorKey::from_bytes(&[0; 31]).is_err());
    }
}
