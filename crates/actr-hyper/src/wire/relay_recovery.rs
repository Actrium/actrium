//! Bounded, authenticated Relay stream replay across reconnects of the same running Actor.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use actr_protocol::{ActrId, DataChunk, PayloadType, prost::Message as _};
use actr_relay_security::RecoveryControl;
use bytes::{BufMut, Bytes, BytesMut};
use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore, watch};
use tokio_util::sync::CancellationToken;

use crate::transport::{NetworkError, NetworkResult, VerifiedPeerContext};

const MAX_PEERS: usize = 256;
const RETAIN_BYTES: usize = 16 * 1024 * 1024;
const PEER_BYTES: usize = actr_relay_security::MAX_APPLICATION_BYTES;
const MAX_PENDING: usize = 256;
const MAX_RECEIPTS: usize = 512;
const MAX_EPOCHS: usize = 8;
const RETAIN_FOR: Duration = Duration::from_secs(30);
const MARKER: &[u8] = b"\0ACTR-DELIVERY\0";

/// Ticket validity is bound to the signed identity and runtime incarnation, so
/// a replacement process or a different actor key cannot receive old data.
type PeerKey = (ActrId, Vec<u8>, String, Vec<u8>, Vec<u8>);

pub(super) struct RecoveryBook {
    peers: Mutex<HashMap<PeerKey, Arc<Journal>>>,
    bytes: Arc<Semaphore>,
}

impl Default for RecoveryBook {
    fn default() -> Self {
        Self {
            peers: Mutex::default(),
            bytes: Arc::new(Semaphore::new(RETAIN_BYTES)),
        }
    }
}

impl RecoveryBook {
    pub(super) fn abandon_peer(&self, peer: &ActrId) {
        let peers = self.peers.lock().unwrap();
        for ((actor, ..), journal) in peers.iter() {
            if actor == peer {
                let mut outgoing = journal.outgoing.lock().unwrap();
                outgoing.pending.clear();
                outgoing.bytes = 0;
                journal.changed.notify_waiters();
            }
        }
    }

    pub(super) fn expire(&self) {
        self.peers.lock().unwrap().retain(|_, journal| {
            journal.outgoing.lock().unwrap().expire();
            Arc::strong_count(journal) > 1
                || journal.activity.lock().unwrap().elapsed() < RETAIN_FOR * 3
        });
    }

    pub(super) fn bind(&self, context: &VerifiedPeerContext) -> Option<Arc<Journal>> {
        let key = (
            context.peer_actor_id.clone(),
            context.peer_actor_public_key.clone(),
            context.peer_runtime_instance_id.clone(),
            context.current_policy_binding.clone(),
            context.current_state_binding.clone(),
        );
        let mut peers = self.peers.lock().unwrap();
        if let Some(existing) = peers.get(&key) {
            existing.touch();
            return Some(existing.clone());
        }
        // Keep the receive watermark for a live incarnation even when its
        // send spool is empty. Never evict an identity and then replay against
        // a fresh dedup window for it.
        if peers.len() == MAX_PEERS {
            return None;
        }
        let journal = Arc::new(Journal {
            bytes: self.bytes.clone(),
            outgoing: Mutex::default(),
            inbound: tokio::sync::Mutex::default(),
            epoch: rand::random::<u64>().max(1),
            activity: Mutex::new(Instant::now()),
            changed: Notify::new(),
        });
        peers.insert(key, journal.clone());
        Some(journal)
    }
}

struct Entry {
    frame: Bytes,
    expires: Instant,
    _permit: Arc<OwnedSemaphorePermit>,
}

#[derive(Default)]
struct Outgoing {
    last: u64,
    bytes: usize,
    pending: BTreeMap<u64, Entry>,
}

impl Outgoing {
    fn expire(&mut self) {
        let now = Instant::now();
        while self
            .pending
            .first_key_value()
            .is_some_and(|(_, entry)| entry.expires <= now)
        {
            let (_, entry) = self.pending.pop_first().expect("entry checked");
            self.bytes -= entry.frame.len();
        }
    }
}

/// Accepted IDs are usually contiguous. Keep bounded intervals instead of
/// evicting old IDs and accidentally admitting a delayed duplicate.
#[derive(Default)]
pub(super) struct ReceiptRanges(BTreeMap<u64, u64>);

impl ReceiptRanges {
    pub(super) fn contains(&self, id: u64) -> bool {
        self.0
            .range(..=id)
            .next_back()
            .is_some_and(|(_, end)| id <= *end)
    }

    pub(super) fn has_space(&self, id: u64) -> bool {
        self.0.len() < MAX_RECEIPTS
            || self.contains(id.saturating_sub(1))
            || self.0.contains_key(&id.saturating_add(1))
    }

    pub(super) fn insert(&mut self, id: u64) {
        let left = self
            .0
            .range(..id)
            .next_back()
            .and_then(|(&start, &end)| (end.checked_add(1) == Some(id)).then_some(start));
        let right = id.checked_add(1).and_then(|id| self.0.remove(&id));
        self.0.insert(left.unwrap_or(id), right.unwrap_or(id));
    }
}

