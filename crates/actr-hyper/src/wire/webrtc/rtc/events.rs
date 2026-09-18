//! Driver events and the non-blocking mailbox that carries them.
//!
//! Per RFC §"Command and event mailboxes" and §"WebRtcEventHandler", a
//! `WebRtcEventHandler` reports *facts* about one PC generation to the owning
//! `RtcPeerController` without ever awaiting capacity (RFC: "EventHandlers MUST
//! NOT wait for queue capacity"). The mailbox is split in two by event class,
//! exactly as the RFC recommends:
//!
//! * **Mergeable state events** (PC/ICE/gathering/signaling state) go to a
//!   bounded, per-generation latest-value `watch` book ([`PeerStateBook`]).
//!   Each generation keeps an independent [`PeerStateTick`], so a closing old
//!   PC can never lend `Failed`/`Closed` fields to a newer PC generation.
//! * **Structural events** (ICE candidate / data channel / track / negotiation /
//!   driver-exit) are not mergeable, so they use a bounded non-blocking queue.
//!   On overflow the event is dropped and an observable counter is incremented;
//!   the controller polls that counter and, on new overflow, treats the
//!   generation as unhealthy and hard-recovers (O1) rather than silently losing
//!   critical events (RFC §487-489).
//!
//! Both halves are published through the single non-blocking
//! [`PeerEventMailbox::try_publish`], so the handler stays a pure, non-blocking
//! translator (it never calls hooks, takes locks, does SDP, or starts pumps).

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use tokio::sync::{mpsc, watch};

use webrtc::peer_connection::{
    RTCIceCandidate, RTCIceConnectionState, RTCIceGatheringState, RTCPeerConnectionState,
    RTCSignalingState,
};

use super::media::TrackMeta;
use super::peer::session::SessionResourceId;

/// Default bound for the per-generation driver-event queue.
///
/// Sized so a burst of trickle candidates plus state transitions during a
/// single negotiation fits without overflowing under normal controller drain
/// rates; overflow is still handled (counted) rather than blocking the driver.
pub(crate) const PEER_EVENT_MAILBOX_CAPACITY: usize = 256;

