/// Publisher Registry module integration tests
///
/// All tests requiring a database use an in-process SQLite in-memory pool, each test has its own independent pool.
use sqlx::SqlitePool;

use crate::{
    PublisherRegistryError, crypto,
    manager::{
        NonceRequest, PublishRequest, PublisherCertificate, PublisherRegistryManager,
        lookup_package,
    },
    model::{ActrPackage, PkgStatus, PublishNonce, Publisher, PublisherStatus},
};

#[test]
fn public_json_contract_uses_publisher_terminology() {
    let nonce = serde_json::to_value(NonceRequest {
        publisher: "apps.example.com".to_string(),
    })
    .unwrap();
    assert_eq!(nonce["publisher"], "apps.example.com");
    assert!(nonce.get("manufacturer").is_none());

    let certificate = serde_json::to_value(PublisherCertificate {
        key_id: "publisher-key".to_string(),
        publisher: "apps.example.com".to_string(),
        public_key: "base64-key".to_string(),
        issued_at: 1,
        expires_at: 2,
    })
    .unwrap();
    assert_eq!(certificate["publisher"], "apps.example.com");
    assert_eq!(certificate["public_key"], "base64-key");
    assert!(certificate.get("mfr_name").is_none());
    assert!(certificate.get("mfr_pubkey").is_none());
}

#[derive(serde::Serialize)]
struct SignablePublishBody<'a> {
    publisher: &'a str,
    name: &'a str,
    version: &'a str,
    target: &'a str,
    manifest: &'a str,
    signature: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    proto_files: Option<&'a serde_json::Value>,
    nonce: &'a str,
}

// ─── Test helpers ────────────────────────────────────────────────────────────

async fn setup_test_pool() -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .expect("failed to create in-memory sqlite pool");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS publisher (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            public_key TEXT NOT NULL DEFAULT '',
            key_id TEXT NOT NULL DEFAULT '',
            profile_json TEXT NOT NULL DEFAULT '{}',
            contact TEXT,
            status TEXT NOT NULL DEFAULT 'pending',
            created_at INTEGER NOT NULL,
            updated_at INTEGER,
            verified_at INTEGER,
            suspended_at INTEGER,
            revoked_at INTEGER,
            key_expires_at INTEGER
        )",
    )
    .execute(&pool)
    .await
    .expect("failed to create publisher table");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS publisher_package (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            publisher_id INTEGER NOT NULL REFERENCES publisher(id),
            publisher TEXT NOT NULL,
            name TEXT NOT NULL,
            version TEXT NOT NULL,
            type_str TEXT NOT NULL,
            target TEXT NOT NULL,
            key_id INTEGER NOT NULL DEFAULT 1,
            manifest TEXT NOT NULL,
            signature TEXT NOT NULL,
            signing_key_id TEXT,
            proto_files TEXT,
            status TEXT NOT NULL DEFAULT 'active',
            published_at INTEGER NOT NULL,
            revoked_at INTEGER,
            UNIQUE(publisher, name, version, target)
        )",
    )
    .execute(&pool)
    .await
    .expect("failed to create publisher_package table");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS publisher_key_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            publisher_id INTEGER NOT NULL REFERENCES publisher(id),
            key_id TEXT NOT NULL,
            public_key TEXT NOT NULL,
            fingerprint TEXT,
            status TEXT NOT NULL DEFAULT 'Active',
            created_at INTEGER NOT NULL,
            retired_at INTEGER NOT NULL DEFAULT 0,
            revoked_at INTEGER
        )",
    )
    .execute(&pool)
    .await
    .expect("failed to create publisher_key_history table");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS publisher_publish_nonce (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            publisher_id     INTEGER NOT NULL REFERENCES publisher(id),
            nonce      BLOB    NOT NULL UNIQUE,
            status     TEXT    NOT NULL DEFAULT 'pending',
            created_at INTEGER NOT NULL,
            expires_at INTEGER NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .expect("failed to create publisher_publish_nonce table");

    pool
}

fn valid_public_key() -> String {
    let (_, public_key) = crypto::generate_keypair();
    public_key
}

/// A pending publisher record, created without registration.
async fn pending(
    manager: &PublisherRegistryManager,
    name: &str,
) -> Result<(Publisher, ()), PublisherRegistryError> {
    let name = crate::reserved::canonical_publisher_domain(name)?;
    Ok((Publisher::create(manager.pool(), &name, None).await?, ()))
}

fn b64_key(key: &ed25519_dalek::SigningKey) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(key.verifying_key().to_bytes())
}

/// A Publisher Profile of `domain` listing `roots` (signed by the first)
/// and `packages` as package signing keys.
fn signed_profile(
    domain: &str,
    sequence: u64,
    roots: &[(&str, &ed25519_dalek::SigningKey, &str)],
    packages: &[(&ed25519_dalek::SigningKey, &str)],
) -> actr_protocol::PublisherProfile {
    let key = |id: String, key: &ed25519_dalek::SigningKey, status: &str| {
        serde_json::json!({
            "id": id,
            "algorithm": "Ed25519",
            "public_key": b64_key(key),
            "status": status,
        })
    };
    let now = chrono::Utc::now();
    let mut document = serde_json::json!({
        "schema": "actr.publisher/v1",
        "sequence": sequence,
        "domain": domain,
        "profile_signing_keys": roots
            .iter()
            .map(|(id, root, status)| key(id.to_string(), root, status))
            .collect::<Vec<_>>(),
        "keys": packages
            .iter()
            .map(|(package, status)| key(crypto::compute_key_id(package.verifying_key().as_bytes()), package, status))
            .collect::<Vec<_>>(),
        "security_contact": format!("mailto:security@{domain}"),
        "profile_issued_at": (now - chrono::Duration::hours(1)).to_rfc3339(),
        "profile_expires_at": (now + chrono::Duration::days(30)).to_rfc3339(),
    });
    actr_protocol::realm_profile::sign_document(&mut document, roots[0].0, roots[0].1);
    serde_json::from_value(document).unwrap()
}

/// Verify `profile` as the `register_publisher` path does and register it.
async fn register(
    manager: &PublisherRegistryManager,
    profile: &actr_protocol::PublisherProfile,
) -> Result<Publisher, PublisherRegistryError> {
    let keys = profile
        .verify(&profile.domain, chrono::Utc::now())
        .expect("valid Publisher Profile");
    manager
        .register_publisher(&profile.domain, profile, &keys)
        .await
}

/// Generate a valid nonce + nonce_sig for publish requests in tests.
///
/// Creates a real nonce in the DB, then signs the challenge payload with the given key.
/// Returns (nonce_b64, nonce_sig_b64) ready to use in PublishRequest.
#[allow(clippy::too_many_arguments)]
async fn make_publish_nonce(
    pool: &SqlitePool,
    publisher_id: i64,
    publisher: &str,
    name: &str,
    version: &str,
    target: &str,
    manifest: &str,
    signature: &str,
    proto_files: Option<&serde_json::Value>,
    signing_key: &ed25519_dalek::SigningKey,
) -> (String, String) {
    use base64::Engine as _;
    use ed25519_dalek::Signer;
    use sha2::{Digest, Sha256};

    let nonce_bytes = PublishNonce::create(pool, publisher_id)
        .await
        .expect("failed to create test nonce");
    let nonce_b64 = base64::engine::general_purpose::STANDARD.encode(&nonce_bytes);

    let signable_body = SignablePublishBody {
        publisher,
        name,
        version,
        target,
        manifest,
        signature,
        proto_files,
        nonce: &nonce_b64,
    };
    let signable_body_bytes =
        serde_json::to_vec(&signable_body).expect("failed to serialize signable publish body");
    let body_hash = hex::encode(Sha256::digest(&signable_body_bytes));
    let nonce_hex = hex::encode(&nonce_bytes);
    let payload = format!(
        "ACTR-PUBLISH-V2\npublisher={}\nmethod=POST\npath=/registry/packages/publish\nnonce={}\nbody_sha256={}",
        publisher, nonce_hex, body_hash
    );
    let sig = signing_key.sign(payload.as_bytes());
    let nonce_sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());

    (nonce_b64, nonce_sig_b64)
}

fn test_manifest(publisher: &str, name: &str, version: &str, target: &str) -> String {
    format!(
        "publisher = \"{publisher}\"\nname = \"{name}\"\nversion = \"{version}\"\n\n[binary]\npath = \"bin/actor.wasm\"\ntarget = \"{target}\"\nhash = \"sha256:abc123\"\n"
    )
}

// ─── crypto.rs pure unit tests ───────────────────────────────────────────────

#[test]
fn test_generate_keypair_roundtrip() {
    use base64::Engine as _;

    let (private_b64, public_b64) = crypto::generate_keypair();
    assert!(!private_b64.is_empty());
    assert!(!public_b64.is_empty());

    let priv_bytes = base64::engine::general_purpose::STANDARD
        .decode(&private_b64)
        .expect("private key should be valid base64");
    assert_eq!(
        priv_bytes.len(),
        32,
        "Ed25519 private key should be 32 bytes"
    );

    let pub_bytes = base64::engine::general_purpose::STANDARD
        .decode(&public_b64)
        .expect("public key should be valid base64");
    assert_eq!(pub_bytes.len(), 32, "Ed25519 public key should be 32 bytes");
}

#[test]
fn test_generate_keypair_unique() {
    let (priv1, pub1) = crypto::generate_keypair();
    let (priv2, pub2) = crypto::generate_keypair();
    assert_ne!(priv1, priv2);
    assert_ne!(pub1, pub2);
}