pub(super) struct Journal {
    bytes: Arc<Semaphore>,
    outgoing: Mutex<Outgoing>,
    pub(super) inbound: tokio::sync::Mutex<BTreeMap<u64, ReceiptEpoch>>,
    pub(super) epoch: u64,
    activity: Mutex<Instant>,
    changed: Notify,
}

pub(super) struct ReplayFrame {
    pub(super) id: u64,
    pub(super) data: Bytes,
    pub(super) expires: Instant,
    _permit: Arc<OwnedSemaphorePermit>,
}

impl Journal {
    fn touch(&self) {
        *self.activity.lock().unwrap() = Instant::now();
    }

    pub(super) async fn record(&self, frame: Bytes) -> NetworkResult<u64> {
        if frame.len() > PEER_BYTES {
            return Err(NetworkError::InvalidArgument(
                "Relay recovery frame too large".into(),
            ));
        }
        loop {
            self.touch();
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let permit = self
                .bytes
                .clone()
                .acquire_many_owned(frame.len() as u32)
                .await
                .map_err(|_| {
                    NetworkError::WebSocketClosed("Relay recovery buffer closed".into())
                })?;
            let expires = {
                let mut outgoing = self.outgoing.lock().unwrap();
                outgoing.expire();
                if outgoing.pending.len() < MAX_PENDING
                    && outgoing.bytes + frame.len() <= PEER_BYTES
                {
                    let next = outgoing.last.checked_add(1).ok_or_else(|| {
                        NetworkError::SendError("Relay delivery sequence exhausted".into())
                    })?;
                    outgoing.last = next;
                    outgoing.bytes += frame.len();
                    outgoing.pending.insert(
                        next,
                        Entry {
                            frame,
                            expires: Instant::now() + RETAIN_FOR,
                            _permit: Arc::new(permit),
                        },
                    );
                    return Ok(next);
                }
                outgoing
                    .pending
                    .first_key_value()
                    .expect("full spool")
                    .1
                    .expires
            };
            drop(permit);
            tokio::select! {
                _ = &mut changed => {}
                _ = tokio::time::sleep_until(expires.into()) => {}
            }
        }
    }

    pub(super) fn pending(&self, id: u64) -> bool {
        self.outgoing.lock().unwrap().pending.contains_key(&id)
    }

    pub(super) fn replay(&self) -> Vec<ReplayFrame> {
        let mut outgoing = self.outgoing.lock().unwrap();
        outgoing.expire();
        outgoing
            .pending
            .iter()
            .map(|(&id, entry)| ReplayFrame {
                id,
                data: entry.frame.clone(),
                expires: entry.expires,
                _permit: entry._permit.clone(),
            })
            .collect()
    }

    fn receipt(&self, id: u64) -> NetworkResult<()> {
        self.touch();
        let mut outgoing = self.outgoing.lock().unwrap();
        if id > outgoing.last {
            return Err(NetworkError::DeserializationError(
                "Invalid Relay delivery receipt".into(),
            ));
        }
        if let Some(entry) = outgoing.pending.remove(&id) {
            outgoing.bytes -= entry.frame.len();
            self.changed.notify_waiters();
        }
        Ok(())
    }
}

pub(crate) struct RecoveryChannel {
    pub(super) journal: OnceLock<Arc<Journal>>,
    publish: watch::Sender<bool>,
    cancel: CancellationToken,
    next: Mutex<Option<(u64, u64)>>,
    receipts: Mutex<BTreeSet<(u64, u64)>>,
    changed: Notify,
}

impl RecoveryChannel {
    pub(super) fn new(cancel: CancellationToken) -> Arc<Self> {
        Arc::new(Self {
            journal: OnceLock::new(),
            publish: watch::channel(false).0,
            cancel,
            next: Mutex::new(None),
            receipts: Mutex::default(),
            changed: Notify::new(),
        })
    }

    pub(super) fn ready(&self) {
        self.publish.send_replace(true);
    }

    pub(crate) async fn await_replay(&self) -> NetworkResult<()> {
        if self.journal.get().is_none() {
            return Ok(());
        }
        let mut status = self.publish.subscribe();
        tokio::select! {
            biased;
            _ = self.cancel.cancelled() => Err(NetworkError::WebSocketClosed("Relay closed during delivery recovery".into())),
            result = status.wait_for(|ready| *ready) => result.map(|_| ())
                .map_err(|_| NetworkError::WebSocketClosed("Relay delivery recovery closed".into())),
        }
    }

