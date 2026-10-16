//! WirePool - Wire connection pool manager
//!
//! Manages connection strategies: saturated concurrent connections, automatic retry, and fallback strategies.
//! Uses watch channels to broadcast connection status, implementing zero-polling event-driven architecture.

use super::backoff::ExponentialBackoff;
use super::error::{NetworkError, NetworkResult};
use super::wire_handle::{WebRtcTransportLifecycle, WireHandle, WireIdentity, WireStatus};
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::Duration;
use tokio::sync::{RwLock, watch};
use tokio_util::sync::CancellationToken;

const WIRE_CONNECT_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(30);

/// Connection type identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConnType {
    WebSocket,
    WebRTC,
}

impl ConnType {
    /// Convert to array index
    const fn as_index(self) -> usize {
        match self {
            ConnType::WebSocket => 0,
            ConnType::WebRTC => 1,
        }
    }

    /// All connection types
    const ALL: [ConnType; 2] = [ConnType::WebSocket, ConnType::WebRTC];
}

/// Set of ready connections
pub(crate) type ReadySet = HashSet<ConnType>;

/// Retry configuration
#[derive(Debug, Clone, Copy)]
pub(crate) struct RetryConfig {
    pub(crate) max_attempts: u32,
    pub(crate) initial_delay_ms: u64,
    pub(crate) max_delay_ms: u64,
    pub(crate) multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay_ms: 1000,
            max_delay_ms: 10000,
            multiplier: 2.0,
        }
    }
}

impl RetryConfig {
    /// Create ExponentialBackoff from this config
    pub(crate) fn create_backoff(&self) -> ExponentialBackoff {
        ExponentialBackoff::with_multiplier(
            Duration::from_millis(self.initial_delay_ms),
            Duration::from_millis(self.max_delay_ms),
            Some(self.max_attempts),
            self.multiplier,
        )
    }
}

/// Test-only summary of one connection slot's [`WireStatus`] variant.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WireStatusKind {
    Connecting,
    Ready,
    Failed,
}

/// WirePool - Wire connection pool manager
///
/// # Responsibilities
/// - Saturated concurrent connections (simultaneously attempt WebRTC + WebSocket)
/// - Automatic retry with exponential backoff
/// - Broadcast connection status (via watch channel, zero-polling)
/// - Keep all successful connections (no priority-based replacement)
///
/// # Design Highlights
/// - **Event-driven**: Use watch channels to notify status changes
/// - **Zero-polling**: Callers use `await ready_rx.changed()` to wait for connection readiness
/// - **Array optimization**: Use fixed-size array instead of HashMap
pub(crate) struct WirePool {
    /// Connection status (array optimization: WebSocket=0, WebRTC=1)
    connections: Arc<RwLock<[Option<WireStatus>; 2]>>,

    /// Immutable CAS key captured when the WebRTC connection flight obtains a
    /// Controller attempt. It intentionally survives Ready -> Failed so a
    /// racing send-path invalidation cannot erase terminal-cleanup identity.
    webrtc_lifecycle: Arc<RwLock<Option<WebRtcTransportLifecycle>>>,

    /// Ready connection set (broadcast)
    ready_tx: watch::Sender<ReadySet>,
    ready_rx: watch::Receiver<ReadySet>,

    /// Pending connection count
    pending: Arc<AtomicU8>,

    /// Retry configuration
    retry_config: RetryConfig,

    /// Closed flag (used to terminate background tasks)
    closed: Arc<AtomicBool>,

    /// Event-driven shutdown for connection attempts and retry backoff.
    shutdown: CancellationToken,
}

impl WirePool {
    /// Create new wire connection pool
    pub(crate) fn new(retry_config: RetryConfig) -> Self {
        let (tx, rx) = watch::channel(HashSet::new());

        Self {
            connections: Arc::new(RwLock::new([None, None])),
            webrtc_lifecycle: Arc::new(RwLock::new(None)),
            ready_tx: tx,
            ready_rx: rx,
            pending: Arc::new(AtomicU8::new(0)),
            retry_config,
            closed: Arc::new(AtomicBool::new(false)),
            shutdown: CancellationToken::new(),
        }
    }

