//! Exercise a Service switch and rollback after migrating a v16 ACL.
use actr_protocol::RealmSigningKeys;
use actr_protocol::realm_profile::{FrontUpdate, RealmProfile, sign_document};
use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::SigningKey;
use platform::realm::acl::ActorAcl;
use platform::realm::membership::check_key_on;
use platform::realm::{PackageRelease, RealmError, RealmPackageReleaseRecord, RealmProfileRecord};
use platform::storage::db::{Database, get_database, set_db_path};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const REALM: &str = "echo@realm.example";
const OLD_SERVICE: &str = "publisher.example/echo@0.1.1";
const FRONT: &str = "publisher.example/echo-ui@0.1.1";
const KEY_ID: &str = "realm-test";

#[tokio::test]
async fn upgraded_service_needs_owner_acl_before_it_becomes_a_rollback_target() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let old_db = Database::new(dir.path()).await?;
    let old_pool = old_db.get_pool();
    sqlx::query("ALTER TABLE actoracl DROP COLUMN owner_key_id")
        .execute(old_pool)
        .await?;
    sqlx::query("PRAGMA user_version = 16")
        .execute(old_pool)
        .await?;

    let key = SigningKey::generate(&mut rand::rngs::OsRng);
    let public_key = key.verifying_key();
    let mut member = json!({
        "sequence": 1,
        "keys": [{"id": KEY_ID, "algorithm": "Ed25519", "status": "active",
                  "public_key": STANDARD.encode(public_key.as_bytes())}]
    });
    sign_document(&mut member, KEY_ID, &key);
    let (keys, _) = RealmSigningKeys::accept(&member, None)?;
    let mut initial: Value = serde_json::from_slice(include_bytes!(
        "../../../actr/protocols/examples/realm-profile.json"
    ))?;
    initial["sequence"] = 1.into();
    sign_document(&mut initial, KEY_ID, &key);
    let initial_document = initial.to_string();
    let initial_profile = RealmProfile::verify(initial_document.as_bytes(), REALM, &keys)?;
    let baseline_digest = hex::encode(Sha256::digest(initial_document.as_bytes()));
    sqlx::query("INSERT INTO realm (name, created_at) VALUES (?, 1)")
        .bind(REALM)
        .execute(old_pool)
        .await?;
    sqlx::query(
        "INSERT INTO realm_profile (realm, sequence, digest, document, accepted_at)
         VALUES (?, 1, ?, ?, 1)",
    )
    .bind(REALM)
    .bind(&baseline_digest)
    .bind(&initial_document)
    .execute(old_pool)
    .await?;
    sqlx::query(
        "INSERT INTO realm_issuer_key (realm, key_id, signer_name, public_key, state, created_at)
         VALUES (?, 1, 'test-issuer', ?, 'current', 1)",
    )
    .bind(REALM)
    .bind(
        initial["issuer"]["keys"][0]["public_key"]
            .as_str()
            .context("issuer key")?,
    )
    .execute(old_pool)
    .await?;
    sqlx::query(
        "INSERT INTO realm_member (realm, member_id, role, status, created_at)
         VALUES (?, 'owner', 'owner', 'active', 1)",
    )
    .bind(REALM)
    .execute(old_pool)
    .await?;
    sqlx::query(
        "INSERT INTO realm_client_key (realm, key_id, public_key, member_id, registered_at)
         VALUES (?, 'owner-key', ?, 'owner', 1)",
    )
    .bind(REALM)
    .bind(public_key.as_bytes().as_slice())
    .execute(old_pool)
    .await?;
    sqlx::query(
        "INSERT INTO actoracl (realm, source_realm, from_type, to_type, access)
         VALUES (?1, ?1, ?2, ?3, 1)",
    )
    .bind(REALM)
    .bind(FRONT)
    .bind(OLD_SERVICE)
    .execute(old_pool)
    .await?;
    old_pool.close().await;

    set_db_path(dir.path()).await?;
    let pool = get_database().get_pool();
    let mut conn = pool.acquire().await?;
    let owner = check_key_on(&mut conn, REALM, "owner-key").await?;
    drop(conn);
    let mut switched = initial.clone();
    switched["sequence"] = 2.into();
    switched["packages"]["echo"]["type"] = "publisher.example/echo-next@0.1.1".into();
    switched["packages"]["ui"]["type"] = "publisher.example/echo-ui@0.1.2".into();
    sign_document(&mut switched, KEY_ID, &key);
    let switched_document = switched.to_string();
    let switched_profile = RealmProfile::verify(switched_document.as_bytes(), REALM, &keys)?;
    let forward = FrontUpdate {
        operation_id: "a".repeat(32),
        expected_sequence: 1,
        rollback: false,
    };
    let release = PackageRelease::Service(&forward);
    assert!(matches!(
        RealmProfileRecord::store_package_release(
            &switched_profile, &switched_document, Some(&baseline_digest), Some(&owner), release,
        ).await,
        Err(RealmError::Conflict(message))
            if message.contains(OLD_SERVICE) && message.contains("needs approval")
    ));
    assert_eq!(
        RealmProfileRecord::get(REALM).await?.unwrap().digest,
        baseline_digest
    );
    assert!(
        RealmPackageReleaseRecord::get(
            REALM,
            &forward,
            switched_profile.selection.as_ref().unwrap(),
        )
        .await?
        .is_none()
    );

    let mut tx = pool.begin().await?;
    let owner = check_key_on(&mut tx, REALM, "owner-key").await?;
    let rules = [ActorAcl::new(
        REALM.into(),
        FRONT.into(),
        OLD_SERVICE.into(),
        true,
    )];
    ActorAcl::replace_registration_policy(&mut tx, OLD_SERVICE, &rules, &owner).await?;
    tx.commit().await?;
    RealmProfileRecord::store_package_release(
        &switched_profile,
        &switched_document,
        Some(&baseline_digest),
        Some(&owner),
        release,
    )
    .await?;

    let current = RealmProfileRecord::get(REALM).await?.unwrap();
    let mut rollback = initial;
    rollback["sequence"] = 3.into();
    sign_document(&mut rollback, KEY_ID, &key);
    let rollback_document = rollback.to_string();
    let rollback_profile = RealmProfile::verify(rollback_document.as_bytes(), REALM, &keys)?;
    let update = FrontUpdate {
        operation_id: "b".repeat(32),
        expected_sequence: 2,
        rollback: true,
    };
    RealmProfileRecord::store_package_release(
        &rollback_profile,
        &rollback_document,
        Some(&current.digest),
        Some(&owner),
        PackageRelease::Service(&update),
    )
    .await?;
    let restored = RealmProfileRecord::get(REALM)
        .await?
        .unwrap()
        .verify(&keys)?;
    assert_eq!(restored.sequence, 3);
    assert_eq!(restored.selection, initial_profile.selection);
    assert!(ActorAcl::can_discover(REALM, REALM, FRONT, OLD_SERVICE).await?);
    Ok(())
}