/// Maximum number of physical generations whose latest mergeable state is
/// retained at once. A controller can have only building/active/pending plus a
/// small set of closing orphans; least-recently-updated stale generations are
/// evicted when a longer callback tail arrives.
const MAX_COALESCED_STATE_GENERATIONS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DataChannelDescriptor {
    pub(crate) label: String,
    pub(crate) negotiated: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct TrackDescriptor {
    pub(crate) meta: TrackMeta,
}

/// Facts reported by the RTC driver through `WebRtcEventHandler`.
///
/// Mirrors the normative enum in RFC §"WebRtcEventHandler" (lines ~538-571),
/// adapted to the real webrtc-rs type names. Every variant carries the
/// `generation` of the `PeerConnectionSession` that produced it so the consumer
/// can discard stale completions (RFC: "all async results and driver events can
/// be judged as to whether they belong to the current generation").
///
/// Two deliberate deviations from the RFC sketch, both forced by the real
/// RTC API and recorded in `docs/webrtc-v020-capability-matrix.md`:
///
/// * `DriverExited` is declared for forward compatibility but is **not** emitted
///   by the Handler: the RTC `PeerConnectionEventHandler` trait has no
///   driver-exit callback. Driver termination only surfaces as a terminal
///   connection-state change (`Closed`/`Failed`). Emitting an explicit
///   explicit driver exit is supplied by Session/Runtime supervision seams,
///   not by inventing a handler callback.
/// * `SignalingStateChanged` and `IceCandidateError` are added because the RTC
///   handler exposes them and the controller needs them for glare handling and
///   diagnostics.
pub(crate) enum PeerDriverEvent {
    PeerConnectionStateChanged {
        generation: u64,
        state: RTCPeerConnectionState,
    },
    IceConnectionStateChanged {
        generation: u64,
        state: RTCIceConnectionState,
    },
    IceGatheringStateChanged {
        generation: u64,
        state: RTCIceGatheringState,
    },
    SignalingStateChanged {
        generation: u64,
        state: RTCSignalingState,
    },
    LocalIceCandidate {
        generation: u64,
        candidate: RTCIceCandidate,
    },
    IceCandidateError {
        generation: u64,
        error_text: String,
    },
    DataChannelDiscovered {
        generation: u64,
        resource_id: SessionResourceId,
        descriptor: DataChannelDescriptor,
    },
    TrackDiscovered {
        generation: u64,
        resource_id: SessionResourceId,
        descriptor: TrackDescriptor,
    },
    NegotiationNeeded {
        generation: u64,
    },
    /// Declared for the Session/Runtime supervision seam. Not emitted directly
    /// by `PeerConnectionEventHandler` — see the enum-level doc comment.
    #[allow(dead_code)]
    DriverExited {
        generation: u64,
        error: Option<webrtc::error::Error>,
    },
}

impl PeerDriverEvent {
    /// The PC generation that produced this event.
    pub(crate) fn generation(&self) -> u64 {
        match self {
            Self::PeerConnectionStateChanged { generation, .. }
            | Self::IceConnectionStateChanged { generation, .. }
            | Self::IceGatheringStateChanged { generation, .. }
            | Self::SignalingStateChanged { generation, .. }
            | Self::LocalIceCandidate { generation, .. }
            | Self::IceCandidateError { generation, .. }
            | Self::DataChannelDiscovered { generation, .. }
            | Self::TrackDiscovered { generation, .. }
            | Self::NegotiationNeeded { generation }
            | Self::DriverExited { generation, .. } => *generation,
        }
    }
}

// `DataChannel` / `TrackRemote` are `Arc<dyn …>` and not `Debug`; format the
// structural variants with a placeholder instead of the trait object.
impl fmt::Debug for PeerDriverEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PeerConnectionStateChanged { generation, state } => f
                .debug_struct("PeerConnectionStateChanged")
                .field("generation", generation)
                .field("state", state)
                .finish(),
            Self::IceConnectionStateChanged { generation, state } => f
                .debug_struct("IceConnectionStateChanged")
                .field("generation", generation)
                .field("state", state)
                .finish(),
            Self::IceGatheringStateChanged { generation, state } => f
                .debug_struct("IceGatheringStateChanged")
                .field("generation", generation)
                .field("state", state)
                .finish(),
            Self::SignalingStateChanged { generation, state } => f
                .debug_struct("SignalingStateChanged")
                .field("generation", generation)
                .field("state", state)
                .finish(),
            Self::LocalIceCandidate {
                generation,
                candidate,
            } => f
                .debug_struct("LocalIceCandidate")
                .field("generation", generation)
                .field("address", &candidate.address)
                .field("port", &candidate.port)
                .field("typ", &candidate.typ)
                .finish(),
            Self::IceCandidateError {
                generation,
                error_text,
            } => f
                .debug_struct("IceCandidateError")
                .field("generation", generation)
                .field("error_text", error_text)
                .finish(),
            Self::DataChannelDiscovered {
                generation,
                resource_id,
                descriptor,
            } => f
                .debug_struct("DataChannelDiscovered")
                .field("generation", generation)
                .field("resource_id", resource_id)
                .field("descriptor", descriptor)
                .finish(),
            Self::TrackDiscovered {
                generation,
                resource_id,
                descriptor,
            } => f
                .debug_struct("TrackDiscovered")
                .field("generation", generation)
                .field("resource_id", resource_id)
                .field("descriptor", descriptor)
                .finish(),
            Self::NegotiationNeeded { generation } => f
                .debug_struct("NegotiationNeeded")
                .field("generation", generation)
                .finish(),
            Self::DriverExited { generation, error } => f
                .debug_struct("DriverExited")
                .field("generation", generation)
                .field("error", &error.as_ref().map(|e| e.to_string()))
                .finish(),
        }
    }
}

