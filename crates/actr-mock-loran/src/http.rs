//! HTTP handlers for the mock loran server.
//!
//! Covers the REST endpoints that `actr-hyper` (`IssuerClient`, `PublisherCertCache`)
//! talk to, plus admin seeding routes
//! used by integration test scripts.

use std::sync::Arc;

use actr_protocol::prost::Message as ProstMessage;
use actr_protocol::{
    RegisterRequest, RegisterResponse, RenewCredentialRequest, RenewCredentialResponse,
    register_response, renew_credential_response,
};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use base64::Engine;
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};

use crate::signaling;
use crate::state::{MockState, PublisherEntry, RegisteredActor};

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

pub async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

// ---------------------------------------------------------------------------
// Issuer `/register` — protobuf in, protobuf out.
// ---------------------------------------------------------------------------

/// `POST /register` and `POST /issuer/register`.
///
/// Shares the credential/ID allocation path with the WebSocket signaling
/// handler via [`signaling::build_register_ok`].
pub async fn register_handler(
    State(state): State<Arc<MockState>>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let req = RegisterRequest::decode(body.as_ref())
        .map_err(|e| ApiError::bad_request(format!("protobuf decode failed: {e}")))?;
    let req = signaling::ensure_actor_key_proof(req);

    // Lazily register the realm so callers don't have to pre-seed.
    state.add_realm(req.realm.clone()).await;

    let register_ok = signaling::build_register_ok(&req, &state).await;

    // Track the HTTP-registered actor in the WS registry so route discovery
    // works once the actor opens its authenticated WebSocket. We record an
    // empty `client_id` placeholder that the WS upgrade handler fills in when
    // the peer connects.
    state.registry.write().await.push(RegisteredActor {
        actr_id: register_ok.actr_id.clone(),
        actr_type: req.actr_type.clone(),
        client_id: String::new(),
        ws_address: req.ws_address.clone(),
        service_spec: req.service_spec.clone(),
        registration: req.clone(),
    });

    let response = RegisterResponse {
        result: Some(register_response::Result::Success(register_ok.clone())),
    };

    tracing::info!(
        serial = register_ok.actr_id.serial_number,
        publisher = req.actr_type.publisher,
        name = req.actr_type.name,
        "mock-loran: registered actor (http)"
    );

    let bytes = response.encode_to_vec();
    Ok((
        StatusCode::OK,
        [("content-type", "application/x-protobuf")],
        bytes,
    )
        .into_response())
}

// ---------------------------------------------------------------------------
// Issuer `/renew` — protobuf in, protobuf out.
// ---------------------------------------------------------------------------

/// `POST /issuer/renew` and `POST /renew`.
///
/// Validates the renewal token against the actor's expected mock renewal token
/// (derived from the serial number), then re-issues credentials with the same
/// `ActrId`.
pub async fn renew_handler(
    State(state): State<Arc<MockState>>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let req = RenewCredentialRequest::decode(body.as_ref())
        .map_err(|e| ApiError::bad_request(format!("protobuf decode failed: {e}")))?;

    let actor_id = &req.actr_id;

    // Derive the expected mock renewal token from the serial number.
    let expected_token = signaling::mock_renewal_token(actor_id.serial_number);

    if req.renewal_token.as_ref() != expected_token.as_slice() {
        tracing::warn!(
            serial = actor_id.serial_number,
            "mock-loran: renewal token rejected"
        );
        let err_response = RenewCredentialResponse {
            result: Some(renew_credential_response::Result::Error(
                actr_protocol::ErrorResponse {
                    code: 401,
                    message: "Renewal token invalid or expired".to_string(),
                },
            )),
        };
        let bytes = err_response.encode_to_vec();
        return Ok((
            StatusCode::UNAUTHORIZED,
            [("content-type", "application/x-protobuf")],
            bytes,
        )
            .into_response());
    }

    // Reuse the normalized registration so renewal retains the original Actor
    // key and its valid proof. Unknown actors get the same real-key fallback as
    // a fresh mock registration.
    let fake_req = state
        .registry
        .read()
        .await
        .iter()
        .find(|entry| entry.actr_id == *actor_id)
        .map(|entry| entry.registration.clone())
        .unwrap_or_else(|| {
            signaling::ensure_actor_key_proof(RegisterRequest {
                realm: actor_id.realm.clone(),
                ..Default::default()
            })
        });

    let mut register_ok = signaling::build_register_ok(&fake_req, &state).await;
    // Preserve the original ActrId (mock assigns a new serial, so override).
    register_ok.actr_id = actor_id.clone();
    // Keep the mock renewal token bound to the preserved ActrId, not to the
    // temporary serial allocated by build_register_ok().
    register_ok.renewal_token = Some(expected_token.into());

    let response = RenewCredentialResponse {
        result: Some(renew_credential_response::Result::Success(
            register_ok.clone(),
        )),
    };

    tracing::info!(
        serial = register_ok.actr_id.serial_number,
        "mock-loran: renewed credentials (http)"
    );

    let bytes = response.encode_to_vec();
    Ok((
        StatusCode::OK,
        [("content-type", "application/x-protobuf")],
        bytes,
    )
        .into_response())
}

