use actr_protocol::realm_profile::RealmPackage;
use actrix_sdk::control::{
    AdminApiService, ConfigType, CreateRealmRequest, NonceCredential, RealmInfo, UpdateRealmRequest,
};
use axum::{
    Json, Router,
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use issuer::realm_profile::ProfileChange;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use platform::config::AdminUiConfig;
use platform::monitoring::MetricsStore;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::sync::Arc;

// ── State ──────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AdminApiState {
    pub service: AdminApiService,
    pub config: AdminUiConfig,
    pub jwt_secret: Vec<u8>,
    pub advertised_ip: String,
    pub metrics_store: MetricsStore,
    pub realm_writes_enabled: bool,
}

// ── JWT ────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: u64,
    iat: u64,
    /// ADM-07: Fingerprint of the password hash in effect when this token was
    /// issued. The auth middleware rejects the token once the live
    /// `password_hash` changes. The field is mandatory, so tokens minted before
    /// this claim existed (or with it stripped) fail to deserialize and are
    /// rejected fail-closed.
    pwd_ver: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    password: String,
}

#[derive(Serialize)]
struct LoginResponse {
    token: String,
    expires_in: u64,
}

// ── Auth middleware ─────────────────────────────────────────────

pub async fn auth_middleware(
    State(state): State<Arc<AdminApiState>>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    let Some(token) = token else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Missing authorization token"})),
        )
            .into_response();
    };

    let key = DecodingKey::from_secret(&state.jwt_secret);
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;

    match jsonwebtoken::decode::<Claims>(token, &key, &validation) {
        Ok(data) => {
            // ADM-07: Reject tokens issued under a superseded password hash.
            // After the admin rotates `password_hash` and reloads, the state is
            // rebuilt with the new hash, so the recomputed pwd_ver no longer
            // matches tokens signed against the old one — logging out all
            // existing sessions.
            if data.claims.pwd_ver != state.config.password_version() {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"error": "Invalid or expired token"})),
                )
                    .into_response();
            }
            next.run(request).await
        }
        Err(_) => (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Invalid or expired token"})),
        )
            .into_response(),
    }
}

// ── Handlers ───────────────────────────────────────────────────

async fn login(
    State(state): State<Arc<AdminApiState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(body): Json<LoginRequest>,
) -> impl IntoResponse {
    use argon2::{Argon2, PasswordHash, PasswordVerifier};
    let is_match = PasswordHash::new(&state.config.password_hash)
        .ok()
        .is_some_and(|hash| {
            Argon2::default()
                .verify_password(body.password.as_bytes(), &hash)
                .is_ok()
        });

    if !is_match {
        // ADM-03: Record failed admin login on the security channel. The source
        // IP is included; the submitted password is never logged.
        log_login_attempt(peer.ip(), false);
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Invalid password"})),
        )
            .into_response();
    }

    // ADM-03: Record successful credential verification on the security channel.
    log_login_attempt(peer.ip(), true);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let claims = Claims {
        sub: "admin".to_string(),
        iat: now,
        exp: now + state.config.session_expiry_secs,
        // ADM-07: Bind the token to the current password hash so rotation
        // invalidates it immediately.
        pwd_ver: state.config.password_version(),
    };

    let key = EncodingKey::from_secret(&state.jwt_secret);
    match jsonwebtoken::encode(&Header::default(), &claims, &key) {
        Ok(token) => (
            StatusCode::OK,
            Json(json!(LoginResponse {
                token,
                expires_in: state.config.session_expiry_secs,
            })),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Failed to create token"})),
        )
            .into_response(),
    }
}

async fn get_node_info(State(state): State<Arc<AdminApiState>>) -> impl IntoResponse {
    match state.service.node_info_direct().await {
        Ok(resp) => {
            // Collect power reserve level + per-metric details from pwrzv
            let (power_reserve, metrics) =
                match pwrzv::get_power_reserve_level_with_details_direct().await {
                    Ok((level, details)) => {
                        let map: serde_json::Map<String, Value> = details
                            .into_iter()
                            .map(|(k, d)| (k, json!({ "value": d.value, "score": d.score })))
                            .collect();
                        (level as f64, Value::Object(map))
                    }
                    Err(_) => (0.0, Value::Null),
                };

            let services: Vec<Value> = resp.services.iter().map(service_status_to_json).collect();

            (
                StatusCode::OK,
                Json(json!({
                    "success": true,
                    "node_id": resp.node_id,
                    "name": resp.name,
                    "version": resp.version,
                    "location_tag": resp.location_tag,
                    "uptime_secs": resp.uptime_secs,
                    "power_reserve": power_reserve,
                    "metrics": metrics,
                    "services": services,
                })),
            )
                .into_response()
        }
        Err(status) => grpc_status_to_response(status),
    }
}

