//! DestTransport - Transport layer manager for a single destination
//!
//! Manages all connections and message routing to a specific Dest (Actor or Shell).
//! Implements event-driven pattern with zero polling.

use super::Dest; // Re-exported from actr-framework
use super::error::{NetworkError, NetworkResult};
use super::route_table::{DataLaneType, PayloadTypeExt};
use super::wire_handle::{WireHandle, WireIdentity};
use super::wire_pool::{ConnType, ReadySet, RetryConfig, WirePool};
use actr_protocol::PayloadType;
use std::sync::Arc;
use tokio::sync::watch;

/// DestTransport - Transport layer manager for a single destination
///
/// Core responsibilities:
/// - Manage all connections to a specific Dest (WebSocket + WebRTC)
/// - Concurrently establish connections in background (saturated connection pattern)
/// - Event-driven wait for connection status
/// - Cache Lanes within WireHandle
/// - WirePool handles priority selection
pub(crate) struct DestTransport {
    /// Connection manager
    conn_mgr: Arc<WirePool>,
}

/// Cleans up a partially-built destination pool if its async constructor is
/// cancelled. Pool cancellation stops background attempts, while this guard
/// closes every input wire directly so cancellation is safe even when an
/// attempt task has not been polled yet.
struct DestTransportCreationGuard {
    conn_mgr: Arc<WirePool>,
    connections: Option<Vec<Arc<dyn WireHandle>>>,
}

impl DestTransportCreationGuard {
    fn new(conn_mgr: Arc<WirePool>, connections: Vec<Arc<dyn WireHandle>>) -> Self {
        Self {
            conn_mgr,
            connections: Some(connections),
        }
    }

    fn disarm(&mut self) {
        self.connections = None;
    }
}

impl Drop for DestTransportCreationGuard {
    fn drop(&mut self) {
        let Some(connections) = self.connections.take() else {
            return;
        };
        let conn_mgr = Arc::clone(&self.conn_mgr);
        tokio::spawn(async move {
            conn_mgr.close_all().await;
            for connection in connections {
                if let Err(error) = connection.close().await {
                    tracing::warn!(
                        "Failed to close wire abandoned during DestTransport creation: {}",
                        error
                    );
                }
            }
        });
    }
}

impl DestTransport {
    /// Create new DestTransport
    ///
    /// # Arguments
    /// - `dest`: destination
    /// - `connections`: list of pre-built connections (WebSocket/WebRTC)
    pub(crate) async fn new(
        dest: Dest,
        connections: Vec<Arc<dyn WireHandle>>,
    ) -> NetworkResult<Self> {
        Self::new_inner(
            dest,
            connections,
            #[cfg(test)]
            None,
        )
        .await
    }

    #[cfg(test)]
    pub(crate) async fn new_with_creation_barrier(
        dest: Dest,
        connections: Vec<Arc<dyn WireHandle>>,
        creation_barrier: Option<Arc<tokio::sync::Barrier>>,
    ) -> NetworkResult<Self> {
        Self::new_inner(dest, connections, creation_barrier).await
    }

    async fn new_inner(
        dest: Dest,
        connections: Vec<Arc<dyn WireHandle>>,
        #[cfg(test)] creation_barrier: Option<Arc<tokio::sync::Barrier>>,
    ) -> NetworkResult<Self> {
        let conn_mgr = Arc::new(WirePool::new(RetryConfig::default()));
        let mut creation_guard =
            DestTransportCreationGuard::new(Arc::clone(&conn_mgr), connections);

        // Start connection tasks in background (concurrently)
        tracing::info!("🚀 [{:?}] Starting connection tasks...", dest);
        let connections = creation_guard
            .connections
            .as_ref()
            .expect("destination creation guard must be armed")
            .clone();
        #[cfg(test)]
        let mut is_first = true;
        for conn in connections {
            conn_mgr.add_connection(conn).await;
            #[cfg(test)]
            {
                if is_first && let Some(barrier) = creation_barrier.as_ref() {
                    // Two phases let a cancellation test observe that creation
                    // is inside this async constructor, then hold it there.
                    barrier.wait().await;
                    barrier.wait().await;
                }
                is_first = false;
            }
        }

        creation_guard.disarm();
        Ok(Self { conn_mgr })
    }

