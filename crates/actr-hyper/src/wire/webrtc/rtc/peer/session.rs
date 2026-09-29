//! `PeerConnectionSession` — owner of one PC generation's physical resources
//! (RFC §"PeerConnectionSession").
//!
//! A session bundles the object-safe session handle ([`SessionOps`]), every
//! DataChannel it owns (the *resource registry*), every DataChannel/Track pump
//! task for that generation, and the cancellation token + explicit close flow.
//! It owns resources only: it MUST NOT hold business hook state or any
//! cross-generation recovery policy (that lives in the controller).
//!
//! RFC §"Command and event mailboxes": "Raw resources are stored in the Session
//! registry first; the Controller receives only `SessionResourceId`, metadata,
//! and semantic state." The session is the sole owner of concrete
//! `Arc<dyn DataChannel>` objects — it stores them, spawns their pumps, and
//! closes them explicitly on shutdown (RFC work item 11: "Shutdown explicitly
//! calls each `DataChannel::close()`"). The controller only ever sees the
//! `LaneHandle` token this session returns.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use webrtc::error::Error as RtcError;
use webrtc::media_stream::track_local::TrackLocal;
use webrtc::media_stream::track_remote::TrackRemote;

use actr_protocol::ActrId;

use crate::inbound::MediaFrameRegistry;
use crate::wire::webrtc::rtc::inbound::InboundRouter;
use crate::wire::webrtc::rtc::media::TrackMeta;
use crate::wire::webrtc::rtc::status::LaneKind;

use super::adapter::LaneDataChannel;
use super::effects::SessionOps;
use super::lane_handle::LaneHandle;
use super::reducer::LaneEvent;

/// Opaque, generation-owned token for a raw DataChannel or Track.  The handler
/// stores the object before publishing this token; the Controller never sees
/// the WebRTC implementation object (RFC §Command and event mailboxes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct SessionResourceId(u64);

#[derive(Clone)]
enum SessionResource {
    DataChannel(Arc<dyn LaneDataChannel>),
    RemoteTrack(Arc<dyn TrackRemote>),
}

/// Thread-safe raw-resource ingress for one PC generation.  Handler callbacks
/// perform only a short insertion; acceptance/rejection is decided later by the
/// Controller and executed by the owning Session/runner.
#[derive(Clone)]
pub(crate) struct SessionResourceRegistry {
    next_id: Arc<AtomicU64>,
    resources: Arc<Mutex<HashMap<SessionResourceId, SessionResource>>>,
    normalization_tasks: Arc<Mutex<JoinSet<()>>>,
    normalization_cancellation: CancellationToken,
    closed: Arc<AtomicBool>,
    max_resources: usize,
}

impl Default for SessionResourceRegistry {
    fn default() -> Self {
        Self::with_limit(
            crate::wire::webrtc::rtc::config::RtcBufferConfig::default().max_session_resources,
        )
    }
}

impl SessionResourceRegistry {
    pub(crate) fn with_limit(max_resources: usize) -> Self {
        assert!(max_resources > 0, "Session resource limit must be non-zero");
        Self {
            next_id: Arc::new(AtomicU64::new(0)),
            resources: Arc::new(Mutex::new(HashMap::new())),
            normalization_tasks: Arc::new(Mutex::new(JoinSet::new())),
            normalization_cancellation: CancellationToken::new(),
            closed: Arc::new(AtomicBool::new(false)),
            max_resources,
        }
    }

    fn alloc_id(&self) -> SessionResourceId {
        SessionResourceId(self.next_id.fetch_add(1, Ordering::Relaxed).wrapping_add(1))
    }

    pub(crate) fn try_insert_data_channel(
        &self,
        channel: Arc<dyn LaneDataChannel>,
    ) -> Result<SessionResourceId, ()> {
        if self.closed.load(Ordering::Acquire) {
            return Err(());
        }
        let id = self.alloc_id();
        let mut resources = self
            .resources
            .lock()
            .expect("session resource registry lock");
        if self.closed.load(Ordering::Acquire) || resources.len() >= self.max_resources {
            return Err(());
        }
        resources.insert(id, SessionResource::DataChannel(channel));
        Ok(id)
    }