async fn get_node_services(State(state): State<Arc<AdminApiState>>) -> impl IntoResponse {
    let services = state.service.service_statuses().await;
    let services_json: Vec<Value> = services.iter().map(service_status_to_json).collect();
    (StatusCode::OK, Json(json!({"services": services_json}))).into_response()
}

async fn get_admin_capabilities(State(state): State<Arc<AdminApiState>>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(json!({
            "realm_writes_enabled": state.realm_writes_enabled,
            "superv_managed": !state.realm_writes_enabled,
        })),
    )
        .into_response()
}

async fn list_realms(State(state): State<Arc<AdminApiState>>) -> impl IntoResponse {
    match state.service.list_realms_direct().await {
        Ok(resp) => {
            let realms: Vec<Value> = resp.realms.iter().map(realm_info_to_json).collect();
            (
                StatusCode::OK,
                Json(json!({
                    "success": true,
                    "realms": realms,
                    "total_count": resp.total_count,
                })),
            )
                .into_response()
        }
        Err(status) => grpc_status_to_response(status),
    }
}

#[derive(Deserialize)]
struct CreateRealmBody {
    realm: String,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    expires_at: u64,
    /// Package and display name of the first Realm Profile; the package is
    /// required for Realms under the Provider domain.
    #[serde(default)]
    package: Option<RealmPackage>,
    #[serde(default)]
    display_name: Option<String>,
}

fn default_true() -> bool {
    true
}

async fn create_realm(
    State(state): State<Arc<AdminApiState>>,
    Json(body): Json<CreateRealmBody>,
) -> impl IntoResponse {
    if !state.realm_writes_enabled {
        return realm_writes_disabled_response();
    }

    let req = CreateRealmRequest {
        realm: body.realm,
        enabled: body.enabled,
        credential: dummy_credential(),
        expires_at: body.expires_at,
    };

    let profile = ProfileChange {
        package: body.package,
        display_name: body.display_name,
        ..Default::default()
    };
    match state
        .service
        .create_realm_with_invite_direct(req, profile)
        .await
    {
        Ok((resp, owner_invite)) => {
            let status = if resp.success {
                StatusCode::CREATED
            } else {
                StatusCode::BAD_REQUEST
            };
            (
                status,
                // ADM-09: response carries the owner invite; forbid caching.
                [(header::CACHE_CONTROL, "no-store")],
                Json(json!({
                    "success": resp.success,
                    "error_message": resp.error_message,
                    "realm": resp.realm.as_ref().map(realm_info_to_json),
                    "owner_invite": owner_invite.as_ref().map(|invite| &invite.invite),
                    "owner_invite_expires_at": owner_invite
                        .as_ref()
                        .map(|invite| platform::realm::membership::rfc3339(invite.expires_at)),
                })),
            )
                .into_response()
        }
        Err(status) => grpc_status_to_response(status),
    }
}

// ── Identity re-confirmation ───────────────────────────────────
//
// A Realm domain or publisher whose served signing keys do not chain to the
// pinned keys is refused until the operator compares both and confirms the
// new ones (`authority-discovery.md` section 4.4 step 6).

#[derive(Deserialize)]
struct ConfirmIdentityRequest {
    /// The `fetched.digest` the operator reviewed.
    digest: String,
}

/// Fetch an authority descriptor for the operator, uncached so a review sees
/// what the domain serves now.
async fn fetch_descriptor(url: String) -> Result<Arc<[u8]>, platform::net::FetchError> {
    platform::net::fetch_public_json(&url, 5)
        .await
        .map(Arc::from)
}