    /// Add connection and start connection task in background
    ///
    /// Non-blocking, returns immediately and attempts connection concurrently in background
    ///
    /// # Behavior
    /// - **Unconditionally starts**: Always starts connection attempt, even if a connection already exists
    /// - Use `add_connection_smart()` if you want to skip already-ready connections
    pub(crate) async fn add_connection(&self, connection: Arc<dyn WireHandle>) {
        let conn_type = connection.connection_type();
        if conn_type == ConnType::WebRTC {
            *self.webrtc_lifecycle.write().await = None;
        }
        {
            let mut conns = self.connections.write().await;
            conns[conn_type.as_index()] = Some(WireStatus::Connecting(Arc::clone(&connection)));
        }

        let connections = Arc::clone(&self.connections);
        let webrtc_lifecycle = Arc::clone(&self.webrtc_lifecycle);
        let ready_tx = self.ready_tx.clone();
        let pending = Arc::clone(&self.pending);
        let retry_config = self.retry_config;
        let closed = Arc::clone(&self.closed);
        let shutdown = self.shutdown.clone();

        tokio::spawn(async move {
            // Create exponential backoff iterator
            let mut backoff = retry_config.create_backoff();

            // The first attempt starts immediately. Backoff values belong to
            // the gaps before retries, so the first retry consumes the
            // configured initial delay rather than silently skipping it.
            // Wire-close ownership: `close_all` is the sole closer of wires it
            // clears from the pool slots (it closes `Connecting`/`Ready`
            // handles; `Failed` slots were already closed by the upper layer
            // that marked them). Because `closed`/`shutdown` are only set by
            // `close_all`, every exit below that observes them can rely on
            // `close_all` having already closed — or being about to close —
            // this task's wire, so no task exit closes on its own.
            for attempt in 0..retry_config.max_attempts {
                // Check if pool has been closed
                if closed.load(Ordering::Relaxed) {
                    tracing::debug!(
                        "🛑 [{:?}] Connection task terminated (pool closed)",
                        conn_type
                    );
                    return;
                }

                // Wait for delay (first attempt has 0 delay built into iterator)
                if attempt > 0 {
                    let delay = backoff
                        .next()
                        .unwrap_or_else(|| Duration::from_millis(retry_config.max_delay_ms));
                    tracing::debug!(
                        "⏱️ [{:?}] Waiting {:?} before retry {}",
                        conn_type,
                        delay,
                        attempt + 1
                    );
                    tokio::select! {
                        _ = shutdown.cancelled() => {
                            tracing::debug!(
                                "🛑 [{:?}] Retry backoff interrupted by pool close",
                                conn_type
                            );
                            return;
                        }
                        _ = crate::timer::sleep(
                            crate::timer::ids::WIRE_POOL_CONNECT_BACKOFF,
                            delay,
                        ) => {}
                    }

                    // Check again after sleep
                    if closed.load(Ordering::Relaxed) {
                        tracing::debug!(
                            "🛑 [{:?}] Connection task terminated (pool closed)",
                            conn_type
                        );
                        return;
                    }
                }

                pending.fetch_add(1, Ordering::Relaxed);

                tracing::debug!(
                    "🔄 [{:?}] Connecting (attempt {}/{})",
                    conn_type,
                    attempt + 1,
                    retry_config.max_attempts
                );

                let result: NetworkResult<()> = tokio::select! {
                    biased;
                    _ = shutdown.cancelled() => {
                        tracing::debug!(
                            "🛑 [{:?}] Connection attempt interrupted by pool close",
                            conn_type
                        );
                        pending.fetch_sub(1, Ordering::Relaxed);
                        return;
                    }
                    result = crate::timer::timeout(
                        crate::timer::ids::WIRE_POOL_CONNECT_ATTEMPT,
                        WIRE_CONNECT_ATTEMPT_TIMEOUT,
                        connection.connect(),
                    ) => result.unwrap_or_else(|_| {
                        Err(NetworkError::TimeoutError(format!(
                            "{:?} connection attempt timed out after {:?}",
                            conn_type,
                            WIRE_CONNECT_ATTEMPT_TIMEOUT,
                        )))
                    }),
                };
                pending.fetch_sub(1, Ordering::Relaxed);

                match result {
                    Ok(_) => {
                        tracing::info!(
                            "✅ [{:?}] Connection established on attempt {}",
                            conn_type,
                            attempt + 1
                        );

                        // Compare-and-commit under the same lock used by
                        // `close_all`: a connection completing concurrently
                        // with close must never resurrect the cleared slot.
                        if conn_type == ConnType::WebRTC {
                            *webrtc_lifecycle.write().await =
                                connection.webrtc_transport_lifecycle();
                        }
                        let committed = {
                            let mut conns = connections.write().await;
                            if closed.load(Ordering::Acquire) {
                                false
                            } else {
                                conns[conn_type.as_index()] =
                                    Some(WireStatus::Ready(Arc::clone(&connection)));
                                true
                            }
                        };
                        if !committed {
                            if conn_type == ConnType::WebRTC {
                                *webrtc_lifecycle.write().await = None;
                            }
                            // `close_all` owns closing this wire: it cleared
                            // (or is about to clear) the slot under the same
                            // lock and closes every handle it removes.
                            return;
                        }

                        // Snapshot the epoch BEFORE the initial broadcast: any
                        // transition landing between the snapshot and the
                        // listener arming makes `await_secure_state_change`
                        // return immediately, so the change cannot be lost.
                        let initial_epoch = connection.secure_epoch();

                        // Broadcast new ready connection set (keep all connections, no replacement)
                        Self::broadcast_ready_connections(&connections, &ready_tx).await;

                        // Observe asynchronous secure-state transitions (e.g.
                        // `Handshaking → Verified`, rekey, terminal) and
                        // re-broadcast so a sender parked on `watch_ready()` is
                        // woken when a previously-unverified wire becomes
                        // sendable (RFC-0563 review point 2). Inert in phase 2
                        // (state stays `LegacyBypass`); phase 3 drives real
                        // transitions via `SecureCell::set_state_validated`.
                        {
                            let conn = Arc::clone(&connection);
                            let connections = Arc::clone(&connections);
                            let ready_tx = ready_tx.clone();
                            let shutdown = shutdown.clone();
                            tokio::spawn(async move {
                                let mut epoch = initial_epoch;
                                loop {
                                    // Check terminal BEFORE waiting: a wire
                                    // that reached `Closed`/`Failed` before
                                    // this task armed (or in a previous
                                    // iteration) admits no further
                                    // transitions, so parking on
                                    // `await_secure_state_change` would never
                                    // return. Re-broadcast so the ready set
                                    // reflects the removal, then exit.
                                    if conn.secure_channel_state().is_terminal() {
                                        Self::broadcast_ready_connections(&connections, &ready_tx)
                                            .await;
                                        return;
                                    }
                                    tokio::select! {
                                        _ = shutdown.cancelled() => return,
                                        _ = conn.await_secure_state_change(epoch) => {}
                                    }
                                    epoch = conn.secure_epoch();
                                    Self::broadcast_ready_connections(&connections, &ready_tx)
                                        .await;
                                }
                            });
                        }

                        return; // Success, exit
                    }
                    Err(e) => {
                        tracing::warn!(
                            "❌ [{:?}] Connection failed on attempt {}: {}",
                            conn_type,
                            attempt + 1,
                            e
                        );
                    }
                }
            }

            // All retries failed
            tracing::error!(
                "💀 [{:?}] All {} retries exhausted",
                conn_type,
                retry_config.max_attempts
            );

            if conn_type == ConnType::WebRTC {
                *webrtc_lifecycle.write().await = connection.webrtc_transport_lifecycle();
            }

            // Retry exhaustion is the single point that declares the secure
            // session terminally dead. Individual failed attempts stay
            // non-terminal so a later retry could still become sendable.
            connection.mark_secure_failed();

            // A connection can allocate resources before returning an error
            // (notably a WebRTC Controller/PeerConnection that never reaches
            // sendable). Retry exhaustion is terminal for this wire flight, so
            // explicitly close it before discarding the handle from the slot.
            if let Err(error) = connection.close().await {
                tracing::warn!(
                    "Failed to close {:?} connection after retry exhaustion: {}",
                    conn_type,
                    error
                );
            }

            let committed_failure = {
                let mut conns = connections.write().await;
                if closed.load(Ordering::Acquire) {
                    false
                } else {
                    conns[conn_type.as_index()] = Some(WireStatus::Failed(Arc::clone(&connection)));

                    // Check if all connections failed
                    let remaining = pending.load(Ordering::Relaxed);
                    if remaining == 0 {
                        let all_failed = conns
                            .iter()
                            .all(|s| matches!(s, Some(WireStatus::Failed(_)) | None));

                        if all_failed {
                            tracing::error!("💀💀 All connections failed");
                        }
                    }
                    true
                }
            };
            if !committed_failure {
                if conn_type == ConnType::WebRTC {
                    *webrtc_lifecycle.write().await = None;
                }
                return;
            }

            Self::broadcast_ready_connections(&connections, &ready_tx).await;
        });
    }

