use super::{
    DETACHED_READY_POLL_INTERVAL, DetachedRuntimeStartup, short_wid,
    wait_for_detached_runtime_ready,
};
use crate::commands::runtime_state::{RuntimeRecord, RuntimeStateStore};
use chrono::Utc;
use std::process::Command as StdCommand;
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn test_short_wid_handles_short_values() {
    assert_eq!(short_wid("shortwid"), "shortwid");
    assert_eq!(short_wid("1234567890123456"), "123456789012");
}

#[cfg(unix)]
#[tokio::test]
async fn test_wait_for_detached_runtime_ready_returns_ready_when_record_appears() {
    let hyper_dir = TempDir::new().unwrap();
    let store = RuntimeStateStore::new(hyper_dir.path().to_path_buf());
    store.ensure_layout().await.unwrap();

    let wid = "readywid-0000-0000-0000-000000000000".to_string();
    let log_path = hyper_dir.path().join("logs").join("actr-ready.log");
    let config_path = hyper_dir.path().join("actr.toml");
    let writer_store = RuntimeStateStore::new(hyper_dir.path().to_path_buf());
    let writer_wid = wid.clone();
    let writer_log_path = log_path.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let record = RuntimeRecord::new(
            writer_wid,
            "test-actr".to_string(),
            99999,
            config_path,
            writer_log_path,
            Utc::now(),
        );
        writer_store.write_record(&record).await.unwrap();
    });

    let mut child = StdCommand::new("sh")
        .arg("-c")
        .arg("sleep 5")
        .spawn()
        .unwrap();
    let result = wait_for_detached_runtime_ready(
        &store,
        &wid,
        &log_path,
        &mut child,
        Duration::from_secs(1),
        DETACHED_READY_POLL_INTERVAL,
    )
    .await
    .unwrap();

    assert_eq!(result, DetachedRuntimeStartup::Ready);

    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
#[tokio::test]
async fn test_wait_for_detached_runtime_ready_returns_error_when_child_exits() {
    let hyper_dir = TempDir::new().unwrap();
    let store = RuntimeStateStore::new(hyper_dir.path().to_path_buf());
    store.ensure_layout().await.unwrap();

    let log_path = hyper_dir.path().join("logs").join("actr-failed.log");
    let mut child = StdCommand::new("sh")
        .arg("-c")
        .arg("exit 3")
        .spawn()
        .unwrap();

    let error = wait_for_detached_runtime_ready(
        &store,
        "failedwid-0000",
        &log_path,
        &mut child,
        Duration::from_secs(1),
        DETACHED_READY_POLL_INTERVAL,
    )
    .await
    .unwrap_err()
    .to_string();

    assert!(error.contains("Detached child exited before runtime became ready"));
    assert!(error.contains(log_path.to_str().unwrap()));
}

#[cfg(unix)]
#[tokio::test]
async fn test_wait_for_detached_runtime_ready_returns_initializing_on_timeout() {
    let hyper_dir = TempDir::new().unwrap();
    let store = RuntimeStateStore::new(hyper_dir.path().to_path_buf());
    store.ensure_layout().await.unwrap();

    let log_path = hyper_dir.path().join("logs").join("actr-timeout.log");
    let mut child = StdCommand::new("sh")
        .arg("-c")
        .arg("sleep 5")
        .spawn()
        .unwrap();

    let result = wait_for_detached_runtime_ready(
        &store,
        "timeoutwid-0000",
        &log_path,
        &mut child,
        Duration::from_millis(50),
        Duration::from_millis(10),
    )
    .await
    .unwrap();

    assert_eq!(result, DetachedRuntimeStartup::Initializing);

    let _ = child.kill();
    let _ = child.wait();
}