    pub(crate) fn try_insert_remote_track(
        &self,
        track: Arc<dyn TrackRemote>,
    ) -> Result<SessionResourceId, ()> {
        if self.closed.load(Ordering::Acquire) {
            return Err(());
        }
        let id = self.alloc_id();
        let mut resources = self
            .resources
            .lock()
            .expect("session resource registry lock");
        if self.closed.load(Ordering::Acquire) || resources.len() >= self.max_resources {
            return Err(());
        }
        resources.insert(id, SessionResource::RemoteTrack(track));
        Ok(id)
    }

    pub(crate) fn contains_data_channel(&self, id: SessionResourceId) -> bool {
        matches!(
            self.resources
                .lock()
                .expect("session resource registry lock")
                .get(&id),
            Some(SessionResource::DataChannel(_))
        )
    }

    pub(crate) fn contains_remote_track(&self, id: SessionResourceId) -> bool {
        matches!(
            self.resources
                .lock()
                .expect("session resource registry lock")
                .get(&id),
            Some(SessionResource::RemoteTrack(_))
        )
    }

    pub(crate) fn take_data_channel(
        &self,
        id: SessionResourceId,
    ) -> Option<Arc<dyn LaneDataChannel>> {
        let resource = self
            .resources
            .lock()
            .expect("session resource registry lock")
            .remove(&id);
        match resource {
            Some(SessionResource::DataChannel(channel)) => Some(channel),
            Some(other) => {
                self.resources
                    .lock()
                    .expect("session resource registry lock")
                    .insert(id, other);
                None
            }
            None => None,
        }
    }

    pub(crate) fn take_remote_track(&self, id: SessionResourceId) -> Option<Arc<dyn TrackRemote>> {
        let resource = self
            .resources
            .lock()
            .expect("session resource registry lock")
            .remove(&id);
        match resource {
            Some(SessionResource::RemoteTrack(track)) => Some(track),
            Some(other) => {
                self.resources
                    .lock()
                    .expect("session resource registry lock")
                    .insert(id, other);
                None
            }
            None => None,
        }
    }

    pub(crate) async fn discard(&self, id: SessionResourceId) -> bool {
        let resource = self
            .resources
            .lock()
            .expect("session resource registry lock")
            .remove(&id);
        match resource {
            Some(SessionResource::DataChannel(channel)) => {
                let _ = channel.close().await;
                true
            }
            Some(SessionResource::RemoteTrack(track)) => {
                track.stop().await;
                true
            }
            None => false,
        }
    }

    /// Spawn one finite resource-normalization task before `build()` returns.
    /// The task is generation-owned, cancellable, and joined by `close_all`, so
    /// Handler callbacks never create detached metadata futures.
    pub(crate) fn spawn_normalization<F>(&self, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        let cancellation = self.normalization_cancellation.clone();
        let mut tasks = self
            .normalization_tasks
            .lock()
            .expect("session normalization task lock");
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        tasks.spawn(async move {
            tokio::select! {
                _ = cancellation.cancelled() => {}
                _ = future => {}
            }
        });
    }

    pub(crate) async fn close_all(&self) {
        self.closed.store(true, Ordering::Release);
        self.normalization_cancellation.cancel();
        self.normalization_tasks
            .lock()
            .expect("session normalization task lock")
            .abort_all();
        loop {
            let converged = {
                let mut tasks = self
                    .normalization_tasks
                    .lock()
                    .expect("session normalization task lock");
                while let Some(result) = tasks.try_join_next() {
                    if let Err(error) = result
                        && !error.is_cancelled()
                    {
                        tracing::warn!(%error, "session resource normalization task failed");
                    }
                }
                tasks.is_empty()
            };
            if converged {
                break;
            }
            // Keep the JoinSet in the shared registry while waiting. If an
            // outer close deadline cancels this future, the next explicit
            // cleanup call can continue joining instead of losing handles.
            tokio::task::yield_now().await;
        }
        let resources = self
            .resources
            .lock()
            .expect("session resource registry lock")
            .iter()
            .map(|(id, resource)| (*id, resource.clone()))
            .collect::<Vec<_>>();
        for (id, resource) in resources {
            match resource {
                SessionResource::DataChannel(channel) => {
                    let _ = channel.close().await;
                }
                SessionResource::RemoteTrack(track) => track.stop().await,
            }
            self.resources
                .lock()
                .expect("session resource registry lock")
                .remove(&id);
        }
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.resources
            .lock()
            .expect("session resource registry lock")
            .len()
    }
}

