//! Issuer (Actor Identity Service) HTTP Handler

use crate::{idempotency::ControlAttemptStore, issuer::AIdIssuer, ratelimit::ip_rate_limiter};
use actr_protocol::{
    ErrorResponse, RegisterRequest, RegisterResponse, ReissueCredentialRequest,
    RenewCredentialRequest, RenewCredentialResponse, register_response, renew_credential_response,
};
use axum::{
    Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::post,
};
use platform::aid::AidError;
use platform::monitoring::ServiceCounters;
use platform::realm::{
    REALM_SECRET_HEADER, Realm as RealmEntity, RealmSecretCheck, acl::ActorAcl, verify_realm_secret,
};
use prost::Message;
use serde_json::{Value, json};
use std::sync::Arc;

/// Issuer service state
#[derive(Clone)]
pub struct IssuerState {
    pub issuer: Arc<AIdIssuer>,
    #[allow(dead_code)]
    idempotency: Arc<ControlAttemptStore>,
    /// Service-level counters for metrics collection.
    pub counters: Option<Arc<ServiceCounters>>,
}

impl IssuerState {
    pub fn new(issuer: AIdIssuer) -> Self {
        Self::from_shared_issuer(Arc::new(issuer))
    }

    /// Build state around an issuer that is already shared with another
    /// consumer (e.g. the control plane holding `Arc<AIdIssuer>` for
    /// authenticated key rotation).
    pub fn from_shared_issuer(issuer: Arc<AIdIssuer>) -> Self {
        let idempotency = Arc::new(ControlAttemptStore::new(
            &issuer.config.renewal_token_secret,
        ));
        Self {
            issuer,
            idempotency,
            counters: None,
        }
    }

    pub fn with_counters(mut self, counters: Arc<ServiceCounters>) -> Self {
        self.counters = Some(counters);
        self
    }
}

/// Create the Issuer service router
///
/// Applies rate limiting middleware:
/// - IP level: 100 req/min (prevents DoS attacks from a single IP)
pub fn create_router(state: IssuerState) -> Router {
    Router::new()
        .route("/register", post(register_actr))
        .route("/renew", post(renew_credential))
        .route("/reissue", post(reissue_credential))
        .route("/health", axum::routing::get(health_check))
        .route("/current-key", axum::routing::get(get_current_key))
        .route("/signing-pubkey", axum::routing::get(get_signing_pubkey))
        .layer(ip_rate_limiter())
        .with_state(state)
}