    /// Add connection smartly - skip if already Ready or Connecting
    ///
    /// # Behavior
    /// - **Ready**: Skip (reuse existing connection)
    /// - **Connecting**: Skip (avoid duplicate retry)
    /// - **None/Failed**: Start connection attempt
    ///
    /// # Use Case
    /// Perfect for reconnection scenarios where you want to retry failed connections
    /// without disrupting working ones.
    #[cfg(feature = "test-utils")]
    pub(crate) async fn add_connection_smart(&self, connection: Arc<dyn WireHandle>) {
        let conn_type = connection.connection_type();

        // Check current status
        let should_add = {
            let conns = self.connections.read().await;
            match &conns[conn_type.as_index()] {
                Some(WireStatus::Ready(_)) => {
                    tracing::debug!("⏭️ [{:?}] Skipping - already Ready", conn_type);
                    false
                }
                Some(WireStatus::Connecting(_)) => {
                    tracing::debug!("⏭️ [{:?}] Skipping - already Connecting", conn_type);
                    false
                }
                Some(WireStatus::Failed(_)) | None => {
                    tracing::info!(
                        "🔄 [{:?}] Starting connection (was {:?})",
                        conn_type,
                        conns[conn_type.as_index()]
                    );
                    true
                }
            }
        };

        if should_add {
            self.add_connection(connection).await;
        }
    }

