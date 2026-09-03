//! Isolated regressions for AIS signing-key rotation.
//!
//! This is a separate integration-test binary so its temporary Signer and key
//! cache do not share the process-global validator/database state used by the
//! broader AIS HTTP integration suite.

use actr_protocol::{ActrId, ActrType, Realm};
use ais::{
    issuer::{AIdIssuer, IssuerConfig},
    signer_client_wrapper::create_signer_client,
};
use nonce_auth::storage::MemoryStorage;
use platform::{
    aid::{KeyCache, credential::AIdCredentialVerifier},
    config::signer::SignerClientConfig,
};
use signer::{GrpcClient, GrpcClientConfig, KeyStorage, SignerServiceConfig, create_grpc_service};
use std::{net::TcpListener, path::Path, time::Duration};
use tempfile::TempDir;
use tokio::{sync::oneshot, task::JoinHandle};
use tonic::transport::Server;

const TEST_STEP_TIMEOUT: Duration = Duration::from_secs(30);

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
            let mut handle = handle;
            if tokio::time::timeout(Duration::from_secs(5), &mut handle)
                .await
                .is_err()
            {
                handle.abort();
                let _ = handle.await;
            }
        }
    }
}

async fn start_embedded_signer(
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
    .expect("create Signer storage");

    let addr = match fixed_addr {
        Some(addr) => addr,
        None => {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
            let addr = listener.local_addr().expect("read ephemeral port");
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
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let handle = tokio::spawn(async move {
        Server::builder()
            .add_service(service)
            .serve_with_shutdown(addr, async move {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("embedded Signer server");
    });

    let endpoint = format!("http://{addr}");
    for _ in 0..40 {
        let config = GrpcClientConfig {
            endpoint: endpoint.clone(),
            actrix_shared_key: shared_key.to_string(),
            timeout_seconds: 2,
            enable_tls: false,
            tls_domain: None,
            ca_cert: None,
            client_cert: None,
            client_key: None,
        };
        if let Ok(mut client) = GrpcClient::new(&config).await {
            if matches!(client.health_check().await, Ok(status) if status == "healthy") {
                return (endpoint, handle, shutdown_tx);
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("embedded Signer did not become healthy in time");
}

async fn setup_test_environment(signer_config: SignerServiceConfig) -> TestEnv {
    let issuer_temp_dir = TempDir::new().expect("create issuer temp dir");
    let signer_temp_dir = TempDir::new().expect("create Signer temp dir");
    let shared_key = "test-shared-key".to_string();
    let (endpoint, signer_handle, signer_shutdown_tx) =
        start_embedded_signer(&shared_key, signer_temp_dir.path(), signer_config, None).await;
    TestEnv {
        issuer_temp_dir,
        _signer_temp_dir: signer_temp_dir,
        signer_handle: Some(signer_handle),
        signer_shutdown_tx: Some(signer_shutdown_tx),
        signer_config: SignerClientConfig {
            endpoint,
            timeout_seconds: 10,
            enable_tls: false,
            tls_domain: None,
            ca_cert: None,
            client_cert: None,
            client_key: None,
        },
        shared_key,
    }
}

fn issuer_config(temp_dir: &TempDir) -> IssuerConfig {
    IssuerConfig {
        token_ttl_secs: 3600,
        renewal_token_ttl_secs: 24 * 60 * 60,
        renewal_rotation_window_secs: 60 * 60,
        renewal_token_secret: vec![0; 32],
        signaling_heartbeat_interval_secs: 30,
        key_refresh_interval_secs: 3600,
        key_storage_file: temp_dir.path().join("issuer_keys.db"),
        enable_periodic_rotation: false,
        key_rotation_interval_secs: 86400,
        turn_secret: "test-turn-secret".to_string(),
        sqlite_path: temp_dir.path().to_path_buf(),
    }
}

async fn create_issuer(env: &TestEnv, cancel: tokio_util::sync::CancellationToken) -> AIdIssuer {
    let signer_client = create_signer_client(&env.signer_config, &env.shared_key)
        .await
        .expect("create Signer client");
    AIdIssuer::new(signer_client, issuer_config(&env.issuer_temp_dir), cancel)
        .await
        .expect("create issuer")
}

#[tokio::test]
async fn expired_key_rotates_before_credential_issuance() {
    let mut signer_config = SignerServiceConfig::default();
    signer_config.storage.key_ttl_seconds = 20;
    let mut env = setup_test_environment(signer_config).await;
    let cancel = tokio_util::sync::CancellationToken::new();
    cancel.cancel();
    eprintln!("key rotation test: creating issuer");
    let issuer = tokio::time::timeout(TEST_STEP_TIMEOUT, create_issuer(&env, cancel.clone()))
        .await
        .expect("issuer creation timed out");

    eprintln!("key rotation test: waiting for key expiry");
    tokio::time::sleep(Duration::from_secs(22)).await;
    eprintln!("key rotation test: reading expired key status");
    let status = tokio::time::timeout(TEST_STEP_TIMEOUT, issuer.get_current_key_status())
        .await
        .expect("key status read timed out")
        .expect("key status");
    let expired_key_id = status.key_id;
    assert!(!status.usable, "expired key must report as unusable");

    let actor_id = ActrId {
        realm: Realm { realm_id: 1001 },
        serial_number: 42,
        r#type: ActrType {
            manufacturer: "linked-src".to_string(),
            name: "source-workload".to_string(),
            version: "1.0.0".to_string(),
        },
    };
    eprintln!("key rotation test: issuing credential after expiry");
    let issued = tokio::time::timeout(
        TEST_STEP_TIMEOUT,
        issuer.issue_credential_for_actor(&actor_id),
    )
    .await
    .expect("credential issuance timed out")
    .expect("issue after expiry");
    assert_ne!(issued.credential.key_id, expired_key_id);

    eprintln!("key rotation test: verifying persisted key");
    let cache = tokio::time::timeout(
        TEST_STEP_TIMEOUT,
        KeyCache::new(env.issuer_temp_dir.path().join("signaling_key_cache.db")),
    )
    .await
    .expect("open signaling cache timed out")
    .expect("open signaling cache");
    let (verifying_key, _) = tokio::time::timeout(
        TEST_STEP_TIMEOUT,
        cache.get_cached_key(issued.credential.key_id),
    )
    .await
    .expect("read signaling cache timed out")
    .expect("read signaling cache")
    .expect("rotated key persisted to signaling cache");
    let claims = AIdCredentialVerifier::verify(&issued.credential, &verifying_key)
        .expect("fresh credential verifies");
    assert_eq!(claims.realm_id, 1001);

    eprintln!("key rotation test: shutting down signer");
    drop(issuer);
    env.shutdown_signer().await;
}

#[tokio::test]
async fn rotation_recovers_after_signer_restart() {
    let mut env = setup_test_environment(SignerServiceConfig::default()).await;
    let cancel = tokio_util::sync::CancellationToken::new();
    let issuer = create_issuer(&env, cancel.clone()).await;
    issuer.rotate_key().await.expect("rotate before restart");

    let endpoint = env.signer_config.endpoint.clone();
    env.shutdown_signer().await;
    assert!(
        issuer.rotate_key().await.is_err(),
        "rotation fails while down"
    );

    let addr = endpoint
        .trim_start_matches("http://")
        .parse()
        .expect("parse Signer endpoint");
    let signer_temp_dir = TempDir::new().expect("create restarted Signer temp dir");
    let (_, restart_handle, restart_shutdown_tx) = start_embedded_signer(
        &env.shared_key,
        signer_temp_dir.path(),
        SignerServiceConfig::default(),
        Some(addr),
    )
    .await;

    assert!(issuer.rotate_key().await.expect("recover rotation") > 0);

    cancel.cancel();
    let _ = restart_shutdown_tx.send(());
    let _ = restart_handle.await;
}
