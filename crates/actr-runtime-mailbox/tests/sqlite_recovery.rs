#![cfg(feature = "sqlite")]

use actr_runtime_mailbox::{
    Mailbox, MessagePriority, MessageStatus, SqliteConfig, SqliteMailbox, StorageError,
};
use rusqlite::Connection;
use std::{
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tempfile::tempdir;

fn config(path: &Path, enable_wal: bool) -> SqliteConfig {
    SqliteConfig {
        database_path: path.to_owned(),
        enable_wal,
    }
}

#[tokio::test]
async fn uri_options_cannot_bypass_ownership() {
    for wal in [false, true] {
        let dir = tempdir().unwrap();
        let path = dir.path().join("mailbox.db");
        let cfg = config(&path, wal);
        let mailbox = SqliteMailbox::with_config(cfg.clone()).await.unwrap();
        let id = mailbox
            .enqueue(vec![1], vec![2], MessagePriority::Normal)
            .await
            .unwrap();
        mailbox.dequeue().await.unwrap();
        for options in [
            "cache=shared",
            "nolock=1",
            "vfs=unix-none",
            "mode=ro&immutable=1",
        ] {
            let uri = format!("file:{}?{options}", path.display());
            assert!(matches!(
                SqliteMailbox::with_config(config(Path::new(&uri), wal)).await,
                Err(StorageError::ConfigError(_))
            ));
        }
        assert_eq!(mailbox.status().await.unwrap().inflight_messages, 1);
        assert!(mailbox.dequeue().await.unwrap().is_empty());
        drop(mailbox);
        let mailbox = SqliteMailbox::with_config(cfg).await.unwrap();
        assert_eq!(mailbox.dequeue().await.unwrap()[0].id, id);
    }

    // Named shared-memory databases also allow multiple connections to share
    // active messages, unlike the supported independent :memory: databases.
    assert!(matches!(
        SqliteMailbox::new("file:mailbox?mode=memory&cache=shared").await,
        Err(StorageError::ConfigError(_))
    ));
}

#[tokio::test]
async fn reopen_preserves_unacked_records_and_queued_messages() {
    for wal in [false, true] {
        let dir = tempdir().unwrap();
        let cfg = config(&dir.path().join("mailbox.db"), wal);
        let mailbox = SqliteMailbox::with_config(cfg.clone()).await.unwrap();
        let mut ids = Vec::new();
        // Exceed the batch size so recovery must preserve both Queued and Inflight.
        for n in 0..40u8 {
            ids.push(
                mailbox
                    .enqueue(vec![n], vec![n, 42], MessagePriority::Normal)
                    .await
                    .unwrap(),
            );
        }
        let batch = mailbox.dequeue().await.unwrap();
        assert_eq!(batch.len(), 32);
        let acked = batch[0].id;
        mailbox.ack(acked).await.unwrap();
        let high = mailbox
            .enqueue(vec![255], vec![255], MessagePriority::High)
            .await
            .unwrap();
        drop(mailbox);

        let mailbox = SqliteMailbox::with_config(cfg.clone()).await.unwrap();
        let stats = mailbox.status().await.unwrap();
        assert_eq!(stats.queued_messages, 40);
        assert_eq!(stats.inflight_messages, 0);
        let mut recovered = mailbox.dequeue().await.unwrap();
        assert_eq!(recovered[0].id, high);
        recovered.extend(mailbox.dequeue().await.unwrap());
        assert_eq!(recovered.len(), 40);
        assert!(mailbox.dequeue().await.unwrap().is_empty());
        assert!(!recovered.iter().any(|m| m.id == acked));
        for original in batch.iter().skip(1) {
            let restored = recovered.iter().find(|m| m.id == original.id).unwrap();
            assert_eq!(restored.from, original.from);
            assert_eq!(restored.payload, original.payload);
            assert_eq!(restored.priority, original.priority);
            assert_eq!(restored.created_at, original.created_at);
            assert_eq!(restored.status, MessageStatus::Inflight);
        }
        for (n, id) in ids.iter().enumerate().filter(|(_, id)| **id != acked) {
            let restored = recovered.iter().find(|m| m.id == *id).unwrap();
            assert_eq!(restored.payload, vec![n as u8, 42]);
        }
        for record in recovered {
            mailbox.ack(record.id).await.unwrap();
        }
        drop(mailbox);
        let mailbox = SqliteMailbox::with_config(cfg).await.unwrap();
        assert!(mailbox.dequeue().await.unwrap().is_empty());
        assert_eq!(mailbox.status().await.unwrap().inflight_messages, 0);
    }
}

#[tokio::test]
async fn ownership_survives_commits_and_failed_open_does_not_requeue() {
    for wal in [false, true] {
        let dir = tempdir().unwrap();
        let path = dir.path().join("mailbox.db");
        let cfg = config(&path, wal);
        let mailbox = std::sync::Arc::new(SqliteMailbox::with_config(cfg.clone()).await.unwrap());
        // Initialization alone must retain ownership, even with an empty DB.
        assert!(matches!(
            SqliteMailbox::with_config(cfg.clone()).await,
            Err(StorageError::ConcurrencyError(_))
        ));
        let id = mailbox
            .enqueue(vec![1], vec![2], MessagePriority::Normal)
            .await
            .unwrap();
        mailbox.dequeue().await.unwrap();
        // A different spelling of the same path must not bypass ownership.
        let alias = config(&dir.path().join(".").join("mailbox.db"), wal);
        assert!(matches!(
            SqliteMailbox::with_config(alias).await,
            Err(StorageError::ConcurrencyError(_))
        ));
        assert_eq!(mailbox.status().await.unwrap().inflight_messages, 1);
        assert!(mailbox.dequeue().await.unwrap().is_empty());

        // Ordinary SQLite readers also cannot bypass the lock.
        let reader = Connection::open(&path).unwrap();
        reader.busy_timeout(Duration::ZERO).unwrap();
        assert!(matches!(
            reader.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get::<_, i64>(0)),
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == rusqlite::ErrorCode::DatabaseBusy
                    || e.code == rusqlite::ErrorCode::DatabaseLocked
        ));
        drop(reader);
        let shared = mailbox.clone();
        drop(mailbox);
        assert!(matches!(
            SqliteMailbox::with_config(cfg.clone()).await,
            Err(StorageError::ConcurrencyError(_))
        ));
        drop(shared);
        let reopened = SqliteMailbox::with_config(cfg).await.unwrap();
        assert_eq!(reopened.dequeue().await.unwrap()[0].id, id);
    }
}