fn identity_response(result: Result<Value, issuer::realm_management::ManagementError>) -> Response {
    match result {
        Ok(view) => (StatusCode::OK, Json(view)).into_response(),
        Err(error) => (
            error.status(),
            Json(json!({ "success": false, "error": error.to_string() })),
        )
            .into_response(),
    }
}

/// Record a confirmed identity change on the audit channel.
fn audit_identity_confirmed(resource: &str, id: &str, confirmed: &Value) {
    use platform::recording::{AuditPayload, Record, RecordLevel, emit_with_callsite};

    let mut record =
        Record::<{ platform::recording::CHANNEL_AUDIT }>::new("admin.identity.confirm");
    record.common.actor = Some("admin".to_string());
    record.common.level = RecordLevel::Warn;
    record.audit = Some(AuditPayload {
        action: "identity.confirm".to_string(),
        resource: resource.to_string(),
        resource_id: Some(id.to_string()),
        reason: Some("operator confirmed new signing keys".to_string()),
        remote_addr: None,
        session_id: None,
        before: confirmed.get("previous").cloned(),
        after: confirmed.get("pinned").cloned(),
    });
    let _ = emit_with_callsite(record, file!(), line!(), module_path!());
}

async fn get_realm_domain_identity(Path(domain): Path<String>) -> Response {
    identity_response(issuer::identity::realm_domain_identity(&domain, fetch_descriptor).await)
}

async fn confirm_realm_domain_identity(
    State(state): State<Arc<AdminApiState>>,
    Path(domain): Path<String>,
    Json(request): Json<ConfirmIdentityRequest>,
) -> Response {
    if !state.realm_writes_enabled {
        return realm_writes_disabled_response();
    }
    let result =
        issuer::identity::confirm_realm_domain_identity(&domain, &request.digest, fetch_descriptor)
            .await;
    if let Ok(confirmed) = &result {
        audit_identity_confirmed("realm_domain", &domain, confirmed);
    }
    identity_response(result)
}

async fn get_publisher_identity(Path(publisher): Path<String>) -> Response {
    identity_response(issuer::identity::publisher_identity(&publisher, fetch_descriptor).await)
}

async fn confirm_publisher_identity(
    Path(publisher): Path<String>,
    Json(request): Json<ConfirmIdentityRequest>,
) -> Response {
    let result =
        issuer::identity::confirm_publisher_identity(&publisher, &request.digest, fetch_descriptor)
            .await;
    if let Ok(confirmed) = &result {
        audit_identity_confirmed("publisher", &publisher, confirmed);
    }
    identity_response(result)
}

/// A new owner invite for an active Realm under the Provider domain or
/// without a Realm Profile (owner recovery); earlier unused owner invites
/// stop working.
async fn issue_owner_invite(
    State(state): State<Arc<AdminApiState>>,
    Path(realm): Path<String>,
) -> impl IntoResponse {
    if !state.realm_writes_enabled {
        return realm_writes_disabled_response();
    }

    match state.service.issue_owner_invite_direct(&realm).await {
        Ok(invite) => (
            StatusCode::OK,
            [(header::CACHE_CONTROL, "no-store")],
            Json(json!({
                "success": true,
                "realm": realm,
                "owner_invite": invite.invite,
                "expires_at": platform::realm::membership::rfc3339(invite.expires_at),
            })),
        )
            .into_response(),
        Err(status) if status.code() == tonic::Code::FailedPrecondition => (
            StatusCode::CONFLICT,
            Json(json!({ "success": false, "error": status.message() })),
        )
            .into_response(),
        Err(status) => grpc_status_to_response(status),
    }
}

/// Members, clients and open invites of a Realm, read-only.
async fn list_realm_members(
    State(state): State<Arc<AdminApiState>>,
    Path(realm): Path<String>,
) -> impl IntoResponse {
    match state.service.list_realm_members_direct(&realm).await {
        Ok(members) => (
            StatusCode::OK,
            Json(json!({ "success": true, "members": members["members"], "invites": members["invites"] })),
        )
            .into_response(),
        Err(status) => grpc_status_to_response(status),
    }
}

