//! `RtcPeerEffectPort` — the controller's asynchronous effect boundary
//! (RFC §"RtcPeerEffectPort").
//!
//! The controller decides *what* to do (reducer → effects); the effect port
//! decides *how* to do it (spawn tasks, run blocking WebRTC operations, send
//! completions). Every effect and completion carries `generation` and
//! `operation_id` identity so stale results are re-validated before applying.
//!
//! Production uses [`WebRtcEffectRunner`], which owns the per-generation
//! `PeerConnectionSession` registry (RFC: "WebRtcEffectRunner owns the Session
//! registry"). The controller retains only logical ownership through generation
//! lifecycle slots (building/active/pending) and never touches concrete session
//! resources directly.
//!
//! T1 test harnesses substitute a recording sink for the runner: they record
//! submitted effects and inject completions without owning a session or
//! implementing a full v0.20 effect (RFC §"RtcPeerEffectPort").

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use actr_protocol::ActrId;
use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use crate::wire::webrtc::v020::event_handler::WebRtcEventHandler;
use crate::wire::webrtc::v020::events::PeerEventMailbox;
use crate::wire::webrtc::v020::status::LaneKind;

use super::effects::{SessionFactory, SessionOps};
use super::lane_handle::LaneHandle;
use super::reducer::{EffectCompletion, LaneEvent, RtcPeerEffect};
use super::session::{
    InboundGenerationGate, PeerConnectionSession, SessionResourceId, SessionResourceRegistry,
};

pub(crate) enum ResourceRegistration<T> {
    Registered(T),
    /// The handler has stored the resource, but `build()` has not yet returned
    /// and installed the physical Session. The Controller retains only the
    /// token/descriptor and retries after the build completion.
    SessionPending,
    MissingOrMismatched,
}

/// Complete cross-task identity for one blocking effect.  `peer_id` is kept at
/// the port boundary while the reducer additionally authenticates the exact
/// PC/operation/ICE identity against its pending-operation registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RtcEffectIdentity {
    pub(crate) peer_id: ActrId,
    pub(crate) pc_generation: u64,
    pub(crate) operation_id: u64,
    pub(crate) ice_generation: Option<u64>,
}

/// The only value accepted by a production or test effect port.
pub(crate) struct TaggedRtcPeerEffect {
    pub(crate) identity: RtcEffectIdentity,
    pub(crate) effect: RtcPeerEffect,
}

impl TaggedRtcPeerEffect {
    pub(crate) fn new(peer_id: ActrId, effect: RtcPeerEffect) -> Result<Self, RtcEffectPortError> {
        let Some((pc_generation, operation_id, ice_generation, _)) = effect.pending_identity()
        else {
            return Err(RtcEffectPortError::NotBlocking);
        };
        Ok(Self {
            identity: RtcEffectIdentity {
                peer_id,
                pc_generation,
                operation_id,
                ice_generation,
            },
            effect,
        })
    }
}

impl fmt::Debug for TaggedRtcPeerEffect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TaggedRtcPeerEffect")
            .field("identity", &self.identity)
            .field("effect", &self.effect)
            .finish()
    }
}

/// Completion envelope sent across the effect task boundary.  The peer identity
/// never depends on mutable controller state and is checked before the inner
/// completion reaches the reducer.
pub(crate) struct TaggedRtcEffectCompletion {
    pub(crate) identity: RtcEffectIdentity,
    pub(crate) completion: EffectCompletion,
}

impl TaggedRtcEffectCompletion {
    fn new(identity: RtcEffectIdentity, completion: EffectCompletion) -> Self {
        debug_assert_eq!(identity.pc_generation, completion.generation());
        debug_assert_eq!(identity.operation_id, completion.operation_id());
        debug_assert_eq!(identity.ice_generation, completion.ice_generation());
        Self {
            identity,
            completion,
        }
    }

    /// Authenticate the port envelope before exposing the reducer completion.
    pub(crate) fn into_completion_for(
        self,
        peer_id: &ActrId,
    ) -> Result<EffectCompletion, RtcEffectPortError> {
        if &self.identity.peer_id != peer_id {
            return Err(RtcEffectPortError::WrongPeer);
        }
        if self.identity.pc_generation != self.completion.generation()
            || self.identity.operation_id != self.completion.operation_id()
            || self.identity.ice_generation != self.completion.ice_generation()
        {
            return Err(RtcEffectPortError::IdentityMismatch);
        }
        Ok(self.completion)
    }
}

impl fmt::Debug for TaggedRtcEffectCompletion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TaggedRtcEffectCompletion")
            .field("identity", &self.identity)
            .field("completion", &self.completion)
            .finish()
    }
}

pub(crate) type RtcEffectCompletionSender = mpsc::Sender<TaggedRtcEffectCompletion>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RtcEffectPortError {
    NotBlocking,
    WrongPeer,
    IdentityMismatch,
    TestSinkClosed,
    UnknownOperation,
    Closed,
}

