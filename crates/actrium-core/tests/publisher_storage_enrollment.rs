#[path = "support/publisher_storage.rs"]
mod support;

use actr_protocol::{AuthorityDescriptor, KeyPinOutcome, KeyStatus::Active};
use actrium_core::{GuestLocalStorage, GuestLocalStorageError};
use chrono::DateTime;
use serde_json::{Value, json};
use support::{key, origin, profile, stored};

fn document(sequence: u64, package: u8) -> String {
    profile(
        sequence,
        ("root", &key(1)),
        &[("root", &key(1), Active)],
        &[(&key(package), Active)],
        json!([]),
    )
}

fn pins(body: &str) -> String {
    let descriptor: AuthorityDescriptor = serde_json::from_str(body).unwrap();
    let (_, outcome) = descriptor
        .publisher
        .unwrap()
        .accept(
            "publisher.example",
            DateTime::parse_from_rfc3339("2020-01-02T00:00:00Z")
                .unwrap()
                .into(),
            None,
        )
        .unwrap();
    let KeyPinOutcome::Accepted(pins) = outcome else {
        panic!("fixture was not accepted")
    };
    serde_json::to_string(&pins).unwrap()
}

#[test]
fn first_enrollment_finds_data_signed_by_a_key_removed_from_the_current_profile() {
    let store = GuestLocalStorage::new(":memory:".into());
    let old = document(1, 3);
    let old_origin = origin(&key(3), "counter");
    let current = origin(&key(4), "counter");
    store
        .set_item(old_origin.clone(), "theme".into(), "dark".into())
        .unwrap();
    let accepted = store
        .accept_publisher_profile(
            document(2, 4),
            "publisher.example".into(),
            Some(pins(&old)),
            Some(old),
        )
        .unwrap();
    let identity = accepted.profile.storage_identity.unwrap();
    assert!(matches!(
        store.resolve_signed_origin(current.clone(), Some(identity.clone()), None),
        Err(GuestLocalStorageError::MigrationRequired)
    ));
    let token = store
        .storage_migration_token(current.clone(), identity.clone())
        .unwrap();
    assert!(token.is_some());
    let canonical = store
        .resolve_signed_origin(current, Some(identity.clone()), token)
        .unwrap();
    assert_eq!(stored(&store, canonical, "theme").as_deref(), Some("dark"));
    assert_eq!(
        stored(&store, old_origin.clone(), "theme").as_deref(),
        Some("dark")
    );
    assert!(
        store
            .resolve_signed_origin(old_origin, Some(identity), None)
            .is_err()
    );
}

#[test]
fn expired_but_pinned_history_can_locate_data_without_becoming_current_trust() {
    let mut old: Value = serde_json::from_str(&document(1, 3)).unwrap();
    old["publisher"]["profile_expires_at"] = json!("2021-01-01T00:00:00Z");
    actr_protocol::realm_profile::sign_document(&mut old["publisher"], "root", &key(1));
    let old = old.to_string();
    let store = GuestLocalStorage::new(":memory:".into());
    store
        .set_item(origin(&key(3), "counter"), "theme".into(), "dark".into())
        .unwrap();
    let accepted = store
        .accept_publisher_profile(
            document(2, 4),
            "publisher.example".into(),
            Some(pins(&old)),
            Some(old),
        )
        .unwrap();
    assert_eq!(accepted.profile.profile_sequence, 2);
    assert!(
        store
            .storage_migration_token(
                origin(&key(4), "counter"),
                accepted.profile.storage_identity.unwrap()
            )
            .unwrap()
            .is_some()
    );
}

#[test]
fn an_unpinned_or_different_signed_snapshot_cannot_supply_package_history() {
    let old = document(1, 3);
    for previous_pins in [None, Some(pins(&document(1, 5)))] {
        let store = GuestLocalStorage::new(":memory:".into());
        assert!(
            store
                .accept_publisher_profile(
                    document(2, 4),
                    "publisher.example".into(),
                    previous_pins,
                    Some(old.clone())
                )
                .is_err()
        );
    }
}

#[test]
fn a_tampered_snapshot_cannot_supply_package_history() {
    let old = document(1, 3);
    let mut tampered: Value = serde_json::from_str(&old).unwrap();
    tampered["publisher"]["keys"] =
        serde_json::from_str::<Value>(&document(1, 5)).unwrap()["publisher"]["keys"].clone();
    let store = GuestLocalStorage::new(":memory:".into());
    assert!(
        store
            .accept_publisher_profile(
                document(2, 4),
                "publisher.example".into(),
                Some(pins(&old)),
                Some(tampered.to_string())
            )
            .is_err()
    );
}