async fn rotate_realm_issuer_key(
    State(state): State<Arc<AdminApiState>>,
    Path(realm): Path<String>,
) -> impl IntoResponse {
    if !state.realm_writes_enabled {
        return realm_writes_disabled_response();
    }

    match state.service.rotate_realm_issuer_key_direct(&realm).await {
        Ok(key_id) => (
            StatusCode::OK,
            Json(json!({ "success": true, "realm": realm, "key_id": key_id })),
        )
            .into_response(),
        Err(status) => grpc_status_to_response(status),
    }
}

async fn get_realm(
    State(state): State<Arc<AdminApiState>>,
    Path(realm): Path<String>,
) -> impl IntoResponse {
    match state.service.get_realm_direct(&realm).await {
        Ok(resp) => {
            let status = if resp.success {
                StatusCode::OK
            } else {
                StatusCode::NOT_FOUND
            };
            (
                status,
                Json(json!({
                    "success": resp.success,
                    "error_message": resp.error_message,
                    "realm": resp.realm.as_ref().map(realm_info_to_json),
                })),
            )
                .into_response()
        }
        Err(status) => grpc_status_to_response(status),
    }
}

/// `enabled: false` suspends the Realm and `true` resumes it. The Realm name
/// never changes.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateRealmBody {
    enabled: Option<bool>,
    expires_at: Option<u64>,
}

async fn update_realm(
    State(state): State<Arc<AdminApiState>>,
    Path(realm): Path<String>,
    Json(body): Json<UpdateRealmBody>,
) -> impl IntoResponse {
    if !state.realm_writes_enabled {
        return realm_writes_disabled_response();
    }

    let req = UpdateRealmRequest {
        realm,
        enabled: body.enabled,
        credential: dummy_credential(),
        expires_at: body.expires_at,
    };

    match state.service.update_realm_direct(req).await {
        Ok(resp) => {
            let status = if resp.success {
                StatusCode::OK
            } else {
                StatusCode::BAD_REQUEST
            };
            (
                status,
                Json(json!({
                    "success": resp.success,
                    "error_message": resp.error_message,
                    "realm": resp.realm.as_ref().map(realm_info_to_json),
                })),
            )
                .into_response()
        }
        Err(status) => grpc_status_to_response(status),
    }
}

async fn retire_realm(
    State(state): State<Arc<AdminApiState>>,
    Path(realm): Path<String>,
) -> impl IntoResponse {
    if !state.realm_writes_enabled {
        return realm_writes_disabled_response();
    }

    match state.service.retire_realm_direct(&realm).await {
        Ok(resp) => {
            let status = if resp.success {
                StatusCode::OK
            } else {
                StatusCode::BAD_REQUEST
            };
            (
                status,
                Json(json!({
                    "success": resp.success,
                    "error_message": resp.error_message,
                    "realm": resp.realm.as_ref().map(realm_info_to_json),
                })),
            )
                .into_response()
        }
        Err(status) => grpc_status_to_response(status),
    }
}

/// The accepted Realm Profile, as signed.
async fn get_realm_profile(
    State(state): State<Arc<AdminApiState>>,
    Path(realm): Path<String>,
) -> impl IntoResponse {
    match state.service.get_realm_profile_direct(&realm).await {
        Ok(Some(document)) => {
            let profile: Value = serde_json::from_str(&document).unwrap_or(Value::Null);
            (
                StatusCode::OK,
                Json(json!({ "success": true, "profile": profile })),
            )
                .into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "success": false,
                "error_message": format!("Realm {realm} has no Realm Profile"),
            })),
        )
            .into_response(),
        Err(status) => grpc_status_to_response(status),
    }
}

async fn get_config(
    State(state): State<Arc<AdminApiState>>,
    Path((config_type, config_key)): Path<(i32, String)>,
) -> impl IntoResponse {
    let ct = ConfigType::try_from(config_type).unwrap_or(ConfigType::Unspecified);
    match state.service.get_config_direct(ct, config_key).await {
        Ok(resp) => (
            StatusCode::OK,
            Json(json!({
                "success": resp.success,
                "error_message": resp.error_message,
                "config_value": resp.config_value,
            })),
        )
            .into_response(),
        Err(status) => grpc_status_to_response(status),
    }
}

#[derive(Deserialize)]
struct UpdateConfigBody {
    config_value: String,
}

