//! In-memory signaling hub for tests: a set of connected [`SignalingTransport`]s
//! that relay messages between any two nodes and arbitrate role negotiation
//! deterministically, standing in for the signaling server.
//!
//! This is a *test* transport only. It proves the router + controllers +
//! real-PeerConnection flow end to end (offer/answer/candidate/role) without a
//! WebSocket server; the production transport binds the 0.14 signaling client.
//! Role arbitration mirrors the mock actrix rule (`from.serial_number <
//! to.serial_number` → `from` is the offerer), which is deterministic and
//! symmetric, so a simultaneous connect on both ends converges to one offerer.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

use actr_protocol::ActrId;
use tokio::sync::{Mutex as TokioMutex, mpsc};

use super::router::SignalingTransport;
use crate::wire::webrtc::v020::peer::reducer::SignalingMessage;

/// One endpoint's transport onto the in-memory hub.
pub(crate) struct InMemoryTransport {
    local_id: ActrId,
    outbound_tx: mpsc::Sender<(ActrId, ActrId, SignalingMessage)>,
    /// Tokio mutex because `recv` holds the guard across its `.await`.
    inbound_rx: Arc<TokioMutex<mpsc::Receiver<(ActrId, SignalingMessage)>>>,
}

#[async_trait::async_trait]
impl SignalingTransport for InMemoryTransport {
    async fn send(&self, to: ActrId, message: SignalingMessage) -> Result<(), String> {
        self.outbound_tx
            .send((self.local_id.clone(), to, message))
            .await
            .map_err(|_| "in-memory signaling hub closed".to_string())
    }

    async fn recv(&self) -> Option<(ActrId, SignalingMessage)> {
        self.inbound_rx.lock().await.recv().await
    }
}

/// A central in-memory signaling hub. Nodes [`Self::connect`] to it; the hub
/// relays messages between them and answers role negotiation for both ends.
pub(crate) struct InMemorySignalingHub {
    hub_tx: mpsc::Sender<(ActrId, ActrId, SignalingMessage)>,
    nodes: StdMutex<HashMap<ActrId, mpsc::Sender<(ActrId, SignalingMessage)>>>,
}

impl InMemorySignalingHub {
    pub(crate) fn new() -> Arc<Self> {
        let (hub_tx, hub_rx) = mpsc::channel(256);
        let hub = Arc::new(Self {
            hub_tx,
            nodes: StdMutex::new(HashMap::new()),
        });
        tokio::spawn(run_hub(hub.clone(), hub_rx));
        hub
    }

    /// Attach a node to the hub, returning its transport.
    pub(crate) fn connect(&self, id: ActrId) -> InMemoryTransport {
        let (in_tx, in_rx) = mpsc::channel(64);
        self.nodes
            .lock()
            .expect("hub nodes lock")
            .insert(id.clone(), in_tx);
        InMemoryTransport {
            local_id: id,
            outbound_tx: self.hub_tx.clone(),
            inbound_rx: Arc::new(TokioMutex::new(in_rx)),
        }
    }

    async fn send_to(&self, to: &ActrId, from: ActrId, message: SignalingMessage) {
        let tx = self.nodes.lock().expect("hub nodes lock").get(to).cloned();
        if let Some(tx) = tx {
            let _ = tx.send((from, message)).await;
        }
    }
}

async fn run_hub(
    hub: Arc<InMemorySignalingHub>,
    mut hub_rx: mpsc::Receiver<(ActrId, ActrId, SignalingMessage)>,
) {
    while let Some((from, to, message)) = hub_rx.recv().await {
        match message {
            SignalingMessage::RoleNegotiation => {
                // Deterministic, symmetric arbitration (mirrors the mock actrix):
                // `from` is the offerer iff `from.serial_number < to.serial_number`.
                // Tell the *sender* its role (from = the remote peer), and the
                // *target* its role, so both ends agree on exactly one offerer.
                let from_is_offerer = from.serial_number < to.serial_number;
                hub.send_to(
                    &from,
                    to.clone(),
                    SignalingMessage::RoleAssignment {
                        is_offerer: from_is_offerer,
                    },
                )
                .await;
                hub.send_to(
                    &to,
                    from.clone(),
                    SignalingMessage::RoleAssignment {
                        is_offerer: !from_is_offerer,
                    },
                )
                .await;
            }
            other => hub.send_to(&to, from, other).await,
        }
    }
}

/// Convenience: a connected pair of transports for two nodes.
#[allow(dead_code)]
pub(crate) fn in_memory_signaling_pair(
    a: ActrId,
    b: ActrId,
) -> (InMemoryTransport, InMemoryTransport) {
    let hub = InMemorySignalingHub::new();
    (hub.connect(a), hub.connect(b))
}