#[test]
fn test_verify_signature_valid() {
    use base64::Engine as _;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    let message = b"hello publisher";
    let sig = signing_key.sign(message);

    let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());
    let pub_b64 = base64::engine::general_purpose::STANDARD.encode(verifying_key.to_bytes());

    let result = crypto::verify_signature(message, &sig_b64, &pub_b64)
        .expect("verify_signature should not error on valid inputs");
    assert!(result, "valid signature should verify as true");
}

#[test]
fn test_verify_signature_invalid_zeros() {
    use base64::Engine as _;
    let (_, pub_b64) = crypto::generate_keypair();
    let bad_sig = base64::engine::general_purpose::STANDARD.encode([0u8; 64]);
    let result = crypto::verify_signature(b"message", &bad_sig, &pub_b64);
    assert!(
        matches!(result, Ok(false) | Err(PublisherRegistryError::Crypto(_))),
        "all-zero signature should fail: {result:?}"
    );
}

#[test]
fn test_verify_signature_wrong_key() {
    use base64::Engine as _;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    let key1 = SigningKey::generate(&mut OsRng);
    let key2 = SigningKey::generate(&mut OsRng);
    let message = b"test message";
    let sig = key1.sign(message);

    let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());
    let wrong_pub_b64 =
        base64::engine::general_purpose::STANDARD.encode(key2.verifying_key().to_bytes());

    let result = crypto::verify_signature(message, &sig_b64, &wrong_pub_b64)
        .expect("should not return error for valid encoding");
    assert!(!result, "signature with wrong key should verify as false");
}

#[test]
fn test_verify_signature_bad_pubkey_encoding() {
    let bad_pub = "not-valid-base64!!!";
    let result = crypto::verify_signature(b"msg", "anysig", bad_pub);
    assert!(
        matches!(result, Err(PublisherRegistryError::Crypto(_))),
        "bad base64 pubkey should return Crypto error"
    );
}

// ─── model/publisher.rs tests (require DB) ────────────────────────────────

#[tokio::test]
async fn test_publisher_create_and_get() {
    let pool = setup_test_pool().await;
    let record = Publisher::create(&pool, "testco", Some("admin@testco.com"))
        .await
        .expect("create should succeed");

    assert_eq!(record.name, "testco");
    assert_eq!(record.status, PublisherStatus::Pending);
    assert!(record.verified_at.is_none());
    assert_eq!(record.contact.as_deref(), Some("admin@testco.com"));

    let found = Publisher::get(&pool, record.id)
        .await
        .expect("get should succeed")
        .expect("should find created publisher");
    assert_eq!(found.name, "testco");
    assert_eq!(found.id, record.id);
}

#[tokio::test]
async fn test_publisher_get_nonexistent() {
    let pool = setup_test_pool().await;
    let found = Publisher::get(&pool, 9999).await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn test_publisher_get_by_name() {
    let pool = setup_test_pool().await;
    let record = Publisher::create(&pool, "namedco", None).await.unwrap();

    let found = Publisher::get_by_name(&pool, "namedco")
        .await
        .unwrap()
        .expect("should find by name");
    assert_eq!(found.id, record.id);

    let missing = Publisher::get_by_name(&pool, "nobody").await.unwrap();
    assert!(missing.is_none());
}

#[tokio::test]
async fn test_publisher_duplicate_name() {
    let pool = setup_test_pool().await;
    Publisher::create(&pool, "dupco", None).await.unwrap();
    let result = Publisher::create(&pool, "dupco", None).await;
    assert!(
        matches!(result, Err(PublisherRegistryError::AlreadyExists(_))),
        "duplicate name should return AlreadyExists"
    );
}

#[tokio::test]
async fn test_publisher_activate() {
    let pool = setup_test_pool().await;
    let mut record = Publisher::create(&pool, "activeco", None).await.unwrap();
    let public_key = valid_public_key();

    record
        .activate(&pool, public_key.clone())
        .await
        .expect("activate from pending should succeed");

    assert_eq!(record.status, PublisherStatus::Active);
    assert_eq!(record.public_key, public_key);
    assert!(record.verified_at.is_some());
    assert!(record.updated_at.is_some());

    let from_db = Publisher::get(&pool, record.id).await.unwrap().unwrap();
    assert_eq!(from_db.status, PublisherStatus::Active);
    assert_eq!(from_db.public_key, public_key);
}

#[tokio::test]
async fn test_publisher_lifecycle_full() {
    let pool = setup_test_pool().await;
    let mut record = Publisher::create(&pool, "lifecycle", None).await.unwrap();

    record.activate(&pool, valid_public_key()).await.unwrap();
    assert_eq!(record.status, PublisherStatus::Active);

    record.suspend(&pool).await.unwrap();
    assert_eq!(record.status, PublisherStatus::Suspended);
    assert!(record.suspended_at.is_some());

    record.reinstate(&pool).await.unwrap();
    assert_eq!(record.status, PublisherStatus::Active);

    record.revoke(&pool).await.unwrap();
    assert_eq!(record.status, PublisherStatus::Revoked);
    assert!(record.revoked_at.is_some());
}

#[tokio::test]
async fn test_publisher_invalid_transitions_from_pending() {
    let pool = setup_test_pool().await;
    let mut record = Publisher::create(&pool, "transco", None).await.unwrap();

    let err = record.suspend(&pool).await.unwrap_err();
    assert!(matches!(err, PublisherRegistryError::InvalidStatus(_)));

    let err = record.reinstate(&pool).await.unwrap_err();
    assert!(matches!(err, PublisherRegistryError::InvalidStatus(_)));
}

#[tokio::test]
async fn test_publisher_cannot_activate_twice() {
    let pool = setup_test_pool().await;
    let mut record = Publisher::create(&pool, "twiceco", None).await.unwrap();

    record.activate(&pool, "key1".to_string()).await.unwrap();
    let err = record
        .activate(&pool, "key2".to_string())
        .await
        .unwrap_err();
    assert!(matches!(err, PublisherRegistryError::InvalidStatus(_)));
}

#[tokio::test]
async fn test_publisher_list_all() {
    let pool = setup_test_pool().await;
    Publisher::create(&pool, "list1", None).await.unwrap();
    Publisher::create(&pool, "list2", None).await.unwrap();

    let all = Publisher::list(&pool, None).await.unwrap();
    assert_eq!(all.len(), 2);
}

#[tokio::test]
async fn test_publisher_list_by_status() {
    let pool = setup_test_pool().await;
    Publisher::create(&pool, "statuslist1", None).await.unwrap();
    let mut publisher2 = Publisher::create(&pool, "statuslist2", None).await.unwrap();
    publisher2
        .activate(&pool, valid_public_key())
        .await
        .unwrap();

    let active = Publisher::list(&pool, Some(PublisherStatus::Active))
        .await
        .unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].name, "statuslist2");

    let pending = Publisher::list(&pool, Some(PublisherStatus::Pending))
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].name, "statuslist1");
}

#[tokio::test]
async fn test_publisher_delete() {
    let pool = setup_test_pool().await;
    let record = Publisher::create(&pool, "delco", None).await.unwrap();
    let id = record.id;

    Publisher::delete(&pool, id).await.unwrap();

    let found = Publisher::get(&pool, id).await.unwrap();
    assert!(found.is_none());
}

// ─── model/package.rs tests (require DB) ─────────────────────────────────────

#[tokio::test]
async fn test_package_publish_and_get() {
    use base64::Engine as _;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    let pool = setup_test_pool().await;
    let mut record = Publisher::create(&pool, "pkgco", None).await.unwrap();
    let key = SigningKey::generate(&mut OsRng);
    let pub_b64 = base64::engine::general_purpose::STANDARD.encode(key.verifying_key().to_bytes());
    record.activate(&pool, pub_b64).await.unwrap();

    let pkg = ActrPackage::publish(
        &pool,
        record.id,
        "pkgco",
        "client",
        "1.0.0",
        "wasm32-wasip1",
        "manifest content",
        "sig123",
        &record.key_id,
        None,
    )
    .await
    .unwrap();

    assert_eq!(pkg.type_str, "pkgco/client@1.0.0");
    assert_eq!(pkg.status, PkgStatus::Active);
    assert_eq!(pkg.publisher, "pkgco");
    assert_eq!(pkg.name, "client");
    assert_eq!(pkg.version, "1.0.0");

    let found = ActrPackage::get_by_type(&pool, "pkgco/client@1.0.0")
        .await
        .unwrap()
        .expect("should find published package");
    assert_eq!(found.id, pkg.id);
}

