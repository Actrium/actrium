//! Deployment adapters for the application-independent relay core.

use actr_protocol::ActrId;
use actr_relay_security::{IdentityClaims, IssuerKey, RelayCredential, verify_certificate};
use async_trait::async_trait;
use axum::{Json, Router, http::header, routing::get};
use base64::{Engine, engine::general_purpose::STANDARD};
use loran_relay::{AuthError, Authenticator, Observer, Principal, Relay};
use loran_relay_protocol::RelayDiscovery;
use platform::{
    aid::credential::AIdCredentialValidator,
    config::{LoranConfig, RelayAuthMode, relay::RelayStaticCredential},
    monitoring::ServiceCounters,
};
use std::sync::atomic::Ordering;
use std::{
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};
use subtle::ConstantTimeEq;
use tokio_util::sync::CancellationToken;

struct CredentialAuth {
    inactive: turn::InactiveRealms,
}

#[async_trait]
impl Authenticator for CredentialAuth {
    async fn authenticate(&self, endpoint: &str, token: &str) -> Result<Principal, AuthError> {
        let identity = ActrId::from_string_repr(endpoint).map_err(|_| AuthError)?;
        if identity.to_string_repr() != endpoint {
            return Err(AuthError);
        }
        let bytes = STANDARD.decode(token).map_err(|_| AuthError)?;
        let credential: RelayCredential = serde_json::from_slice(&bytes).map_err(|_| AuthError)?;
        let certificate = &credential.certificate;
        let claims: IdentityClaims =
            serde_json::from_slice(&certificate.claims).map_err(|_| AuthError)?;
        let (key, expires_at) =
            AIdCredentialValidator::get_verifying_key(&identity.realm, certificate.key_id)
                .await
                .map_err(|_| AuthError)?
                .ok_or(AuthError)?;
        let now = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| AuthError)?
            .as_secs();
        // The private admission secret is never included in the certificate
        // shared with peers and cannot be used at signaling or Issuer.
        let claims = verify_certificate(
            certificate,
            &IssuerKey {
                key_id: certificate.key_id,
                public_key: *key.as_bytes(),
                expires_at,
            },
            endpoint,
            &claims.public_key,
            now,
        )
        .map_err(|_| AuthError)?;
        credential
            .verify_admission(&claims)
            .map_err(|_| AuthError)?;
        // This value is the server's actual key validity bound, rather than
        // an SDK's shorter local verification lease.
        if claims.expires_at > expires_at {
            return Err(AuthError);
        }
        let actor_type =
            actr_protocol::ActrType::from_string_repr(&claims.actor_type).map_err(|_| AuthError)?;
        let principal = Principal {
            namespace: identity.realm.clone(),
            endpoint: endpoint.to_owned(),
            metadata: claims.client_key,
            authorization_context: actor_type.to_string_repr(),
            expires_at: Some(
                UNIX_EPOCH
                    .checked_add(Duration::from_secs(claims.expires_at))
                    .ok_or(AuthError)?,
            ),
        };
        if !self.active(&principal) {
            return Err(AuthError);
        }
        Ok(principal)
    }

    fn active(&self, principal: &Principal) -> bool {
        !self.inactive.contains(&principal.namespace)
            && !self.inactive.contains_actor(&principal.endpoint)
            && self
                .inactive
                .matches_binding(&principal.endpoint, &principal.metadata)
    }

    fn authorize(&self, source: &Principal, target: &Principal) -> bool {
        self.active(source) && self.active(target)
    }

    async fn authorize_pair(&self, source: &Principal, target: &Principal) -> bool {
        self.authorize(source, target)
            && platform::realm::acl::ActorAcl::can_discover(
                &source.namespace,
                &target.namespace,
                &source.authorization_context,
                &target.authorization_context,
            )
            .await
            .unwrap_or(false)
    }
}

struct StaticAuth(Vec<RelayStaticCredential>);