// ---------------------------------------------------------------------------
// Publisher Registry verifying key — `GET /registry/publishers/{name}/verifying-key?key_id=X`
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct VerifyingKeyQuery {
    // Issuer allows callers to pin a specific key_id; we don't key-rotate in the
    // mock but we accept the parameter for protocol compatibility.
    pub key_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct VerifyingKeyResponse {
    public_key: String,
}

pub async fn verifying_key_handler(
    State(state): State<Arc<MockState>>,
    Path(name): Path<String>,
    Query(_query): Query<VerifyingKeyQuery>,
) -> Result<Response, ApiError> {
    let key = match state.publisher_verifying_key(&name).await {
        Some(k) => k,
        None => {
            tracing::warn!(publisher = %name, "mock-loran: Publisher Registry not registered");
            return Err(ApiError::not_found(format!(
                "publisher '{name}' not registered"
            )));
        }
    };

    let body = VerifyingKeyResponse {
        public_key: base64::engine::general_purpose::STANDARD.encode(key.to_bytes()),
    };
    Ok(Json(body).into_response())
}

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// Admin seeding — replaces register.sh sqlite3 INSERTs.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct AdminRealmRequest {
    pub realm: String,
}

#[derive(Debug, Serialize)]
pub struct AdminOk {
    pub ok: bool,
}

pub async fn admin_realm_handler(
    State(state): State<Arc<MockState>>,
    Json(req): Json<AdminRealmRequest>,
) -> Json<AdminOk> {
    state.add_realm(req.realm).await;
    Json(AdminOk { ok: true })
}

#[derive(Debug, Deserialize)]
pub struct AdminPublisherRequest {
    pub name: String,
    pub pubkey_b64: String,
    #[serde(default)]
    pub contact: Option<String>,
}

pub async fn admin_publisher_registry_handler(
    State(state): State<Arc<MockState>>,
    Json(req): Json<AdminPublisherRequest>,
) -> Result<Json<AdminOk>, ApiError> {
    let key_bytes = base64::engine::general_purpose::STANDARD
        .decode(req.pubkey_b64.trim())
        .map_err(|e| ApiError::bad_request(format!("pubkey base64 decode failed: {e}")))?;
    let arr: [u8; 32] = key_bytes.try_into().map_err(|v: Vec<u8>| {
        ApiError::bad_request(format!("pubkey must be 32 bytes, got {}", v.len()))
    })?;
    let verifying_key = VerifyingKey::from_bytes(&arr)
        .map_err(|e| ApiError::bad_request(format!("invalid ed25519 pubkey: {e}")))?;

    state.publishers.write().await.insert(
        req.name.clone(),
        PublisherEntry {
            name: req.name.clone(),
            verifying_key,
            contact: req.contact,
        },
    );
    tracing::info!(name = %req.name, "mock-loran: Publisher Registry seeded via /admin/publisher");
    Ok(Json(AdminOk { ok: true }))
}