/// ActrId registration handler - returns RegisterResponse strictly per the proto definition
/// RegisterRequest -> RegisterResponse
async fn register_actr(State(state): State<IssuerState>, headers: HeaderMap, body: Bytes) -> Bytes {
    let start = std::time::Instant::now();

    // Decode the protobuf request
    let request = match RegisterRequest::decode(body) {
        Ok(req) => req,
        Err(err) => {
            platform::recording::error!("Failed to decode protobuf request: {}", err);
            let error_result = RegisterResponse {
                result: Some(register_response::Result::Error(ErrorResponse {
                    code: 400, // Bad Request
                    message: format!("Invalid protobuf: {err}"),
                })),
            };
            return encode_result(error_result);
        }
    };

    platform::recording::debug!(
        "Received register request for realm {}, type {}",
        request.realm,
        request.actr_type.to_string_repr()
    );

    // Validate the identity fields before minting an ActrId or deriving ACL keys.
    // ActrType is a protobuf type with public fields, so a caller can construct
    // values (e.g. publisher "x:y") that collide when formatted into the same
    // type string and silently overwrite another actor's ACL rules, or that never
    // re-parse downstream. Reject early.
    if let Err(err) = request.actr_type.validate() {
        let error_result = RegisterResponse {
            result: Some(register_response::Result::Error(ErrorResponse {
                code: 400,
                message: format!("Invalid actor identity: {err}"),
            })),
        };
        return encode_result(error_result);
    }

    // Validate that the Realm exists, is in a healthy state, and has not expired
    if let Err(validation_err) = RealmEntity::validate_realm(&request.realm).await {
        let (code, message) = RealmEntity::map_validation_error(validation_err);
        let error_result = RegisterResponse {
            result: Some(register_response::Result::Error(ErrorResponse {
                code,
                message: format!("Realm validation failed: {message}"),
            })),
        };
        return encode_result(error_result);
    }

    // Registration is fail-closed: every provisioned Realm must have a secret.
    let provided_secret = headers
        .get(REALM_SECRET_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty());

    let realm_secret_check = match verify_realm_secret(&request.realm, provided_secret).await {
        Ok(check @ RealmSecretCheck::ValidCurrent)
        | Ok(check @ RealmSecretCheck::ValidPrevious) => check,
        Ok(RealmSecretCheck::NotConfigured) => {
            let error_result = RegisterResponse {
                result: Some(register_response::Result::Error(ErrorResponse {
                    code: 403,
                    message: "Realm is not provisioned with a registration secret".to_string(),
                })),
            };
            return encode_result(error_result);
        }
        Ok(RealmSecretCheck::MissingRequired) => {
            let error_result = RegisterResponse {
                result: Some(register_response::Result::Error(ErrorResponse {
                    code: 403,
                    message: "Realm secret required".to_string(),
                })),
            };
            return encode_result(error_result);
        }
        Ok(RealmSecretCheck::Invalid) => {
            let error_result = RegisterResponse {
                result: Some(register_response::Result::Error(ErrorResponse {
                    code: 403,
                    message: "Invalid realm secret".to_string(),
                })),
            };
            return encode_result(error_result);
        }
        Err(e) => {
            platform::recording::error!(
                "Failed to verify realm secret for realm {}: {}",
                request.realm,
                e
            );
            let error_result = RegisterResponse {
                result: Some(register_response::Result::Error(ErrorResponse {
                    code: 500,
                    message: "Internal error while verifying realm secret".to_string(),
                })),
            };
            return encode_result(error_result);
        }
    };

    // Invoke the issuer to issue a credential
    let result = match state
        .issuer
        .issue_credential_with_realm_secret_check(&request, Some(realm_secret_check))
        .await
    {
        Ok(response) => {
            if let Some(register_response::Result::Success(ref register_ok)) = response.result {
                platform::recording::debug!(
                    "Successfully registered ActrId: {}, type={}",
                    register_ok.actr_id,
                    request.actr_type.to_string_repr()
                );

                // ACL policy and pending signaling metadata commit together.
                if let Err(message) = persist_registration_state(&request, register_ok).await {
                    if let Some(ref ctr) = state.counters {
                        ctr.record_request(false, start.elapsed().as_secs_f64() * 1000.0)
                            .await;
                    }
                    return encode_result(RegisterResponse {
                        result: Some(register_response::Result::Error(ErrorResponse {
                            code: 500,
                            message: message.to_string(),
                        })),
                    });
                }
            }
            if let Some(ref ctr) = state.counters {
                ctr.record_request(true, start.elapsed().as_secs_f64() * 1000.0)
                    .await;
            }
            response
        }
        Err(err) => {
            platform::recording::error!("Failed to register ActrId: {}", err);
            if let Some(ref ctr) = state.counters {
                ctr.record_request(false, start.elapsed().as_secs_f64() * 1000.0)
                    .await;
            }
            RegisterResponse {
                result: Some(register_response::Result::Error(
                    aid_error_to_error_response(err),
                )),
            }
        }
    };

    encode_result(result)
}

