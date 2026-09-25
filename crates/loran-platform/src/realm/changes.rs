//! Realm state changes inside this process.
//!
//! Every write that changes whether a Realm is served (creation, the enabled
//! switch, expiry, activation, retirement, removal of a pending Realm)
//! announces the Realm name once it has committed. Services that keep
//! per-Realm state, such as Signaling connections, TURN or the Issuer's keys,
//! subscribe and re-read the Realm.

use std::sync::LazyLock;
use tokio::sync::broadcast;

static CHANGES: LazyLock<broadcast::Sender<String>> = LazyLock::new(|| broadcast::channel(1024).0);

/// Receive the names of Realms whose state changed. A receiver that falls
/// behind gets `RecvError::Lagged` and should re-read every Realm it tracks.
pub fn subscribe() -> broadcast::Receiver<String> {
    CHANGES.subscribe()
}

/// Announce that `name` changed; called after the write committed.
pub(crate) fn announce(name: &str) {
    let _ = CHANGES.send(name.to_string());
}

static ACTOR_CHANGES: LazyLock<broadcast::Sender<actr_protocol::ActrId>> =
    LazyLock::new(|| broadcast::channel(1024).0);

/// Receive the Actors whose Realm client key binding changed: the key was
/// revoked (the Actor is disabled) or a reissue moved it to another key. A
/// receiver that falls behind gets `RecvError::Lagged` and should re-read
/// every Actor it tracks.
pub fn subscribe_actors() -> broadcast::Receiver<actr_protocol::ActrId> {
    ACTOR_CHANGES.subscribe()
}

/// Announce that `actor`'s binding changed; called after the write
/// committed.
pub fn announce_actor(actor: &actr_protocol::ActrId) {
    let _ = ACTOR_CHANGES.send(actor.clone());
}
