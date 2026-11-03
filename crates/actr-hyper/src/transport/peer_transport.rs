//! PeerTransport - Cross-process transport manager
//!
//! Manages transport layer for multiple Dests, providing unified send/recv interface
//!
//! # Naming Convention
//! - **PeerTransport**: Manages cross-process communication (WebRTC, WebSocket)
//! - **HostTransport**: Manages intra-process communication (mpsc channels)
//!
//! These two form a symmetric design, handling different transport scenarios

use super::Dest; // Re-exported from actr-framework
use super::dest_transport::DestTransport;
use super::error::{NetworkError, NetworkResult};
use super::handover::HandoverBook;
use super::lane::{DeliveryProgress, DeliveryProgressCell};
use super::route_table::PayloadTypeExt;
use super::wire_handle::{WireHandle, WireIdentity};
use super::wire_pool::ReadySet;
use actr_protocol::{ActrId, PayloadType};
use async_trait::async_trait;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
#[cfg(test)]
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::{Mutex, Notify, RwLock, watch};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

const PEER_TRANSPORT_CREATE_TIMEOUT: Duration = Duration::from_secs(30);

/// Wire builder trait: asynchronously creates Wire components based on Dest
///
/// Implement this trait to customize Wire layer component creation logic (e.g., WebRTC, WebSocket)
#[async_trait]
pub trait WireBuilder: Send + Sync {
    /// Create Wire handle list to specified Dest
    ///
    /// # Arguments
    /// - `dest`: Target destination
    ///
    /// # Returns
    /// - Wire handle list (may contain multiple types: WebSocket, WebRTC, etc.)
    async fn create_connections(&self, dest: &Dest) -> NetworkResult<Vec<Arc<dyn WireHandle>>>;

    /// Return physical connections that already exist outside `PeerTransport`.
    ///
    /// Implementations must not dial, negotiate, or otherwise create a new
    /// physical connection in this method. The default implementation reports
    /// no reusable connection. This hook lets an inbound WebRTC session be
    /// materialized as a `DestTransport` without weakening `ExistingOnly`
    /// admission into a check-then-create race.
    async fn existing_connections(&self, _dest: &Dest) -> NetworkResult<Vec<Arc<dyn WireHandle>>> {
        Ok(Vec::new())
    }

    /// Create Wire handle list with cancellation support
    ///
    /// # Arguments
    /// - `dest`: Target destination
    /// - `cancel_token`: Optional cancellation token to terminate the operation
    ///
    /// # Returns
    /// - Wire handle list (may contain multiple types: WebSocket, WebRTC, etc.)
    /// - Returns error if cancelled
    ///
    /// Default implementation ignores the cancel token and calls `create_connections`.
    async fn create_connections_with_cancel(
        &self,
        dest: &Dest,
        cancel_token: Option<CancellationToken>,
    ) -> NetworkResult<Vec<Arc<dyn WireHandle>>> {
        // Check if already cancelled
        if let Some(ref token) = cancel_token {
            if token.is_cancelled() {
                return Err(NetworkError::ConnectionClosed(
                    "Connection creation cancelled".to_string(),
                ));
            }
        }

        // Default: just call create_connections
        self.create_connections(dest).await
    }
}

struct ConnectionFlight {
    completion: Notify,
    cancel: CancellationToken,
    deadline: Instant,
}

impl ConnectionFlight {
    fn new() -> Self {
        Self {
            completion: Notify::new(),
            cancel: CancellationToken::new(),
            deadline: Instant::now() + PEER_TRANSPORT_CREATE_TIMEOUT,
        }
    }
}

#[derive(Clone)]
enum DestState {
    Connecting(Arc<ConnectionFlight>),
    Connected(Arc<DestTransport>),
}

/// Cancellation guard for the task that owns a connection flight.
///
/// `get_or_create_transport` is commonly wrapped in a caller deadline. If that
/// caller future is dropped while it owns the flight, ordinary async cleanup
/// cannot run and the destination would otherwise remain stuck in
/// `Connecting` until every waiter times out. The guard removes only its exact
/// generation, then wakes waiters so a replacement flight can start
/// immediately.
struct ConnectionCreatorGuard {
    transports: Arc<RwLock<HashMap<Dest, DestState>>>,
    transports_revision: watch::Sender<u64>,
    dest: Dest,
    flight: Arc<ConnectionFlight>,
    armed: bool,
}

impl ConnectionCreatorGuard {
    fn new(
        transports: Arc<RwLock<HashMap<Dest, DestState>>>,
        transports_revision: watch::Sender<u64>,
        dest: Dest,
        flight: Arc<ConnectionFlight>,
    ) -> Self {
        Self {
            transports,
            transports_revision,
            dest,
            flight,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for ConnectionCreatorGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }

        self.flight.cancel.cancel();
        let transports = Arc::clone(&self.transports);
        let dest = self.dest.clone();
        let flight = Arc::clone(&self.flight);
        let revision = self.transports_revision.clone();
        tokio::spawn(async move {
            let mut transports = transports.write().await;
            let owns_flight = matches!(
                transports.get(&dest),
                Some(DestState::Connecting(current)) if Arc::ptr_eq(current, &flight)
            );
            if owns_flight {
                transports.remove(&dest);
                revision.send_modify(|rev| *rev += 1);
            }
            drop(transports);
            flight.completion.notify_waiters();
        });
    }
}

/// Owns cleanup rights for a fully-created transport until its flight commits.
/// This covers cancellation while awaiting the final destination-map lock.
struct CreatedDestTransportGuard {
    dest: Dest,
    transport: Option<Arc<DestTransport>>,
}

