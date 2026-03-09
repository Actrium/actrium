/// MFR 模块集成测试
///
/// 所有需要数据库的测试使用 in-process SQLite 内存库，每个测试独立 pool，互不干扰。

use sqlx::SqlitePool;

use crate::{
    crypto,
    manager::{lookup_package, MfrManager, PublishRequest},
    model::{ActrPackage, DomainChallenge, Manufacturer, MfrStatus, PkgStatus},
    reserved::{domain_to_name, is_reserved, validate_name},
    MfrError,
};

// ─── 测试辅助 ────────────────────────────────────────────────────────────────

async fn setup_test_pool() -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .expect("failed to create in-memory sqlite pool");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS mfr (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            domain TEXT NOT NULL UNIQUE,
            public_key TEXT NOT NULL DEFAULT '',
            contact TEXT,
            status TEXT NOT NULL DEFAULT 'pending',
            created_at INTEGER NOT NULL,
            updated_at INTEGER,
            verified_at INTEGER,
            suspended_at INTEGER,
            revoked_at INTEGER
        )",
    )
    .execute(&pool)
    .await
    .expect("failed to create mfr table");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS mfr_challenge (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            mfr_id INTEGER NOT NULL REFERENCES mfr(id),
            token TEXT NOT NULL,
            dns_host TEXT NOT NULL,
            expires_at INTEGER NOT NULL,
            verified_at INTEGER,
            created_at INTEGER NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .expect("failed to create mfr_challenge table");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS mfr_package (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            mfr_id INTEGER NOT NULL REFERENCES mfr(id),
            manufacturer TEXT NOT NULL,
            name TEXT NOT NULL,
            version TEXT NOT NULL,
            type_str TEXT NOT NULL,
            manifest TEXT NOT NULL,
            signature TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'active',
            published_at INTEGER NOT NULL,
            revoked_at INTEGER,
            UNIQUE(manufacturer, name, version)
        )",
    )
    .execute(&pool)
    .await
    .expect("failed to create mfr_package table");

    pool
}

// ─── reserved.rs 纯单元测试 ──────────────────────────────────────────────────

#[test]
fn test_reserved_names_exact() {
    assert!(is_reserved("self"));
    assert!(is_reserved("acme"));
    assert!(is_reserved("actrix"));
}

#[test]
fn test_reserved_names_case_insensitive() {
    assert!(is_reserved("SELF"));
    assert!(is_reserved("Acme"));
    assert!(is_reserved("ACTRIX"));
}

#[test]
fn test_non_reserved_names() {
    assert!(!is_reserved("mycompany"));
    assert!(!is_reserved("apple"));
    assert!(!is_reserved("openai"));
}

#[test]
fn test_validate_name_reserved() {
    assert!(matches!(
        validate_name("self"),
        Err(MfrError::ReservedName(_))
    ));
    assert!(matches!(
        validate_name("acme"),
        Err(MfrError::ReservedName(_))
    ));
    assert!(matches!(
        validate_name("actrix"),
        Err(MfrError::ReservedName(_))
    ));
}

#[test]
fn test_validate_name_too_short() {
    assert!(matches!(validate_name("ab"), Err(MfrError::InvalidName(_))));
}

#[test]
fn test_validate_name_too_long() {
    let long = "a".repeat(129);
    assert!(matches!(
        validate_name(&long),
        Err(MfrError::InvalidName(_))
    ));
}

#[test]
fn test_validate_name_invalid_chars() {
    assert!(matches!(
        validate_name("My Company"),
        Err(MfrError::InvalidName(_))
    ));
    assert!(matches!(
        validate_name("my_company"),
        Err(MfrError::InvalidName(_))
    ));
    assert!(matches!(
        validate_name("MyCompany"),
        Err(MfrError::InvalidName(_))
    ));
}

#[test]
fn test_validate_name_dot_boundary() {
    // must not start or end with dot
    assert!(matches!(
        validate_name(".com.myco"),
        Err(MfrError::InvalidName(_))
    ));
    assert!(matches!(
        validate_name("com.myco."),
        Err(MfrError::InvalidName(_))
    ));
}

