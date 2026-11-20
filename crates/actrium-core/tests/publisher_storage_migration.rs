#[path = "support/publisher_storage.rs"]
mod support;
use actr_protocol::KeyStatus::Active;
use actrium_core::{GuestLocalStorage, GuestLocalStorageError};
use serde_json::json;
use support::{accepted, adopted, key, origin, profile, resolved, stored, unqualified};

fn accept(store: &GuestLocalStorage) -> String {
    let root = key(31);
    accepted(
        store,
        profile(
            1,
            ("root", &root),
            &[("root", &root, Active)],
            &[(&key(32), Active), (&key(33), Active)],
            json!([]),
        ),
    )
}

#[test]
fn legacy_data_requires_consent_and_copy_is_idempotent_across_connections() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("storage.sqlite");
    let path = path.to_str().unwrap().to_owned();
    let store = GuestLocalStorage::new(path.clone());
    let first = origin(&key(32), "counter");
    let second = origin(&key(33), "counter");
    store
        .set_item(first.clone(), "theme".into(), "dark".into())
        .unwrap();
    store
        .set_item(second.clone(), "language".into(), "en".into())
        .unwrap();
    let identity = accept(&store);
    assert!(matches!(
        store.resolve_signed_origin(second.clone(), Some(identity.clone()), None),
        Err(GuestLocalStorageError::MigrationRequired)
    ));
    assert_eq!(stored(&store, second.clone(), "theme"), None);
    let canonical = adopted(&store, second.clone(), &identity).unwrap();
    let other = GuestLocalStorage::new(path);
    assert_eq!(unqualified(&other, first.clone()), first);
    assert_eq!(
        stored(&other, canonical.clone(), "theme").as_deref(),
        Some("dark")
    );
    other
        .set_item(first.clone(), "theme".into(), "light".into())
        .unwrap();
    assert_eq!(
        stored(&store, canonical.clone(), "theme").as_deref(),
        Some("dark")
    );
    assert_eq!(adopted(&store, second, &identity).unwrap(), canonical);
    store.remove_item(first, "language".into()).unwrap();
    assert_eq!(stored(&other, canonical, "language").as_deref(), Some("en"));
}

#[test]
fn conflicting_values_abort_the_whole_migration() {
    let store = GuestLocalStorage::new(":memory:".into());
    let first = origin(&key(32), "counter");
    let second = origin(&key(33), "counter");
    store
        .set_item(first.clone(), "theme".into(), "dark".into())
        .unwrap();
    store
        .set_item(second.clone(), "theme".into(), "light".into())
        .unwrap();
    let identity = accept(&store);
    assert!(matches!(
        adopted(&store, first.clone(), &identity),
        Err(GuestLocalStorageError::MigrationConflict(_))
    ));
    assert_eq!(stored(&store, first, "theme").as_deref(), Some("dark"));
    assert_eq!(stored(&store, second, "theme").as_deref(), Some("light"));
}

#[test]
fn quota_conflicts_leave_both_sources_unchanged() {
    let store = GuestLocalStorage::new(":memory:".into());
    let first = origin(&key(32), "counter");
    let second = origin(&key(33), "counter");
    for index in 0..18 {
        store
            .set_item(
                if index < 9 {
                    first.clone()
                } else {
                    second.clone()
                },
                index.to_string(),
                "x".repeat(64 * 1024),
            )
            .unwrap();
    }
    let identity = accept(&store);
    assert!(matches!(
        adopted(&store, first.clone(), &identity),
        Err(GuestLocalStorageError::QuotaExceeded)
    ));
    assert_eq!(stored(&store, first, "0").unwrap().len(), 64 * 1024);
    assert_eq!(stored(&store, second, "17").unwrap().len(), 64 * 1024);
}

#[test]
fn equal_values_deduplicate_but_other_programs_and_signers_stay_isolated() {
    let store = GuestLocalStorage::new(":memory:".into());
    let first = origin(&key(32), "counter");
    let second = origin(&key(33), "counter");
    let independent = origin(&key(34), "counter");
    for signed in [&first, &second, &independent] {
        store
            .set_item(signed.clone(), "theme".into(), "dark".into())
            .unwrap();
    }
    let identity = accept(&store);
    let canonical = adopted(&store, first.clone(), &identity).unwrap();
    assert_eq!(unqualified(&store, independent.clone()), independent);
    let another = resolved(&store, origin(&key(33), "other"), &identity);
    assert_ne!(canonical, another);
    assert_eq!(stored(&store, another, "theme"), None);
    store.remove_item(second.clone(), "theme".into()).unwrap();
    assert_eq!(stored(&store, second, "theme"), None);
    assert_eq!(stored(&store, first, "theme").as_deref(), Some("dark"));
    assert_eq!(stored(&store, canonical, "theme").as_deref(), Some("dark"));
    assert_eq!(
        stored(&store, independent, "theme").as_deref(),
        Some("dark")
    );
}

