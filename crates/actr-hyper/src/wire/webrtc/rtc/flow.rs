//! Per-generation receive credit for WebRTC application DataChannels.

use std::sync::{Arc, Mutex};

use actr_relay_security::flow_control::{
    Class, Control, RTC_CONTROL_PREFIX, RTC_CREDITED_FRAGMENT,
};
use actr_relay_security::peer_flow::{MAX_CHANNELS, PeerFlow, SHARED_HEADROOM};
use bytes::{BufMut, Bytes, BytesMut};
use tokio::sync::{Notify, Semaphore, mpsc};
use tokio_util::sync::CancellationToken;

use super::{peer::adapter::LaneDataChannel, status::LaneKind};

/// Fragment count zero is reserved. The marked data count carries a four-byte
/// application length in every fragment, for admission before reassembly.
const LENGTH: usize = 4;

pub(super) const CREDITED_PAYLOAD: usize = crate::transport::DC_MAX_PAYLOAD_SIZE - LENGTH;

pub(super) fn class(lane: LaneKind) -> Option<Class> {
    match lane {
        LaneKind::RpcReliable => Some(Class::Rpc),
        LaneKind::StreamReliable => Some(Class::Stream),
        _ => None,
    }
}

#[derive(Clone)]
pub(super) struct Pools {
    base: Arc<Semaphore>,
    large: Arc<Semaphore>,
    waits: Arc<Semaphore>,
}

impl Default for Pools {
    fn default() -> Self {
        const BYTES: usize = 16 * 1024 * 1024;
        Self {
            base: Arc::new(Semaphore::new(MAX_CHANNELS)),
            large: Arc::new(Semaphore::new(BYTES - SHARED_HEADROOM)),
            waits: Arc::new(Semaphore::new(BYTES)),
        }
    }
}

pub(super) struct SessionFlow {
    pub(super) peer: Arc<PeerFlow>,
    open: Notify,
    controls: Mutex<Option<mpsc::Receiver<Control>>>,
    cancellation: CancellationToken,
    pools: Pools,
}

impl SessionFlow {
    pub(super) fn new(pools: Pools, cancellation: CancellationToken) -> Arc<Self> {
        let (peer, controls) = PeerFlow::new(cancellation.clone(), pools.waits.clone());
        peer.activate();
        Arc::new(Self {
            peer,
            open: Notify::new(),
            controls: Mutex::new(Some(controls)),
            cancellation,
            pools,
        })
    }

    pub(super) fn opened(&self) {
        self.open.notify_one();
    }

    pub(super) fn receive(&self, wire: &[u8]) -> Result<bool, ()> {
        let Some(record) = wire.strip_prefix(RTC_CONTROL_PREFIX) else {
            return Ok(false);
        };
        let control = Control::decode(record).ok_or(())?;
        if let Some(task) = self
            .peer
            .receive_control(control, &self.pools.large)
            .map_err(|_| ())?
        {
            tokio::spawn(task);
        }
        Ok(true)
    }

    pub(super) async fn writer(self: Arc<Self>, channel: Arc<dyn LaneDataChannel>) {
        let Some(mut controls) = self.controls.lock().unwrap().take() else {
            return;
        };
        let write = async {
            self.open.notified().await;
            let permit = self
                .pools
                .base
                .clone()
                .acquire_owned()
                .await
                .map_err(|_| ())?;
            self.peer.open_window(permit);
            loop {
                let credit = tokio::select! {
                    biased;
                    credit = controls.recv() => credit.ok_or(())?,
                    credit = self.peer.window_update() => credit,
                };
                let record = credit.encode();
                let mut frame = BytesMut::with_capacity(RTC_CONTROL_PREFIX.len() + record.len());
                frame.extend_from_slice(RTC_CONTROL_PREFIX);
                frame.extend_from_slice(&record);
                channel.send(frame).await.map_err(|_| ())?;
            }
        };
        let result: Result<(), ()> = tokio::select! {
            biased;
            _ = self.cancellation.cancelled() => Ok(()),
            result = write => result,
        };
        self.peer.retire();
        if result.is_err() {
            let _ = channel.close().await;
        }
    }
}

pub(super) fn append_length(frame: &mut Vec<u8>, len: usize) {
    frame.put_u32(len as u32);
}

pub(super) fn credited_count(count: u16) -> u16 {
    RTC_CREDITED_FRAGMENT | count
}

pub(super) fn split_length(count: u16, frame: Bytes) -> Option<(u16, Option<usize>, Bytes)> {
    if count & RTC_CREDITED_FRAGMENT == 0 {
        return Some((count, None, frame));
    }
    let count = count & !RTC_CREDITED_FRAGMENT;
    let len = u32::from_be_bytes(frame.get(..LENGTH)?.try_into().ok()?) as usize;
    (count != 0).then(|| (count, Some(len), frame.slice(LENGTH..)))
}