#[test]
fn test_validate_name_valid() {
    assert!(validate_name("com.mycompany").is_ok());
    assert!(validate_name("com.my-company").is_ok());
    assert!(validate_name("com.example.sub").is_ok());
    assert!(validate_name("abc").is_ok());
    let max = "a".repeat(128);
    assert!(validate_name(&max).is_ok());
}

#[test]
fn test_domain_to_name() {
    assert_eq!(domain_to_name("myco.com"), "com.myco");
    assert_eq!(domain_to_name("sub.example.com"), "com.example.sub");
    assert_eq!(domain_to_name("example.com:8080"), "com.example");
    assert_eq!(domain_to_name("single"), "single");
}

// ─── crypto.rs 纯单元测试 ────────────────────────────────────────────────────

#[test]
fn test_generate_keypair_roundtrip() {
    use base64::Engine as _;

    let (private_b64, public_b64) = crypto::generate_keypair();
    assert!(!private_b64.is_empty());
    assert!(!public_b64.is_empty());

    let priv_bytes = base64::engine::general_purpose::STANDARD
        .decode(&private_b64)
        .expect("private key should be valid base64");
    assert_eq!(priv_bytes.len(), 32, "Ed25519 private key should be 32 bytes");

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
    let message = b"hello mfr";
    let sig = signing_key.sign(message);

    let sig_b64 =
        base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());
    let pub_b64 =
        base64::engine::general_purpose::STANDARD.encode(verifying_key.to_bytes());

    let result = crypto::verify_signature(message, &sig_b64, &pub_b64)
        .expect("verify_signature should not error on valid inputs");
    assert!(result, "valid signature should verify as true");
}

#[test]
fn test_verify_signature_invalid_zeros() {
    use base64::Engine as _;
    let (_, pub_b64) = crypto::generate_keypair();
    let bad_sig =
        base64::engine::general_purpose::STANDARD.encode([0u8; 64]);
    let result = crypto::verify_signature(b"message", &bad_sig, &pub_b64);
    assert!(
        matches!(result, Ok(false) | Err(MfrError::Crypto(_))),
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

    let sig_b64 =
        base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());
    let wrong_pub_b64 = base64::engine::general_purpose::STANDARD
        .encode(key2.verifying_key().to_bytes());

    let result = crypto::verify_signature(message, &sig_b64, &wrong_pub_b64)
        .expect("should not return error for valid encoding");
    assert!(!result, "signature with wrong key should verify as false");
}

#[test]
fn test_verify_signature_bad_pubkey_encoding() {
    let bad_pub = "not-valid-base64!!!";
    let result = crypto::verify_signature(b"msg", "anysig", bad_pub);
    assert!(
        matches!(result, Err(MfrError::Crypto(_))),
        "bad base64 pubkey should return Crypto error"
    );
}

// ─── model/manufacturer.rs 测试（需 DB）─────────────────────────────────────

#[tokio::test]
async fn test_manufacturer_create_and_get() {
    let pool = setup_test_pool().await;
    let mfr = Manufacturer::create(
        &pool,
        "testco",
        "testco.example.com",
        Some("admin@testco.com"),
    )
    .await
    .expect("create should succeed");

    assert_eq!(mfr.name, "testco");
    assert_eq!(mfr.domain, "testco.example.com");
    assert_eq!(mfr.status, MfrStatus::Pending);
    assert!(mfr.verified_at.is_none());
    assert_eq!(mfr.contact.as_deref(), Some("admin@testco.com"));

    let found = Manufacturer::get(&pool, mfr.id)
        .await
        .expect("get should succeed")
        .expect("should find created manufacturer");
    assert_eq!(found.name, "testco");
    assert_eq!(found.id, mfr.id);
}