impl fmt::Display for RtcEffectPortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotBlocking => f.write_str("effect does not produce a completion"),
            Self::WrongPeer => f.write_str("effect/completion belongs to another peer"),
            Self::IdentityMismatch => f.write_str("effect completion identity mismatch"),
            Self::TestSinkClosed => f.write_str("test effect sink is closed"),
            Self::UnknownOperation => f.write_str("effect operation is not pending"),
            Self::Closed => f.write_str("effect runner is closed"),
        }
    }
}

/// Runner-owned task set for every asynchronous WebRTC effect.
///
/// `submit()` is synchronous, so the set is protected by a short std mutex.
/// The mutex is never held across `.await`: shutdown atomically closes the
/// admission gate and takes the `JoinSet`, then aborts/joins it asynchronously.
/// This prevents a build or restart task from outliving its Controller and
/// inserting a physical Session after teardown has already drained the
/// registry.
struct EffectTaskGroup {
    closed: bool,
    tasks: JoinSet<()>,
}

impl EffectTaskGroup {
    fn new() -> Self {
        Self {
            closed: false,
            tasks: JoinSet::new(),
        }
    }

    fn spawn<F>(&mut self, future: F) -> Result<(), RtcEffectPortError>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.spawn_with(|| future)
    }

    fn spawn_with<F, Make>(&mut self, make_future: Make) -> Result<(), RtcEffectPortError>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
        Make: FnOnce() -> F,
    {
        if self.closed {
            return Err(RtcEffectPortError::Closed);
        }
        while let Some(result) = self.tasks.try_join_next() {
            if let Err(error) = result
                && !error.is_cancelled()
            {
                tracing::warn!(%error, "RTC effect task failed");
            }
        }
        self.tasks.spawn(make_future());
        Ok(())
    }

    fn begin_shutdown(&mut self) {
        self.closed = true;
        self.tasks.abort_all();
    }

    fn reap_shutdown(&mut self) -> bool {
        while let Some(result) = self.tasks.try_join_next() {
            if let Err(error) = result
                && !error.is_cancelled()
            {
                tracing::warn!(%error, "RTC effect task failed during runner shutdown");
            }
        }
        self.tasks.is_empty()
    }
}

impl std::error::Error for RtcEffectPortError {}

/// T1 recording boundary. It runs no WebRTC operation and owns no Session;
/// tests observe the exact tagged effects produced by the real Controller and
/// explicitly inject tagged semantic completions.
#[cfg(test)]
pub(crate) struct TestEffectSink {
    peer_id: ActrId,
    effects_tx: mpsc::UnboundedSender<TaggedRtcPeerEffect>,
    effects_rx: tokio::sync::Mutex<mpsc::UnboundedReceiver<TaggedRtcPeerEffect>>,
    pending: Mutex<HashMap<u64, (RtcEffectIdentity, RtcEffectCompletionSender)>>,
    close_all_calls: std::sync::atomic::AtomicU64,
}

#[cfg(test)]
impl TestEffectSink {
    pub(crate) fn new(peer_id: ActrId) -> Arc<Self> {
        let (effects_tx, effects_rx) = mpsc::unbounded_channel();
        Arc::new(Self {
            peer_id,
            effects_tx,
            effects_rx: tokio::sync::Mutex::new(effects_rx),
            pending: Mutex::new(HashMap::new()),
            close_all_calls: std::sync::atomic::AtomicU64::new(0),
        })
    }

    pub(crate) async fn next_effect(&self) -> Option<TaggedRtcPeerEffect> {
        self.effects_rx.lock().await.recv().await
    }

    /// Complete a recorded operation with its authentic envelope. The inner
    /// completion must match the recorded PC/operation/ICE identity exactly.
    pub(crate) async fn complete(
        &self,
        completion: EffectCompletion,
    ) -> Result<(), RtcEffectPortError> {
        let operation_id = completion.operation_id();
        let (identity, completion_tx) = self
            .pending
            .lock()
            .expect("test effect pending lock")
            .remove(&operation_id)
            .ok_or(RtcEffectPortError::UnknownOperation)?;
        if identity.pc_generation != completion.generation()
            || identity.ice_generation != completion.ice_generation()
            || identity.operation_id != operation_id
        {
            self.pending
                .lock()
                .expect("test effect pending lock")
                .insert(operation_id, (identity, completion_tx));
            return Err(RtcEffectPortError::IdentityMismatch);
        }
        completion_tx
            .send(TaggedRtcEffectCompletion::new(identity, completion))
            .await
            .map_err(|_| RtcEffectPortError::TestSinkClosed)
    }

    /// Inject an arbitrary envelope through the Controller completion channel
    /// without consuming the authentic pending operation. This is used to
    /// prove wrong-peer/generation/operation completions cannot mutate state.
    pub(crate) async fn inject(
        &self,
        identity: RtcEffectIdentity,
        completion: EffectCompletion,
    ) -> Result<(), RtcEffectPortError> {
        let completion_tx = self
            .pending
            .lock()
            .expect("test effect pending lock")
            .values()
            .next()
            .map(|(_, tx)| tx.clone())
            .ok_or(RtcEffectPortError::UnknownOperation)?;
        completion_tx
            .send(TaggedRtcEffectCompletion {
                identity,
                completion,
            })
            .await
            .map_err(|_| RtcEffectPortError::TestSinkClosed)
    }