#[tokio::test]
async fn test_package_get_by_type_not_found() {
    let pool = setup_test_pool().await;
    let found = ActrPackage::get_by_type(&pool, "nobody/nothing@v0")
        .await
        .unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn test_package_duplicate_rejected() {
    let pool = setup_test_pool().await;
    let mut record = Publisher::create(&pool, "dupkg", None).await.unwrap();
    record.activate(&pool, valid_public_key()).await.unwrap();

    ActrPackage::publish(
        &pool,
        record.id,
        "dupkg",
        "svc",
        "1.0.0",
        "wasm32-wasip1",
        "m",
        "s",
        &record.key_id,
        None,
    )
    .await
    .unwrap();
    let result = ActrPackage::publish(
        &pool,
        record.id,
        "dupkg",
        "svc",
        "1.0.0",
        "wasm32-wasip1",
        "m2",
        "s2",
        &record.key_id,
        None,
    )
    .await;
    assert!(
        matches!(result, Err(PublisherRegistryError::PackageAlreadyPublished)),
        "duplicate publish should return PackageAlreadyPublished"
    );
}

#[tokio::test]
async fn test_package_revoke() {
    let pool = setup_test_pool().await;
    let mut record = Publisher::create(&pool, "revpkg", None).await.unwrap();
    record.activate(&pool, valid_public_key()).await.unwrap();

    let mut pkg = ActrPackage::publish(
        &pool,
        record.id,
        "revpkg",
        "svc",
        "1.0.0",
        "wasm32-wasip1",
        "m",
        "s",
        &record.key_id,
        None,
    )
    .await
    .unwrap();
    pkg.revoke(&pool).await.unwrap();

    assert_eq!(pkg.status, PkgStatus::Revoked);
    assert!(pkg.revoked_at.is_some());

    let found = ActrPackage::get_by_type(&pool, "revpkg/svc@1.0.0")
        .await
        .unwrap();
    assert!(
        found.is_none(),
        "revoked package should not be found by get_by_type"
    );

    let found_any_status =
        ActrPackage::get_by_type_and_target_any_status(&pool, "revpkg/svc@1.0.0", "wasm32-wasip1")
            .await
            .unwrap()
            .expect("revoked package must remain visible to lifecycle-aware callers");
    assert_eq!(found_any_status.status, PkgStatus::Revoked);
}

#[tokio::test]
async fn test_package_list_by_publisher() {
    let pool = setup_test_pool().await;
    let mut record = Publisher::create(&pool, "listpkg", None).await.unwrap();
    record.activate(&pool, valid_public_key()).await.unwrap();

    ActrPackage::publish(
        &pool,
        record.id,
        "listpkg",
        "alpha",
        "1.0.0",
        "wasm32-wasip1",
        "m",
        "s",
        &record.key_id,
        None,
    )
    .await
    .unwrap();
    ActrPackage::publish(
        &pool,
        record.id,
        "listpkg",
        "beta",
        "1.0.0",
        "wasm32-wasip1",
        "m",
        "s",
        &record.key_id,
        None,
    )
    .await
    .unwrap();

    let pkgs = ActrPackage::list_by_publisher(&pool, record.id)
        .await
        .unwrap();
    assert_eq!(pkgs.len(), 2);
}

#[tokio::test]
async fn test_package_get_by_id() {
    let pool = setup_test_pool().await;
    let mut record = Publisher::create(&pool, "idpkg", None).await.unwrap();
    record.activate(&pool, valid_public_key()).await.unwrap();

    let pkg = ActrPackage::publish(
        &pool,
        record.id,
        "idpkg",
        "svc",
        "1.0.0",
        "wasm32-wasip1",
        "m",
        "s",
        &record.key_id,
        None,
    )
    .await
    .unwrap();

    let found = ActrPackage::get_by_id(&pool, pkg.id).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().type_str, "idpkg/svc@1.0.0");
}