#[tokio::test]
async fn test_manufacturer_get_nonexistent() {
    let pool = setup_test_pool().await;
    let found = Manufacturer::get(&pool, 9999).await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn test_manufacturer_get_by_name() {
    let pool = setup_test_pool().await;
    let mfr = Manufacturer::create(&pool, "namedco", "namedco.com", None)
        .await
        .unwrap();

    let found = Manufacturer::get_by_name(&pool, "namedco")
        .await
        .unwrap()
        .expect("should find by name");
    assert_eq!(found.id, mfr.id);

    let missing = Manufacturer::get_by_name(&pool, "nobody").await.unwrap();
    assert!(missing.is_none());
}

#[tokio::test]
async fn test_manufacturer_duplicate_name() {
    let pool = setup_test_pool().await;
    Manufacturer::create(&pool, "dupco", "dupco.com", None)
        .await
        .unwrap();
    let result = Manufacturer::create(&pool, "dupco", "other.com", None).await;
    assert!(
        matches!(result, Err(MfrError::AlreadyExists(_))),
        "duplicate name should return AlreadyExists"
    );
}

#[tokio::test]
async fn test_manufacturer_duplicate_domain() {
    let pool = setup_test_pool().await;
    Manufacturer::create(&pool, "co1", "shared.com", None)
        .await
        .unwrap();
    let result = Manufacturer::create(&pool, "co2", "shared.com", None).await;
    assert!(
        matches!(result, Err(MfrError::AlreadyExists(_))),
        "duplicate domain should return AlreadyExists"
    );
}

#[tokio::test]
async fn test_manufacturer_activate() {
    let pool = setup_test_pool().await;
    let mut mfr =
        Manufacturer::create(&pool, "activeco", "activeco.com", None)
            .await
            .unwrap();

    mfr.activate(&pool, "pubkey_base64".to_string())
        .await
        .expect("activate from pending should succeed");

    assert_eq!(mfr.status, MfrStatus::Active);
    assert_eq!(mfr.public_key, "pubkey_base64");
    assert!(mfr.verified_at.is_some());
    assert!(mfr.updated_at.is_some());

    let from_db = Manufacturer::get(&pool, mfr.id).await.unwrap().unwrap();
    assert_eq!(from_db.status, MfrStatus::Active);
    assert_eq!(from_db.public_key, "pubkey_base64");
}

#[tokio::test]
async fn test_manufacturer_lifecycle_full() {
    let pool = setup_test_pool().await;
    let mut mfr =
        Manufacturer::create(&pool, "lifecycle", "lifecycle.com", None)
            .await
            .unwrap();

    mfr.activate(&pool, "pubkey123".to_string()).await.unwrap();
    assert_eq!(mfr.status, MfrStatus::Active);

    mfr.suspend(&pool).await.unwrap();
    assert_eq!(mfr.status, MfrStatus::Suspended);
    assert!(mfr.suspended_at.is_some());

    mfr.reinstate(&pool).await.unwrap();
    assert_eq!(mfr.status, MfrStatus::Active);

    mfr.revoke(&pool).await.unwrap();
    assert_eq!(mfr.status, MfrStatus::Revoked);
    assert!(mfr.revoked_at.is_some());
}

#[tokio::test]
async fn test_manufacturer_invalid_transitions_from_pending() {
    let pool = setup_test_pool().await;
    let mut mfr =
        Manufacturer::create(&pool, "transco", "transco.com", None)
            .await
            .unwrap();

    let err = mfr.suspend(&pool).await.unwrap_err();
    assert!(matches!(err, MfrError::InvalidStatus(_)));

    let err = mfr.reinstate(&pool).await.unwrap_err();
    assert!(matches!(err, MfrError::InvalidStatus(_)));
}

#[tokio::test]
async fn test_manufacturer_cannot_activate_twice() {
    let pool = setup_test_pool().await;
    let mut mfr =
        Manufacturer::create(&pool, "twiceco", "twiceco.com", None)
            .await
            .unwrap();

    mfr.activate(&pool, "key1".to_string()).await.unwrap();
    let err = mfr.activate(&pool, "key2".to_string()).await.unwrap_err();
    assert!(matches!(err, MfrError::InvalidStatus(_)));
}

#[tokio::test]
async fn test_manufacturer_list_all() {
    let pool = setup_test_pool().await;
    Manufacturer::create(&pool, "list1", "list1.com", None)
        .await
        .unwrap();
    Manufacturer::create(&pool, "list2", "list2.com", None)
        .await
        .unwrap();

    let all = Manufacturer::list(&pool, None).await.unwrap();
    assert_eq!(all.len(), 2);
}

#[tokio::test]
async fn test_manufacturer_list_by_status() {
    let pool = setup_test_pool().await;
    Manufacturer::create(&pool, "statuslist1", "statuslist1.com", None)
        .await
        .unwrap();
    let mut mfr2 =
        Manufacturer::create(&pool, "statuslist2", "statuslist2.com", None)
            .await
            .unwrap();
    mfr2.activate(&pool, "pk".to_string()).await.unwrap();

    let active = Manufacturer::list(&pool, Some(MfrStatus::Active))
        .await
        .unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].name, "statuslist2");

    let pending = Manufacturer::list(&pool, Some(MfrStatus::Pending))
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].name, "statuslist1");
}

