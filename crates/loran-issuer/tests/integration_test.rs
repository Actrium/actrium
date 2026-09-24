//! Issuer integration tests (self-bootstrapping)
//!
//! Starts a temporary Signer gRPC service within the test process to verify the Issuer issuance and validation chain.

use actr_protocol::{
    Acl, AclRule, ActrType, RegisterAuthMode, RegisterRequest, RegisterResponse,
    ReissueCredentialRequest, RenewCredentialRequest, RenewCredentialResponse, acl_rule,
    register_response, renew_credential_response,
};
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use base64::Engine as _;
use issuer::signer_client_wrapper::create_signer_client;
use issuer::{
    handlers::{IssuerState, create_router},
    issuer::{AIdIssuer, IssuerConfig, REISSUE_POSSESSION_NOT_PROVEN_CODE},
};
use nonce_auth::storage::MemoryStorage;
use platform::aid::credential::validator::AIdCredentialValidator;
use platform::config::signer::SignerClientConfig;
use platform::realm::{REALM_SECRET_HEADER, RealmSecretCheck, hash_realm_secret};
use prost::Message;
use serial_test::serial;
use signer::{GrpcClient, GrpcClientConfig, KeyStorage, SignerServiceConfig, create_grpc_service};
use std::net::TcpListener;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tempfile::TempDir;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tonic::transport::Server;
use tower::ServiceExt;

const TEST_REALM: &str = "test@example.com";
const ROUTE_REALM_SECRET: &str = "test-route-realm-registration-root-secret";

struct TestEnv {
    issuer_temp_dir: TempDir,
    _signer_temp_dir: TempDir,
    signer_handle: Option<JoinHandle<()>>,
    signer_shutdown_tx: Option<oneshot::Sender<()>>,
    signer_config: SignerClientConfig,
    shared_key: String,
}

impl TestEnv {
    async fn shutdown_signer(&mut self) {
        if let Some(tx) = self.signer_shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.signer_handle.take() {
            let _ = handle.await;
        }
    }
}

async fn start_embedded_signer_with_config(
    shared_key: &str,
    sqlite_path: &Path,
    service_config: SignerServiceConfig,
) -> (String, JoinHandle<()>, oneshot::Sender<()>) {
    start_embedded_signer_at(shared_key, sqlite_path, service_config, None).await
}