async fn update_config(
    State(state): State<Arc<AdminApiState>>,
    Path((config_type, config_key)): Path<(i32, String)>,
    Json(body): Json<UpdateConfigBody>,
) -> impl IntoResponse {
    // The config store accepts arbitrary keys with no namespace separation, so
    // runtime config feeding realm services (TURN secret, signer, bind
    // addresses) is writable here. Same lock as the realm write endpoints.
    if !state.realm_writes_enabled {
        return realm_writes_disabled_response();
    }

    let ct = ConfigType::try_from(config_type).unwrap_or(ConfigType::Unspecified);
    match state
        .service
        .update_config_direct(ct, config_key, body.config_value)
        .await
    {
        Ok(resp) => (
            StatusCode::OK,
            Json(json!({
                "success": resp.success,
                "error_message": resp.error_message,
                "old_value": resp.old_value,
            })),
        )
            .into_response(),
        Err(status) => grpc_status_to_response(status),
    }
}

#[derive(Deserialize)]
struct ShutdownBody {
    #[serde(default = "default_true")]
    graceful: bool,
    timeout_secs: Option<i32>,
    reason: Option<String>,
}

async fn shutdown_node(
    State(state): State<Arc<AdminApiState>>,
    Json(body): Json<ShutdownBody>,
) -> impl IntoResponse {
    match state
        .service
        .shutdown_direct(body.graceful, body.timeout_secs, body.reason)
        .await
    {
        Ok(resp) => (
            StatusCode::OK,
            Json(json!({
                "accepted": resp.accepted,
                "error_message": resp.error_message,
                "estimated_shutdown_time": resp.estimated_shutdown_time,
            })),
        )
            .into_response(),
        Err(status) => grpc_status_to_response(status),
    }
}

async fn reload_node(State(state): State<Arc<AdminApiState>>) -> impl IntoResponse {
    match state.service.reload_direct().await {
        Ok(_) => (StatusCode::OK, Json(json!({"accepted": true}))).into_response(),
        Err(status) => grpc_status_to_response(status),
    }
}

async fn restart_node(State(state): State<Arc<AdminApiState>>) -> impl IntoResponse {
    match state
        .service
        .shutdown_direct(
            true,
            Some(10),
            Some("Restart requested via admin API".into()),
        )
        .await
    {
        Ok(resp) => (
            StatusCode::OK,
            Json(json!({
                "accepted": resp.accepted,
                "error_message": resp.error_message,
            })),
        )
            .into_response(),
        Err(status) => grpc_status_to_response(status),
    }
}

async fn get_config_file(State(state): State<Arc<AdminApiState>>) -> impl IntoResponse {
    match state.service.get_config_file_direct().await {
        Ok(cf) => (
            StatusCode::OK,
            Json(json!({"content": cf.content, "path": cf.path})),
        )
            .into_response(),
        Err(status) => grpc_status_to_response(status),
    }
}

#[derive(Deserialize)]
struct SaveConfigBody {
    content: String,
}

