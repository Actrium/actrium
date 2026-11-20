#![cfg(all(target_os = "macos", feature = "guest-storage-sqlcipher"))]

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Barrier;
use std::thread;

use actrium_core::{GuestLocalStorage, GuestLocalStorageError};

const ORIGIN: &str = "encrypted-test-origin";
const KEY: &str = "encrypted-test-item-key";
const VALUE: &str = "encrypted-test-item-value";
const DATABASE_KEY: [u8; 32] = [42; 32];

fn assert_no_plaintext(path: &Path) {
    let bytes = fs::read(path).unwrap();
    assert!(!bytes.starts_with(b"SQLite format 3\0"));
    for secret in [ORIGIN, KEY, VALUE] {
        assert!(
            !bytes
                .windows(secret.len())
                .any(|window| window == secret.as_bytes())
        );
    }
}

#[test]
fn encrypted_database_survives_reopen_and_rejects_other_readers() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("guest.sqlite");
    let store = GuestLocalStorage::new_encrypted(path.display().to_string(), DATABASE_KEY.to_vec())
        .unwrap();
    store
        .set_item(ORIGIN.into(), KEY.into(), VALUE.into())
        .unwrap();
    drop(store);

    assert_no_plaintext(&path);
    let before = fs::read(&path).unwrap();
    let sqlite = Command::new("/usr/bin/sqlite3")
        .arg("-readonly")
        .arg(&path)
        .arg("SELECT value FROM guest_local_storage")
        .output()
        .unwrap();
    assert!(
        !sqlite.status.success(),
        "plain SQLite could read the encrypted database"
    );
    assert!(matches!(
        GuestLocalStorage::new_encrypted(path.display().to_string(), vec![1; 32]),
        Err(GuestLocalStorageError::DatabaseUnlockFailed)
    ));
    assert_eq!(fs::read(&path).unwrap(), before);

    let reopened =
        GuestLocalStorage::new_encrypted(path.display().to_string(), DATABASE_KEY.to_vec())
            .unwrap();
    assert_eq!(
        reopened
            .get_item(ORIGIN.into(), KEY.into())
            .unwrap()
            .as_deref(),
        Some(VALUE)
    );
}

#[test]
fn invalid_key_never_creates_a_database() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("guest.sqlite");
    assert!(matches!(
        GuestLocalStorage::new_encrypted(path.display().to_string(), vec![1; 31]),
        Err(GuestLocalStorageError::DatabaseUnlockFailed)
    ));
    assert!(!path.exists());
}

#[test]
fn existing_plaintext_database_is_preserved() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("guest.sqlite");
    let legacy = GuestLocalStorage::new(path.display().to_string());
    legacy
        .set_item(ORIGIN.into(), KEY.into(), VALUE.into())
        .unwrap();
    drop(legacy);
    let before = fs::read(&path).unwrap();

    assert!(matches!(
        GuestLocalStorage::new_encrypted(path.display().to_string(), DATABASE_KEY.to_vec()),
        Err(GuestLocalStorageError::DatabaseUnlockFailed)
    ));
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn concurrent_initial_open_uses_the_same_database_key() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("guest.sqlite");
    let barrier = Barrier::new(4);
    thread::scope(|scope| {
        for index in 0..4 {
            let barrier = &barrier;
            let path = &path;
            scope.spawn(move || {
                barrier.wait();
                let store = GuestLocalStorage::new_encrypted(
                    path.display().to_string(),
                    DATABASE_KEY.to_vec(),
                )
                .unwrap();
                store
                    .set_item(index.to_string(), KEY.into(), VALUE.into())
                    .unwrap();
            });
        }
    });
    let store = GuestLocalStorage::new_encrypted(path.display().to_string(), DATABASE_KEY.to_vec())
        .unwrap();
    for index in 0..4 {
        assert_eq!(
            store
                .get_item(index.to_string(), KEY.into())
                .unwrap()
                .as_deref(),
            Some(VALUE)
        );
    }
}
