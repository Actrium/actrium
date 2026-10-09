use super::{
    DETACHED_READY_POLL_INTERVAL, DetachedRuntimeStartup, short_wid,
    wait_for_detached_runtime_ready,
};
use crate::commands::runtime_state::{RuntimeRecord, RuntimeStateStore};
use chrono::Utc;
use std::process::Command as StdCommand;
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn test_short_wid_handles_short_values() {
    assert_eq!(short_wid("shortwid"), "shortwid");
    assert_eq!(short_wid("1234567890123456"), "123456789012");
}

#[cfg(unix)]
#[tokio::test]
async fn test_wait_for_detached_runtime_ready_returns_ready_when_record_appears() {
    let hyper_dir = TempDir::new().unwrap();
    let store = RuntimeStateStore::new(hyper_dir.path().to_path_buf());
    store.ensure_layout().await.unwrap();

    let wid = "readywid-0000-0000-0000-000000000000".to_string();
    let log_path = hyper_dir.path().join("logs").join("actr-ready.log");
    let config_path = hyper_dir.path().join("actr.toml");
    let writer_store = RuntimeStateStore::new(hyper_dir.path().to_path_buf());
    let writer_wid = wid.clone();
    let writer_log_path = log_path.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let record = RuntimeRecord::new(
            writer_wid,
            "test-actr".to_string(),
            99999,
            config_path,
            writer_log_path,
            Utc::now(),
        );
        writer_store.write_record(&record).await.unwrap();
    });

    let mut child = StdCommand::new("sh")
        .arg("-c")
        .arg("sleep 5")
        .spawn()
        .unwrap();
    let result = wait_for_detached_runtime_ready(
        &store,
        &wid,
        &log_path,
        &mut child,
        Duration::from_secs(1),
        DETACHED_READY_POLL_INTERVAL,
    )
    .await
    .unwrap();

    assert_eq!(result, DetachedRuntimeStartup::Ready);

    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
#[tokio::test]
async fn test_wait_for_detached_runtime_ready_returns_error_when_child_exits() {
    let hyper_dir = TempDir::new().unwrap();
    let store = RuntimeStateStore::new(hyper_dir.path().to_path_buf());
    store.ensure_layout().await.unwrap();

    let log_path = hyper_dir.path().join("logs").join("actr-failed.log");
    let mut child = StdCommand::new("sh")
        .arg("-c")
        .arg("exit 3")
        .spawn()
        .unwrap();

    let error = wait_for_detached_runtime_ready(
        &store,
        "failedwid-0000",
        &log_path,
        &mut child,
        Duration::from_secs(1),
        DETACHED_READY_POLL_INTERVAL,
    )
    .await
    .unwrap_err()
    .to_string();

    assert!(error.contains("Detached child exited before runtime became ready"));
    assert!(error.contains(log_path.to_str().unwrap()));
}

#[cfg(unix)]
#[tokio::test]
async fn test_wait_for_detached_runtime_ready_returns_initializing_on_timeout() {
    let hyper_dir = TempDir::new().unwrap();
    let store = RuntimeStateStore::new(hyper_dir.path().to_path_buf());
    store.ensure_layout().await.unwrap();

    let log_path = hyper_dir.path().join("logs").join("actr-timeout.log");
    let mut child = StdCommand::new("sh")
        .arg("-c")
        .arg("sleep 5")
        .spawn()
        .unwrap();

    let result = wait_for_detached_runtime_ready(
        &store,
        "timeoutwid-0000",
        &log_path,
        &mut child,
        Duration::from_millis(50),
        Duration::from_millis(10),
    )
    .await
    .unwrap();

    assert_eq!(result, DetachedRuntimeStartup::Initializing);

    let _ = child.kill();
    let _ = child.wait();
}