    pub(crate) fn pending_count(&self) -> usize {
        self.pending.lock().expect("test effect pending lock").len()
    }

    pub(crate) fn close_all_calls(&self) -> u64 {
        self.close_all_calls
            .load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// The controller's asynchronous effect boundary. Each controller holds one
/// port for its lifetime (RFC: "A Controller is bound to one runner for its
/// lifetime").
///
/// The port is crate-private, not an external WebRTC facade: it MUST NOT copy
/// the complete v0.20 object interface or restore the 0.14 callback-registration
/// model.
#[async_trait]
pub(crate) trait RtcPeerEffectPort: Send + Sync {
    /// Submit a blocking effect for execution. The implementation spawns the
    /// work and sends the completion back via `completion_tx`. Effects carry
    /// `generation`/`operation_id` identity; completions echo it.
    fn submit(
        &self,
        operation: TaggedRtcPeerEffect,
        completion_tx: RtcEffectCompletionSender,
    ) -> Result<(), RtcEffectPortError>;

    async fn create_lane(
        &self,
        generation: u64,
        lane: LaneKind,
    ) -> Result<Option<LaneHandle>, webrtc::error::Error>;

    fn register_remote_channel(
        &self,
        generation: u64,
        resource_id: SessionResourceId,
        lane: LaneKind,
    ) -> ResourceRegistration<LaneHandle>;

    async fn register_remote_track(
        &self,
        generation: u64,
        resource_id: SessionResourceId,
        meta: crate::wire::webrtc::v020::media::TrackMeta,
    ) -> ResourceRegistration<String>;

    async fn discard_remote_resource(&self, generation: u64, resource_id: SessionResourceId);

    async fn add_track(
        &self,
        generation: u64,
        track: Arc<dyn webrtc::media_stream::track_local::TrackLocal>,
    ) -> Option<Result<Arc<dyn webrtc::rtp_transceiver::RtpSender>, webrtc::error::Error>>;

    async fn remove_track(
        &self,
        generation: u64,
        sender: Arc<dyn webrtc::rtp_transceiver::RtpSender>,
    ) -> Option<Result<(), webrtc::error::Error>>;

    async fn close_all_sessions(&self);
}

/// Production effect runner: owns the session registry and executes effects
/// against real (or test-fake) `SessionOps`.
///
/// Sessions are stored behind an `Arc<Mutex<…>>` so effect tasks (spawned by
/// `submit`) can register/remove sessions without the controller holding a
/// lock across `.await`. The Mutex is never held across an `.await` — session
/// ops are cloned out, the lock released, then the async operation runs.
pub(crate) struct WebRtcEffectRunner {
    peer_id: ActrId,
    /// The session registry, owned by the runner (RFC: "WebRtcEffectRunner owns
    /// the Session registry"). The controller never touches this directly.
    sessions: Arc<Mutex<HashMap<u64, PeerConnectionSession>>>,
    /// Sessions removed from the active registry while their explicit close
    /// effect is running. Keeping the Session in this registry makes close
    /// cancellation-safe: abnormal runner teardown can resume explicit close
    /// after aborting the effect task.
    closing_sessions: Arc<Mutex<HashMap<u64, Arc<tokio::sync::Mutex<PeerConnectionSession>>>>>,
    /// Registries are installed before `build()` so callbacks fired during the
    /// build can store raw resources safely. They are removed only after the
    /// generation is explicitly closed or a build fails.
    resources: Arc<Mutex<HashMap<u64, SessionResourceRegistry>>>,
    /// Every effect task is admitted through this runner-owned set. There are
    /// no detached production effect tasks.
    effect_tasks: Arc<Mutex<EffectTaskGroup>>,
    factory: Arc<dyn SessionFactory>,
    event_mailbox: PeerEventMailbox,
    lane_tx: mpsc::Sender<LaneEvent>,
    /// Shared admission fence for generation-scoped inbound data pumps. The
    /// Controller stores the exact sendable generation here; retired or
    /// recovering generations are rejected before reaching business code.
    inbound_generation: Arc<InboundGenerationGate>,
    /// Node-level shared dependencies (inbound router,
    /// media registry).
    deps: super::controller::SharedNodeDeps,
}

impl WebRtcEffectRunner {
    pub(crate) fn new(
        peer_id: ActrId,
        factory: Arc<dyn SessionFactory>,
        event_mailbox: PeerEventMailbox,
        lane_tx: mpsc::Sender<LaneEvent>,
        deps: super::controller::SharedNodeDeps,
        inbound_generation: Arc<InboundGenerationGate>,
    ) -> Self {
        Self {
            peer_id,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            closing_sessions: Arc::new(Mutex::new(HashMap::new())),
            resources: Arc::new(Mutex::new(HashMap::new())),
            effect_tasks: Arc::new(Mutex::new(EffectTaskGroup::new())),
            factory,
            event_mailbox,
            lane_tx,
            inbound_generation,
            deps,
        }
    }

    /// Create a data-channel lane on a session (inline; awaited by the
    /// controller so the channel exists before the subsequent CreateOffer is
    /// issued, keeping it in the offer SDP). The session stores the channel
    /// and spawns the pump; the runner returns the `LaneHandle` token for the
    /// controller's send-path registry.
    pub(crate) async fn create_lane(
        &self,
        generation: u64,
        lane: LaneKind,
    ) -> Result<Option<LaneHandle>, webrtc::error::Error> {
        // Get the session's ops handle (brief lock, released before .await).
        let ops = {
            let sessions = self.sessions.lock().expect("sessions lock");
            sessions.get(&generation).map(|s| s.ops().clone())
        }
        .ok_or_else(|| {
            webrtc::error::Error::Other(format!(
                "RTC session generation {generation} is not registered"
            ))
        })?;
        // Create the channel (async, no lock held).
        let channel = ops
            .create_data_channel(lane.wire_label(), super::lane::lane_config(lane))
            .await?;
        // Register the channel in the session (brief lock).
        let registered = {
            let mut sessions = self.sessions.lock().expect("sessions lock");
            sessions.get_mut(&generation).map(|session| {
                session.register_channel(
                    lane,
                    channel.clone(),
                    super::session::LanePlumbing {
                        peer_id: self.peer_id.clone(),
                        lane_tx: self.lane_tx.clone(),
                        inbound: self.deps.inbound.clone(),
                        inbound_generation: self.inbound_generation.clone(),
                        reassembly_limits: crate::transport::ReassemblyLimits::from_buffer_config(
                            &self.deps.buffer_config,
                        ),
                    },
                )
            })
        };
        let Some(handle) = registered else {
            let _ = channel.close().await;
            return Err(webrtc::error::Error::Other(format!(
                "RTC session generation {generation} retired while creating lane"
            )));
        };
        Ok(Some(handle))
    }

    /// Resolve and accept a remote-created DataChannel token. The raw channel
    /// never crosses this runner boundary into the Controller.
    pub(crate) fn register_remote_channel(
        &self,
        generation: u64,
        resource_id: SessionResourceId,
        lane: LaneKind,
    ) -> ResourceRegistration<LaneHandle> {
        let mut sessions = self.sessions.lock().expect("sessions lock");
        let Some(session) = sessions.get_mut(&generation) else {
            return if self
                .resources
                .lock()
                .expect("resource registries lock")
                .contains_key(&generation)
            {
                ResourceRegistration::SessionPending
            } else {
                ResourceRegistration::MissingOrMismatched
            };
        };
        if !session.has_data_channel_resource(resource_id) {
            return ResourceRegistration::MissingOrMismatched;
        }
        let Some(handle) = session.accept_data_channel(
            resource_id,
            lane,
            super::session::LanePlumbing {
                peer_id: self.peer_id.clone(),
                lane_tx: self.lane_tx.clone(),
                inbound: self.deps.inbound.clone(),
                inbound_generation: self.inbound_generation.clone(),
                reassembly_limits: crate::transport::ReassemblyLimits::from_buffer_config(
                    &self.deps.buffer_config,
                ),
            },
        ) else {
            return ResourceRegistration::MissingOrMismatched;
        };
        tracing::debug!(
            peer = %self.peer_id.to_string_repr(),
            generation,
            ?lane,
            "v0.20 remote data channel accepted"
        );
        ResourceRegistration::Registered(handle)
    }

    /// Register a remote track with the session for `generation` (Stage 6).
    /// The session stores the track and spawns its single poll-consumer pump,
    /// which dispatches received RTP to the node media registry. Returns the
    /// track id, or `None` if the session doesn't exist (stale generation).
    /// Metadata is resolved *before* the sessions lock is taken, so the lock is
    /// never held across an `.await`.
    pub(crate) fn register_remote_track(
        &self,
        generation: u64,
        resource_id: SessionResourceId,
        meta: crate::wire::webrtc::v020::media::TrackMeta,
    ) -> ResourceRegistration<String> {
        let mut sessions = self.sessions.lock().expect("sessions lock");
        let Some(session) = sessions.get_mut(&generation) else {
            return if self
                .resources
                .lock()
                .expect("resource registries lock")
                .contains_key(&generation)
            {
                ResourceRegistration::SessionPending
            } else {
                ResourceRegistration::MissingOrMismatched
            };
        };
        match session.accept_remote_track(
            resource_id,
            meta,
            &self.peer_id,
            self.deps.media_registry.clone(),
        ) {
            Some(track_id) => ResourceRegistration::Registered(track_id),
            None => ResourceRegistration::MissingOrMismatched,
        }
    }

    pub(crate) async fn discard_remote_resource(
        &self,
        generation: u64,
        resource_id: SessionResourceId,
    ) {
        let registry = self
            .resources
            .lock()
            .expect("resource registries lock")
            .get(&generation)
            .cloned();
        if let Some(registry) = registry {
            registry.discard(resource_id).await;
        }
    }

    /// S6-2: add a local media track on the session for `generation`. Returns
    /// `None` if the session is gone (stale generation). The sessions lock is
    /// released before the async op runs. On success the session takes
    /// ownership of the track (S6-10: poll pump + explicit stop on close).
    pub(crate) async fn add_track(
        &self,
        generation: u64,
        track: Arc<dyn webrtc::media_stream::track_local::TrackLocal>,
    ) -> Option<Result<Arc<dyn webrtc::rtp_transceiver::RtpSender>, webrtc::error::Error>> {
        let ops = {
            let sessions = self.sessions.lock().expect("sessions lock");
            sessions.get(&generation).map(|s| s.ops().clone())
        }?;
        let result = ops.add_track(track.clone()).await;
        if result.is_ok() {
            // Resolve the id before taking the lock (no lock across .await).
            let track_id = track.track_id().await;
            let mut sessions = self.sessions.lock().expect("sessions lock");
            if let Some(session) = sessions.get_mut(&generation) {
                session.register_local_track(track_id, track, &self.peer_id);
            }
        }
        Some(result)
    }

    /// S6-2: remove a local media track by its sender. Returns `None` if the
    /// session is gone (stale generation). On success the session stops and
    /// drops its ownership of the track.
    pub(crate) async fn remove_track(
        &self,
        generation: u64,
        sender: Arc<dyn webrtc::rtp_transceiver::RtpSender>,
    ) -> Option<Result<(), webrtc::error::Error>> {
        let ops = {
            let sessions = self.sessions.lock().expect("sessions lock");
            sessions.get(&generation).map(|s| s.ops().clone())
        }?;
        let result = ops.remove_track(&sender).await;
        if result.is_ok() {
            let track_id = sender.track().track_id().await;
            let track = {
                let mut sessions = self.sessions.lock().expect("sessions lock");
                sessions
                    .get_mut(&generation)
                    .and_then(|session| session.take_local_track(&track_id))
            };
            // Stop outside the lock (stop() is async).
            if let Some(track) = track {
                track.stop().await;
            }
        }
        Some(result)
    }

    /// Close every remaining session (controller teardown, invariant #12).
    /// Drains the registry and closes each session explicitly.
    pub(crate) async fn close_all_sessions(&self) {
        // Close effect admission first, then abort and join every task before
        // draining registries. The JoinSet remains in the shared group while
        // this future waits: if an outer close deadline cancels this call, a
        // later Manager fallback resumes reaping instead of losing detached
        // JoinHandles. Once this loop completes, no late build can insert a
        // Session and no operation can still be using a cloned PC.
        self.effect_tasks
            .lock()
            .expect("RTC effect task group lock")
            .begin_shutdown();
        loop {
            let converged = self
                .effect_tasks
                .lock()
                .expect("RTC effect task group lock")
                .reap_shutdown();
            if converged {
                break;
            }
            tokio::task::yield_now().await;
        }

        // Move every still-active Session into the cancellation-safe retiring
        // registry before the first await. A cancelled caller therefore leaves
        // physical ownership discoverable for the next cleanup attempt.
        {
            let mut map = self.sessions.lock().expect("sessions lock");
            let mut closing = self.closing_sessions.lock().expect("closing sessions lock");
            if !map.is_empty() {
                tracing::debug!(
                    peer = %self.peer_id.to_string_repr(),
                    generations = ?map.keys().collect::<Vec<_>>(),
                    "v0.20 controller teardown: closing all remaining sessions"
                );
            }
            for (generation, session) in map.drain() {
                closing
                    .entry(generation)
                    .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(session)));
            }
        }
        let closing: Vec<(u64, Arc<tokio::sync::Mutex<PeerConnectionSession>>)> = {
            let map = self.closing_sessions.lock().expect("closing sessions lock");
            map.iter()
                .map(|(generation, session)| (*generation, session.clone()))
                .collect()
        };
        for (generation, session) in closing {
            let mut session = session.lock().await;
            let _ = session.close_in_place().await;
            drop(session);
            self.closing_sessions
                .lock()
                .expect("closing sessions lock")
                .remove(&generation);
        }
        let registries: Vec<(u64, SessionResourceRegistry)> = {
            let map = self.resources.lock().expect("resource registries lock");
            map.iter()
                .map(|(generation, registry)| (*generation, registry.clone()))
                .collect()
        };
        for (generation, registry) in registries {
            registry.close_all().await;
            self.resources
                .lock()
                .expect("resource registries lock")
                .remove(&generation);
        }
    }