#[derive(Debug, Serialize)]
pub struct AdminStateSnapshot {
    pub realms: Vec<String>,
    pub publishers: Vec<String>,
}

pub async fn admin_state_handler(State(state): State<Arc<MockState>>) -> Json<AdminStateSnapshot> {
    let realms: Vec<String> = state.realms.read().await.keys().cloned().collect();
    let publishers: Vec<String> = state.publishers.read().await.keys().cloned().collect();
    Json(AdminStateSnapshot { realms, publishers })
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: String) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message,
        }
    }

    fn not_found(message: String) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = serde_json::json!({ "error": self.message });
        (self.status, Json(body)).into_response()
    }
}

pub async fn relay_identity_handler(
    State(state): State<Arc<MockState>>,
    Json(request): Json<actr_relay_security::IdentityRequest>,
) -> Result<Json<actr_relay_security::IssuedIdentity>, StatusCode> {
    if state
        .relay_enrollment_unavailable
        .load(std::sync::atomic::Ordering::Relaxed)
    {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    use actr_relay_security::*;
    use ed25519_dalek::Signer;
    let actor = actr_protocol::ActrId::from_string_repr(&request.endpoint)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    if actor.to_string_repr() != request.endpoint
        || request.public_key == [0; 32]
        || request.admission_commitment == [0; 32]
        || request.renewal_token != signaling::mock_renewal_token(actor.serial_number)
    {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let actor_type = state
        .registry
        .read()
        .await
        .iter()
        .find(|entry| entry.actr_id == actor)
        .map(|entry| entry.actr_type.to_string_repr())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let actor_certificate = issue_actor_certificate(&state, &request, &actor, &actor_type, now);
    let claims = serde_json::to_vec(&IdentityClaims {
        domain: CERTIFICATE_DOMAIN.into(),
        endpoint: request.endpoint,
        actor_type,
        client_key: "mock-client-key".into(),
        public_key: request.public_key,
        admission_commitment: request.admission_commitment,
        expires_at: now + CERTIFICATE_TTL,
    })
    .unwrap();
    let signature = state
        .issuer_signing_key()
        .sign(&signing_bytes(&claims))
        .to_bytes()
        .to_vec();
    Ok(Json(IssuedIdentity {
        certificate: Certificate {
            key_id: state.issuer_signing_key_id(),
            claims,
            signature,
        },
        issuer: IssuerKey {
            key_id: state.issuer_signing_key_id(),
            public_key: *state.issuer_signing_key().verifying_key().as_bytes(),
            expires_at: now + CERTIFICATE_TTL,
        },
        actor_certificate,
    }))
}

/// Mint an Actor channel certificate when the test enabled them and the
/// enrollment carries a decodable Actor credential. Missing or malformed
/// material degrades to the legacy response instead of failing enrollment.
fn issue_actor_certificate(
    state: &MockState,
    request: &actr_relay_security::IdentityRequest,
    actor: &actr_protocol::ActrId,
    actor_type: &str,
    now: u64,
) -> Option<actr_relay_security::ActorChannelCertificate> {
    use actr_protocol::prost::Message as _;
    use actr_relay_security::*;
    if !state
        .actor_certificates_enabled
        .load(std::sync::atomic::Ordering::Relaxed)
    {
        return None;
    }
    let enrollment = request.actor.as_ref()?;
    let credential =
        actr_protocol::AIdCredential::decode(enrollment.actor_credential.as_slice()).ok()?;
    let credential_claims =
        actr_protocol::IdentityClaims::decode(credential.claims.as_ref()).ok()?;
    let actor_public_key: [u8; 32] = credential_claims
        .actor_public_key
        .as_deref()?
        .try_into()
        .ok()?;
    let role = actr_protocol::MemberRole::try_from(credential_claims.role).ok()?;
    let claims = ActorChannelClaims {
        realm: actor.realm.clone(),
        actor_id: request.endpoint.clone(),
        actor_type: actor_type.to_string(),
        member: credential_claims.member,
        client_key: credential_claims.client_key,
        role: format!("{role:?}").to_lowercase(),
        actor_public_key,
        noise_public_key: request.public_key,
        actor_credential_digest: enrollment.actor_credential_digest,
        policy_digest: enrollment.policy_digest,
        actor_state_digest: enrollment.actor_state_digest,
        issuer: actor.realm.clone(),
        provider: "mock-provider".into(),
        audience: ACTOR_CHANNEL_AUDIENCE.into(),
        expires_at: now + CERTIFICATE_TTL,
        runtime_instance_id: enrollment.runtime_instance_id.clone(),
    };
    let certificate = ActorChannelCertificate::issue(
        &claims,
        state.issuer_signing_key_id(),
        state.issuer_signing_key(),
    )
    .ok();
    if certificate.is_none() {
        tracing::warn!("mock-loran: Actor channel certificate issuance failed");
    }
    certificate
}

pub async fn relay_key_handler(
    State(state): State<Arc<MockState>>,
    Path((_realm, key_id)): Path<(String, u32)>,
) -> Result<Json<actr_relay_security::IssuerKey>, StatusCode> {
    if key_id != state.issuer_signing_key_id() {
        return Err(StatusCode::NOT_FOUND);
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    Ok(Json(actr_relay_security::IssuerKey {
        key_id,
        public_key: *state.issuer_signing_key().verifying_key().as_bytes(),
        expires_at: now + actr_relay_security::CERTIFICATE_TTL,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actr_protocol::actor_key_proof::{ActorKeyProofOperation, verify_actor_key_proof};
    use actr_protocol::{ActrType, IdentityClaims};
    use ed25519_dalek::SigningKey;
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn renewal_preserves_the_registered_actor_key() {
        let state = Arc::new(MockState::new(
            SigningKey::from_bytes(&[42; 32]),
            7,
            SigningKey::from_bytes(&[43; 32]),
            CancellationToken::new(),
        ));
        let request = RegisterRequest {
            actr_type: ActrType {
                publisher: "mock".into(),
                name: "echo".into(),
                version: "1.0.0".into(),
            },
            realm: "mock@example.com".into(),
            ..Default::default()
        };
        let response = register_handler(State(state.clone()), Bytes::from(request.encode_to_vec()))
            .await
            .unwrap();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let initial = match RegisterResponse::decode(body).unwrap().result.unwrap() {
            register_response::Result::Success(ok) => ok,
            register_response::Result::Error(error) => panic!("register failed: {error:?}"),
        };
        let initial_claims = IdentityClaims::decode(&initial.credential.claims[..]).unwrap();

        let response = renew_handler(
            State(state.clone()),
            Bytes::from(
                RenewCredentialRequest {
                    actr_id: initial.actr_id.clone(),
                    renewal_token: initial.renewal_token.clone().unwrap(),
                    attempt_id: Default::default(),
                }
                .encode_to_vec(),
            ),
        )
        .await
        .unwrap();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let renewed = match RenewCredentialResponse::decode(body)
            .unwrap()
            .result
            .unwrap()
        {
            renew_credential_response::Result::Success(ok) => ok,
            renew_credential_response::Result::Error(error) => panic!("renew failed: {error:?}"),
        };
        let renewed_claims = IdentityClaims::decode(&renewed.credential.claims[..]).unwrap();

        assert_eq!(renewed.actr_id, initial.actr_id);
        assert_eq!(
            renewed_claims.actor_public_key,
            initial_claims.actor_public_key
        );
        let registry = state.registry.read().await;
        let registration = &registry[0].registration;
        verify_actor_key_proof(registration, ActorKeyProofOperation::Register).unwrap();
        assert_eq!(
            registration.actor_public_key,
            initial_claims.actor_public_key
        );
    }
}