/// The latest mergeable connection/ICE state for one physical generation.
///
/// Mergeable state changes (PC/ICE/gathering/signaling) are *not* queued —
/// Each new value overwrites only the same generation's prior field in
/// [`PeerStateBook`] and bumps `seq`, so a state-change storm cannot overflow a
/// queue or contaminate a concurrent replacement generation. Structural events
/// remain non-mergeable and use the bounded queue (see [`PeerEventMailbox`]).
#[derive(Debug, Clone, Default)]
pub(crate) struct PeerStateTick {
    pub(crate) generation: u64,
    /// Monotonic sequence, bumped on every published state change so a consumer
    /// can tell "a newer state exists" even when the value is unchanged.
    pub(crate) seq: u64,
    pub(crate) pc_state: Option<RTCPeerConnectionState>,
    pub(crate) ice_state: Option<RTCIceConnectionState>,
    pub(crate) gathering_state: Option<RTCIceGatheringState>,
    pub(crate) signaling_state: Option<RTCSignalingState>,
    /// Structural events dropped for this exact generation. Overflow is a
    /// generation-scoped health fact; a late old-PC storm must not force Hard
    /// Recovery of the current PC.
    pub(crate) overflow_count: u64,
}

impl PeerStateTick {
    pub(crate) fn has_driver_state(&self) -> bool {
        self.pc_state.is_some()
            || self.ice_state.is_some()
            || self.gathering_state.is_some()
            || self.signaling_state.is_some()
    }
}

/// Bounded per-generation latest-value state carried by one peer mailbox.
///
/// `watch` may coalesce notifications, so one scalar tick is not sufficient:
/// callbacks from two concurrently live generations can overwrite each other.
/// The book retains the newest complete tick for every recently active
/// generation and gives every update a peer-global sequence. Consumers replay
/// all ticks newer than their last sequence in publication order.
#[derive(Debug, Clone, Default)]
pub(crate) struct PeerStateBook {
    seq: u64,
    ticks: BTreeMap<u64, PeerStateTick>,
}

impl PeerStateBook {
    fn modify_generation(&mut self, generation: u64, modify: impl FnOnce(&mut PeerStateTick)) {
        self.seq = self.seq.wrapping_add(1).max(1);
        let tick = self
            .ticks
            .entry(generation)
            .or_insert_with(|| PeerStateTick {
                generation,
                ..PeerStateTick::default()
            });
        modify(tick);
        tick.seq = self.seq;

        if self.ticks.len() > MAX_COALESCED_STATE_GENERATIONS
            && let Some(stalest_generation) = self
                .ticks
                .values()
                .min_by_key(|tick| tick.seq)
                .map(|tick| tick.generation)
        {
            self.ticks.remove(&stalest_generation);
        }
    }

    /// Return every generation updated after `last_seq`, preserving the
    /// peer-global publication order even when `watch` merged notifications.
    pub(crate) fn ticks_after(&self, last_seq: u64) -> Vec<PeerStateTick> {
        let mut ticks = self
            .ticks
            .values()
            .filter(|tick| tick.seq > last_seq)
            .cloned()
            .collect::<Vec<_>>();
        ticks.sort_unstable_by_key(|tick| tick.seq);
        ticks
    }

    pub(crate) fn sequence(&self) -> u64 {
        self.seq
    }

    fn latest_tick(&self) -> PeerStateTick {
        self.ticks
            .values()
            .max_by_key(|tick| tick.seq)
            .cloned()
            .unwrap_or_default()
    }

    pub(crate) fn tick_for_generation(&self, generation: u64) -> Option<PeerStateTick> {
        self.ticks.get(&generation).cloned()
    }

    pub(crate) fn contains_generation(&self, generation: u64) -> bool {
        self.ticks.contains_key(&generation)
    }
}

#[cfg(feature = "test-utils")]
pub(crate) type PeerDriverEventObserver = Arc<dyn Fn(&PeerDriverEvent) + Send + Sync>;

/// Sending half of a peer's driver-event mailbox, owned by the handler.
///
/// `Clone` so the controller can hand a clone to each generation's handler
/// while keeping one for supervision; clones feed the same queue/state slot.
#[derive(Clone)]
pub(crate) struct PeerEventMailbox {
    structural_tx: mpsc::Sender<QueuedStructuralEvent>,
    state_tx: watch::Sender<PeerStateBook>,
    overflow_count: Arc<AtomicU64>,
    structural_bytes: Arc<AtomicUsize>,
    max_structural_bytes: usize,
    #[cfg(feature = "test-utils")]
    observer: Option<PeerDriverEventObserver>,
}

struct QueuedStructuralEvent {
    event: PeerDriverEvent,
    _lease: StructuralByteLease,
}

struct StructuralByteLease {
    bytes: usize,
    structural_bytes: Arc<AtomicUsize>,
}