async fn save_config_file(
    State(state): State<Arc<AdminApiState>>,
    Json(body): Json<SaveConfigBody>,
) -> impl IntoResponse {
    // superv_managed is the inverse of realm_writes_enabled: when the realm is
    // supervisor-managed the Admin UI is read-only for the control plane, so
    // control.* edits via config-file save are rejected downstream.
    let superv_managed = !state.realm_writes_enabled;
    match state
        .service
        .save_config_file_direct(&body.content, superv_managed)
        .await
    {
        Ok(()) => (StatusCode::OK, Json(json!({"saved": true}))).into_response(),
        Err(status) => {
            let http_status = match status.code() {
                tonic::Code::InvalidArgument => StatusCode::BAD_REQUEST,
                tonic::Code::PermissionDenied => StatusCode::FORBIDDEN,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (
                http_status,
                Json(json!({"saved": false, "error": status.message()})),
            )
                .into_response()
        }
    }
}

// ── Service detail handlers ─────────────────────────────────────

async fn get_service_detail(
    State(state): State<Arc<AdminApiState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match state.service.get_service_detail_direct(&name).await {
        Ok(detail) => (StatusCode::OK, Json(serde_json::to_value(detail).unwrap())).into_response(),
        Err(status) => grpc_status_to_response(status),
    }
}

async fn get_signer_keys(State(state): State<Arc<AdminApiState>>) -> impl IntoResponse {
    match state.service.get_signer_keys_direct().await {
        Ok(result) => (StatusCode::OK, Json(serde_json::to_value(result).unwrap())).into_response(),
        Err(status) => grpc_status_to_response(status),
    }
}

// ── Platform detail endpoint ─────────────────────────────────────

async fn get_platform_detail(State(state): State<Arc<AdminApiState>>) -> impl IntoResponse {
    match state.service.get_platform_detail_direct().await {
        Ok(detail) => (StatusCode::OK, Json(serde_json::to_value(detail).unwrap())).into_response(),
        Err(status) => grpc_status_to_response(status),
    }
}

// ── Config registry & override endpoints ────────────────────────

async fn get_registry(State(state): State<Arc<AdminApiState>>) -> impl IntoResponse {
    let fields = state.service.get_registry_direct();
    (StatusCode::OK, Json(json!(fields))).into_response()
}

async fn list_overrides(State(state): State<Arc<AdminApiState>>) -> impl IntoResponse {
    match state.service.list_overrides_direct().await {
        Ok(overrides) => (StatusCode::OK, Json(json!(overrides))).into_response(),
        Err(status) => grpc_status_to_response(status),
    }
}

#[derive(Deserialize)]
struct SetOverrideBody {
    value: String,
}

async fn set_override(
    State(state): State<Arc<AdminApiState>>,
    Path(key): Path<String>,
    Json(body): Json<SetOverrideBody>,
) -> impl IntoResponse {
    // Overrides apply to arbitrary config keys with no namespace separation;
    // while the realm is supervisor-managed the local Admin UI must not be
    // able to override (or shadow supervisor-set overrides on) realm-serving
    // config.
    if !state.realm_writes_enabled {
        return realm_writes_disabled_response();
    }

    match state
        .service
        .set_override_direct(&key, &body.value, "admin")
        .await
    {
        Ok(()) => (StatusCode::OK, Json(json!({"success": true}))).into_response(),
        Err(status) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"success": false, "error": status.message()})),
        )
            .into_response(),
    }
}

async fn delete_override(
    State(state): State<Arc<AdminApiState>>,
    Path(key): Path<String>,
) -> impl IntoResponse {
    // Deleting an override can silently revert realm-serving config to file
    // defaults; locked down together with set_override.
    if !state.realm_writes_enabled {
        return realm_writes_disabled_response();
    }

    match state.service.delete_override_direct(&key).await {
        Ok(deleted) => (
            StatusCode::OK,
            Json(json!({"success": true, "deleted": deleted})),
        )
            .into_response(),
        Err(status) => grpc_status_to_response(status),
    }
}

// ── Network probe endpoint ───────────────────────────────────────

async fn probe_port(
    State(state): State<Arc<AdminApiState>>,
    Path(port): Path<u16>,
) -> impl IntoResponse {
    use std::time::Instant;
    use tokio::net::TcpStream;
    use tokio::time::{Duration, timeout};

    let addr = format!("{}:{}", state.advertised_ip, port);
    let start = Instant::now();

    match timeout(Duration::from_secs(5), TcpStream::connect(&addr)).await {
        Ok(Ok(_)) => {
            let latency_ms = start.elapsed().as_millis() as u64;
            (
                StatusCode::OK,
                Json(json!({"reachable": true, "latency_ms": latency_ms})),
            )
                .into_response()
        }
        Ok(Err(e)) => (
            StatusCode::OK,
            Json(json!({"reachable": false, "error": e.to_string()})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::OK,
            Json(json!({"reachable": false, "error": "Connection timed out"})),
        )
            .into_response(),
    }
}

// ── Metrics timeseries endpoint ────────────────────────────────

#[derive(Deserialize)]
struct TimeseriesQuery {
    service_type: i32,
    #[serde(default)]
    tier: u8,
}

async fn get_metrics_timeseries(
    State(state): State<Arc<AdminApiState>>,
    Query(q): Query<TimeseriesQuery>,
) -> impl IntoResponse {
    let interval_secs: u64 = match q.tier {
        0 => 60,
        1 => 900,
        2 => 14400,
        _ => 0,
    };
    let samples = state.metrics_store.query(q.service_type, q.tier).await;
    (
        StatusCode::OK,
        Json(json!({
            "service_type": q.service_type,
            "tier": q.tier,
            "interval_secs": interval_secs,
            "samples": samples,
        })),
    )
        .into_response()
}

// ── SPA static files ───────────────────────────────────────────

#[cfg(feature = "admin-ui")]
#[derive(rust_embed::RustEmbed)]
#[folder = "$OUT_DIR/admin-web/dist"]
struct AdminAssets;

#[cfg(feature = "admin-ui")]
async fn serve_spa(uri: axum::http::Uri) -> impl IntoResponse {
    let path = uri.path().strip_prefix("/admin/").unwrap_or(uri.path());
    let path = if path.is_empty() { "index.html" } else { path };

    match AdminAssets::get(path) {
        Some(content) => {
            let mime = content.metadata.mimetype();
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, mime)],
                content.data.into_owned(),
            )
                .into_response()
        }
        None => {
            // SPA fallback: serve index.html for client-side routing
            match AdminAssets::get("index.html") {
                Some(content) => (
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, "text/html")],
                    content.data.into_owned(),
                )
                    .into_response(),
                None => (StatusCode::NOT_FOUND, "Admin UI not found").into_response(),
            }
        }
    }
}

