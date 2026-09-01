//! AIS (Actor Identity Service) HTTP Handler

use crate::{
    idempotency::{
        AttemptError, AttemptLease, ClaimOutcome, ControlAttemptStore, Operation,
        register_ok_response_expiry,
    },
    issuer::{AIdIssuer, ManufacturerNonceBinding, ReissuePreparation},
    ratelimit::ip_rate_limiter,
};
use actr_protocol::{
    ErrorResponse, RegisterRequest, RegisterResponse, ReissueCredentialRequest,
    RenewCredentialRequest, RenewCredentialResponse, register_response, renew_credential_response,
};
use axum::{
    Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header::RETRY_AFTER},
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

/// AIS service state
#[derive(Clone)]
pub struct AISState {
    pub issuer: Arc<AIdIssuer>,
    idempotency: Arc<ControlAttemptStore>,
    /// Service-level counters for metrics collection.
    pub counters: Option<Arc<ServiceCounters>>,
}

impl AISState {
    pub fn new(issuer: AIdIssuer) -> Self {
        let idempotency = Arc::new(ControlAttemptStore::new(
            &issuer.config.renewal_token_secret,
        ));
        Self {
            issuer: Arc::new(issuer),
            idempotency,
            counters: None,
        }
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

/// Create the AIS service router
///
/// Apply rate limiting middleware:
/// - IP level: 100 req/min (prevents DoS attacks from a single IP)
pub fn create_router(state: AISState) -> Router {
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
///
/// Registration is intentionally a create operation: every accepted call
/// allocates a new identity. Idempotent recovery of an existing identity goes
/// through `/renew` or `/reissue`, both of which require an `attempt_id`.
async fn register_actr(State(state): State<AISState>, headers: HeaderMap, body: Bytes) -> Bytes {
    let start = std::time::Instant::now();

    // Decode protobuf request
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
        "Received register request for realm {}, type {}:{}",
        request.realm.realm_id,
        request.actr_type.manufacturer,
        request.actr_type.name
    );

    // AIS is an independent server-side trust boundary: do not assume Hyper
    // already validated the identity. Reject any ActrType that violates the
    // protocol naming rules before minting an ActrId or writing an ACL key
    // (a key derived from an invalid identity would be unparseable later).
    if let Err(e) = request.actr_type.validate() {
        let error_result = RegisterResponse {
            result: Some(register_response::Result::Error(ErrorResponse {
                code: 400, // Bad Request
                message: format!("Invalid actor identity: {e}"),
            })),
        };
        return encode_result(error_result);
    }

    // Validate realm exists, is in good status, and is not expired
    if let Err(validation_err) = RealmEntity::validate_realm(request.realm.realm_id).await {
        let (code, message) = RealmEntity::map_validation_error(validation_err);
        let error_result = RegisterResponse {
            result: Some(register_response::Result::Error(ErrorResponse {
                code,
                message: format!("Realm validation failed: {message}"),
            })),
        };
        return encode_result(error_result);
    }

    // Verify realm secret (legacy realms without a configured secret are allowed through)
    let provided_secret = headers
        .get(REALM_SECRET_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty());

    let realm_secret_check =
        match verify_realm_secret(request.realm.realm_id, provided_secret).await {
            Ok(check @ RealmSecretCheck::NotConfigured)
            | Ok(check @ RealmSecretCheck::ValidCurrent)
            | Ok(check @ RealmSecretCheck::ValidPrevious) => check,
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
                    request.realm.realm_id,
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

    // Prepare signing and identity material without exposing durable state.
    // The renewal-token binding, ACL, and pending metadata commit together.
    let result = match state
        .issuer
        .prepare_registration_with_realm_secret_check(&request, Some(realm_secret_check))
        .await
    {
        Ok(prepared) => {
            let pool = platform::storage::db::get_database().get_pool();
            let mut tx = match pool.begin().await {
                Ok(tx) => tx,
                Err(error) => {
                    platform::recording::error!(
                        "Failed to begin registration transaction: {}",
                        error
                    );
                    if let Some(ref counters) = state.counters {
                        counters
                            .record_request(false, start.elapsed().as_secs_f64() * 1000.0)
                            .await;
                    }
                    return encode_result(RegisterResponse {
                        result: Some(register_response::Result::Error(ErrorResponse {
                            code: 500,
                            message: "Failed to persist registration state".to_string(),
                        })),
                    });
                }
            };
            if let Err(error) = state
                .issuer
                .commit_prepared_registration(&mut tx, &prepared)
                .await
            {
                platform::recording::error!(
                    "Failed to persist prepared registration identity: {}",
                    error
                );
                if let Some(ref counters) = state.counters {
                    counters
                        .record_request(false, start.elapsed().as_secs_f64() * 1000.0)
                        .await;
                }
                let error = aid_error_to_error_response(error);
                return encode_result(RegisterResponse {
                    result: Some(register_response::Result::Error(error)),
                });
            }
            let response = RegisterResponse {
                result: Some(register_response::Result::Success(prepared.register_ok)),
            };
            if let Some(register_response::Result::Success(ref register_ok)) = response.result {
                platform::recording::debug!(
                    "Successfully registered ActrId: realm={}, serial_number={}, type={}:{}",
                    register_ok.actr_id.realm.realm_id,
                    register_ok.actr_id.serial_number,
                    register_ok.actr_id.r#type.manufacturer,
                    register_ok.actr_id.r#type.name
                );

                // Persist ACL rules to database
                if let Some(ref acl) = request.acl {
                    use actr_protocol::acl_rule::SourceRealm;

                    let realm_id = register_ok.actr_id.realm.realm_id;
                    let t = &register_ok.actr_id.r#type;
                    let my_type = t.to_string_repr();

                    let rules = acl
                        .rules
                        .iter()
                        .map(|rule| {
                            let allow = rule.permission
                                == actr_protocol::acl_rule::Permission::Allow as i32;
                            let from_type = rule.from_type.to_string_repr();
                            let source_realm_id = match &rule.source_realm {
                                Some(SourceRealm::AnyRealm(_)) => None,
                                Some(SourceRealm::RealmId(id)) => Some(*id),
                                None => Some(realm_id),
                            };
                            ActorAcl::new_with_source_realm(
                                realm_id,
                                source_realm_id,
                                from_type,
                                my_type.clone(),
                                allow,
                            )
                        })
                        .collect::<Vec<_>>();

                    if let Err(e) = ActorAcl::replace_for_target_in_transaction(
                        &mut tx, realm_id, &my_type, &rules,
                    )
                    .await
                    {
                        platform::recording::error!(
                            "Failed to atomically replace ACL rules for {}: {}",
                            my_type,
                            e
                        );
                        if let Some(ref counters) = state.counters {
                            counters
                                .record_request(false, start.elapsed().as_secs_f64() * 1000.0)
                                .await;
                        }
                        return encode_result(RegisterResponse {
                            result: Some(register_response::Result::Error(ErrorResponse {
                                code: 500,
                                message: "Failed to persist registration ACL".to_string(),
                            })),
                        });
                    }
                }

                // Store pending registration (service_spec) for signaling to pick up
                if request.service_spec.is_some() || request.ws_address.is_some() {
                    let serial = register_ok.actr_id.serial_number;
                    let realm = register_ok.actr_id.realm.realm_id;
                    let spec_blob = request
                        .service_spec
                        .as_ref()
                        .map(prost::Message::encode_to_vec);
                    let ws_address = request.ws_address.clone();
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;
                    let expires_at = register_ok
                        .renewal_token_expires_at
                        .map(|timestamp| timestamp.seconds)
                        .unwrap_or(now)
                        .saturating_add(crate::renewal::REISSUE_BINDING_GRACE_SECS);
                    if let Err(error) = sqlx::query(
                        "INSERT OR REPLACE INTO pending_registration \
                         (serial_number, realm_id, service_spec_blob, ws_address, created_at, expires_at) \
                         VALUES (?, ?, ?, ?, ?, ?)",
                    )
                    .bind(serial as i64)
                    .bind(realm as i64)
                    .bind(spec_blob)
                    .bind(ws_address)
                    .bind(now)
                    .bind(expires_at)
                    .execute(&mut *tx)
                    .await
                    {
                        platform::recording::error!(
                            "Failed to persist pending registration metadata: {}",
                            error
                        );
                        if let Some(ref counters) = state.counters {
                            counters
                                .record_request(false, start.elapsed().as_secs_f64() * 1000.0)
                                .await;
                        }
                        return encode_result(RegisterResponse {
                            result: Some(register_response::Result::Error(ErrorResponse {
                                code: 500,
                                message: "Failed to persist registration metadata".to_string(),
                            })),
                        });
                    }

                    if let Err(error) = sqlx::query(
                        "DELETE FROM pending_registration
                         WHERE expires_at IS NOT NULL AND expires_at <= ?1",
                    )
                    .bind(now)
                    .execute(&mut *tx)
                    .await
                    {
                        platform::recording::error!(
                            "Failed to clean expired pending registration metadata: {}",
                            error
                        );
                        if let Some(ref counters) = state.counters {
                            counters
                                .record_request(false, start.elapsed().as_secs_f64() * 1000.0)
                                .await;
                        }
                        return encode_result(RegisterResponse {
                            result: Some(register_response::Result::Error(ErrorResponse {
                                code: 500,
                                message: "Failed to persist registration metadata".to_string(),
                            })),
                        });
                    }
                }
            }
            if let Err(error) = tx.commit().await {
                platform::recording::error!("Failed to commit registration state: {}", error);
                if let Some(ref counters) = state.counters {
                    counters
                        .record_request(false, start.elapsed().as_secs_f64() * 1000.0)
                        .await;
                }
                return encode_result(RegisterResponse {
                    result: Some(register_response::Result::Error(ErrorResponse {
                        code: 500,
                        message: "Failed to persist registration state".to_string(),
                    })),
                });
            }
            let _ = crate::renewal::gc_expired_tokens().await;
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

/// POST /ais/reissue — re-authenticate and issue credentials for an existing
/// stable ActrId without allocating a new serial number.
async fn reissue_credential(
    State(state): State<AISState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let start = std::time::Instant::now();
    let request = match ReissueCredentialRequest::decode(body.clone()) {
        Ok(request) => request,
        Err(err) => {
            platform::recording::error!("Failed to decode ReissueCredentialRequest: {}", err);
            return encode_register_result(
                StatusCode::OK,
                RegisterResponse {
                    result: Some(register_response::Result::Error(ErrorResponse {
                        code: 400,
                        message: format!("Invalid protobuf: {err}"),
                    })),
                },
            );
        }
    };

    let lease = match state
        .idempotency
        .claim(
            Operation::Reissue,
            &request.attempt_id,
            &request.actr_id,
            &body,
        )
        .await
    {
        Ok(ClaimOutcome::Acquired(lease)) => lease,
        Ok(ClaimOutcome::Replay(cached)) => {
            return raw_protobuf_response(cached.status, cached.body);
        }
        Err(error) => return encode_register_attempt_error(error),
    };

    let registration = &request.registration;
    if let Err(validation_err) = RealmEntity::validate_realm(registration.realm.realm_id).await {
        let (code, message) = RealmEntity::map_validation_error(validation_err);
        let response = RegisterResponse {
            result: Some(register_response::Result::Error(ErrorResponse {
                code,
                message: format!("Realm validation failed: {message}"),
            })),
        };
        return complete_register_attempt(&state, &lease, StatusCode::OK, response, None).await;
    }

    let provided_secret = headers
        .get(REALM_SECRET_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let realm_secret_check = match verify_realm_secret(registration.realm.realm_id, provided_secret)
        .await
    {
        Ok(check @ RealmSecretCheck::NotConfigured)
        | Ok(check @ RealmSecretCheck::ValidCurrent)
        | Ok(check @ RealmSecretCheck::ValidPrevious) => check,
        Ok(RealmSecretCheck::MissingRequired) => {
            let response = RegisterResponse {
                result: Some(register_response::Result::Error(ErrorResponse {
                    code: 403,
                    message: "Realm secret required".to_string(),
                })),
            };
            return complete_register_attempt(&state, &lease, StatusCode::OK, response, None).await;
        }
        Ok(RealmSecretCheck::Invalid) => {
            let response = RegisterResponse {
                result: Some(register_response::Result::Error(ErrorResponse {
                    code: 403,
                    message: "Invalid realm secret".to_string(),
                })),
            };
            return complete_register_attempt(&state, &lease, StatusCode::OK, response, None).await;
        }
        Err(err) => {
            platform::recording::error!(
                "Failed to verify realm secret for credential reissue in realm {}: {}",
                registration.realm.realm_id,
                err
            );
            let response = RegisterResponse {
                result: Some(register_response::Result::Error(ErrorResponse {
                    code: 500,
                    message: "Internal error while verifying realm secret".to_string(),
                })),
            };
            let _ = state.idempotency.release(&lease).await;
            return encode_register_result(StatusCode::OK, response);
        }
    };

    let preparation = state
        .issuer
        .prepare_reissue_credential_with_realm_secret_check(
            &request.actr_id,
            registration,
            request.renewal_proof.as_deref(),
            Some(realm_secret_check),
        )
        .await;

    let (response, succeeded) = match preparation {
        ReissuePreparation::Success(prepared) => {
            match state
                .idempotency
                .complete_reissue(&lease, &request.actr_id, prepared)
                .await
            {
                Ok(cached) => (raw_protobuf_response(cached.status, cached.body), true),
                Err(error) => {
                    let _ = state.idempotency.release(&lease).await;
                    (encode_register_attempt_error(error), false)
                }
            }
        }
        ReissuePreparation::Rejected {
            response,
            manufacturer_nonce,
        } => (
            complete_register_attempt(
                &state,
                &lease,
                StatusCode::OK,
                response,
                manufacturer_nonce.as_ref(),
            )
            .await,
            false,
        ),
        ReissuePreparation::Retryable(response) => {
            let _ = state.idempotency.release(&lease).await;
            (encode_register_result(StatusCode::OK, response), false)
        }
    };

    if let Some(ref counters) = state.counters {
        counters
            .record_request(succeeded, start.elapsed().as_secs_f64() * 1000.0)
            .await;
    }
    if succeeded {
        platform::recording::info!(
            "Credential reissued for actor {}",
            request.actr_id.to_string_repr()
        );
    }

    response
}

/// POST /ais/renew — renew credentials for an existing ActrId
///
/// Processing flow:
/// 1. Decode and validate `RenewCredentialRequest`.
/// 2. Validate realm state.
/// 3. Validate and rotate renewal token in a transaction.
/// 4. Re-issue access and TURN credentials for the original ActrId.
/// 5. Attach the transaction result renewal token and expiry.
async fn renew_credential(State(state): State<AISState>, body: Bytes) -> Response {
    let start = std::time::Instant::now();

    // 1. Decode protobuf request.
    let request = match RenewCredentialRequest::decode(body.clone()) {
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

    let lease = match state
        .idempotency
        .claim(
            Operation::Renew,
            &request.attempt_id,
            &request.actr_id,
            &body,
        )
        .await
    {
        Ok(ClaimOutcome::Acquired(lease)) => lease,
        Ok(ClaimOutcome::Replay(cached)) => {
            return raw_protobuf_response(cached.status, cached.body);
        }
        Err(error) => return encode_renew_attempt_error(error),
    };

    // Validate token length (32 bytes expected).
    if request.renewal_token.len() != 32 {
        platform::recording::warn!(
            "Renewal token has invalid length: {} bytes",
            request.renewal_token.len()
        );
        return complete_renew_attempt(
            &state,
            &lease,
            StatusCode::BAD_REQUEST,
            RenewCredentialResponse {
                result: Some(renew_credential_response::Result::Error(ErrorResponse {
                    code: 400,
                    message: "Invalid renewal token length".to_string(),
                })),
            },
        )
        .await;
    }

    let actor_id = &request.actr_id;
    platform::recording::debug!(
        "Received renewal request for actor {}",
        actor_id.to_string_repr()
    );

    // 2. Validate realm state.
    if let Err(validation_err) = RealmEntity::validate_realm(actor_id.realm.realm_id).await {
        let (code, message) = RealmEntity::map_validation_error(validation_err);
        return complete_renew_attempt(
            &state,
            &lease,
            status_for_error_code(code),
            RenewCredentialResponse {
                result: Some(renew_credential_response::Result::Error(ErrorResponse {
                    code,
                    message: format!("Realm validation failed: {message}"),
                })),
            },
        )
        .await;
    }

    // 3. Validate and prepare token rotation without mutating SQLite. The
    // rotation is committed together with the encrypted response below.
    let issuer = &state.issuer;
    let prepared = match crate::renewal::prepare_renewal_token_rotation(
        actor_id,
        &request.renewal_token,
        &issuer.config.renewal_token_secret,
        issuer.config.renewal_rotation_window_secs,
        issuer.config.renewal_token_ttl_secs,
    )
    .await
    {
        Ok(prepared) => prepared,
        Err(crate::renewal::RenewalError::TokenRejected) => {
            platform::recording::warn!(
                "Renewal token rejected for actor {}",
                actor_id.to_string_repr()
            );
            return complete_renew_attempt(
                &state,
                &lease,
                StatusCode::UNAUTHORIZED,
                RenewCredentialResponse {
                    result: Some(renew_credential_response::Result::Error(ErrorResponse {
                        code: 401,
                        message: "Renewal token invalid or expired".to_string(),
                    })),
                },
            )
            .await;
        }
        Err(crate::renewal::RenewalError::StoreError(msg)) => {
            platform::recording::error!(
                "Renewal store error for actor {}: {}",
                actor_id.to_string_repr(),
                msg
            );
            let _ = state.idempotency.release(&lease).await;
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
    let mut register_ok = match issuer.issue_credential_for_actor(actor_id).await {
        Ok(ok) => ok,
        Err(err) => {
            platform::recording::error!(
                "Failed to re-issue credential for actor {}: {}",
                actor_id.to_string_repr(),
                err
            );
            let (code, message) = aid_error_to_response(err);
            let _ = state.idempotency.release(&lease).await;
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
    let (token, expires_at) = match &prepared.outcome {
        crate::renewal::RotationOutcome::Unchanged { token, expires_at } => (
            token.clone(),
            Some(prost_types::Timestamp {
                seconds: *expires_at,
                nanos: 0,
            }),
        ),
        crate::renewal::RotationOutcome::Rotated { token, expires_at } => (
            token.clone(),
            Some(prost_types::Timestamp {
                seconds: *expires_at,
                nanos: 0,
            }),
        ),
    };

    register_ok.renewal_token = Some(token);
    register_ok.renewal_token_expires_at = expires_at;

    let response_expires_at = register_ok_response_expiry(&register_ok);
    let response = RenewCredentialResponse {
        result: Some(renew_credential_response::Result::Success(register_ok)),
    };
    let body = response.encode_to_vec();
    if let Err(error) = state
        .idempotency
        .complete_renewal(
            &lease,
            actor_id,
            &prepared,
            StatusCode::OK.as_u16(),
            &body,
            response_expires_at,
        )
        .await
    {
        platform::recording::error!(
            "Failed to commit credential renewal for actor {}: {}",
            actor_id.to_string_repr(),
            error
        );
        let _ = state.idempotency.release(&lease).await;
        return encode_renew_attempt_error(error);
    }

    if let Some(ref ctr) = state.counters {
        ctr.record_request(true, start.elapsed().as_secs_f64() * 1000.0)
            .await;
    }

    platform::recording::info!("Credential renewed for actor {}", actor_id.to_string_repr());

    raw_protobuf_response(StatusCode::OK.as_u16(), body)
}

/// Health check endpoint
///
/// Performs the following checks:
/// 1. Database connectivity
/// 2. KS service reachability
/// 3. Key cache validity
async fn health_check(State(state): State<AISState>) -> Json<Value> {
    let mut checks = json!({
        "service": "ais",
        "version": env!("CARGO_PKG_VERSION"),
        "status": "healthy"
    });

    // Check database connectivity
    let db_status = match state.issuer.check_database_health().await {
        Ok(()) => "ok",
        Err(e) => {
            platform::recording::error!("Database health check failed: {}", e);
            checks["status"] = json!("degraded");
            "failed"
        }
    };
    checks["database"] = json!(db_status);

    // Check KS service reachability
    let ks_status = match state.issuer.check_ks_health().await {
        Ok(()) => "ok",
        Err(e) => {
            platform::recording::error!("KS health check failed: {}", e);
            checks["status"] = json!("degraded");
            "failed"
        }
    };
    checks["ks_service"] = json!(ks_status);

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

/// Get the current key ID
///
/// For monitoring and debugging
async fn get_current_key(State(state): State<AISState>) -> Json<Value> {
    match state.issuer.get_current_key_id().await {
        Ok(key_id) => Json(json!({
            "status": "success",
            "key_id": key_id
        })),
        Err(e) => {
            platform::recording::error!("Failed to get current key: {}", e);
            Json(json!({
                "status": "error",
                "message": format!("Failed to get key: {}", e)
            }))
        }
    }
}

/// Return the current AIS signing public key (key_id + base64 pubkey)
///
/// Allows signaling servers or other services to fetch the public key on
/// demand for credential signature verification when a key_cache miss occurs.
async fn get_signing_pubkey(State(state): State<AISState>) -> Json<Value> {
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

/// Encode RegisterResponse as protobuf bytes
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

const REJECTED_ATTEMPT_RESPONSE_TTL_SECS: i64 = 5 * 60;

async fn complete_register_attempt(
    state: &AISState,
    lease: &AttemptLease,
    status: StatusCode,
    response: RegisterResponse,
    manufacturer_nonce: Option<&ManufacturerNonceBinding>,
) -> Response {
    let body = response.encode_to_vec();
    let expires_at = crate::renewal::now_secs().saturating_add(REJECTED_ATTEMPT_RESPONSE_TTL_SECS);
    match state
        .idempotency
        .complete_response(
            lease,
            status.as_u16(),
            &body,
            expires_at,
            manufacturer_nonce,
        )
        .await
    {
        Ok(()) => raw_protobuf_response(status.as_u16(), body),
        Err(error) => {
            let _ = state.idempotency.release(lease).await;
            encode_register_attempt_error(error)
        }
    }
}

async fn complete_renew_attempt(
    state: &AISState,
    lease: &AttemptLease,
    status: StatusCode,
    response: RenewCredentialResponse,
) -> Response {
    let body = response.encode_to_vec();
    let expires_at = crate::renewal::now_secs().saturating_add(REJECTED_ATTEMPT_RESPONSE_TTL_SECS);
    match state
        .idempotency
        .complete_response(lease, status.as_u16(), &body, expires_at, None)
        .await
    {
        Ok(()) => raw_protobuf_response(status.as_u16(), body),
        Err(error) => {
            let _ = state.idempotency.release(lease).await;
            encode_renew_attempt_error(error)
        }
    }
}

fn attempt_error_parts(error: &AttemptError) -> (StatusCode, u32, &'static str, Option<u64>) {
    match error {
        AttemptError::InvalidAttemptId => (
            StatusCode::BAD_REQUEST,
            400,
            "attempt_id must contain exactly 16 UUID bytes",
            None,
        ),
        AttemptError::Conflict => (
            StatusCode::CONFLICT,
            409,
            "attempt_id was already used with a different request",
            None,
        ),
        AttemptError::Expired => (
            StatusCode::GONE,
            410,
            "idempotency result retention has expired; start a new attempt",
            None,
        ),
        AttemptError::InProgress { retry_after_secs } => (
            StatusCode::TOO_EARLY,
            425,
            "the same control-plane attempt is still in progress",
            Some(*retry_after_secs),
        ),
        AttemptError::LeaseLost => (
            StatusCode::TOO_EARLY,
            425,
            "the same control-plane attempt is still in progress",
            Some(1),
        ),
        AttemptError::TokenRejected => (
            StatusCode::UNAUTHORIZED,
            401,
            "renewal token invalid or expired",
            None,
        ),
        AttemptError::PossessionNotProven => (
            StatusCode::UNPROCESSABLE_ENTITY,
            422,
            "actor identity possession not proven",
            None,
        ),
        AttemptError::ManufacturerNonceRejected => (
            StatusCode::FORBIDDEN,
            403,
            "manufacturer authorization nonce already consumed",
            None,
        ),
        AttemptError::Store(_) | AttemptError::Crypto(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "internal control-attempt store error",
            None,
        ),
    }
}

fn encode_register_attempt_error(error: AttemptError) -> Response {
    let (status, code, message, retry_after_secs) = attempt_error_parts(&error);
    if code >= 500 {
        platform::recording::error!("Control-attempt failure: {}", error);
    }
    let mut response = encode_register_result(
        status,
        RegisterResponse {
            result: Some(register_response::Result::Error(ErrorResponse {
                code,
                message: message.to_string(),
            })),
        },
    );
    set_retry_after(&mut response, retry_after_secs);
    response
}

fn encode_renew_attempt_error(error: AttemptError) -> Response {
    let (status, code, message, retry_after_secs) = attempt_error_parts(&error);
    if code >= 500 {
        platform::recording::error!("Control-attempt failure: {}", error);
    }
    let mut response = encode_renew_result(
        status,
        RenewCredentialResponse {
            result: Some(renew_credential_response::Result::Error(ErrorResponse {
                code,
                message: message.to_string(),
            })),
        },
    );
    set_retry_after(&mut response, retry_after_secs);
    response
}

fn set_retry_after(response: &mut Response, retry_after_secs: Option<u64>) {
    if let Some(seconds) = retry_after_secs
        && let Ok(value) = HeaderValue::from_str(&seconds.to_string())
    {
        response.headers_mut().insert(RETRY_AFTER, value);
    }
}

fn encode_register_result(status: StatusCode, result: RegisterResponse) -> Response {
    raw_protobuf_response(status.as_u16(), encode_result(result).to_vec())
}

fn raw_protobuf_response(status: u16, body: Vec<u8>) -> Response {
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (
        status,
        [("content-type", "application/x-protobuf")],
        Bytes::from(body),
    )
        .into_response()
}

/// Encode RenewCredentialResponse as protobuf bytes and set the matching HTTP status.
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

/// Convert AidError to a (code, message) tuple
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
        AidError::ManufacturerNotVerified => 403,
        AidError::PackageRevoked => 403,

        // Server errors (5xx)
        AidError::GenerationFailed(msg) => {
            // If KS is unavailable, return 503 (Service Unavailable)
            if msg.contains("KS unavailable") || msg.contains("KS service") {
                503
            } else {
                500
            }
        }
        AidError::InvalidSignature(_) => 500,
        AidError::DecodeFailure(_) => 500,
        // `AidError` is `#[non_exhaustive]`: unmapped future variants are
        // treated as generic server errors so the boundary never panics.
        _ => 500,
    };
    (code, err.to_string())
}

/// Convert AidError to a proto ErrorResponse
///
/// Error code mapping strategy:
/// - 4xx: client errors (format, expiry, validation failure)
/// - 5xx: server errors (generation failure, internal error)
fn aid_error_to_error_response(err: AidError) -> ErrorResponse {
    let (code, message) = aid_error_to_response(err);
    ErrorResponse { code, message }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actr_protocol::{AIdCredential, ActrType, Realm, RegisterAuthMode};
    use prost::bytes::Bytes as ProstBytes;

    #[test]
    fn test_protobuf_request_encoding_decoding() {
        // Test full protobuf request encoding/decoding
        let actr_type = ActrType {
            manufacturer: "apple".to_string(),
            name: "iPhone15".to_string(),
            version: "1.0.0".to_string(),
        };

        let realm = Realm { realm_id: 12345 };

        let request = RegisterRequest {
            actr_type,
            realm,
            service: None,
            service_spec: None,
            acl: None,
            ws_address: None,
            manifest_raw: None,
            mfr_signature: None,
            target: None,
            auth_mode: Some(RegisterAuthMode::Package as i32),
            manufacturer_auth_signature: None,
            manufacturer_auth_signed_at: None,
            manufacturer_auth_nonce: None,
        };

        // Encode
        let mut buf = Vec::new();
        request.encode(&mut buf).unwrap();

        // Decode
        let decoded_request = RegisterRequest::decode(buf.as_slice()).unwrap();
        assert_eq!(decoded_request.realm.realm_id, 12345);
        assert_eq!(decoded_request.actr_type.manufacturer, "apple");
        assert_eq!(decoded_request.actr_type.name, "iPhone15");
    }

    #[test]
    fn test_protobuf_minimal_request() {
        // Test minimal-field protobuf request
        let request = RegisterRequest {
            actr_type: ActrType {
                manufacturer: "test".to_string(),
                name: "actor".to_string(),
                version: "1.0.0".to_string(),
            },
            realm: Realm { realm_id: 456 },
            service: None,
            service_spec: None,
            acl: None,
            ws_address: None,
            manifest_raw: None,
            mfr_signature: None,
            target: None,
            auth_mode: Some(RegisterAuthMode::Package as i32),
            manufacturer_auth_signature: None,
            manufacturer_auth_signed_at: None,
            manufacturer_auth_nonce: None,
        };

        // Encode/decode round-trip
        let mut buf = Vec::new();
        request.encode(&mut buf).unwrap();
        let decoded_request = RegisterRequest::decode(buf.as_slice()).unwrap();

        assert_eq!(decoded_request.realm.realm_id, 456);
        assert_eq!(decoded_request.actr_type.manufacturer, "test");
        assert_eq!(decoded_request.actr_type.name, "actor");
    }

    #[test]
    fn test_register_response_success() {
        use actr_protocol::{ActrId, register_response::RegisterOk};
        use prost_types::Timestamp;

        // Test successful RegisterResponse
        let register_ok = RegisterOk {
            actr_id: ActrId {
                realm: Realm { realm_id: 1 },
                serial_number: 123456,
                r#type: ActrType {
                    manufacturer: "test".to_string(),
                    name: "actor".to_string(),
                    version: "1.0.0".to_string(),
                },
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

        // Encode/decode round-trip
        let mut buf = Vec::new();
        response.encode(&mut buf).unwrap();
        let decoded_response = RegisterResponse::decode(buf.as_slice()).unwrap();

        assert!(decoded_response.result.is_some());
        if let Some(register_response::Result::Success(resp)) = decoded_response.result {
            assert_eq!(resp.actr_id.realm.realm_id, 1);
            assert_eq!(resp.actr_id.serial_number, 123456);
            assert_eq!(resp.credential.key_id, 42);
            assert_eq!(resp.signaling_heartbeat_interval_secs, 30);
        } else {
            panic!("Expected success result");
        }
    }

    #[test]
    fn test_register_response_error() {
        // Test error RegisterResponse
        let error = ErrorResponse {
            code: 400,
            message: "Bad request".to_string(),
        };

        let response = RegisterResponse {
            result: Some(register_response::Result::Error(error)),
        };

        // Encode/decode round-trip
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

    #[test]
    fn in_progress_attempt_returns_remaining_lease_hint() {
        let response = encode_renew_attempt_error(AttemptError::InProgress {
            retry_after_secs: 73,
        });
        assert_eq!(response.status(), StatusCode::TOO_EARLY);
        assert_eq!(response.headers().get(RETRY_AFTER).unwrap(), "73");
    }
}