#[test]
fn approval_cannot_expand_to_sources_added_after_the_prompt() {
    let store = GuestLocalStorage::new(":memory:".into());
    let first = origin(&key(32), "counter");
    let current = origin(&key(33), "counter");
    let added = origin(&key(34), "counter");
    store
        .set_item(first.clone(), "theme".into(), "dark".into())
        .unwrap();
    let identity = accept(&store);
    let reviewed = store
        .storage_migration_token(current.clone(), identity.clone())
        .unwrap();
    store
        .set_item(added.clone(), "language".into(), "en".into())
        .unwrap();
    accepted(
        &store,
        profile(
            2,
            ("root", &key(31)),
            &[("root", &key(31), Active)],
            &[(&key(33), Active), (&key(34), Active)],
            json!([]),
        ),
    );
    assert!(matches!(
        store.resolve_signed_origin(current.clone(), Some(identity.clone()), reviewed),
        Err(GuestLocalStorageError::MigrationRequired)
    ));
    assert_eq!(stored(&store, first, "theme").as_deref(), Some("dark"));
    assert_eq!(stored(&store, added, "language").as_deref(), Some("en"));
    let canonical = adopted(&store, current, &identity).unwrap();
    assert_eq!(
        stored(&store, canonical.clone(), "theme").as_deref(),
        Some("dark")
    );
    assert_eq!(stored(&store, canonical, "language").as_deref(), Some("en"));
}

#[test]
fn choosing_separate_storage_never_adopts_independent_data() {
    let store = GuestLocalStorage::new(":memory:".into());
    let raw = origin(&key(32), "counter");
    store
        .set_item(raw.clone(), "theme".into(), "private".into())
        .unwrap();
    let identity = accept(&store);
    let separate = store
        .keep_storage_separate(raw.clone(), identity.clone())
        .unwrap();
    assert_eq!(stored(&store, separate.clone(), "theme"), None);
    assert_eq!(unqualified(&store, raw.clone()), raw);
    assert_eq!(
        resolved(&store, origin(&key(33), "counter"), &identity),
        separate
    );
    assert_eq!(stored(&store, raw, "theme").as_deref(), Some("private"));
}

#[test]
fn an_explicit_inflight_copy_is_not_silently_dropped_by_another_tab() {
    let store = GuestLocalStorage::new(":memory:".into());
    let raw = origin(&key(32), "counter");
    let current = origin(&key(33), "counter");
    store.set_item(raw, "theme".into(), "dark".into()).unwrap();
    let identity = accept(&store);
    let approval = store
        .storage_migration_token(current.clone(), identity.clone())
        .unwrap();
    let canonical = store
        .keep_storage_separate(current.clone(), identity.clone())
        .unwrap();
    assert_eq!(
        store
            .resolve_signed_origin(current, Some(identity), approval)
            .unwrap(),
        canonical
    );
    assert_eq!(stored(&store, canonical, "theme").as_deref(), Some("dark"));
}

#[test]
fn a_one_time_copy_never_grants_the_old_guest_access_to_future_publisher_data() {
    let store = GuestLocalStorage::new(":memory:".into());
    let raw = origin(&key(32), "counter");
    store
        .set_item(raw.clone(), "theme".into(), "dark".into())
        .unwrap();
    let identity = accept(&store);
    let canonical = adopted(&store, origin(&key(33), "counter"), &identity).unwrap();
    assert_eq!(unqualified(&store, raw.clone()), raw);
    assert_eq!(
        stored(&store, raw.clone(), "theme").as_deref(),
        Some("dark")
    );
    store
        .set_item(
            canonical.clone(),
            "new-secret".into(),
            "publisher-only".into(),
        )
        .unwrap();
    assert_eq!(stored(&store, raw.clone(), "new-secret"), None);
    store
        .set_item(raw.clone(), "theme".into(), "local-only".into())
        .unwrap();
    store.remove_item(raw.clone(), "new-secret".into()).unwrap();
    assert_eq!(
        stored(&store, canonical.clone(), "theme").as_deref(),
        Some("dark")
    );
    assert_eq!(
        stored(&store, canonical, "new-secret").as_deref(),
        Some("publisher-only")
    );
}

#[test]
fn replaying_an_approved_copy_does_not_import_later_local_changes() {
    let store = GuestLocalStorage::new(":memory:".into());
    let raw = origin(&key(32), "counter");
    let current = origin(&key(33), "counter");
    store
        .set_item(raw.clone(), "theme".into(), "dark".into())
        .unwrap();
    let identity = accept(&store);
    let approval = store
        .storage_migration_token(current.clone(), identity.clone())
        .unwrap();
    let canonical = store
        .resolve_signed_origin(current.clone(), Some(identity.clone()), approval.clone())
        .unwrap();
    store
        .set_item(raw.clone(), "later".into(), "local-only".into())
        .unwrap();
    store
        .resolve_signed_origin(current, Some(identity), approval)
        .unwrap();
    assert_eq!(stored(&store, canonical, "later"), None);
    assert_eq!(stored(&store, raw, "later").as_deref(), Some("local-only"));
}