#[tokio::test]
async fn test_verify_legacy_package_without_signing_key_id_after_expiry() {
    use base64::Engine as _;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    let pool = setup_test_pool().await;
    let key = SigningKey::generate(&mut OsRng);
    let public_key =
        base64::engine::general_purpose::STANDARD.encode(key.verifying_key().to_bytes());
    let mut record = Publisher::create(&pool, "legacypkg", None).await.unwrap();
    record.activate(&pool, public_key).await.unwrap();

    let manifest = test_manifest("legacypkg", "svc", "1.0.0", "wasm32-wasip1");
    let signature =
        base64::engine::general_purpose::STANDARD.encode(key.sign(manifest.as_bytes()).to_bytes());
    let package = ActrPackage::publish(
        &pool,
        record.id,
        "legacypkg",
        "svc",
        "1.0.0",
        "wasm32-wasip1",
        &manifest,
        &signature,
        &record.key_id,
        None,
    )
    .await
    .unwrap();
    sqlx::query("UPDATE publisher_package SET signing_key_id = NULL WHERE id = ?")
        .bind(package.id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE publisher SET key_expires_at = ? WHERE id = ?")
        .bind(chrono::Utc::now().timestamp() - 1)
        .bind(record.id)
        .execute(&pool)
        .await
        .unwrap();

    let legacy_package = ActrPackage::get_by_id(&pool, package.id)
        .await
        .unwrap()
        .unwrap();
    let resolved_key_id = PublisherRegistryManager::new(pool)
        .verify_published_package_signing_key(&legacy_package)
        .await
        .expect("legacy package signature should remain verifiable");
    assert_eq!(resolved_key_id, record.key_id);
}

// ─── manager.rs tests (require DB) ────────────────────────────────────────────

#[tokio::test]
async fn test_lookup_package_no_reserved_shortcut() {
    let pool = setup_test_pool().await;
    // No reserved names anymore; unregistered names return false
    assert!(
        !lookup_package(&pool, "self/anything@1.0.0", None, None)
            .await
            .unwrap()
    );
    assert!(
        !lookup_package(&pool, "acme/client@1.0.0", None, None)
            .await
            .unwrap()
    );
    assert!(
        !lookup_package(&pool, "actrix/core@1.0.0", None, None)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn test_lookup_package_not_registered() {
    let pool = setup_test_pool().await;
    assert!(
        !lookup_package(&pool, "unknown/svc@1.0.0", None, None)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn test_lookup_package_active() {
    let pool = setup_test_pool().await;
    let mut record = Publisher::create(&pool, "lookco", None).await.unwrap();
    record.activate(&pool, valid_public_key()).await.unwrap();
    ActrPackage::publish(
        &pool,
        record.id,
        "lookco",
        "svc",
        "1.0.0",
        "wasm32-wasip1",
        "m",
        "s",
        &record.key_id,
        None,
    )
    .await
    .unwrap();

    assert!(
        lookup_package(&pool, "lookco/svc@1.0.0", None, None)
            .await
            .unwrap()
    );
    assert!(
        !lookup_package(&pool, "lookco/svc@v2", None, None)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn test_lookup_package_revoked() {
    let pool = setup_test_pool().await;
    let mut record = Publisher::create(&pool, "revokedlook", None).await.unwrap();
    record.activate(&pool, valid_public_key()).await.unwrap();
    let mut pkg = ActrPackage::publish(
        &pool,
        record.id,
        "revokedlook",
        "svc",
        "1.0.0",
        "wasm32-wasip1",
        "m",
        "s",
        &record.key_id,
        None,
    )
    .await
    .unwrap();
    pkg.revoke(&pool).await.unwrap();

    assert!(
        !lookup_package(&pool, "revokedlook/svc@1.0.0", None, None)
            .await
            .unwrap(),
        "revoked package should not be found"
    );
}

#[tokio::test]
async fn test_formerly_reserved_publisher_names_accepted() {
    let pool = setup_test_pool().await;
    let manager = PublisherRegistryManager::new(pool);
    // Previously reserved names are now accepted
    let result = pending(&manager, "acme.example").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_publisher_name_must_be_a_domain() {
    let pool = setup_test_pool().await;
    let manager = PublisherRegistryManager::new(pool);
    // Underscore is not allowed in a DNS name
    let result = pending(&manager, "my_company").await;
    assert!(matches!(
        result,
        Err(PublisherRegistryError::InvalidName(_))
    ));
}

#[tokio::test]
async fn test_manager_get_status() {
    let pool = setup_test_pool().await;
    let manager = PublisherRegistryManager::new(pool);
    let (record, _) = pending(&manager, "statusco.example").await.unwrap();

    let status = manager.get_status(record.id).await.unwrap();
    assert_eq!(status.name, "statusco.example");
    assert_eq!(status.status, PublisherStatus::Pending);
}

#[tokio::test]
async fn test_manager_get_status_not_found() {
    let pool = setup_test_pool().await;
    let manager = PublisherRegistryManager::new(pool);
    let result = manager.get_status(9999).await;
    assert!(matches!(result, Err(PublisherRegistryError::NotFound)));
}

#[tokio::test]
async fn test_manager_admin_approve() {
    let pool = setup_test_pool().await;
    let manager = PublisherRegistryManager::new(pool);
    let (record, _) = pending(&manager, "approveco.example").await.unwrap();

    let response = manager.admin_approve(record.id, None).await.unwrap();
    assert_eq!(response.certificate.publisher, "approveco.example");
    assert!(!response.certificate.public_key.is_empty());
    assert!(response.certificate.expires_at > response.certificate.issued_at);
}

#[tokio::test]
async fn test_manager_admin_suspend_reinstate() {
    let pool = setup_test_pool().await;
    let manager = PublisherRegistryManager::new(pool);
    let (record, _) = pending(&manager, "suspco.example").await.unwrap();
    manager.admin_approve(record.id, None).await.unwrap();

    manager.admin_suspend(record.id).await.unwrap();
    let status = manager.get_status(record.id).await.unwrap();
    assert_eq!(status.status, PublisherStatus::Suspended);

    manager.admin_reinstate(record.id).await.unwrap();
    let status = manager.get_status(record.id).await.unwrap();
    assert_eq!(status.status, PublisherStatus::Active);
}

#[tokio::test]
async fn test_manager_admin_delete() {
    let pool = setup_test_pool().await;
    let manager = PublisherRegistryManager::new(pool);
    let (record, _) = pending(&manager, "deleteco.example").await.unwrap();
    let id = record.id;

    manager.admin_delete(id).await.unwrap();
    let result = manager.get_status(id).await;
    assert!(matches!(result, Err(PublisherRegistryError::NotFound)));
}

#[tokio::test]
async fn test_manager_publish_invalid_signature() {
    use base64::Engine as _;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    let pool = setup_test_pool().await;
    let key = SigningKey::generate(&mut OsRng);
    let pub_b64 = base64::engine::general_purpose::STANDARD.encode(key.verifying_key().to_bytes());

    let manager = PublisherRegistryManager::new(pool.clone());
    let (record, _) = pending(&manager, "sigco.example").await.unwrap();
    manager
        .admin_approve(record.id, Some(&pub_b64))
        .await
        .unwrap();

    let manifest = test_manifest("sigco.example", "svc", "1.0.0", "wasm32-wasip1");
    let bad_sig = base64::engine::general_purpose::STANDARD.encode([0u8; 64]);

    // Valid nonce but bad manifest signature
    let (nonce_b64, nonce_sig_b64) = make_publish_nonce(
        &pool,
        record.id,
        "sigco.example",
        "svc",
        "1.0.0",
        "wasm32-wasip1",
        &manifest,
        &bad_sig,
        None,
        &key,
    )
    .await;

    let result = manager
        .publish_package(PublishRequest {
            publisher: "sigco.example".to_string(),
            name: "svc".to_string(),
            version: "1.0.0".to_string(),
            target: "wasm32-wasip1".to_string(),
            manifest,
            signature: bad_sig,
            proto_files: None,
            nonce: Some(nonce_b64),
            nonce_sig: Some(nonce_sig_b64),
        })
        .await;
    assert!(
        matches!(
            result,
            Err(PublisherRegistryError::InvalidSignature) | Err(PublisherRegistryError::Crypto(_))
        ),
        "invalid signature should be rejected: {result:?}"
    );
}

#[tokio::test]
async fn test_manager_publish_valid_signature() {
    use base64::Engine as _;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    let pool = setup_test_pool().await;

    let signing_key = SigningKey::generate(&mut OsRng);
    let pub_b64 =
        base64::engine::general_purpose::STANDARD.encode(signing_key.verifying_key().to_bytes());

    let mut record = Publisher::create(&pool, "validpub", None).await.unwrap();
    record.activate(&pool, pub_b64).await.unwrap();

    let manifest = test_manifest("validpub", "client", "1.0.0", "wasm32-wasip1");
    let sig = signing_key.sign(manifest.as_bytes());
    let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());

    let (nonce_b64, nonce_sig_b64) = make_publish_nonce(
        &pool,
        record.id,
        "validpub",
        "client",
        "1.0.0",
        "wasm32-wasip1",
        &manifest,
        &sig_b64,
        None,
        &signing_key,
    )
    .await;

    let manager = PublisherRegistryManager::new(pool);
    let pkg = manager
        .publish_package(PublishRequest {
            publisher: "validpub".to_string(),
            name: "client".to_string(),
            version: "1.0.0".to_string(),
            target: "wasm32-wasip1".to_string(),
            manifest,
            signature: sig_b64,
            proto_files: None,
            nonce: Some(nonce_b64),
            nonce_sig: Some(nonce_sig_b64),
        })
        .await
        .unwrap();

    assert_eq!(pkg.type_str, "validpub/client@1.0.0");
    assert_eq!(pkg.status, PkgStatus::Active);
}

#[tokio::test]
async fn test_manager_publish_inactive_publisher() {
    let pool = setup_test_pool().await;
    Publisher::create(&pool, "pendingpublisher", None)
        .await
        .unwrap();

    let manager = PublisherRegistryManager::new(pool);
    let result = manager
        .publish_package(PublishRequest {
            publisher: "pendingpublisher".to_string(),
            name: "svc".to_string(),
            version: "1.0.0".to_string(),
            target: "wasm32-wasip1".to_string(),
            manifest: "m".to_string(),
            signature: "s".to_string(),
            proto_files: None,
            nonce: None,
            nonce_sig: None,
        })
        .await;
    assert!(
        matches!(result, Err(PublisherRegistryError::InvalidStatus(_))),
        "publishing for pending Publisher Registry should fail with InvalidStatus"
    );
}

#[tokio::test]
async fn test_manager_resolve_by_name() {
    let pool = setup_test_pool().await;
    let manager = PublisherRegistryManager::new(pool);
    let (record, _) = pending(&manager, "resolveco.example").await.unwrap();
    manager.admin_approve(record.id, None).await.unwrap();

    let info = manager.resolve_by_name("resolveco.example").await.unwrap();
    assert_eq!(info.name, "resolveco.example");
    assert!(!info.public_key.is_empty());
}

#[tokio::test]
async fn test_manager_resolve_by_name_not_active() {
    let pool = setup_test_pool().await;
    let manager = PublisherRegistryManager::new(pool);
    pending(&manager, "pendingres.example").await.unwrap();

    let result = manager.resolve_by_name("pendingres.example").await;
    assert!(matches!(
        result,
        Err(PublisherRegistryError::InvalidStatus(_))
    ));
}

#[tokio::test]
async fn test_manager_get_and_revoke_package() {
    use base64::Engine as _;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    let pool = setup_test_pool().await;
    let key = SigningKey::generate(&mut OsRng);
    let pub_b64 = base64::engine::general_purpose::STANDARD.encode(key.verifying_key().to_bytes());

    let mut record = Publisher::create(&pool, "revmgr", None).await.unwrap();
    record.activate(&pool, pub_b64).await.unwrap();

    let manifest = test_manifest("revmgr", "svc", "1.0.0", "wasm32-wasip1");
    let sig = key.sign(manifest.as_bytes());
    let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());

    let (nonce_b64, nonce_sig_b64) = make_publish_nonce(
        &pool,
        record.id,
        "revmgr",
        "svc",
        "1.0.0",
        "wasm32-wasip1",
        &manifest,
        &sig_b64,
        None,
        &key,
    )
    .await;

    let manager = PublisherRegistryManager::new(pool);
    let pkg = manager
        .publish_package(PublishRequest {
            publisher: "revmgr".to_string(),
            name: "svc".to_string(),
            version: "1.0.0".to_string(),
            target: "wasm32-wasip1".to_string(),
            manifest,
            signature: sig_b64,
            proto_files: None,
            nonce: Some(nonce_b64),
            nonce_sig: Some(nonce_sig_b64),
        })
        .await
        .unwrap();

    let found = manager.get_package("revmgr/svc@1.0.0").await.unwrap();
    assert_eq!(found.id, pkg.id);

    manager.revoke_package(pkg.id).await.unwrap();

    let result = manager.get_package("revmgr/svc@1.0.0").await;
    assert!(matches!(result, Err(PublisherRegistryError::NotFound)));
}

#[tokio::test]
async fn test_manager_admin_list() {
    let pool = setup_test_pool().await;
    let manager = PublisherRegistryManager::new(pool);

    pending(&manager, "adminlist1.example").await.unwrap();
    let (publisher2, _) = pending(&manager, "adminlist2.example").await.unwrap();
    manager.admin_approve(publisher2.id, None).await.unwrap();

    let all = manager.admin_list(None).await.unwrap();
    assert_eq!(all.len(), 2);

    let active = manager
        .admin_list(Some(PublisherStatus::Active))
        .await
        .unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].name, "adminlist2.example");
}

#[tokio::test]
async fn test_manager_list_packages_by_publisher() {
    use base64::Engine as _;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    let pool = setup_test_pool().await;
    let key = SigningKey::generate(&mut OsRng);
    let pub_b64 = base64::engine::general_purpose::STANDARD.encode(key.verifying_key().to_bytes());

    let mut record = Publisher::create(&pool, "listmgr", None).await.unwrap();
    record.activate(&pool, pub_b64).await.unwrap();

    let manager = PublisherRegistryManager::new(pool.clone());

    for pkg_name in &["alpha", "beta"] {
        let manifest = test_manifest("listmgr", pkg_name, "1.0.0", "wasm32-wasip1");
        let sig = key.sign(manifest.as_bytes());
        let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());
        let (nonce_b64, nonce_sig_b64) = make_publish_nonce(
            &pool,
            record.id,
            "listmgr",
            pkg_name,
            "1.0.0",
            "wasm32-wasip1",
            &manifest,
            &sig_b64,
            None,
            &key,
        )
        .await;
        manager
            .publish_package(PublishRequest {
                publisher: "listmgr".to_string(),
                name: pkg_name.to_string(),
                version: "1.0.0".to_string(),
                target: "wasm32-wasip1".to_string(),
                manifest,
                signature: sig_b64,
                proto_files: None,
                nonce: Some(nonce_b64),
                nonce_sig: Some(nonce_sig_b64),
            })
            .await
            .unwrap();
    }

    let pkgs = manager.list_packages(Some("listmgr")).await.unwrap();
    assert_eq!(pkgs.len(), 2);

    let all = manager.list_packages(None).await.unwrap();
    assert_eq!(all.len(), 2);
}

// ─── dual key mode tests ──────────────────────────────────────────────────────

#[test]
fn test_validate_public_key_valid() {
    let (_, pub_b64) = crypto::generate_keypair();
    assert!(crypto::validate_public_key(&pub_b64).is_ok());
}

#[test]
fn test_validate_public_key_bad_base64() {
    let result = crypto::validate_public_key("not-valid-base64!!!");
    assert!(
        matches!(result, Err(PublisherRegistryError::Crypto(_))),
        "bad base64 should return Crypto error"
    );
}

#[test]
fn test_validate_public_key_wrong_length() {
    use base64::Engine as _;
    let short = base64::engine::general_purpose::STANDARD.encode([0u8; 16]);
    let result = crypto::validate_public_key(&short);
    assert!(
        matches!(result, Err(PublisherRegistryError::Crypto(_))),
        "16-byte key should be rejected"
    );
}

#[test]
fn test_validate_public_key_all_zeros() {
    use base64::Engine as _;
    let zeros = base64::engine::general_purpose::STANDARD.encode([0u8; 32]);
    // All-zero bytes may or may not be a valid curve point depending on the library.
    // We just verify it does not panic — either Ok or Err(Crypto) is acceptable.
    let result = crypto::validate_public_key(&zeros);
    assert!(
        result.is_ok() || matches!(result, Err(PublisherRegistryError::Crypto(_))),
        "all-zero key should return Ok or Crypto error, not panic"
    );
}

#[tokio::test]
async fn test_manager_admin_approve_with_uploaded_key() {
    use base64::Engine as _;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    let pool = setup_test_pool().await;
    let manager = PublisherRegistryManager::new(pool);
    let (record, _) = pending(&manager, "uploadco.example").await.unwrap();

    // User generates their own keypair
    let user_key = SigningKey::generate(&mut OsRng);
    let user_pub_b64 =
        base64::engine::general_purpose::STANDARD.encode(user_key.verifying_key().to_bytes());

    let response = manager
        .admin_approve(record.id, Some(&user_pub_b64))
        .await
        .unwrap();

    assert_eq!(response.certificate.public_key, user_pub_b64);
    assert_eq!(response.certificate.publisher, "uploadco.example");
}

#[tokio::test]
async fn test_manager_admin_approve_with_invalid_key_rejected() {
    let pool = setup_test_pool().await;
    let manager = PublisherRegistryManager::new(pool);
    let (record, _) = pending(&manager, "badkeyco.example").await.unwrap();

    let result = manager
        .admin_approve(record.id, Some("not-a-valid-key"))
        .await;
    assert!(
        matches!(result, Err(PublisherRegistryError::Crypto(_))),
        "invalid public key should be rejected with Crypto error: {result:?}"
    );

    // Verify Publisher Registry is still pending (not corrupted)
    let status = manager.get_status(record.id).await.unwrap();
    assert_eq!(
        status.status,
        PublisherStatus::Pending,
        "Publisher Registry should remain pending after failed activation"
    );
}

#[tokio::test]
async fn test_manager_admin_approve_test_helper_generates_public_key() {
    let pool = setup_test_pool().await;
    let manager = PublisherRegistryManager::new(pool);
    let (record, _) = pending(&manager, "gendefault.example").await.unwrap();

    // The private key exists only inside this test helper and is never returned.
    let response = manager.admin_approve(record.id, None).await.unwrap();

    assert!(!response.certificate.public_key.is_empty());
}

#[tokio::test]
async fn test_manager_uploaded_key_can_publish() {
    use base64::Engine as _;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    let pool = setup_test_pool().await;
    let manager = PublisherRegistryManager::new(pool);
    let (record, _) = pending(&manager, "pubupload.example").await.unwrap();

    // User generates their own keypair and uploads only the public key
    let user_key = SigningKey::generate(&mut OsRng);
    let user_pub_b64 =
        base64::engine::general_purpose::STANDARD.encode(user_key.verifying_key().to_bytes());

    manager
        .admin_approve(record.id, Some(&user_pub_b64))
        .await
        .unwrap();

    // Now sign and publish using the user's private key
    let manifest = test_manifest("pubupload.example", "widget", "1.0.0", "wasm32-wasip1");
    let sig = user_key.sign(manifest.as_bytes());
    let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());

    let (nonce_b64, nonce_sig_b64) = make_publish_nonce(
        manager.pool(),
        record.id,
        "pubupload.example",
        "widget",
        "1.0.0",
        "wasm32-wasip1",
        &manifest,
        &sig_b64,
        None,
        &user_key,
    )
    .await;

    let pkg = manager
        .publish_package(PublishRequest {
            publisher: "pubupload.example".to_string(),
            name: "widget".to_string(),
            version: "1.0.0".to_string(),
            target: "wasm32-wasip1".to_string(),
            manifest,
            signature: sig_b64,
            proto_files: None,
            nonce: Some(nonce_b64),
            nonce_sig: Some(nonce_sig_b64),
        })
        .await
        .unwrap();

    assert_eq!(pkg.type_str, "pubupload.example/widget@1.0.0");
    assert_eq!(pkg.status, PkgStatus::Active);
}

// ─── register_publisher tests ─────────────────────────────────────────────────

#[tokio::test]
async fn register_publisher_pins_the_first_profile() {
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    let manager = PublisherRegistryManager::new(setup_test_pool().await);
    let root = SigningKey::generate(&mut OsRng);
    let package = SigningKey::generate(&mut OsRng);
    let roots = [("root-1", &root, "active")];
    let first = signed_profile("apps.example.com", 1, &roots, &[(&package, "active")]);
    let record = register(&manager, &first).await.unwrap();
    assert_eq!(record.status, PublisherStatus::Active);
    assert_eq!(record.public_key, b64_key(&package));
    assert_eq!(
        record.key_id,
        crypto::compute_key_id(package.verifying_key().as_bytes())
    );
    assert_eq!(
        record.contact.as_deref(),
        Some("mailto:security@apps.example.com")
    );
    let info = manager.resolve_by_name("apps.example.com").await.unwrap();
    assert_eq!(info.profile.unwrap()["sequence"], 1);

    // Registering the same profile again changes nothing.
    let again = register(&manager, &first).await.unwrap();
    assert_eq!(again.profile_json, record.profile_json);

    // A later profile needs a higher sequence.
    let replay = signed_profile(
        "apps.example.com",
        1,
        &roots,
        &[(&SigningKey::generate(&mut OsRng), "active")],
    );
    assert!(matches!(
        register(&manager, &replay).await,
        Err(PublisherRegistryError::VerificationFailed(_))
    ));

    // It must be signed by a pinned active key and list no new signing key.
    let stranger = SigningKey::generate(&mut OsRng);
    for profile in [
        signed_profile(
            "apps.example.com",
            2,
            &[("root-1", &stranger, "active")],
            &[(&package, "active")],
        ),
        signed_profile(
            "apps.example.com",
            2,
            &[("root-1", &root, "active"), ("root-2", &stranger, "active")],
            &[(&package, "active")],
        ),
    ] {
        assert!(matches!(
            register(&manager, &profile).await,
            Err(PublisherRegistryError::VerificationFailed(_))
        ));
    }

    // Exactly one active package signing key is required.
    let two = signed_profile(
        "apps.example.com",
        2,
        &roots,
        &[(&package, "active"), (&stranger, "active")],
    );
    assert!(matches!(
        register(&manager, &two).await,
        Err(PublisherRegistryError::InvalidRequest(_))
    ));
}

#[tokio::test]
async fn register_publisher_never_reinstates_a_used_key() {
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    let manager = PublisherRegistryManager::new(setup_test_pool().await);
    let root = SigningKey::generate(&mut OsRng);
    let roots = [("root-1", &root, "active")];
    let key_a = SigningKey::generate(&mut OsRng);
    let key_b = SigningKey::generate(&mut OsRng);
    let domain = "rollback.example";
    register(
        &manager,
        &signed_profile(domain, 1, &roots, &[(&key_a, "active")]),
    )
    .await
    .unwrap();
    let record = register(
        &manager,
        &signed_profile(
            domain,
            2,
            &roots,
            &[(&key_a, "retired"), (&key_b, "active")],
        ),
    )
    .await
    .unwrap();
    let history = manager.admin_list_keys(record.id).await.unwrap();
    assert_eq!(history.len(), 1);

    let back = signed_profile(domain, 3, &roots, &[(&key_a, "active")]);
    assert!(matches!(
        register(&manager, &back).await,
        Err(PublisherRegistryError::InvalidRequest(_))
    ));

    // A suspended publisher accepts no profile.
    manager.admin_suspend(record.id).await.unwrap();
    let later = signed_profile(domain, 4, &roots, &[(&key_b, "active")]);
    assert!(matches!(
        register(&manager, &later).await,
        Err(PublisherRegistryError::InvalidStatus(_))
    ));
}

// ─── Key rotation + publish/verify tests ─────────────────────────────────────

#[tokio::test]
async fn test_key_rotation_then_publish_with_new_key() {
    use base64::Engine as _;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    let pool = setup_test_pool().await;
    let manager = PublisherRegistryManager::new(pool.clone());

    // Step 1: Register a Publisher Profile with the initial key (key_A)
    let root = SigningKey::generate(&mut OsRng);
    let key_a = SigningKey::generate(&mut OsRng);
    let record = register(
        &manager,
        &signed_profile(
            "rotpub.example",
            1,
            &[("root-1", &root, "active")],
            &[(&key_a, "active")],
        ),
    )
    .await
    .unwrap();

    // Step 2: Publish with key_A — should succeed
    let manifest_a = test_manifest("rotpub.example", "svc", "1.0.0", "wasm32-wasip1");
    let sig_a = key_a.sign(manifest_a.as_bytes());
    let sig_a_b64 = base64::engine::general_purpose::STANDARD.encode(sig_a.to_bytes());
    let (nonce_a, nonce_sig_a) = make_publish_nonce(
        &pool,
        record.id,
        "rotpub.example",
        "svc",
        "1.0.0",
        "wasm32-wasip1",
        &manifest_a,
        &sig_a_b64,
        None,
        &key_a,
    )
    .await;
    let pkg_a = manager
        .publish_package(PublishRequest {
            publisher: "rotpub.example".to_string(),
            name: "svc".to_string(),
            version: "1.0.0".to_string(),
            target: "wasm32-wasip1".to_string(),
            manifest: manifest_a,
            signature: sig_a_b64,
            proto_files: None,
            nonce: Some(nonce_a),
            nonce_sig: Some(nonce_sig_a),
        })
        .await
        .unwrap();
    assert_eq!(pkg_a.type_str, "rotpub.example/svc@1.0.0");

    // Step 3: Rotate key → key_B with a later Publisher Profile
    let key_b = SigningKey::generate(&mut OsRng);
    register(
        &manager,
        &signed_profile(
            "rotpub.example",
            2,
            &[("root-1", &root, "active")],
            &[(&key_a, "retired"), (&key_b, "active")],
        ),
    )
    .await
    .unwrap();

    // Step 4: Publish with key_B — should succeed (new version)
    let manifest_b = test_manifest("rotpub.example", "svc", "2.0.0", "wasm32-wasip1");
    let sig_b = key_b.sign(manifest_b.as_bytes());
    let sig_b_b64 = base64::engine::general_purpose::STANDARD.encode(sig_b.to_bytes());
    let (nonce_b, nonce_sig_b) = make_publish_nonce(
        &pool,
        record.id,
        "rotpub.example",
        "svc",
        "2.0.0",
        "wasm32-wasip1",
        &manifest_b,
        &sig_b_b64,
        None,
        &key_b,
    )
    .await;
    let pkg_b = manager
        .publish_package(PublishRequest {
            publisher: "rotpub.example".to_string(),
            name: "svc".to_string(),
            version: "2.0.0".to_string(),
            target: "wasm32-wasip1".to_string(),
            manifest: manifest_b,
            signature: sig_b_b64,
            proto_files: None,
            nonce: Some(nonce_b),
            nonce_sig: Some(nonce_sig_b),
        })
        .await
        .unwrap();
    assert_eq!(pkg_b.type_str, "rotpub.example/svc@2.0.0");

    // Step 5: Publish with OLD key_A — should FAIL
    // After key rotation, Publisher Registry's public_key is key_B. The nonce_sig signed by key_A
    // won't verify against key_B → Unauthorized before reaching manifest sig check.
    let manifest_c = test_manifest("rotpub.example", "svc", "3.0.0", "wasm32-wasip1");
    let sig_c = key_a.sign(manifest_c.as_bytes());
    let sig_c_b64 = base64::engine::general_purpose::STANDARD.encode(sig_c.to_bytes());
    // Sign nonce with old key_a — verification will fail since Publisher Registry now uses key_b
    let (nonce_c, nonce_sig_c) = make_publish_nonce(
        &pool,
        record.id,
        "rotpub.example",
        "svc",
        "3.0.0",
        "wasm32-wasip1",
        &manifest_c,
        &sig_c_b64,
        None,
        &key_a,
    )
    .await;
    let result = manager
        .publish_package(PublishRequest {
            publisher: "rotpub.example".to_string(),
            name: "svc".to_string(),
            version: "3.0.0".to_string(),
            target: "wasm32-wasip1".to_string(),
            manifest: manifest_c,
            signature: sig_c_b64,
            proto_files: None,
            nonce: Some(nonce_c),
            nonce_sig: Some(nonce_sig_c),
        })
        .await;
    assert!(
        matches!(result, Err(PublisherRegistryError::Unauthorized)),
        "old key should be rejected after rotation (nonce sig fails): {result:?}"
    );
}

#[tokio::test]
async fn test_key_rotation_historical_key_resolved() {
    use base64::Engine as _;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    let pool = setup_test_pool().await;
    let manager = PublisherRegistryManager::new(pool.clone());

    // Register key_A, then rotate to key_B
    let root = SigningKey::generate(&mut OsRng);
    let key_a = SigningKey::generate(&mut OsRng);
    let pub_a = base64::engine::general_purpose::STANDARD.encode(key_a.verifying_key().to_bytes());
    let roots = [("root-1", &root, "active")];
    let record = register(
        &manager,
        &signed_profile("histkey.example", 1, &roots, &[(&key_a, "active")]),
    )
    .await
    .unwrap();
    let key_id_a = record.key_id.clone();

    let key_b = SigningKey::generate(&mut OsRng);
    let pub_b = base64::engine::general_purpose::STANDARD.encode(key_b.verifying_key().to_bytes());
    let key_id_b = register(
        &manager,
        &signed_profile(
            "histkey.example",
            2,
            &roots,
            &[(&key_a, "retired"), (&key_b, "active")],
        ),
    )
    .await
    .unwrap()
    .key_id;

    // Resolve current key (key_B)
    let current = manager.resolve_by_name("histkey.example").await.unwrap();
    assert_eq!(current.key_id, key_id_b);
    assert_eq!(current.public_key, pub_b);

    // Resolve old key (key_A) by key_id — should still work
    let historical = manager
        .resolve_key_by_id("histkey.example", &key_id_a)
        .await
        .unwrap();
    assert_eq!(historical.key_id, key_id_a);
    assert_eq!(historical.public_key, pub_a);

    // Resolve non-existent key_id — should fail
    let result = manager
        .resolve_key_by_id("histkey.example", "actr-pub-nonexistent")
        .await;
    assert!(matches!(result, Err(PublisherRegistryError::NotFound)));

    sqlx::query("UPDATE publisher SET status = 'suspended' WHERE id = ?")
        .bind(record.id)
        .execute(&pool)
        .await
        .unwrap();
    let result = manager
        .resolve_key_by_id("histkey.example", &key_id_a)
        .await;
    assert!(
        matches!(result, Err(PublisherRegistryError::InvalidStatus(_))),
        "historical keys must not bypass publisher suspension: {result:?}"
    );
}

// ─── Error path tests ────────────────────────────────────────────────────────

#[tokio::test]
async fn test_publish_with_expired_key_rejected() {
    use base64::Engine as _;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    let pool = setup_test_pool().await;
    let key = SigningKey::generate(&mut OsRng);
    let pub_b64 = base64::engine::general_purpose::STANDARD.encode(key.verifying_key().to_bytes());

    // Create and activate publisher
    let mut record = Publisher::create(&pool, "expiredco", None).await.unwrap();
    record.activate(&pool, pub_b64).await.unwrap();

    // Force key_expires_at to past (already expired)
    sqlx::query("UPDATE publisher SET key_expires_at = ? WHERE id = ?")
        .bind(1000i64) // Unix timestamp year ~1970
        .bind(record.id)
        .execute(&pool)
        .await
        .unwrap();

    let manifest = test_manifest("expiredco", "svc", "1.0.0", "wasm32-wasip1");
    let sig = key.sign(manifest.as_bytes());
    let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());

    let manager = PublisherRegistryManager::new(pool);
    let result = manager
        .publish_package(PublishRequest {
            publisher: "expiredco".to_string(),
            name: "svc".to_string(),
            version: "1.0.0".to_string(),
            target: "wasm32-wasip1".to_string(),
            manifest,
            signature: sig_b64,
            proto_files: None,
            nonce: None,
            nonce_sig: None,
        })
        .await;
    assert!(
        matches!(result, Err(PublisherRegistryError::CertificateExpired)),
        "expired key should be rejected: {result:?}"
    );
}

#[tokio::test]
async fn test_request_nonce_with_expired_key_rejected() {
    use base64::Engine as _;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    let pool = setup_test_pool().await;
    let key = SigningKey::generate(&mut OsRng);
    let pub_b64 = base64::engine::general_purpose::STANDARD.encode(key.verifying_key().to_bytes());
    let mut record = Publisher::create(&pool, "expirednonce", None)
        .await
        .unwrap();
    record.activate(&pool, pub_b64).await.unwrap();

    sqlx::query("UPDATE publisher SET key_expires_at = ? WHERE id = ?")
        .bind(chrono::Utc::now().timestamp())
        .bind(record.id)
        .execute(&pool)
        .await
        .unwrap();

    let result = PublisherRegistryManager::new(pool)
        .request_nonce("expirednonce")
        .await;
    assert!(
        matches!(result, Err(PublisherRegistryError::CertificateExpired)),
        "an expired key must not obtain a publish nonce: {result:?}"
    );
}

#[tokio::test]
async fn test_publish_with_wrong_key_rejected() {
    use base64::Engine as _;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    let pool = setup_test_pool().await;
    let key_registered = SigningKey::generate(&mut OsRng);
    let key_attacker = SigningKey::generate(&mut OsRng);
    let pub_registered =
        base64::engine::general_purpose::STANDARD.encode(key_registered.verifying_key().to_bytes());

    let mut record = Publisher::create(&pool, "wrongkeyco", None).await.unwrap();
    record.activate(&pool, pub_registered).await.unwrap();

    // Attacker signs with their own key, not the registered one
    let manifest = test_manifest("wrongkeyco", "svc", "1.0.0", "wasm32-wasip1");
    let sig = key_attacker.sign(manifest.as_bytes());
    let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());

    // Attacker signs nonce with their key — nonce_sig won't verify against registered key
    let (nonce_b64, nonce_sig_b64) = make_publish_nonce(
        &pool,
        record.id,
        "wrongkeyco",
        "svc",
        "1.0.0",
        "wasm32-wasip1",
        &manifest,
        &sig_b64,
        None,
        &key_attacker,
    )
    .await;

    let manager = PublisherRegistryManager::new(pool);
    let result = manager
        .publish_package(PublishRequest {
            publisher: "wrongkeyco".to_string(),
            name: "svc".to_string(),
            version: "1.0.0".to_string(),
            target: "wasm32-wasip1".to_string(),
            manifest,
            signature: sig_b64,
            proto_files: None,
            nonce: Some(nonce_b64),
            nonce_sig: Some(nonce_sig_b64),
        })
        .await;
    assert!(
        matches!(result, Err(PublisherRegistryError::Unauthorized)),
        "wrong key should be rejected (nonce sig fails): {result:?}"
    );
}