/// The plumbing one lane's pump needs: the controller event channel and the
/// inbound data-plane router. Bundled so `register_channel`/`lane_pump`
/// stay under the argument-count lint.
pub(crate) struct LanePlumbing {
    pub(crate) peer_id: ActrId,
    pub(crate) lane_tx: mpsc::Sender<LaneEvent>,
    pub(crate) inbound: Arc<InboundRouter>,
    /// Only the Controller's exact sendable generation may dispatch inbound
    /// business traffic. A retired/recovering generation may still deliver a
    /// late DataChannel event while its asynchronous close is draining.
    pub(crate) inbound_generation: Arc<InboundGenerationGate>,
    pub(crate) reassembly_limits: crate::transport::ReassemblyLimits,
}

/// Generation-scoped admission for the inbound data plane.
///
/// `None` means that no generation is currently allowed to dispatch business
/// messages. A concrete generation is admitted only after its Controller has
/// published `sendable=true`. The watch channel lets a pump briefly wait for a
/// transient recovery to converge while still fencing a retired generation
/// from a later replacement.
#[derive(Clone)]
pub(crate) struct InboundGenerationGate {
    admitted: watch::Sender<Option<u64>>,
}

impl InboundGenerationGate {
    pub(crate) fn new() -> Self {
        let (admitted, _) = watch::channel(None);
        Self { admitted }
    }

    #[cfg(test)]
    pub(crate) fn with_generation(generation: u64) -> Self {
        let gate = Self::new();
        gate.set(Some(generation));
        gate
    }

    pub(crate) fn set(&self, generation: Option<u64>) {
        self.admitted.send_if_modified(|current| {
            if *current == generation {
                false
            } else {
                *current = generation;
                true
            }
        });
    }

    fn subscribe(&self) -> watch::Receiver<Option<u64>> {
        self.admitted.subscribe()
    }
}

#[derive(Clone, Copy)]
struct MessageAdmission {
    admitted: bool,
    first_seen: Instant,
}

/// Remembers the admission decision for fragmented messages so every
/// fragment follows the decision made for the first observed fragment. The
/// cache is bounded by the reassembly message budget and uses the same TTL as
/// the fragment buffer, so rejected or abandoned messages cannot grow it
/// without limit.
struct InboundAdmissionCache {
    entries: HashMap<u32, MessageAdmission>,
    max_entries: usize,
}