#[tokio::test]
async fn test_manufacturer_delete() {
    let pool = setup_test_pool().await;
    let mfr = Manufacturer::create(&pool, "delco", "delco.com", None)
        .await
        .unwrap();
    let id = mfr.id;

    Manufacturer::delete(&pool, id).await.unwrap();

    let found = Manufacturer::get(&pool, id).await.unwrap();
    assert!(found.is_none());
}

// ─── model/challenge.rs 测试（需 DB）─────────────────────────────────────────

#[tokio::test]
async fn test_challenge_create() {
    let pool = setup_test_pool().await;
    let mfr = Manufacturer::create(&pool, "chco", "challenge.com", None)
        .await
        .unwrap();

    let ch = DomainChallenge::create(&pool, mfr.id, "challenge.com")
        .await
        .unwrap();

    assert!(
        ch.token.starts_with("actrix-verify="),
        "token should start with 'actrix-verify=', got: {}",
        ch.token
    );
    assert_eq!(ch.dns_host, "_actrix-verify.challenge.com");
    assert!(ch.verified_at.is_none());
    assert!(ch.expires_at > ch.created_at);
    assert_eq!(ch.mfr_id, mfr.id);
}

#[tokio::test]
async fn test_challenge_get_active_found() {
    let pool = setup_test_pool().await;
    let mfr = Manufacturer::create(&pool, "activech", "activech.com", None)
        .await
        .unwrap();
    let ch = DomainChallenge::create(&pool, mfr.id, "activech.com")
        .await
        .unwrap();

    let active = DomainChallenge::get_active(&pool, mfr.id).await.unwrap();
    assert!(active.is_some());
    assert_eq!(active.unwrap().id, ch.id);
}

#[tokio::test]
async fn test_challenge_get_active_none_when_empty() {
    let pool = setup_test_pool().await;
    let mfr = Manufacturer::create(&pool, "nochco", "nochco.com", None)
        .await
        .unwrap();

    let active = DomainChallenge::get_active(&pool, mfr.id).await.unwrap();
    assert!(active.is_none());
}

#[tokio::test]
async fn test_challenge_mark_verified() {
    let pool = setup_test_pool().await;
    let mfr = Manufacturer::create(&pool, "verch", "verch.com", None)
        .await
        .unwrap();
    let mut ch = DomainChallenge::create(&pool, mfr.id, "verch.com")
        .await
        .unwrap();

    ch.mark_verified(&pool).await.unwrap();
    assert!(ch.verified_at.is_some());

    let active = DomainChallenge::get_active(&pool, mfr.id).await.unwrap();
    assert!(
        active.is_none(),
        "verified challenge should not appear in get_active"
    );
}

#[tokio::test]
async fn test_challenge_token_unique() {
    let pool = setup_test_pool().await;
    let mfr = Manufacturer::create(&pool, "tokenco", "tokenco.com", None)
        .await
        .unwrap();

    let ch1 = DomainChallenge::create(&pool, mfr.id, "tokenco.com")
        .await
        .unwrap();
    let ch2 = DomainChallenge::create(&pool, mfr.id, "tokenco.com")
        .await
        .unwrap();

    assert_ne!(ch1.token, ch2.token, "each challenge should have a unique token");
}

// ─── model/package.rs 测试（需 DB）──────────────────────────────────────────