/// Start an embedded Signer, optionally pinned to a fixed address (used to
/// simulate a Signer restart on the same endpoint).
async fn start_embedded_signer_at(
    shared_key: &str,
    sqlite_path: &Path,
    service_config: SignerServiceConfig,
    fixed_addr: Option<std::net::SocketAddr>,
) -> (String, JoinHandle<()>, oneshot::Sender<()>) {
    let storage = KeyStorage::from_config(
        &service_config.storage,
        signer::KeyEncryptor::no_encryption(),
        sqlite_path,
    )
    .await
    .expect("Failed to create Signer storage");

    let addr = match fixed_addr {
        Some(addr) => addr,
        None => {
            let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind ephemeral port");
            let addr = listener.local_addr().expect("Failed to get local addr");
            drop(listener);
            addr
        }
    };

    let service = create_grpc_service(
        storage,
        MemoryStorage::new(),
        shared_key.to_string(),
        service_config.tolerance_seconds,
    );

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let handle = tokio::spawn(async move {
        if let Err(err) = Server::builder()
            .add_service(service)
            .serve_with_shutdown(addr, async move {
                let _ = shutdown_rx.await;
            })
            .await
        {
            panic!("Embedded Signer server failed: {err}");
        }
    });

    let endpoint = format!("http://{addr}");

    // Wait until gRPC health check is reachable.
    let mut last_error = String::new();
    for _ in 0..40 {
        let cfg = GrpcClientConfig {
            endpoint: endpoint.clone(),
            actrix_shared_key: shared_key.to_string(),
            timeout_seconds: 2,
            enable_tls: false,
            tls_domain: None,
            ca_cert: None,
            client_cert: None,
            client_key: None,
        };

        match GrpcClient::new(&cfg).await {
            Ok(mut client) => match client.health_check().await {
                Ok(status) if status == "healthy" => return (endpoint, handle, shutdown_tx),
                Ok(status) => last_error = format!("unexpected Signer health status: {status}"),
                Err(err) => last_error = err.to_string(),
            },
            Err(err) => last_error = err.to_string(),
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    panic!("Embedded Signer did not become healthy in time: {last_error}");
}

async fn setup_test_environment() -> TestEnv {
    setup_test_environment_with_signer_config(SignerServiceConfig::default()).await
}

async fn setup_test_environment_with_signer_config(
    signer_service_config: SignerServiceConfig,
) -> TestEnv {
    use std::sync::OnceLock;
    static DB_DIR: OnceLock<TempDir> = OnceLock::new();

    let issuer_temp_dir = TempDir::new().expect("Failed to create issuer temp dir");
    let signer_temp_dir = TempDir::new().expect("Failed to create signer temp dir");
    let shared_key = "test-shared-key".to_string();
    let (endpoint, signer_handle, signer_shutdown_tx) = start_embedded_signer_with_config(
        &shared_key,
        signer_temp_dir.path(),
        signer_service_config,
    )
    .await;

    // Initialize the global database once with a persistent temp dir.
    // The OnceLock ensures the TempDir (and its SQLite file) lives for the
    // entire process, avoiding "unable to open database file" in serial tests.
    let db_dir = DB_DIR.get_or_init(|| TempDir::new().expect("Failed to create DB temp dir"));
    if !platform::storage::db::is_database_initialized() {
        platform::storage::db::set_db_path(db_dir.path())
            .await
            .expect("Failed to initialize test database");
    }
    seed_realm(TEST_REALM, None).await;

    let signer_config = SignerClientConfig {
        endpoint,
        timeout_seconds: 10,
        enable_tls: false,
        tls_domain: None,
        ca_cert: None,
        client_cert: None,
        client_key: None,
    };

    TestEnv {
        issuer_temp_dir,
        _signer_temp_dir: signer_temp_dir,
        signer_handle: Some(signer_handle),
        signer_shutdown_tx: Some(signer_shutdown_tx),
        signer_config,
        shared_key,
    }
}

fn default_issuer_config(temp_dir: &TempDir) -> IssuerConfig {
    IssuerConfig {
        worker_id: 0,
        token_ttl_secs: 3600,
        renewal_token_ttl_secs: 24 * 60 * 60,
        renewal_rotation_window_secs: 60 * 60,
        renewal_token_secret: vec![0; 32],
        signaling_heartbeat_interval_secs: 30,
        key_refresh_interval_secs: 3600,
        key_storage_file: temp_dir.path().join("issuer_keys.db"),
        enable_periodic_rotation: false,
        key_rotation_interval_secs: 86400,
        turn_secret: "test-turn-secret-at-least-32-bytes!".to_string(),
        sqlite_path: temp_dir.path().to_path_buf(),
    }
}

fn linked_register_request() -> RegisterRequest {
    RegisterRequest {
        actr_type: ActrType {
            publisher: "linked-src".to_string(),
            name: "source-workload".to_string(),
            version: "1.0.0".to_string(),
        },
        realm: TEST_REALM.to_string(),
        service_spec: None,
        acl: None,
        service: None,
        ws_address: None,
        manifest_raw: None,
        publisher_signature: None,
        target: None,
        auth_mode: Some(RegisterAuthMode::Linked as i32),
        publisher_auth_signature: None,
        publisher_auth_signed_at: None,
        publisher_auth_nonce: None,
    }
}

/// Actor type signed into the credential claims of a registration.
fn registered_type(ok: &register_response::RegisterOk) -> ActrType {
    actr_protocol::IdentityClaims::decode(&ok.credential.claims[..])
        .expect("decode claims")
        .actor_type
}

fn control_attempt_id() -> prost::bytes::Bytes {
    prost::bytes::Bytes::copy_from_slice(uuid::Uuid::new_v4().as_bytes())
}

fn package_register_request(
    publisher: &str,
    name: &str,
    version: &str,
    realm: &str,
    target: &str,
    manifest: Vec<u8>,
    manifest_signature: Option<Vec<u8>>,
) -> RegisterRequest {
    RegisterRequest {
        actr_type: ActrType {
            publisher: publisher.to_string(),
            name: name.to_string(),
            version: version.to_string(),
        },
        realm: realm.to_string(),
        service_spec: None,
        acl: None,
        service: None,
        ws_address: None,
        manifest_raw: Some(prost::bytes::Bytes::from(manifest)),
        publisher_signature: manifest_signature.map(prost::bytes::Bytes::from),
        target: Some(target.to_string()),
        auth_mode: Some(RegisterAuthMode::Package as i32),
        publisher_auth_signature: None,
        publisher_auth_signed_at: None,
        publisher_auth_nonce: None,
    }
}

async fn create_test_router(env: &TestEnv) -> Router {
    create_router(IssuerState::new(create_test_issuer(env).await))
}

async fn create_test_issuer(env: &TestEnv) -> AIdIssuer {
    let signer_client = create_signer_client(&env.signer_config, &env.shared_key)
        .await
        .expect("signer client");
    AIdIssuer::new(
        signer_client,
        default_issuer_config(&env.issuer_temp_dir),
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    .expect("issuer")
}

async fn seed_realm(realm: &str, secret: Option<&str>) {
    let pool = platform::storage::db::get_database().get_pool();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    // A realm always has a non-empty secret at the storage layer (enforced by a
    // CHECK constraint). When a test does not care about the secret value, seed a
    // fixed placeholder so the row is valid; tests that exercise secret
    // verification pass an explicit secret.
    let secret_current = secret
        .map(hash_realm_secret)
        .unwrap_or_else(|| hash_realm_secret("seed-realm-secret"));

    sqlx::query(
        "INSERT OR REPLACE INTO realm (name, status, enabled, created_at, secret_current)
         VALUES (?, 'active', 1, ?, ?)",
    )
    .bind(realm)
    .bind(now)
    .bind(secret_current)
    .execute(pool)
    .await
    .expect("seed realm");
}

fn test_registry_manifest(publisher: &str, name: &str, version: &str, target: &str) -> Vec<u8> {
    format!(
        r#"publisher = "{publisher}"
name = "{name}"
version = "{version}"

[binary]
path = "bin/actor.wasm"
target = "{target}"
hash = "0000000000000000000000000000000000000000000000000000000000000000"
"#,
    )
    .into_bytes()
}

async fn seed_active_package(publisher: &str, name: &str, version: &str, target: &str) -> Vec<u8> {
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    let signing_key = SigningKey::generate(&mut OsRng);
    seed_active_package_with_key(publisher, name, version, target, &signing_key).await
}

async fn seed_active_package_with_key(
    publisher: &str,
    name: &str,
    version: &str,
    target: &str,
    signing_key: &ed25519_dalek::SigningKey,
) -> Vec<u8> {
    let pool = platform::storage::db::get_database().get_pool();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let (publisher_id, signing_key_id) =
        seed_publisher_with_key(pool, publisher, signing_key).await;
    let (manifest, signature) =
        build_signed_manifest(signing_key, publisher, name, version, target);
    let manifest_str = std::str::from_utf8(&manifest).expect("test manifest is UTF-8");
    let signature_b64 = base64::prelude::BASE64_STANDARD.encode(signature);

    sqlx::query(
        "INSERT OR REPLACE INTO publisher_package
         (publisher_id, publisher, name, version, type_str, target, manifest, signature, signing_key_id, status, published_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'active', ?)",
    )
    .bind(publisher_id)
    .bind(publisher)
    .bind(name)
    .bind(version)
    .bind(format!("{publisher}/{name}@{version}"))
    .bind(target)
    .bind(manifest_str)
    .bind(signature_b64)
    .bind(signing_key_id)
    .bind(now)
    .execute(pool)
    .await
    .expect("seed publisher package");

    manifest
}

/// Seed an active published package and return both its manifest bytes and the
/// signing key. Published-package (Path 1) registration now requires an AIS-02
/// publisher possession proof, so tests that exercise the success path need
/// the key to produce that proof.
async fn seed_active_package_returning_key(
    publisher: &str,
    name: &str,
    version: &str,
    target: &str,
) -> (Vec<u8>, ed25519_dalek::SigningKey) {
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    let signing_key = SigningKey::generate(&mut OsRng);
    let manifest =
        seed_active_package_with_key(publisher, name, version, target, &signing_key).await;
    (manifest, signing_key)
}

/// Build a published-package (Path 1) register request already carrying a valid
/// AIS-02 publisher possession proof signed by `signing_key` — the
/// publisher's current key, which Path 1 verifies the proof against.
#[allow(clippy::too_many_arguments)]
fn path1_request_with_auth(
    publisher: &str,
    name: &str,
    version: &str,
    realm: &str,
    target: &str,
    manifest: Vec<u8>,
    signing_key: &ed25519_dalek::SigningKey,
    nonce: Vec<u8>,
) -> RegisterRequest {
    let mut request = package_register_request(
        publisher,
        name,
        version,
        realm,
        target,
        manifest.clone(),
        None,
    );
    sign_publisher_request_at(
        &mut request,
        signing_key,
        &manifest,
        test_unix_now_secs(),
        nonce,
    );
    request
}

async fn post_register(
    app: Router,
    request: RegisterRequest,
    realm_secret: Option<&str>,
) -> RegisterResponse {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/register")
        .header("content-type", "application/protobuf")
        .header("x-real-ip", "127.0.0.1");
    if let Some(secret) = realm_secret {
        builder = builder.header(REALM_SECRET_HEADER, secret);
    }

    // The rate limiter keys on the peer IP via ConnectInfo, which the production
    // server injects per connection but oneshot does not, so add it here.
    let mut http_request = builder.body(Body::from(request.encode_to_vec())).unwrap();
    http_request
        .extensions_mut()
        .insert(axum::extract::ConnectInfo(std::net::SocketAddr::from((
            [127, 0, 0, 1],
            12345,
        ))));
    let response = app
        .oneshot(http_request)
        .await
        .expect("register route response");
    let status = response.status();

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("register response body");
    assert_eq!(
        status,
        StatusCode::OK,
        "unexpected /register status {status}: {}",
        String::from_utf8_lossy(&body)
    );
    RegisterResponse::decode(body).expect("decode register response")
}

async fn post_reissue(
    app: Router,
    request: ReissueCredentialRequest,
    realm_secret: Option<&str>,
) -> RegisterResponse {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/reissue")
        .header("content-type", "application/protobuf")
        .header("x-real-ip", "127.0.0.1");
    if let Some(secret) = realm_secret {
        builder = builder.header(REALM_SECRET_HEADER, secret);
    }

    let mut http_request = builder.body(Body::from(request.encode_to_vec())).unwrap();
    http_request
        .extensions_mut()
        .insert(axum::extract::ConnectInfo(std::net::SocketAddr::from((
            [127, 0, 0, 1],
            12345,
        ))));
    let response = app
        .oneshot(http_request)
        .await
        .expect("reissue route response");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("reissue response body");
    assert_eq!(status, StatusCode::OK);
    RegisterResponse::decode(body).expect("decode reissue response")
}

#[tokio::test]
#[serial]
async fn test_register_route_linked_with_realm_secret_succeeds() {
    let env = setup_test_environment().await;
    let realm_secret = "linked-http-route-secret";
    let realm = "linked-http-route@example.com";
    seed_realm(realm, Some(realm_secret)).await;
    let app = create_test_router(&env).await;

    let mut request = linked_register_request();
    request.realm = realm.to_string();
    let response = post_register(app, request, Some(realm_secret)).await;

    match response.result.expect("result") {
        register_response::Result::Success(ok) => {
            assert_eq!(ok.actr_id.realm, realm);
            assert_eq!(registered_type(&ok).name, "source-workload");
        }
        register_response::Result::Error(err) => {
            panic!("linked /register with realm secret should succeed: {err:?}")
        }
    }
}

#[tokio::test]
#[serial]
async fn test_register_pending_failure_rolls_back_acl() {
    let mut env = setup_test_environment().await;
    let realm_secret = "linked-register-atomic-secret";
    let realm = "linked-register-atomic@example.com";
    seed_realm(realm, Some(realm_secret)).await;
    let app = create_test_router(&env).await;
    let pool = platform::storage::db::get_database().get_pool();

    let mut request = linked_register_request();
    request.realm = realm.to_string();
    request.ws_address = Some("wss://example.invalid/actor".to_string());
    request.acl = Some(Acl {
        rules: vec![AclRule {
            permission: acl_rule::Permission::Allow as i32,
            from_type: ActrType {
                publisher: "new-client".to_string(),
                name: "caller".to_string(),
                version: "1.0.0".to_string(),
            },
            source_realm: Some(acl_rule::SourceRealm::Realm(realm.to_string())),
        }],
    });
    let target_type = request.actr_type.to_string_repr();

    sqlx::query("DELETE FROM actoracl WHERE realm = ?1 AND to_type = ?2")
        .bind(realm)
        .bind(&target_type)
        .execute(pool)
        .await
        .expect("clear target ACL");
    sqlx::query(
        "INSERT INTO actoracl
            (realm, source_realm, from_type, to_type, access)
         VALUES (?1, ?1, 'old-client/caller@1.0.0', ?2, 0)",
    )
    .bind(realm)
    .bind(&target_type)
    .execute(pool)
    .await
    .expect("seed old ACL");

    let pending_before: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM pending_registration WHERE realm = ?1")
            .bind(realm)
            .fetch_one(pool)
            .await
            .expect("count pending registrations before");

    sqlx::query("DROP TRIGGER IF EXISTS test_fail_pending_registration")
        .execute(pool)
        .await
        .expect("drop stale test trigger");
    sqlx::query(
        "CREATE TRIGGER test_fail_pending_registration
         BEFORE INSERT ON pending_registration
         BEGIN
             SELECT RAISE(FAIL, 'forced pending registration failure');
         END",
    )
    .execute(pool)
    .await
    .expect("create pending failure trigger");

    let response = post_register(app, request, Some(realm_secret)).await;
    sqlx::query("DROP TRIGGER IF EXISTS test_fail_pending_registration")
        .execute(pool)
        .await
        .expect("drop pending failure trigger");

    match response.result.expect("register result") {
        register_response::Result::Error(error) => assert_eq!(error.code, 500),
        register_response::Result::Success(_) => panic!("forced pending failure must not succeed"),
    }

    let acl_rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT from_type, access FROM actoracl
         WHERE realm = ?1 AND to_type = ?2",
    )
    .bind(realm)
    .bind(&target_type)
    .fetch_all(pool)
    .await
    .expect("read target ACL after rollback");
    assert_eq!(
        acl_rows,
        vec![("old-client/caller@1.0.0".to_string(), 0)],
        "previous type-level ACL must remain intact"
    );

    let pending_after: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM pending_registration WHERE realm = ?1")
            .bind(realm)
            .fetch_one(pool)
            .await
            .expect("count pending registrations after");
    assert_eq!(pending_after, pending_before);

    env.shutdown_signer().await;
}

#[tokio::test]
#[serial]
async fn test_register_sets_pending_retention_and_drops_expired_rows() {
    let mut env = setup_test_environment().await;
    let realm_secret = "linked-register-retention-secret";
    let realm = "linked-register-retention@example.com";
    seed_realm(realm, Some(realm_secret)).await;
    let app = create_test_router(&env).await;
    let pool = platform::storage::db::get_database().get_pool();

    sqlx::query(
        "INSERT INTO pending_registration
            (realm, serial_number, ws_address, created_at, expires_at)
         VALUES (?1, 1, 'wss://example.invalid/stale', 1, 2)",
    )
    .bind(realm)
    .execute(pool)
    .await
    .expect("seed expired pending registration");

    let mut request = linked_register_request();
    request.realm = realm.to_string();
    request.ws_address = Some("wss://example.invalid/actor".to_string());
    let ok = match post_register(app, request, Some(realm_secret))
        .await
        .result
        .expect("register result")
    {
        register_response::Result::Success(ok) => ok,
        register_response::Result::Error(error) => panic!("register failed: {error:?}"),
    };

    let rows: Vec<(i64, Option<i64>)> = sqlx::query_as(
        "SELECT serial_number, expires_at FROM pending_registration WHERE realm = ?1",
    )
    .bind(realm)
    .fetch_all(pool)
    .await
    .expect("read pending registrations");
    let renewal_expires_at = ok
        .renewal_token_expires_at
        .expect("renewal token expiry")
        .seconds;
    assert_eq!(
        rows,
        vec![(
            ok.actr_id.serial_number as i64,
            Some(renewal_expires_at + issuer::renewal::REISSUE_BINDING_GRACE_SECS)
        )],
        "expired pending rows must be dropped and the new row must carry a retention"
    );

    env.shutdown_signer().await;
}

#[tokio::test]
#[serial]
async fn test_renew_and_reissue_extend_pending_retention() {
    let mut env = setup_test_environment().await;
    let realm_secret = "linked-pending-extend-secret";
    let realm = "linked-pending-extend@example.com";
    seed_realm(realm, Some(realm_secret)).await;
    let app = create_test_router(&env).await;
    let pool = platform::storage::db::get_database().get_pool();

    let mut registration = linked_register_request();
    registration.realm = realm.to_string();
    registration.ws_address = Some("wss://example.invalid/actor".to_string());
    let registered = match post_register(app.clone(), registration.clone(), Some(realm_secret))
        .await
        .result
        .expect("register result")
    {
        register_response::Result::Success(ok) => ok,
        register_response::Result::Error(error) => panic!("register failed: {error:?}"),
    };
    let actr_id = registered.actr_id.clone();

    let shorten_retention = || async {
        sqlx::query(
            "UPDATE pending_registration SET expires_at = ?1
             WHERE realm = ?2 AND serial_number = ?3",
        )
        .bind(issuer::renewal::now_secs() + 60)
        .bind(realm)
        .bind(actr_id.serial_number as i64)
        .execute(pool)
        .await
        .expect("shorten pending retention");
    };
    let retention = || async {
        sqlx::query_scalar::<_, Option<i64>>(
            "SELECT expires_at FROM pending_registration
             WHERE realm = ?1 AND serial_number = ?2",
        )
        .bind(realm)
        .bind(actr_id.serial_number as i64)
        .fetch_one(pool)
        .await
        .expect("read pending retention")
    };

    shorten_retention().await;
    let renewed = match post_renew(
        app.clone(),
        RenewCredentialRequest {
            actr_id: actr_id.clone(),
            renewal_token: registered.renewal_token.expect("renewal token"),
            attempt_id: control_attempt_id(),
        },
    )
    .await
    .result
    .expect("renew result")
    {
        renew_credential_response::Result::Success(ok) => ok,
        renew_credential_response::Result::Error(error) => panic!("renew failed: {error:?}"),
    };
    assert_eq!(
        retention().await,
        Some(
            renewed
                .renewal_token_expires_at
                .expect("renewal token expiry")
                .seconds
                + issuer::renewal::REISSUE_BINDING_GRACE_SECS
        )
    );

    shorten_retention().await;
    let reissued = match post_reissue(
        app,
        ReissueCredentialRequest {
            actr_id: actr_id.clone(),
            registration,
            renewal_proof: renewed.renewal_token.clone(),
            attempt_id: control_attempt_id(),
        },
        Some(realm_secret),
    )
    .await
    .result
    .expect("reissue result")
    {
        register_response::Result::Success(ok) => ok,
        register_response::Result::Error(error) => panic!("reissue failed: {error:?}"),
    };
    assert_eq!(
        retention().await,
        Some(
            reissued
                .renewal_token_expires_at
                .expect("renewal token expiry")
                .seconds
                + issuer::renewal::REISSUE_BINDING_GRACE_SECS
        )
    );

    env.shutdown_signer().await;
}

#[tokio::test]
#[serial]
async fn test_reissue_route_preserves_linked_actor_id() {
    let mut env = setup_test_environment().await;
    let realm_secret = "linked-reissue-secret";
    let realm = "linked-reissue@example.com";
    seed_realm(realm, Some(realm_secret)).await;
    let app = create_test_router(&env).await;

    let mut registration = linked_register_request();
    registration.realm = realm.to_string();
    let initial = post_register(app.clone(), registration.clone(), Some(realm_secret)).await;
    let initial_ok = match initial.result.expect("register result") {
        register_response::Result::Success(ok) => ok,
        register_response::Result::Error(err) => panic!("initial registration failed: {err:?}"),
    };

    let response = post_reissue(
        app,
        ReissueCredentialRequest {
            actr_id: initial_ok.actr_id.clone(),
            registration,
            renewal_proof: initial_ok.renewal_token.clone(),
            attempt_id: control_attempt_id(),
        },
        Some(realm_secret),
    )
    .await;
    match response.result.expect("reissue result") {
        register_response::Result::Success(ok) => {
            assert_eq!(ok.actr_id, initial_ok.actr_id);
            assert!(ok.renewal_token.is_some());
            assert!(ok.renewal_token_expires_at.is_some());
        }
        register_response::Result::Error(err) => {
            panic!("credential reissue should preserve the actor id: {err:?}")
        }
    }

    env.shutdown_signer().await;
}

/// P0 regression: a caller that authenticates the workload context (realm
/// secret) but supplies another actor's serial must NOT receive credentials.
/// The registration context is realm/type-wide; only the renewal-token
/// possession proof binds the caller to a specific serial.
#[tokio::test]
#[serial]
async fn test_reissue_route_rejects_foreign_serial() {
    let mut env = setup_test_environment().await;
    let realm_secret = "linked-foreign-serial-secret";
    let realm = "linked-foreign-serial@example.com";
    seed_realm(realm, Some(realm_secret)).await;
    let app = create_test_router(&env).await;

    let mut registration = linked_register_request();
    registration.realm = realm.to_string();

    // Two same-realm/type actors, each with its own renewal token.
    let victim = match post_register(app.clone(), registration.clone(), Some(realm_secret))
        .await
        .result
        .expect("victim register result")
    {
        register_response::Result::Success(ok) => ok,
        register_response::Result::Error(err) => panic!("victim registration failed: {err:?}"),
    };
    let attacker = match post_register(app.clone(), registration.clone(), Some(realm_secret))
        .await
        .result
        .expect("attacker register result")
    {
        register_response::Result::Success(ok) => ok,
        register_response::Result::Error(err) => panic!("attacker registration failed: {err:?}"),
    };
    assert_ne!(victim.actr_id, attacker.actr_id);

    // The attacker presents its own (valid) renewal token as proof but names
    // the victim's serial. Must be rejected: the proof is recorded under the
    // attacker's ActrId, not the victim's.
    let response = post_reissue(
        app.clone(),
        ReissueCredentialRequest {
            actr_id: victim.actr_id.clone(),
            registration: registration.clone(),
            renewal_proof: attacker.renewal_token.clone(),
            attempt_id: control_attempt_id(),
        },
        Some(realm_secret),
    )
    .await;
    match response.result.expect("foreign-serial reissue result") {
        register_response::Result::Error(err) => {
            // A possession failure carries its OWN wire code, distinct from the
            // realm-denial 403, so the client never mistakes it for terminal
            // realm denial.
            assert_eq!(
                err.code, REISSUE_POSSESSION_NOT_PROVEN_CODE,
                "foreign serial should fail possession, not realm denial"
            );
            assert_ne!(
                err.code, 403,
                "possession failure must not reuse realm-denial 403"
            );
            assert!(err.message.contains("possession"));
        }
        register_response::Result::Success(_) => {
            panic!("reissue must not mint credentials for a foreign serial")
        }
    }

    // An arbitrary, never-registered serial must be rejected the same way.
    let mut unknown_id = attacker.actr_id.clone();
    unknown_id.serial_number = attacker.actr_id.serial_number.wrapping_add(987_654_321);
    let response = post_reissue(
        app,
        ReissueCredentialRequest {
            actr_id: unknown_id,
            registration,
            renewal_proof: attacker.renewal_token.clone(),
            attempt_id: control_attempt_id(),
        },
        Some(realm_secret),
    )
    .await;
    match response.result.expect("unknown-serial reissue result") {
        register_response::Result::Error(err) => {
            assert_eq!(
                err.code, REISSUE_POSSESSION_NOT_PROVEN_CODE,
                "unknown serial should fail possession, not realm denial"
            );
        }
        register_response::Result::Success(_) => {
            panic!("reissue must not mint credentials for an unknown serial")
        }
    }

    env.shutdown_signer().await;
}

/// Reissue keeps the Actor type recorded at issuance; a request naming another
/// type is rejected even with a valid possession proof.
#[tokio::test]
#[serial]
async fn test_reissue_route_rejects_changed_actor_type() {
    let mut env = setup_test_environment().await;
    let realm_secret = "linked-reissue-type-secret";
    let realm = "linked-reissue-type@example.com";
    seed_realm(realm, Some(realm_secret)).await;
    let app = create_test_router(&env).await;

    let mut registration = linked_register_request();
    registration.realm = realm.to_string();
    let ok = match post_register(app.clone(), registration.clone(), Some(realm_secret))
        .await
        .result
        .expect("register result")
    {
        register_response::Result::Success(ok) => ok,
        register_response::Result::Error(err) => panic!("registration failed: {err:?}"),
    };

    registration.actr_type.name = "other-workload".to_string();
    let response = post_reissue(
        app,
        ReissueCredentialRequest {
            actr_id: ok.actr_id.clone(),
            registration,
            renewal_proof: ok.renewal_token.clone(),
            attempt_id: control_attempt_id(),
        },
        Some(realm_secret),
    )
    .await;
    assert!(
        matches!(response.result, Some(register_response::Result::Error(_))),
        "reissue must not change the Actor type"
    );

    env.shutdown_signer().await;
}

/// A reissue request without any possession proof must be rejected even when
/// the workload context authenticates.
#[tokio::test]
#[serial]
async fn test_reissue_route_rejects_missing_possession_proof() {
    let mut env = setup_test_environment().await;
    let realm_secret = "linked-missing-proof-secret";
    let realm = "linked-missing-proof@example.com";
    seed_realm(realm, Some(realm_secret)).await;
    let app = create_test_router(&env).await;

    let mut registration = linked_register_request();
    registration.realm = realm.to_string();
    let initial = post_register(app.clone(), registration.clone(), Some(realm_secret)).await;
    let initial_ok = match initial.result.expect("register result") {
        register_response::Result::Success(ok) => ok,
        register_response::Result::Error(err) => panic!("initial registration failed: {err:?}"),
    };

    let response = post_reissue(
        app,
        ReissueCredentialRequest {
            actr_id: initial_ok.actr_id.clone(),
            registration,
            renewal_proof: None,
            attempt_id: control_attempt_id(),
        },
        Some(realm_secret),
    )
    .await;
    match response.result.expect("missing-proof reissue result") {
        register_response::Result::Error(err) => {
            assert_eq!(
                err.code, REISSUE_POSSESSION_NOT_PROVEN_CODE,
                "missing proof is a possession failure, not realm denial"
            );
            assert!(err.message.contains("possession"));
        }
        register_response::Result::Success(_) => {
            panic!("reissue without possession proof must be rejected")
        }
    }

    env.shutdown_signer().await;
}

/// A renewal token that already expired (so `/renew` rejects it) must still
/// prove possession for `/reissue` within the binding grace window — this is
/// the self-healing path for long-offline nodes.
#[tokio::test]
#[serial]
async fn test_reissue_route_accepts_expired_recorded_proof() {
    let mut env = setup_test_environment().await;
    let realm_secret = "linked-expired-proof-secret";
    let realm = "linked-expired-proof@example.com";
    seed_realm(realm, Some(realm_secret)).await;
    let app = create_test_router(&env).await;

    let mut registration = linked_register_request();
    registration.realm = realm.to_string();
    let initial = post_register(app.clone(), registration.clone(), Some(realm_secret)).await;
    let initial_ok = match initial.result.expect("register result") {
        register_response::Result::Success(ok) => ok,
        register_response::Result::Error(err) => panic!("initial registration failed: {err:?}"),
    };

    // Force the recorded token to be expired (but well inside the grace
    // window), as if the node had been offline past the renewal TTL.
    let pool = platform::storage::db::get_database().get_pool();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    sqlx::query("UPDATE issuer_renewal_token SET expires_at = ?1 WHERE actor_id = ?2")
        .bind(now - 3600)
        .bind(initial_ok.actr_id.to_string_repr())
        .execute(pool)
        .await
        .expect("expire recorded renewal token");

    let response = post_reissue(
        app,
        ReissueCredentialRequest {
            actr_id: initial_ok.actr_id.clone(),
            registration,
            renewal_proof: initial_ok.renewal_token.clone(),
            attempt_id: control_attempt_id(),
        },
        Some(realm_secret),
    )
    .await;
    match response.result.expect("expired-proof reissue result") {
        register_response::Result::Success(ok) => {
            assert_eq!(ok.actr_id, initial_ok.actr_id);
            assert!(ok.renewal_token.is_some());
        }
        register_response::Result::Error(err) => {
            panic!("expired-but-recorded proof should still reissue within grace: {err:?}")
        }
    }

    env.shutdown_signer().await;
}

/// A reissue whose response is lost must replay its exact response without
/// repeating the durable state change.
///
/// The client cannot tell "the server never got it" from "the server committed
/// and the response was dropped", so it reuses the same attempt id and exact
/// body. The durable attempt journal must return the cached first result.
#[tokio::test]
#[serial]
async fn test_reissue_route_is_idempotent_across_lost_responses() {
    let mut env = setup_test_environment().await;
    let realm_secret = "linked-replay-secret";
    let realm = "linked-replay@example.com";
    seed_realm(realm, Some(realm_secret)).await;
    let app = create_test_router(&env).await;

    let mut registration = linked_register_request();
    registration.realm = realm.to_string();
    let initial = post_register(app.clone(), registration.clone(), Some(realm_secret)).await;
    let initial_ok = match initial.result.expect("register result") {
        register_response::Result::Success(ok) => ok,
        register_response::Result::Error(err) => panic!("initial registration failed: {err:?}"),
    };

    let request = ReissueCredentialRequest {
        actr_id: initial_ok.actr_id.clone(),
        registration,
        renewal_proof: initial_ok.renewal_token.clone(),
        attempt_id: control_attempt_id(),
    };

    // Three attempts with the identical proof, as if the first two responses
    // never reached the client.
    let mut outcomes = Vec::new();
    for attempt in 1..=3 {
        let response = post_reissue(app.clone(), request.clone(), Some(realm_secret)).await;
        match response.result.expect("reissue result") {
            register_response::Result::Success(ok) => outcomes.push(ok),
            register_response::Result::Error(err) => {
                panic!("reissue attempt {attempt} rejected after lost responses: {err:?}")
            }
        }
    }

    let first = &outcomes[0];
    for (index, outcome) in outcomes.iter().enumerate().skip(1) {
        assert_eq!(
            outcome.encode_to_vec(),
            first.encode_to_vec(),
            "attempt {} must replay the exact cached response",
            index + 1
        );
        assert_eq!(
            outcome.actr_id,
            first.actr_id,
            "attempt {} returned a different ActrId",
            index + 1
        );
        assert_eq!(
            outcome.renewal_token,
            first.renewal_token,
            "attempt {} returned a different successor token; replays must be idempotent",
            index + 1
        );
        assert_eq!(
            outcome.renewal_token_expires_at,
            first.renewal_token_expires_at,
            "attempt {} returned an expiry that disagrees with the stored record",
            index + 1
        );
    }

    // The proof itself must still resolve — that is what lets a fourth retry
    // succeed — and the replays must not have accumulated extra rows.
    let pool = platform::storage::db::get_database().get_pool();
    let proof_hash = issuer::renewal::hash_token(
        initial_ok
            .renewal_token
            .as_ref()
            .expect("initial renewal token"),
    );
    let proof_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issuer_renewal_token WHERE actor_id = ?1 AND token_hash = ?2",
    )
    .bind(initial_ok.actr_id.to_string_repr())
    .bind(&proof_hash)
    .fetch_one(pool)
    .await
    .expect("count proof binding");
    assert_eq!(
        proof_rows, 1,
        "the presented proof must remain resolvable after replays"
    );

    let total_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM issuer_renewal_token WHERE actor_id = ?1")
            .bind(initial_ok.actr_id.to_string_repr())
            .fetch_one(pool)
            .await
            .expect("count actor tokens");
    assert_eq!(
        total_rows, 2,
        "three replays of one attempt must leave exactly the proof and its single successor"
    );

    env.shutdown_signer().await;
}

#[tokio::test]
#[serial]
async fn test_register_route_package_with_publisher_identity_succeeds() {
    let env = setup_test_environment().await;
    let realm = "package-http-route@example.com";
    seed_realm(realm, Some(ROUTE_REALM_SECRET)).await;
    let (manifest_raw, key) =
        seed_active_package_returning_key("httppkg", "PackagedService", "1.0.0", "wasm32-wasip1")
            .await;
    let app = create_test_router(&env).await;

    // AIS-02: a published package now also requires a publisher possession
    // proof signed by the publisher's current key.
    let request = path1_request_with_auth(
        "httppkg",
        "PackagedService",
        "1.0.0",
        realm,
        "wasm32-wasip1",
        manifest_raw,
        &key,
        vec![0x71; 32],
    );
    let response = post_register(app, request, Some(ROUTE_REALM_SECRET)).await;

    match response.result.expect("result") {
        register_response::Result::Success(ok) => {
            assert_eq!(ok.actr_id.realm, realm);
            assert_eq!(registered_type(&ok).publisher, "httppkg");
        }
        register_response::Result::Error(err) => {
            panic!("package /register with Publisher Registry identity should succeed: {err:?}")
        }
    }
}

#[tokio::test]
#[serial]
async fn test_register_route_unspecified_auth_mode_uses_package_identity() {
    let env = setup_test_environment().await;
    let realm = "unspecified-http-route@example.com";
    seed_realm(realm, Some(ROUTE_REALM_SECRET)).await;
    let (manifest_raw, key) =
        seed_active_package_returning_key("legacyhttp", "LegacyService", "1.0.0", "wasm32-wasip1")
            .await;
    let app = create_test_router(&env).await;

    // Omitted auth_mode must still take the package path, and (AIS-02) a
    // published package now requires the publisher possession proof.
    let mut request = RegisterRequest {
        actr_type: ActrType {
            publisher: "legacyhttp".to_string(),
            name: "LegacyService".to_string(),
            version: "1.0.0".to_string(),
        },
        realm: realm.to_string(),
        service_spec: None,
        acl: None,
        service: None,
        ws_address: None,
        manifest_raw: Some(prost::bytes::Bytes::from(manifest_raw.clone())),
        publisher_signature: None,
        target: Some("wasm32-wasip1".to_string()),
        auth_mode: None,
        publisher_auth_signature: None,
        publisher_auth_signed_at: None,
        publisher_auth_nonce: None,
    };
    sign_publisher_request_at(
        &mut request,
        &key,
        &manifest_raw,
        test_unix_now_secs(),
        vec![0x72; 32],
    );
    let response = post_register(app, request, Some(ROUTE_REALM_SECRET)).await;

    match response.result.expect("result") {
        register_response::Result::Success(ok) => {
            assert_eq!(ok.actr_id.realm, realm);
            assert_eq!(registered_type(&ok).publisher, "legacyhttp");
        }
        register_response::Result::Error(err) => {
            panic!("omitted auth_mode should remain package-compatible: {err:?}")
        }
    }
}

#[tokio::test]
#[serial]
async fn test_register_route_published_package_without_manifest_raw_is_rejected() {
    let env = setup_test_environment().await;
    let realm = "published-package-missing-manifest@example.com";
    seed_realm(realm, Some(ROUTE_REALM_SECRET)).await;
    seed_active_package(
        "missingmanifest",
        "PublishedService",
        "1.0.0",
        "wasm32-wasip1",
    )
    .await;
    let app = create_test_router(&env).await;

    let request = RegisterRequest {
        actr_type: ActrType {
            publisher: "missingmanifest".to_string(),
            name: "PublishedService".to_string(),
            version: "1.0.0".to_string(),
        },
        realm: realm.to_string(),
        service_spec: None,
        acl: None,
        service: None,
        ws_address: None,
        manifest_raw: None,
        publisher_signature: None,
        target: Some("wasm32-wasip1".to_string()),
        auth_mode: Some(RegisterAuthMode::Package as i32),
        publisher_auth_signature: None,
        publisher_auth_signed_at: None,
        publisher_auth_nonce: None,
    };
    let response = post_register(app, request, Some(ROUTE_REALM_SECRET)).await;

    match response.result.expect("result") {
        register_response::Result::Error(err) => {
            assert_eq!(err.code, 400);
        }
        register_response::Result::Success(_) => {
            panic!("published package registration without manifest_raw should be rejected")
        }
    }
}

#[tokio::test]
#[serial]
async fn test_register_route_published_package_without_target_is_rejected() {
    let env = setup_test_environment().await;
    let realm = "published-package-missing-target@example.com";
    seed_realm(realm, Some(ROUTE_REALM_SECRET)).await;
    let manifest_raw = seed_active_package(
        "missingtarget",
        "PublishedService",
        "1.0.0",
        "wasm32-wasip1",
    )
    .await;
    let app = create_test_router(&env).await;

    let request = RegisterRequest {
        actr_type: ActrType {
            publisher: "missingtarget".to_string(),
            name: "PublishedService".to_string(),
            version: "1.0.0".to_string(),
        },
        realm: realm.to_string(),
        service_spec: None,
        acl: None,
        service: None,
        ws_address: None,
        manifest_raw: Some(prost::bytes::Bytes::from(manifest_raw)),
        publisher_signature: None,
        target: None,
        auth_mode: Some(RegisterAuthMode::Package as i32),
        publisher_auth_signature: None,
        publisher_auth_signed_at: None,
        publisher_auth_nonce: None,
    };
    let response = post_register(app, request, Some(ROUTE_REALM_SECRET)).await;

    match response.result.expect("result") {
        register_response::Result::Error(err) => {
            assert_eq!(err.code, 400);
        }
        register_response::Result::Success(_) => {
            panic!("published package registration without target should be rejected")
        }
    }
}

#[tokio::test]
#[serial]
async fn test_published_package_manifest_hash_mismatch_does_not_fall_back_to_path2() {
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    let env = setup_test_environment().await;
    let signer_client = create_signer_client(&env.signer_config, &env.shared_key)
        .await
        .expect("signer client");
    let issuer = AIdIssuer::new(
        signer_client,
        default_issuer_config(&env.issuer_temp_dir),
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    .expect("issuer");

    seed_realm("mismatch@example.com", None).await;
    let key = SigningKey::generate(&mut OsRng);
    seed_active_package_with_key(
        "publishedmismatch",
        "MismatchService",
        "1.0.0",
        "wasm32-wasip1",
        &key,
    )
    .await;

    // This manifest is validly signed and would pass Path 2, but it is not the
    // same manifest registered in publisher_package. A published package hash mismatch
    // must be rejected immediately instead of falling through to Path 2.
    let (mut manifest_bytes, _) = build_signed_manifest(
        &key,
        "publishedmismatch",
        "MismatchService",
        "1.0.0",
        "wasm32-wasip1",
    );
    manifest_bytes.extend_from_slice(b"\n# different, but still valid, manifest bytes\n");
    let sig_bytes = key.sign(&manifest_bytes).to_bytes().to_vec();
    let mut request = RegisterRequest {
        actr_type: ActrType {
            publisher: "publishedmismatch".to_string(),
            name: "MismatchService".to_string(),
            version: "1.0.0".to_string(),
        },
        realm: "mismatch@example.com".to_string(),
        service_spec: None,
        acl: None,
        service: None,
        ws_address: None,
        manifest_raw: Some(prost::bytes::Bytes::from(manifest_bytes.clone())),
        publisher_signature: Some(prost::bytes::Bytes::from(sig_bytes)),
        target: Some("wasm32-wasip1".to_string()),
        auth_mode: Some(RegisterAuthMode::Package as i32),
        publisher_auth_signature: None,
        publisher_auth_signed_at: None,
        publisher_auth_nonce: None,
    };
    sign_publisher_request_at(
        &mut request,
        &key,
        &manifest_bytes,
        test_unix_now_secs(),
        vec![0x55; 32],
    );

    let response = issuer.issue_credential(&request).await.expect("issue");
    match response.result.expect("result") {
        register_response::Result::Error(err) => {
            assert_eq!(err.code, 403);
            assert!(err.message.contains("publisher not verified"));
        }
        register_response::Result::Success(_) => {
            panic!("published package manifest hash mismatch should not fall back to Path 2")
        }
    }
}

#[tokio::test]
#[serial]
async fn test_expired_publisher_key_allows_existing_package_and_path2_registration() {
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    let env = setup_test_environment().await;
    let issuer = create_test_issuer(&env).await;
    let realm = "expired-publisher-runtime-verification@example.com";
    seed_realm(realm, None).await;

    let pool = platform::storage::db::get_database().get_pool();
    let key = SigningKey::generate(&mut OsRng);
    let published_manifest = seed_active_package_with_key(
        "expiredruntime",
        "ExistingService",
        "1.0.0",
        "wasm32-wasip1",
        &key,
    )
    .await;
    sqlx::query("UPDATE publisher SET key_expires_at = ? WHERE name = ?")
        .bind(test_unix_now_secs() as i64 - 1)
        .bind("expiredruntime")
        .execute(pool)
        .await
        .expect("expire Publisher Registry publish authority");

    // AIS-02: Path 1 also requires the publisher possession proof. A merely
    // expired (not revoked) key still resolves, so the proof still verifies.
    let path1_request = path1_request_with_auth(
        "expiredruntime",
        "ExistingService",
        "1.0.0",
        realm,
        "wasm32-wasip1",
        published_manifest,
        &key,
        vec![0x56; 32],
    );
    let path1_response = issuer
        .issue_credential(&path1_request)
        .await
        .expect("issue Path 1 credential");
    assert!(
        matches!(
            &path1_response.result,
            Some(register_response::Result::Success(_))
        ),
        "natural key expiry must not invalidate an existing package: {path1_response:?}"
    );

    let (path2_manifest, path2_signature) = build_signed_manifest(
        &key,
        "expiredruntime",
        "UnpublishedService",
        "1.0.0",
        "wasm32-wasip1",
    );
    let mut path2_request = package_register_request(
        "expiredruntime",
        "UnpublishedService",
        "1.0.0",
        realm,
        "wasm32-wasip1",
        path2_manifest.clone(),
        Some(path2_signature),
    );
    sign_publisher_request_at(
        &mut path2_request,
        &key,
        &path2_manifest,
        test_unix_now_secs(),
        vec![0x58; 32],
    );
    let path2_response = issuer
        .issue_credential(&path2_request)
        .await
        .expect("issue Path 2 credential");
    assert!(
        matches!(
            &path2_response.result,
            Some(register_response::Result::Success(_))
        ),
        "natural key expiry must not disable Path 2 verification: {path2_response:?}"
    );
}

#[tokio::test]
#[serial]
async fn test_revoked_published_package_does_not_fall_back_to_path2() {
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    let env = setup_test_environment().await;
    let issuer = create_test_issuer(&env).await;
    let realm = "revoked-package-terminal@example.com";
    seed_realm(realm, None).await;

    let pool = platform::storage::db::get_database().get_pool();
    let key = SigningKey::generate(&mut OsRng);
    let manifest = seed_active_package_with_key(
        "revokedterminal",
        "RevokedService",
        "1.0.0",
        "wasm32-wasip1",
        &key,
    )
    .await;
    sqlx::query(
        "UPDATE publisher_package SET status = 'revoked', revoked_at = ? \
         WHERE type_str = ? AND target = ?",
    )
    .bind(test_unix_now_secs() as i64)
    .bind("revokedterminal/RevokedService@1.0.0")
    .bind("wasm32-wasip1")
    .execute(pool)
    .await
    .expect("revoke package");

    let manifest_signature = key.sign(&manifest).to_bytes().to_vec();
    let mut request = package_register_request(
        "revokedterminal",
        "RevokedService",
        "1.0.0",
        realm,
        "wasm32-wasip1",
        manifest.clone(),
        Some(manifest_signature),
    );
    sign_publisher_request_at(
        &mut request,
        &key,
        &manifest,
        test_unix_now_secs(),
        vec![0x59; 32],
    );

    let response = issuer.issue_credential(&request).await.expect("issue");
    match response.result.expect("result") {
        register_response::Result::Error(err) => {
            assert_eq!(err.code, 403);
            assert!(err.message.contains("revoked"));
        }
        register_response::Result::Success(_) => {
            panic!("a revoked package must be terminal and must not fall back to Path 2")
        }
    }
}

#[tokio::test]
#[serial]
async fn test_suspended_publisher_rejects_path1_and_path2() {
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    let env = setup_test_environment().await;
    let issuer = create_test_issuer(&env).await;
    let realm = "suspended-publisher-runtime-verification@example.com";
    seed_realm(realm, None).await;

    let pool = platform::storage::db::get_database().get_pool();
    let key = SigningKey::generate(&mut OsRng);
    let published_manifest = seed_active_package_with_key(
        "suspendedruntime",
        "ExistingService",
        "1.0.0",
        "wasm32-wasip1",
        &key,
    )
    .await;
    sqlx::query("UPDATE publisher SET status = 'suspended' WHERE name = ?")
        .bind("suspendedruntime")
        .execute(pool)
        .await
        .expect("suspend Publisher Registry");

    let path1_request = package_register_request(
        "suspendedruntime",
        "ExistingService",
        "1.0.0",
        realm,
        "wasm32-wasip1",
        published_manifest,
        None,
    );
    let path1_response = issuer
        .issue_credential(&path1_request)
        .await
        .expect("issue Path 1 credential");
    assert!(
        matches!(
            &path1_response.result,
            Some(register_response::Result::Error(error)) if error.code == 403
        ),
        "suspended Publisher Registry must reject Path 1: {path1_response:?}"
    );

    let (path2_manifest, path2_signature) = build_signed_manifest(
        &key,
        "suspendedruntime",
        "UnpublishedService",
        "1.0.0",
        "wasm32-wasip1",
    );
    let mut path2_request = package_register_request(
        "suspendedruntime",
        "UnpublishedService",
        "1.0.0",
        realm,
        "wasm32-wasip1",
        path2_manifest.clone(),
        Some(path2_signature),
    );
    sign_publisher_request_at(
        &mut path2_request,
        &key,
        &path2_manifest,
        test_unix_now_secs(),
        vec![0x5A; 32],
    );
    let path2_response = issuer
        .issue_credential(&path2_request)
        .await
        .expect("issue Path 2 credential");
    assert!(
        matches!(
            &path2_response.result,
            Some(register_response::Result::Error(error)) if error.code == 403
        ),
        "suspended Publisher Registry must reject Path 2: {path2_response:?}"
    );
}

#[tokio::test]
#[serial]
async fn test_published_package_retired_key_survives_until_explicit_revocation() {
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    let env = setup_test_environment().await;
    let issuer = create_test_issuer(&env).await;
    let realm = "revoked-history-path1@example.com";
    seed_realm(realm, None).await;

    let pool = platform::storage::db::get_database().get_pool();
    let old_key = SigningKey::generate(&mut OsRng);
    let manifest = seed_active_package_with_key(
        "revokedhistory",
        "ExistingService",
        "1.0.0",
        "wasm32-wasip1",
        &old_key,
    )
    .await;
    let (publisher_id, old_key_id): (i64, String) =
        sqlx::query_as("SELECT id, key_id FROM publisher WHERE name = ?")
            .bind("revokedhistory")
            .fetch_one(pool)
            .await
            .expect("load Publisher Registry current key");
    let old_public_key =
        base64::prelude::BASE64_STANDARD.encode(old_key.verifying_key().to_bytes());
    archive_key_to_history(pool, publisher_id, &old_key_id, &old_public_key, "retired").await;

    let new_key = SigningKey::generate(&mut OsRng);
    let new_key_id =
        actrix_publisher_registry::crypto::compute_key_id(&new_key.verifying_key().to_bytes());
    let new_public_key =
        base64::prelude::BASE64_STANDARD.encode(new_key.verifying_key().to_bytes());
    sqlx::query("UPDATE publisher SET public_key = ?, key_id = ? WHERE id = ?")
        .bind(new_public_key)
        .bind(new_key_id)
        .bind(publisher_id)
        .execute(pool)
        .await
        .expect("rotate current Publisher Registry key");

    // AIS-02: the publisher possession proof must be signed by the
    // publisher's CURRENT key (new_key). The stored manifest was signed by the
    // now-retired old_key; retirement keeps the package valid, and the proof
    // proves present control of the publisher.
    let request = path1_request_with_auth(
        "revokedhistory",
        "ExistingService",
        "1.0.0",
        realm,
        "wasm32-wasip1",
        manifest,
        &new_key,
        vec![0x5C; 32],
    );
    let retired_response = issuer
        .issue_credential(&request)
        .await
        .expect("issue with retired key");
    assert!(
        matches!(
            &retired_response.result,
            Some(register_response::Result::Success(_))
        ),
        "ordinary key retirement must not invalidate an existing package: {retired_response:?}"
    );

    sqlx::query(
        "UPDATE publisher_key_history SET status = 'revoked' WHERE publisher_id = ? AND key_id = ?",
    )
    .bind(publisher_id)
    .bind(&old_key_id)
    .execute(pool)
    .await
    .expect("revoke historical key");

    let response = issuer
        .issue_credential(&request)
        .await
        .expect("issue with revoked key");
    assert!(
        matches!(
            &response.result,
            Some(register_response::Result::Error(error)) if error.code == 403
        ),
        "key revocation must invalidate packages signed by that key: {response:?}"
    );
}

#[tokio::test]
#[serial]
async fn test_register_route_package_without_publisher_identity_is_still_rejected() {
    let env = setup_test_environment().await;
    let realm = "package-http-route-missing-publisher@example.com";
    seed_realm(realm, Some(ROUTE_REALM_SECRET)).await;
    let app = create_test_router(&env).await;

    let request = RegisterRequest {
        actr_type: ActrType {
            publisher: "missing-http-publisher".to_string(),
            name: "PackagedService".to_string(),
            version: "1.0.0".to_string(),
        },
        realm: realm.to_string(),
        service_spec: None,
        acl: None,
        service: None,
        ws_address: None,
        manifest_raw: Some(prost::bytes::Bytes::from(test_registry_manifest(
            "missing-http-publisher",
            "PackagedService",
            "1.0.0",
            "wasm32-wasip1",
        ))),
        publisher_signature: None,
        target: Some("wasm32-wasip1".to_string()),
        auth_mode: Some(RegisterAuthMode::Package as i32),
        publisher_auth_signature: None,
        publisher_auth_signed_at: None,
        publisher_auth_nonce: None,
    };
    let response = post_register(app, request, Some(ROUTE_REALM_SECRET)).await;

    match response.result.expect("result") {
        register_response::Result::Error(err) => {
            assert_eq!(err.code, 403);
            assert!(err.message.contains("publisher not verified"));
        }
        register_response::Result::Success(_) => {
            panic!("package /register without Publisher Registry identity should still be rejected")
        }
    }
}

#[tokio::test]
#[serial]
async fn test_linked_registration_with_verified_realm_secret_succeeds() {
    let env = setup_test_environment().await;

    let signer_client = create_signer_client(&env.signer_config, &env.shared_key)
        .await
        .expect("signer client");
    let issuer = AIdIssuer::new(
        signer_client,
        default_issuer_config(&env.issuer_temp_dir),
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    .expect("issuer");

    let response = issuer
        .issue_credential_with_realm_secret_check(
            &linked_register_request(),
            Some(RealmSecretCheck::ValidCurrent),
        )
        .await
        .expect("issue linked credential");

    match response.result.expect("result") {
        register_response::Result::Success(ok) => {
            assert_eq!(registered_type(&ok).name, "source-workload");
            assert_eq!(ok.actr_id.realm, TEST_REALM);
        }
        register_response::Result::Error(err) => {
            panic!("linked registration with verified realm secret should succeed: {err:?}")
        }
    }
}

#[tokio::test]
#[serial]
async fn test_linked_registration_without_verified_realm_secret_is_rejected() {
    let env = setup_test_environment().await;

    let signer_client = create_signer_client(&env.signer_config, &env.shared_key)
        .await
        .expect("signer client");
    let issuer = AIdIssuer::new(
        signer_client,
        default_issuer_config(&env.issuer_temp_dir),
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    .expect("issuer");

    let response = issuer
        .issue_credential_with_realm_secret_check(
            &linked_register_request(),
            Some(RealmSecretCheck::NotConfigured),
        )
        .await
        .expect("issue linked credential");

    match response.result.expect("result") {
        register_response::Result::Error(err) => {
            assert_eq!(err.code, 403);
            assert!(err.message.contains("publisher not verified"));
        }
        register_response::Result::Success(_) => {
            panic!("linked registration without verified realm secret should be rejected")
        }
    }
}

#[tokio::test]
#[serial]
async fn test_package_registration_without_publisher_identity_is_still_rejected() {
    let env = setup_test_environment().await;

    let signer_client = create_signer_client(&env.signer_config, &env.shared_key)
        .await
        .expect("signer client");
    let issuer = AIdIssuer::new(
        signer_client,
        default_issuer_config(&env.issuer_temp_dir),
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    .expect("issuer");

    let request = RegisterRequest {
        actr_type: ActrType {
            publisher: "missing-publisher-for-package-auth-test".to_string(),
            name: "package-workload".to_string(),
            version: "1.0.0".to_string(),
        },
        realm: TEST_REALM.to_string(),
        service_spec: None,
        acl: None,
        service: None,
        ws_address: None,
        manifest_raw: Some(prost::bytes::Bytes::from(test_registry_manifest(
            "missing-publisher-for-package-auth-test",
            "package-workload",
            "1.0.0",
            "wasm32-wasip1",
        ))),
        publisher_signature: None,
        target: Some("wasm32-wasip1".to_string()),
        auth_mode: Some(RegisterAuthMode::Package as i32),
        publisher_auth_signature: None,
        publisher_auth_signed_at: None,
        publisher_auth_nonce: None,
    };

    let response = issuer
        .issue_credential(&request)
        .await
        .expect("issue package credential");

    match response.result.expect("result") {
        register_response::Result::Error(err) => {
            assert_eq!(err.code, 403);
            assert!(err.message.contains("publisher not verified"));
        }
        register_response::Result::Success(_) => {
            panic!(
                "package registration without Publisher Registry identity should still be rejected"
            )
        }
    }
}

#[tokio::test]
#[serial]
async fn test_end_to_end_credential_flow() {
    let env = setup_test_environment().await;

    AIdCredentialValidator::init(env.issuer_temp_dir.path())
        .await
        .expect("Failed to initialize validator");

    let signer_client = create_signer_client(&env.signer_config, &env.shared_key)
        .await
        .expect("Failed to create Signer gRPC client");
    let issuer = AIdIssuer::new(
        signer_client,
        default_issuer_config(&env.issuer_temp_dir),
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    .expect("Failed to create issuer");

    // Seed database with Publisher Registry and package data so verify_publisher_identity path-1 passes
    let (manifest_raw, key) =
        seed_active_package_returning_key("acme", "test-device", "1.0.0", "wasm32-wasip1").await;

    // AIS-02: published-package registration requires the publisher possession proof.
    let mut request = RegisterRequest {
        actr_type: ActrType {
            publisher: "acme".to_string(),
            name: "test-device".to_string(),
            version: "1.0.0".to_string(),
        },
        realm: TEST_REALM.to_string(),
        service_spec: None,
        acl: None,
        service: None,
        ws_address: None,
        manifest_raw: Some(prost::bytes::Bytes::from(manifest_raw.clone())),
        publisher_signature: None,
        target: Some("wasm32-wasip1".to_string()),
        auth_mode: Some(RegisterAuthMode::Package as i32),
        publisher_auth_signature: None,
        publisher_auth_signed_at: None,
        publisher_auth_nonce: None,
    };
    sign_publisher_request_at(
        &mut request,
        &key,
        &manifest_raw,
        test_unix_now_secs(),
        vec![0x10; 32],
    );

    let response = issuer
        .issue_credential(&request)
        .await
        .expect("Failed to issue credential");

    let register_ok = match response.result.expect("Response should contain result") {
        register_response::Result::Success(ok) => ok,
        register_response::Result::Error(err) => panic!("Expected success but got error: {err:?}"),
    };

    assert!(
        !register_ok.turn_credential.username.is_empty(),
        "TURN credential should be present"
    );
    assert!(
        register_ok.credential_expires_at.is_some(),
        "Credential expiry should be present"
    );
    assert_eq!(register_ok.actr_id.realm, TEST_REALM);
    assert!(register_ok.actr_id.serial_number > 0);

    let (claims, _) =
        AIdCredentialValidator::check(&register_ok.credential, TEST_REALM, "actr.signaling")
            .await
            .expect("Token validation should succeed");
    assert_eq!(claims.realm, TEST_REALM);
    assert!(
        !claims.actor_id.is_empty(),
        "Actor ID should be present in claims"
    );
    assert_eq!(
        claims.actor_id,
        register_ok.actr_id.to_string_repr(),
        "Actor ID must use <serial_hex>.<realm_id>@<domain>"
    );
    assert_eq!(claims.issuer, TEST_REALM);
    assert_eq!(claims.actor_type, request.actr_type);

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs();
    assert!(claims.expires_at > now);
    assert!(claims.expires_at <= now + 3600);

    let wrong_realm_result = AIdCredentialValidator::check(
        &register_ok.credential,
        "other@example.com",
        "actr.signaling",
    )
    .await;
    assert!(
        wrong_realm_result.is_err(),
        "Validation should fail with a mismatched Realm"
    );

    // Issue and validate multiple credentials to verify stability. Each
    // registration consumes a one-time publisher nonce, so give every
    // iteration a distinct nonce.
    for idx in 0..5 {
        let mut req = RegisterRequest {
            actr_type: ActrType {
                publisher: "acme".to_string(),
                name: "test-device".to_string(),
                version: "1.0.0".to_string(),
            },
            realm: TEST_REALM.to_string(),
            service_spec: None,
            acl: None,
            service: None,
            ws_address: None,
            manifest_raw: Some(prost::bytes::Bytes::from(manifest_raw.clone())),
            publisher_signature: None,
            target: Some("wasm32-wasip1".to_string()),
            auth_mode: Some(RegisterAuthMode::Package as i32),
            publisher_auth_signature: None,
            publisher_auth_signed_at: None,
            publisher_auth_nonce: None,
        };
        sign_publisher_request_at(
            &mut req,
            &key,
            &manifest_raw,
            test_unix_now_secs(),
            vec![0x20 + idx as u8; 32],
        );

        let rsp = issuer
            .issue_credential(&req)
            .await
            .unwrap_or_else(|e| panic!("Failed to issue credential {idx}: {e}"));
        let ok = match rsp.result.expect("Response should contain result") {
            register_response::Result::Success(ok) => ok,
            register_response::Result::Error(err) => {
                panic!("Expected success for token {idx}, got error: {err:?}")
            }
        };

        let (claims, _) =
            AIdCredentialValidator::check(&ok.credential, TEST_REALM, "actr.signaling")
                .await
                .unwrap_or_else(|e| panic!("Failed to validate credential {idx}: {e}"));
        assert_eq!(claims.realm, TEST_REALM);
    }
}

#[tokio::test]
#[serial]
async fn test_issuer_health_checks() {
    let env = setup_test_environment().await;

    let signer_client = create_signer_client(&env.signer_config, &env.shared_key)
        .await
        .expect("Failed to create Signer gRPC client");
    let issuer = AIdIssuer::new(
        signer_client,
        default_issuer_config(&env.issuer_temp_dir),
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    .expect("Failed to create issuer");

    issuer
        .check_database_health()
        .await
        .expect("Database health check should pass");
    issuer
        .check_key_cache_health()
        .await
        .expect("Key cache health check should pass");
}

#[tokio::test]
#[serial]
async fn test_issuer_rotate_key_updates_current_key() {
    let env = setup_test_environment().await;

    let signer_client = create_signer_client(&env.signer_config, &env.shared_key)
        .await
        .expect("Failed to create Signer gRPC client");
    let issuer = AIdIssuer::new(
        signer_client,
        default_issuer_config(&env.issuer_temp_dir),
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    .expect("Failed to create issuer");

    let key_before = issuer
        .get_current_key_id()
        .await
        .expect("current key before rotate");
    let rotated = issuer.rotate_key().await.expect("rotate key");
    let key_after = issuer
        .get_current_key_id()
        .await
        .expect("current key after rotate");

    assert_ne!(key_before, rotated, "rotate_key should change key id");
    assert_eq!(key_after, rotated, "current key should match rotated key");

    issuer
        .check_signer_health()
        .await
        .expect("Signer health should still pass after rotation");
    let cache = issuer
        .check_key_cache_health()
        .await
        .expect("key cache should remain healthy");
    assert_eq!(cache.key_id, rotated);
}

#[tokio::test]
#[serial]
async fn test_issuer_creation_fails_with_wrong_shared_key() {
    let env = setup_test_environment().await;

    let signer_client = create_signer_client(&env.signer_config, "wrong-shared-key")
        .await
        .expect("gRPC channel creation should succeed even with wrong secret");

    // With lazy Signer connection, issuer creation succeeds — auth failure happens on first use
    let issuer = AIdIssuer::new(
        signer_client,
        default_issuer_config(&env.issuer_temp_dir),
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    .expect("issuer creation succeeds with lazy Signer connection");

    // Credential issuance should return an error response due to wrong shared key
    let request = actr_protocol::RegisterRequest {
        actr_type: actr_protocol::ActrType {
            publisher: "acme".to_string(),
            name: "bad-key-test".to_string(),
            version: "1.0.0".to_string(),
        },
        realm: TEST_REALM.to_string(),
        service_spec: None,
        acl: None,
        service: None,
        ws_address: None,
        manifest_raw: None,
        publisher_signature: None,
        target: None,
        auth_mode: Some(RegisterAuthMode::Package as i32),
        publisher_auth_signature: None,
        publisher_auth_signed_at: None,
        publisher_auth_nonce: None,
    };
    let resp = issuer
        .issue_credential(&request)
        .await
        .expect("issue_credential returns Ok wrapping the error");
    assert!(
        matches!(
            resp.result,
            Some(actr_protocol::register_response::Result::Error(_))
        ),
        "expected error result in register response with wrong shared key, got {:?}",
        resp.result
    );
}

#[tokio::test]
#[serial]
async fn test_issuer_check_signer_health_fails_after_signer_shutdown() {
    let mut env = setup_test_environment().await;

    let signer_client = create_signer_client(&env.signer_config, &env.shared_key)
        .await
        .expect("Failed to create Signer gRPC client");
    let issuer = AIdIssuer::new(
        signer_client,
        default_issuer_config(&env.issuer_temp_dir),
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    .expect("Failed to create issuer");

    env.shutdown_signer().await;

    for _ in 0..40 {
        match issuer.check_signer_health().await {
            Ok(()) => tokio::time::sleep(Duration::from_millis(50)).await,
            Err(err) => {
                let msg = err.to_string();
                assert!(
                    msg.contains("Signer service unhealthy")
                        || msg.contains("Failed")
                        || msg.contains("transport")
                        || msg.contains("unavailable")
                        || msg.contains("connection"),
                    "unexpected Signer health error after shutdown: {msg}"
                );
                return;
            }
        }
    }

    panic!("issuer Signer health should fail after embedded Signer shutdown");
}

#[tokio::test]
#[serial]
async fn test_issuer_rotate_key_fails_when_signer_is_unavailable() {
    let mut env = setup_test_environment().await;

    let signer_client = create_signer_client(&env.signer_config, &env.shared_key)
        .await
        .expect("Failed to create Signer gRPC client");
    let issuer = AIdIssuer::new(
        signer_client,
        default_issuer_config(&env.issuer_temp_dir),
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    .expect("Failed to create issuer");

    env.shutdown_signer().await;

    for _ in 0..40 {
        match issuer.rotate_key().await {
            Ok(_) => tokio::time::sleep(Duration::from_millis(50)).await,
            Err(err) => {
                let msg = err.to_string();
                assert!(
                    msg.contains("Signer unavailable")
                        || msg.contains("Failed")
                        || msg.contains("transport")
                        || msg.contains("connection"),
                    "unexpected rotate_key error after Signer shutdown: {msg}"
                );
                return;
            }
        }
    }

    panic!("rotate_key should fail after embedded Signer shutdown");
}

// ═══════════════════════════════════════════════════════════════════════════
// Path 2 tests: verify_publisher_identity with manifest_raw + publisher_signature
// ═══════════════════════════════════════════════════════════════════════════

/// Helper: build a signed manifest TOML string and its Ed25519 signature.
fn build_signed_manifest(
    signing_key: &ed25519_dalek::SigningKey,
    publisher: &str,
    name: &str,
    version: &str,
    target: &str,
) -> (Vec<u8>, Vec<u8>) {
    use ed25519_dalek::Signer;

    let key_id =
        actrix_publisher_registry::crypto::compute_key_id(&signing_key.verifying_key().to_bytes());
    let manifest = format!(
        r#"publisher = "{publisher}"
name = "{name}"
version = "{version}"
signing_key_id = "{key_id}"

[binary]
path = "bin/actor.wasm"
target = "{target}"
hash = "0000000000000000000000000000000000000000000000000000000000000000"
"#,
    );
    let manifest_bytes = manifest.into_bytes();
    let signature = signing_key.sign(&manifest_bytes);
    (manifest_bytes, signature.to_bytes().to_vec())
}

fn test_unix_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn sign_publisher_request_at(
    request: &mut RegisterRequest,
    signing_key: &ed25519_dalek::SigningKey,
    manifest_bytes: &[u8],
    signed_at: u64,
    nonce: Vec<u8>,
) {
    use ed25519_dalek::Signer;
    use sha2::{Digest, Sha256};

    let target = request
        .target
        .as_deref()
        .expect("test request should carry target");
    let digest = Sha256::digest(manifest_bytes);
    let manifest_sha256_hex = hex::encode(digest);
    let payload =
        actr_protocol::build_publisher_register_payload(actr_protocol::PublisherRegisterPayload {
            realm: &request.realm,
            actr_type: &request.actr_type,
            target,
            manifest_sha256_hex: &manifest_sha256_hex,
            publisher_auth_signed_at: signed_at,
            publisher_auth_nonce: &nonce,
        });
    let signature = signing_key.sign(payload.as_bytes());

    request.publisher_auth_signature =
        Some(prost::bytes::Bytes::from(signature.to_bytes().to_vec()));
    request.publisher_auth_signed_at = Some(signed_at);
    request.publisher_auth_nonce = Some(prost::bytes::Bytes::from(nonce));
}

fn sign_publisher_request(
    request: &mut RegisterRequest,
    signing_key: &ed25519_dalek::SigningKey,
    manifest_bytes: &[u8],
) {
    sign_publisher_request_at(
        request,
        signing_key,
        manifest_bytes,
        test_unix_now_secs(),
        vec![0x42; 32],
    );
}

/// Helper: seed an Publisher Registry with a specific keypair and return (publisher_id, key_id, public_key_b64).
async fn seed_publisher_with_key(
    pool: &sqlx::SqlitePool,
    publisher_name: &str,
    signing_key: &ed25519_dalek::SigningKey,
) -> (i64, String) {
    let pub_b64 = base64::prelude::BASE64_STANDARD.encode(signing_key.verifying_key().to_bytes());
    let key_id =
        actrix_publisher_registry::crypto::compute_key_id(&signing_key.verifying_key().to_bytes());
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let expires_at = now + 86400 * 365;

    sqlx::query(
        "INSERT OR REPLACE INTO publisher (name, public_key, key_id, contact, status, created_at, verified_at, key_expires_at) \
         VALUES (?, ?, ?, 'test@example.com', 'active', ?, ?, ?)"
    )
    .bind(publisher_name)
    .bind(&pub_b64)
    .bind(&key_id)
    .bind(now)
    .bind(now)
    .bind(expires_at)
    .execute(pool)
    .await
    .expect("seed publisher");

    let publisher_id: i64 = sqlx::query_scalar("SELECT id FROM publisher WHERE name = ?")
        .bind(publisher_name)
        .fetch_one(pool)
        .await
        .expect("get publisher id");

    (publisher_id, key_id)
}

/// Helper: archive a key into publisher_key_history with given status.
async fn archive_key_to_history(
    pool: &sqlx::SqlitePool,
    publisher_id: i64,
    key_id: &str,
    public_key_b64: &str,
    status: &str,
) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    sqlx::query(
        "INSERT INTO publisher_key_history (publisher_id, key_id, public_key, status, created_at, retired_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(publisher_id)
    .bind(key_id)
    .bind(public_key_b64)
    .bind(status)
    .bind(now - 1000)
    .bind(now)
    .execute(pool)
    .await
    .expect("archive key to history");
}

/// Path 2: missing publisher_auth_signature is rejected for unpublished packages.
#[tokio::test]
#[serial]
async fn test_path2_missing_publisher_auth_signature_rejected() {
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    let env = setup_test_environment().await;
    let signer_client = create_signer_client(&env.signer_config, &env.shared_key)
        .await
        .expect("signer client");
    let issuer = AIdIssuer::new(
        signer_client,
        default_issuer_config(&env.issuer_temp_dir),
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    .expect("issuer");

    let pool = platform::storage::db::get_database().get_pool();
    seed_realm(TEST_REALM, None).await;

    let key = SigningKey::generate(&mut OsRng);
    let (_publisher_id, _key_id) = seed_publisher_with_key(pool, "missrunner", &key).await;
    let (manifest_bytes, sig_bytes) = build_signed_manifest(
        &key,
        "missrunner",
        "MissingRunnerService",
        "1.0.0",
        "wasm32-wasip1",
    );

    let request = RegisterRequest {
        actr_type: ActrType {
            publisher: "missrunner".to_string(),
            name: "MissingRunnerService".to_string(),
            version: "1.0.0".to_string(),
        },
        realm: TEST_REALM.to_string(),
        service_spec: None,
        acl: None,
        service: None,
        ws_address: None,
        manifest_raw: Some(prost::bytes::Bytes::from(manifest_bytes)),
        publisher_signature: Some(prost::bytes::Bytes::from(sig_bytes)),
        target: Some("wasm32-wasip1".to_string()),
        auth_mode: Some(RegisterAuthMode::Package as i32),
        publisher_auth_signature: None,
        publisher_auth_signed_at: None,
        publisher_auth_nonce: None,
    };

    let response = issuer.issue_credential(&request).await.expect("issue");
    match response.result.expect("result") {
        register_response::Result::Error(_) => {}
        register_response::Result::Success(_) => {
            panic!("Path 2 without publisher_auth_signature should be rejected")
        }
    }
}

#[tokio::test]
#[serial]
async fn test_path2_missing_publisher_record_returns_forbidden() {
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    let env = setup_test_environment().await;
    let issuer = create_test_issuer(&env).await;
    seed_realm("missing-publisher-path2@example.com", None).await;

    let key = SigningKey::generate(&mut OsRng);
    let (manifest, signature) = build_signed_manifest(
        &key,
        "missingpath2publisher",
        "UnpublishedService",
        "1.0.0",
        "wasm32-wasip1",
    );
    let mut request = package_register_request(
        "missingpath2publisher",
        "UnpublishedService",
        "1.0.0",
        "missing-publisher-path2@example.com",
        "wasm32-wasip1",
        manifest.clone(),
        Some(signature),
    );
    sign_publisher_request_at(
        &mut request,
        &key,
        &manifest,
        test_unix_now_secs(),
        vec![0x5B; 32],
    );

    let response = issuer.issue_credential(&request).await.expect("issue");
    assert!(
        matches!(
            &response.result,
            Some(register_response::Result::Error(error)) if error.code == 403
        ),
        "a missing Publisher Registry record is an authorization rejection, not an internal error: {response:?}"
    );
}

/// Path 2: a manifest that does not declare `binary.target` is rejected.
/// The Publisher Registry signature must bind target; Issuer must not fall back to the
/// request's target, otherwise one signed manifest could register under
/// arbitrary targets.
#[tokio::test]
#[serial]
async fn test_path2_manifest_without_binary_target_is_rejected() {
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    let env = setup_test_environment().await;
    let signer_client = create_signer_client(&env.signer_config, &env.shared_key)
        .await
        .expect("signer client");
    let issuer = AIdIssuer::new(
        signer_client,
        default_issuer_config(&env.issuer_temp_dir),
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    .expect("issuer");

    let pool = platform::storage::db::get_database().get_pool();
    seed_realm(TEST_REALM, None).await;

    let key = SigningKey::generate(&mut OsRng);
    let (_publisher_id, _key_id) = seed_publisher_with_key(pool, "notargetpublisher", &key).await;

    // Validly signed manifest that omits [binary].target.
    let key_id = actrix_publisher_registry::crypto::compute_key_id(&key.verifying_key().to_bytes());
    let manifest = format!(
        r#"publisher = "notargetpublisher"
name = "NoTargetService"
version = "1.0.0"
signing_key_id = "{key_id}"

[binary]
path = "bin/actor.wasm"
hash = "0000000000000000000000000000000000000000000000000000000000000000"
"#,
    );
    let manifest_bytes = manifest.into_bytes();
    let sig_bytes = key.sign(&manifest_bytes).to_bytes().to_vec();

    let mut request = RegisterRequest {
        actr_type: ActrType {
            publisher: "notargetpublisher".to_string(),
            name: "NoTargetService".to_string(),
            version: "1.0.0".to_string(),
        },
        realm: TEST_REALM.to_string(),
        service_spec: None,
        acl: None,
        service: None,
        ws_address: None,
        manifest_raw: Some(prost::bytes::Bytes::from(manifest_bytes.clone())),
        publisher_signature: Some(prost::bytes::Bytes::from(sig_bytes)),
        target: Some("wasm32-wasip1".to_string()),
        auth_mode: Some(RegisterAuthMode::Package as i32),
        publisher_auth_signature: None,
        publisher_auth_signed_at: None,
        publisher_auth_nonce: None,
    };
    // Provide a valid publisher proof so the request clears the five-field and
    // signature checks and reaches the binary.target guard.
    sign_publisher_request(&mut request, &key, &manifest_bytes);

    let response = issuer.issue_credential(&request).await.expect("issue");
    match response.result.expect("result") {
        register_response::Result::Error(err) => {
            assert_eq!(
                err.code, 400,
                "manifest without binary.target should be InvalidFormat (400)"
            );
        }
        register_response::Result::Success(_) => {
            panic!("manifest without binary.target should be rejected")
        }
    }
}

/// Path 2: replaying the same publisher_auth_nonce is rejected after the first success.
#[tokio::test]
#[serial]
async fn test_path2_publisher_auth_nonce_replay_rejected() {
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    let env = setup_test_environment().await;
    let signer_client = create_signer_client(&env.signer_config, &env.shared_key)
        .await
        .expect("signer client");
    let issuer = AIdIssuer::new(
        signer_client,
        default_issuer_config(&env.issuer_temp_dir),
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    .expect("issuer");

    let pool = platform::storage::db::get_database().get_pool();
    seed_realm(TEST_REALM, None).await;

    let key = SigningKey::generate(&mut OsRng);
    let (_publisher_id, _key_id) = seed_publisher_with_key(pool, "replaypublisher", &key).await;
    let (manifest_bytes, sig_bytes) = build_signed_manifest(
        &key,
        "replaypublisher",
        "ReplayService",
        "1.0.0",
        "wasm32-wasip1",
    );

    let mut request = RegisterRequest {
        actr_type: ActrType {
            publisher: "replaypublisher".to_string(),
            name: "ReplayService".to_string(),
            version: "1.0.0".to_string(),
        },
        realm: TEST_REALM.to_string(),
        service_spec: None,
        acl: None,
        service: None,
        ws_address: None,
        manifest_raw: Some(prost::bytes::Bytes::from(manifest_bytes.clone())),
        publisher_signature: Some(prost::bytes::Bytes::from(sig_bytes)),
        target: Some("wasm32-wasip1".to_string()),
        auth_mode: Some(RegisterAuthMode::Package as i32),
        publisher_auth_signature: None,
        publisher_auth_signed_at: None,
        publisher_auth_nonce: None,
    };
    sign_publisher_request_at(
        &mut request,
        &key,
        &manifest_bytes,
        test_unix_now_secs(),
        vec![0x99; 32],
    );

    let first = issuer
        .issue_credential(&request)
        .await
        .expect("first issue");
    match first.result.expect("first result") {
        register_response::Result::Success(_) => {}
        register_response::Result::Error(err) => {
            panic!("first Path 2 registration should succeed, got error: {err:?}")
        }
    }

    let second = issuer
        .issue_credential(&request)
        .await
        .expect("second issue");
    match second.result.expect("second result") {
        register_response::Result::Error(_) => {}
        register_response::Result::Success(_) => {
            panic!("replayed publisher_auth_nonce should be rejected")
        }
    }
}

/// Path 2: historical (retired) key passes signature verification.
#[tokio::test]
#[serial]
async fn test_path2_historical_retired_key_passes() {
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    let env = setup_test_environment().await;
    let signer_client = create_signer_client(&env.signer_config, &env.shared_key)
        .await
        .expect("signer client");
    let issuer = AIdIssuer::new(
        signer_client,
        default_issuer_config(&env.issuer_temp_dir),
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    .expect("issuer");

    let pool = platform::storage::db::get_database().get_pool();

    // Key A (old) and Key B (current)
    let key_a = SigningKey::generate(&mut OsRng);
    let key_b = SigningKey::generate(&mut OsRng);

    let key_a_pub_b64 = base64::prelude::BASE64_STANDARD.encode(key_a.verifying_key().to_bytes());
    let key_a_id =
        actrix_publisher_registry::crypto::compute_key_id(&key_a.verifying_key().to_bytes());

    // Seed Publisher Registry with Key B as current
    let (publisher_id, _key_b_id) = seed_publisher_with_key(pool, "histpublisher", &key_b).await;

    // Archive Key A as retired
    archive_key_to_history(pool, publisher_id, &key_a_id, &key_a_pub_b64, "retired").await;

    // Seed realm
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    sqlx::query("INSERT OR IGNORE INTO realm (name, status, enabled, created_at, secret_current) VALUES ('test@example.com', 'active', 1, ?, 'seed-placeholder-secret')")
        .bind(now)
        .execute(pool)
        .await
        .expect("seed realm");

    // Build manifest signed by Key A (the retired key)
    let (manifest_bytes, sig_bytes) = build_signed_manifest(
        &key_a,
        "histpublisher",
        "HistService",
        "1.0.0",
        "wasm32-wasip1",
    );

    let mut request = RegisterRequest {
        actr_type: ActrType {
            publisher: "histpublisher".to_string(),
            name: "HistService".to_string(),
            version: "1.0.0".to_string(),
        },
        realm: TEST_REALM.to_string(),
        service_spec: None,
        acl: None,
        service: None,
        ws_address: None,
        manifest_raw: Some(prost::bytes::Bytes::from(manifest_bytes.clone())),
        publisher_signature: Some(prost::bytes::Bytes::from(sig_bytes)),
        target: Some("wasm32-wasip1".to_string()),
        auth_mode: Some(RegisterAuthMode::Package as i32),
        publisher_auth_signature: None,
        publisher_auth_signed_at: None,
        publisher_auth_nonce: None,
    };
    sign_publisher_request(&mut request, &key_a, &manifest_bytes);

    let response = issuer.issue_credential(&request).await.expect("issue");
    match response.result.expect("result") {
        register_response::Result::Success(_) => {} // expected
        register_response::Result::Error(err) => {
            panic!("Path 2 with retired historical key should succeed, got error: {err:?}")
        }
    }
}

/// Path 2: revoked key is rejected.
#[tokio::test]
#[serial]
async fn test_path2_revoked_key_rejected() {
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    let env = setup_test_environment().await;
    let signer_client = create_signer_client(&env.signer_config, &env.shared_key)
        .await
        .expect("signer client");
    let issuer = AIdIssuer::new(
        signer_client,
        default_issuer_config(&env.issuer_temp_dir),
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    .expect("issuer");

    let pool = platform::storage::db::get_database().get_pool();

    // Seed realm
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    sqlx::query("INSERT OR IGNORE INTO realm (name, status, enabled, created_at, secret_current) VALUES ('test@example.com', 'active', 1, ?, 'seed-placeholder-secret')")
        .bind(now)
        .execute(pool)
        .await
        .expect("seed realm");

    let key_revoked = SigningKey::generate(&mut OsRng);
    let key_current = SigningKey::generate(&mut OsRng);

    let revoked_pub_b64 =
        base64::prelude::BASE64_STANDARD.encode(key_revoked.verifying_key().to_bytes());
    let revoked_key_id =
        actrix_publisher_registry::crypto::compute_key_id(&key_revoked.verifying_key().to_bytes());

    // Seed Publisher Registry with current key
    let (publisher_id, _) = seed_publisher_with_key(pool, "revpublisher", &key_current).await;

    // Archive revoked key
    archive_key_to_history(
        pool,
        publisher_id,
        &revoked_key_id,
        &revoked_pub_b64,
        "revoked",
    )
    .await;

    // Build manifest signed by the revoked key
    let (manifest_bytes, sig_bytes) = build_signed_manifest(
        &key_revoked,
        "revpublisher",
        "RevService",
        "1.0.0",
        "wasm32-wasip1",
    );

    let mut request = RegisterRequest {
        actr_type: ActrType {
            publisher: "revpublisher".to_string(),
            name: "RevService".to_string(),
            version: "1.0.0".to_string(),
        },
        realm: TEST_REALM.to_string(),
        service_spec: None,
        acl: None,
        service: None,
        ws_address: None,
        manifest_raw: Some(prost::bytes::Bytes::from(manifest_bytes.clone())),
        publisher_signature: Some(prost::bytes::Bytes::from(sig_bytes)),
        target: Some("wasm32-wasip1".to_string()),
        auth_mode: Some(RegisterAuthMode::Package as i32),
        publisher_auth_signature: None,
        publisher_auth_signed_at: None,
        publisher_auth_nonce: None,
    };
    sign_publisher_request(&mut request, &key_revoked, &manifest_bytes);

    let response = issuer.issue_credential(&request).await.expect("issue");
    match response.result.expect("result") {
        register_response::Result::Error(_) => {} // expected: revoked key rejected
        register_response::Result::Success(_) => {
            panic!("Path 2 with revoked key should be rejected, but got success")
        }
    }
}

/// Path 2: manifest identity mismatch (actr_type spoofing) is rejected.
#[tokio::test]
#[serial]
async fn test_path2_identity_mismatch_rejected() {
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    let env = setup_test_environment().await;
    let signer_client = create_signer_client(&env.signer_config, &env.shared_key)
        .await
        .expect("signer client");
    let issuer = AIdIssuer::new(
        signer_client,
        default_issuer_config(&env.issuer_temp_dir),
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    .expect("issuer");

    let pool = platform::storage::db::get_database().get_pool();

    // Seed realm (shared global DB may not have it if run in isolation)
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    sqlx::query("INSERT OR IGNORE INTO realm (name, status, enabled, created_at, secret_current) VALUES ('test@example.com', 'active', 1, ?, 'seed-placeholder-secret')")
        .bind(now)
        .execute(pool)
        .await
        .expect("seed realm");

    let key = SigningKey::generate(&mut OsRng);
    let (_publisher_id, _key_id) = seed_publisher_with_key(pool, "spoofpublisher", &key).await;

    // Build manifest for ServiceA
    let (manifest_bytes, sig_bytes) =
        build_signed_manifest(&key, "spoofpublisher", "ServiceA", "1.0.0", "wasm32-wasip1");

    // But register as ServiceB — identity mismatch!
    let mut request = RegisterRequest {
        actr_type: ActrType {
            publisher: "spoofpublisher".to_string(),
            name: "ServiceB".to_string(), // ← does not match manifest
            version: "1.0.0".to_string(),
        },
        realm: TEST_REALM.to_string(),
        service_spec: None,
        acl: None,
        service: None,
        ws_address: None,
        manifest_raw: Some(prost::bytes::Bytes::from(manifest_bytes.clone())),
        publisher_signature: Some(prost::bytes::Bytes::from(sig_bytes)),
        target: Some("wasm32-wasip1".to_string()),
        auth_mode: Some(RegisterAuthMode::Package as i32),
        publisher_auth_signature: None,
        publisher_auth_signed_at: None,
        publisher_auth_nonce: None,
    };
    sign_publisher_request(&mut request, &key, &manifest_bytes);

    let response = issuer.issue_credential(&request).await.expect("issue");
    match response.result.expect("result") {
        register_response::Result::Error(_) => {} // expected: identity mismatch rejected
        register_response::Result::Success(_) => {
            panic!("Path 2 with identity mismatch should be rejected, but got success")
        }
    }
}

// ============================================================================
// Publisher signature matrix tests (Group A / B / C + dedicated cases)
//
// These cover the gaps left by the scenario matrix in the design doc:
// missing-field, tamper/signature-invalidity, time-window, concurrent nonce,
// Path 1 triple-ignored, Linked triple-rejected, and renewal bypass.
// ============================================================================

/// Shared Path 2 fixture: realm seeded, Publisher Registry + active key registered, and a
/// validly signed manifest. No `publisher_package` row is seeded, so registration
/// always falls through to Path 2 (signature + publisher proof).
///
/// `env` is held for the fixture's lifetime: dropping it would drop the
/// embedded signer's shutdown sender, which resolves `serve_with_shutdown`
/// and tears the signer down before the test can call it.
struct Path2Fixture {
    _env: TestEnv,
    issuer: AIdIssuer,
    key: ed25519_dalek::SigningKey,
    wrong_key: ed25519_dalek::SigningKey,
    manifest_bytes: Vec<u8>,
    sig_bytes: Vec<u8>,
    publisher: String,
    name: String,
    target: String,
}

async fn path2_fixture(publisher: &str, name: &str, target: &str) -> Path2Fixture {
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    let env = setup_test_environment().await;
    let signer_client = create_signer_client(&env.signer_config, &env.shared_key)
        .await
        .expect("signer client");
    let issuer = AIdIssuer::new(
        signer_client,
        default_issuer_config(&env.issuer_temp_dir),
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    .expect("issuer");

    let pool = platform::storage::db::get_database().get_pool();
    seed_realm(TEST_REALM, None).await;

    let key = SigningKey::generate(&mut OsRng);
    let wrong_key = SigningKey::generate(&mut OsRng);
    let (_publisher_id, _key_id) = seed_publisher_with_key(pool, publisher, &key).await;
    let (manifest_bytes, sig_bytes) = build_signed_manifest(&key, publisher, name, "1.0.0", target);

    Path2Fixture {
        _env: env,
        issuer,
        key,
        wrong_key,
        manifest_bytes,
        sig_bytes,
        publisher: publisher.to_string(),
        name: name.to_string(),
        target: target.to_string(),
    }
}

/// Build a Path 2 `RegisterRequest` matching the fixture's manifest, with all
/// publisher fields left empty. The caller signs (and optionally mutates) it.
fn base_path2_request(fx: &Path2Fixture, realm: &str) -> RegisterRequest {
    RegisterRequest {
        actr_type: ActrType {
            publisher: fx.publisher.clone(),
            name: fx.name.clone(),
            version: "1.0.0".to_string(),
        },
        realm: realm.to_string(),
        service_spec: None,
        acl: None,
        service: None,
        ws_address: None,
        manifest_raw: Some(prost::bytes::Bytes::from(fx.manifest_bytes.clone())),
        publisher_signature: Some(prost::bytes::Bytes::from(fx.sig_bytes.clone())),
        target: Some(fx.target.clone()),
        auth_mode: Some(RegisterAuthMode::Package as i32),
        publisher_auth_signature: None,
        publisher_auth_signed_at: None,
        publisher_auth_nonce: None,
    }
}

async fn post_renew(app: Router, request: RenewCredentialRequest) -> RenewCredentialResponse {
    let builder = Request::builder()
        .method("POST")
        .uri("/renew")
        .header("content-type", "application/protobuf")
        .header("x-real-ip", "127.0.0.1");
    let mut http_request = builder.body(Body::from(request.encode_to_vec())).unwrap();
    http_request
        .extensions_mut()
        .insert(axum::extract::ConnectInfo(std::net::SocketAddr::from((
            [127, 0, 0, 1],
            12345,
        ))));
    let response = app
        .oneshot(http_request)
        .await
        .expect("renew route response");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("renew response body");
    assert_eq!(
        status,
        StatusCode::OK,
        "unexpected /renew status {status}: {}",
        String::from_utf8_lossy(&body)
    );
    RenewCredentialResponse::decode(body).expect("decode renew response")
}

#[tokio::test]
#[serial]
async fn test_renew_rotation_deletes_expired_token_rows() {
    let mut env = setup_test_environment().await;
    let realm_secret = "renew-expired-cleanup-secret";
    let realm = "renew-expired-cleanup@example.com";
    seed_realm(realm, Some(realm_secret)).await;
    let app = create_test_router(&env).await;

    let mut registration = linked_register_request();
    registration.realm = realm.to_string();
    let initial = post_register(app.clone(), registration, Some(realm_secret)).await;
    let initial_ok = match initial.result.expect("register result") {
        register_response::Result::Success(ok) => ok,
        register_response::Result::Error(error) => panic!("register failed: {error:?}"),
    };
    let actor_id = initial_ok.actr_id;
    let renewal_token = initial_ok.renewal_token.expect("initial renewal token");
    let actor_id_repr = actor_id.to_string_repr();
    let now = test_unix_now_secs() as i64;
    let pool = platform::storage::db::get_database().get_pool();

    sqlx::query("UPDATE issuer_renewal_token SET expires_at = ?1 WHERE actor_id = ?2")
        .bind(now + 60)
        .bind(&actor_id_repr)
        .execute(pool)
        .await
        .expect("move presented token into rotation window");
    sqlx::query(
        "INSERT INTO issuer_renewal_token (actor_id, token_hash, expires_at, created_at)
         VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(&actor_id_repr)
    .bind(vec![0xEE; 32])
    .bind(now - 1)
    .bind(now - 10)
    .execute(pool)
    .await
    .expect("seed expired token row");

    let response = post_renew(
        app,
        RenewCredentialRequest {
            actr_id: actor_id.clone(),
            renewal_token,
            attempt_id: control_attempt_id(),
        },
    )
    .await;
    assert!(matches!(
        response.result,
        Some(renew_credential_response::Result::Success(_))
    ));

    let expired_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issuer_renewal_token
         WHERE actor_id = ?1 AND expires_at <= ?2",
    )
    .bind(&actor_id_repr)
    .bind(now)
    .fetch_one(pool)
    .await
    .expect("count expired rows after rotation");
    assert_eq!(expired_rows, 0);

    env.shutdown_signer().await;
}

type FieldBlank<'a> = &'a dyn Fn(&mut RegisterRequest);
type TamperMutation<'a> = &'a dyn Fn(&mut RegisterRequest, &Path2Fixture, u64);

/// Group A: any one of the publisher triple missing → rejected (PublisherNotVerified).
/// Covers scenario-matrix #2 / #3 / #4 as a single table.
#[tokio::test]
#[serial]
async fn test_path2_missing_publisher_field_matrix() {
    let fx = path2_fixture("missfieldpublisher", "MissFieldService", "wasm32-wasip1").await;
    let now = test_unix_now_secs();

    let cases: &[(&str, FieldBlank)] = &[
        ("missing_signature", &|r| r.publisher_auth_signature = None),
        ("missing_signed_at", &|r| r.publisher_auth_signed_at = None),
        ("missing_nonce", &|r| r.publisher_auth_nonce = None),
    ];

    for (i, (label, blank)) in cases.iter().enumerate() {
        let mut request = base_path2_request(&fx, TEST_REALM);
        sign_publisher_request_at(
            &mut request,
            &fx.key,
            &fx.manifest_bytes,
            now,
            vec![i as u8 + 0x40; 32],
        );
        blank(&mut request);

        let response = fx.issuer.issue_credential(&request).await.expect("issue");
        match response.result.expect("result") {
            register_response::Result::Error(err) => assert_eq!(
                err.code, 403,
                "case {label}: expected PublisherNotVerified (403), got {}",
                err.code
            ),
            register_response::Result::Success(_) => {
                panic!("case {label}: missing publisher field should be rejected")
            }
        }
    }
}

/// Group B: tamper / wrong-key matrix. A valid proof is signed first, then one
/// field is mutated per row. All tamper rows must be rejected (403); the
/// control row succeeds (covers #5 happy path).
/// Covers scenario-matrix #5 / #6 / #7 / #9 / #10 / #11 / #12.
#[tokio::test]
#[serial]
async fn test_path2_tamper_and_signature_matrix() {
    let fx = path2_fixture("tamperpublisher", "TamperService", "wasm32-wasip1").await;
    let now = test_unix_now_secs();

    let cases: &[(&str, bool, TamperMutation)] = &[
        ("control", true, &|_r, _f, _n| {}),
        ("wrong_key", false, &|r, f, n| {
            // overwrite the valid proof with one signed by an unregistered key
            sign_publisher_request_at(r, &f.wrong_key, &f.manifest_bytes, n, vec![0x21; 32]);
        }),
        ("tamper_realm", false, &|r, _f, _n| {
            r.realm = "other@example.com".to_string()
        }),
        ("tamper_target", false, &|r, _f, _n| {
            r.target = Some("x86_64-unknown-linux-gnu".to_string());
        }),
        ("tamper_manifest_raw", false, &|r, _f, _n| {
            // flip a byte inside the manifest hash field: still valid TOML, but
            // the Publisher Registry signature over the original bytes no longer verifies.
            let mut bytes = r.manifest_raw.as_deref().unwrap().to_vec();
            if let Some(pos) = bytes.iter().rposition(|&b| b == b'0') {
                bytes[pos] = b'1';
            }
            r.manifest_raw = Some(prost::bytes::Bytes::from(bytes));
        }),
        ("tamper_signed_at", false, &|r, _f, n| {
            r.publisher_auth_signed_at = Some(n + 1); // still in window, so sig mismatch is what fails
        }),
        ("tamper_nonce", false, &|r, _f, _n| {
            r.publisher_auth_nonce = Some(prost::bytes::Bytes::from(vec![0xEE; 32]));
        }),
    ];

    for (i, (label, expect_success, mutation)) in cases.iter().enumerate() {
        let mut request = base_path2_request(&fx, TEST_REALM);
        let nonce = vec![i as u8 + 0x30; 32];
        sign_publisher_request_at(&mut request, &fx.key, &fx.manifest_bytes, now, nonce);
        mutation(&mut request, &fx, now);

        let response = fx.issuer.issue_credential(&request).await.expect("issue");
        match (response.result.expect("result"), expect_success) {
            (register_response::Result::Success(_), true) => {}
            (register_response::Result::Error(err), false) => assert_eq!(
                err.code, 403,
                "case {label}: expected PublisherNotVerified (403), got {}",
                err.code
            ),
            (register_response::Result::Success(_), false) => {
                panic!("case {label}: tamper should be rejected, got success")
            }
            (register_response::Result::Error(err), true) => {
                panic!("case {label}: control should succeed, got error {err:?}")
            }
        }
    }
}

/// Group C: `publisher_auth_signed_at` time window. `verify_publisher_auth_signed_at` has two
/// branches (Expired / InvalidTimestamp) with zero coverage today.
/// Covers scenario-matrix #13 / #14 plus both boundaries.
#[tokio::test]
#[serial]
async fn test_path2_publisher_auth_signed_at_window_matrix() {
    let fx = path2_fixture("winpublisher", "WinService", "wasm32-wasip1").await;
    let now = test_unix_now_secs();

    // (label, signed_at, expected: None = success, Some(code) = error)
    let cases: &[(&str, u64, Option<u32>)] = &[
        ("control_now", now, None),
        ("too_old", now - 600, Some(401)), // Expired (MAX_AGE = 300s)
        ("too_future", now + 300, Some(400)), // InvalidTimestamp (FUTURE_SKEW = 60s)
        ("boundary_back", now - 301, Some(401)), // first failing past boundary
        // Keep this comfortably beyond the 60s allowance: using exactly now+61
        // races the wall clock while the matrix executes and can become valid.
        ("future_guard", now + 120, Some(400)),
    ];

    for (i, (label, signed_at, expect)) in cases.iter().enumerate() {
        let mut request = base_path2_request(&fx, TEST_REALM);
        sign_publisher_request_at(
            &mut request,
            &fx.key,
            &fx.manifest_bytes,
            *signed_at,
            vec![i as u8 + 0x10; 32],
        );
        let response = fx.issuer.issue_credential(&request).await.expect("issue");
        match (response.result.expect("result"), expect) {
            (register_response::Result::Success(_), None) => {}
            (register_response::Result::Error(err), Some(code)) => assert_eq!(
                err.code, *code,
                "case {label}: expected code {code}, got {}",
                err.code
            ),
            (register_response::Result::Success(_), Some(code)) => {
                panic!("case {label}: expected error {code}, got success")
            }
            (register_response::Result::Error(err), None) => {
                panic!("case {label}: expected success, got error {err:?}")
            }
        }
    }
}

/// #16: two concurrent registrations with the same publisher nonce — the
/// UNIQUE(publisher, key_id, nonce) constraint must let exactly one win.
#[tokio::test]
#[serial]
async fn test_path2_concurrent_nonce_only_one_succeeds() {
    let fx = path2_fixture("concurrencypublisher", "ConcurrentService", "wasm32-wasip1").await;
    let now = test_unix_now_secs();

    let build = || {
        let mut r = base_path2_request(&fx, TEST_REALM);
        sign_publisher_request_at(&mut r, &fx.key, &fx.manifest_bytes, now, vec![0xC0; 32]);
        r
    };
    let req_a = build();
    let req_b = build();

    let (resp_a, resp_b) = tokio::join!(
        fx.issuer.issue_credential(&req_a),
        fx.issuer.issue_credential(&req_b)
    );

    let success_a = matches!(
        resp_a.expect("issue a").result,
        Some(register_response::Result::Success(_))
    );
    let success_b = matches!(
        resp_b.expect("issue b").result,
        Some(register_response::Result::Success(_))
    );
    assert!(
        success_a ^ success_b,
        "exactly one concurrent registration should succeed (a={success_a}, b={success_b})"
    );
}

/// AIS-02: a published-package (Path 1) registration carrying a VALID publisher
/// possession proof (fresh nonce + signature by the publisher's current key)
/// succeeds. Mirrors `test_register_route_package_with_publisher_identity_succeeds`.
#[tokio::test]
#[serial]
async fn test_path1_published_package_with_publisher_triple_succeeds() {
    let env = setup_test_environment().await;
    let realm = "package-triple-route@example.com";
    seed_realm(realm, Some(ROUTE_REALM_SECRET)).await;
    let (manifest_raw, key) =
        seed_active_package_returning_key("triplepkg", "TripleService", "1.0.0", "wasm32-wasip1")
            .await;
    let app = create_test_router(&env).await;

    let request = path1_request_with_auth(
        "triplepkg",
        "TripleService",
        "1.0.0",
        realm,
        "wasm32-wasip1",
        manifest_raw,
        &key,
        vec![0x77; 32],
    );
    let response = post_register(app, request, Some(ROUTE_REALM_SECRET)).await;

    match response.result.expect("result") {
        register_response::Result::Success(ok) => {
            assert_eq!(ok.actr_id.realm, realm);
            assert_eq!(registered_type(&ok).publisher, "triplepkg");
        }
        register_response::Result::Error(err) => {
            panic!("Path 1 with a valid publisher proof should succeed, got error: {err:?}")
        }
    }
}

/// AIS-02: a published-package (Path 1) registration that presents a valid
/// manifest and target but NO publisher possession proof must now be
/// rejected. The published package, the realm secret, and the public manifest
/// are all obtainable without controlling the publisher, so they are no longer
/// sufficient on their own.
#[tokio::test]
#[serial]
async fn test_path1_published_package_without_publisher_auth_is_rejected() {
    let env = setup_test_environment().await;
    let realm = "package-no-auth-route@example.com";
    seed_realm(realm, Some(ROUTE_REALM_SECRET)).await;
    let manifest_raw =
        seed_active_package("noauthpkg", "NoAuthService", "1.0.0", "wasm32-wasip1").await;
    let app = create_test_router(&env).await;

    // Manifest + target present, publisher triple absent.
    let request = package_register_request(
        "noauthpkg",
        "NoAuthService",
        "1.0.0",
        realm,
        "wasm32-wasip1",
        manifest_raw,
        None,
    );
    let response = post_register(app, request, Some(ROUTE_REALM_SECRET)).await;

    match response.result.expect("result") {
        register_response::Result::Error(err) => {
            assert_eq!(
                err.code, 403,
                "published package without a publisher proof must be PublisherNotVerified (403)"
            );
            assert!(err.message.contains("publisher not verified"));
        }
        register_response::Result::Success(_) => {
            panic!("published package registration without a publisher proof must be rejected")
        }
    }
}

/// #18: Linked auth must reject any publisher triple (InvalidFormat). The triple
/// must not leak into the Linked path; Linked enforces its absence.
#[tokio::test]
#[serial]
async fn test_linked_auth_with_publisher_triple_rejected() {
    let env = setup_test_environment().await;
    let realm_secret = "linked-triple-secret";
    let realm = "linked-triple@example.com";
    seed_realm(realm, Some(realm_secret)).await;
    let app = create_test_router(&env).await;

    let mut request = linked_register_request();
    request.realm = realm.to_string();
    request.publisher_auth_signature =
        Some(prost::bytes::Bytes::from_static(b"should-not-be-present"));
    request.publisher_auth_signed_at = Some(test_unix_now_secs());
    request.publisher_auth_nonce = Some(prost::bytes::Bytes::from(vec![0x88; 32]));

    let response = post_register(app, request, Some(realm_secret)).await;
    match response.result.expect("result") {
        register_response::Result::Error(err) => assert_eq!(
            err.code, 400,
            "Linked with publisher triple should be InvalidFormat (400), got {}",
            err.code
        ),
        register_response::Result::Success(_) => {
            panic!("Linked auth should reject publisher triple presence")
        }
    }
}

/// #17: renewal_token renewal does not require (and cannot carry) a publisher
/// triple. Register a published package, then renew via `POST /issuer/renew`.
#[tokio::test]
#[serial]
async fn test_renewal_token_renew_without_publisher_triple() {
    let env = setup_test_environment().await;
    let realm = "renew-route@example.com";
    seed_realm(realm, Some(ROUTE_REALM_SECRET)).await;
    let (manifest_raw, key) =
        seed_active_package_returning_key("renewpkg", "RenewService", "1.0.0", "wasm32-wasip1")
            .await;
    let app = create_test_router(&env).await;

    // AIS-02: the initial published-package registration requires the
    // publisher possession proof; the subsequent renew does not.
    let register = path1_request_with_auth(
        "renewpkg",
        "RenewService",
        "1.0.0",
        realm,
        "wasm32-wasip1",
        manifest_raw,
        &key,
        vec![0x73; 32],
    );
    // `post_register` consumes the Router; clone it so the renew call hits the
    // same issuer (shared via Arc<AIdIssuer> inside IssuerState).
    let reg_response = post_register(app.clone(), register, Some(ROUTE_REALM_SECRET)).await;
    let ok = match reg_response.result.expect("register result") {
        register_response::Result::Success(ok) => ok,
        register_response::Result::Error(err) => {
            panic!("register should succeed before renew, got error: {err:?}")
        }
    };
    let actr_id = ok.actr_id;
    let renewal_token = ok
        .renewal_token
        .expect("register should issue a renewal_token");

    let renew_request = RenewCredentialRequest {
        actr_id,
        renewal_token,
        attempt_id: control_attempt_id(),
    };
    let renew_response = post_renew(app, renew_request).await;
    match renew_response.result.expect("renew result") {
        renew_credential_response::Result::Success(ok) => {
            let new_token = ok
                .renewal_token
                .expect("renew should issue a rotated renewal_token");
            assert!(!new_token.is_empty(), "renew should rotate the token");
        }
        renew_credential_response::Result::Error(err) => {
            panic!("renewal without publisher triple should succeed, got error: {err:?}")
        }
    }
}

/// Issue a `/renew` request and return the raw HTTP status alongside the decoded
/// response. Unlike [`post_renew`], this does not assert a 200 status, so it can
/// exercise rejection paths (revocation, realm/package/publisher denial) that
/// surface as a non-2xx status.
async fn post_renew_raw(
    app: Router,
    request: RenewCredentialRequest,
) -> (StatusCode, RenewCredentialResponse) {
    let builder = Request::builder()
        .method("POST")
        .uri("/renew")
        .header("content-type", "application/protobuf")
        .header("x-real-ip", "127.0.0.1");
    let mut http_request = builder.body(Body::from(request.encode_to_vec())).unwrap();
    http_request
        .extensions_mut()
        .insert(axum::extract::ConnectInfo(std::net::SocketAddr::from((
            [127, 0, 0, 1],
            12345,
        ))));
    let response = app
        .oneshot(http_request)
        .await
        .expect("renew route response");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("renew response body");
    (
        status,
        RenewCredentialResponse::decode(body).expect("decode renew response"),
    )
}

/// AIS-04: an actor placed on the revocation list can no longer renew, even
/// while it still holds a valid, unexpired renewal token.
#[tokio::test]
#[serial]
async fn test_renew_rejected_after_actor_revoked() {
    let mut env = setup_test_environment().await;
    let realm_secret = "linked-revoked-renew-secret";
    let realm = "linked-revoked-renew@example.com";
    seed_realm(realm, Some(realm_secret)).await;
    let app = create_test_router(&env).await;

    let mut registration = linked_register_request();
    registration.realm = realm.to_string();
    let initial = post_register(app.clone(), registration, Some(realm_secret)).await;
    let ok = match initial.result.expect("register result") {
        register_response::Result::Success(ok) => ok,
        register_response::Result::Error(err) => panic!("register failed: {err:?}"),
    };

    issuer::renewal::revoke_actor(&ok.actr_id, "security-review-test")
        .await
        .expect("revoke actor");

    let renew_request = RenewCredentialRequest {
        actr_id: ok.actr_id.clone(),
        renewal_token: ok.renewal_token.clone().expect("renewal token"),
        attempt_id: vec![0xa1; 16].into(),
    };
    let (status, response) = post_renew_raw(app, renew_request).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a revoked actor must be forbidden from renewing"
    );
    match response.result.expect("renew result") {
        renew_credential_response::Result::Error(err) => {
            assert_eq!(err.code, 403, "revoked actor renew must be forbidden");
        }
        renew_credential_response::Result::Success(_) => {
            panic!("a revoked actor must not be able to renew")
        }
    }

    env.shutdown_signer().await;
}

/// AIS-04: an actor placed on the revocation list can no longer reissue.
#[tokio::test]
#[serial]
async fn test_reissue_rejected_after_actor_revoked() {
    let mut env = setup_test_environment().await;
    let realm_secret = "linked-revoked-reissue-secret";
    let realm = "linked-revoked-reissue@example.com";
    seed_realm(realm, Some(realm_secret)).await;
    let app = create_test_router(&env).await;

    let mut registration = linked_register_request();
    registration.realm = realm.to_string();
    let initial = post_register(app.clone(), registration.clone(), Some(realm_secret)).await;
    let ok = match initial.result.expect("register result") {
        register_response::Result::Success(ok) => ok,
        register_response::Result::Error(err) => panic!("register failed: {err:?}"),
    };

    issuer::renewal::revoke_actor(&ok.actr_id, "security-review-test")
        .await
        .expect("revoke actor");

    let response = post_reissue(
        app,
        ReissueCredentialRequest {
            actr_id: ok.actr_id.clone(),
            registration,
            renewal_proof: ok.renewal_token.clone(),
            attempt_id: vec![0xb2; 16].into(),
        },
        Some(realm_secret),
    )
    .await;
    match response.result.expect("reissue result") {
        register_response::Result::Error(err) => {
            assert_eq!(err.code, 403, "revoked actor reissue must be forbidden");
        }
        register_response::Result::Success(_) => {
            panic!("a revoked actor must not be able to reissue")
        }
    }

    env.shutdown_signer().await;
}

/// AIS-03: renewing a package identity re-checks package lifecycle. A package
/// that was revoked after the credential was first issued must block re-issuance
/// even though the renewal token is still valid.
#[tokio::test]
#[serial]
async fn test_issue_for_actor_rejected_after_package_revoked() {
    let env = setup_test_environment().await;
    let issuer = create_test_issuer(&env).await;
    let realm = "renew-revoked-package@example.com";
    seed_realm(realm, None).await;
    let (manifest_raw, key) = seed_active_package_returning_key(
        "revokepkg",
        "RevokeRenewService",
        "1.0.0",
        "wasm32-wasip1",
    )
    .await;

    // AIS-02: the initial published-package registration requires the publisher proof.
    let request = path1_request_with_auth(
        "revokepkg",
        "RevokeRenewService",
        "1.0.0",
        realm,
        "wasm32-wasip1",
        manifest_raw,
        &key,
        vec![0x74; 32],
    );
    let ok = match issuer
        .issue_credential(&request)
        .await
        .expect("issue")
        .result
        .expect("result")
    {
        register_response::Result::Success(ok) => ok,
        register_response::Result::Error(err) => panic!("register failed: {err:?}"),
    };

    // Re-issuing for the actor succeeds while the package is still active.
    issuer
        .issue_credential_for_actor(&ok.actr_id, None)
        .await
        .expect("reissue before revoke should succeed");

    let pool = platform::storage::db::get_database().get_pool();
    sqlx::query(
        "UPDATE publisher_package SET status = 'revoked', revoked_at = ? WHERE type_str = ?",
    )
    .bind(test_unix_now_secs() as i64)
    .bind("revokepkg/RevokeRenewService@1.0.0")
    .execute(pool)
    .await
    .expect("revoke package");

    let err = issuer
        .issue_credential_for_actor(&ok.actr_id, None)
        .await
        .expect_err("a revoked package must block re-issuance");
    assert!(
        matches!(err, platform::aid::AidError::PackageRevoked),
        "expected PackageRevoked, got {err:?}"
    );
}

/// AIS-03: renewing a package identity re-checks the publisher. A publisher that
/// was suspended after issuance must block re-issuance for its packages.
#[tokio::test]
#[serial]
async fn test_issue_for_actor_rejected_after_publisher_suspended() {
    let env = setup_test_environment().await;
    let issuer = create_test_issuer(&env).await;
    let realm = "renew-suspended-publisher@example.com";
    seed_realm(realm, None).await;
    let (manifest_raw, key) = seed_active_package_returning_key(
        "suspendpub",
        "SuspendRenewService",
        "1.0.0",
        "wasm32-wasip1",
    )
    .await;

    // AIS-02: the initial published-package registration requires the publisher proof.
    let request = path1_request_with_auth(
        "suspendpub",
        "SuspendRenewService",
        "1.0.0",
        realm,
        "wasm32-wasip1",
        manifest_raw,
        &key,
        vec![0x75; 32],
    );
    let ok = match issuer
        .issue_credential(&request)
        .await
        .expect("issue")
        .result
        .expect("result")
    {
        register_response::Result::Success(ok) => ok,
        register_response::Result::Error(err) => panic!("register failed: {err:?}"),
    };

    let pool = platform::storage::db::get_database().get_pool();
    sqlx::query("UPDATE publisher SET status = 'suspended' WHERE name = ?")
        .bind("suspendpub")
        .execute(pool)
        .await
        .expect("suspend publisher");

    let err = issuer
        .issue_credential_for_actor(&ok.actr_id, None)
        .await
        .expect_err("a suspended publisher must block re-issuance");
    assert!(
        matches!(err, platform::aid::AidError::PublisherNotVerified),
        "expected PublisherNotVerified, got {err:?}"
    );
}

/// AIS-04 proof invalidation: once a possession proof has been consumed by a
/// reissue and its short replay window has elapsed, replaying that same proof
/// must be rejected — a single leaked proof cannot be chained indefinitely.
#[tokio::test]
#[serial]
async fn test_reissue_proof_rejected_after_replay_window() {
    let mut env = setup_test_environment().await;
    let realm_secret = "linked-proof-window-secret";
    let realm = "linked-proof-window@example.com";
    seed_realm(realm, Some(realm_secret)).await;
    let app = create_test_router(&env).await;

    let mut registration = linked_register_request();
    registration.realm = realm.to_string();
    let initial = post_register(app.clone(), registration.clone(), Some(realm_secret)).await;
    let initial_ok = match initial.result.expect("register result") {
        register_response::Result::Success(ok) => ok,
        register_response::Result::Error(err) => panic!("register failed: {err:?}"),
    };

    let request = ReissueCredentialRequest {
        actr_id: initial_ok.actr_id.clone(),
        registration,
        renewal_proof: initial_ok.renewal_token.clone(),
        attempt_id: vec![0xc3; 16].into(),
    };

    // First reissue consumes the proof and mints the deterministic successor.
    let first = post_reissue(app.clone(), request.clone(), Some(realm_secret)).await;
    let first_ok = match first.result.expect("first reissue result") {
        register_response::Result::Success(ok) => ok,
        register_response::Result::Error(err) => panic!("first reissue rejected: {err:?}"),
    };
    let successor = first_ok
        .renewal_token
        .clone()
        .expect("first reissue must return a successor token");

    // Age the successor row's creation time (the proof's consumption marker) past
    // the replay window, while leaving the original proof binding inside the
    // 30-day grace so only the window — not grace — is what closes.
    let pool = platform::storage::db::get_database().get_pool();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let successor_hash = issuer::renewal::hash_token(&successor);
    let updated =
        sqlx::query("UPDATE issuer_renewal_token SET created_at = ?1 WHERE token_hash = ?2")
            .bind(now - (issuer::renewal::REISSUE_PROOF_REPLAY_WINDOW_SECS + 3600))
            .bind(&successor_hash)
            .execute(pool)
            .await
            .expect("age successor created_at");
    assert_eq!(
        updated.rows_affected(),
        1,
        "test must have found and aged exactly the successor row"
    );

    // Replaying the ORIGINAL proof after the window closed must fail possession.
    let replay = post_reissue(app, request, Some(realm_secret)).await;
    match replay.result.expect("replay reissue result") {
        register_response::Result::Error(err) => {
            assert_eq!(
                err.code, REISSUE_POSSESSION_NOT_PROVEN_CODE,
                "a proof consumed past its replay window must fail possession"
            );
        }
        register_response::Result::Success(_) => {
            panic!("a spent proof past its replay window must not mint another successor")
        }
    }

    env.shutdown_signer().await;
}

/// A nonce is public authorization context, not a bearer credential for cached
/// reissue responses. Exact retries must work; modified credentials must not.
// Reloading the signing key from storage uses `block_in_place`.
#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn test_reissue_cache_requires_original_authenticated_request() {
    let fx = path2_fixture("cache.example.com", "CacheBinding", "wasm32-wasip1").await;
    let app = create_test_router(&fx._env).await;
    let mut registration = base_path2_request(&fx, TEST_REALM);
    sign_publisher_request(&mut registration, &fx.key, &fx.manifest_bytes);
    let initial = post_register(app.clone(), registration.clone(), Some("seed-realm-secret")).await;
    let initial = match initial.result.unwrap() {
        register_response::Result::Success(ok) => ok,
        other => panic!("initial registration failed: {other:?}"),
    };
    sign_publisher_request_at(
        &mut registration,
        &fx.key,
        &fx.manifest_bytes,
        test_unix_now_secs(),
        vec![0x93; 32],
    );
    let request = ReissueCredentialRequest {
        actr_id: initial.actr_id.clone(),
        registration,
        renewal_proof: initial.renewal_token,
        attempt_id: control_attempt_id(),
    };
    let first = post_reissue(app.clone(), request.clone(), Some("seed-realm-secret")).await;
    assert!(matches!(
        &first.result,
        Some(register_response::Result::Success(_))
    ));
    // A new issuer instance must still recognize the durable request binding.
    let restarted = create_test_router(&fx._env).await;
    let replay = post_reissue(restarted, request.clone(), Some("seed-realm-secret")).await;
    assert_eq!(first.encode_to_vec(), replay.encode_to_vec());

    let mut mutations = Vec::new();
    let mut missing_proof = request.clone();
    missing_proof.renewal_proof = None;
    mutations.push(missing_proof);
    let mut wrong_proof = request.clone();
    wrong_proof.renewal_proof = Some(prost::bytes::Bytes::from(vec![0xff; 32]));
    mutations.push(wrong_proof);
    let mut missing_signature = request.clone();
    missing_signature.registration.publisher_auth_signature = None;
    mutations.push(missing_signature);
    let mut wrong_actor = request.clone();
    wrong_actor.actr_id.serial_number += 1;
    mutations.push(wrong_actor);
    let mut wrong_target = request.clone();
    wrong_target.registration.target = Some("aarch64-apple-darwin".to_string());
    mutations.push(wrong_target);
    let mut linked = request.clone();
    linked.registration.auth_mode = Some(RegisterAuthMode::Linked as i32);
    mutations.push(linked);
    for changed in mutations {
        let response = post_reissue(app.clone(), changed, Some("seed-realm-secret")).await;
        assert!(
            matches!(response.result, Some(register_response::Result::Error(_))),
            "a changed request must not retrieve cached credentials"
        );
    }

    // Cached credentials must not bypass an actor revocation either.
    issuer::renewal::revoke_actor(&request.actr_id, "cache regression")
        .await
        .unwrap();
    let revoked = post_reissue(app, request, Some("seed-realm-secret")).await;
    assert!(matches!(
        revoked.result,
        Some(register_response::Result::Error(_))
    ));
}

/// Path 2 has no package row, but is still publisher-authenticated. Exercise
/// the real renewal endpoint, legacy NULL auth modes, and publisher deletion.
// Reloading the signing key from storage uses `block_in_place`.
#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn test_unpublished_renewal_enforces_publisher_status() {
    let fx = path2_fixture("renew.example.com", "UnpublishedRenew", "wasm32-wasip1").await;
    let app = create_test_router(&fx._env).await;
    let mut registration = base_path2_request(&fx, TEST_REALM);
    sign_publisher_request(&mut registration, &fx.key, &fx.manifest_bytes);
    let initial = post_register(app.clone(), registration, Some("seed-realm-secret")).await;
    let initial = match initial.result.unwrap() {
        register_response::Result::Success(ok) => ok,
        other => panic!("initial registration failed: {other:?}"),
    };
    let pool = platform::storage::db::get_database().get_pool();
    let mode: i32 = sqlx::query_scalar(
        "SELECT auth_mode FROM issued_actor_identity WHERE realm = ? AND serial_number = ?",
    )
    .bind(TEST_REALM)
    .bind(initial.actr_id.serial_number as i64)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(mode, RegisterAuthMode::Package as i32);
    let renew = RenewCredentialRequest {
        actr_id: initial.actr_id.clone(),
        renewal_token: initial.renewal_token.unwrap(),
        attempt_id: control_attempt_id(),
    };
    let (status, _) = post_renew_raw(app.clone(), renew.clone()).await;
    assert_eq!(status, StatusCode::OK);

    for legacy in [false, true] {
        if legacy {
            sqlx::query("UPDATE issued_actor_identity SET auth_mode = NULL WHERE realm = ? AND serial_number = ?")
                .bind(TEST_REALM).bind(initial.actr_id.serial_number as i64)
                .execute(pool).await.unwrap();
        }
        for publisher_status in ["suspended", "revoked"] {
            sqlx::query("UPDATE publisher SET status = ? WHERE name = ?")
                .bind(publisher_status)
                .bind(&fx.publisher)
                .execute(pool)
                .await
                .unwrap();
            let (status, response) = post_renew_raw(app.clone(), renew.clone()).await;
            assert_eq!(
                status,
                StatusCode::FORBIDDEN,
                "legacy={legacy}, status={publisher_status}"
            );
            assert!(matches!(
                response.result,
                Some(renew_credential_response::Result::Error(_))
            ));
        }
    }
    sqlx::query("DELETE FROM publisher WHERE name = ?")
        .bind(&fx.publisher)
        .execute(pool)
        .await
        .unwrap();
    let (status, _) = post_renew_raw(app.clone(), renew).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Explicit Linked registration still renews without a publisher row.
    let linked = post_register(
        app.clone(),
        linked_register_request(),
        Some("seed-realm-secret"),
    )
    .await;
    let linked = match linked.result.unwrap() {
        register_response::Result::Success(ok) => ok,
        other => panic!("linked registration failed: {other:?}"),
    };
    let (status, _) = post_renew_raw(
        app,
        RenewCredentialRequest {
            actr_id: linked.actr_id,
            renewal_token: linked.renewal_token.unwrap(),
            attempt_id: control_attempt_id(),
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}
