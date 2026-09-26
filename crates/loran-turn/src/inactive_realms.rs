//! Realms and Actors whose TURN credentials are refused.
//!
//! TURN authentication is synchronous and has no database access, so it
//! consults these in-memory sets: the Realms that are not served (pending,
//! suspended, expired or retired) and the disabled Actors (revoked, or bound
//! to a revoked Realm client key). They are loaded from the database,
//! updated as soon as a Realm or Actor change is announced, and reloaded
//! every [`RELOAD_INTERVAL`] because expiry is not announced.
//! [`InactiveRealms::deactivated`] wakes when a Realm or Actor joins a set,
//! so the TURN service can close their allocations.

use actr_protocol::ActrId;
use platform::realm::Realm;
use platform::realm::membership::{actor_disabled, disabled_actors};
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

/// The Realms that are not served and the disabled Actors; empty until
/// [`InactiveRealms::watch`].
#[derive(Debug, Clone, Default)]
pub struct InactiveRealms {
    realms: Arc<RwLock<HashSet<String>>>,
    /// Text form of the disabled ActorIds.
    actors: Arc<RwLock<HashSet<String>>>,
    deactivated: Arc<Notify>,
}

impl InactiveRealms {
    /// Whether credentials of `realm` are refused
    pub fn contains(&self, realm: &str) -> bool {
        self.realms
            .read()
            .expect("inactive realms poisoned")
            .contains(realm)
    }

    /// Whether credentials of the Actor `actor_id` (text form) are refused
    pub fn contains_actor(&self, actor_id: &str) -> bool {
        self.actors
            .read()
            .expect("inactive actors poisoned")
            .contains(actor_id)
    }

    /// Wait until a Realm or Actor becomes inactive (returns at once if one
    /// did since the last call).
    pub async fn deactivated(&self) {
        self.deactivated.notified().await;
    }

    /// Load the sets now, then keep them current in a background task that
    /// stops when the returned guard is dropped.
    pub async fn watch(&self) -> WatchGuard {
        let mut changes = platform::realm::changes::subscribe();
        let mut actor_changes = platform::realm::changes::subscribe_actors();
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
                    change = actor_changes.recv() => match change {
                        Ok(actor) => this.reload_actor(&actor).await,
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
                self.replace(&self.realms, inactive);
            }
            Err(e) => platform::recording::warn!("Failed to load Realm states for TURN: {}", e),
        }
        match disabled_actors().await {
            Ok(actors) => {
                let disabled = actors.iter().map(ActrId::to_string_repr).collect();
                self.replace(&self.actors, disabled);
            }
            Err(e) => platform::recording::warn!("Failed to load disabled Actors for TURN: {}", e),
        }
    }

    fn replace(&self, set: &RwLock<HashSet<String>>, entries: HashSet<String>) {
        let mut current = set.write().expect("inactive set poisoned");
        let deactivated = !entries.is_subset(&current);
        *current = entries;
        if deactivated {
            self.deactivated.notify_one();
        }
    }

    fn update(&self, set: &RwLock<HashSet<String>>, entry: &str, inactive: bool) {
        let mut current = set.write().expect("inactive set poisoned");
        if inactive {
            if current.insert(entry.to_string()) {
                self.deactivated.notify_one();
            }
        } else {
            current.remove(entry);
        }
    }

    async fn reload(&self, name: &str) {
        match Realm::get(name).await {
            Ok(realm) => self.update(
                &self.realms,
                name,
                realm.is_some_and(|realm| !realm.is_active()),
            ),
            Err(e) => platform::recording::warn!("Failed to load Realm {} for TURN: {}", name, e),
        }
    }

    async fn reload_actor(&self, actor: &ActrId) {
        match actor_disabled(actor).await {
            Ok(disabled) => self.update(&self.actors, &actor.to_string_repr(), disabled),
            Err(e) => platform::recording::warn!(
                "Failed to load Actor {} for TURN: {}",
                actor.to_string_repr(),
                e
            ),
        }
    }

    #[cfg(test)]
    pub(crate) fn insert(&self, realm: &str) {
        self.update(&self.realms, realm, true);
    }

    #[cfg(test)]
    pub(crate) fn insert_actor(&self, actor_id: &str) {
        self.update(&self.actors, actor_id, true);
    }
}