impl InboundAdmissionCache {
    fn new(max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            max_entries: max_entries.max(1),
        }
    }

    fn get(&mut self, msg_id: u32) -> Option<bool> {
        self.evict_stale();
        self.entries.get(&msg_id).map(|entry| entry.admitted)
    }

    fn insert(&mut self, msg_id: u32, admitted: bool) {
        self.evict_stale();
        if !self.entries.contains_key(&msg_id) && self.entries.len() >= self.max_entries {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.first_seen)
                .map(|(msg_id, _)| *msg_id);
            if let Some(oldest) = oldest {
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(
            msg_id,
            MessageAdmission {
                admitted,
                first_seen: Instant::now(),
            },
        );
    }

    fn remove(&mut self, msg_id: u32) {
        self.entries.remove(&msg_id);
    }

    fn evict_stale(&mut self) {
        let now = Instant::now();
        self.entries.retain(|_, entry| {
            now.duration_since(entry.first_seen) <= crate::transport::REASSEMBLY_TTL
        });
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InboundAdmission {
    Admitted,
    Rejected,
    Closed,
}

/// Wait for the current generation to become sendable at most once for the
/// lane. The deadline is shared by all messages handled while the gate is
/// `None`, preventing a stale message burst from extending the pump stall by
/// another full grace period for every frame. Session cancellation always
/// interrupts the wait immediately.
async fn await_inbound_admission(
    generation: u64,
    admission: &mut watch::Receiver<Option<u64>>,
    cancellation: &CancellationToken,
    deadline: &mut Option<Instant>,
) -> InboundAdmission {
    match *admission.borrow() {
        Some(admitted) if admitted == generation => {
            *deadline = None;
            return InboundAdmission::Admitted;
        }
        Some(_) => {
            *deadline = None;
            return InboundAdmission::Rejected;
        }
        None => {}
    }

    let wait_deadline = *deadline.get_or_insert_with(|| Instant::now() + INBOUND_ADMISSION_WAIT);
    let result = crate::timer::timeout_at(
        crate::timer::ids::WEBRTC_PEER_INBOUND_ADMISSION,
        wait_deadline,
        async {
            loop {
                let current = *admission.borrow();
                match current {
                    Some(admitted) if admitted == generation => {
                        return InboundAdmission::Admitted;
                    }
                    Some(_) => return InboundAdmission::Rejected,
                    None => {
                        tokio::select! {
                            _ = cancellation.cancelled() => return InboundAdmission::Closed,
                            changed = admission.changed() => {
                                if changed.is_err() {
                                    return InboundAdmission::Closed;
                                }
                            }
                        }
                    }
                }
            }
        },
    )
    .await;

    match result {
        Ok(admission_result) => {
            if !matches!(admission_result, InboundAdmission::Closed) {
                *deadline = None;
            }
            admission_result
        }
        Err(_) => InboundAdmission::Rejected,
    }
}

/// Do not hold a lane pump on a stale inbound message indefinitely. A healthy
/// same-generation recovery can still release it; a failed/retired generation
/// is discarded once the replacement generation wins the gate.
const INBOUND_ADMISSION_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// Poll a data channel as its single `poll()` consumer (RFC §"DataChannel
/// model": "There MUST be only one `DataChannel::poll()` consumer per
/// DataChannel"), covering every `DataChannelEvent`:
///
/// * `OnOpen` → report lane ready to the controller (`RpcReliable` is the
///   readiness anchor).
/// * `OnMessage` → route the payload to the `InboundRouter` (data plane;
///   bypasses the Controller entirely).
/// * `OnBufferedAmountHigh`/`Low` → ignored (send-buffer flow control is the
///   webrtc crate's per-channel send-buffer limit; see the factory config).
/// * `OnError` → log as a lane-level control signal.
/// * `OnClosing`/`OnClose` → report lane closed and exit.
///
/// The pump never writes control phase; it reports readiness/liveness to the
/// controller as `LaneEvent`s and payloads to the inbound router. This is a
/// plain async fn (not a spawn wrapper) so the owning session registers it in
/// its `JoinSet` and cancels it on close.
pub(crate) async fn lane_pump(
    generation: u64,
    lane: LaneKind,
    channel: Arc<dyn LaneDataChannel>,
    plumbing: LanePlumbing,
    cancellation: CancellationToken,
) {
    let LanePlumbing {
        peer_id,
        lane_tx,
        inbound,
        inbound_generation,
        reassembly_limits,
    } = plumbing;
    use crate::transport::{ReassemblyBuffer, decode_fragment_header};
    use crate::wire::webrtc::rtc::inbound::InboundMessage;
    use webrtc::data_channel::DataChannelEvent as E;

    // Per-lane reassembly state for multi-fragment messages. The pump is the
    // sole consumer of this DataChannel's messages, so local state is safe.
    let mut reassembly = ReassemblyBuffer::with_limits_and_label(
        reassembly_limits,
        format!(
            "peer={} generation={} lane={lane:?}",
            peer_id.to_string_repr(),
            generation,
        ),
    );
    let mut admission = inbound_generation.subscribe();
    let mut admission_deadline = None;
    let mut message_admissions = InboundAdmissionCache::new(reassembly_limits.max_messages);

    while let Some(event) = channel.poll().await {
        match event {
            E::OnOpen => {
                tracing::debug!(
                    peer = %peer_id.to_string_repr(),
                    generation,
                    ?lane,
                    "RTC data-channel lane opened"
                );
                if lane_tx
                    .send(LaneEvent::Opened { generation, lane })
                    .await
                    .is_err()
                {
                    return;
                }
            }
            E::OnMessage(message) => {
                // Strip the 8-byte fragment header (wire-compatible with 0.14)
                // and reassemble multi-fragment messages before dispatch.
                let raw = message.data.freeze();
                let (msg_id, frag_index, total_frags, payload) = match decode_fragment_header(raw) {
                    Ok(fragment) => fragment,
                    Err(e) => {
                        tracing::warn!(
                            peer = %peer_id.to_string_repr(),
                            generation,
                            ?lane,
                            error = %e,
                            "RTC inbound: failed to decode fragment header, dropping message"
                        );
                        continue;
                    }
                };

                let admitted = if total_frags == 1 {
                    await_inbound_admission(
                        generation,
                        &mut admission,
                        &cancellation,
                        &mut admission_deadline,
                    )
                    .await
                } else if let Some(admitted) = message_admissions.get(msg_id) {
                    if admitted {
                        InboundAdmission::Admitted
                    } else {
                        InboundAdmission::Rejected
                    }
                } else {
                    let result = await_inbound_admission(
                        generation,
                        &mut admission,
                        &cancellation,
                        &mut admission_deadline,
                    )
                    .await;
                    match result {
                        InboundAdmission::Admitted => message_admissions.insert(msg_id, true),
                        InboundAdmission::Rejected => message_admissions.insert(msg_id, false),
                        InboundAdmission::Closed => {}
                    }
                    result
                };

                match admitted {
                    InboundAdmission::Admitted => {
                        if total_frags == 1 {
                            if !inbound.route(InboundMessage {
                                peer: peer_id.clone(),
                                generation,
                                lane,
                                payload,
                            }) {
                                tracing::warn!(
                                    peer = %peer_id.to_string_repr(),
                                    generation,
                                    ?lane,
                                    "RTC inbound dispatch budget exhausted; dropping message"
                                );
                            }
                        } else {
                            match reassembly.try_insert(msg_id, frag_index, total_frags, payload) {
                                Ok(Some(complete)) => {
                                    message_admissions.remove(msg_id);
                                    tracing::debug!(
                                        peer = %peer_id.to_string_repr(),
                                        generation,
                                        ?lane,
                                        msg_id,
                                        total_frags,
                                        "RTC reassembled multi-fragment message"
                                    );
                                    if !inbound.route(InboundMessage {
                                        peer: peer_id.clone(),
                                        generation,
                                        lane,
                                        payload: complete,
                                    }) {
                                        tracing::warn!(
                                            peer = %peer_id.to_string_repr(),
                                            generation,
                                            ?lane,
                                            "RTC inbound dispatch budget exhausted; dropping reassembled message"
                                        );
                                    }
                                }
                                Ok(None) => {}
                                Err(error) => {
                                    message_admissions.remove(msg_id);
                                    inbound.record_drop();
                                    tracing::warn!(
                                        peer = %peer_id.to_string_repr(),
                                        generation,
                                        ?lane,
                                        msg_id,
                                        %error,
                                        "RTC inbound reassembly rejected fragment"
                                    );
                                }
                            }
                        }
                        // else: fragment stored, waiting for the rest
                    }
                    InboundAdmission::Rejected => {
                        inbound.record_drop();
                        let admitted_generation = *admission.borrow();
                        tracing::debug!(
                            peer = %peer_id.to_string_repr(),
                            generation,
                            ?admitted_generation,
                            ?lane,
                            msg_id,
                            total_frags,
                            frag_index,
                            "RTC inbound message rejected: generation is not sendable"
                        );
                    }
                    InboundAdmission::Closed => {
                        return;
                    }
                }
            }
            E::OnBufferedAmountHigh | E::OnBufferedAmountLow => {
                // No Actr-level watermark tracking: send-buffer flow control
                // is the webrtc crate's per-channel send-buffer limit.
            }
            E::OnError => {
                tracing::warn!(
                    peer = %peer_id.to_string_repr(),
                    generation,
                    ?lane,
                    "data channel error event"
                );
            }
            E::OnClosing | E::OnClose => {
                tracing::debug!(
                    peer = %peer_id.to_string_repr(),
                    generation,
                    ?lane,
                    "RTC data-channel lane closed"
                );
                let _ = lane_tx.send(LaneEvent::Closed { generation, lane }).await;
                return;
            }
            _ => {}
        }
    }
}

/// Owns the physical resources of one PC generation.
///
/// `ops` is the object-safe session handle; in production it wraps the real
/// `Arc<dyn PeerConnection>` (`RealSessionOps`), in Stage 2 tests it is a fake.
/// DataChannels are stored in `channels` (the resource registry); their pump
/// tasks are registered in `tasks` and bound to `cancellation`, so closing the
/// session stops them and then explicitly closes the connection (invariant
/// #12: every PeerConnection is closed by its session).
pub(crate) struct PeerConnectionSession {
    generation: u64,
    ops: Arc<dyn SessionOps>,
    cancellation: CancellationToken,
    tasks: JoinSet<()>,
    /// Raw resources discovered by the generation's handler but not yet
    /// accepted by the Controller.
    resources: SessionResourceRegistry,
    /// The session's DataChannel resource registry, keyed by lane. The session
    /// is the sole owner of concrete channel objects; the controller only sees
    /// the `LaneHandle` tokens returned by `register_channel`/`create_lane`.
    channels: HashMap<LaneKind, Arc<dyn LaneDataChannel>>,
    /// The session's remote-track resource registry, keyed by track id
    /// (Stage 6). Each track has exactly one pump (its sole `poll()` consumer)
    /// owned by this session's JoinSet.
    tracks: HashMap<String, Arc<dyn TrackRemote>>,
    /// The session's local (outbound) track registry, keyed by track id
    /// (S6-10). Each local track has one poll pump consuming its RTCP
    /// feedback events.
    local_tracks: HashMap<String, Arc<dyn TrackLocal>>,
    /// Set only after the complete explicit close protocol has converged.
    /// A cancelled close future leaves this false so runner teardown can
    /// safely resume cleanup from the remaining registries/tasks.
    close_complete: bool,
}

impl PeerConnectionSession {
    pub(crate) fn new(generation: u64, ops: Arc<dyn SessionOps>) -> Self {
        Self::new_with_resources(generation, ops, SessionResourceRegistry::default())
    }

    pub(crate) fn new_with_resources(
        generation: u64,
        ops: Arc<dyn SessionOps>,
        resources: SessionResourceRegistry,
    ) -> Self {
        Self {
            generation,
            ops,
            cancellation: CancellationToken::new(),
            tasks: JoinSet::new(),
            resources,
            channels: HashMap::new(),
            tracks: HashMap::new(),
            local_tracks: HashMap::new(),
            close_complete: false,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    /// The session's operation handle, used by the controller's effect shell.
    pub(crate) fn ops(&self) -> &Arc<dyn SessionOps> {
        &self.ops
    }

    /// A clone of the session's cancellation token, handed to pump tasks so they
    /// stop when the session closes.
    pub(crate) fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    /// Register a remote-created data channel: store it in the resource
    /// registry, spawn its pump (owned by this session), and return the
    /// `LaneHandle` token the controller installs into the send-path registry.
    ///
    /// The session takes ownership of the concrete channel; the controller
    /// only receives the token.
    pub(crate) fn register_channel(
        &mut self,
        lane: LaneKind,
        channel: Arc<dyn LaneDataChannel>,
        plumbing: LanePlumbing,
    ) -> LaneHandle {
        let handle = LaneHandle::new(self.generation, lane, channel.clone(), self.cancellation());
        // Spawn the pump as a session-owned task (cancelled on close).
        self.tasks.spawn(lane_pump(
            self.generation,
            lane,
            channel.clone(),
            plumbing,
            self.cancellation(),
        ));
        self.channels.insert(lane, channel);
        handle
    }

    /// Accept a handler-discovered DataChannel token and start its sole pump.
    pub(crate) fn accept_data_channel(
        &mut self,
        resource_id: SessionResourceId,
        lane: LaneKind,
        plumbing: LanePlumbing,
    ) -> Option<LaneHandle> {
        let channel = self.resources.take_data_channel(resource_id)?;
        Some(self.register_channel(lane, channel, plumbing))
    }

    pub(crate) fn has_data_channel_resource(&self, resource_id: SessionResourceId) -> bool {
        self.resources.contains_data_channel(resource_id)
    }

    /// Create a data channel for a locally-initiated lane (via the session's
    /// ops), then register it. The session owns the channel and its pump.
    pub(crate) async fn create_lane(
        &mut self,
        lane: LaneKind,
        plumbing: LanePlumbing,
    ) -> Result<LaneHandle, RtcError> {
        let channel = self
            .ops
            .create_data_channel(lane.wire_label(), super::lane::lane_config(lane))
            .await?;
        Ok(self.register_channel(lane, channel, plumbing))
    }

    /// Register a pump task owned by this session. Registered tasks are aborted
    /// on close.
    #[allow(dead_code)]
    pub(crate) fn spawn_task<F>(&mut self, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.tasks.spawn(future);
    }

    /// Register a remote track with pre-resolved metadata: store it in the
    /// resource registry and spawn its pump as the single `poll()` consumer
    /// (Stage 6 unique poll ownership). Returns the track id (the media
    /// registry key). The caller resolves the metadata first so this method
    /// never holds the session registry lock across an `.await`.
    pub(crate) fn register_track_with_meta(
        &mut self,
        track: Arc<dyn TrackRemote>,
        meta: TrackMeta,
        peer_id: &ActrId,
        media_registry: Option<Arc<MediaFrameRegistry>>,
    ) -> String {
        let track_id = meta.track_id.clone();
        self.tasks
            .spawn(crate::wire::webrtc::rtc::media::track_pump(
                peer_id.clone(),
                self.generation,
                track.clone(),
                meta,
                media_registry,
            ));
        self.tracks.insert(track_id.clone(), track);
        track_id
    }

    /// Accept a handler-discovered Track token and start its sole RTP pump.
    pub(crate) fn accept_remote_track(
        &mut self,
        resource_id: SessionResourceId,
        meta: TrackMeta,
        peer_id: &ActrId,
        media_registry: Option<Arc<MediaFrameRegistry>>,
    ) -> Option<String> {
        if self.tracks.contains_key(&meta.track_id)
            || !self.resources.contains_remote_track(resource_id)
        {
            return None;
        }
        let track = self.resources.take_remote_track(resource_id)?;
        Some(self.register_track_with_meta(track, meta, peer_id, media_registry))
    }

    /// Register a remote track, resolving its metadata inline (tests).
    #[cfg(test)]
    pub(crate) async fn register_track(
        &mut self,
        track: Arc<dyn TrackRemote>,
        peer_id: &ActrId,
        media_registry: Option<Arc<MediaFrameRegistry>>,
    ) -> String {
        let meta = TrackMeta::resolve(&track).await;
        self.register_track_with_meta(track, meta, peer_id, media_registry)
    }

    /// Register a local (outbound) track after a successful `add_track`
    /// (S6-10): store it in the resource registry and spawn its poll pump —
    /// the single consumer of its RTCP feedback events. The caller resolves
    /// the track id first so this method never spans an `.await` under the
    /// session registry lock.
    pub(crate) fn register_local_track(
        &mut self,
        track_id: String,
        track: Arc<dyn TrackLocal>,
        peer_id: &ActrId,
    ) {
        self.tasks
            .spawn(crate::wire::webrtc::rtc::media::local_track_pump(
                peer_id.clone(),
                self.generation,
                track_id.clone(),
                track.clone(),
            ));
        self.local_tracks.insert(track_id, track);
    }

    /// Take a local track out of the registry (remove_track). The caller
    /// stops it *after* releasing the session registry lock (`stop()` is
    /// async; the lock never spans an `.await`).
    pub(crate) fn take_local_track(&mut self, track_id: &str) -> Option<Arc<dyn TrackLocal>> {
        self.local_tracks.remove(track_id)
    }

    /// Explicitly close this session: cancel its pumps, close each owned
    /// DataChannel, stop its tasks, and close the connection. Consumes the
    /// session so it cannot be reused (RFC: a retiring session MUST NOT
    /// re-enter active).
    ///
    /// RFC work item 11: "Shutdown explicitly calls each `DataChannel::close()`"
    /// — each channel is closed before the PeerConnection, aggregating errors
    /// without blocking resource shutdown. Stage 6: owned tracks are stopped
    /// explicitly too ("Track cancellation" in the shutdown sequence).
    pub(crate) async fn close(mut self) -> Result<(), RtcError> {
        self.close_in_place().await
    }

    /// Cancellation-safe form used by the runner's retiring-session registry.
    /// If the effect task is aborted midway, the Session remains registered;
    /// runner shutdown obtains it again and resumes this idempotent drain.
    pub(crate) async fn close_in_place(&mut self) -> Result<(), RtcError> {
        if self.close_complete {
            return Ok(());
        }
        self.cancellation.cancel();
        // Reject/close every handler-discovered resource that the Controller
        // never accepted (stale, duplicate, invalid metadata, or shutdown).
        self.resources.close_all().await;
        // Stop each owned remote track (cancels its RTP flow).
        let remote_tracks = self
            .tracks
            .iter()
            .map(|(track_id, track)| (track_id.clone(), track.clone()))
            .collect::<Vec<_>>();
        for (track_id, track) in remote_tracks {
            track.stop().await;
            self.tracks.remove(&track_id);
            tracing::debug!(
                generation = self.generation,
                %track_id,
                "stopped remote track during session close"
            );
        }
        // S6-10: stop each owned local track.
        let local_tracks = self
            .local_tracks
            .iter()
            .map(|(track_id, track)| (track_id.clone(), track.clone()))
            .collect::<Vec<_>>();
        for (track_id, track) in local_tracks {
            track.stop().await;
            self.local_tracks.remove(&track_id);
            tracing::debug!(
                generation = self.generation,
                %track_id,
                "stopped local track during session close"
            );
        }
        // Explicitly close each owned DataChannel before the PC (RFC item 11).
        let channels = self
            .channels
            .iter()
            .map(|(lane, channel)| (*lane, channel.clone()))
            .collect::<Vec<_>>();
        for (lane, channel) in channels {
            if let Err(e) = channel.close().await {
                tracing::debug!(
                    generation = self.generation,
                    ?lane,
                    "data channel close returned error during shutdown: {e}"
                );
            }
            self.channels.remove(&lane);
        }
        self.tasks.abort_all();
        let close_result = self.ops.close().await;
        // `abort_all` only requests cancellation; draining the JoinSet is what
        // proves every generation-owned pump has actually converged before the
        // physical Session disappears from the runner registry.
        while let Some(result) = self.tasks.join_next().await {
            if let Err(error) = result
                && !error.is_cancelled()
            {
                tracing::warn!(
                    generation = self.generation,
                    %error,
                    "session-owned task failed while closing"
                );
            }
        }
        self.close_complete = true;
        close_result
    }
}