#[tokio::test]
async fn test_package_publish_and_get() {
    let pool = setup_test_pool().await;
    let mut mfr = Manufacturer::create(&pool, "pkgco", "pkgco.com", None)
        .await
        .unwrap();
    mfr.activate(&pool, "pubkey".to_string()).await.unwrap();

    let pkg = ActrPackage::publish(
        &pool, mfr.id, "pkgco", "client", "v1", "manifest content", "sig123",
    )
    .await
    .unwrap();

    assert_eq!(pkg.type_str, "pkgco:client:v1");
    assert_eq!(pkg.status, PkgStatus::Active);
    assert_eq!(pkg.manufacturer, "pkgco");
    assert_eq!(pkg.name, "client");
    assert_eq!(pkg.version, "v1");

    let found = ActrPackage::get_by_type(&pool, "pkgco:client:v1")
        .await
        .unwrap()
        .expect("should find published package");
    assert_eq!(found.id, pkg.id);
}

#[tokio::test]
async fn test_package_get_by_type_not_found() {
    let pool = setup_test_pool().await;
    let found = ActrPackage::get_by_type(&pool, "nobody:nothing:v0")
        .await
        .unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn test_package_duplicate_rejected() {
    let pool = setup_test_pool().await;
    let mut mfr = Manufacturer::create(&pool, "dupkg", "dupkg.com", None)
        .await
        .unwrap();
    mfr.activate(&pool, "pk".to_string()).await.unwrap();

    ActrPackage::publish(&pool, mfr.id, "dupkg", "svc", "v1", "m", "s")
        .await
        .unwrap();
    let result =
        ActrPackage::publish(&pool, mfr.id, "dupkg", "svc", "v1", "m2", "s2").await;
    assert!(
        matches!(result, Err(MfrError::PackageAlreadyPublished)),
        "duplicate publish should return PackageAlreadyPublished"
    );
}

#[tokio::test]
async fn test_package_revoke() {
    let pool = setup_test_pool().await;
    let mut mfr = Manufacturer::create(&pool, "revpkg", "revpkg.com", None)
        .await
        .unwrap();
    mfr.activate(&pool, "pk".to_string()).await.unwrap();

    let mut pkg =
        ActrPackage::publish(&pool, mfr.id, "revpkg", "svc", "v1", "m", "s")
            .await
            .unwrap();
    pkg.revoke(&pool).await.unwrap();

    assert_eq!(pkg.status, PkgStatus::Revoked);
    assert!(pkg.revoked_at.is_some());

    let found = ActrPackage::get_by_type(&pool, "revpkg:svc:v1").await.unwrap();
    assert!(found.is_none(), "revoked package should not be found by get_by_type");
}

#[tokio::test]
async fn test_package_list_by_mfr() {
    let pool = setup_test_pool().await;
    let mut mfr = Manufacturer::create(&pool, "listpkg", "listpkg.com", None)
        .await
        .unwrap();
    mfr.activate(&pool, "pk".to_string()).await.unwrap();

    ActrPackage::publish(&pool, mfr.id, "listpkg", "alpha", "v1", "m", "s")
        .await
        .unwrap();
    ActrPackage::publish(&pool, mfr.id, "listpkg", "beta", "v1", "m", "s")
        .await
        .unwrap();

    let pkgs = ActrPackage::list_by_mfr(&pool, mfr.id).await.unwrap();
    assert_eq!(pkgs.len(), 2);
}

#[tokio::test]
async fn test_package_get_by_id() {
    let pool = setup_test_pool().await;
    let mut mfr = Manufacturer::create(&pool, "idpkg", "idpkg.com", None)
        .await
        .unwrap();
    mfr.activate(&pool, "pk".to_string()).await.unwrap();

    let pkg =
        ActrPackage::publish(&pool, mfr.id, "idpkg", "svc", "v1", "m", "s")
            .await
            .unwrap();

    let found = ActrPackage::get_by_id(&pool, pkg.id).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().type_str, "idpkg:svc:v1");
}

// ─── manager.rs 测试（需 DB）─────────────────────────────────────────────────

#[tokio::test]
async fn test_lookup_package_reserved() {
    let pool = setup_test_pool().await;
    assert!(lookup_package(&pool, "self:anything:v1").await.unwrap());
    assert!(lookup_package(&pool, "acme:client:v1").await.unwrap());
    assert!(lookup_package(&pool, "actrix:core:v1").await.unwrap());
    assert!(lookup_package(&pool, "SELF:svc:v1").await.unwrap());
}