/// Persist the registration's ACL policy and pending signaling metadata in
/// one transaction, and drop pending rows whose retention has passed.
///
/// On failure returns the client-facing error message.
async fn persist_registration_state(
    request: &RegisterRequest,
    register_ok: &register_response::RegisterOk,
) -> Result<(), &'static str> {
    const STATE_ERROR: &str = "Failed to persist registration state";

    let pool = platform::storage::db::get_database().get_pool();
    let mut tx = pool.begin().await.map_err(|error| {
        platform::recording::error!("Failed to begin registration transaction: {}", error);
        STATE_ERROR
    })?;

    if let Some(ref acl) = request.acl {
        use actr_protocol::acl_rule::SourceRealm;

        let realm = &register_ok.actr_id.realm;
        let my_type = request.actr_type.to_string_repr();

        let replacement = acl
            .rules
            .iter()
            .map(|rule| {
                let allow = rule.permission == actr_protocol::acl_rule::Permission::Allow as i32;
                let source_realm = match &rule.source_realm {
                    Some(SourceRealm::AnyRealm(_)) => None,
                    Some(SourceRealm::Realm(name)) => Some(name.clone()),
                    None => Some(realm.clone()), // fallback: treat missing as self-realm
                };
                ActorAcl::new_with_source_realm(
                    realm.clone(),
                    source_realm,
                    rule.from_type.to_string_repr(),
                    my_type.clone(),
                    allow,
                )
            })
            .collect::<Vec<_>>();

        ActorAcl::replace_for_target_in_transaction(&mut tx, realm, &my_type, &replacement)
            .await
            .map_err(|error| {
                platform::recording::error!(
                    "Failed to atomically persist ACL rules for {}: {}",
                    my_type,
                    error
                );
                "Failed to persist registration policy"
            })?;
    }

    let now = crate::renewal::now_secs();

    // Store pending registration (service_spec) for signaling to pick up.
    // It is kept as long as the renewal-token binding that /reissue accepts.
    if request.service_spec.is_some() || request.ws_address.is_some() {
        let spec_blob = request
            .service_spec
            .as_ref()
            .map(prost::Message::encode_to_vec);
        let expires_at = register_ok
            .renewal_token_expires_at
            .map(|timestamp| timestamp.seconds)
            .unwrap_or(now)
            .saturating_add(crate::renewal::REISSUE_BINDING_GRACE_SECS);
        sqlx::query(
            "INSERT OR REPLACE INTO pending_registration \
             (realm, serial_number, service_spec_blob, ws_address, created_at, expires_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&register_ok.actr_id.realm)
        .bind(register_ok.actr_id.serial_number as i64)
        .bind(spec_blob)
        .bind(request.ws_address.clone())
        .bind(now)
        .bind(expires_at)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            platform::recording::error!(
                "Failed to persist pending registration metadata: {}",
                error
            );
            "Failed to persist registration metadata"
        })?;
    }

    sqlx::query(
        "DELETE FROM pending_registration WHERE expires_at IS NOT NULL AND expires_at <= ?",
    )
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(|error| {
        platform::recording::error!(
            "Failed to clean expired pending registration metadata: {}",
            error
        );
        "Failed to persist registration metadata"
    })?;

    tx.commit().await.map_err(|error| {
        platform::recording::error!("Failed to commit registration state: {}", error);
        STATE_ERROR
    })
}

/// Keep pending signaling metadata alive for as long as the actor holds a
/// renewal token, so expiry cleanup does not drop a live registration.
async fn keep_pending_registration(
    actor_id: &actr_protocol::ActrId,
    register_ok: &register_response::RegisterOk,
) {
    let Some(expires_at) = register_ok.renewal_token_expires_at else {
        return;
    };
    if let Err(error) =
        crate::renewal::refresh_pending_registration(actor_id, expires_at.seconds).await
    {
        platform::recording::warn!(
            "Failed to extend pending registration for actor {}: {}",
            actor_id.to_string_repr(),
            error
        );
    }
}