    /// Spawn an effect task and always return one tagged completion. A missing
    /// session is an explicit operation failure, not a silently lost completion:
    /// otherwise the reducer's pending-operation registry could never converge.
    fn with_session_ops<F, Fut>(
        &self,
        identity: RtcEffectIdentity,
        completion_tx: RtcEffectCompletionSender,
        f: F,
    ) -> Result<(), RtcEffectPortError>
    where
        F: FnOnce(Result<Arc<dyn SessionOps>, webrtc::error::Error>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = EffectCompletion> + Send + 'static,
    {
        let ops = {
            let sessions = self.sessions.lock().expect("sessions lock");
            sessions
                .get(&identity.pc_generation)
                .map(|s| s.ops().clone())
                .ok_or_else(|| {
                    webrtc::error::Error::Other(format!(
                        "RTC session generation {} is not registered",
                        identity.pc_generation
                    ))
                })
        };
        self.spawn_effect(async move {
            let completion = f(ops).await;
            let _ = completion_tx
                .send(TaggedRtcEffectCompletion::new(identity, completion))
                .await;
        })
    }

    fn spawn_effect<F>(&self, future: F) -> Result<(), RtcEffectPortError>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.effect_tasks
            .lock()
            .expect("RTC effect task group lock")
            .spawn(future)
    }

