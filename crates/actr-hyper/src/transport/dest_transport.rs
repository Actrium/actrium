//! DestTransport - Transport layer manager for a single destination
//!
//! Manages all connections and message routing to a specific Dest (Actor or Shell).
//! Implements event-driven pattern with zero polling.

use super::Dest; // Re-exported from actr-framework
use super::error::{NetworkError, NetworkResult};
use super::handover::HandoverBook;
use super::lane::{
    DeliveryProgress, DeliveryProgressCell, mark_delivery_write_in_flight, record_delivery_progress,
};
use super::route_table::{DataLaneType, PayloadTypeExt};
use super::wire_handle::{WebRtcTransportLifecycle, WireHandle, WireIdentity};
use super::wire_pool::{ConnType, ReadySet, RetryConfig, WirePool};
use crate::timer;
use actr_protocol::prost::Message;
use actr_protocol::{PayloadType, transport_control};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, watch};
use tokio::time::Instant;

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
    dest: Dest,
    handover: Arc<HandoverBook>,
    reliable: Mutex<ReliableRoute>,
}

#[derive(Default)]
struct ReliableRoute {
    current: Option<Arc<dyn WireHandle>>,
    retry_at: Option<Instant>,
}

/// Result of one destination send, retaining the low-level delivery fact while
/// the route identity remains available to timeout/recovery cleanup.
pub(crate) struct DestSendAttempt {
    pub(crate) result: NetworkResult<Option<WireIdentity>>,
    pub(crate) progress: DeliveryProgress,
    pub(crate) proven_pre_send: bool,
}

impl DestSendAttempt {
    fn submitted(identity: Option<WireIdentity>) -> Self {
        Self {
            result: Ok(identity),
            progress: DeliveryProgress::SubmittedLocally,
            proven_pre_send: false,
        }
    }

    fn failed(error: NetworkError, progress: DeliveryProgress) -> Self {
        Self::failed_with_proof(error, progress, false)
    }

    fn failed_pre_send(error: NetworkError) -> Self {
        Self::failed_with_proof(error, DeliveryProgress::NotAttempted, true)
    }