impl CreatedDestTransportGuard {
    fn new(dest: Dest, transport: Arc<DestTransport>) -> Self {
        Self {
            dest,
            transport: Some(transport),
        }
    }

    fn transport(&self) -> &Arc<DestTransport> {
        self.transport
            .as_ref()
            .expect("created transport guard must be armed")
    }

    fn disarm(&mut self) {
        self.transport = None;
    }

    async fn close_now(&mut self) -> NetworkResult<()> {
        let transport = Arc::clone(self.transport());
        let result = transport.close().await;
        self.transport = None;
        result
    }
}

impl Drop for CreatedDestTransportGuard {
    fn drop(&mut self) {
        let Some(transport) = self.transport.take() else {
            return;
        };
        let dest = self.dest.clone();
        tokio::spawn(async move {
            if let Err(error) = transport.close().await {
                tracing::warn!(
                    "Failed to close transport abandoned by cancelled creator for {:?}: {}",
                    dest,
                    error
                );
            }
        });
    }
}

/// Identity information for a send through a selected DestTransport.
///
/// `wire_identity` is `Some` once the send has resolved on a wire carrying a
/// session identity (WebRTC or WebSocket). It stays `None` when the send state
/// was recorded at transport-selection time and the send has not resolved, or
/// when the wire has no session identity (fake test wires).
#[derive(Clone)]
pub(crate) struct DestTransportRef {
    pub(crate) wire_identity: Option<WireIdentity>,
}

/// A peer send result paired with the progress observed by the destination
/// transport.  The generic payload keeps normal and `ExistingOnly` paths
/// distinct without exposing route identity to public callers.
pub(crate) struct PeerSendAttempt<T> {
    pub(crate) result: NetworkResult<T>,
    pub(crate) progress: DeliveryProgress,
    pub(crate) proven_pre_send: bool,
}

impl DestTransportRef {
    fn new(wire_identity: Option<WireIdentity>) -> Self {
        Self { wire_identity }
    }
}

/// Shared slot recording whether an in-flight send selected a `DestTransport`.
///
/// The send path writes the selection before the wire send can hang, then
/// records the wire identity once the send resolves. `None` means transport
/// selection did not complete; a reference with no wire identity means the
/// selected send has not resolved or the wire exposes no session identity.
pub(crate) type SentTransportSlot = Arc<Mutex<Option<DestTransportRef>>>;

/// PeerTransport - Cross-process transport manager
///
/// Responsibilities:
/// - Manage transport layer for multiple Dests (each Dest maps to one DestTransport)
/// - Create DestTransport on-demand (lazy initialization)
/// - Provide unified send/recv interface
/// - Support custom connection factories
/// - Prevent duplicate connection creation using an explicit per-destination flight
///
/// # Comparison with HostTransport
/// - **PeerTransport**: Cross-process, uses WebRTC/WebSocket
/// - **HostTransport**: Intra-process, uses mpsc channels, zero serialization
///
/// # State Machine
/// ```text
/// None -> Connecting(Flight) -> Connected(Transport)
///         |                      |
///      (multiple waiters)     (ready)
/// ```
pub struct PeerTransport {
    handover: Arc<HandoverBook>,
    /// Local Actor ID
    #[allow(dead_code)]
    local_id: ActrId,

    /// Dest -> lifecycle state
    transports: Arc<RwLock<HashMap<Dest, DestState>>>,

    /// Wire builder (used to create Wire handles for new DestTransport)
    conn_factory: Arc<dyn WireBuilder>,

    /// Generation counter bumped on every destination lifecycle transition
    /// (`Connecting -> Connected`, including fallback materializations).
    /// Readiness waiters that hold no per-destination ready-set yet (the
    /// destination did not exist when they parked) subscribe to this so a
    /// fallback that becomes ready wakes them instead of parking until the
    /// operation deadline.
    dest_state_epoch: watch::Sender<u64>,

    #[allow(unused)]
    /// todo: Set of peers currently being closed (to reject new connection attempts) ,closed requests will be cleaned up in event listener
    #[allow(unused)]
    closing_peers: Arc<RwLock<HashSet<Dest>>>,

    /// Revision of the `transports` map, bumped inside every mutation.
    /// `wait_until_route_ready` subscribers that found no destination
    /// ready-set watch this instead of parking forever, so a later
    /// materialization (another path committing a `DestTransport`) wakes
    /// them to re-run the lookup.
    transports_revision: watch::Sender<u64>,

    #[cfg(test)]
    dest_transport_creation_barrier: OnceLock<Arc<tokio::sync::Barrier>>,
    #[cfg(test)]
    final_commit_barrier: OnceLock<Arc<tokio::sync::Barrier>>,
}

impl PeerTransport {
    pub(crate) async fn adopt_relay(&self, peer: ActrId, wire: Arc<dyn WireHandle>) {
        let transport = {
            let states = self.transports.read().await;
            match states.get(&Dest::Peer(peer)) {
                Some(DestState::Connected(transport)) => Some(transport.clone()),
                _ => None,
            }
        };
        if let Some(transport) = transport {
            transport.adopt_ready(wire).await;
        }
    }
    /// Subscribe to the `transports`-map revision. Every insert, commit, or
    /// removal increments it; readiness waiters use it as the wake source for
    /// destination materialization itself.
    pub(crate) fn watch_transports_revision(&self) -> watch::Receiver<u64> {
        self.transports_revision.subscribe()
    }