    /// Atomically perform synchronous registry setup and admit its effect
    /// task under the same runner lifecycle lock. Shutdown therefore cannot
    /// drain the registries between setup and task admission.
    fn spawn_effect_with<F, Make>(&self, make_future: Make) -> Result<(), RtcEffectPortError>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
        Make: FnOnce() -> F,
    {
        self.effect_tasks
            .lock()
            .expect("RTC effect task group lock")
            .spawn_with(make_future)
    }
}

#[async_trait]
impl RtcPeerEffectPort for WebRtcEffectRunner {
    fn submit(
        &self,
        operation: TaggedRtcPeerEffect,
        completion_tx: RtcEffectCompletionSender,
    ) -> Result<(), RtcEffectPortError> {
        if operation.identity.peer_id != self.peer_id {
            return Err(RtcEffectPortError::WrongPeer);
        }
        let TaggedRtcPeerEffect { identity, effect } = operation;
        match effect {
            RtcPeerEffect::BuildSession {
                generation,
                operation_id,
            } => {
                let handler = Arc::new(WebRtcEventHandler::new_with_resource_limit(
                    self.peer_id.clone(),
                    generation,
                    self.event_mailbox.clone(),
                    self.deps.buffer_config.max_session_resources,
                ));
                let resource_registry = handler.resources();
                let factory = self.factory.clone();
                let sessions = self.sessions.clone();
                let resources = self.resources.clone();
                let peer_id = self.peer_id.clone();
                self.spawn_effect_with(move || {
                    resources
                        .lock()
                        .expect("resource registries lock")
                        .insert(generation, resource_registry.clone());
                    async move {
                        let result = factory.build(handler).await;
                        if let Err(error) = &result {
                            tracing::error!(
                                peer = %peer_id.to_string_repr(),
                                generation,
                                error = %error,
                                "v0.20 PeerConnection build failed"
                            );
                        }
                        // Register the session on success (RFC: "Register a Session
                        // after a successful build and send completion").
                        if let Ok(ref ops) = result {
                            tracing::debug!(
                                peer = %peer_id.to_string_repr(),
                                generation,
                                operation_id,
                                "v0.20 PeerConnection built; session registered"
                            );
                            sessions.lock().expect("sessions lock").insert(
                                generation,
                                PeerConnectionSession::new_with_resources(
                                    generation,
                                    ops.clone(),
                                    resource_registry.clone(),
                                ),
                            );
                        } else {
                            // Keep the registry discoverable until raw resources
                            // and normalization tasks have converged. If shutdown
                            // aborts this task, runner teardown resumes cleanup from
                            // the still-registered generation.
                            resource_registry.close_all().await;
                            resources
                                .lock()
                                .expect("resource registries lock")
                                .remove(&generation);
                        }
                        let _ = completion_tx
                            .send(TaggedRtcEffectCompletion::new(
                                identity,
                                EffectCompletion::BuildSession {
                                    generation,
                                    operation_id,
                                    result: result.map(|_| ()),
                                },
                            ))
                            .await;
                    }
                })
            }
            RtcPeerEffect::CreateOffer {
                generation,
                operation_id,
            } => self.with_session_ops(identity, completion_tx, move |ops| async move {
                let started = tokio::time::Instant::now();
                tracing::debug!(
                    generation,
                    operation_id,
                    stage = "create_offer",
                    "SDP operation started"
                );
                let result = match ops {
                    Ok(ops) => ops.create_offer().await.map(Box::new),
                    Err(error) => Err(error),
                };
                // Do not log SDP or opaque backend errors: they may contain ICE credentials.
                tracing::debug!(
                    generation,
                    operation_id,
                    stage = "create_offer",
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    success = result.is_ok(),
                    "SDP operation completed"
                );
                EffectCompletion::CreateOffer {
                    generation,
                    operation_id,
                    result,
                }
            }),
            RtcPeerEffect::CreateAnswer {
                generation,
                operation_id,
            } => self.with_session_ops(identity, completion_tx, move |ops| async move {
                let started = tokio::time::Instant::now();
                tracing::debug!(
                    generation,
                    operation_id,
                    stage = "create_answer",
                    "SDP operation started"
                );
                let result = match ops {
                    Ok(ops) => ops.create_answer().await.map(Box::new),
                    Err(error) => Err(error),
                };
                // Do not log SDP or opaque backend errors: they may contain ICE credentials.
                tracing::debug!(
                    generation,
                    operation_id,
                    stage = "create_answer",
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    success = result.is_ok(),
                    "SDP operation completed"
                );
                EffectCompletion::CreateAnswer {
                    generation,
                    operation_id,
                    result,
                }
            }),
            RtcPeerEffect::SetLocalDescription {
                generation,
                operation_id,
                desc,
            } => self.with_session_ops(identity, completion_tx, move |ops| async move {
                let started = tokio::time::Instant::now();
                tracing::debug!(
                    generation,
                    operation_id,
                    stage = "set_local_description",
                    "SDP operation started"
                );
                let result = match ops {
                    Ok(ops) => ops.set_local_description(*desc).await,
                    Err(error) => Err(error),
                };
                // Do not log SDP or opaque backend errors: they may contain ICE credentials.
                tracing::debug!(
                    generation,
                    operation_id,
                    stage = "set_local_description",
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    success = result.is_ok(),
                    "SDP operation completed"
                );
                EffectCompletion::SetLocalDescription {
                    generation,
                    operation_id,
                    result,
                }
            }),
            RtcPeerEffect::SetRemoteDescription {
                generation,
                operation_id,
                desc,
            } => self.with_session_ops(identity, completion_tx, move |ops| async move {
                let started = tokio::time::Instant::now();
                tracing::debug!(
                    generation,
                    operation_id,
                    stage = "set_remote_description",
                    "SDP operation started"
                );
                let result = match ops {
                    Ok(ops) => ops.set_remote_description(*desc).await,
                    Err(error) => Err(error),
                };
                // Do not log SDP or opaque backend errors: they may contain ICE credentials.
                tracing::debug!(
                    generation,
                    operation_id,
                    stage = "set_remote_description",
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    success = result.is_ok(),
                    "SDP operation completed"
                );
                EffectCompletion::SetRemoteDescription {
                    generation,
                    operation_id,
                    result,
                }
            }),
            RtcPeerEffect::AddIceCandidate {
                generation,
                operation_id,
                ice_generation,
                candidate,
            } => self.with_session_ops(identity, completion_tx, move |ops| async move {
                let result = match ops {
                    Ok(ops) => ops.add_ice_candidate(candidate).await,
                    Err(error) => Err(error),
                };
                EffectCompletion::AddIceCandidate {
                    generation,
                    operation_id,
                    ice_generation,
                    result,
                }
            }),
            RtcPeerEffect::RestartIce {
                generation,
                operation_id,
                ice_generation,
            } => self.with_session_ops(identity, completion_tx, move |ops| async move {
                let result = match ops {
                    Ok(ops) => ops.restart_ice().await,
                    Err(error) => Err(error),
                };
                EffectCompletion::RestartIce {
                    generation,
                    operation_id,
                    ice_generation,
                    result,
                }
            }),
            RtcPeerEffect::RefreshConfiguration {
                generation,
                operation_id,
                config,
            } => {
                let peer_id = self.peer_id.clone();
                self.with_session_ops(identity, completion_tx, move |ops| async move {
                    let result = match ops {
                        Ok(ops) => ops.set_configuration(*config).await,
                        Err(error) => Err(error),
                    };
                    if let Err(error) = &result {
                        tracing::error!(
                            peer = %peer_id.to_string_repr(),
                            generation,
                            error = %error,
                            "v0.20 PeerConnection configuration refresh failed"
                        );
                    }
                    EffectCompletion::RefreshConfiguration {
                        generation,
                        operation_id,
                        result,
                    }
                })
            }
            // S6-3: one operation in the bounded stats sample sequence used to
            // resolve network_path (best-effort observation).
            RtcPeerEffect::GetStats {
                generation,
                operation_id,
            } => self.with_session_ops(identity, completion_tx, move |ops| async move {
                let report = match ops {
                    Ok(ops) => ops.get_stats().await,
                    Err(_) => Default::default(),
                };
                EffectCompletion::GetStats {
                    generation,
                    operation_id,
                    report,
                }
            }),
            RtcPeerEffect::CloseSession {
                generation,
                operation_id,
            } => {
                let sessions = self.sessions.clone();
                let closing_sessions = self.closing_sessions.clone();
                let resources = self.resources.clone();
                self.spawn_effect_with(move || {
                    // Move an active Session to the retiring registry before
                    // asynchronous close starts. The transition and task
                    // admission are atomic with respect to runner shutdown.
                    let closing = sessions
                        .lock()
                        .expect("sessions lock")
                        .remove(&generation)
                        .map(|session| Arc::new(tokio::sync::Mutex::new(session)));
                    if let Some(session) = &closing {
                        closing_sessions
                            .lock()
                            .expect("closing sessions lock")
                            .insert(generation, session.clone());
                    }
                    let orphan_resources = resources
                        .lock()
                        .expect("resource registries lock")
                        .get(&generation)
                        .cloned();
                    async move {
                        let result = if let Some(session) = closing {
                            let result = session.lock().await.close_in_place().await;
                            closing_sessions
                                .lock()
                                .expect("closing sessions lock")
                                .remove(&generation);
                            result
                        } else {
                            if let Some(registry) = orphan_resources {
                                registry.close_all().await;
                            }
                            Ok(())
                        };
                        // The physical Session/raw registry is removed only
                        // after explicit resource close, and before completion.
                        resources
                            .lock()
                            .expect("resource registries lock")
                            .remove(&generation);
                        let _ = completion_tx
                            .send(TaggedRtcEffectCompletion::new(
                                identity,
                                EffectCompletion::CloseSession {
                                    generation,
                                    operation_id,
                                    result,
                                },
                            ))
                            .await;
                    }
                })
            }
            // SendSignaling and CreateLane are handled directly by the
            // controller (fire-and-forget / inline-await respectively).
            _ => Ok(()),
        }?;
        Ok(())
    }

