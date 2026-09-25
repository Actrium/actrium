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
