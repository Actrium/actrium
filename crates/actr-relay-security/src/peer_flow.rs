//! Asynchronous, runtime-independent peer credit and receive reservations.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use crate::flow_control::{Class, Control, Credit, Window};
use crate::{MAX_APPLICATION_BYTES, encrypted_bytes};
use tokio::sync::{Notify, OwnedMutexGuard, OwnedSemaphorePermit, Semaphore, mpsc};
use tokio_util::sync::CancellationToken;

pub const MAX_CHANNELS: usize = 256;
pub const WINDOW: Credit = Credit {
    bytes: 12 * 1024,
    messages: 16,
};
pub const INBOUND_SLOTS: usize = Class::ALL.len() * WINDOW.messages as usize + 1;
pub const SHARED_HEADROOM: usize = MAX_CHANNELS * Class::ALL.len() * WINDOW.bytes as usize;

#[derive(Debug, thiserror::Error)]
pub enum FlowError {
    #[error("peer flow control closed")]
    Closed,
    #[error("peer flow control protocol violation")]
    Protocol,
    #[error("message exceeds the flow control size limit")]
    TooLarge,
    #[error("peer flow control {0} budget is full")]
    Busy(&'static str),
}

/// A pending, cancel-safe large receive reservation. The adapter drives it on
/// its own executor, independently of the shared receive loop.
pub type ReservationTask = Pin<Box<dyn Future<Output = ()> + Send>>;

#[derive(Default)]
struct SendState {
    windows: [Window; 2],
    next_request: u64,
    waiting: Option<(u64, bool)>,
}

struct LargeReceive {
    id: u64,
    bytes: usize,
    cancel: CancellationToken,
    permits: Option<(OwnedSemaphorePermit, OwnedSemaphorePermit)>,
}

#[derive(Default)]
struct ReceiveState {
    windows: [Window; 2],
    released: [Credit; 2],
    large: Option<LargeReceive>,
    pending_large: Option<LargeReceive>,
    last_request: u64,
    base: Option<Arc<OwnedSemaphorePermit>>,
}

pub struct PeerFlow {
    enabled: std::sync::atomic::AtomicBool,
    send: Mutex<SendState>,
    recv: Mutex<ReceiveState>,
    serial: [Arc<tokio::sync::Mutex<()>>; 2],
    large_serial: Arc<tokio::sync::Mutex<()>>,
    wait_slots: Arc<Semaphore>,
    wait_bytes: Arc<Semaphore>,
    peer_wait_bytes: Arc<Semaphore>,
    large_slot: Arc<Semaphore>,
    send_changed: Notify,
    recv_changed: Notify,
    control: mpsc::Sender<Control>,
    cancel: CancellationToken,
}

impl PeerFlow {
    pub fn new(
        cancel: CancellationToken,
        wait_bytes: Arc<Semaphore>,
    ) -> (Arc<Self>, mpsc::Receiver<Control>) {
        let (control, reader) = mpsc::channel(8);
        let flow = Arc::new(Self {
            enabled: false.into(),
            send: Mutex::default(),
            recv: Mutex::default(),
            serial: Default::default(),
            large_serial: Arc::default(),
            wait_slots: Arc::new(Semaphore::new(64)),
            wait_bytes,
            peer_wait_bytes: Arc::new(Semaphore::new(MAX_APPLICATION_BYTES)),
            large_slot: Arc::new(Semaphore::new(1)),
            send_changed: Notify::new(),
            recv_changed: Notify::new(),
            control,
            cancel,
        });
        (flow, reader)
    }

    pub fn activate(&self) {
        self.enabled
            .store(true, std::sync::atomic::Ordering::Release);
    }

    pub fn open_window(&self, permit: OwnedSemaphorePermit) {
        let mut recv = self.recv.lock().unwrap();
        if !self.cancel.is_cancelled() {
            recv.base = Some(Arc::new(permit));
            recv.released = [WINDOW; 2];
            self.recv_changed.notify_one();
        }
    }

