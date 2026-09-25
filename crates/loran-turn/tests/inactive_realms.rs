//! The TURN Realm filter follows the Realm database.

use platform::realm::Realm;
use std::time::Duration;
use turn::InactiveRealms;

async fn eventually(inactive: &InactiveRealms, realm: &str, expected: bool) {
    for _ in 0..100 {
        if inactive.contains(realm) == expected {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("{realm} inactive should be {expected}");
}

#[tokio::test]
async fn follows_suspension_resumption_and_retirement() {
    let dir = tempfile::tempdir().expect("temp dir");
    platform::storage::db::set_db_path(dir.path())
        .await
        .expect("init db");
    Realm::create("early@example.com".to_string())
        .await
        .expect("create realm");
    Realm::set_enabled("early@example.com", false)
        .await
        .expect("suspend realm");

    let inactive = InactiveRealms::default();
    let watcher = inactive.watch().await;
    // Loaded at start.
    assert!(inactive.contains("early@example.com"));

    Realm::create("late@example.com".to_string())
        .await
        .expect("create realm");
    assert!(!inactive.contains("late@example.com"));
    // The Realm loaded at start already counts as deactivated.
    tokio::time::timeout(Duration::from_secs(2), inactive.deactivated())
        .await
        .expect("deactivation announced");
    Realm::set_enabled("late@example.com", false)
        .await
        .expect("suspend realm");
    eventually(&inactive, "late@example.com", true).await;
    tokio::time::timeout(Duration::from_secs(2), inactive.deactivated())
        .await
        .expect("suspension announced");
    Realm::set_enabled("late@example.com", true)
        .await
        .expect("resume realm");
    eventually(&inactive, "late@example.com", false).await;
    Realm::retire("late@example.com")
        .await
        .expect("retire realm");
    eventually(&inactive, "late@example.com", true).await;

    drop(watcher);
}
