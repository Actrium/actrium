//! Opt-in relay fixture with the same signed-credential binding as Loran.
use actr_protocol::ActrId;
use actr_relay_security::{IdentityClaims, IssuerKey, RelayCredential, verify_certificate};
use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::STANDARD};
use loran_relay::{AuthError, Authenticator, Principal};
use std::{
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};

pub(crate) struct RelayAuth(pub Arc<crate::MockState>);

#[async_trait]
impl Authenticator for RelayAuth {
    async fn authenticate(&self, endpoint: &str, token: &str) -> Result<Principal, AuthError> {
        let identity = ActrId::from_string_repr(endpoint).map_err(|_| AuthError)?;
        if identity.to_string_repr() != endpoint {
            return Err(AuthError);
        }
        let raw = STANDARD.decode(token).map_err(|_| AuthError)?;
        let credential: RelayCredential = serde_json::from_slice(&raw).map_err(|_| AuthError)?;
        let certificate = &credential.certificate;
        let claims: IdentityClaims =
            serde_json::from_slice(&certificate.claims).map_err(|_| AuthError)?;
        let now = chrono::Utc::now().timestamp() as u64;
        let claims = verify_certificate(
            certificate,
            &IssuerKey {
                key_id: self.0.issuer_signing_key_id(),
                public_key: *self.0.issuer_signing_key().verifying_key().as_bytes(),
                expires_at: now + 3600,
            },
            endpoint,
            &claims.public_key,
            now,
        )
        .map_err(|_| AuthError)?;
        credential
            .verify_admission(&claims)
            .map_err(|_| AuthError)?;
        Ok(Principal {
            namespace: identity.realm.clone(),
            endpoint: endpoint.into(),
            metadata: String::new(),
            authorization_context: String::new(),
            expires_at: Some(
                UNIX_EPOCH
                    .checked_add(Duration::from_secs(claims.expires_at))
                    .ok_or(AuthError)?,
            ),
        })
    }
}