    pub fn retire(&self) {
        self.cancel.cancel();
        let mut recv = self.recv.lock().unwrap();
        recv.base.take();
        recv.large.take();
        recv.pending_large.take();
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Called only by the peer's control writer. Multiple releases coalesce.
    pub async fn window_update(&self) -> Control {
        loop {
            let notified = self.recv_changed.notified();
            {
                let mut recv = self.recv.lock().unwrap();
                if recv.released != [Credit::default(); 2] {
                    let released = std::mem::take(&mut recv.released);
                    return Control::Window(Class::ALL.map(|class| {
                        let i = class.index();
                        recv.windows[i]
                            .grant(released[i])
                            .expect("relay credit exhausted")
                    }));
                }
            }
            notified.await;
        }
    }

    pub async fn reserve(
        self: &Arc<Self>,
        class: Class,
        bytes: usize,
    ) -> Result<Option<SendCredit>, FlowError> {
        if !self.enabled() {
            return Ok(None);
        }
        if bytes > MAX_APPLICATION_BYTES {
            return Err(FlowError::TooLarge);
        }
        let _wait_slot = self
            .wait_slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| FlowError::Busy("per-peer wait count"))?;
        let charge = Credit::message(bytes);
        let _peer_bytes = self
            .peer_wait_bytes
            .clone()
            .try_acquire_many_owned(charge.bytes as u32)
            .map_err(|_| FlowError::Busy("per-peer wait bytes"))?;
        let _wait_bytes = self
            .wait_bytes
            .clone()
            .try_acquire_many_owned(charge.bytes as u32)
            .map_err(|_| FlowError::Busy("shared wait bytes"))?;
        let reserve = async {
            let serial = self.serial[class.index()].clone().lock_owned().await;
            let mut credit = SendCredit {
                flow: self.clone(),
                charge,
                class,
                large: None,
                write_in_flight: false,
                _serial: serial,
                _large_serial: None,
            };
            if WINDOW.covers(charge) {
                loop {
                    let changed = self.send_changed.notified();
                    tokio::pin!(changed);
                    changed.as_mut().enable();
                    if self.send.lock().unwrap().windows[class.index()]
                        .available()
                        .covers(charge)
                    {
                        break;
                    }
                    changed.await;
                }
            } else {
                credit._large_serial = Some(self.large_serial.clone().lock_owned().await);
                // Reserve a slot for returning credit even if this future is dropped.
                let release = self
                    .control
                    .clone()
                    .reserve_owned()
                    .await
                    .map_err(|_| FlowError::Closed)?;
                let request = self
                    .control
                    .clone()
                    .reserve_owned()
                    .await
                    .map_err(|_| FlowError::Closed)?;
                let id = {
                    let mut send = self.send.lock().unwrap();
                    send.next_request =
                        send.next_request.checked_add(1).ok_or(FlowError::Closed)?;
                    let id = send.next_request;
                    send.waiting = Some((id, false));
                    id
                };
                credit.large = Some((id, release));
                request.send(Control::Request {
                    id,
                    bytes: bytes as u32,
                });
                loop {
                    let changed = self.send_changed.notified();
                    tokio::pin!(changed);
                    changed.as_mut().enable();
                    if self.send.lock().unwrap().waiting == Some((id, true)) {
                        break;
                    }
                    changed.await;
                }
            }
            Ok(Some(credit))
        };
        tokio::select! {
            biased;
            _ = self.cancel.cancelled() => Err(FlowError::Closed),
            result = reserve => result,
        }
    }