    /// Broadcast current ready connections
    async fn broadcast_ready_connections(
        connections: &Arc<RwLock<[Option<WireStatus>; 2]>>,
        ready_tx: &watch::Sender<ReadySet>,
    ) {
        let conns = connections.read().await;

        // Collect all ready connections
        let mut ready_set: ReadySet = HashSet::new();

        for conn_type in ConnType::ALL {
            if let Some(WireStatus::Ready(conn)) = &conns[conn_type.as_index()] {
                if conn.secure_channel_state().is_sendable() {
                    ready_set.insert(conn_type);
                }
            }
        }

        // Broadcast ready set
        let _ = ready_tx.send(ready_set);
    }

    /// Watch for connection status changes
    pub(crate) fn watch_ready(&self) -> watch::Receiver<ReadySet> {
        self.ready_rx.clone()
    }

    /// Test-only status summary for one connection slot (see
    /// [`WireStatusKind`]).
    #[cfg(test)]
    pub(crate) async fn connection_status_for_test(
        &self,
        conn_type: ConnType,
    ) -> Option<WireStatusKind> {
        self.connections.read().await[conn_type.as_index()]
            .as_ref()
            .map(|status| match status {
                WireStatus::Connecting(_) => WireStatusKind::Connecting,
                WireStatus::Ready(_) => WireStatusKind::Ready,
                WireStatus::Failed(_) => WireStatusKind::Failed,
            })
    }

    /// Get connection of specified type
    pub(crate) async fn get_connection(&self, conn_type: ConnType) -> Option<Arc<dyn WireHandle>> {
        let conns = self.connections.read().await;

        match &conns[conn_type.as_index()] {
            Some(WireStatus::Ready(conn)) if conn.secure_channel_state().is_sendable() => {
                Some(Arc::clone(conn))
            }
            _ => None,
        }
    }

    /// Return true if any candidate can still become sendable.
    pub(crate) async fn has_live_candidate(&self, conn_types: &[ConnType]) -> bool {
        let conns = self.connections.read().await;

        conn_types
            .iter()
            .any(|conn_type| match &conns[conn_type.as_index()] {
                Some(WireStatus::Ready(conn)) => !conn.secure_channel_state().is_terminal(),
                Some(WireStatus::Connecting(_)) => true,
                _ => false,
            })
    }

    /// Mark a connection as closed/failed
    ///
    /// Called by upper layers (DestTransport) when closing connections.
    /// This replaces the per-connection event listener pattern.
    pub(crate) async fn mark_connection_closed(&self, conn_type: ConnType) {
        {
            let mut conns = self.connections.write().await;
            let status = conns[conn_type.as_index()].take();
            conns[conn_type.as_index()] = match status {
                Some(
                    WireStatus::Connecting(handle)
                    | WireStatus::Ready(handle)
                    | WireStatus::Failed(handle),
                ) => Some(WireStatus::Failed(handle)),
                None => None,
            };
        }

        // Update ready set
        Self::broadcast_ready_connections(&self.connections, &self.ready_tx).await;

        tracing::debug!("🔌 Marked {:?} connection as closed", conn_type);
    }

    /// Check whether the active ready slot for a connection type still matches
    /// the given `WireIdentity`.
    ///
    /// Returns `true` only when the slot is `Ready` and the underlying wire
    /// reports the same identity. Returns `false` for `Connecting`, `Failed`,
    /// `None`, or a mismatched wire (stale or replaced).
    pub(crate) async fn connection_matches_identity(
        &self,
        conn_type: ConnType,
        expected_identity: &WireIdentity,
    ) -> bool {
        let conns = self.connections.read().await;
        match &conns[conn_type.as_index()] {
            Some(WireStatus::Ready(handle)) => {
                handle.identity().as_ref() == Some(expected_identity)
            }
            _ => false,
        }
    }