#[tokio::test]
async fn test_lookup_package_not_registered() {
    let pool = setup_test_pool().await;
    assert!(!lookup_package(&pool, "unknown:svc:v1").await.unwrap());
}

#[tokio::test]
async fn test_lookup_package_active() {
    let pool = setup_test_pool().await;
    let mut mfr = Manufacturer::create(&pool, "lookco", "lookco.com", None)
        .await
        .unwrap();
    mfr.activate(&pool, "pk".to_string()).await.unwrap();
    ActrPackage::publish(&pool, mfr.id, "lookco", "svc", "v1", "m", "s")
        .await
        .unwrap();

    assert!(lookup_package(&pool, "lookco:svc:v1").await.unwrap());
    assert!(!lookup_package(&pool, "lookco:svc:v2").await.unwrap());
}

#[tokio::test]
async fn test_lookup_package_revoked() {
    let pool = setup_test_pool().await;
    let mut mfr =
        Manufacturer::create(&pool, "revokedlook", "revokedlook.com", None)
            .await
            .unwrap();
    mfr.activate(&pool, "pk".to_string()).await.unwrap();
    let mut pkg =
        ActrPackage::publish(&pool, mfr.id, "revokedlook", "svc", "v1", "m", "s")
            .await
            .unwrap();
    pkg.revoke(&pool).await.unwrap();

    assert!(
        !lookup_package(&pool, "revokedlook:svc:v1").await.unwrap(),
        "revoked package should not be found"
    );
}

#[tokio::test]
async fn test_manager_apply_reserved_rejected() {
    let pool = setup_test_pool().await;
    let manager = MfrManager::new(pool);
    // "acme.example.com" → name = "com.example.acme", not reserved
    // Use a domain that maps directly to a reserved name (single-label)
    // Reserved names: "self", "acme", "actrix" — single-label domains map to themselves
    let result = manager.apply("acme", None).await;
    assert!(matches!(result, Err(MfrError::ReservedName(_))));
}

#[tokio::test]
async fn test_manager_apply_valid() {
    let pool = setup_test_pool().await;
    let manager = MfrManager::new(pool);
    let (mfr, challenge) = manager
        .apply("newco.com", Some("admin@newco.com"))
        .await
        .unwrap();

    assert_eq!(mfr.name, "com.newco");
    assert_eq!(mfr.status, MfrStatus::Pending);
    assert!(challenge.token.starts_with("actrix-verify="));
    assert_eq!(challenge.dns_host, "_actrix-verify.newco.com");
}

#[tokio::test]
async fn test_manager_apply_invalid_name() {
    let pool = setup_test_pool().await;
    let manager = MfrManager::new(pool);
    // Domain with uppercase letters in derived name — domain is case-sensitive in DNS
    // but we lowercase in domain_to_name; let's use a domain that produces underscores
    // Actually domain_to_name just reverses parts, valid domains can't have underscores
    // Use single char domain to trigger too-short error
    let result = manager.apply("ab", None).await;
    assert!(matches!(result, Err(MfrError::InvalidName(_))));
}

#[tokio::test]
async fn test_manager_get_status() {
    let pool = setup_test_pool().await;
    let manager = MfrManager::new(pool);
    let (mfr, _) = manager.apply("statusco.com", None).await.unwrap();

    let status = manager.get_status(mfr.id).await.unwrap();
    assert_eq!(status.name, "com.statusco");
    assert_eq!(status.status, MfrStatus::Pending);
}

#[tokio::test]
async fn test_manager_get_status_not_found() {
    let pool = setup_test_pool().await;
    let manager = MfrManager::new(pool);
    let result = manager.get_status(9999).await;
    assert!(matches!(result, Err(MfrError::NotFound)));
}

