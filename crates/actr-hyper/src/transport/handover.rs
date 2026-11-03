//! Peer-bound acknowledgements for reliable stream transport handover.

use super::{Dest, PeerTransport};
use crate::timer;
use actr_protocol::prost::Message;
use actr_protocol::{ActrId, PayloadType, transport_control};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use tokio::sync::oneshot;

type Pending = HashMap<String, (ActrId, oneshot::Sender<()>)>;

#[derive(Default)]
pub(crate) struct HandoverBook {
    pending: Mutex<Pending>,
    transport: OnceLock<Weak<PeerTransport>>,
}

impl HandoverBook {
    pub(crate) fn bind(&self, transport: &Arc<PeerTransport>) {
        let _ = self.transport.set(Arc::downgrade(transport));
    }

    pub(crate) fn begin(self: &Arc<Self>, peer: &ActrId) -> Option<PendingBarrier> {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if pending.len() >= 1024 {
            return None;
        }
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let (sender, receiver) = oneshot::channel();
        pending.insert(nonce.clone(), (peer.clone(), sender));
        Some(PendingBarrier {
            book: self.clone(),
            nonce,
            receiver,
        })
    }

    pub(crate) fn acknowledge(&self, peer: &ActrId, nonce: &str) {
        let entry = {
            let mut pending = self
                .pending
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if !pending
                .get(nonce)
                .is_some_and(|(expected, _)| expected == peer)
            {
                return;
            }
            pending.remove(nonce)
        };
        if let Some((_, sender)) = entry {
            let _ = sender.send(());
        }
    }

    /// Called by the ordered receive pump after earlier queue admissions.
    pub(crate) async fn receive_barrier(&self, peer: ActrId, nonce: &str) {
        let Some(transport) = self.transport.get().and_then(Weak::upgrade) else {
            return;
        };
        let data = transport_control::acknowledgement(nonce).encode_to_vec();
        // An acknowledgement must not dial a peer or wait indefinitely behind
        // an unavailable route. The sender will retain its healthy old route.
        let _ = timer::timeout(
            timer::ids::TRANSPORT_HANDOVER_ACK,
            std::time::Duration::from_secs(1),
            transport.send_existing_with_identity(
                &Dest::Peer(peer),
                PayloadType::RpcSignal,
                &data,
                None,
            ),
        )
        .await;
    }
}

pub(crate) struct PendingBarrier {
    book: Arc<HandoverBook>,
    pub(crate) nonce: String,
    pub(crate) receiver: oneshot::Receiver<()>,
}

impl Drop for PendingBarrier {
    fn drop(&mut self) {
        self.book
            .pending
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.nonce);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn acknowledgement_is_peer_bound_and_cancellation_removes_pending_state() {
        let book = Arc::new(HandoverBook::default());
        let peer = ActrId::default();
        let mut pending = book.begin(&peer).unwrap();
        let mut other = peer.clone();
        other.serial_number = 1;
        book.acknowledge(&other, &pending.nonce);
        assert!(matches!(
            pending.receiver.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
        book.acknowledge(&peer, &pending.nonce);
        assert_eq!((&mut pending.receiver).await, Ok(()));
        drop(pending);
        let cancelled = book.begin(&peer).unwrap();
        drop(cancelled);
        assert!(book.pending.lock().unwrap().is_empty());
    }
}
