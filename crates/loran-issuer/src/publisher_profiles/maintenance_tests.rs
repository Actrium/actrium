use super::*;
use actr_protocol::realm_profile::sign_document;
use actrix_publisher_registry::{PublisherRegistryManager, model::Publisher};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::SigningKey;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct Fetcher {
    documents: Mutex<HashMap<String, Vec<u8>>>,
    calls: Mutex<Vec<String>>,
    active: AtomicUsize,
    peak: AtomicUsize,
    blocked: Option<tokio::sync::Semaphore>,
}

impl Fetcher {
    fn serve(&self, domain: &str, profile: Value) {
        self.documents.lock().unwrap().insert(
            format!("https://{domain}/.well-known/actr"),
            serde_json::to_vec(&json!({
                "version": 1,
                "authority": format!("actr://{domain}"),
                "services": [{"rel": "contracts", "href": format!("https://{domain}/contracts")}],
                "publisher": profile,
            }))
            .unwrap(),
        );
    }

    fn count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

struct ActiveFetch<'a>(&'a AtomicUsize);

impl Drop for ActiveFetch<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

#[async_trait::async_trait]
impl PublicDocumentFetcher for Fetcher {
    async fn fetch(&self, url: &str) -> Result<Arc<[u8]>, FetchError> {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        let _guard = ActiveFetch(&self.active);
        self.peak.fetch_max(active, Ordering::SeqCst);
        self.calls.lock().unwrap().push(url.to_string());
        if let Some(blocked) = &self.blocked {
            let _permit = blocked.acquire().await.unwrap();
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
        self.documents
            .lock()
            .unwrap()
            .get(url)
            .cloned()
            .map(Arc::from)
            .ok_or_else(|| FetchError::new("domain unavailable"))
    }
}

async fn setup() -> (tempfile::TempDir, PublisherRegistryManager) {
    // Each test owns a production-schema database, independent of the
    // process-wide database used by other Issuer tests.
    let dir = tempfile::tempdir().unwrap();
    let database = platform::storage::db::Database::new(dir.path())
        .await
        .unwrap();
    let manager = PublisherRegistryManager::new(database.get_pool().clone());
    (dir, manager)
}

fn profile(domain: &str, sequence: u64, root: u8, package: u8) -> Value {
    let root = SigningKey::from_bytes(&[root; 32]);
    let package = SigningKey::from_bytes(&[package; 32]);
    let key = |id: &str, key: &SigningKey| {
        json!({
            "id": id, "algorithm": "Ed25519", "status": "active",
            "public_key": BASE64.encode(key.verifying_key().to_bytes()),
        })
    };
    let now = chrono::Utc::now();
    let mut profile = json!({
        "schema": "actr.publisher/v1", "sequence": sequence, "domain": domain,
        "profile_signing_keys": [key("root", &root)],
        "keys": [key(&actr_protocol::package_key_id(package.verifying_key().as_bytes()), &package)],
        "profile_issued_at": (now - chrono::Duration::hours(1)).to_rfc3339(),
        "profile_expires_at": (now + chrono::Duration::days(7)).to_rfc3339(),
    });
    sign_document(&mut profile, "root", &root);
    profile
}

async fn register(manager: &PublisherRegistryManager, document: Value) {
    let profile: PublisherProfile = serde_json::from_value(document).unwrap();
    let keys = profile.verify(&profile.domain, chrono::Utc::now()).unwrap();
    manager
        .register_publisher(&profile.domain, &profile, &keys)
        .await
        .unwrap();
}

async fn wait_until(mut condition: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("background check made progress");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_sweeps_keep_their_databases_isolated() {
    let (_first_dir, first) = setup().await;
    let (_second_dir, second) = setup().await;
    let domain = "isolated.example";
    let mut refreshers = Vec::new();
    // The same domain has different successors in the two registries. Both
    // the scan and its refresh must use the database supplied to that test.
    for (manager, package) in [(&first, 3), (&second, 4)] {
        register(manager, profile(domain, 1, 1, 2)).await;
        let fetcher = Arc::new(Fetcher::default());
        fetcher.serve(domain, profile(domain, 2, 1, package));
        refreshers.push(Arc::new(PublisherProfileRefresher::with_pool(
            manager.pool().clone(),
            fetcher,
            DEFAULT_TTL,
            RETRY_AFTER_FAILURE,
        )));
    }
    let (a, b) = tokio::join!(refreshers[0].sweep(), refreshers[1].sweep());
    a.unwrap();
    b.unwrap();
    for (manager, package) in [(&first, 3), (&second, 4)] {
        let row = Publisher::get_by_name(manager.pool(), domain)
            .await
            .unwrap()
            .unwrap();
        let accepted: Value = serde_json::from_str(&row.profile_json).unwrap();
        assert_eq!(accepted["sequence"], 2);
        assert_eq!(accepted["keys"], profile(domain, 2, 1, package)["keys"]);
    }
}

#[tokio::test]
async fn background_revokes_removed_keys_without_registration_and_repeats() {
    let (_dir, manager) = setup().await;
    let domain = "background.example";
    let first = profile(domain, 1, 1, 2);
    let first_id = first["keys"][0]["id"].as_str().unwrap().to_string();
    register(&manager, first).await;
    let second = profile(domain, 2, 1, 3);
    let second_id = second["keys"][0]["id"].as_str().unwrap().to_string();
    let fetcher = Arc::new(Fetcher::default());
    fetcher.serve(domain, second);
    let refresher = Arc::new(PublisherProfileRefresher::with_pool(
        manager.pool().clone(),
        fetcher.clone(),
        Duration::ZERO,
        Duration::ZERO,
    ));
    let cancel = CancellationToken::new();
    let task = tokio::spawn(refresher.run(Duration::from_millis(20), cancel.clone()));
    tokio::time::timeout(Duration::from_secs(5), async {
        while manager.package_signing_key(domain, &first_id).await.is_ok() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        manager
            .package_signing_key(domain, &second_id)
            .await
            .is_ok()
    );
    fetcher.serve(domain, profile(domain, 3, 1, 4));
    tokio::time::timeout(Duration::from_secs(5), async {
        while manager
            .package_signing_key(domain, &second_id)
            .await
            .is_ok()
        {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    cancel.cancel();
    task.await.unwrap();
    wait_until(|| fetcher.active.load(Ordering::SeqCst) == 0).await;
}

#[tokio::test]
async fn sweep_is_bounded_paginates_and_isolates_failed_and_disabled_publishers() {
    let (_dir, manager) = setup().await;
    let fetcher = Arc::new(Fetcher::default());
    for n in 0..SWEEP_PAGE_SIZE + 3 {
        let domain = format!("publisher-{n}.example");
        register(&manager, profile(&domain, 2, 1, 2)).await;
        match n {
            0 => {} // Unreachable; must not block the remaining domains.
            1 => fetcher.serve(&domain, profile(&domain, 3, 5, 3)), // Unlinked identity.
            2 => fetcher.serve(&domain, profile(&domain, 1, 1, 3)), // Rollback.
            _ => fetcher.serve(&domain, profile(&domain, 3, 1, 3)),
        }
    }
    let pool = manager.pool();
    for status in ["pending", "suspended", "revoked"] {
        let domain = format!("{status}.example");
        register(&manager, profile(&domain, 1, 1, 2)).await;
        sqlx::query("UPDATE publisher SET status = ? WHERE name = ?")
            .bind(status)
            .bind(&domain)
            .execute(pool)
            .await
            .unwrap();
    }
    let refresher = Arc::new(PublisherProfileRefresher::with_pool(
        pool.clone(),
        fetcher.clone(),
        DEFAULT_TTL,
        RETRY_AFTER_FAILURE,
    ));
    refresher.sweep().await.unwrap();
    assert_eq!(fetcher.count(), (SWEEP_PAGE_SIZE + 3) as usize);
    assert!(fetcher.peak.load(Ordering::SeqCst) <= SWEEP_CONCURRENCY);
    for n in 0..SWEEP_PAGE_SIZE + 3 {
        let row = Publisher::get_by_name(pool, &format!("publisher-{n}.example"))
            .await
            .unwrap()
            .unwrap();
        let accepted: Value = serde_json::from_str(&row.profile_json).unwrap();
        assert_eq!(accepted["sequence"], if n < 3 { 2 } else { 3 });
    }
    // Both successes and failures share the request-time cache/backoff.
    refresher.sweep().await.unwrap();
    assert_eq!(fetcher.count(), (SWEEP_PAGE_SIZE + 3) as usize);
    assert!(
        refresher
            .ensure_current("publisher-0.example")
            .await
            .is_err()
    );
    assert!(
        refresher
            .ensure_current("publisher-1.example")
            .await
            .is_err()
    );
    assert!(
        refresher
            .ensure_current("publisher-2.example")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn registration_and_sweep_share_one_domain_check() {
    let (_dir, manager) = setup().await;
    let domain = "single-flight.example";
    let document = profile(domain, 1, 1, 2);
    register(&manager, document.clone()).await;
    let fetcher = Arc::new(Fetcher::default());
    fetcher.serve(domain, document);
    let refresher = Arc::new(PublisherProfileRefresher::with_pool(
        manager.pool().clone(),
        fetcher.clone(),
        DEFAULT_TTL,
        RETRY_AFTER_FAILURE,
    ));
    let (sweep, registration) = tokio::join!(refresher.sweep(), refresher.ensure_current(domain));
    sweep.unwrap();
    registration.unwrap();
    assert_eq!(fetcher.count(), 1);
}

#[tokio::test]
async fn cancellation_stops_pending_fetches_and_startup() {
    let (_dir, manager) = setup().await;
    register(&manager, profile("blocked.example", 1, 1, 2)).await;
    let fetcher = Arc::new(Fetcher {
        blocked: Some(tokio::sync::Semaphore::new(0)),
        ..Fetcher::default()
    });
    let refresher = Arc::new(PublisherProfileRefresher::with_pool(
        manager.pool().clone(),
        fetcher.clone(),
        DEFAULT_TTL,
        RETRY_AFTER_FAILURE,
    ));
    let cancel = CancellationToken::new();
    let task = refresher.spawn(cancel.clone());
    wait_until(|| fetcher.count() == 1).await;
    cancel.cancel();
    tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap();
    wait_until(|| fetcher.active.load(Ordering::SeqCst) == 0).await;
    // A service stopped before startup does no network or database work.
    refresher.spawn(cancel).await.unwrap();
    assert_eq!(fetcher.count(), 1);
}

#[tokio::test]
async fn refresh_cannot_recreate_a_deleted_registration() {
    let (_dir, manager) = setup().await;
    let domain = "deleted.example";
    let document = profile(domain, 1, 1, 2);
    register(&manager, document.clone()).await;
    let fetcher = Arc::new(Fetcher {
        blocked: Some(tokio::sync::Semaphore::new(0)),
        ..Fetcher::default()
    });
    fetcher.serve(domain, document);
    let refresher = Arc::new(PublisherProfileRefresher::with_pool(
        manager.pool().clone(),
        fetcher.clone(),
        DEFAULT_TTL,
        RETRY_AFTER_FAILURE,
    ));
    let task = tokio::spawn(async move { refresher.sweep().await });
    wait_until(|| fetcher.count() == 1).await;
    let pool = manager.pool();
    sqlx::query("DELETE FROM publisher WHERE name = ?")
        .bind(domain)
        .execute(pool)
        .await
        .unwrap();
    fetcher.blocked.as_ref().unwrap().add_permits(1);
    task.await.unwrap().unwrap();
    assert!(
        Publisher::get_by_name(pool, domain)
            .await
            .unwrap()
            .is_none()
    );
}