    async fn create_lane(
        &self,
        generation: u64,
        lane: LaneKind,
    ) -> Result<Option<LaneHandle>, webrtc::error::Error> {
        WebRtcEffectRunner::create_lane(self, generation, lane).await
    }

    fn register_remote_channel(
        &self,
        generation: u64,
        resource_id: SessionResourceId,
        lane: LaneKind,
    ) -> ResourceRegistration<LaneHandle> {
        WebRtcEffectRunner::register_remote_channel(self, generation, resource_id, lane)
    }

    async fn register_remote_track(
        &self,
        generation: u64,
        resource_id: SessionResourceId,
        meta: crate::wire::webrtc::v020::media::TrackMeta,
    ) -> ResourceRegistration<String> {
        WebRtcEffectRunner::register_remote_track(self, generation, resource_id, meta)
    }

    async fn discard_remote_resource(&self, generation: u64, resource_id: SessionResourceId) {
        WebRtcEffectRunner::discard_remote_resource(self, generation, resource_id).await;
    }

    async fn add_track(
        &self,
        generation: u64,
        track: Arc<dyn webrtc::media_stream::track_local::TrackLocal>,
    ) -> Option<Result<Arc<dyn webrtc::rtp_transceiver::RtpSender>, webrtc::error::Error>> {
        WebRtcEffectRunner::add_track(self, generation, track).await
    }