    /// Wake destination-materialization waiters. Call inside the same
    /// critical section as the map mutation.
    fn bump_transports_revision(&self) {
        self.transports_revision
            .send_modify(|revision| *revision += 1);
    }

    /// Create new PeerTransport
    ///
    /// # Arguments
    /// - `local_id`: Local Actor ID
    /// - `conn_factory`: Wire builder, asynchronously creates Wire handle list based on Dest
    pub fn new(local_id: ActrId, conn_factory: Arc<dyn WireBuilder>) -> Self {
        Self {
            handover: Arc::new(HandoverBook::default()),
            local_id,
            transports: Arc::new(RwLock::new(HashMap::new())),
            conn_factory,
            dest_state_epoch: watch::channel(0).0,
            closing_peers: Arc::new(RwLock::new(HashSet::new())),
            transports_revision: watch::channel(0).0,
            #[cfg(test)]
            dest_transport_creation_barrier: OnceLock::new(),
            #[cfg(test)]
            final_commit_barrier: OnceLock::new(),
        }
    }

    pub(crate) fn with_handover(mut self, handover: Arc<HandoverBook>) -> Self {
        self.handover = handover;
        self
    }

    /// Subscribe to destination materialization transitions. A Connected
    /// `DestTransport` bumps this watch; flight insertion and cancellation or
    /// removal bump `watch_transports_revision()` instead. Callers waiting for
    /// a destination to become usable must subscribe to both watches and
    /// re-run their lookup after either one changes.
    pub(crate) fn watch_dest_state(&self) -> watch::Receiver<u64> {
        self.dest_state_epoch.subscribe()
    }

    /// Announce that `dest` has become `Connected`.
    fn announce_dest_connected(&self) {
        self.dest_state_epoch
            .send_modify(|generation| *generation += 1);
    }

    #[cfg(test)]
    fn set_dest_transport_creation_barrier(&self, barrier: Arc<tokio::sync::Barrier>) {
        self.dest_transport_creation_barrier
            .set(barrier)
            .expect("destination creation barrier should only be installed once");
    }

    #[cfg(test)]
    fn set_final_commit_barrier(&self, barrier: Arc<tokio::sync::Barrier>) {
        self.final_commit_barrier
            .set(barrier)
            .expect("final commit barrier should only be installed once");
    }

    /// Check if a destination is currently being closed
    #[allow(dead_code)]
    pub async fn is_closing(&self, dest: &Dest) -> bool {
        self.closing_peers.read().await.contains(dest)
    }

    /// Check whether a destination is currently in the Connecting state.
    ///
    /// Only test code reads this (the production callers went away with the
    /// 0.14 gate listener); kept as a small introspection API.
    #[allow(dead_code)]
    pub async fn is_connecting(&self, dest: &Dest) -> bool {
        let transports = self.transports.read().await;
        matches!(transports.get(dest), Some(DestState::Connecting(_)))
    }

    /// Whether this manager already holds an outbound materialization for
    /// `dest` — either a flight still in progress or a committed transport.
    ///
    /// This is an *admission* predicate, not a readiness one. A destination
    /// flips from `Connecting` to `Connected` the moment a `DestTransport` is
    /// committed, which happens before the wire is ready to carry business
    /// traffic. Concurrent senders that arrive inside that window must be
    /// allowed to join the same materialization and wait on its ready
    /// watcher; distinguishing the two states here would reject them for
    /// having lost the creation race. A destination being torn down is
    /// excluded so explicit teardown keeps failing fast.
    #[allow(dead_code)]
    pub async fn has_outbound_materialization(&self, dest: &Dest) -> bool {
        let closing = self.closing_peers.read().await.contains(dest);
        if closing {
            return false;
        }
        self.transports.read().await.contains_key(dest)
    }

    /// Whether an already-materialized destination currently has a non-WebRTC
    /// fallback route that can carry `payload_type`.  WebRTC is deliberately
    /// excluded: its `ReadySet` entry reflects the physical wire, while the
    /// RTC controller separately owns peer sendability and may be in
    /// `Recovering` even when the physical WebRTC wire is still marked ready.
    /// This is a read-only check used by the readiness waiter; it never creates
    /// a connection or a lane.
    pub(crate) async fn has_ready_fallback_connection_for_payload(
        &self,
        dest: &Dest,
        payload_type: PayloadType,
    ) -> bool {
        let transport = {
            let transports = self.transports.read().await;
            match transports.get(dest) {
                Some(DestState::Connected(transport)) => Some(Arc::clone(transport)),
                _ => None,
            }
        };
        let Some(transport) = transport else {
            return false;
        };
        let ready = transport.watch_ready().borrow().clone();
        // The fallback question is "could a plain (non-WebRTC) lane of this
        // payload ride the already-ready WebSocket". WebSocket readiness is
        // per-transport, not per-lane, so the per-lane term only decides
        // whether the payload can use non-WebRTC lanes at all.
        let can_use_plain_lane = payload_type
            .data_lane_types()
            .iter()
            .any(|lane| !lane.needs_webrtc());
        can_use_plain_lane && ready.contains(&super::wire_pool::ConnType::WebSocket)
    }

    /// Subscribe to the ready-set of an already-materialized destination.
    /// `None` means the first send still needs to create its destination
    /// transport, so the caller should wait on the other authoritative hints.
    pub(crate) async fn watch_ready_for(&self, dest: &Dest) -> Option<watch::Receiver<ReadySet>> {
        let transport = {
            let transports = self.transports.read().await;
            match transports.get(dest) {
                Some(DestState::Connected(transport)) => Some(Arc::clone(transport)),
                _ => None,
            }
        }?;
        Some(transport.watch_ready())
    }

