use actrium_core::{GuestLocalStorage, GuestLocalStorageError, GuestLocalStorageItem};
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
        params![
            origin,
            "\0key",
            "\u{96ea}\0🌲",
            "\0key".len() + "\u{96ea}\0🌲".len()
        ],
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
        Some("\u{96ea}\0🌲")
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
fn replaces_a_history_that_fits_only_after_removing_the_old_chunks() {
    let store = GuestLocalStorage::new(":memory:".into());
    let history = |prefix: &str, count| {
        let mut entries = (0..count)
            .map(|index| GuestLocalStorageItem {
                key: format!("{prefix}/{index}"),
                value: "x".repeat(64 * 1024),
            })
            .collect::<Vec<_>>();
        entries.push(GuestLocalStorageItem {
            key: "history".into(),
            value: prefix.into(),
        });
        entries
    };
    store
        .set_item("one".into(), "settings".into(), "keep".into())
        .unwrap();
    let old = history("old", 9);
    for GuestLocalStorageItem { key, value } in &old {
        store
            .set_item("one".into(), key.clone(), value.clone())
            .unwrap();
    }
    let next = history("new", 9);
    let remove = old
        .into_iter()
        .map(|item| item.key)
        .filter(|key| key != "history")
        .collect::<Vec<_>>();
    assert!(matches!(
        store.replace_items(
            "one".into(),
            "history".into(),
            Some("different".into()),
            remove.clone(),
            next.clone()
        ),
        Err(GuestLocalStorageError::Conflict)
    ));
    let oversized = history("new", 16);
    assert!(matches!(
        store.replace_items(
            "one".into(),
            "history".into(),
            Some("old".into()),
            remove.clone(),
            oversized
        ),
        Err(GuestLocalStorageError::QuotaExceeded)
    ));
    let get = |key: &str| store.get_item("one".into(), key.into()).unwrap();
    assert_eq!(get("history").as_deref(), Some("old"));
    assert!(get("old/0").is_some());
    assert_eq!(get("new/0"), None);

    store
        .replace_items(
            "one".into(),
            "history".into(),
            Some("old".into()),
            remove,
            next,
        )
        .unwrap();
    assert_eq!(get("history").as_deref(), Some("new"));
    assert_eq!(get("old/0"), None);
    assert!(get("new/0").is_some());
    assert_eq!(get("settings").as_deref(), Some("keep"));
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
        store.get_item("one".into(), "\u{96ea}".repeat(342)),
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