    pub fn receive_control(
        self: &Arc<Self>,
        control: Control,
        shared: &Arc<Semaphore>,
    ) -> Result<Option<ReservationTask>, FlowError> {
        match control {
            Control::Window(granted) => {
                let mut send = self.send.lock().unwrap();
                let mut windows = send.windows;
                for class in Class::ALL {
                    let i = class.index();
                    if !windows[i].update(granted[i], WINDOW) {
                        return Err(FlowError::Protocol);
                    }
                }
                send.windows = windows;
                self.send_changed.notify_waiters();
            }
            Control::Reserved { id } => {
                let mut send = self.send.lock().unwrap();
                if id > send.next_request {
                    return Err(FlowError::Protocol);
                }
                if send.waiting == Some((id, false)) {
                    send.waiting = Some((id, true));
                    self.send_changed.notify_waiters();
                }
            }
            Control::Request { id, bytes } => {
                let bytes = bytes as usize;
                if WINDOW.covers(Credit::message(bytes)) {
                    return Err(FlowError::Protocol);
                }
                let charge = encrypted_bytes(bytes).map_err(|_| FlowError::Protocol)?;
                let cancel = self.cancel.child_token();
                {
                    let mut recv = self.recv.lock().unwrap();
                    // On separate RTC lanes the next request can arrive
                    // before data already submitted on the previous grant.
                    if recv.pending_large.is_some() || id <= recv.last_request {
                        return Err(FlowError::Protocol);
                    }
                    recv.last_request = id;
                    recv.pending_large = Some(LargeReceive {
                        id,
                        bytes,
                        cancel: cancel.clone(),
                        permits: None,
                    });
                }
                let flow = self.clone();
                let shared = shared.clone();
                return Ok(Some(Box::pin(async move {
                    let reserve = async {
                        let slot = flow
                            .large_slot
                            .clone()
                            .acquire_owned()
                            .await
                            .map_err(|_| FlowError::Closed)?;
                        let bytes = shared
                            .acquire_many_owned(charge as u32)
                            .await
                            .map_err(|_| FlowError::Closed)?;
                        let control = flow
                            .control
                            .reserve()
                            .await
                            .map_err(|_| FlowError::Closed)?;
                        let mut recv = flow.recv.lock().unwrap();
                        if recv
                            .pending_large
                            .as_ref()
                            .is_some_and(|large| large.id == id)
                        {
                            let mut pending =
                                recv.pending_large.take().expect("matched reservation");
                            pending.permits = Some((slot, bytes));
                            debug_assert!(recv.large.is_none());
                            recv.large = Some(pending);
                            control.send(Control::Reserved { id });
                        }
                        Ok::<_, FlowError>(())
                    };
                    tokio::select! {
                        _ = cancel.cancelled() => {}
                        _ = reserve => {}
                    }
                })));
            }
            Control::Release { id } => {
                let mut recv = self.recv.lock().unwrap();
                let large = if recv
                    .pending_large
                    .as_ref()
                    .is_some_and(|large| large.id == id)
                {
                    recv.pending_large.take()
                } else if recv.large.as_ref().is_some_and(|large| large.id == id) {
                    recv.large.take()
                } else {
                    return Err(FlowError::Protocol);
                }
                .expect("matched large receive");
                large.cancel.cancel();
            }
        }
        Ok(None)
    }

    pub fn admit(self: &Arc<Self>, class: Class, bytes: usize) -> Option<ReceiveCredit> {
        let charge = Credit::message(bytes);
        let mut recv = self.recv.lock().unwrap();
        if WINDOW.covers(charge) {
            let base = recv.base.clone()?;
            return recv.windows[class.index()]
                .consume(charge)
                .then(|| ReceiveCredit::Window {
                    flow: self.clone(),
                    charge,
                    class,
                    _base: base,
                });
        }
        let large = recv.large.as_ref()?;
        if large.bytes != bytes || large.permits.is_none() {
            return None;
        }
        let mut large = recv.large.take()?;
        Some(ReceiveCredit::Large {
            _permits: large.permits.take()?,
        })
    }
}

pub enum ReceiveCredit {
    Window {
        flow: Arc<PeerFlow>,
        charge: Credit,
        class: Class,
        _base: Arc<OwnedSemaphorePermit>,
    },
    Large {
        _permits: (OwnedSemaphorePermit, OwnedSemaphorePermit),
    },
}

impl Drop for ReceiveCredit {
    fn drop(&mut self) {
        if let Self::Window {
            flow,
            charge,
            class,
            ..
        } = self
        {
            let mut recv = flow.recv.lock().unwrap();
            let released = &mut recv.released[class.index()];
            *released = released
                .checked_add(*charge)
                .expect("bounded relay release");
            flow.recv_changed.notify_one();
        }
    }
}

pub struct SendCredit {
    flow: Arc<PeerFlow>,
    charge: Credit,
    class: Class,
    large: Option<(u64, mpsc::OwnedPermit<Control>)>,
    write_in_flight: bool,
    _serial: OwnedMutexGuard<()>,
    _large_serial: Option<OwnedMutexGuard<()>>,
}

impl SendCredit {
    pub fn commit(&mut self) {
        self.write_in_flight = true;
        let mut send = self.flow.send.lock().unwrap();
        if self.large.take().is_some() {
            send.waiting = None;
        } else {
            assert!(send.windows[self.class.index()].consume(self.charge));
        }
    }

    pub fn submitted(&mut self) {
        self.write_in_flight = false;
    }
}

impl Drop for SendCredit {
    fn drop(&mut self) {
        if let Some((id, release)) = self.large.take() {
            self.flow.send.lock().unwrap().waiting = None;
            release.send(Control::Release { id });
        }
        // A cancelled partial write cannot safely return spent credit or
        // skip already assigned Noise nonces. Retire just this channel.
        if self.write_in_flight {
            self.flow.cancel.cancel();
        }
    }
}
