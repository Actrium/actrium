//! RTC inbound bridge — routes RTC `InboundRouter` messages to the shared
//! `InboundDispatcher` (W3b).
//!
//! The RTC lane pumps deliver payloads to the `InboundRouter` (push model).
//! This bridge converts each `InboundMessage` to the dispatcher's format and
//! calls `dispatch()`, ensuring RTC inbound messages take the same path as
//! 0.14 (RPC → Mailbox/pending_requests, DataChunk → DataChunkRegistry).
//!
//! Dispatch preserves the legacy `WebRtcGate` ordering contract. Every message
//! belongs to one of three traffic classes — RPC (`RpcReliable` and
//! `RpcSignal`, which the 0.14 gate drained from a single `receive_rpc_message`
//! loop), `StreamReliable`, and `StreamLatencyFirst` — and each class is
//! drained by exactly one FIFO worker. Same-class messages are dispatched
//! strictly in arrival order regardless of peer, generation, or lane, while the
//! three classes make progress independently once admitted.
//!
//! Those guarantees only cover messages the bridge receives. Admission happens
//! earlier, in `InboundRouter`, which enforces a single message and byte budget
//! across every lane and keeps each accepted message's capacity lease until its
//! dispatch completes. A saturated class therefore consumes shared admission
//! capacity, and further messages — including ones belonging to an idle class —
//! are dropped by `InboundRouter::route()` before any class worker can run.
//! `bridge_admission_is_shared_across_classes` pins that overload behavior.
//!
//! The bridge also handles stream-level uncertain-delivery notification: when
//! a lane closes with outstanding bytes, the bridge notifies the data-chunk
//! tracker so the business can observe the uncertainty.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use actr_protocol::prost::Message as ProstMessage;
use actr_runtime_mailbox::Mailbox;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::inbound::dispatcher::InboundDispatcher;

use super::inbound::{InboundHandler, InboundMessage};
use super::status::LaneKind;

/// A shared, late-bound mailbox reference. The RTC Manager and its
/// `InboundRouter` are created before the node's mailbox exists; the bridge
/// holds a slot that is filled when the mailbox becomes available.
pub(crate) type MailboxSlot = Arc<RwLock<Option<Arc<dyn Mailbox>>>>;

/// The FIFO scope of one inbound dispatch worker.
///
/// The pre-v0.20 gate kept three long-lived sequential receive loops:
/// `RpcReliable` and `RpcSignal` were read from the same `receive_rpc_message()`
/// queue, `StreamReliable` had its own loop, and `StreamLatencyFirst` had an
/// independent one. Keying queues by traffic class — instead of by peer,
/// generation, and lane — restores that contract, so a class stays FIFO across
/// peers and generations even though the three classes run concurrently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum DispatchClass {
    /// `RpcReliable` and `RpcSignal` share one FIFO worker.
    Rpc,
    /// Reliable stream chunks; blocking on a full queue applies backpressure.
    StreamReliable,
    /// Latency-first chunks, isolated from `StreamReliable` so a backpressured
    /// reliable stream cannot stall them upstream of the registry's
    /// drop-newest policy.
    StreamLatencyFirst,
}

impl DispatchClass {
    fn from_lane(lane: LaneKind) -> Self {
        match lane {
            LaneKind::RpcReliable | LaneKind::RpcSignal => Self::Rpc,
            LaneKind::StreamReliable => Self::StreamReliable,
            LaneKind::StreamLatencyFirst => Self::StreamLatencyFirst,
        }
    }
}

struct DispatchItem {
    message: InboundMessage,
    lease: super::inbound::InboundLease,
}

#[derive(Default)]
struct DispatchTaskState {
    closed: bool,
    queues: HashMap<DispatchClass, VecDeque<DispatchItem>>,
    running: HashSet<DispatchClass>,
}

#[derive(Clone)]
struct DispatchTasks {
    state: Arc<std::sync::Mutex<DispatchTaskState>>,
    tracker: TaskTracker,
}

fn spawn_dispatch_worker(
    tasks: &DispatchTasks,
    class: DispatchClass,
    dispatcher: Arc<InboundDispatcher>,
    mailbox_slot: MailboxSlot,
    shutdown: CancellationToken,
) {
    let state = tasks.state.clone();
    tasks.tracker.spawn(async move {
        loop {
            let item = {
                let mut state = state.lock().expect("RTC inbound dispatch task lock");
                if state.closed {
                    return;
                }
                let item = state.queues.get_mut(&class).and_then(VecDeque::pop_front);
                if item.is_none() {
                    state.queues.remove(&class);
                    state.running.remove(&class);
                }
                item
            };
            let Some(DispatchItem { message, lease }) = item else {
                break;
            };
            // Retain the router's capacity lease through the complete
            // asynchronous dispatch. Dropping it earlier would bound only
            // task creation, allowing queued work to bypass admission limits.
            let _lease = lease;
            let dispatch = async {
                let mailbox = {
                    let guard = mailbox_slot.read().await;
                    guard.clone()
                };
                let Some(mailbox) = mailbox else {
                    tracing::warn!(
                        peer = %message.peer.to_string_repr(),
                        generation = message.generation,
                        lane = ?message.lane,
                        "RTC inbound message dropped: mailbox not yet available"
                    );
                    return;
                };
                dispatcher
                    .dispatch(
                        message.peer.encode_to_vec(),
                        message.payload,
                        message.lane.to_payload_type(),
                        &mailbox,
                    )
                    .await;
            };
            tokio::select! {
                _ = shutdown.cancelled() => return,
                _ = dispatch => {}
            }
        }
    });
}

/// Create an [`InboundHandler`] that routes RTC inbound messages to the
/// shared `InboundDispatcher`.
///
/// The `mailbox_slot` is late-bound: messages arriving before the mailbox is
/// set are dropped with a warning (they can only arrive before the node starts
/// its receive loop, which is a bootstrap race).
pub(crate) fn dispatcher_bridge(
    dispatcher: Arc<InboundDispatcher>,
    mailbox_slot: MailboxSlot,
    shutdown: CancellationToken,
) -> (InboundHandler, JoinHandle<()>) {
    let tasks = DispatchTasks {
        state: Arc::new(std::sync::Mutex::new(DispatchTaskState::default())),
        tracker: TaskTracker::new(),
    };
    let handler_tasks = tasks.clone();
    let dispatch_shutdown = shutdown.clone();
    let handler: InboundHandler = std::sync::Arc::new(move |msg: InboundMessage, lease| {
        let class = DispatchClass::from_lane(msg.lane);
        // Keep admission and worker registration atomic with shutdown.
        // TaskTracker::close() does not prevent spawning new tasks, so the
        // worker must be tracked before the monitor can close and wait.
        let mut state = handler_tasks
            .state
            .lock()
            .expect("RTC inbound dispatch task lock");
        if state.closed {
            return;
        }
        state
            .queues
            .entry(class)
            .or_default()
            .push_back(DispatchItem {
                message: msg,
                lease,
            });
        if state.running.insert(class) {
            spawn_dispatch_worker(
                &handler_tasks,
                class,
                dispatcher.clone(),
                mailbox_slot.clone(),
                dispatch_shutdown.clone(),
            );
        }
    });

    let monitor = tokio::spawn(async move {
        shutdown.cancelled().await;
        {
            let mut state = tasks.state.lock().expect("RTC inbound dispatch task lock");
            state.closed = true;
            state.queues.clear();
            tasks.tracker.close();
        }
        tasks.tracker.wait().await;
    });
    (handler, monitor)
}
