#[path = "support/publisher_storage.rs"]
mod support;

use actr_protocol::{
    KeyRotation,
    KeyStatus::{Active, Retired},
    rotation_payload,
};
use actrium_core::GuestLocalStorage;
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::Signer;
use serde_json::json;
use support::{accepted, key, origin, profile, public, resolved, stored};

fn profiles() -> (String, String, String) {
    let (first, next, package, next_package) = (key(1), key(2), key(3), key(4));
    let mut rotation = KeyRotation {
        from: "first".into(),
        to: "next".into(),
        sequence: 2,
        signature: String::new(),
    };
    rotation.signature = BASE64.encode(
        first
            .sign(rotation_payload(&rotation, &public(&next)).as_bytes())
            .to_bytes(),
    );
    let initial = profile(
        1,
        ("first", &first),
        &[("first", &first, Active)],
        &[(&package, Active)],
        json!([]),
    );
    let rotated = profile(
        2,
        ("next", &next),
        &[("next", &next, Active), ("first", &first, Retired)],
        &[(&package, Retired), (&next_package, Active)],
        json!([rotation]),
    );
    let removed = profile(
        3,
        ("next", &next),
        &[("next", &next, Active)],
        &[(&next_package, Active)],
        json!([]),
    );
    (initial, rotated, removed)
}

#[test]
fn reopening_after_a_database_commit_ignores_the_stale_preference_anchor() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory
        .path()
        .join("storage.sqlite")
        .to_str()
        .unwrap()
        .to_owned();
    let (initial, rotated, removed) = profiles();
    let old_pins = actrium_core::accept_realm_package_publisher(
        initial.clone(),
        "publisher.example".into(),
        None,
    )
    .unwrap()
    .pins_json;
    let identity;
    {
        let database = GuestLocalStorage::new(path.clone());
        identity = accepted(&database, initial.clone());
        let canonical = resolved(&database, origin(&key(3), "counter"), &identity);
        database
            .set_item(canonical, "theme".into(), "dark".into())
            .unwrap();
        assert_eq!(accepted(&database, rotated), identity);
        // Simulate process exit before the Host mirrors the committed pins to preferences.
    }
    let reopened = GuestLocalStorage::new(path);
    let current = reopened
        .accept_publisher_profile(
            removed,
            "publisher.example".into(),
            Some(old_pins),
            Some(initial),
        )
        .unwrap();
    assert_eq!(
        current.profile.storage_identity.as_deref(),
        Some(identity.as_str())
    );
    let canonical = resolved(&reopened, origin(&key(4), "counter"), &identity);
    assert_eq!(
        stored(&reopened, canonical, "theme").as_deref(),
        Some("dark")
    );
}

#[test]
fn registered_trust_does_not_parse_obsolete_or_corrupt_bootstrap_input() {
    let database = GuestLocalStorage::new(":memory:".into());
    let (initial, rotated, removed) = profiles();
    let identity = accepted(&database, initial);
    accepted(&database, rotated);
    let current = database
        .accept_publisher_profile(
            removed,
            "publisher.example".into(),
            Some("broken pins".into()),
            Some("broken profile".into()),
        )
        .unwrap();
    assert_eq!(
        current.profile.storage_identity.as_deref(),
        Some(identity.as_str())
    );
}

#[test]
fn a_preference_anchor_cannot_replace_registered_database_trust() {
    let database = GuestLocalStorage::new(":memory:".into());
    let (initial, _, _) = profiles();
    accepted(&database, initial);
    let pins = database
        .accepted_publisher_pins("publisher.example".into())
        .unwrap();
    let unrelated = profile(
        4,
        ("stranger", &key(8)),
        &[("stranger", &key(8), Active)],
        &[(&key(9), Active)],
        json!([]),
    );
    let cache = actrium_core::accept_realm_package_publisher(
        unrelated.clone(),
        "publisher.example".into(),
        None,
    )
    .unwrap();
    assert!(
        database
            .accept_publisher_profile(
                unrelated.clone(),
                "publisher.example".into(),
                Some(cache.pins_json),
                Some(unrelated)
            )
            .is_err()
    );
    assert_eq!(
        database
            .accepted_publisher_pins("publisher.example".into())
            .unwrap(),
        pins
    );
}

#[test]
fn reading_database_pins_verifies_the_stored_profile_before_using_it() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory
        .path()
        .join("storage.sqlite")
        .to_str()
        .unwrap()
        .to_owned();
    let database = GuestLocalStorage::new(path.clone());
    assert_eq!(
        database
            .accepted_publisher_pins("publisher.example".into())
            .unwrap(),
        None
    );
    let (initial, _, _) = profiles();
    accepted(&database, initial);
    assert!(
        database
            .accepted_publisher_pins("publisher.example".into())
            .unwrap()
            .is_some()
    );
    rusqlite::Connection::open(path)
        .unwrap()
        .execute("UPDATE guest_publishers SET pins = '{}'", [])
        .unwrap();
    assert!(matches!(
        database.accepted_publisher_pins("publisher.example".into()),
        Err(actrium_core::GuestLocalStorageError::DatabaseFailure(_))
    ));
}