    async fn remove_track(
        &self,
        generation: u64,
        sender: Arc<dyn webrtc::rtp_transceiver::RtpSender>,
    ) -> Option<Result<(), webrtc::error::Error>> {
        WebRtcEffectRunner::remove_track(self, generation, sender).await
    }

    async fn close_all_sessions(&self) {
        WebRtcEffectRunner::close_all_sessions(self).await;
    }
}

#[cfg(test)]
#[async_trait]
impl RtcPeerEffectPort for TestEffectSink {
    fn submit(
        &self,
        operation: TaggedRtcPeerEffect,
        completion_tx: RtcEffectCompletionSender,
    ) -> Result<(), RtcEffectPortError> {
        if operation.identity.peer_id != self.peer_id {
            return Err(RtcEffectPortError::WrongPeer);
        }
        let operation_id = operation.identity.operation_id;
        self.pending
            .lock()
            .expect("test effect pending lock")
            .insert(operation_id, (operation.identity.clone(), completion_tx));
        if self.effects_tx.send(operation).is_err() {
            self.pending
                .lock()
                .expect("test effect pending lock")
                .remove(&operation_id);
            return Err(RtcEffectPortError::TestSinkClosed);
        }
        Ok(())
    }

    async fn create_lane(
        &self,
        _generation: u64,
        _lane: LaneKind,
    ) -> Result<Option<LaneHandle>, webrtc::error::Error> {
        Ok(None)
    }

    fn register_remote_channel(
        &self,
        _generation: u64,
        _resource_id: SessionResourceId,
        _lane: LaneKind,
    ) -> ResourceRegistration<LaneHandle> {
        ResourceRegistration::MissingOrMismatched
    }

    async fn register_remote_track(
        &self,
        _generation: u64,
        _resource_id: SessionResourceId,
        _meta: crate::wire::webrtc::v020::media::TrackMeta,
    ) -> ResourceRegistration<String> {
        ResourceRegistration::MissingOrMismatched
    }

    async fn discard_remote_resource(&self, _generation: u64, _resource_id: SessionResourceId) {}

    async fn add_track(
        &self,
        _generation: u64,
        _track: Arc<dyn webrtc::media_stream::track_local::TrackLocal>,
    ) -> Option<Result<Arc<dyn webrtc::rtp_transceiver::RtpSender>, webrtc::error::Error>> {
        None
    }

    async fn remove_track(
        &self,
        _generation: u64,
        _sender: Arc<dyn webrtc::rtp_transceiver::RtpSender>,
    ) -> Option<Result<(), webrtc::error::Error>> {
        None
    }

    async fn close_all_sessions(&self) {
        self.close_all_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}
