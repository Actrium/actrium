//! The TURN Realm filter follows the Realm database.

use actr_protocol::realm_membership::{RealmInvite, client_key_id, invite_secret_hash};
use platform::realm::{Realm, membership};
use std::time::Duration;
use turn::InactiveRealms;

async fn eventually(inactive: &InactiveRealms, realm: &str, expected: bool) {
    for _ in 0..100 {
        if inactive.contains(realm) == expected {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("{realm} inactive should be {expected}");
}

#[tokio::test]
async fn follows_suspension_resumption_and_retirement() {
    let dir = tempfile::tempdir().expect("temp dir");
    platform::storage::db::set_db_path(dir.path())
        .await
        .expect("init db");
    Realm::create("early@example.com".to_string())
        .await
        .expect("create realm");
    Realm::set_enabled("early@example.com", false)
        .await
        .expect("suspend realm");

    let inactive = InactiveRealms::default();
    let watcher = inactive.watch().await;
    // Loaded at start.
    assert!(inactive.contains("early@example.com"));

    Realm::create("late@example.com".to_string())
        .await
        .expect("create realm");
    assert!(!inactive.contains("late@example.com"));
    // The Realm loaded at start already counts as deactivated.
    tokio::time::timeout(Duration::from_secs(2), inactive.deactivated())
        .await
        .expect("deactivation announced");
    Realm::set_enabled("late@example.com", false)
        .await
        .expect("suspend realm");
    eventually(&inactive, "late@example.com", true).await;
    tokio::time::timeout(Duration::from_secs(2), inactive.deactivated())
        .await
        .expect("suspension announced");
    Realm::set_enabled("late@example.com", true)
        .await
        .expect("resume realm");
    eventually(&inactive, "late@example.com", false).await;
    Realm::retire("late@example.com")
        .await
        .expect("retire realm");
    eventually(&inactive, "late@example.com", true).await;

    // An Actor bound to a Realm client key is disabled once the key is
    // revoked.
    let realm = "keys@example.com";
    Realm::create(realm.to_string())
        .await
        .expect("create realm");
    let invite = membership::issue_owner_invite(realm, membership::BY_OPERATOR)
        .await
        .expect("owner invite");
    let secret = RealmInvite::decode(&invite.invite).unwrap().secret;
    let key = |seed: u8| {
        ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
            .verifying_key()
            .to_bytes()
    };
    let limits = membership::Limits {
        max_members: 10,
        max_clients_per_member: 10,
        max_open_invites: 10,
    };
    membership::join(
        realm,
        &invite_secret_hash(&secret).unwrap(),
        &key(1),
        "",
        limits,
    )
    .await
    .expect("join");
    let owner = membership::authenticate(
        membership::client_key(realm, &client_key_id(&key(1)))
            .await
            .unwrap()
            .unwrap(),
    )
    .await
    .unwrap();
    let client = membership::create_invite(
        &owner,
        None,
        Some(&owner.member_id),
        chrono::Utc::now().timestamp() + 60,
        limits,
    )
    .await
    .unwrap();
    let secret = RealmInvite::decode(&client.invite).unwrap().secret;
    membership::join(
        realm,
        &invite_secret_hash(&secret).unwrap(),
        &key(2),
        "",
        limits,
    )
    .await
    .expect("second client");
    sqlx::query(
        "INSERT INTO issued_actor_identity
            (realm, serial_number, actor_type, member_id, client_key_id, issued_at)
         VALUES (?, 5, 'acme.com/notes@1', ?, ?, 1)",
    )
    .bind(realm)
    .bind(&owner.member_id)
    .bind(&owner.key_id)
    .execute(platform::storage::db::get_database().get_pool())
    .await
    .unwrap();
    let actor = format!("5.{realm}");
    assert!(!inactive.contains_actor(&actor));
    membership::revoke_client(&owner, &owner.key_id)
        .await
        .expect("revoke");
    for _ in 0..100 {
        if inactive.contains_actor(&actor) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(inactive.contains_actor(&actor));
    tokio::time::timeout(Duration::from_secs(2), inactive.deactivated())
        .await
        .expect("disabled Actor announced");

    drop(watcher);
}