#[tokio::test]
async fn recovery_failure_rolls_back_and_releases_ownership() {
    for wal in [false, true] {
        let dir = tempdir().unwrap();
        let path = dir.path().join("mailbox.db");
        let cfg = config(&path, wal);
        let mailbox = SqliteMailbox::with_config(cfg.clone()).await.unwrap();
        for n in 0..2 {
            mailbox
                .enqueue(vec![n], vec![n], MessagePriority::Normal)
                .await
                .unwrap();
        }
        mailbox.dequeue().await.unwrap();
        drop(mailbox);
        let conn = Connection::open(&path).unwrap();
        // FAIL preserves earlier row updates within the statement, so only
        // transaction rollback can undo the partially completed recovery.
        conn.execute_batch(
            "CREATE TRIGGER fail_recovery BEFORE UPDATE OF status ON messages
             WHEN NEW.status = 0 AND EXISTS (SELECT 1 FROM messages WHERE status = 0)
             BEGIN SELECT RAISE(FAIL, 'injected recovery failure'); END;",
        )
        .unwrap();
        drop(conn);
        assert!(matches!(
            SqliteMailbox::with_config(cfg.clone()).await,
            Err(StorageError::IntegrityError(_))
        ));
        let conn = Connection::open(&path).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM messages WHERE status = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 2);
        conn.execute_batch("DROP TRIGGER fail_recovery").unwrap();
        drop(conn);
        let mailbox = SqliteMailbox::with_config(cfg).await.unwrap();
        assert_eq!(mailbox.status().await.unwrap().queued_messages, 2);
    }
}

#[tokio::test]
async fn legacy_database_recovers_without_schema_migration() {
    for wal in [false, true] {
        let dir = tempdir().unwrap();
        let path = dir.path().join("legacy.db");
        let conn = Connection::open(&path).unwrap();
        if wal {
            conn.execute_batch("PRAGMA journal_mode = WAL").unwrap();
        }
        // Database produced by the previous implementation, with no ownership
        // metadata. Fixed timestamps also verify ordering across recovery.
        conn.execute_batch(
            "CREATE TABLE messages (
                 id TEXT PRIMARY KEY, from_actr_id BLOB NOT NULL,
                 payload BLOB NOT NULL, priority INTEGER NOT NULL,
                 status INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL
             );
             INSERT INTO messages VALUES
                 ('00000000-0000-0000-0000-000000000001', X'01', X'41', 0, 1, '2026-01-01T00:00:00Z'),
                 ('00000000-0000-0000-0000-000000000002', X'01', X'42', 0, 0, '2026-01-01T00:00:01Z');",
        ).unwrap();
        drop(conn);
        let cfg = config(&path, wal);
        // Repeated restarts before ACK must keep the same records recoverable.
        for _ in 0..2 {
            let mailbox = SqliteMailbox::with_config(cfg.clone()).await.unwrap();
            let records = mailbox.dequeue().await.unwrap();
            assert_eq!(records.len(), 2);
            assert_eq!(records[0].payload, b"A");
            assert_eq!(records[1].payload, b"B");
            assert_eq!(
                records[0].id.to_string(),
                "00000000-0000-0000-0000-000000000001"
            );
        }
    }
}