impl Drop for StructuralByteLease {
    fn drop(&mut self) {
        let _ =
            self.structural_bytes
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                    Some(current.saturating_sub(self.bytes))
                });
    }
}

fn structural_event_bytes(event: &PeerDriverEvent) -> usize {
    let base = std::mem::size_of::<PeerDriverEvent>();
    match event {
        PeerDriverEvent::LocalIceCandidate { candidate, .. } => {
            base + candidate.id.len()
                + candidate.foundation.len()
                + candidate.address.len()
                + candidate.related_address.len()
                + candidate.url.as_ref().map_or(0, String::len)
        }
        PeerDriverEvent::IceCandidateError { error_text, .. } => base + error_text.len(),
        PeerDriverEvent::DataChannelDiscovered { descriptor, .. } => base + descriptor.label.len(),
        PeerDriverEvent::TrackDiscovered { descriptor, .. } => {
            base + descriptor.meta.track_id.len() + descriptor.meta.codec.len()
        }
        PeerDriverEvent::DriverExited { error, .. } => {
            base + error.as_ref().map_or(0, |error| error.to_string().len())
        }
        _ => base,
    }
}

impl PeerEventMailbox {
    /// Attach a synchronous observer at the Handler→mailbox boundary. This is
    /// used by integration-test evidence only; production mailboxes have no
    /// observer and retain the same non-blocking path.
    #[cfg(feature = "test-utils")]
    pub(crate) fn with_observer(mut self, observer: PeerDriverEventObserver) -> Self {
        self.observer = Some(observer);
        self
    }

    /// Non-blocking publish. This is the *only* send primitive the handler is
    /// allowed to use: it never waits for capacity (RFC hard rule).
    ///
    /// Mergeable state events go to the latest-value slot (never overflow).
    /// Structural events go to the bounded queue: on a full queue the event is
    /// dropped and the overflow counter is incremented so the controller can
    /// observe the loss and treat the generation as unhealthy (RFC §487-489).
    /// On a closed queue (controller already gone) the event is dropped
    /// silently — a stale generation reporting after teardown is expected.
    pub(crate) fn try_publish(&self, event: PeerDriverEvent) {
        #[cfg(feature = "test-utils")]
        if let Some(observer) = &self.observer {
            observer(&event);
        }
        match event {
            // Mergeable state → per-generation latest-value book (notify,
            // never Full). Fields from different generations never share a
            // tick, which is essential while an old PC closes beside a fresh
            // building/pending PC.
            PeerDriverEvent::PeerConnectionStateChanged { generation, state } => {
                self.state_tx.send_modify(|book| {
                    book.modify_generation(generation, |tick| tick.pc_state = Some(state));
                })
            }
            PeerDriverEvent::IceConnectionStateChanged { generation, state } => {
                self.state_tx.send_modify(|book| {
                    book.modify_generation(generation, |tick| tick.ice_state = Some(state));
                })
            }
            PeerDriverEvent::IceGatheringStateChanged { generation, state } => {
                self.state_tx.send_modify(|book| {
                    book.modify_generation(generation, |tick| tick.gathering_state = Some(state));
                })
            }
            PeerDriverEvent::SignalingStateChanged { generation, state } => {
                self.state_tx.send_modify(|book| {
                    book.modify_generation(generation, |tick| tick.signaling_state = Some(state));
                })
            }
            // Structural → bounded queue with overflow counting.
            structural => {
                let generation = structural.generation();
                let bytes = structural_event_bytes(&structural);
                let reserved = self
                    .structural_bytes
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                        current
                            .checked_add(bytes)
                            .filter(|next| *next <= self.max_structural_bytes)
                    })
                    .is_ok();
                if !reserved {
                    self.mark_overflow(generation);
                    return;
                }
                let queued = QueuedStructuralEvent {
                    event: structural,
                    _lease: StructuralByteLease {
                        bytes,
                        structural_bytes: self.structural_bytes.clone(),
                    },
                };
                match self.structural_tx.try_send(queued) {
                    Ok(()) => {}
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        self.mark_overflow(generation);
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => {}
                }
            }
        }
    }

    /// Mark an ingress overflow that occurred before an event could enter the
    /// structural queue (for example the raw Session resource registry was
    /// full). The controller treats it exactly like queue count/byte overflow.
    pub(crate) fn mark_overflow(&self, generation: u64) {
        let n = self.overflow_count.fetch_add(1, Ordering::Relaxed) + 1;
        self.state_tx.send_modify(|book| {
            book.modify_generation(generation, |tick| {
                tick.overflow_count = tick.overflow_count.saturating_add(1);
            });
        });
        if n.is_power_of_two() {
            tracing::warn!(
                generation,
                overflow_count = n,
                "peer driver-event ingress full; dropping structural event \
                 (generation will be treated unhealthy)"
            );
        }
    }

    /// Number of structural events dropped because the queue was full.
    pub(crate) fn overflow_count(&self) -> u64 {
        self.overflow_count.load(Ordering::Relaxed)
    }
}

