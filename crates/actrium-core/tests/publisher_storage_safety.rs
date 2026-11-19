#[path = "support/publisher_storage.rs"]
mod support;
use support::{accepted, resolved, stored};

use actr_protocol::KeyStatus::Active;
use actrium_core::{GuestLocalStorage, GuestLocalStorageError};
use rusqlite::Connection;
use serde_json::json;
use support::{key, origin, profile};

fn accept(store: &GuestLocalStorage) -> (String, String) {
    let (root, package) = (key(41), key(42));
    let body = profile(
        1,
        ("root", &root),
        &[("root", &root, Active)],
        &[(&package, Active)],
        json!([]),
    );
    let identity = accepted(store, body.clone());
    (body, identity)
}

#[test]
fn corrupt_trust_is_not_silently_replaced_with_a_fresh_identity() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("storage.sqlite");
    let path = path.to_str().unwrap().to_owned();
    let store = GuestLocalStorage::new(path.clone());
    let (profile, identity) = accept(&store);
    let signed = origin(&key(42), "counter");
    let canonical = resolved(&store, signed.clone(), &identity);
    store
        .set_item(canonical.clone(), "theme".into(), "dark".into())
        .unwrap();
    Connection::open(path)
        .unwrap()
        .execute("UPDATE guest_publishers SET pins = '{}'", [])
        .unwrap();
    assert!(matches!(
        store.resolve_signed_origin(signed, Some(identity.clone()), None),
        Err(GuestLocalStorageError::DatabaseFailure(_))
    ));
    assert!(matches!(
        store.accept_publisher_profile(profile, "publisher.example".into(), None),
        Err(GuestLocalStorageError::DatabaseFailure(_))
    ));
    assert_eq!(stored(&store, canonical, "theme").as_deref(), Some("dark"));
}

#[test]
fn interrupted_migration_rolls_back_copies_and_aliases_before_a_retry() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("storage.sqlite");
    let path = path.to_str().unwrap().to_owned();
    let store = GuestLocalStorage::new(path.clone());
    let signed = origin(&key(42), "counter");
    store
        .set_item(signed.clone(), "theme".into(), "dark".into())
        .unwrap();
    let (_, identity) = accept(&store);
    let external = Connection::open(path).unwrap();
    external.execute_batch("CREATE TRIGGER interrupt_migration BEFORE DELETE ON guest_local_storage BEGIN SELECT RAISE(ABORT, 'interrupted'); END").unwrap();
    assert!(matches!(
        store.resolve_signed_origin(
            signed.clone(),
            Some(identity.clone()),
            store
                .storage_migration_token(signed.clone(), identity.clone())
                .unwrap()
        ),
        Err(GuestLocalStorageError::DatabaseFailure(_))
    ));
    let aliases: i64 = external
        .query_row("SELECT COUNT(*) FROM guest_storage_aliases", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(aliases, 0);
    assert_eq!(
        stored(&store, signed.clone(), "theme").as_deref(),
        Some("dark")
    );
    external
        .execute_batch("DROP TRIGGER interrupt_migration")
        .unwrap();
    let canonical = store
        .resolve_signed_origin(
            signed.clone(),
            Some(identity.clone()),
            store
                .storage_migration_token(signed.clone(), identity.clone())
                .unwrap(),
        )
        .unwrap();
    assert_eq!(stored(&store, canonical, "theme").as_deref(), Some("dark"));
}

#[test]
fn old_origin_writes_on_another_connection_do_not_become_stranded_during_migration() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("storage.sqlite");
    let path = path.to_str().unwrap().to_owned();
    let store = GuestLocalStorage::new(path.clone());
    let signed = origin(&key(42), "counter");
    store
        .set_item(signed.clone(), "theme".into(), "dark".into())
        .unwrap();
    let (_, identity) = accept(&store);
    let thread_origin = signed.clone();
    let writer = std::thread::spawn(move || {
        let other = GuestLocalStorage::new(path);
        for value in 0..20 {
            other
                .set_item(thread_origin.clone(), "theme".into(), value.to_string())
                .unwrap();
        }
    });
    let canonical = store
        .resolve_signed_origin(
            signed.clone(),
            Some(identity.clone()),
            store
                .storage_migration_token(signed.clone(), identity.clone())
                .unwrap(),
        )
        .unwrap();
    writer.join().unwrap();
    assert_eq!(stored(&store, canonical, "theme").as_deref(), Some("19"));
    assert_eq!(stored(&store, signed, "theme").as_deref(), Some("19"));
}