    pub(super) fn receive(&self, control: RecoveryControl) -> NetworkResult<()> {
        let journal = self.journal.get().ok_or_else(|| {
            NetworkError::DeserializationError("Relay delivery recovery was not established".into())
        })?;
        match control {
            RecoveryControl::Received { epoch, id } => {
                if epoch == journal.epoch {
                    journal.receipt(id)
                } else {
                    Ok(())
                }
            }
            RecoveryControl::Message { epoch, id } => {
                journal.touch();
                let mut next = self.next.lock().unwrap();
                if next.is_some() {
                    return Err(NetworkError::DeserializationError(
                        "Overlapping Relay delivery markers".into(),
                    ));
                }
                *next = Some((epoch, id));
                Ok(())
            }
        }
    }

    pub(super) fn mark(&self, data: Vec<u8>) -> NetworkResult<Bytes> {
        let Some((epoch, id)) = self.next.lock().unwrap().take() else {
            if data.get(5..).is_some_and(|frame| frame.starts_with(MARKER)) {
                return Err(NetworkError::DeserializationError(
                    "Unframed Relay delivery marker".into(),
                ));
            }
            return Ok(data.into());
        };
        if !can_replay(&data) {
            return Err(NetworkError::DeserializationError(
                "Invalid recoverable Relay stream frame".into(),
            ));
        }
        let mut frame = BytesMut::with_capacity(data.len() + MARKER.len() + 16);
        frame.put_u8(PayloadType::StreamReliable as u8);
        frame.put_u32((data.len() - 5 + MARKER.len() + 16) as u32);
        frame.extend_from_slice(MARKER);
        frame.put_u64(epoch);
        frame.put_u64(id);
        frame.extend_from_slice(&data[5..]);
        Ok(frame.freeze())
    }

    pub(super) fn receipt(&self, epoch: u64, id: u64) {
        let mut receipts = self.receipts.lock().unwrap();
        if receipts.len() < MAX_RECEIPTS {
            receipts.insert((epoch, id));
            self.changed.notify_one();
        }
    }

    pub(super) async fn next_receipt(&self) -> RecoveryControl {
        loop {
            let changed = self.changed.notified();
            if let Some((epoch, id)) = self.receipts.lock().unwrap().pop_first() {
                return RecoveryControl::Received { epoch, id };
            }
            changed.await;
        }
    }
}

pub(super) fn can_replay(frame: &[u8]) -> bool {
    if frame.first() != Some(&(PayloadType::StreamReliable as u8)) {
        return false;
    }
    DataChunk::decode(frame.get(5..).unwrap_or_default()).is_ok_and(|chunk| {
        !chunk.stream_id.is_empty()
            && chunk.metadata.is_empty()
            && actr_protocol::transport_control::barrier_nonce(&chunk).is_none()
    })
}

pub(super) fn marked_chunk(data: &[u8]) -> Option<(u64, u64, &[u8])> {
    let encoded = data.strip_prefix(MARKER)?;
    let (epoch, encoded) = encoded.split_at_checked(8)?;
    let (id, payload) = encoded.split_at_checked(8)?;
    let epoch = u64::from_be_bytes(epoch.try_into().ok()?);
    let id = u64::from_be_bytes(id.try_into().ok()?);
    Some((epoch, id, payload))
}

pub(super) struct ReceiptEpoch {
    observed: Instant,
    pub(super) ranges: ReceiptRanges,
}

pub(super) fn admission_epoch(
    receipts: &mut BTreeMap<u64, ReceiptEpoch>,
    epoch: u64,
) -> Option<&mut ReceiptRanges> {
    // The sender's replay deadline is shorter than this retention period.
    receipts.retain(|_, entry| entry.observed.elapsed() < RETAIN_FOR * 3);
    if !receipts.contains_key(&epoch) && receipts.len() == MAX_EPOCHS {
        return None;
    }
    let now = Instant::now();
    let entry = receipts.entry(epoch).or_insert_with(|| ReceiptEpoch {
        observed: now,
        ranges: ReceiptRanges::default(),
    });
    entry.observed = now;
    Some(&mut entry.ranges)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipt_ranges_merge_without_acknowledging_unadmitted_gaps() {
        let mut receipts = ReceiptRanges::default();
        receipts.insert(1);
        receipts.insert(3);
        assert!(receipts.contains(1));
        assert!(!receipts.contains(2));
        assert!(receipts.contains(3));
        receipts.insert(2);
        assert_eq!(receipts.0, [(1, 3)].into_iter().collect());
    }

    #[tokio::test]
    async fn replay_holds_its_byte_reservation_even_after_receipt() {
        let journal = Journal {
            bytes: Arc::new(Semaphore::new(8)),
            outgoing: Mutex::default(),
            inbound: tokio::sync::Mutex::default(),
            epoch: 1,
            activity: Mutex::new(Instant::now()),
            changed: Notify::new(),
        };
        let id = journal
            .record(Bytes::from_static(b"original"))
            .await
            .unwrap();
        let replay = journal.replay();
        journal.receipt(id).unwrap();
        assert!(journal.replay().is_empty());
        assert_eq!(journal.bytes.available_permits(), 0);
        drop(replay);
        assert_eq!(journal.bytes.available_permits(), 8);
    }
}
