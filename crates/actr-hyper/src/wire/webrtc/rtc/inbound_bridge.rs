//! RTC inbound bridge — routes RTC `InboundRouter` messages to the shared
//! `InboundDispatcher` (W3b).
//!
//! The RTC lane pumps deliver payloads to the `InboundRouter` (push model).
//! This bridge converts each `InboundMessage` to the dispatcher's format and
//! calls `dispatch()`, ensuring RTC inbound messages take the same path as
//! 0.14 (RPC → Mailbox/pending_requests, DataChunk → DataChunkRegistry).
//!
//! The bridge also handles stream-level uncertain-delivery notification: when
//! a lane closes with outstanding bytes, the bridge notifies the data-chunk
//! tracker so the business can observe the uncertainty.

use std::sync::Arc;

use actr_protocol::prost::Message as ProstMessage;
use actr_runtime_mailbox::Mailbox;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::inbound::dispatcher::InboundDispatcher;

use super::inbound::{InboundHandler, InboundMessage};

/// A shared, late-bound mailbox reference. The RTC Manager and its
/// `InboundRouter` are created before the node's mailbox exists; the bridge
/// holds a slot that is filled when the mailbox becomes available.
pub(crate) type MailboxSlot = Arc<RwLock<Option<Arc<dyn Mailbox>>>>;

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
    #[derive(Clone)]
    struct DispatchTasks {
        state: Arc<std::sync::Mutex<DispatchTaskState>>,
        tracker: TaskTracker,
    }

    #[derive(Default)]
    struct DispatchTaskState {
        closed: bool,
    }

    let tasks = DispatchTasks {
        state: Arc::new(std::sync::Mutex::new(DispatchTaskState::default())),
        tracker: TaskTracker::new(),
    };
    let handler_tasks = tasks.clone();
    let dispatch_shutdown = shutdown.clone();
    let handler: InboundHandler = std::sync::Arc::new(move |msg: InboundMessage, lease| {
        let dispatcher = dispatcher.clone();
        let mailbox_slot = mailbox_slot.clone();
        let shutdown = dispatch_shutdown.clone();
        // Admission and shutdown are serialized. Once the node starts closing,
        // no new detached dispatcher can appear after the convergence wait.
        let state = handler_tasks
            .state
            .lock()
            .expect("RTC inbound dispatch task lock");
        if state.closed {
            return;
        }
        handler_tasks.tracker.spawn(async move {
            // Retain the router's count+byte capacity through the complete
            // asynchronous dispatch. Dropping it earlier would bound only task
            // creation, allowing an unbounded set of dispatch tasks.
            let _lease = lease;
            let dispatch = async move {
                let mailbox = {
                    let guard = mailbox_slot.read().await;
                    guard.clone()
                };
                let Some(mailbox) = mailbox else {
                    tracing::warn!(
                        peer = %msg.peer.to_string_repr(),
                        generation = msg.generation,
                        lane = ?msg.lane,
                        "RTC inbound message dropped: mailbox not yet available"
                    );
                    return;
                };
                dispatcher
                    .dispatch(
                        msg.peer.encode_to_vec(),
                        msg.payload,
                        msg.lane.to_payload_type(),
                        &mailbox,
                    )
                    .await;
            };
            tokio::select! {
                _ = shutdown.cancelled() => {}
                _ = dispatch => {}
            }
        });
    });

    let monitor = tokio::spawn(async move {
        shutdown.cancelled().await;
        {
            let mut state = tasks.state.lock().expect("RTC inbound dispatch task lock");
            state.closed = true;
            tasks.tracker.close();
        }
        tasks.tracker.wait().await;
    });
    (handler, monitor)
}