    /// Send message and return the wire identity that accepted the payload.
    ///
    /// Core design: event-driven waiting
    /// - If connection available, send immediately
    /// - If not, wait for connection status change (via watch channel)
    /// - WirePool already handles priority, only need to try DataLane Types in order
    /// - Stale self-heal: when `get_lane()` returns a closed-like error, the
    ///   current WebRTC wire may be stale. We conditionally mark it Failed
    ///   (only if it still carries the same identity) so the outer loop can
    ///   wait for a new connection or fallback instead of spinning forever.
    #[cfg_attr(
        feature = "opentelemetry",
        tracing::instrument(skip_all, name = "DestTransport.send")
    )]
    pub(crate) async fn send_with_identity(
        &self,
        payload_type: PayloadType,
        data: &[u8],
    ) -> NetworkResult<Option<WireIdentity>> {
        tracing::debug!(
            "📤 Sending message: type={:?}, size={}",
            payload_type,
            data.len()
        );

        // 1. Get supported DataLane Types for this PayloadType (already prioritized)
        let lane_types = payload_type.data_lane_types();

        if lane_types.is_empty() {
            return Err(NetworkError::NoRoute(format!(
                "No route for: {payload_type:?}"
            )));
        }
        let candidate_conn_types = candidate_conn_types(lane_types);

        // 2. Subscribe to connection status changes
        let mut conn_watcher = self.conn_mgr.watch_ready();

        'send: loop {
            // 3. Check currently available connections (clone to avoid borrowing across await)
            let ready_connections = {
                let ready = conn_watcher.borrow_and_update();
                tracing::trace!("🔍 Available connections: {:?}", ready);
                ready.clone()
            };

            // 4. Try each DataLane Type in priority order
            for &lane_type in lane_types {
                // Determine required connection type
                let conn_type = if lane_type.needs_webrtc() {
                    ConnType::WebRTC
                } else {
                    ConnType::WebSocket
                };

                // Check if this connection is ready
                if !ready_connections.contains(&conn_type) {
                    tracing::trace!("🔍 {:?} not ready, trying next", conn_type);
                    continue;
                }

                // Get connection and create/get Lane
                if let Some(conn) = self.conn_mgr.get_connection(conn_type).await {
                    let wire_identity = conn.identity();

                    // Use original payload_type to create DataLane
                    match conn.get_lane(payload_type).await {
                        Ok(lane) => {
                            tracing::info!(
                                "📡 [channel={:?}] {:?} ({} bytes)",
                                conn_type,
                                payload_type,
                                data.len()
                            );
                            // Convert to Bytes (zero-copy)
                            let payload = bytes::Bytes::copy_from_slice(data);
                            let result = lane.send(payload.clone()).await;

                            // If the lane went stale after selection, drop the
                            // cached lane and retry once before surfacing the
                            // error. Transport-agnostic: a closed-like error
                            // means the underlying wire (WebRTC *or* WebSocket)
                            // is no longer sendable and should be self-healed.
                            if let Err(e) = &result {
                                if e.is_closed_like() {
                                    tracing::warn!(
                                        "♻️ lane closed-like error for {:?} ({:?}), invalidating lane and retrying once: {}",
                                        payload_type,
                                        conn_type,
                                        e,
                                    );
                                    conn.invalidate_lane(payload_type).await;
                                    match conn.get_lane(payload_type).await {
                                        Ok(new_lane) => {
                                            return new_lane
                                                .send(payload)
                                                .await
                                                .map(|_| wire_identity);
                                        }
                                        Err(retry_err) => {
                                            if matches!(
                                                retry_err,
                                                NetworkError::SecureChannelNotReady(_)
                                            ) {
                                                // Secure epoch rotated again mid-retry:
                                                // wait for readiness and re-acquire a
                                                // lane — the wire itself is healthy.
                                                conn.invalidate_lane(payload_type).await;
                                                continue 'send;
                                            }
                                            if retry_err.is_closed_like() {
                                                self.evict_closed_like_connection(
                                                    &conn,
                                                    conn_type,
                                                    payload_type,
                                                    "retry get_lane",
                                                )
                                                .await;
                                                continue 'send;
                                            }
                                            return Err(retry_err);
                                        }
                                    }
                                }
                                if matches!(e, NetworkError::SecureChannelNotReady(_)) {
                                    // Secure session alive but not sendable
                                    // (e.g. rekey in flight), or the lane's
                                    // epoch rotated: invalidate the lane and
                                    // wait for the pool's readiness broadcast —
                                    // do NOT evict or close the wire.
                                    tracing::info!(
                                        "🔐 secure channel not ready for {:?} ({}), waiting for readiness",
                                        conn_type,
                                        e,
                                    );
                                    conn.invalidate_lane(payload_type).await;
                                    continue 'send;
                                }
                            }

                            return result.map(|_| wire_identity);
                        }
                        Err(e) => {
                            if matches!(e, NetworkError::SecureChannelNotReady(_)) {
                                // Same as the send path above: wait for secure
                                // readiness instead of evicting the wire.
                                tracing::info!(
                                    "🔐 secure channel not ready for {:?} ({}), waiting for readiness",
                                    conn_type,
                                    e,
                                );
                                conn.invalidate_lane(payload_type).await;
                                continue 'send;
                            }

                            let is_closed_like = e.is_closed_like();

                            if is_closed_like {
                                self.evict_closed_like_connection(
                                    &conn,
                                    conn_type,
                                    payload_type,
                                    "get_lane",
                                )
                                .await;
                                continue 'send;
                            }
                            tracing::warn!("❌ Failed to get DataLane: {:?}: {}", lane_type, e);
                            continue;
                        }
                    }
                }
            }

            // 5. All attempts failed, wait for connection status change
            tracing::info!("⏳ Waiting for connection status...");

            if self.conn_mgr.is_closed() {
                return Err(NetworkError::ChannelClosed(
                    "connection manager closed".into(),
                ));
            }

            if !self
                .conn_mgr
                .has_live_candidate(&candidate_conn_types)
                .await
            {
                return Err(NetworkError::NoRoute(format!(
                    "all transport candidates exhausted for {payload_type:?}: {candidate_conn_types:?}"
                )));
            }

            // Event-driven wait!
            if conn_watcher.changed().await.is_err() {
                return Err(NetworkError::ChannelClosed(
                    "connection manager closed".into(),
                ));
            }

            tracing::debug!("🔔 Connection status updated, retrying...");
        }
    }

    async fn evict_closed_like_connection(
        &self,
        conn: &Arc<dyn WireHandle>,
        conn_type: ConnType,
        payload_type: PayloadType,
        source: &'static str,
    ) {
        tracing::warn!(
            "❌ {source} returned closed-like error for {:?}, invalidating lane",
            conn_type
        );
        conn.invalidate_lane(payload_type).await;

        // Stale self-heal: if the wire still carries the same identity, mark it
        // Failed so the outer loop can re-read readiness and retry/fallback
        // without waiting on a watch update that may never arrive.
        let wire_identity = conn.identity();
        let changed = match wire_identity.as_ref() {
            Some(identity) => {
                self.conn_mgr
                    .mark_connection_closed_if_same(conn_type, identity)
                    .await
            }
            None => false,
        };

        // The wire returned a closed-like error, so it is dead: drive its
        // secure state to terminal (and free its transport resources) so the
        // pool's state-listener task exits instead of parking forever on the
        // retired wire's epoch. Spawned so the send path is not blocked by a
        // potentially-slow graceful drain. `close()` is idempotent on both
        // transports, so callers that already closed are unaffected.
        let conn_for_close = Arc::clone(conn);
        tokio::spawn(async move {
            let _ = conn_for_close.close().await;
        });

        tracing::warn!(
            "♻️ DestTransport stale self-heal: payload_type={:?}, conn_type={:?}, expected_identity={:?}, changed={}",
            payload_type,
            conn_type,
            wire_identity,
            changed
        );
    }

    /// Retry failed connections (smart reconnect)
    ///
    /// # Behavior
    /// - Calls WireBuilder to create new connections
    /// - Uses `add_connection_smart()` to skip already-working connections
    /// - Perfect for reconnection after detecting connection failures
    ///
    /// # Arguments
    /// - `dest`: destination (used by WireBuilder)
    /// - `wire_builder`: factory to create new WireHandles
    #[cfg(feature = "test-utils")]
    pub(crate) async fn retry_failed_connections(
        &self,
        dest: &Dest,
        wire_builder: &dyn super::WireBuilder,
    ) -> NetworkResult<()> {
        tracing::info!("🔄 Retrying failed connections for: {:?}", dest);

        // Get fresh connections from builder (no cancel token for retry)
        let connections = wire_builder
            .create_connections_with_cancel(dest, None)
            .await?;

        if connections.is_empty() {
            return Err(NetworkError::ConfigurationError(
                "WireBuilder returned no connections".to_string(),
            ));
        }

        // Add each connection smartly (skip Ready/Connecting)
        for conn in connections {
            self.conn_mgr.add_connection_smart(conn).await;
        }

        Ok(())
    }

    /// Close a single connection type without affecting other connections in
    /// the pool.
    ///
    /// Used when a WebRTC connection closes but the WebSocket connection for
    /// the same peer should remain alive (e.g. for response routing).
    pub(crate) async fn close_connection(&self, conn_type: ConnType) -> NetworkResult<()> {
        if let Some(conn) = self.conn_mgr.get_connection(conn_type).await {
            if let Err(e) = conn.close().await {
                tracing::warn!("Failed to close {:?} connection: {}", conn_type, e);
            } else {
                tracing::debug!("Closed {:?} connection", conn_type);
            }
            self.conn_mgr.mark_connection_closed(conn_type).await;
        }
        Ok(())
    }

    /// Close DestTransport and release all connection resources
    pub(crate) async fn close(&self) -> NetworkResult<()> {
        tracing::info!("🔌 Closing DestTransport");

        // 1. Get all connections and close them one by one
        let mut first_error = None;
        for conn_type in [ConnType::WebSocket, ConnType::WebRTC] {
            if let Some(conn) = self.conn_mgr.get_connection(conn_type).await {
                if let Err(e) = conn.close().await {
                    tracing::warn!("❌ Failed to close {:?} connection: {}", conn_type, e);
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                } else {
                    tracing::debug!("✅ Closed {:?} connection", conn_type);
                }

                // 2. Mark connection as closed in the pool
                // This updates the ready_tx and notifies waiters
                self.conn_mgr.mark_connection_closed(conn_type).await;
            }
        }

        // 3. Clean up the entire pool
        self.conn_mgr.close_all().await;

        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    /// Check if any connection is still healthy
    ///
    /// Used by health checker to detect failed connections
    ///
    /// # Returns
    /// - `true`: at least one connection is healthy (connected)
    /// - `false`: all connections are unhealthy or no connections exist
    #[cfg(feature = "test-utils")]
    pub(crate) async fn has_healthy_connection(&self) -> bool {
        for conn_type in [ConnType::WebRTC, ConnType::WebSocket] {
            if let Some(conn) = self.conn_mgr.get_connection(conn_type).await {
                if conn.is_connected() {
                    return true;
                }
            }
        }
        false
    }

    /// Subscribe to ready-set changes (used for manager-side cleanup).
    pub(crate) fn watch_ready(&self) -> watch::Receiver<ReadySet> {
        self.conn_mgr.watch_ready()
    }

    /// Check whether the active WebRTC wire still carries the given identity.
    ///
    /// Used by PeerTransport (session-guarded cleanup) to decide whether a
    /// `ConnectionClosed` event is stale or matches the current wire.
    pub(crate) async fn matches_webrtc_session(
        &self,
        peer_id: &actr_protocol::ActrId,
        session_id: u64,
    ) -> bool {
        let expected = WireIdentity::WebRtc {
            peer_id: peer_id.clone(),
            session_id,
        };
        self.conn_mgr
            .connection_matches_identity(ConnType::WebRTC, &expected)
            .await
    }
}

fn candidate_conn_types(lane_types: &[DataLaneType]) -> Vec<ConnType> {
    let mut conn_types = Vec::new();
    for &lane_type in lane_types {
        let conn_type = if lane_type.needs_webrtc() {
            ConnType::WebRTC
        } else {
            ConnType::WebSocket
        };
        if !conn_types.contains(&conn_type) {
            conn_types.push(conn_type);
        }
    }
    conn_types
}

#[cfg(test)]
#[path = "dest_transport_tests.rs"]
mod tests;
