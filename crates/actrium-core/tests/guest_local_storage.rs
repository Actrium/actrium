use actrium_core::{GuestLocalStorage, GuestLocalStorageError};
use rusqlite::{Connection, params};

#[test]
fn opens_the_existing_swift_database_in_place() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("guest-local-storage.sqlite");
    let origin = "signed:publisher:example.org/counter";
    let old = Connection::open(&path).unwrap();
    old.execute_batch(
        "CREATE TABLE guest_local_storage (
            origin TEXT NOT NULL, key TEXT NOT NULL, value TEXT NOT NULL, size INTEGER NOT NULL,
            PRIMARY KEY (origin, key)
        ) WITHOUT ROWID",
    )
    .unwrap();
    old.execute(
        "INSERT INTO guest_local_storage (origin, key, value, size) VALUES (?1, ?2, ?3, ?4)",
        params![origin, "\0key", "雪\0🌲", "\0key".len() + "雪\0🌲".len()],
    )
    .unwrap();
    drop(old);

    let path = path.to_str().unwrap();
    let store = GuestLocalStorage::new(path.into());
    assert_eq!(
        store
            .get_item(origin.into(), "\0key".into())
            .unwrap()
            .as_deref(),
        Some("雪\0🌲")
    );
    assert_eq!(
        store.get_item("dev:other".into(), "\0key".into()).unwrap(),
        None
    );
    store
        .set_item(origin.into(), "\0key".into(), "".into())
        .unwrap();
    drop(store);

    let reopened = GuestLocalStorage::new(path.into());
    assert_eq!(
        reopened
            .get_item(origin.into(), "\0key".into())
            .unwrap()
            .as_deref(),
        Some("")
    );
    reopened.remove_item(origin.into(), "\0key".into()).unwrap();
    reopened.remove_item(origin.into(), "\0key".into()).unwrap();
    assert_eq!(
        reopened.get_item(origin.into(), "\0key".into()).unwrap(),
        None
    );
}

#[test]
fn rejects_over_quota_overwrites_without_losing_the_old_value() {
    let store = GuestLocalStorage::new(":memory:".into());
    let full_value = "x".repeat(64 * 1024);
    for index in 0..15 {
        store
            .set_item("one".into(), index.to_string(), full_value.clone())
            .unwrap();
    }
    store
        .set_item("one".into(), "tail".into(), "original".into())
        .unwrap();
    assert!(matches!(
        store.set_item("one".into(), "tail".into(), full_value.clone()),
        Err(GuestLocalStorageError::QuotaExceeded)
    ));
    assert_eq!(
        store
            .get_item("one".into(), "tail".into())
            .unwrap()
            .as_deref(),
        Some("original")
    );
    store
        .set_item("two".into(), "tail".into(), full_value)
        .unwrap();
}

#[test]
fn validates_utf8_limits_and_propagates_database_failures() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory
        .path()
        .join("missing-directory")
        .join("storage.sqlite");
    let store = GuestLocalStorage::new(path.to_str().unwrap().into());
    assert!(matches!(
        store.get_item("one".into(), "雪".repeat(342)),
        Err(GuestLocalStorageError::InvalidKey)
    ));
    assert!(matches!(
        store.set_item("one".into(), "".into(), "💾".repeat(16_385)),
        Err(GuestLocalStorageError::InvalidValue)
    ));
    assert!(matches!(
        store.get_item("one".into(), "".into()),
        Err(GuestLocalStorageError::DatabaseFailure(_))
    ));
}