/// The publisher re-signing provider must (a) mint a fresh random nonce on
/// every call — so hard rebind never replays the nonce Issuer consumed on the
/// first registration — and (b) re-read the private key from the keychain
/// file on every call, never caching it in memory.
#[test]
fn keychain_publisher_auth_provider_mints_fresh_proof_and_reloads_key() {
    use super::KeychainPublisherAuthProvider;
    use actr_hyper::PublisherAuthProvider;
    use actr_protocol::ActrType;
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as B64;
    use ed25519_dalek::{Signature, SigningKey, Verifier as _};
    use sha2::{Digest as _, Sha256};
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    let key_path = dir.path().join("keychain.json");
    let write_key = |path: &Path, seed: [u8; 32]| {
        let signing_key = SigningKey::from_bytes(&seed);
        let json = serde_json::json!({
            "private_key": B64.encode(seed),
            "public_key": B64.encode(signing_key.verifying_key().to_bytes()),
        });
        fs::write(path, json.to_string()).unwrap();
    };
    let original_seed = [0x11u8; 32];
    write_key(&key_path, original_seed);

    let provider = KeychainPublisherAuthProvider {
        key_path: key_path.clone(),
    };
    let actr_type = ActrType {
        publisher: "acme".into(),
        name: "svc".into(),
        version: "1.0.0".into(),
    };
    let manifest = b"manifest-bytes";

    let auth_a = provider
        .sign("test@example.com", &actr_type, "wasm32-wasip1", manifest)
        .unwrap();
    let auth_b = provider
        .sign("test@example.com", &actr_type, "wasm32-wasip1", manifest)
        .unwrap();

    // Fresh nonce each call — the property that lets hard rebind avoid
    // replaying the single-use nonce from the initial registration.
    assert_ne!(
        auth_a.nonce, auth_b.nonce,
        "nonce must differ across sign calls"
    );
    assert_ne!(
        auth_a.signature, auth_b.signature,
        "signature must differ across sign calls"
    );

    // The private key is NOT cached. A rotated key is observed immediately.
    // This proof can still be ignored by published Path 1, but cannot pass
    // unpublished Path 2 for the old manifest because that manifest pins
    // verification to its original signing_key_id.
    let rotated_key = SigningKey::from_bytes(&[0x22u8; 32]);
    write_key(&key_path, [0x22u8; 32]);
    let auth_c = provider
        .sign("test@example.com", &actr_type, "wasm32-wasip1", manifest)
        .unwrap();
    let manifest_sha256 = hex::encode(Sha256::digest(manifest));
    let payload =
        actr_protocol::build_publisher_register_payload(actr_protocol::PublisherRegisterPayload {
            realm: "test@example.com",
            actr_type: &actr_type,
            target: "wasm32-wasip1",
            manifest_sha256_hex: &manifest_sha256,
            publisher_auth_signed_at: auth_c.signed_at,
            publisher_auth_nonce: &auth_c.nonce,
        });
    let signature = Signature::from_slice(&auth_c.signature).unwrap();
    rotated_key
        .verifying_key()
        .verify(payload.as_bytes(), &signature)
        .expect("proof should be signed by the reloaded rotated key");

    // Corrupting the keychain also proves each call re-reads the file.
    fs::write(&key_path, "not-json").unwrap();
    let err = provider.sign("test@example.com", &actr_type, "wasm32-wasip1", manifest);
    assert!(
        err.is_err(),
        "sign must re-read the keychain and fail when it is corrupt"
    );
}

/// A keychain that declares a `publisher` binding may only register a
/// package for that same publisher. Cross-publisher registration signing
/// must be rejected before the proof is minted (issue #425). Unbound
/// keychains continue to register freely for backward compatibility.
#[test]
fn keychain_publisher_auth_provider_rejects_cross_publisher_binding() {
    use super::KeychainPublisherAuthProvider;
    use actr_hyper::PublisherAuthProvider;
    use actr_protocol::ActrType;
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as B64;
    use ed25519_dalek::SigningKey;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    let key_path = dir.path().join("keychain.json");
    let write_bound_key = |path: &Path, publisher: &str| {
        let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let json = serde_json::json!({
            "private_key": B64.encode(signing_key.to_bytes()),
            "public_key": B64.encode(signing_key.verifying_key().to_bytes()),
            "publisher": publisher,
        });
        fs::write(path, json.to_string()).unwrap();
    };
    let write_unbound_key = |path: &Path| {
        let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let json = serde_json::json!({
            "private_key": B64.encode(signing_key.to_bytes()),
            "public_key": B64.encode(signing_key.verifying_key().to_bytes()),
        });
        fs::write(path, json.to_string()).unwrap();
    };

    let provider = KeychainPublisherAuthProvider {
        key_path: key_path.clone(),
    };
    let acme = ActrType {
        publisher: "acme".into(),
        name: "svc".into(),
        version: "1.0.0".into(),
    };
    let rival = ActrType {
        publisher: "rival".into(),
        ..acme.clone()
    };
    let manifest = b"manifest-bytes";

    // Bound to "acme" → registering an "acme" package succeeds.
    write_bound_key(&key_path, "acme");
    assert!(
        provider
            .sign("test@example.com", &acme, "wasm32-wasip1", manifest)
            .is_ok()
    );

    // Bound to "acme" → registering a "rival" package is rejected.
    let err = provider
        .sign("test@example.com", &rival, "wasm32-wasip1", manifest)
        .unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("publisher mismatch"),
        "expected publisher mismatch, got: {msg}"
    );

    // Unbound keychain → registers any publisher (backward compatible).
    write_unbound_key(&key_path);
    assert!(
        provider
            .sign("test@example.com", &rival, "wasm32-wasip1", manifest)
            .is_ok(),
        "unbound keychain should remain usable"
    );
}