#[cfg(not(feature = "admin-ui"))]
async fn serve_spa() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html")],
        "<html><body><h1>Admin UI</h1><p>Admin UI is not compiled into this build. \
         Rebuild with <code>--features admin-ui</code>.</p></body></html>",
    )
}

// ── Router builder ─────────────────────────────────────────────

pub fn build_admin_api_router(
    state: Arc<AdminApiState>,
    publisher_registry_router: Option<Router>,
) -> Router {
    let authed_api = Router::new()
        .route("/admin/api/capabilities", get(get_admin_capabilities))
        .route("/admin/api/node", get(get_node_info))
        .route("/admin/api/node/services", get(get_node_services))
        .route("/admin/api/node/shutdown", post(shutdown_node))
        .route("/admin/api/node/reload", post(reload_node))
        .route("/admin/api/node/restart", post(restart_node))
        .route("/admin/api/realms", get(list_realms))
        .route("/admin/api/realms", post(create_realm))
        .route("/admin/api/realms/{realm}", get(get_realm))
        .route("/admin/api/realms/{realm}", put(update_realm))
        .route(
            "/admin/api/realms/{realm}/issuer-key/rotate",
            post(rotate_realm_issuer_key),
        )
        .route("/admin/api/realms/{realm}/retire", post(retire_realm))
        .route("/admin/api/realms/{realm}/profile", get(get_realm_profile))
        .route(
            "/admin/api/realms/{realm}/owner-invite",
            post(issue_owner_invite),
        )
        .route("/admin/api/realms/{realm}/members", get(list_realm_members))
        .route(
            "/admin/api/identity/realm-domains/{domain}",
            get(get_realm_domain_identity).post(confirm_realm_domain_identity),
        )
        .route(
            "/admin/api/identity/publishers/{publisher}",
            get(get_publisher_identity).post(confirm_publisher_identity),
        )
        .route(
            "/admin/api/config/{config_type}/{config_key}",
            get(get_config),
        )
        .route(
            "/admin/api/config/{config_type}/{config_key}",
            put(update_config),
        )
        .route(
            "/admin/api/config-file",
            get(get_config_file).put(save_config_file),
        )
        // Platform detail route
        .route("/admin/api/platform", get(get_platform_detail))
        // Config registry & override routes
        .route("/admin/api/registry", get(get_registry))
        .route("/admin/api/config/overrides", get(list_overrides))
        .route(
            "/admin/api/config/overrides/{key}",
            put(set_override).delete(delete_override),
        )
        // Service detail routes — specific routes before parameterized
        .route("/admin/api/services/signer/keys", get(get_signer_keys))
        .route("/admin/api/services/{name}", get(get_service_detail))
        .route("/admin/api/network/probe/{port}", get(probe_port))
        .route("/admin/api/metrics/timeseries", get(get_metrics_timeseries));

    let authed_api = authed_api
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .with_state(state.clone());

    // Nest Publisher Registry management behind Admin authentication.
    let authed_publisher_registry = publisher_registry_router.map(|publisher| {
        Router::new()
            .nest("/admin/api/publishers", publisher)
            .layer(middleware::from_fn_with_state(
                state.clone(),
                auth_middleware,
            ))
    });

    let public = Router::new()
        .route("/admin/api/auth/login", post(login))
        .with_state(state.clone());

    // ADM-09: Attach hardening response headers (CSP, nosniff, no-referrer) to
    // the whole Admin UI tree.
    let spa = Router::new()
        .route("/admin/{*path}", get(serve_spa))
        .route("/admin", get(serve_spa))
        .layer(middleware::from_fn(admin_security_headers));

    let combined = public.merge(authed_api).merge(spa);
    if let Some(publisher) = authed_publisher_registry {
        combined.merge(publisher)
    } else {
        combined
    }
}