#[tokio::test]
async fn test_publish_for_revoked_publisher_rejected() {
    use base64::Engine as _;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    let pool = setup_test_pool().await;
    let key = SigningKey::generate(&mut OsRng);
    let pub_b64 = base64::engine::general_purpose::STANDARD.encode(key.verifying_key().to_bytes());

    let mut record = Publisher::create(&pool, "revokedpublisher", None)
        .await
        .unwrap();
    record.activate(&pool, pub_b64).await.unwrap();
    record.revoke(&pool).await.unwrap();

    let manifest = test_manifest("revokedpublisher", "svc", "1.0.0", "wasm32-wasip1");
    let sig = key.sign(manifest.as_bytes());
    let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());

    let manager = PublisherRegistryManager::new(pool);
    let result = manager
        .publish_package(PublishRequest {
            publisher: "revokedpublisher".to_string(),
            name: "svc".to_string(),
            version: "1.0.0".to_string(),
            target: "wasm32-wasip1".to_string(),
            manifest,
            signature: sig_b64,
            proto_files: None,
            nonce: None,
            nonce_sig: None,
        })
        .await;
    assert!(
        matches!(result, Err(PublisherRegistryError::InvalidStatus(_))),
        "revoked Publisher Registry should not be able to publish: {result:?}"
    );
}