/// POST /issuer/reissue — re-authenticate and issue credentials for an existing
/// stable ActrId without allocating a new serial number.
async fn reissue_credential(
    State(state): State<IssuerState>,
    headers: HeaderMap,
    body: Bytes,
) -> Bytes {
    let start = std::time::Instant::now();
    let request = match ReissueCredentialRequest::decode(body) {
        Ok(request) => request,
        Err(err) => {
            platform::recording::error!("Failed to decode ReissueCredentialRequest: {}", err);
            return encode_result(RegisterResponse {
                result: Some(register_response::Result::Error(ErrorResponse {
                    code: 400,
                    message: format!("Invalid protobuf: {err}"),
                })),
            });
        }
    };

    // Validate the identity fields before any signing path (same guard as /register).
    if let Err(err) = request.registration.actr_type.validate() {
        return encode_result(RegisterResponse {
            result: Some(register_response::Result::Error(ErrorResponse {
                code: 400,
                message: format!("Invalid actor identity: {err}"),
            })),
        });
    }

    let registration = &request.registration;
    if let Err(validation_err) = RealmEntity::validate_realm(&registration.realm).await {
        let (code, message) = RealmEntity::map_validation_error(validation_err);
        return encode_result(RegisterResponse {
            result: Some(register_response::Result::Error(ErrorResponse {
                code,
                message: format!("Realm validation failed: {message}"),
            })),
        });
    }

    let provided_secret = headers
        .get(REALM_SECRET_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let realm_secret_check = match verify_realm_secret(&registration.realm, provided_secret).await {
        Ok(check @ RealmSecretCheck::ValidCurrent)
        | Ok(check @ RealmSecretCheck::ValidPrevious) => check,
        Ok(RealmSecretCheck::NotConfigured) => {
            return encode_result(RegisterResponse {
                result: Some(register_response::Result::Error(ErrorResponse {
                    code: 403,
                    message: "Realm is not provisioned with a registration secret".to_string(),
                })),
            });
        }
        Ok(RealmSecretCheck::MissingRequired) => {
            return encode_result(RegisterResponse {
                result: Some(register_response::Result::Error(ErrorResponse {
                    code: 403,
                    message: "Realm secret required".to_string(),
                })),
            });
        }
        Ok(RealmSecretCheck::Invalid) => {
            return encode_result(RegisterResponse {
                result: Some(register_response::Result::Error(ErrorResponse {
                    code: 403,
                    message: "Invalid realm secret".to_string(),
                })),
            });
        }
        Err(err) => {
            platform::recording::error!(
                "Failed to verify realm secret for credential reissue in realm {}: {}",
                registration.realm,
                err
            );
            return encode_result(RegisterResponse {
                result: Some(register_response::Result::Error(ErrorResponse {
                    code: 500,
                    message: "Internal error while verifying realm secret".to_string(),
                })),
            });
        }
    };

    // Replay only an exact previously authenticated package reissue request.
    // The issuer binds the cache key to the signatures and renewal proof too.
    if let Some(cached) = state
        .issuer
        .lookup_reissue_cache(
            &request.actr_id,
            registration,
            request.renewal_proof.as_deref(),
        )
        .await
    {
        let succeeded = matches!(&cached.result, Some(register_response::Result::Success(_)));
        if let Some(ref counters) = state.counters {
            counters
                .record_request(succeeded, start.elapsed().as_secs_f64() * 1000.0)
                .await;
        }
        platform::recording::info!(
            "Credential reissue replayed from response cache for actor {}",
            request.actr_id.to_string_repr()
        );
        return encode_result(cached);
    }

    let result = state
        .issuer
        .reissue_credential_with_realm_secret_check(
            &request.actr_id,
            registration,
            request.renewal_proof.as_deref(),
            Some(realm_secret_check),
        )
        .await;

    let succeeded = matches!(&result.result, Some(register_response::Result::Success(_)));
    if let Some(ref counters) = state.counters {
        counters
            .record_request(succeeded, start.elapsed().as_secs_f64() * 1000.0)
            .await;
    }
    if let Some(register_response::Result::Success(ref register_ok)) = result.result {
        keep_pending_registration(&request.actr_id, register_ok).await;
        platform::recording::info!(
            "Credential reissued for actor {}",
            request.actr_id.to_string_repr()
        );
    }

    encode_result(result)
}

/// POST /issuer/renew — renew credentials for an existing ActrId
///
/// Processing flow:
/// 1. Decode and validate `RenewCredentialRequest`.
/// 2. Validate realm state.
/// 3. Validate and rotate renewal token in a transaction.
/// 4. Re-issue access and TURN credentials for the original ActrId.
/// 5. Attach the transaction result renewal token and expiry.
async fn renew_credential(State(state): State<IssuerState>, body: Bytes) -> Response {
    let start = std::time::Instant::now();

    // 1. Decode protobuf request.
    let request = match RenewCredentialRequest::decode(body) {
        Ok(req) => req,
        Err(err) => {
            platform::recording::error!("Failed to decode RenewCredentialRequest: {}", err);
            return encode_renew_result(
                StatusCode::BAD_REQUEST,
                RenewCredentialResponse {
                    result: Some(renew_credential_response::Result::Error(ErrorResponse {
                        code: 400,
                        message: format!("Invalid protobuf: {err}"),
                    })),
                },
            );
        }
    };

    // Validate token length (32 bytes expected).
    if request.renewal_token.len() != 32 {
        platform::recording::warn!(
            "Renewal token has invalid length: {} bytes",
            request.renewal_token.len()
        );
        return encode_renew_result(
            StatusCode::BAD_REQUEST,
            RenewCredentialResponse {
                result: Some(renew_credential_response::Result::Error(ErrorResponse {
                    code: 400,
                    message: "Invalid renewal token length".to_string(),
                })),
            },
        );
    }

    let actor_id = &request.actr_id;
    platform::recording::debug!(
        "Received renewal request for actor {}",
        actor_id.to_string_repr()
    );

    // 2. Validate realm state.
    if let Err(validation_err) = RealmEntity::validate_realm(&actor_id.realm).await {
        let (code, message) = RealmEntity::map_validation_error(validation_err);
        return encode_renew_result(
            status_for_error_code(code),
            RenewCredentialResponse {
                result: Some(renew_credential_response::Result::Error(ErrorResponse {
                    code,
                    message: format!("Realm validation failed: {message}"),
                })),
            },
        );
    }

    // 3. Validate and rotate renewal token in one transaction.
    let issuer = &state.issuer;
    let renewal_outcome = match crate::renewal::rotate_renewal_token(
        actor_id,
        &request.renewal_token,
        &issuer.config.renewal_token_secret,
        issuer.config.renewal_rotation_window_secs,
        issuer.config.renewal_token_ttl_secs,
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(crate::renewal::RenewalError::TokenRejected) => {
            platform::recording::warn!(
                "Renewal token rejected for actor {}",
                actor_id.to_string_repr()
            );
            return encode_renew_result(
                StatusCode::UNAUTHORIZED,
                RenewCredentialResponse {
                    result: Some(renew_credential_response::Result::Error(ErrorResponse {
                        code: 401,
                        message: "Renewal token invalid or expired".to_string(),
                    })),
                },
            );
        }
        Err(crate::renewal::RenewalError::StoreError(msg)) => {
            platform::recording::error!(
                "Renewal store error for actor {}: {}",
                actor_id.to_string_repr(),
                msg
            );
            return encode_renew_result(
                StatusCode::INTERNAL_SERVER_ERROR,
                RenewCredentialResponse {
                    result: Some(renew_credential_response::Result::Error(ErrorResponse {
                        code: 500,
                        message: "Internal renewal store error".to_string(),
                    })),
                },
            );
        }
    };

    // 4. Re-issue access and TURN credentials for the original ActrId.
    // /renew does not carry the package target in its request, so look up the
    // target persisted at registration time to enable per-target revocation
    // recheck. A NULL target (legacy row) degrades to type-only matching.
    let renew_target: Option<String> = match sqlx::query_scalar::<_, Option<String>>(
        "SELECT target FROM issued_actor_identity WHERE realm = ? AND serial_number = ?",
    )
    .bind(&actor_id.realm)
    .bind(actor_id.serial_number as i64)
    .fetch_one(platform::storage::db::get_database().get_pool())
    .await
    {
        Ok(target) => target,
        Err(err) => {
            platform::recording::warn!(
                "Failed to look up target for actor {}: {}",
                actor_id.to_string_repr(),
                err
            );
            None
        }
    };

    let mut register_ok = match issuer
        .issue_credential_for_actor(actor_id, renew_target.as_deref())
        .await
    {
        Ok(ok) => ok,
        Err(err) => {
            platform::recording::error!(
                "Failed to re-issue credential for actor {}: {}",
                actor_id.to_string_repr(),
                err
            );
            let (code, message) = aid_error_to_response(err);
            return encode_renew_result(
                status_for_error_code(code),
                RenewCredentialResponse {
                    result: Some(renew_credential_response::Result::Error(ErrorResponse {
                        code,
                        message,
                    })),
                },
            );
        }
    };

    // 5. Attach renewal token and expiry.
    let (token, expires_at) = match renewal_outcome {
        crate::renewal::RotationOutcome::Unchanged { token, expires_at } => (
            token,
            Some(prost_types::Timestamp {
                seconds: expires_at,
                nanos: 0,
            }),
        ),
        crate::renewal::RotationOutcome::Rotated { token, expires_at } => (
            token,
            Some(prost_types::Timestamp {
                seconds: expires_at,
                nanos: 0,
            }),
        ),
    };

    register_ok.renewal_token = Some(token);
    register_ok.renewal_token_expires_at = expires_at;
    keep_pending_registration(actor_id, &register_ok).await;

    if let Some(ref ctr) = state.counters {
        ctr.record_request(true, start.elapsed().as_secs_f64() * 1000.0)
            .await;
    }

    platform::recording::info!("Credential renewed for actor {}", actor_id.to_string_repr());

    encode_renew_result(
        StatusCode::OK,
        RenewCredentialResponse {
            result: Some(renew_credential_response::Result::Success(register_ok)),
        },
    )
}

/// Health check endpoint
///
/// Performs the following checks:
/// 1. Whether the database connection is healthy
/// 2. Whether the Signer service is reachable
/// 3. Whether the key cache is valid
async fn health_check(State(state): State<IssuerState>) -> Json<Value> {
    let mut checks = json!({
        "service": "issuer",
        "version": env!("CARGO_PKG_VERSION"),
        "status": "healthy"
    });

    // Check database connection
    let db_status = match state.issuer.check_database_health().await {
        Ok(()) => "ok",
        Err(e) => {
            platform::recording::error!("Database health check failed: {}", e);
            checks["status"] = json!("degraded");
            "failed"
        }
    };
    checks["database"] = json!(db_status);

    // Check Signer service connectivity
    let signer_status = match state.issuer.check_signer_health().await {
        Ok(()) => "ok",
        Err(e) => {
            platform::recording::error!("Signer health check failed: {}", e);
            checks["status"] = json!("degraded");
            "failed"
        }
    };
    checks["signer_service"] = json!(signer_status);

    // Check key cache status
    let cache_status = match state.issuer.check_key_cache_health().await {
        Ok(info) => json!({"status": "ok", "key_id": info.key_id, "expires_in": info.expires_in}),
        Err(e) => {
            platform::recording::error!("Key cache health check failed: {}", e);
            checks["status"] = json!("degraded");
            json!({"status": "failed", "error": e.to_string()})
        }
    };
    checks["key_cache"] = cache_status;

    Json(checks)
}

/// Get the current key status (id, expiry, minting usability)
///
/// For monitoring and debugging. Reports `status: "expired"` once the key
/// can no longer mint credentials: the endpoint used to
/// keep answering `success` for an expired key, hiding an ongoing outage
/// while every signaling handshake failed with 401.
async fn get_current_key(State(state): State<IssuerState>) -> Json<Value> {
    match state.issuer.get_current_key_status().await {
        Ok(status) => {
            let status_word = if status.usable { "success" } else { "expired" };
            Json(json!({
                "status": status_word,
                "key_id": status.key_id,
                "expires_at": status.expires_at,
                "expires_in": status.expires_in,
                "expired": !status.usable
            }))
        }
        Err(e) => {
            platform::recording::error!("Failed to get current key: {}", e);
            Json(json!({
                "status": "error",
                "message": format!("Failed to get key: {}", e)
            }))
        }
    }
}

/// Returns the current Issuer signing public key (key_id + base64 pubkey)
///
/// Allows the signaling server or other services to pull the public key on demand for credential
/// signature verification when the key cache misses.
async fn get_signing_pubkey(State(state): State<IssuerState>) -> Json<Value> {
    match state.issuer.get_current_signing_pubkey().await {
        Ok((key_id, pubkey_bytes)) => {
            use base64::Engine as _;
            let pubkey_b64 = base64::engine::general_purpose::STANDARD.encode(&pubkey_bytes);
            Json(json!({
                "status": "success",
                "key_id": key_id,
                "pubkey": pubkey_b64
            }))
        }
        Err(e) => {
            platform::recording::error!("Failed to get signing pubkey: {}", e);
            Json(json!({
                "status": "error",
                "message": format!("Failed to get signing pubkey: {}", e)
            }))
        }
    }
}

/// Encode a RegisterResponse as protobuf bytes
fn encode_result(result: RegisterResponse) -> Bytes {
    let mut buf = Vec::new();
    if let Err(err) = result.encode(&mut buf) {
        platform::recording::error!("Failed to encode RegisterResponse: {}", err);
        // Return an ErrorResponse for encoding failure
        let error_result = RegisterResponse {
            result: Some(register_response::Result::Error(ErrorResponse {
                code: 500,
                message: format!("Failed to encode response: {err}"),
            })),
        };
        let mut fallback_buf = Vec::new();
        let _ = error_result.encode(&mut fallback_buf);
        return Bytes::from(fallback_buf);
    }
    Bytes::from(buf)
}

/// Encode a RenewCredentialResponse as protobuf bytes and set the matching HTTP status.
fn encode_renew_result(status: StatusCode, result: RenewCredentialResponse) -> Response {
    let mut buf = Vec::new();
    let body = if let Err(err) = result.encode(&mut buf) {
        platform::recording::error!("Failed to encode RenewCredentialResponse: {}", err);
        let error_result = RenewCredentialResponse {
            result: Some(renew_credential_response::Result::Error(ErrorResponse {
                code: 500,
                message: format!("Failed to encode response: {err}"),
            })),
        };
        let mut fallback_buf = Vec::new();
        let _ = error_result.encode(&mut fallback_buf);
        Bytes::from(fallback_buf)
    } else {
        Bytes::from(buf)
    };

    (status, [("content-type", "application/x-protobuf")], body).into_response()
}

fn status_for_error_code(code: u32) -> StatusCode {
    match code {
        400 => StatusCode::BAD_REQUEST,
        401 => StatusCode::UNAUTHORIZED,
        403 => StatusCode::FORBIDDEN,
        429 => StatusCode::TOO_MANY_REQUESTS,
        500 => StatusCode::INTERNAL_SERVER_ERROR,
        502 => StatusCode::BAD_GATEWAY,
        503 => StatusCode::SERVICE_UNAVAILABLE,
        504 => StatusCode::GATEWAY_TIMEOUT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// Convert an AidError into a (code, message) tuple
fn aid_error_to_response(err: AidError) -> (u32, String) {
    let code = match &err {
        // Client errors (4xx)
        AidError::InvalidFormat => 400,
        AidError::InvalidPrefix => 400,
        AidError::EmptyId => 400,
        AidError::InvalidTimestamp(_) => 400,
        AidError::Base64DecodeError(_) => 400,
        AidError::HexDecodeError(_) => 400,
        AidError::Expired => 401,
        AidError::RealmError(_) => 403, // Forbidden
        AidError::PublisherNotVerified => 403,
        AidError::PackageRevoked => 403,

        // Server errors (5xx)
        AidError::GenerationFailed(msg) => {
            // If the Signer is unavailable, return 503 (Service Unavailable)
            // A lost race with an expired Signer key is also retryable: rotation
            // may have produced a usable key by the next request.
            if msg.contains("Signer unavailable")
                || msg.contains("Signer service")
                || msg.contains("has expired")
            {
                503
            } else {
                500
            }
        }
        AidError::InvalidSignature(_) => 500,
        AidError::DecodeFailure(_) => 500,
        // AidError is #[non_exhaustive]; future variants default to 5xx.
        _ => 500,
    };
    (code, err.to_string())
}

/// Convert an AidError into a proto ErrorResponse
///
/// Error code mapping strategy:
/// - 4xx: client errors (format, expired, validation failed)
/// - 5xx: server errors (generation failed, internal error)
fn aid_error_to_error_response(err: AidError) -> ErrorResponse {
    let (code, message) = aid_error_to_response(err);
    ErrorResponse { code, message }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actr_protocol::{AIdCredential, ActrType, RegisterAuthMode};
    use prost::bytes::Bytes as ProstBytes;

    #[test]
    fn test_protobuf_request_encoding_decoding() {
        // Test full protobuf request encoding/decoding
        let actr_type = ActrType {
            publisher: "apple".to_string(),
            name: "iPhone15".to_string(),
            version: "1.0.0".to_string(),
        };

        let realm = "k7m2q9xh@example.com".to_string();

        let request = RegisterRequest {
            actr_type,
            realm,
            service: None,
            service_spec: None,
            acl: None,
            ws_address: None,
            manifest_raw: None,
            publisher_signature: None,
            target: None,
            auth_mode: Some(RegisterAuthMode::Package as i32),
            publisher_auth_signature: None,
            publisher_auth_signed_at: None,
            publisher_auth_nonce: None,
        };

        // Encode
        let mut buf = Vec::new();
        request.encode(&mut buf).unwrap();

        // Decode
        let decoded_request = RegisterRequest::decode(buf.as_slice()).unwrap();
        assert_eq!(decoded_request.realm, "k7m2q9xh@example.com");
        assert_eq!(decoded_request.actr_type.publisher, "apple");
        assert_eq!(decoded_request.actr_type.name, "iPhone15");
    }

    #[test]
    fn test_protobuf_minimal_request() {
        // Test a minimal-fields protobuf request
        let request = RegisterRequest {
            actr_type: ActrType {
                publisher: "test".to_string(),
                name: "actor".to_string(),
                version: "1.0.0".to_string(),
            },
            realm: "test@example.com".to_string(),
            service: None,
            service_spec: None,
            acl: None,
            ws_address: None,
            manifest_raw: None,
            publisher_signature: None,
            target: None,
            auth_mode: Some(RegisterAuthMode::Package as i32),
            publisher_auth_signature: None,
            publisher_auth_signed_at: None,
            publisher_auth_nonce: None,
        };

        // Encode-decode round-trip
        let mut buf = Vec::new();
        request.encode(&mut buf).unwrap();
        let decoded_request = RegisterRequest::decode(buf.as_slice()).unwrap();

        assert_eq!(decoded_request.realm, "test@example.com");
        assert_eq!(decoded_request.actr_type.publisher, "test");
        assert_eq!(decoded_request.actr_type.name, "actor");
    }

    #[test]
    fn test_register_response_success() {
        use actr_protocol::{ActrId, register_response::RegisterOk};
        use prost_types::Timestamp;

        // Test a successful RegisterResponse
        let register_ok = RegisterOk {
            actr_id: ActrId {
                realm: "test@example.com".to_string(),
                serial_number: 123456,
            },
            credential: AIdCredential {
                key_id: 42,
                claims: ProstBytes::from(vec![1, 2, 3]),
                signature: ProstBytes::from(vec![0u8; 64]),
            },
            turn_credential: actr_protocol::TurnCredential::default(),
            signing_pubkey: ProstBytes::from(vec![0u8; 32]),
            signing_key_id: 42,
            credential_expires_at: Some(Timestamp {
                seconds: 1234567890,
                nanos: 0,
            }),
            signaling_heartbeat_interval_secs: 30,
            renewal_token: None,
            renewal_token_expires_at: None,
        };

        let response = RegisterResponse {
            result: Some(register_response::Result::Success(register_ok)),
        };

        // Encode-decode round-trip
        let mut buf = Vec::new();
        response.encode(&mut buf).unwrap();
        let decoded_response = RegisterResponse::decode(buf.as_slice()).unwrap();

        assert!(decoded_response.result.is_some());
        if let Some(register_response::Result::Success(resp)) = decoded_response.result {
            assert_eq!(resp.actr_id.realm, "test@example.com");
            assert_eq!(resp.actr_id.serial_number, 123456);
            assert_eq!(resp.credential.key_id, 42);
            assert_eq!(resp.signaling_heartbeat_interval_secs, 30);
        } else {
            panic!("Expected success result");
        }
    }

    #[test]
    fn test_register_response_error() {
        // Test an error RegisterResponse
        let error = ErrorResponse {
            code: 400,
            message: "Bad request".to_string(),
        };

        let response = RegisterResponse {
            result: Some(register_response::Result::Error(error)),
        };

        // Encode-decode round-trip
        let mut buf = Vec::new();
        response.encode(&mut buf).unwrap();
        let decoded_response = RegisterResponse::decode(buf.as_slice()).unwrap();

        assert!(decoded_response.result.is_some());
        if let Some(register_response::Result::Error(err)) = decoded_response.result {
            assert_eq!(err.code, 400);
            assert_eq!(err.message, "Bad request");
        } else {
            panic!("Expected error result");
        }
    }

    #[test]
    fn test_invalid_format_maps_to_400() {
        let error = aid_error_to_error_response(AidError::InvalidFormat);
        assert_eq!(error.code, 400);
    }
}
