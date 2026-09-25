//! Realms whose TURN credentials are refused.
//!
//! TURN authentication is synchronous and has no database access, so it
//! consults this in-memory set of Realms that are not served (pending,
//! suspended, expired or retired). The set is loaded from the Realm database,
//! updated as soon as a Realm's state change is announced, and reloaded every
//! [`RELOAD_INTERVAL`] because expiry is not announced.
//! [`InactiveRealms::deactivated`] wakes when a Realm joins the set, so the
//! TURN service can close that Realm's allocations.

use platform::realm::Realm;
use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::Notify;
use tokio::sync::broadcast::error::RecvError;
use tokio::task::JoinHandle;

/// How often the whole set is reloaded
pub const RELOAD_INTERVAL: Duration = Duration::from_secs(10);

/// Stops [`InactiveRealms::watch`] when dropped
#[derive(Debug)]
pub struct WatchGuard(JoinHandle<()>);

impl Drop for WatchGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// The Realms that are not served; empty until [`InactiveRealms::watch`].
#[derive(Debug, Clone, Default)]
pub struct InactiveRealms(Arc<RwLock<HashSet<String>>>, Arc<Notify>);

impl InactiveRealms {
    /// Whether credentials of `realm` are refused
    pub fn contains(&self, realm: &str) -> bool {
        self.0
            .read()
            .expect("inactive realms poisoned")
            .contains(realm)
    }

    /// Wait until a Realm becomes inactive (returns at once if one did since
    /// the last call).
    pub async fn deactivated(&self) {
        self.1.notified().await;
    }

    /// Load the set now, then keep it current in a background task that
    /// stops when the returned guard is dropped.
    pub async fn watch(&self) -> WatchGuard {
        let mut changes = platform::realm::changes::subscribe();
        self.reload_all().await;
        let this = self.clone();
        WatchGuard(tokio::spawn(async move {
            let mut reload = tokio::time::interval(RELOAD_INTERVAL);
            loop {
                tokio::select! {
                    _ = reload.tick() => this.reload_all().await,
                    change = changes.recv() => match change {
                        Ok(realm) => this.reload(&realm).await,
                        Err(RecvError::Lagged(_)) => this.reload_all().await,
                        Err(RecvError::Closed) => break,
                    },
                }
            }
        }))
    }

    async fn reload_all(&self) {
        match Realm::get_all().await {
            Ok(realms) => {
                let inactive: HashSet<String> = realms
                    .into_iter()
                    .filter(|realm| !realm.is_active())
                    .map(|realm| realm.name)
                    .collect();
                let mut current = self.0.write().expect("inactive realms poisoned");
                let deactivated = !inactive.is_subset(&current);
                *current = inactive;
                if deactivated {
                    self.1.notify_one();
                }
            }
            Err(e) => platform::recording::warn!("Failed to load Realm states for TURN: {}", e),
        }
    }

    async fn reload(&self, name: &str) {
        match Realm::get(name).await {
            Ok(realm) => {
                let mut inactive = self.0.write().expect("inactive realms poisoned");
                if realm.is_some_and(|realm| !realm.is_active()) {
                    if inactive.insert(name.to_string()) {
                        self.1.notify_one();
                    }
                } else {
                    inactive.remove(name);
                }
            }
            Err(e) => platform::recording::warn!("Failed to load Realm {} for TURN: {}", name, e),
        }
    }

    #[cfg(test)]
    pub(crate) fn insert(&self, realm: &str) {
        self.0
            .write()
            .expect("inactive realms poisoned")
            .insert(realm.to_string());
    }
}