#[tokio::test]
async fn test_publish_for_suspended_publisher_rejected() {
    use base64::Engine as _;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    let pool = setup_test_pool().await;
    let key = SigningKey::generate(&mut OsRng);
    let pub_b64 = base64::engine::general_purpose::STANDARD.encode(key.verifying_key().to_bytes());

    let mut record = Publisher::create(&pool, "suspendedpublisher", None)
        .await
        .unwrap();
    record.activate(&pool, pub_b64).await.unwrap();
    record.suspend(&pool).await.unwrap();

    let manifest = test_manifest("suspendedpublisher", "svc", "1.0.0", "wasm32-wasip1");
    let sig = key.sign(manifest.as_bytes());
    let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());

    let manager = PublisherRegistryManager::new(pool);
    let result = manager
        .publish_package(PublishRequest {
            publisher: "suspendedpublisher".to_string(),
            name: "svc".to_string(),
            version: "1.0.0".to_string(),
            target: "wasm32-wasip1".to_string(),
            manifest,
            signature: sig_b64,
            proto_files: None,
            nonce: None,
            nonce_sig: None,
        })
        .await;
    assert!(
        matches!(result, Err(PublisherRegistryError::InvalidStatus(_))),
        "suspended Publisher Registry should not be able to publish: {result:?}"
    );
}