    /// Get or create DestTransport for specified Dest
    ///
    /// # Arguments
    /// - `dest`: Target destination
    ///
    /// # Returns
    /// - DestTransport for this Dest (Arc-shared)
    ///
    /// # State Machine
    /// Uses a destination-scoped flight to prevent duplicate connections:
    /// 1. If Connected -> return transport
    /// 2. If Connecting -> wait for notify, then retry
    /// 3. If None -> insert Connecting(notify), create connection outside lock
    #[cfg_attr(feature = "opentelemetry", tracing::instrument(
        skip_all,
        name = "PeerTransport.get_or_create_transport",
        fields(dest = ?dest.as_peer_id().map(|id| id))
    ))]
    pub(crate) async fn get_or_create_transport(
        &self,
        dest: &Dest,
    ) -> NetworkResult<Arc<DestTransport>> {
        loop {
            if self.closing_peers.read().await.contains(dest) {
                return Err(NetworkError::ConnectionClosed(format!(
                    "Destination {:?} is being closed.",
                    dest
                )));
            }

            let (flight, is_creator) = {
                let mut transports = self.transports.write().await;
                match transports.get(dest) {
                    Some(DestState::Connected(transport)) => {
                        tracing::debug!("Reusing existing DestTransport: {:?}", dest);
                        return Ok(Arc::clone(transport));
                    }
                    Some(DestState::Connecting(flight)) => (Arc::clone(flight), false),
                    None => {
                        if self.closing_peers.read().await.contains(dest) {
                            return Err(NetworkError::ConnectionClosed(format!(
                                "Destination {:?} is being closed",
                                dest
                            )));
                        }
                        let flight = Arc::new(ConnectionFlight::new());
                        transports.insert(dest.clone(), DestState::Connecting(Arc::clone(&flight)));
                        self.bump_transports_revision();
                        tracing::debug!("Started connection flight for: {:?}", dest);
                        (flight, true)
                    }
                }
            };

            if !is_creator {
                tracing::debug!("Another thread is creating connection: {:?}", dest);
                let completed = flight.completion.notified();
                tokio::pin!(completed);
                completed.as_mut().enable();

                let still_current = {
                    let transports = self.transports.read().await;
                    matches!(
                        transports.get(dest),
                        Some(DestState::Connecting(current))
                            if Arc::ptr_eq(current, &flight)
                    )
                };
                if !still_current {
                    continue;
                }

                crate::timer::timeout_at(
                    crate::timer::ids::PEER_TRANSPORT_CREATE,
                    flight.deadline,
                    &mut completed,
                )
                .await
                .map_err(|error| {
                    NetworkError::TimeoutError(format!(
                        "Timeout waiting for connection flight: {:?} {}",
                        dest, error
                    ))
                })?;
                continue;
            }

            tracing::info!("Creating new connection for: {:?}", dest);
            let mut creator_guard = ConnectionCreatorGuard::new(
                Arc::clone(&self.transports),
                self.transports_revision.clone(),
                dest.clone(),
                Arc::clone(&flight),
            );
            let cancel_token = flight.cancel.clone();

            let result = crate::timer::timeout_at(
                crate::timer::ids::PEER_TRANSPORT_CREATE,
                flight.deadline,
                async {
                let connections = self
                    .conn_factory
                    .create_connections_with_cancel(dest, Some(cancel_token.clone()))
                    .await?;

                if cancel_token.is_cancelled() {
                    for conn in connections {
                        if let Err(e) = conn.close().await {
                            tracing::warn!(
                                "Failed to close connection created after cancellation for {:?}: {}",
                                dest,
                                e
                            );
                        }
                    }
                    return Err(NetworkError::ConnectionClosed(
                        "Connection creation cancelled".to_string(),
                    ));
                }

                if connections.is_empty() {
                    return Err(NetworkError::ConfigurationError(format!(
                        "Connection factory returned no connections: {dest:?}"
                    )));
                }

                tracing::info!(
                    "Creating DestTransport: {:?} ({} connections)",
                    dest,
                    connections.len()
                );
                #[cfg(test)]
                let mut transport = DestTransport::new_with_creation_barrier(
                    dest.clone(),
                    connections,
                    self.dest_transport_creation_barrier.get().cloned(),
                )
                .await?;
                #[cfg(not(test))]
                let mut transport = DestTransport::new(dest.clone(), connections).await?;
                transport.set_handover(self.handover.clone());
                let mut transport_guard =
                    CreatedDestTransportGuard::new(dest.clone(), Arc::new(transport));

                if cancel_token.is_cancelled() {
                    if let Err(e) = transport_guard.close_now().await {
                        tracing::warn!(
                            "Failed to close DestTransport created after cancellation for {:?}: {}",
                            dest,
                            e
                        );
                    }
                    return Err(NetworkError::ConnectionClosed(
                        "Connection creation cancelled".to_string(),
                    ));
                }

                Ok(transport_guard)
                },
            )
            .await
            .unwrap_or_else(|error| {
                flight.cancel.cancel();
                Err(NetworkError::TimeoutError(format!(
                    "Connection flight deadline expired for {dest:?}: {error}"
                )))
            });

            #[cfg(test)]
            if let Some(barrier) = self.final_commit_barrier.get() {
                barrier.wait().await;
                barrier.wait().await;
            }

            let mut transports = self.transports.write().await;
            let owns_flight = matches!(
                transports.get(dest),
                Some(DestState::Connecting(current)) if Arc::ptr_eq(current, &flight)
            );
            if !owns_flight {
                drop(transports);
                if let Ok(mut transport_guard) = result {
                    if let Err(error) = transport_guard.close_now().await {
                        tracing::warn!(
                            "Failed to close stale DestTransport for {:?}: {}",
                            dest,
                            error
                        );
                    }
                }
                creator_guard.disarm();
                flight.completion.notify_waiters();
                return Err(NetworkError::ConnectionClosed(format!(
                    "Connection flight for {:?} was superseded",
                    dest
                )));
            }

            match result {
                Ok(mut transport_guard) => {
                    let transport = Arc::clone(transport_guard.transport());
                    tracing::info!("Connection established: {:?}", dest);
                    transports.insert(dest.clone(), DestState::Connected(Arc::clone(&transport)));
                    self.bump_transports_revision();
                    drop(transports);
                    self.announce_dest_connected();
                    self.spawn_ready_monitor(dest.clone(), Arc::clone(&transport));
                    transport_guard.disarm();
                    creator_guard.disarm();
                    flight.completion.notify_waiters();
                    return Ok(transport);
                }
                Err(e) => {
                    tracing::error!("Connection failed: {:?}: {}", dest, e);
                    transports.remove(dest);
                    self.bump_transports_revision();
                    drop(transports);
                    creator_guard.disarm();
                    flight.completion.notify_waiters();
                    return Err(e);
                }
            }
        }
    }

    /// Send message to specified Dest
    ///
    /// # Arguments
    /// - `dest`: Target destination
    /// - `payload_type`: Message type
    /// - `data`: Message data
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// mgr.send(&dest, PayloadType::RpcSignal, b"hello").await?;
    /// ```
    #[allow(dead_code)] // Retained as the public compatibility wrapper.
    pub async fn send(
        &self,
        dest: &Dest,
        payload_type: PayloadType,
        data: &[u8],
    ) -> NetworkResult<()> {
        self.send_with_identity(dest, payload_type, data, None)
            .await
            .map(|_| ())
    }

    pub(crate) async fn send_with_identity(
        &self,
        dest: &Dest,
        payload_type: PayloadType,
        data: &[u8],
        record: Option<&SentTransportSlot>,
    ) -> NetworkResult<DestTransportRef> {
        self.send_with_progress(dest, payload_type, data, record, None)
            .await
            .result
    }

    /// Send through a destination transport and retain the lane's delivery
    /// progress for the operation owner.
    pub(crate) async fn send_with_progress(
        &self,
        dest: &Dest,
        payload_type: PayloadType,
        data: &[u8],
        record: Option<&SentTransportSlot>,
        progress_sink: Option<&DeliveryProgressCell>,
    ) -> PeerSendAttempt<DestTransportRef> {
        // Get or create DestTransport for this Dest
        let transport = match self.get_or_create_transport(dest).await {
            Ok(transport) => transport,
            Err(error) => {
                return PeerSendAttempt {
                    result: Err(error),
                    progress: DeliveryProgress::NotAttempted,
                    proven_pre_send: true,
                };
            }
        };

        // Record transport selection before the wire send so a cancelled
        // caller can distinguish an in-flight send from one that never
        // selected a transport.
        if let Some(slot) = record {
            *slot.lock().await = Some(DestTransportRef::new(None));
        }

        // Send through DestTransport
        let attempt = transport
            .send_with_progress(payload_type, data, progress_sink)
            .await;
        let progress = attempt.progress;
        let proven_pre_send = attempt.proven_pre_send;
        match attempt.result {
            Ok(wire_identity) => {
                let sent = DestTransportRef::new(wire_identity);
                if let Some(slot) = record {
                    *slot.lock().await = Some(sent.clone());
                }
                PeerSendAttempt {
                    result: Ok(sent),
                    progress,
                    proven_pre_send: false,
                }
            }
            Err(error) => PeerSendAttempt {
                result: Err(error),
                progress,
                proven_pre_send,
            },
        }
    }

    /// Send only through an existing physical connection.
    ///
    /// A lower layer may already own an inbound WebRTC session before this
    /// manager has a local `DestTransport`; that session is materialized on
    /// demand. `Ok(None)` means no reusable physical connection exists, another
    /// destination flight is active, or the destination is being closed.
    /// Unlike [`Self::send_with_identity`], this path never dials or negotiates
    /// a replacement connection.
    #[allow(dead_code)] // Compatibility wrapper retained for transport tests.
    pub(crate) async fn send_existing_with_identity(
        &self,
        dest: &Dest,
        payload_type: PayloadType,
        data: &[u8],
        record: Option<&SentTransportSlot>,
    ) -> NetworkResult<Option<DestTransportRef>> {
        self.send_existing_with_progress(dest, payload_type, data, record, None)
            .await
            .result
    }

    /// Existing-only variant of [`Self::send_with_progress`].
    pub(crate) async fn send_existing_with_progress(
        &self,
        dest: &Dest,
        payload_type: PayloadType,
        data: &[u8],
        record: Option<&SentTransportSlot>,
        progress_sink: Option<&DeliveryProgressCell>,
    ) -> PeerSendAttempt<Option<DestTransportRef>> {
        let transport = match self.get_or_materialize_existing_transport(dest).await {
            Ok(Some(transport)) => transport,
            Ok(None) => {
                return PeerSendAttempt {
                    result: Ok(None),
                    progress: DeliveryProgress::NotAttempted,
                    proven_pre_send: false,
                };
            }
            Err(error) => {
                return PeerSendAttempt {
                    result: Err(error),
                    progress: DeliveryProgress::NotAttempted,
                    proven_pre_send: true,
                };
            }
        };

        // Same selection-time record as `send_with_identity`: a hung wire send
        // must still leave a selected-send marker behind.
        if let Some(slot) = record {
            *slot.lock().await = Some(DestTransportRef::new(None));
        }

        let attempt = transport
            .send_with_progress(payload_type, data, progress_sink)
            .await;
        let progress = attempt.progress;
        let proven_pre_send = attempt.proven_pre_send;
        match attempt.result {
            Ok(wire_identity) => {
                let sent = DestTransportRef::new(wire_identity);
                if let Some(slot) = record {
                    *slot.lock().await = Some(sent.clone());
                }
                PeerSendAttempt {
                    result: Ok(Some(sent)),
                    progress,
                    proven_pre_send: false,
                }
            }
            Err(error) => PeerSendAttempt {
                result: Err(error),
                progress,
                proven_pre_send,
            },
        }
    }

    /// Return a committed destination transport, or materialize one around a
    /// physical connection already owned by a lower transport layer.
    ///
    /// This method may create local routing state, but it never requests the
    /// builder to establish a new physical connection. A short-lived flight
    /// serializes the materialization with normal creation and close paths.
    /// Existing-only callers never wait on somebody else's flight.
    async fn get_or_materialize_existing_transport(
        &self,
        dest: &Dest,
    ) -> NetworkResult<Option<Arc<DestTransport>>> {
        if self.closing_peers.read().await.contains(dest) {
            return Ok(None);
        }

        let flight = {
            let mut transports = self.transports.write().await;
            match transports.get(dest) {
                Some(DestState::Connected(transport)) => {
                    return Ok(Some(Arc::clone(transport)));
                }
                Some(DestState::Connecting(_)) => return Ok(None),
                None => {
                    if self.closing_peers.read().await.contains(dest) {
                        return Ok(None);
                    }
                    let flight = Arc::new(ConnectionFlight::new());
                    transports.insert(dest.clone(), DestState::Connecting(Arc::clone(&flight)));
                    self.bump_transports_revision();
                    flight
                }
            }
        };

        let mut creator_guard = ConnectionCreatorGuard::new(
            Arc::clone(&self.transports),
            self.transports_revision.clone(),
            dest.clone(),
            Arc::clone(&flight),
        );
        let cancel_token = flight.cancel.clone();

        let result = async {
            let connections = self.conn_factory.existing_connections(dest).await?;

            if cancel_token.is_cancelled() {
                for connection in connections {
                    if let Err(error) = connection.close().await {
                        tracing::warn!(
                            "Failed to close reusable connection after cancellation for {:?}: {}",
                            dest,
                            error
                        );
                    }
                }
                return Ok(None);
            }

            if connections.is_empty() {
                return Ok(None);
            }

            #[cfg(test)]
            let mut transport = DestTransport::new_with_creation_barrier(
                dest.clone(),
                connections,
                self.dest_transport_creation_barrier.get().cloned(),
            )
            .await?;
            #[cfg(not(test))]
            let mut transport = DestTransport::new(dest.clone(), connections).await?;
            transport.set_handover(self.handover.clone());
            let mut transport_guard =
                CreatedDestTransportGuard::new(dest.clone(), Arc::new(transport));

            if cancel_token.is_cancelled() {
                if let Err(error) = transport_guard.close_now().await {
                    tracing::warn!(
                        "Failed to close reusable DestTransport after cancellation for {:?}: {}",
                        dest,
                        error
                    );
                }
                return Ok(None);
            }

            Ok(Some(transport_guard))
        }
        .await;

        #[cfg(test)]
        if let Some(barrier) = self.final_commit_barrier.get() {
            barrier.wait().await;
            barrier.wait().await;
        }

        let mut transports = self.transports.write().await;
        let owns_flight = matches!(
            transports.get(dest),
            Some(DestState::Connecting(current)) if Arc::ptr_eq(current, &flight)
        );
        if !owns_flight {
            drop(transports);
            if let Ok(Some(mut transport_guard)) = result
                && let Err(error) = transport_guard.close_now().await
            {
                tracing::warn!(
                    "Failed to close superseded reusable DestTransport for {:?}: {}",
                    dest,
                    error
                );
            }
            creator_guard.disarm();
            flight.completion.notify_waiters();
            return Ok(None);
        }

        match result {
            Ok(Some(mut transport_guard)) => {
                let transport = Arc::clone(transport_guard.transport());
                transports.insert(dest.clone(), DestState::Connected(Arc::clone(&transport)));
                self.bump_transports_revision();
                drop(transports);
                self.announce_dest_connected();
                self.spawn_ready_monitor(dest.clone(), Arc::clone(&transport));
                transport_guard.disarm();
                creator_guard.disarm();
                flight.completion.notify_waiters();
                Ok(Some(transport))
            }
            Ok(None) => {
                transports.remove(dest);
                self.bump_transports_revision();
                drop(transports);
                creator_guard.disarm();
                flight.completion.notify_waiters();
                Ok(None)
            }
            Err(error) => {
                transports.remove(dest);
                self.bump_transports_revision();
                drop(transports);
                creator_guard.disarm();
                flight.completion.notify_waiters();
                Err(error)
            }
        }
    }

    /// Close DestTransport for specified Dest (full teardown, including WebSocket).
    ///
    /// # Arguments
    /// - `dest`: Target destination
    #[allow(dead_code)]
    pub async fn close_transport(&self, dest: &Dest) -> NetworkResult<()> {
        // 1. Mark as closing
        self.closing_peers.write().await.insert(dest.clone());

        // 2. Atomically detach the current generation. A creator that completes
        // after this point fails its compare-and-commit check and closes the
        // stale transport it produced.
        let current_state = {
            let mut transports = self.transports.write().await;
            let removed = transports.remove(dest);
            self.bump_transports_revision();
            removed
        };

        let close_result = match current_state {
            Some(DestState::Connecting(flight)) => {
                tracing::info!("Cancelling in-progress connection for {:?}", dest);
                flight.cancel.cancel();
                flight.completion.notify_waiters();
                Ok(())
            }
            Some(DestState::Connected(transport)) => {
                tracing::info!("Closing DestTransport: {:?}", dest);
                transport.close().await
            }
            None => {
                tracing::debug!(
                    "Ignoring close request for {:?}; no transport state exists",
                    dest
                );
                Ok(())
            }
        };

        // 5. Remove from closing set after cleanup completes
        self.closing_peers.write().await.remove(dest);

        close_result
    }

    /// Session-guarded close: only tear down the transport if the active
    /// WebRTC wire still matches the given `(peer_id, session_id)` pair.
    ///
    /// Returns `Ok(true)` when the transport was actually closed.
    /// Returns `Ok(false)` when the event is stale (identity mismatch)
    /// or the transport state has already changed — in that case **no**
    /// pending requests should be cleaned, and the peer should be removed
    /// from `closing_peers` immediately.
    pub(crate) async fn close_transport_if_webrtc_session(
        &self,
        dest: &Dest,
        peer_id: &ActrId,
        session_id: u64,
    ) -> NetworkResult<bool> {
        // 1. Clone the current transport without awaiting under the map lock.
        let transport = {
            let transports = self.transports.read().await;
            match transports.get(dest) {
                Some(DestState::Connected(transport)) => Some(Arc::clone(transport)),
                _ => None, // Connecting or absent → stale
            }
        };

        let Some(transport) = transport else {
            tracing::debug!(
                "Stale close event for {:?} (session {} mismatch or transport absent)",
                dest,
                session_id
            );
            return Ok(false);
        };

        if !transport.matches_webrtc_session(peer_id, session_id).await {
            tracing::debug!(
                "Stale close event for {:?} (session {} mismatch or transport absent)",
                dest,
                session_id
            );
            return Ok(false);
        }

        // 2. Identity matches → proceed with the normal close path.
        self.close_transport(dest).await?;
        Ok(true)
    }

    /// Retire a terminal WebRTC connection flight only when the currently
    /// cached `DestTransport` still contains the exact Controller/attempt
    /// lifecycle that emitted the event. The final detach is instance-CAS
    /// guarded so a concurrent replacement cannot be closed after matching.
    pub(crate) async fn close_transport_if_webrtc_lifecycle(
        &self,
        dest: &Dest,
        expected: &super::wire_handle::WebRtcTransportLifecycle,
    ) -> NetworkResult<bool> {
        let transport = {
            let transports = self.transports.read().await;
            match transports.get(dest) {
                Some(DestState::Connected(transport)) => Some(Arc::clone(transport)),
                _ => None,
            }
        };
        let Some(transport) = transport else {
            return Ok(false);
        };
        if !transport.matches_webrtc_transport_lifecycle(expected).await {
            return Ok(false);
        }

        self.closing_peers.write().await.insert(dest.clone());
        let detached = {
            let mut transports = self.transports.write().await;
            let detached = match transports.get(dest) {
                Some(DestState::Connected(current)) if Arc::ptr_eq(current, &transport) => {
                    match transports.remove(dest) {
                        Some(DestState::Connected(current)) => Some(current),
                        _ => None,
                    }
                }
                _ => None,
            };
            if detached.is_some() {
                self.bump_transports_revision();
            }
            detached
        };
        let Some(detached) = detached else {
            self.closing_peers.write().await.remove(dest);
            return Ok(false);
        };

        // Physical WebRTC generations are closed by the Controller effects
        // that emitted this retirement edge. Closing the WireHandle again here
        // would recursively remove that Controller before its close barrier
        // finishes; this path only retires the transport/cache flight and any
        // fallback wire it owns.
        let result = detached.retire_terminal_webrtc_flight().await.map(|_| true);
        self.closing_peers.write().await.remove(dest);
        result
    }

    /// Close all DestTransports
    #[allow(dead_code)]
    pub async fn close_all(&self) -> NetworkResult<()> {
        let states = {
            let mut transports = self.transports.write().await;
            tracing::info!("Closing all DestTransports (count: {})", transports.len());
            let states = transports.drain().collect::<Vec<_>>();
            self.bump_transports_revision();
            states
        };

        for (dest, state) in states {
            match state {
                DestState::Connected(transport) => {
                    self.closing_peers.write().await.insert(dest.clone());
                    if let Err(e) = transport.close().await {
                        tracing::warn!("Failed to close DestTransport {:?}: {}", dest, e);
                    }
                    self.closing_peers.write().await.remove(&dest);
                }
                DestState::Connecting(flight) => {
                    self.closing_peers.write().await.insert(dest.clone());
                    tracing::debug!("Cancelled Connecting state for: {:?}", dest);
                    flight.cancel.cancel();
                    flight.completion.notify_waiters();
                    self.closing_peers.write().await.remove(&dest);
                }
            }
        }

        Ok(())
    }

    /// Get count of currently managed Dests
    #[cfg(feature = "test-utils")]
    pub async fn dest_count(&self) -> usize {
        self.transports.read().await.len()
    }

    /// Get local Actor ID
    #[inline]
    #[cfg(feature = "test-utils")]
    pub fn local_id(&self) -> &ActrId {
        &self.local_id
    }

    /// List all connected Dests
    #[cfg(feature = "test-utils")]
    pub async fn list_dests(&self) -> Vec<Dest> {
        self.transports.read().await.keys().cloned().collect()
    }

    /// Check if connection to specified Dest exists
    #[cfg(feature = "test-utils")]
    pub async fn has_dest(&self, dest: &Dest) -> bool {
        self.transports.read().await.contains_key(dest)
    }

    /// Monitor a DestTransport ready-set and remove it when every connection
    /// is terminal. This also covers an initial connection flight whose wires
    /// all fail before any one of them has ever reached `Ready`.
    fn spawn_ready_monitor(&self, dest: Dest, transport: Arc<DestTransport>) {
        let transports = Arc::clone(&self.transports);
        // Share the revision sender so the teardown removal below wakes
        // destination-less readiness waiters exactly like every other map
        // mutation; a closed ready-watch receiver alone is not an
        // authoritative edge a parked send can re-evaluate on.
        let revision = self.transports_revision.clone();
        tokio::spawn(async move {
            let mut rx = transport.watch_ready();

            loop {
                let ready = rx.borrow().clone();

                if ready.is_empty() && !transport.has_live_connection_candidate().await {
                    // Only remove if the same transport is still mapped.
                    let mut map = transports.write().await;
                    let matched = matches!(
                        map.get(&dest),
                        Some(DestState::Connected(existing)) if Arc::ptr_eq(existing, &transport)
                    );
                    if matched {
                        map.remove(&dest);
                        revision.send_modify(|rev| *rev += 1);
                        drop(map);

                        tracing::warn!(
                            "Removing DestTransport for {:?} after all connections closed",
                            dest
                        );
                        if let Err(e) = transport.close().await {
                            tracing::warn!("Failed to close DestTransport {:?}: {}", dest, e);
                        }
                    }
                    break;
                }

                if rx.changed().await.is_err() {
                    break;
                }
            }
        });
    }

    /// Spawn health checker background task with smart reconnect
    ///
    /// Periodically checks all DestTransport health status:
    /// - If some connections failed -> trigger smart reconnect (reuse working connections)
    /// - If all connections failed -> remove entire DestTransport
    ///
    /// # Arguments
    /// - `interval`: Health check interval (recommended: 10-30 seconds)
    ///
    /// # Returns
    /// - JoinHandle for the background task (can be used to cancel)
    ///
    /// # Example
    /// ```rust,ignore
    /// let mgr = Arc::new(PeerTransport::new(local_id, factory));
    /// let health_check_handle = mgr.spawn_health_checker(Duration::from_secs(10));
    /// ```
    #[cfg(feature = "test-utils")]
    pub fn spawn_health_checker(&self, interval: Duration) -> tokio::task::JoinHandle<()> {
        let transports = Arc::clone(&self.transports);
        let conn_factory = Arc::clone(&self.conn_factory);

        tokio::spawn(async move {
            let mut interval_timer =
                crate::timer::interval(crate::timer::ids::PEER_TRANSPORT_HEALTH, interval);
            interval_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                interval_timer.tick().await;

                // Collect snapshot of connected Dests first (no async under lock)
                let snapshot: Vec<(Dest, Arc<DestTransport>)> = {
                    let transports_read = transports.read().await;

                    transports_read
                        .iter()
                        .filter_map(|(dest, state)| {
                            // Only check Connected transports, skip Connecting
                            if let DestState::Connected(transport) = state {
                                Some((dest.clone(), Arc::clone(transport)))
                            } else {
                                None
                            }
                        })
                        .collect()
                };

                // Process each Dest outside of the lock
                for (dest_clone, transport) in snapshot {
                    let healthy = transport.has_healthy_connection().await;

                    if !healthy {
                        // All connections failed - schedule for removal
                        tracing::warn!("All connections failed for {:?}, will remove", dest_clone);

                        // Remove entire DestTransport
                        let mut transports_write = transports.write().await;
                        if let Some(DestState::Connected(transport)) =
                            transports_write.remove(&dest_clone)
                        {
                            tracing::info!(
                                "Removing completely failed DestTransport: {:?}",
                                dest_clone
                            );
                            // Drop lock before awaiting close
                            drop(transports_write);

                            if let Err(e) = transport.close().await {
                                tracing::warn!(
                                    "Failed to close DestTransport {:?}: {}",
                                    dest_clone,
                                    e
                                );
                            }
                        } else {
                            // State changed between snapshot and removal; skip safely
                            drop(transports_write);
                        }
                    } else {
                        // At least one connection is working
                        // Try to reconnect failed ones (smart reconnect)
                        tracing::debug!("Triggering smart reconnect for: {:?}", dest_clone);
                        if let Err(e) = transport
                            .retry_failed_connections(&dest_clone, conn_factory.as_ref())
                            .await
                        {
                            tracing::warn!("Smart reconnect failed for {:?}: {}", dest_clone, e);
                        }
                    }
                }
            }
        })
    }
}

impl Drop for PeerTransport {
    fn drop(&mut self) {
        tracing::debug!("PeerTransport dropped");
        // Note: async cleanup requires external call to close_all()
    }
}

#[cfg(test)]
#[path = "peer_transport_tests.rs"]
mod tests;