    /// Check whether a settled WebRTC slot belongs to the exact logical
    /// connection flight named by a terminal Controller event. A Connecting
    /// slot deliberately never matches because WirePool still owns retries;
    /// Ready/Failed slots retain identity for terminal cleanup CAS.
    pub(crate) async fn connection_matches_webrtc_lifecycle(
        &self,
        expected: &WebRtcTransportLifecycle,
    ) -> bool {
        let conns = self.connections.read().await;
        let current = match &conns[ConnType::WebRTC.as_index()] {
            // A terminal Controller snapshot is the outcome of one bounded
            // connect attempt while this slot is Connecting. WirePool still
            // owns its remaining retries, so terminal cleanup must not detach
            // the whole DestTransport before the next attempt starts.
            Some(WireStatus::Connecting(_)) => return false,
            Some(WireStatus::Ready(handle) | WireStatus::Failed(handle)) => {
                handle.webrtc_transport_lifecycle()
            }
            _ => None,
        };
        drop(conns);

        // A long-lived RTC Controller can advance to a fresh transport epoch
        // while the same Ready WireHandle remains cached. Its live identity is
        // authoritative: consulting the commit-time snapshot first would let a
        // delayed terminal event for epoch N evict the healthy epoch N+1.
        if let Some(current) = current {
            return &current == expected;
        }

        // Failed/closed handles may no longer expose their Controller. Retain
        // the commit-time key only as a fallback so terminal cleanup can still
        // remove that dead flight.
        self.webrtc_lifecycle.read().await.as_ref() == Some(expected)
    }

    /// Compare-and-swap close: mark a connection as Failed **only if** the
    /// current ready wire still carries the expected identity.
    ///
    /// Returns `true` when the slot was actually transitioned to Failed.
    /// Returns `false` when the identity no longer matches (the wire has
    /// already been replaced by a fresh connection) — in that case the
    /// ready set is left untouched.
    pub(crate) async fn mark_connection_closed_if_same(
        &self,
        conn_type: ConnType,
        expected_identity: &WireIdentity,
    ) -> bool {
        {
            let mut conns = self.connections.write().await;
            match &conns[conn_type.as_index()] {
                Some(WireStatus::Ready(handle)) => {
                    if handle.identity().as_ref() != Some(expected_identity) {
                        tracing::debug!(
                            "🔌 {:?} identity mismatch — not marking closed (wire already replaced)",
                            conn_type
                        );
                        return false;
                    }
                }
                _ => {
                    // Not Ready — nothing to mark
                    return false;
                }
            }
            let handle = match conns[conn_type.as_index()].take() {
                Some(WireStatus::Ready(handle)) => handle,
                _ => unreachable!("ready slot was validated above"),
            };
            conns[conn_type.as_index()] = Some(WireStatus::Failed(handle));
        }

        Self::broadcast_ready_connections(&self.connections, &self.ready_tx).await;
        tracing::debug!(
            "🔌 Marked {:?} closed (identity matched {:?})",
            conn_type,
            expected_identity
        );
        true
    }

    /// Close all connections in the pool
    ///
    /// Called by DestTransport.close() to clean up all connections.
    /// This also terminates all background connection tasks.
    pub(crate) async fn close_all(&self) {
        // 1. Set closed flag to terminate background tasks
        self.closed.store(true, Ordering::Relaxed);
        self.shutdown.cancel();

        // 2. Clear all connection status, closing any wire the pool still
        // owns. A background attempt task that has not been polled yet (or
        // that commits Ready concurrently with this call) exits without
        // closing — its slot here is the last other owner, so this is the
        // final guaranteed close point. `Failed` slots were already closed by
        // the upper layer that marked them, so they are skipped to keep close
        // counts exact.
        let mut conns = self.connections.write().await;
        let cleared = std::mem::take(&mut *conns);
        drop(conns);
        for status in cleared.into_iter().flatten() {
            if let WireStatus::Connecting(handle) | WireStatus::Ready(handle) = &status {
                if let Err(error) = handle.close().await {
                    tracing::warn!(
                        "Failed to close connection during pool close_all: {}",
                        error
                    );
                }
            }
        }
        *self.webrtc_lifecycle.write().await = None;

        // 3. Broadcast empty ready set
        let _ = self.ready_tx.send(HashSet::new());

        tracing::debug!("🔌 Closed all connections in pool (background tasks will terminate)");
    }

    /// Check if pool is closed
    pub(crate) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
#[path = "wire_pool_tests.rs"]
mod tests;