// ─── Multi-publisher / multi-package tests ────────────────────────────────

#[tokio::test]
async fn test_multi_publisher_independent_publish() {
    use base64::Engine as _;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    let pool = setup_test_pool().await;

    // Create two independent publishers with different keys
    let key_alpha = SigningKey::generate(&mut OsRng);
    let key_beta = SigningKey::generate(&mut OsRng);
    let pub_alpha =
        base64::engine::general_purpose::STANDARD.encode(key_alpha.verifying_key().to_bytes());
    let pub_beta =
        base64::engine::general_purpose::STANDARD.encode(key_beta.verifying_key().to_bytes());

    let mut publisher_alpha = Publisher::create(&pool, "alpha-corp", None).await.unwrap();
    publisher_alpha.activate(&pool, pub_alpha).await.unwrap();
    let mut publisher_beta = Publisher::create(&pool, "beta-corp", None).await.unwrap();
    publisher_beta.activate(&pool, pub_beta).await.unwrap();

    let manager = PublisherRegistryManager::new(pool.clone());

    // Alpha publishes services
    for (name, ver) in &[("gateway", "1.0.0"), ("analytics", "2.0.0")] {
        let manifest = test_manifest("alpha-corp", name, ver, "wasm32-wasip1");
        let sig = key_alpha.sign(manifest.as_bytes());
        let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());
        let (nonce_b64, nonce_sig_b64) = make_publish_nonce(
            &pool,
            publisher_alpha.id,
            "alpha-corp",
            name,
            ver,
            "wasm32-wasip1",
            &manifest,
            &sig_b64,
            None,
            &key_alpha,
        )
        .await;
        manager
            .publish_package(PublishRequest {
                publisher: "alpha-corp".to_string(),
                name: name.to_string(),
                version: ver.to_string(),
                target: "wasm32-wasip1".to_string(),
                manifest,
                signature: sig_b64,
                proto_files: None,
                nonce: Some(nonce_b64),
                nonce_sig: Some(nonce_sig_b64),
            })
            .await
            .unwrap();
    }

    // Beta publishes services
    for (name, ver) in &[("stream", "1.0.0"), ("auth", "1.0.0"), ("cache", "3.0.0")] {
        let manifest = test_manifest("beta-corp", name, ver, "wasm32-wasip1");
        let sig = key_beta.sign(manifest.as_bytes());
        let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());
        let (nonce_b64, nonce_sig_b64) = make_publish_nonce(
            &pool,
            publisher_beta.id,
            "beta-corp",
            name,
            ver,
            "wasm32-wasip1",
            &manifest,
            &sig_b64,
            None,
            &key_beta,
        )
        .await;
        manager
            .publish_package(PublishRequest {
                publisher: "beta-corp".to_string(),
                name: name.to_string(),
                version: ver.to_string(),
                target: "wasm32-wasip1".to_string(),
                manifest,
                signature: sig_b64,
                proto_files: None,
                nonce: Some(nonce_b64),
                nonce_sig: Some(nonce_sig_b64),
            })
            .await
            .unwrap();
    }

    // Verify isolation: each Publisher Registry sees only their own packages
    let alpha_pkgs = manager.list_packages(Some("alpha-corp")).await.unwrap();
    assert_eq!(alpha_pkgs.len(), 2);
    assert!(alpha_pkgs.iter().all(|p| p.publisher == "alpha-corp"));

    let beta_pkgs = manager.list_packages(Some("beta-corp")).await.unwrap();
    assert_eq!(beta_pkgs.len(), 3);
    assert!(beta_pkgs.iter().all(|p| p.publisher == "beta-corp"));

    // All packages visible in global listing
    let all = manager.list_packages(None).await.unwrap();
    assert_eq!(all.len(), 5);

    // Cross-publisher publish should FAIL: alpha's key signing for beta's name
    // The nonce is issued for beta-corp (publisher_beta.id), but signed with alpha's key.
    // Nonce sig verification will fail since Publisher Registry beta-corp uses beta's public key.
    let cross_manifest = test_manifest("beta-corp", "hack", "1.0.0", "wasm32-wasip1");
    let cross_sig = key_alpha.sign(cross_manifest.as_bytes());
    let cross_sig_b64 = base64::engine::general_purpose::STANDARD.encode(cross_sig.to_bytes());
    let (nonce_cross, nonce_sig_cross) = make_publish_nonce(
        &pool,
        publisher_beta.id,
        "beta-corp",
        "hack",
        "1.0.0",
        "wasm32-wasip1",
        &cross_manifest,
        &cross_sig_b64,
        None,
        &key_alpha,
    )
    .await;
    let result = manager
        .publish_package(PublishRequest {
            publisher: "beta-corp".to_string(),
            name: "hack".to_string(),
            version: "1.0.0".to_string(),
            target: "wasm32-wasip1".to_string(),
            manifest: cross_manifest,
            signature: cross_sig_b64,
            proto_files: None,
            nonce: Some(nonce_cross),
            nonce_sig: Some(nonce_sig_cross),
        })
        .await;
    assert!(
        matches!(result, Err(PublisherRegistryError::Unauthorized)),
        "cross-publisher publish should fail (nonce sig mismatch): {result:?}"
    );
}

