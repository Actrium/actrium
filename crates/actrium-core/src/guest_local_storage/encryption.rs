use std::path::Path;

use libsqlite3_sys as ffi;
use rusqlite::{Connection, ErrorCode, OpenFlags, OptionalExtension};
use zeroize::Zeroizing;

use super::GuestLocalStorageError;

pub(super) fn open(path: &Path, key: Vec<u8>) -> Result<Connection, GuestLocalStorageError> {
    let key = Zeroizing::new(key);
    if key.len() != 32 {
        return Err(GuestLocalStorageError::DatabaseUnlockFailed);
    }
    // Resolve directory aliases such as macOS /var, while leaving the database file itself untrusted.
    let filename = path.file_name().ok_or_else(|| {
        GuestLocalStorageError::DatabaseFailure("missing local storage filename".into())
    })?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let directory = parent.canonicalize().map_err(|error| {
        GuestLocalStorageError::DatabaseFailure(format!(
            "could not resolve local storage directory: {error}"
        ))
    })?;
    let flags =
        (OpenFlags::default() - OpenFlags::SQLITE_OPEN_URI) | OpenFlags::SQLITE_OPEN_NOFOLLOW;
    let connection = Connection::open_with_flags(directory.join(filename), flags)?;
    apply_key(&connection, &key)?;
    let version = connection
        .pragma_query_value(None, "cipher_version", |row| row.get::<_, String>(0))
        .optional()?;
    if version.is_none() {
        return Err(GuestLocalStorageError::EncryptionUnavailable);
    }
    connection
        .query_row("SELECT count(*) FROM sqlite_schema", [], |_| Ok(()))
        .map_err(|error| match error.sqlite_error_code() {
            Some(ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt) => {
                GuestLocalStorageError::DatabaseUnlockFailed
            }
            _ => error.into(),
        })?;
    connection.pragma_update(None, "temp_store", "MEMORY")?;
    Ok(connection)
}

fn apply_key(connection: &Connection, key: &[u8]) -> Result<(), GuestLocalStorageError> {
    // SQLCipher recognizes the x'...' form as raw key bytes. Keep it out of SQL text.
    let hex = Zeroizing::new(hex::encode(key));
    let mut raw_key = Zeroizing::new([b'\''; 67]);
    raw_key[0] = b'x';
    raw_key[2..66].copy_from_slice(hex.as_bytes());
    // SAFETY: The database handle is open and the key buffer covers all 67 bytes.
    // SQLCipher copies the key before returning.
    let status = unsafe { ffi::sqlite3_key(connection.handle(), raw_key.as_ptr().cast(), 67) };
    if status != ffi::SQLITE_OK {
        return Err(GuestLocalStorageError::DatabaseUnlockFailed);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::{GuestLocalStorageError, open};

    const MARKER: &str = "plaintext guest value used to check database sidecars";

    fn assert_encrypted(path: &Path) {
        let bytes = fs::read(path).unwrap();
        assert!(bytes.len() > 4096, "the file must include a data page");
        assert!(
            !bytes
                .windows(MARKER.len())
                .any(|part| part == MARKER.as_bytes())
        );
    }

    #[test]
    fn journal_and_wal_data_pages_are_encrypted() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("guest.sqlite");
        let connection = open(&path, vec![42; 32]).unwrap();
        assert_eq!(
            connection
                .pragma_query_value(None, "temp_store", |row| row.get::<_, i32>(0))
                .unwrap(),
            2
        );
        connection
            .execute_batch("CREATE TABLE records (value TEXT);")
            .unwrap();
        connection
            .execute("INSERT INTO records VALUES (?1)", [MARKER])
            .unwrap();
        connection
            .execute_batch("BEGIN IMMEDIATE; UPDATE records SET value = 'replacement';")
            .unwrap();
        assert_encrypted(&path.with_extension("sqlite-journal"));
        connection.execute_batch("ROLLBACK;").unwrap();

        let journal_mode: String = connection
            .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journal_mode, "wal");
        connection
            .execute("INSERT INTO records VALUES (?1)", [MARKER])
            .unwrap();
        let wal = path.with_extension("sqlite-wal");
        assert_encrypted(&wal);
        let before = fs::read(&wal).unwrap();
        assert!(matches!(
            open(&path, vec![1; 32]),
            Err(GuestLocalStorageError::DatabaseUnlockFailed)
        ));
        assert_eq!(fs::read(&wal).unwrap(), before);
        let reopened = open(&path, vec![42; 32]).unwrap();
        let count: i32 = reopened
            .query_row("SELECT count(*) FROM records", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }
}