#[tokio::test]
async fn test_manager_admin_approve() {
    let pool = setup_test_pool().await;
    let manager = MfrManager::new(pool);
    let (mfr, _) = manager
        .apply("approveco.com", None)
        .await
        .unwrap();

    let keychain = manager.admin_approve(mfr.id).await.unwrap();
    assert_eq!(keychain.certificate.mfr_name, "com.approveco");
    assert!(!keychain.private_key.is_empty());
    assert!(!keychain.certificate.mfr_pubkey.is_empty());
    assert!(keychain.certificate.expires_at > keychain.certificate.issued_at);
}

#[tokio::test]
async fn test_manager_admin_suspend_reinstate() {
    let pool = setup_test_pool().await;
    let manager = MfrManager::new(pool);
    let (mfr, _) = manager.apply("suspco.com", None).await.unwrap();
    manager.admin_approve(mfr.id).await.unwrap();

    manager.admin_suspend(mfr.id).await.unwrap();
    let status = manager.get_status(mfr.id).await.unwrap();
    assert_eq!(status.status, MfrStatus::Suspended);

    manager.admin_reinstate(mfr.id).await.unwrap();
    let status = manager.get_status(mfr.id).await.unwrap();
    assert_eq!(status.status, MfrStatus::Active);
}

#[tokio::test]
async fn test_manager_admin_delete() {
    let pool = setup_test_pool().await;
    let manager = MfrManager::new(pool);
    let (mfr, _) = manager
        .apply("deleteco.com", None)
        .await
        .unwrap();
    let id = mfr.id;

    manager.admin_delete(id).await.unwrap();
    let result = manager.get_status(id).await;
    assert!(matches!(result, Err(MfrError::NotFound)));
}

#[tokio::test]
async fn test_manager_publish_invalid_signature() {
    use base64::Engine as _;

    let pool = setup_test_pool().await;
    let manager = MfrManager::new(pool);
    let (mfr, _) = manager.apply("sigco.com", None).await.unwrap();
    manager.admin_approve(mfr.id).await.unwrap();

    let bad_sig = base64::engine::general_purpose::STANDARD.encode([0u8; 64]);
    let result = manager
        .publish_package(PublishRequest {
            manufacturer: "com.sigco".to_string(),
            name: "svc".to_string(),
            version: "v1".to_string(),
            manifest: "manifest content".to_string(),
            signature: bad_sig,
        })
        .await;
    assert!(
        matches!(result, Err(MfrError::InvalidSignature) | Err(MfrError::Crypto(_))),
        "invalid signature should be rejected: {result:?}"
    );
}

#[tokio::test]
async fn test_manager_publish_valid_signature() {
    use base64::Engine as _;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    let pool = setup_test_pool().await;

    // 使用已知密钥对，绕过 DNS 验证直接激活 MFR
    let signing_key = SigningKey::generate(&mut OsRng);
    let pub_b64 = base64::engine::general_purpose::STANDARD
        .encode(signing_key.verifying_key().to_bytes());

    let mut mfr = Manufacturer::create(&pool, "validpub", "validpub.com", None)
        .await
        .unwrap();
    mfr.activate(&pool, pub_b64).await.unwrap();

    let manifest = "type = \"validpub:client:v1\"\nbinary_hash = \"sha256:abc\"";
    let sig = signing_key.sign(manifest.as_bytes());
    let sig_b64 =
        base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());

    let manager = MfrManager::new(pool);
    let pkg = manager
        .publish_package(PublishRequest {
            manufacturer: "validpub".to_string(),
            name: "client".to_string(),
            version: "v1".to_string(),
            manifest: manifest.to_string(),
            signature: sig_b64,
        })
        .await
        .unwrap();

    assert_eq!(pkg.type_str, "validpub:client:v1");
    assert_eq!(pkg.status, PkgStatus::Active);
}

#[tokio::test]
async fn test_manager_publish_inactive_mfr() {
    let pool = setup_test_pool().await;
    Manufacturer::create(&pool, "pendingmfr", "pendingmfr.com", None)
        .await
        .unwrap();

    let manager = MfrManager::new(pool);
    let result = manager
        .publish_package(PublishRequest {
            manufacturer: "pendingmfr".to_string(),
            name: "svc".to_string(),
            version: "v1".to_string(),
            manifest: "m".to_string(),
            signature: "s".to_string(),
        })
        .await;
    assert!(
        matches!(result, Err(MfrError::InvalidStatus(_))),
        "publishing for pending MFR should fail with InvalidStatus"
    );
}