#[tokio::test]
async fn test_multi_target_same_package() {
    use base64::Engine as _;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    let pool = setup_test_pool().await;
    let key = SigningKey::generate(&mut OsRng);
    let pub_b64 = base64::engine::general_purpose::STANDARD.encode(key.verifying_key().to_bytes());

    let mut record = Publisher::create(&pool, "multiarch", None).await.unwrap();
    record.activate(&pool, pub_b64).await.unwrap();

    let manager = PublisherRegistryManager::new(pool.clone());

    // Same package name+version, different targets
    for target in &[
        "wasm32-wasip1",
        "x86_64-unknown-linux-gnu",
        "aarch64-apple-darwin",
    ] {
        let manifest = format!(
            "publisher = \"multiarch\"\nname = \"server\"\nversion = \"1.0.0\"\n\n[binary]\npath = \"server.wasm\"\ntarget = \"{target}\"\nhash = \"sha256:abc123\""
        );
        let sig = key.sign(manifest.as_bytes());
        let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());
        let (nonce_b64, nonce_sig_b64) = make_publish_nonce(
            &pool,
            record.id,
            "multiarch",
            "server",
            "1.0.0",
            target,
            &manifest,
            &sig_b64,
            None,
            &key,
        )
        .await;
        manager
            .publish_package(PublishRequest {
                publisher: "multiarch".to_string(),
                name: "server".to_string(),
                version: "1.0.0".to_string(),
                target: target.to_string(),
                manifest,
                signature: sig_b64,
                proto_files: None,
                nonce: Some(nonce_b64),
                nonce_sig: Some(nonce_sig_b64),
            })
            .await
            .unwrap();
    }

    let pkgs = manager.list_packages(Some("multiarch")).await.unwrap();
    assert_eq!(
        pkgs.len(),
        3,
        "same package with 3 targets should produce 3 entries"
    );
    let targets: Vec<&str> = pkgs.iter().map(|p| p.target.as_str()).collect();
    assert!(targets.contains(&"wasm32-wasip1"));
    assert!(targets.contains(&"x86_64-unknown-linux-gnu"));
    assert!(targets.contains(&"aarch64-apple-darwin"));
}

#[tokio::test]
async fn test_publish_tampered_proto_files_rejected() {
    use base64::Engine as _;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    let pool = setup_test_pool().await;
    let key = SigningKey::generate(&mut OsRng);
    let pub_b64 = base64::engine::general_purpose::STANDARD.encode(key.verifying_key().to_bytes());

    let mut record = Publisher::create(&pool, "protoco", None).await.unwrap();
    record.activate(&pool, pub_b64).await.unwrap();

    let manifest = test_manifest("protoco", "svc", "1.0.0", "wasm32-wasip1");
    let sig = key.sign(manifest.as_bytes());
    let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());
    let proto_original = serde_json::json!({
        "protobufs": [
            { "relative_path": "api/v1/service.proto", "content_b64": "YWJj" }
        ]
    });
    let proto_tampered = serde_json::json!({
        "protobufs": [
            { "relative_path": "api/v1/service.proto", "content_b64": "ZGVm" }
        ]
    });

    let (nonce_b64, nonce_sig_b64) = make_publish_nonce(
        &pool,
        record.id,
        "protoco",
        "svc",
        "1.0.0",
        "wasm32-wasip1",
        &manifest,
        &sig_b64,
        Some(&proto_original),
        &key,
    )
    .await;

    let manager = PublisherRegistryManager::new(pool);
    let result = manager
        .publish_package(PublishRequest {
            publisher: "protoco".to_string(),
            name: "svc".to_string(),
            version: "1.0.0".to_string(),
            target: "wasm32-wasip1".to_string(),
            manifest,
            signature: sig_b64,
            proto_files: Some(proto_tampered),
            nonce: Some(nonce_b64),
            nonce_sig: Some(nonce_sig_b64),
        })
        .await;

    assert!(
        matches!(result, Err(PublisherRegistryError::Unauthorized)),
        "tampered proto_files should invalidate nonce authorization: {result:?}"
    );
}

#[tokio::test]
async fn test_publish_manifest_missing_required_field_rejected() {
    use base64::Engine as _;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    let pool = setup_test_pool().await;
    let key = SigningKey::generate(&mut OsRng);
    let pub_b64 = base64::engine::general_purpose::STANDARD.encode(key.verifying_key().to_bytes());

    let mut record = Publisher::create(&pool, "missingfield", None)
        .await
        .unwrap();
    record.activate(&pool, pub_b64).await.unwrap();

    let manifest = r#"publisher = "missingfield"
name = "svc"

[binary]
path = "bin/actor.wasm"
target = "wasm32-wasip1"
hash = "sha256:abc123"
"#;
    let sig = key.sign(manifest.as_bytes());
    let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());
    let (nonce_b64, nonce_sig_b64) = make_publish_nonce(
        &pool,
        record.id,
        "missingfield",
        "svc",
        "1.0.0",
        "wasm32-wasip1",
        manifest,
        &sig_b64,
        None,
        &key,
    )
    .await;

    let manager = PublisherRegistryManager::new(pool);
    let result = manager
        .publish_package(PublishRequest {
            publisher: "missingfield".to_string(),
            name: "svc".to_string(),
            version: "1.0.0".to_string(),
            target: "wasm32-wasip1".to_string(),
            manifest: manifest.to_string(),
            signature: sig_b64,
            proto_files: None,
            nonce: Some(nonce_b64),
            nonce_sig: Some(nonce_sig_b64),
        })
        .await;

    assert!(
        matches!(result, Err(PublisherRegistryError::InvalidRequest(ref msg)) if msg.contains("missing required field 'version'")),
        "manifest missing version should be rejected: {result:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_register_publisher_requests_do_not_fail_internally() {
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    // A file database in WAL mode with several connections, as in production.
    let dir = tempfile::tempdir().unwrap();
    let database = platform::storage::db::Database::new(dir.path())
        .await
        .unwrap();
    let manager = std::sync::Arc::new(PublisherRegistryManager::new(database.get_pool().clone()));
    let root = SigningKey::generate(&mut OsRng);
    let roots = [("root-1", &root, "active")];
    let domain = "race.example";
    let first = signed_profile(
        domain,
        1,
        &roots,
        &[(&SigningKey::generate(&mut OsRng), "active")],
    );
    let later: Vec<_> = (2..10)
        .map(|sequence| {
            signed_profile(
                domain,
                sequence,
                &roots,
                &[(&SigningKey::generate(&mut OsRng), "active")],
            )
        })
        .collect();

    // The first registration, sent many times at once.
    let tasks: Vec<_> = (0..8)
        .map(|_| {
            let (manager, profile) = (manager.clone(), first.clone());
            tokio::spawn(async move { register(&manager, &profile).await })
        })
        .collect();
    for task in tasks {
        task.await
            .unwrap()
            .expect("the same first profile registers");
    }

    // Later profiles at once: each either wins or is refused as not
    // following the accepted one, never an internal error.
    let tasks: Vec<_> = later
        .into_iter()
        .map(|profile| {
            let manager = manager.clone();
            tokio::spawn(async move { register(&manager, &profile).await })
        })
        .collect();
    for task in tasks {
        match task.await.unwrap() {
            Ok(_)
            | Err(PublisherRegistryError::VerificationFailed(_))
            | Err(PublisherRegistryError::InvalidStatus(_)) => {}
            Err(error) => panic!("unexpected error: {error:?}"),
        }
    }
}

#[tokio::test]
async fn register_publisher_refuses_a_package_key_outside_its_validity() {
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    let manager = PublisherRegistryManager::new(setup_test_pool().await);
    let root = SigningKey::generate(&mut OsRng);
    let now = chrono::Utc::now();
    let with_window = |not_before: Option<String>, not_after: Option<String>| {
        let profile = signed_profile(
            "window.example",
            1,
            &[("root-1", &root, "active")],
            &[(&SigningKey::generate(&mut OsRng), "active")],
        );
        let mut document = serde_json::to_value(&profile).unwrap();
        document["keys"][0]["not_before"] = not_before.into();
        document["keys"][0]["not_after"] = not_after.into();
        actr_protocol::realm_profile::sign_document(&mut document, "root-1", &root);
        serde_json::from_value::<actr_protocol::PublisherProfile>(document).unwrap()
    };
    let at = |offset: chrono::Duration| Some((now + offset).to_rfc3339());

    for (not_before, not_after) in [
        // Not valid yet.
        (at(chrono::Duration::days(1)), None),
        // Already expired.
        (None, at(-chrono::Duration::days(1))),
        // Not RFC 3339.
        (Some("tomorrow".to_string()), None),
    ] {
        let profile = with_window(not_before.clone(), not_after.clone());
        assert!(
            matches!(
                register(&manager, &profile).await,
                Err(PublisherRegistryError::InvalidRequest(_))
            ),
            "{not_before:?} {not_after:?}"
        );
    }
    let valid = with_window(
        at(-chrono::Duration::days(1)),
        at(chrono::Duration::days(30)),
    );
    let record = register(&manager, &valid).await.unwrap();
    assert!(record.key_expires_at.unwrap() > now.timestamp());
}

#[tokio::test]
async fn register_publisher_refuses_a_package_key_that_is_not_a_curve_point() {
    use base64::Engine as _;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    // 32 bytes that do not decode to an Ed25519 point.
    let not_a_point = (0u8..=255)
        .map(|byte| {
            let mut bytes = [0u8; 32];
            bytes[0] = byte;
            bytes[31] = 0x7f;
            bytes
        })
        .find(|bytes| ed25519_dalek::VerifyingKey::from_bytes(bytes).is_err())
        .expect("some bytes are not a point");

    let manager = PublisherRegistryManager::new(setup_test_pool().await);
    let root = SigningKey::generate(&mut OsRng);
    let profile = signed_profile(
        "point.example",
        1,
        &[("root-1", &root, "active")],
        &[(&SigningKey::generate(&mut OsRng), "active")],
    );
    let mut document = serde_json::to_value(&profile).unwrap();
    document["keys"][0]["public_key"] = base64::engine::general_purpose::STANDARD
        .encode(not_a_point)
        .into();
    actr_protocol::realm_profile::sign_document(&mut document, "root-1", &root);
    let profile: actr_protocol::PublisherProfile = serde_json::from_value(document).unwrap();
    let result = register(&manager, &profile).await;
    assert!(
        matches!(result, Err(PublisherRegistryError::InvalidRequest(_))),
        "{result:?}"
    );
}