/// Receiving half of a peer's driver-event mailbox, owned by the controller
/// (or by a T2 validation harness).
pub(crate) struct PeerEventMailboxReceiver {
    structural_rx: mpsc::Receiver<QueuedStructuralEvent>,
    state_rx: watch::Receiver<PeerStateBook>,
    overflow_count: Arc<AtomicU64>,
}

impl PeerEventMailboxReceiver {
    /// Wait for the next structural driver event (candidate / data channel /
    /// track / negotiation / driver-exit).
    pub(crate) async fn recv_structural(&mut self) -> Option<PeerDriverEvent> {
        self.structural_rx.recv().await.map(|queued| queued.event)
    }

    /// A clone of the latest-value state watch. The controller `.changed()`s on
    /// it to be woken when the coalesced state advances, and `.borrow()`s the
    /// newest value — mergeable states are only ever applied at their latest.
    pub(crate) fn state(&self) -> watch::Receiver<PeerStateBook> {
        self.state_rx.clone()
    }

    /// The most recently published coalesced state (non-blocking).
    pub(crate) fn latest_state(&self) -> PeerStateTick {
        self.state_rx.borrow().latest_tick()
    }

    /// Latest coalesced state for one exact physical generation.
    pub(crate) fn latest_state_for_generation(&self, generation: u64) -> Option<PeerStateTick> {
        self.state_rx.borrow().tick_for_generation(generation)
    }

    /// Number of structural events dropped because the queue was full. The
    /// controller compares this against the value it last saw to detect new
    /// overflow and treat the generation as unhealthy (O1).
    pub(crate) fn overflow_count(&self) -> u64 {
        self.overflow_count.load(Ordering::Relaxed)
    }
}

/// Create a driver-event mailbox with the default structural-queue capacity.
pub(crate) fn peer_event_mailbox() -> (PeerEventMailbox, PeerEventMailboxReceiver) {
    peer_event_mailbox_with_capacity_and_bytes(
        PEER_EVENT_MAILBOX_CAPACITY,
        super::config::RtcBufferConfig::default().max_structural_event_bytes,
    )
}

/// Create a driver-event mailbox with an explicit structural-queue capacity
/// (tests use a tiny capacity to exercise the overflow path deterministically).
pub(crate) fn peer_event_mailbox_with_capacity(
    capacity: usize,
) -> (PeerEventMailbox, PeerEventMailboxReceiver) {
    peer_event_mailbox_with_capacity_and_bytes(
        capacity,
        super::config::RtcBufferConfig::default().max_structural_event_bytes,
    )
}

pub(crate) fn peer_event_mailbox_with_capacity_and_bytes(
    capacity: usize,
    max_structural_bytes: usize,
) -> (PeerEventMailbox, PeerEventMailboxReceiver) {
    let (structural_tx, structural_rx) = mpsc::channel(capacity);
    let (state_tx, state_rx) = watch::channel(PeerStateBook::default());
    let overflow_count = Arc::new(AtomicU64::new(0));
    let structural_bytes = Arc::new(AtomicUsize::new(0));
    (
        PeerEventMailbox {
            structural_tx,
            state_tx,
            overflow_count: overflow_count.clone(),
            structural_bytes,
            max_structural_bytes,
            #[cfg(feature = "test-utils")]
            observer: None,
        },
        PeerEventMailboxReceiver {
            structural_rx,
            state_rx,
            overflow_count,
        },
    )
}