// ── Helpers ────────────────────────────────────────────────────

/// ADM-03: Emit an admin login attempt on the security channel.
///
/// Records the outcome and source IP for both success and failure. The
/// submitted password is never included.
fn log_login_attempt(source_ip: std::net::IpAddr, success: bool) {
    use platform::recording::{
        CHANNEL_SECURITY, Outcome, Record, RecordLevel, SecurityPayload, SecuritySeverity,
        emit_with_callsite,
    };

    let mut record = Record::<{ CHANNEL_SECURITY }>::new("admin.auth.login");
    record.common.actor = Some("admin".to_string());
    record.common.level = if success {
        RecordLevel::Info
    } else {
        RecordLevel::Warn
    };
    record.common.outcome = if success {
        Outcome::Success
    } else {
        Outcome::Denied
    };
    record.security = Some(SecurityPayload {
        control: "admin.auth.login".to_string(),
        severity: if success {
            SecuritySeverity::Low
        } else {
            SecuritySeverity::Medium
        },
        category: Some("authn".to_string()),
        subject: Some("admin".to_string()),
        source_addr: Some(source_ip.to_string()),
        destination_addr: None,
        evidence: None,
    });
    let _ = emit_with_callsite(record, file!(), line!(), module_path!());
}

/// ADM-09: Attach hardening response headers to every Admin UI response.
///
/// tower-http's `set-header` feature is not enabled in this build, so we add the
/// static headers through a small axum middleware instead of `SetResponseHeaderLayer`
/// (no new dependency). Applied to the `/admin` SPA tree.
async fn admin_security_headers(request: axum::extract::Request, next: Next) -> Response {
    use axum::http::HeaderValue;

    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'self'; frame-ancestors 'none'"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response
}

fn dummy_credential() -> NonceCredential {
    NonceCredential {
        timestamp: 0,
        nonce: String::new(),
        signature: String::new(),
    }
}

fn realm_writes_disabled_response() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({
            "success": false,
            "error_message": "Realm writes are disabled while NodeAdminService gRPC API is enabled",
        })),
    )
        .into_response()
}

fn realm_info_to_json(info: &RealmInfo) -> Value {
    json!({
        "realm": info.realm,
        "enabled": info.enabled,
        "created_at": info.created_at,
        "updated_at": info.updated_at,
        "expires_at": info.expires_at,
        "status": info.status,
    })
}

fn service_status_to_json(s: &actrix_sdk::control::ServiceStatus) -> Value {
    json!({
        "name": s.name,
        "type": s.r#type,
        "is_healthy": s.is_healthy,
        "active_connections": s.active_connections,
        "total_requests": s.total_requests,
        "failed_requests": s.failed_requests,
        "average_latency_ms": s.average_latency_ms,
        "url": s.url,
        "port": s.port,
        "domain": s.domain,
    })
}

fn grpc_status_to_response(status: tonic::Status) -> Response {
    let http_status = match status.code() {
        tonic::Code::NotFound => StatusCode::NOT_FOUND,
        tonic::Code::InvalidArgument => StatusCode::BAD_REQUEST,
        tonic::Code::PermissionDenied => StatusCode::FORBIDDEN,
        tonic::Code::Unauthenticated => StatusCode::UNAUTHORIZED,
        tonic::Code::FailedPrecondition | tonic::Code::Unavailable => {
            StatusCode::SERVICE_UNAVAILABLE
        }
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };

    (
        http_status,
        Json(json!({
            "success": false,
            "error": status.message(),
        })),
    )
        .into_response()
}