    fn failed_with_proof(
        error: NetworkError,
        progress: DeliveryProgress,
        proven_pre_send: bool,
    ) -> Self {
        Self {
            result: Err(error),
            progress,
            proven_pre_send: proven_pre_send && progress == DeliveryProgress::NotAttempted,
        }
    }
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
            RetryConfig::default(),
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
        Self::new_inner(dest, connections, RetryConfig::default(), creation_barrier).await
    }

    #[cfg(test)]
    pub(crate) async fn new_with_retry_config(
        dest: Dest,
        connections: Vec<Arc<dyn WireHandle>>,
        retry_config: RetryConfig,
    ) -> NetworkResult<Self> {
        Self::new_inner(dest, connections, retry_config, None).await
    }

    async fn new_inner(
        dest: Dest,
        connections: Vec<Arc<dyn WireHandle>>,
        retry_config: RetryConfig,
        #[cfg(test)] creation_barrier: Option<Arc<tokio::sync::Barrier>>,
    ) -> NetworkResult<Self> {
        let conn_mgr = Arc::new(WirePool::new(retry_config));
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
        Ok(Self {
            conn_mgr,
            dest,
            handover: Arc::new(HandoverBook::default()),
            reliable: Mutex::new(ReliableRoute::default()),
        })
    }

    pub(crate) fn set_handover(&mut self, handover: Arc<HandoverBook>) {
        self.handover = handover;
    }

    async fn reliable_wire(
        &self,
        preferred: Arc<dyn WireHandle>,
        route: &mut ReliableRoute,
    ) -> NetworkResult<Arc<dyn WireHandle>> {
        let Some(previous) = route.current.clone() else {
            return Ok(preferred);
        };
        if Arc::ptr_eq(&previous, &preferred) {
            return Ok(preferred);
        }
        let still_current = self.conn_mgr.owns_active_wire(&previous).await;
        if !still_current || previous.secure_channel_state().is_terminal() {
            // Failure recovery cannot prove delivery of the failed route's
            // buffered data. Do not replay application chunks automatically.
            route.retry_at = None;
            return Ok(preferred);
        }
        if !previous.is_connected() || !previous.secure_channel_state().is_sendable() {
            return Err(NetworkError::SecureChannelNotReady(
                "previous reliable route is recovering; handover is not acknowledged".into(),
            ));
        }
        let lane = match previous.get_lane(PayloadType::StreamReliable).await {
            Ok(lane) if lane.is_healthy() => lane,
            Ok(_) => {
                return Err(NetworkError::SecureChannelNotReady(
                    "previous reliable lane is temporarily unavailable for handover".into(),
                ));
            }
            Err(error) if error.is_closed_like() => {
                route.retry_at = None;
                return Ok(preferred);
            }
            Err(error) => return Err(error),
        };
        if route.retry_at.is_some_and(|retry| retry > Instant::now()) {
            return Ok(previous);
        }
        let Dest::Peer(peer) = &self.dest else {
            return Ok(preferred);
        };
        let Some(mut pending) = self.handover.begin(peer) else {
            return Ok(previous);
        };
        // Cancellation removes the pending entry and leaves the old route in
        // place. These control bytes never update application delivery state.
        let barrier = transport_control::barrier(&pending.nonce).encode_to_vec();
        let acknowledged = timer::timeout(
            timer::ids::TRANSPORT_HANDOVER,
            Duration::from_secs(3),
            async {
                lane.send(bytes::Bytes::from(barrier)).await.ok()?;
                (&mut pending.receiver).await.ok()
            },
        )
        .await
        .is_ok_and(|result| result.is_some());
        if acknowledged {
            route.retry_at = None;
            Ok(preferred)
        } else {
            route.retry_at = Some(Instant::now() + Duration::from_secs(30));
            Ok(previous)
        }
    }

    /// Send message and return the wire identity that accepted the payload.
    ///
    /// Core design: event-driven waiting
    /// - If connection available, send immediately
    /// - If not, wait for connection status change (via watch channel)
    /// - WirePool already handles priority, only need to try DataLane Types in order
    /// - Stale self-heal: when lane acquisition or a concrete send fails before
    ///   submission, the current wire may be stale. We conditionally mark it
    ///   Failed (only if it still carries the same identity) so the outer loop
    ///   can wait for a new connection or fallback instead of spinning
    ///   forever. An ambiguous send is returned immediately and is never
    ///   replayed here.
    #[cfg_attr(
        feature = "opentelemetry",
        tracing::instrument(skip_all, name = "DestTransport.send")
    )]
    #[allow(dead_code)] // Compatibility wrapper retained for transport tests.
    pub(crate) async fn send_with_identity(
        &self,
        payload_type: PayloadType,
        data: &[u8],
    ) -> NetworkResult<Option<WireIdentity>> {
        self.send_with_progress(payload_type, data, None)
            .await
            .result
    }

    /// Send a message while retaining whether the concrete lane submission
    /// was not attempted, accepted locally, or became ambiguous.
    ///
    /// `progress_sink`, when given, receives the progress fact as soon as it
    /// exists — tentatively `Unknown` immediately before each lane write and
    /// authoritatively when the lane resolves — so a caller whose deadline
    /// cancels this future can still classify the send.
    pub(crate) async fn send_with_progress(
        &self,
        payload_type: PayloadType,
        data: &[u8],
        progress_sink: Option<&DeliveryProgressCell>,
    ) -> DestSendAttempt {
        let mut reliable = if payload_type == PayloadType::StreamReliable {
            Some(self.reliable.lock().await)
        } else {
            None
        };
        tracing::debug!(
            "📤 Sending message: type={:?}, size={}",
            payload_type,
            data.len()
        );

        // 1. Get supported DataLane Types for this PayloadType (already prioritized)
        let lane_types = payload_type.data_lane_types();

        if lane_types.is_empty() {
            return DestSendAttempt::failed(
                NetworkError::NoRoute(format!("No route for: {payload_type:?}")),
                DeliveryProgress::NotAttempted,
            );
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

            // Re-evaluate ICE classification for every send. An open RTC
            // channel can still be using TURN, and ICE may later change its
            // selected pair without replacing the wire.
            let mut ordered_lanes = lane_types.to_vec();
            let rtc_priority = self
                .conn_mgr
                .get_connection(ConnType::WebRTC)
                .await
                .map_or(0, |wire| wire.priority());
            let relay_priority = self
                .conn_mgr
                .get_connection(ConnType::WebSocket)
                .await
                .map_or(0, |wire| wire.priority());
            if relay_priority > rtc_priority {
                ordered_lanes.sort_by_key(|lane| lane.needs_webrtc());
            }
            for lane_type in ordered_lanes {
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
                    let conn = if let Some(route) = reliable.as_deref_mut() {
                        match self.reliable_wire(conn, route).await {
                            Ok(conn) => conn,
                            Err(error) => return DestSendAttempt::failed_pre_send(error),
                        }
                    } else {
                        conn
                    };
                    let conn_type = conn.connection_type();
                    let wire_identity = conn.identity();

                    // Use original payload_type to create DataLane
                    match conn.get_lane(payload_type).await {
                        Ok(lane) => {
                            // A physical wire can remain in WirePool's
                            // ready-set while its concrete lane is no longer
                            // sendable (for example, RTC controller
                            // recovery).  Do not select that stale candidate;
                            // continue to the next payload-capable route so a
                            // ready WebSocket fallback can carry the send.
                            if !lane.is_healthy() {
                                tracing::debug!(
                                    "🔍 {:?} lane on {:?} is not healthy, trying next route",
                                    payload_type,
                                    conn_type
                                );
                                continue;
                            }
                            tracing::info!(
                                "📡 [channel={:?}] {:?} ({} bytes)",
                                conn_type,
                                payload_type,
                                data.len()
                            );
                            // Commit the ordered route only when a healthy lane
                            // is about to receive application bytes. Merely
                            // inspecting an unusable candidate must not pin it.
                            if let Some(route) = reliable.as_deref_mut() {
                                route.current = Some(conn.clone());
                            }
                            // Convert to Bytes (zero-copy)
                            let payload = bytes::Bytes::copy_from_slice(data);
                            if let Some(cell) = progress_sink {
                                mark_delivery_write_in_flight(cell);
                            }
                            let attempt = lane.send_with_progress(payload.clone()).await;
                            let progress = attempt.progress;
                            if let Some(cell) = progress_sink {
                                record_delivery_progress(cell, progress);
                            }

                            // Secure readiness is a pre-send condition even
                            // when a custom adapter misreports its progress
                            // as `Unknown`. Recover the lane before applying
                            // the terminal ambiguous-send rule below.
                            if let Err(error) = attempt.result.as_ref() {
                                if matches!(error, NetworkError::SecureChannelNotReady(_)) {
                                    tracing::info!(
                                        "🔐 secure channel not ready for {:?} ({}), waiting for readiness",
                                        conn_type,
                                        error,
                                    );
                                    conn.invalidate_lane(payload_type).await;
                                    continue 'send;
                                }
                            }

                            // Once a concrete adapter reports that bytes may
                            // have entered the transport, the route loop must
                            // not replay this payload.  Preserve the progress
                            // fact for the operation owner instead.
                            if progress == DeliveryProgress::Unknown {
                                return match attempt.result {
                                    Ok(()) => DestSendAttempt::submitted(wire_identity),
                                    Err(error) => DestSendAttempt::failed_with_proof(
                                        error,
                                        progress,
                                        attempt.proven_pre_send,
                                    ),
                                };
                            }

                            // If the lane went stale after selection, drop the
                            // cached lane and retry once before surfacing the
                            // error. This self-heal applies only to proven
                            // `NotAttempted` failures (ambiguous `Unknown`
                            // attempts returned above): a closed-like error on
                            // that path means the underlying wire (WebRTC *or*
                            // WebSocket) is no longer sendable and nothing
                            // entered a transport, so evicting the lane and
                            // re-dialing cannot duplicate a payload.
                            if let Err(e) = attempt.result.as_ref() {
                                if e.is_closed_like() && attempt.proven_pre_send {
                                    tracing::warn!(
                                        "♻️ lane closed-like error for {:?} ({:?}), invalidating lane and retrying once: {}",
                                        payload_type,
                                        conn_type,
                                        e,
                                    );
                                    conn.invalidate_lane(payload_type).await;
                                    match conn.get_lane(payload_type).await {
                                        Ok(new_lane) => {
                                            if let Some(cell) = progress_sink {
                                                mark_delivery_write_in_flight(cell);
                                            }
                                            let retry = new_lane.send_with_progress(payload).await;
                                            let retry_progress = retry.progress;
                                            if let Some(cell) = progress_sink {
                                                record_delivery_progress(cell, retry_progress);
                                            }
                                            return match retry.result {
                                                Ok(()) => DestSendAttempt::submitted(wire_identity),
                                                Err(NetworkError::SecureChannelNotReady(_)) => {
                                                    // Secure epoch rotated again
                                                    // mid-retry: wait for a
                                                    // fresh lane; the wire is
                                                    // still healthy.
                                                    conn.invalidate_lane(payload_type).await;
                                                    continue 'send;
                                                }
                                                Err(error)
                                                    if retry_progress
                                                        == DeliveryProgress::Unknown =>
                                                {
                                                    DestSendAttempt::failed_with_proof(
                                                        error,
                                                        retry_progress,
                                                        retry.proven_pre_send,
                                                    )
                                                }
                                                Err(error)
                                                    if error.is_closed_like()
                                                        && retry.proven_pre_send =>
                                                {
                                                    self.evict_closed_like_connection(
                                                        &conn,
                                                        conn_type,
                                                        payload_type,
                                                        "retry send",
                                                    )
                                                    .await;
                                                    continue 'send;
                                                }
                                                Err(error) => DestSendAttempt::failed_with_proof(
                                                    error,
                                                    retry_progress,
                                                    retry.proven_pre_send,
                                                ),
                                            };
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
                                            return DestSendAttempt::failed_pre_send(retry_err);
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

                            return match attempt.result {
                                Ok(()) => DestSendAttempt::submitted(wire_identity),
                                Err(error) => DestSendAttempt::failed_with_proof(
                                    error,
                                    progress,
                                    attempt.proven_pre_send,
                                ),
                            };
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
                return DestSendAttempt::failed_pre_send(NetworkError::ChannelClosed(
                    "connection manager closed".into(),
                ));
            }

            if !self
                .conn_mgr
                .has_live_candidate(&candidate_conn_types)
                .await
            {
                return DestSendAttempt::failed(
                    NetworkError::NoRoute(format!(
                        "all transport candidates exhausted for {payload_type:?}: {candidate_conn_types:?}"
                    )),
                    DeliveryProgress::NotAttempted,
                );
            }

            // Event-driven wait!
            if conn_watcher.changed().await.is_err() {
                return DestSendAttempt::failed_pre_send(NetworkError::ChannelClosed(
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

    /// Close one connection type without affecting the other fallback wire.
    #[allow(dead_code)]
    pub(crate) async fn close_connection(&self, conn_type: ConnType) -> NetworkResult<()> {
        if let Some(connection) = self.conn_mgr.get_connection(conn_type).await {
            if let Err(error) = connection.close().await {
                tracing::warn!("Failed to close {:?} connection: {}", conn_type, error);
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

    /// Retire a connection flight after the WebRTC Controller has already
    /// entered terminal cleanup. The Controller owns and closes the physical
    /// Session/PeerConnection; invoking `WireHandle::close()` here would feed
    /// an explicit manager close back into that in-flight close barrier. Close
    /// fallback wires, detach the terminal WebRTC slot, and stop all pool work.
    pub(crate) async fn retire_terminal_webrtc_flight(&self) -> NetworkResult<()> {
        let mut first_error = None;
        if let Some(connection) = self.conn_mgr.get_connection(ConnType::WebSocket).await {
            if let Err(error) = connection.close().await {
                first_error = Some(error);
            }
            self.conn_mgr
                .mark_connection_closed(ConnType::WebSocket)
                .await;
        }
        // The matching Controller retirement edge guarantees that every
        // physical WebRTC generation has its own CloseSession effect/barrier.
        // This layer owns only the ready-slot/cache lifetime.
        self.conn_mgr.mark_connection_closed(ConnType::WebRTC).await;
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

    /// The verified Actrium context of this destination's wire, if Actor
    /// mutual authentication has completed on it.
    ///
    /// The reliable route is only pinned by the first committed send, so a
    /// pre-traffic Attach must also consider ready wires the pool already
    /// holds; otherwise a fresh session could never bootstrap.
    pub(crate) async fn verified_peer_context(
        &self,
    ) -> Option<Arc<crate::transport::VerifiedPeerContext>> {
        let route = self.reliable.lock().await;
        if let Some(context) = route
            .current
            .as_ref()
            .and_then(|wire| wire.verified_peer_context())
        {
            return Some(context);
        }
        drop(route);
        for conn_type in [ConnType::WebSocket, ConnType::WebRTC] {
            if let Some(wire) = self.conn_mgr.get_connection(conn_type).await
                && let Some(context) = wire.verified_peer_context()
            {
                return Some(context);
            }
        }
        None
    }

    pub(crate) async fn adopt_ready(&self, connection: Arc<dyn WireHandle>) {
        self.conn_mgr.adopt_ready(connection).await;
    }

    /// Whether at least one wire can still become or remain ready.
    pub(crate) async fn has_live_connection_candidate(&self) -> bool {
        self.conn_mgr
            .has_live_candidate(&[ConnType::WebSocket, ConnType::WebRTC])
            .await
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

    /// Whether the ready WebRTC wire belongs to the exact logical transport
    /// attempt that reached terminal `Idle`.
    pub(crate) async fn matches_webrtc_transport_lifecycle(
        &self,
        expected: &WebRtcTransportLifecycle,
    ) -> bool {
        self.conn_mgr
            .connection_matches_webrtc_lifecycle(expected)
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