struct RelayMetrics(Option<Arc<ServiceCounters>>);
impl Observer for RelayMetrics {
    fn connected(&self) {
        if let Some(counters) = &self.0 {
            counters.inc_conns();
        }
    }
    fn disconnected(&self) {
        if let Some(counters) = &self.0 {
            counters.dec_conns();
        }
    }
    fn authentication_failed(&self) {
        if let Some(counters) = &self.0 {
            counters.total_requests.fetch_add(1, Ordering::Relaxed);
            counters.failed_requests.fetch_add(1, Ordering::Relaxed);
        }
    }
    fn received_frame(&self) {
        if let Some(counters) = &self.0 {
            counters.total_requests.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[async_trait]
impl Authenticator for StaticAuth {
    async fn authenticate(&self, endpoint: &str, token: &str) -> Result<Principal, AuthError> {
        let credential = self
            .0
            .iter()
            .find(|entry| entry.endpoint == endpoint)
            .ok_or(AuthError)?;
        if !bool::from(credential.token.as_bytes().ct_eq(token.as_bytes())) {
            return Err(AuthError);
        }
        Ok(Principal {
            namespace: credential.namespace.clone(),
            endpoint: endpoint.to_owned(),
            metadata: String::new(),
            authorization_context: String::new(),
            expires_at: None,
        })
    }
}

pub async fn build_relay_router(
    config: &LoranConfig,
    cancel: CancellationToken,
    counters: Option<Arc<ServiceCounters>>,
) -> anyhow::Result<Router> {
    let discovery = RelayDiscovery {
        version: 1,
        enabled: config.is_relay_enabled(),
        url: config.relay.public_url.clone(),
    };
    let mut router = Router::new().route(
        "/config",
        get(move || async move {
            (
                [(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")],
                Json(discovery),
            )
        }),
    );
    if config.is_relay_enabled() {
        let mut watch = None;
        let auth: Arc<dyn Authenticator> = match config.relay.auth {
            RelayAuthMode::Aid => {
                AIdCredentialValidator::init(&config.sqlite_path).await?;
                let inactive = turn::InactiveRealms::default();
                let guard = inactive.watch().await;
                watch = Some((inactive.clone(), guard));
                Arc::new(CredentialAuth { inactive })
            }
            RelayAuthMode::Static => Arc::new(StaticAuth(config.relay.static_credentials.clone())),
        };
        let relay = Relay::with_observer(
            config.relay.limits.clone(),
            auth,
            cancel.clone(),
            Arc::new(RelayMetrics(counters)),
        )
        .map_err(anyhow::Error::msg)?;
        if let Some((inactive, guard)) = watch {
            let relay = relay.clone();
            let cancel = cancel.clone();
            tokio::spawn(async move {
                let _guard = guard;
                loop {
                    tokio::select! {
                        _ = cancel.cancelled() => break,
                        _ = inactive.deactivated() => { relay.disconnect_inactive(); },
                    }
                }
            });
        }
        router = router.merge(relay.router());
    }
    Ok(router)
}

#[cfg(test)]
mod tests {
    use super::*;
    use actr_protocol::{AIdCredential, prost::Message};
    use actr_relay_security::{CERTIFICATE_DOMAIN, Certificate, IdentityKey, signing_bytes};
    use ed25519_dalek::{Signer, SigningKey};

    #[tokio::test]
    async fn aid_adapter_binds_scoped_certificates_to_canonical_endpoint_and_realm() {
        let directory = tempfile::tempdir().unwrap();
        AIdCredentialValidator::init(directory.path())
            .await
            .unwrap();
        let key = SigningKey::from_bytes(&[47; 32]);
        let now = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        AIdCredentialValidator::populate_key(
            "test@example.com",
            1,
            &key.verifying_key(),
            now + 600,
        )
        .await
        .unwrap();
        let endpoint = "1.test@example.com";
        let inactive = turn::InactiveRealms::default();
        inactive.bind_actor(endpoint, "fixture-key");
        let adapter = CredentialAuth { inactive };
        let identity = IdentityKey::generate().unwrap();
        let credential = |domain: &str, endpoint: &str, expires_at| {
            let claims = IdentityClaims {
                domain: domain.into(),
                endpoint: endpoint.into(),
                actor_type: "example/Client@1.0.0".into(),
                client_key: "fixture-key".into(),
                public_key: identity.public,
                admission_commitment: identity.admission_commitment(),
                expires_at,
            };
            let claims = serde_json::to_vec(&claims).unwrap();
            STANDARD.encode(
                serde_json::to_vec(&identity.relay_credential(Certificate {
                    key_id: 1,
                    signature: key.sign(&signing_bytes(&claims)).to_bytes().to_vec(),
                    claims,
                }))
                .unwrap(),
            )
        };
        let token = credential(CERTIFICATE_DOMAIN, endpoint, now + 60);
        let principal = adapter.authenticate(endpoint, &token).await.unwrap();
        assert_eq!(principal.endpoint, endpoint);
        assert_eq!(principal.namespace, "test@example.com");
        assert_eq!(
            principal.expires_at,
            Some(UNIX_EPOCH + Duration::from_secs(now + 60))
        );
        for endpoint in [
            "2.test@example.com",
            "01.test@example.com",
            "1.other@example.com",
        ] {
            assert!(adapter.authenticate(endpoint, &token).await.is_err());
        }
        for token in [
            "invalid token".into(),
            credential("OTHER-PROTOCOL", endpoint, now + 60),
            credential(CERTIFICATE_DOMAIN, endpoint, now),
            credential(CERTIFICATE_DOMAIN, endpoint, now - 1),
            credential(CERTIFICATE_DOMAIN, endpoint, now + 601),
        ] {
            assert!(adapter.authenticate(endpoint, &token).await.is_err());
        }
        let mut forged: RelayCredential =
            serde_json::from_slice(&STANDARD.decode(&token).unwrap()).unwrap();
        forged.certificate.signature = vec![0; 64];
        assert!(
            adapter
                .authenticate(
                    endpoint,
                    &STANDARD.encode(serde_json::to_vec(&forged).unwrap())
                )
                .await
                .is_err()
        );
        let claims = actr_protocol::IdentityClaims {
            realm: "test@example.com".into(),
            issuer: "test@example.com".into(),
            provider: "provider.example".into(),
            audience: "actr.signaling".into(),
            protocol_version: 1,
            actor_id: endpoint.into(),
            expires_at: now + 60,
            ..Default::default()
        }
        .encode_to_vec();
        let aid = AIdCredential {
            key_id: 1,
            signature: key.sign(&claims).to_bytes().to_vec().into(),
            claims: claims.into(),
        };
        assert!(
            AIdCredentialValidator::check(
                &aid,
                "test@example.com",
                "provider.example",
                "actr.signaling"
            )
            .await
            .is_ok()
        );
        assert!(
            adapter
                .authenticate(endpoint, &STANDARD.encode(aid.encode_to_vec()))
                .await
                .is_err()
        );
        // Conversely the relay certificate is never a valid signaling bearer.
        if let Ok(aid) = AIdCredential::decode(STANDARD.decode(&token).unwrap().as_slice()) {
            assert!(
                AIdCredentialValidator::check(
                    &aid,
                    "test@example.com",
                    "provider.example",
                    "actr.signaling"
                )
                .await
                .is_err()
            );
        }
    }
}
