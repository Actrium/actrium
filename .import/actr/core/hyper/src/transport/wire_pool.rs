//! WirePool - Wire connection pool manager
//!
//! Manages connection strategies: saturated concurrent connections, automatic retry, and fallback strategies.
//! Uses watch channels to broadcast connection status, implementing zero-polling event-driven architecture.

use super::backoff::ExponentialBackoff;
use super::error::{NetworkError, NetworkResult};
use super::wire_handle::{WireHandle, WireIdentity, WireStatus};
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

/// Outcome of comparing a connection slot against an expected `WireIdentity`.
///
/// Distinguishes a live same-session wire from a dead one so callers can pick
/// the right teardown/cleanup scope instead of collapsing both into "match".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SessionMatch {
    /// The slot is `Ready` and carries the expected identity — the live wire
    /// for this session.
    Ready,
    /// The slot is `Failed` but retains the expected identity — this
    /// session's wire already died. The event is NOT stale, but closing the
    /// wire is a no-op; cleanup must be decided from the remaining lanes.
    Retained,
    /// No match: slot empty, `Connecting`, or carrying a different identity
    /// (replaced by a fresh session).
    NoMatch,
}

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
        {
            let mut conns = self.connections.write().await;
            conns[conn_type.as_index()] = Some(WireStatus::Connecting);
        }

        let connections = Arc::clone(&self.connections);
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
                        if let Err(error) = connection.close().await {
                            tracing::warn!(
                                "Failed to close {:?} connection after pool cancellation: {}",
                                conn_type,
                                error
                            );
                        }
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
                            if let Err(error) = connection.close().await {
                                tracing::warn!(
                                    "Failed to close {:?} connection completed after pool close: {}",
                                    conn_type,
                                    error
                                );
                            }
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

            // Retry exhaustion is the single point that declares the secure
            // session terminally dead. Individual failed attempts stay
            // non-terminal so a later retry could still become sendable.
            connection.mark_secure_failed();

            // Capture the wire's identity before transitioning out of Ready so
            // a later same-session close event is not misjudged as stale.
            let failed_identity = connection.identity();
            {
                let mut conns = connections.write().await;
                conns[conn_type.as_index()] = Some(WireStatus::Failed(failed_identity));

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
                Some(WireStatus::Connecting) => {
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

        conn_types.iter().any(|conn_type| {
            matches!(
                &conns[conn_type.as_index()],
                Some(WireStatus::Ready(_)) | Some(WireStatus::Connecting)
            )
        })
    }

    /// Mark a connection as closed/failed
    ///
    /// Called by upper layers (DestTransport) when closing connections.
    /// This replaces the per-connection event listener pattern. Preserves the
    /// wire's last `WireIdentity` so a subsequent same-session close event is
    /// not misjudged as stale.
    pub(crate) async fn mark_connection_closed(&self, conn_type: ConnType) {
        {
            let mut conns = self.connections.write().await;
            let last_identity = match &conns[conn_type.as_index()] {
                // Capture the Ready wire's identity before retiring it.
                Some(WireStatus::Ready(handle)) => handle.identity(),
                // Already Failed — preserve the previously captured identity.
                Some(WireStatus::Failed(id)) => id.clone(),
                // Never Ready, no identity to retain.
                _ => None,
            };
            conns[conn_type.as_index()] = Some(WireStatus::Failed(last_identity));
        }

        // Update ready set
        Self::broadcast_ready_connections(&self.connections, &self.ready_tx).await;

        tracing::debug!("🔌 Marked {:?} connection as closed", conn_type);
    }

    /// Compare a connection slot against the given `WireIdentity`.
    ///
    /// Returns [`SessionMatch::Ready`] when the slot holds the live wire for
    /// this session, [`SessionMatch::Retained`] when the wire for this
    /// session already died but its identity is retained on the `Failed`
    /// slot, and [`SessionMatch::NoMatch`] otherwise (empty, `Connecting`,
    /// or a different session).
    pub(crate) async fn session_match(
        &self,
        conn_type: ConnType,
        expected_identity: &WireIdentity,
    ) -> SessionMatch {
        let conns = self.connections.read().await;
        match &conns[conn_type.as_index()] {
            Some(WireStatus::Ready(handle)) => {
                if handle.identity().as_ref() == Some(expected_identity) {
                    SessionMatch::Ready
                } else {
                    SessionMatch::NoMatch
                }
            }
            Some(WireStatus::Failed(Some(last_identity))) if last_identity == expected_identity => {
                SessionMatch::Retained
            }
            _ => SessionMatch::NoMatch,
        }
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
                    // Not Ready — nothing to transition, and we must not
                    // clobber a previously-retained Failed identity.
                    return false;
                }
            }
            conns[conn_type.as_index()] = Some(WireStatus::Failed(Some(expected_identity.clone())));
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

        // 2. Clear all connection status
        let mut conns = self.connections.write().await;
        *conns = [None, None];

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