#[tokio::test]
async fn memory_mailboxes_are_independent() {
    let first = SqliteMailbox::new(":memory:").await.unwrap();
    let second = SqliteMailbox::new(":memory:").await.unwrap();
    first
        .enqueue(vec![1], vec![2], MessagePriority::Normal)
        .await
        .unwrap();
    first.dequeue().await.unwrap();
    assert!(second.dequeue().await.unwrap().is_empty());
    assert_eq!(first.status().await.unwrap().inflight_messages, 1);
    drop(first);
    let reopened = SqliteMailbox::new(":memory:").await.unwrap();
    assert!(reopened.dequeue().await.unwrap().is_empty());
}

// Always reap the subprocess, including if a parent assertion fails.
struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn process_shared_cache_default_cannot_bypass_ownership() {
    // Isolate SQLite's global setting from other tests and library users.
    let mut child = ChildGuard(
        Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "shared_cache_child"])
            .env("ACTR_MAILBOX_SHARED_CACHE_CHILD", "1")
            .stdin(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success(), "shared-cache child failed: {status}");
            break;
        }
        assert!(Instant::now() < deadline, "shared-cache child timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "subprocess helper for the process-wide shared-cache setting"]
fn shared_cache_child() {
    if std::env::var_os("ACTR_MAILBOX_SHARED_CACHE_CHILD").is_none() {
        return;
    }
    // SAFETY: This standalone subprocess has no other SQLite users. The
    // setting is made before opening any connections and dies with the child.
    assert_eq!(unsafe { rusqlite::ffi::sqlite3_enable_shared_cache(1) }, 0);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    runtime.block_on(async {
        for wal in [false, true] {
            let dir = tempdir().unwrap();
            let cfg = config(&dir.path().join("mailbox.db"), wal);
            let mailbox = SqliteMailbox::with_config(cfg.clone()).await.unwrap();
            let id = mailbox
                .enqueue(vec![1], vec![2], MessagePriority::Normal)
                .await
                .unwrap();
            mailbox.dequeue().await.unwrap();
            assert!(matches!(
                SqliteMailbox::with_config(cfg.clone()).await,
                Err(StorageError::ConcurrencyError(_))
            ));
            assert_eq!(mailbox.status().await.unwrap().inflight_messages, 1);
            drop(mailbox);
            let mailbox = SqliteMailbox::with_config(cfg).await.unwrap();
            assert_eq!(mailbox.dequeue().await.unwrap()[0].id, id);
        }
    });
}

#[tokio::test]
async fn killed_process_releases_ownership_and_recovers_unacked_batch() {
    for wal in [false, true] {
        let dir = tempdir().unwrap();
        let path = dir.path().join("mailbox.db");
        let ready = dir.path().join("ready");
        let mut child = ChildGuard(
            Command::new(std::env::current_exe().unwrap())
                .args(["--ignored", "--exact", "crash_child"])
                .env("ACTR_MAILBOX_CRASH_DB", &path)
                .env("ACTR_MAILBOX_CRASH_READY", &ready)
                .env("ACTR_MAILBOX_CRASH_WAL", if wal { "1" } else { "0" })
                .stdin(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(30);
        while !ready.exists() {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "child exited before readiness"
            );
            assert!(Instant::now() < deadline, "child readiness timed out");
            std::thread::sleep(Duration::from_millis(10));
        }
        let cfg = config(&path, wal);
        assert!(matches!(
            SqliteMailbox::with_config(cfg.clone()).await,
            Err(StorageError::ConcurrencyError(_))
        ));
        // Child::kill terminates without running Rust destructors.
        child.0.kill().unwrap();
        assert!(!child.0.wait().unwrap().success());
        let mailbox = SqliteMailbox::with_config(cfg).await.unwrap();
        let stats = mailbox.status().await.unwrap();
        assert_eq!(stats.queued_messages, 2);
        assert_eq!(stats.inflight_messages, 0);
        let records = mailbox.dequeue().await.unwrap();
        assert_eq!(records.len(), 2);
        assert!(records.iter().any(|m| m.payload == b"A"));
        assert!(records.iter().any(|m| m.payload == b"B"));
        assert!(!records.iter().any(|m| m.payload == b"acked"));
    }
}

#[test]
#[ignore = "subprocess helper invoked by the crash recovery test"]
fn crash_child() {
    let Some(path) = std::env::var_os("ACTR_MAILBOX_CRASH_DB") else {
        return;
    };
    let ready = std::env::var_os("ACTR_MAILBOX_CRASH_READY").unwrap();
    let wal = std::env::var("ACTR_MAILBOX_CRASH_WAL").unwrap() == "1";
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mailbox = SqliteMailbox::with_config(config(Path::new(&path), wal))
            .await
            .unwrap();
        let acked = mailbox
            .enqueue(vec![1], b"acked".to_vec(), MessagePriority::Normal)
            .await
            .unwrap();
        for payload in [b"A", b"B"] {
            mailbox
                .enqueue(vec![1], payload.to_vec(), MessagePriority::Normal)
                .await
                .unwrap();
        }
        assert_eq!(mailbox.dequeue().await.unwrap().len(), 3);
        mailbox.ack(acked).await.unwrap();
        std::fs::write(ready, b"ready").unwrap();
        loop {
            std::thread::park();
        }
    });
}