#[tokio::test]
async fn test_manager_resolve_by_name() {
    let pool = setup_test_pool().await;
    let manager = MfrManager::new(pool);
    let (mfr, _) = manager
        .apply("resolveco.com", None)
        .await
        .unwrap();
    manager.admin_approve(mfr.id).await.unwrap();

    let info = manager.resolve_by_name("com.resolveco").await.unwrap();
    assert_eq!(info.name, "com.resolveco");
    assert_eq!(info.domain, "resolveco.com");
    assert!(!info.public_key.is_empty());
}

#[tokio::test]
async fn test_manager_resolve_by_name_not_active() {
    let pool = setup_test_pool().await;
    let manager = MfrManager::new(pool);
    manager
        .apply("pendingres.com", None)
        .await
        .unwrap();

    let result = manager.resolve_by_name("com.pendingres").await;
    assert!(matches!(result, Err(MfrError::InvalidStatus(_))));
}

#[tokio::test]
async fn test_manager_get_and_revoke_package() {
    use base64::Engine as _;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    let pool = setup_test_pool().await;
    let key = SigningKey::generate(&mut OsRng);
    let pub_b64 =
        base64::engine::general_purpose::STANDARD.encode(key.verifying_key().to_bytes());

    let mut mfr = Manufacturer::create(&pool, "revmgr", "revmgr.com", None)
        .await
        .unwrap();
    mfr.activate(&pool, pub_b64).await.unwrap();

    let manifest = "type = \"revmgr:svc:v1\"";
    let sig = key.sign(manifest.as_bytes());
    let sig_b64 =
        base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());

    let manager = MfrManager::new(pool);
    let pkg = manager
        .publish_package(PublishRequest {
            manufacturer: "revmgr".to_string(),
            name: "svc".to_string(),
            version: "v1".to_string(),
            manifest: manifest.to_string(),
            signature: sig_b64,
        })
        .await
        .unwrap();

    let found = manager.get_package("revmgr:svc:v1").await.unwrap();
    assert_eq!(found.id, pkg.id);

    manager.revoke_package(pkg.id).await.unwrap();

    let result = manager.get_package("revmgr:svc:v1").await;
    assert!(matches!(result, Err(MfrError::NotFound)));
}

#[tokio::test]
async fn test_manager_admin_list() {
    let pool = setup_test_pool().await;
    let manager = MfrManager::new(pool);

    manager
        .apply("adminlist1.com", None)
        .await
        .unwrap();
    let (mfr2, _) = manager
        .apply("adminlist2.com", None)
        .await
        .unwrap();
    manager.admin_approve(mfr2.id).await.unwrap();

    let all = manager.admin_list(None).await.unwrap();
    assert_eq!(all.len(), 2);

    let active = manager.admin_list(Some(MfrStatus::Active)).await.unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].name, "com.adminlist2");
}

#[tokio::test]
async fn test_manager_list_packages_by_mfr() {
    use base64::Engine as _;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    let pool = setup_test_pool().await;
    let key = SigningKey::generate(&mut OsRng);
    let pub_b64 =
        base64::engine::general_purpose::STANDARD.encode(key.verifying_key().to_bytes());

    let mut mfr = Manufacturer::create(&pool, "listmgr", "listmgr.com", None)
        .await
        .unwrap();
    mfr.activate(&pool, pub_b64).await.unwrap();

    let manager = MfrManager::new(pool);

    for pkg_name in &["alpha", "beta"] {
        let manifest = format!("type = \"listmgr:{pkg_name}:v1\"");
        let sig = key.sign(manifest.as_bytes());
        let sig_b64 =
            base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());
        manager
            .publish_package(PublishRequest {
                manufacturer: "listmgr".to_string(),
                name: pkg_name.to_string(),
                version: "v1".to_string(),
                manifest,
                signature: sig_b64,
            })
            .await
            .unwrap();
    }

    let pkgs = manager.list_packages(Some("listmgr")).await.unwrap();
    assert_eq!(pkgs.len(), 2);

    let all = manager.list_packages(None).await.unwrap();
    assert_eq!(all.len(), 2);
}
