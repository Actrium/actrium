//! Exercise a v10 database upgrade and subsequent signed Profile publications.
use actr_protocol::RealmSigningKeys;
use actr_protocol::realm_profile::{RealmProfile, sign_document};
use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::SigningKey;
use platform::realm::{RealmError, RealmProfileRecord};
use platform::storage::db::{Database, set_db_path};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const REALM: &str = "echo@realm.example";
const KEY_ID: &str = "realm-test";

async fn publish(keys: &RealmSigningKeys, key: &SigningKey, mut document: Value) -> Result<()> {
    sign_document(&mut document, KEY_ID, key);
    let document = serde_json::to_string(&document)?;
    let accepted = RealmProfileRecord::get(REALM)
        .await?
        .context("stored Profile")?;
    let candidate = RealmProfile::verify(document.as_bytes(), REALM, keys)?;
    accepted
        .verify_for_update(keys)?
        .check_successor(&candidate)?;
    RealmProfileRecord::store(&candidate, &document, Some(&accepted.digest), None, None).await?;
    Ok(())
}

#[tokio::test]
async fn upgrading_v10_preserves_the_digest_when_a_package_version_returns() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let db = Database::new(dir.path()).await?;
    for sql in [
        "DROP TABLE realm_creation_selection",
        "DROP TABLE realm_package_digest",
        "PRAGMA user_version = 10",
    ] {
        sqlx::query(sql).execute(db.get_pool()).await?;
    }

    let key = SigningKey::generate(&mut rand::rngs::OsRng);
    let mut member = json!({
        "sequence": 1,
        "keys": [{"id": KEY_ID, "algorithm": "Ed25519", "status": "active",
                  "public_key": STANDARD.encode(key.verifying_key().as_bytes())}]
    });
    sign_document(&mut member, KEY_ID, &key);
    let (keys, _) = RealmSigningKeys::accept(&member, None)?;
    let original: Value = serde_json::from_slice(include_bytes!(
        "../../../actr/protocols/examples/realm-profile.json"
    ))?;
    let mut legacy = original.clone();
    let fields = legacy.as_object_mut().unwrap();
    fields.remove("primary_package");
    fields.remove("front_package");
    fields.insert("package".into(), original["packages"]["echo"].clone());
    fields.remove("packages");
    sign_document(&mut legacy, KEY_ID, &key);
    let signed_bytes = serde_json::to_string_pretty(&legacy)?;
    sqlx::query("INSERT INTO realm (name, created_at) VALUES (?, 1)")
        .bind(REALM)
        .execute(db.get_pool())
        .await?;
    sqlx::query("INSERT INTO realm_profile (realm, sequence, digest, document, accepted_at) VALUES (?, 1, ?, ?, 1)")
        .bind(REALM)
        .bind(hex::encode(Sha256::digest(signed_bytes.as_bytes())))
        .bind(&signed_bytes).execute(db.get_pool()).await?;
    db.get_pool().close().await;

    // Upgrade, then reopen v11 before publishing the next signed Profile.
    let upgraded = Database::new(dir.path()).await?;
    upgraded.get_pool().close().await;
    set_db_path(dir.path()).await?;
    let stored = RealmProfileRecord::get(REALM).await?.unwrap();
    assert_eq!(stored.document, signed_bytes);

    let mut next = original.clone();
    next["sequence"] = 2.into();
    next["packages"]["echo"]["type"] = "publisher.example/echo@2.0.0".into();
    next["packages"]["echo"]["sha256"] = "3".repeat(64).into();
    publish(&keys, &key, next).await?;

    let mut returning = original;
    returning["sequence"] = 3.into();
    let mut conflicting = returning.clone();
    conflicting["packages"]["echo"]["sha256"] = "4".repeat(64).into();
    let error = publish(&keys, &key, conflicting).await.unwrap_err();
    assert!(
        matches!(
            error.downcast_ref::<RealmError>(),
            Some(RealmError::Conflict(_))
        ),
        "{error}"
    );
    assert_eq!(RealmProfileRecord::get(REALM).await?.unwrap().sequence, 2);
    publish(&keys, &key, returning).await?;
    Ok(())
}
