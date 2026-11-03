//! Peer-bound acknowledgement state for ordered reliable stream handover.

use futures::channel::oneshot;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

type Pending = HashMap<String, (String, oneshot::Sender<()>)>;

#[derive(Default)]
pub(crate) struct HandoverBook {
    pending: Mutex<Pending>,
}

impl HandoverBook {
    pub(crate) fn begin(self: &Arc<Self>, peer: &str) -> Option<PendingBarrier> {
        let mut pending = self.pending.lock();
        if pending.len() >= 1024 {
            return None;
        }
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let (sender, receiver) = oneshot::channel();
        pending.insert(nonce.clone(), (peer.into(), sender));
        Some(PendingBarrier {
            book: self.clone(),
            nonce,
            receiver,
        })
    }

    pub(crate) fn acknowledge(&self, peer: &str, nonce: &str) {
        let entry = {
            let mut pending = self.pending.lock();
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
}

pub(crate) struct PendingBarrier {
    book: Arc<HandoverBook>,
    pub(crate) nonce: String,
    pub(crate) receiver: oneshot::Receiver<()>,
}

impl Drop for PendingBarrier {
    fn drop(&mut self) {
        self.book.pending.lock().remove(&self.nonce);
    }
}
