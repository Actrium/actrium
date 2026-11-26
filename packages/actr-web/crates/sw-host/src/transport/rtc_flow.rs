//! Peer credit owned by the Worker and carried by a generation's DOM port.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use actr_relay_security::flow_control::{
    Class, Control, RTC_CONTROL_PREFIX, RTC_CREDITED_FRAGMENT,
};
use actr_relay_security::peer_flow::{INBOUND_SLOTS, PeerFlow, ReceiveCredit};
use bytes::BufMut;
use tokio::sync::{Semaphore, mpsc};
use tokio_util::sync::CancellationToken;
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use web_sys::{MessageEvent, MessagePort};

const CONTROL_TAG: u8 = 255;
pub(crate) const CREDITED_TAG: u8 = 128;

struct Assembly {
    id: u32,
    bytes: usize,
    count: u16,
    next: u16,
    data: Vec<u8>,
    credit: ReceiveCredit,
}

struct Delivery {
    data: Vec<u8>,
    _credit: ReceiveCredit,
}

pub struct RtcFlow {
    pub(crate) peer: Arc<PeerFlow>,
    port: MessagePort,
    cancel: CancellationToken,
    assembly: RefCell<[Option<Assembly>; 2]>,
    dispatch: [mpsc::Sender<Delivery>; 2],
    on_message: RefCell<Option<Closure<dyn FnMut(MessageEvent)>>>,
}

impl RtcFlow {
    pub(crate) fn attach(
        client: String,
        peer_name: String,
        port: MessagePort,
        (windows, large, waiting): (Arc<Semaphore>, Arc<Semaphore>, Arc<Semaphore>),
    ) -> Rc<Self> {
        let cancel = CancellationToken::new();
        let (peer, mut controls) = PeerFlow::new(cancel.clone(), waiting);
        peer.activate();
        let dispatch = Class::ALL.map(|_| {
            let (tx, mut rx) = mpsc::channel::<Delivery>(INBOUND_SLOTS);
            let client = client.clone();
            let peer = peer_name.clone();
            let cancel = cancel.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let delivery = async {
                    while let Some(Delivery { data, _credit }) = rx.recv().await {
                        crate::runtime::dispatch_application(&client, &peer, data).await?;
                    }
                    Ok::<_, JsValue>(())
                };
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => {}
                    result = delivery => if result.is_err() { cancel.cancel(); }
                }
            });
            tx
        });
        let flow = Rc::new(Self {
            peer,
            port,
            cancel,
            assembly: RefCell::default(),
            dispatch,
            on_message: RefCell::new(None),
        });
        let weak = Rc::downgrade(&flow);
        let handler: Closure<dyn FnMut(MessageEvent)> = Closure::new(move |event: MessageEvent| {
            if let Some(flow) = weak.upgrade() {
                if flow.receive(event.data(), &large).is_err() {
                    flow.close();
                }
            }
        });
        flow.port
            .set_onmessage(Some(handler.as_ref().unchecked_ref()));
        flow.port.start();
        *flow.on_message.borrow_mut() = Some(handler);
        let writer = flow.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let run = async {
                writer
                    .peer
                    .open_window(windows.acquire_owned().await.map_err(|_| ())?);
                loop {
                    let control = tokio::select! {
                        biased;
                        control = controls.recv() => control.ok_or(())?,
                        update = writer.peer.window_update() => update,
                    };
                    let control = control.encode();
                    let mut frame = Vec::with_capacity(13 + control.len());
                    frame.push(CONTROL_TAG);
                    frame.put_u32((RTC_CONTROL_PREFIX.len() + control.len()) as u32);
                    frame.extend_from_slice(RTC_CONTROL_PREFIX);
                    frame.extend(control);
                    writer
                        .port
                        .post_message(&js_sys::Uint8Array::from(frame.as_slice()).into())
                        .map_err(|_| ())?;
                }
            };
            let _: Result<(), ()> = tokio::select! {
                biased;
                _ = writer.cancel.cancelled() => Ok(()),
                result = run => result,
            };
            writer.close();
        });
        flow
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.cancel.is_cancelled()
    }

    pub(crate) fn close(&self) {
        self.peer.retire();
        self.port.set_onmessage(None);
        self.port.close();
    }

    fn receive(&self, event: JsValue, large: &Arc<Semaphore>) -> Result<(), ()> {
        let field = |name: &str| js_sys::Reflect::get(&event, &name.into()).map_err(|_| ());
        let lane = field("lane")?
            .as_f64()
            .filter(|lane| matches!(*lane, 0.0 | 2.0))
            .ok_or(())? as u8;
        let buffer = field("flowFrame")?
            .dyn_into::<js_sys::ArrayBuffer>()
            .map_err(|_| ())?;
        let wire = js_sys::Uint8Array::new(&buffer);
        if wire.length() > 65_535 {
            return Err(());
        }
        let frame = wire.to_vec();
        if let Some(record) = frame.strip_prefix(RTC_CONTROL_PREFIX) {
            if lane != 0 {
                return Err(());
            }
            let control = Control::decode(record).ok_or(())?;
            if let Some(task) = self.peer.receive_control(control, large).map_err(|_| ())? {
                wasm_bindgen_futures::spawn_local(task);
            }
            return Ok(());
        }

        let id = u32::from_be_bytes(frame.get(..4).ok_or(())?.try_into().map_err(|_| ())?);
        let index = u16::from_be_bytes(frame.get(4..6).ok_or(())?.try_into().map_err(|_| ())?);
        let count = u16::from_be_bytes(frame.get(6..8).ok_or(())?.try_into().map_err(|_| ())?);
        if count & RTC_CREDITED_FRAGMENT == 0 {
            return Err(());
        }
        let count = count & !RTC_CREDITED_FRAGMENT;
        if index >= count {
            return Err(());
        }
        let bytes =
            u32::from_be_bytes(frame.get(8..12).ok_or(())?.try_into().map_err(|_| ())?) as usize;
        let fragment = frame.get(12..).ok_or(())?;
        if bytes > actr_relay_security::MAX_APPLICATION_BYTES {
            return Err(());
        }
        let class = Class::from_payload_tag(lane).ok_or(())?;
        let mut entries = self.assembly.borrow_mut();
        let entry = &mut entries[class.index()];
        if index == 0 {
            if entry.is_some() {
                return Err(());
            }
            let credit = self.peer.admit(class, bytes).ok_or(())?;
            let mut data = Vec::with_capacity(5 + bytes);
            data.push(lane);
            data.put_u32(bytes as u32);
            *entry = Some(Assembly {
                id,
                bytes,
                count,
                next: 0,
                data,
                credit,
            });
        }
        let pending = entry.as_mut().ok_or(())?;
        if pending.id != id
            || pending.bytes != bytes
            || pending.count != count
            || pending.next != index
            || pending.data.len() - 5 + fragment.len() > bytes
        {
            return Err(());
        }
        pending.data.extend_from_slice(fragment);
        pending.next += 1;
        if pending.next == count {
            let complete = entry.take().ok_or(())?;
            if complete.data.len() != 5 + bytes {
                return Err(());
            }
            self.dispatch[class.index()]
                .try_send(Delivery {
                    data: complete.data,
                    _credit: complete.credit,
                })
                .map_err(|_| ())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actr_relay_security::flow_control::Credit;
    use actr_relay_security::peer_flow::WINDOW;
    use bytes::Bytes;
    use wasm_bindgen_test::wasm_bindgen_test;

    fn grant(port: &MessagePort, windows: [Credit; 2]) {
        let mut frame = RTC_CONTROL_PREFIX.to_vec();
        frame.extend(Control::Window(windows).encode());
        let event = js_sys::Object::new();
        let buffer = js_sys::Uint8Array::from(frame.as_slice()).buffer();
        js_sys::Reflect::set(&event, &"flowFrame".into(), &buffer).unwrap();
        js_sys::Reflect::set(&event, &"lane".into(), &0.into()).unwrap();
        port.post_message(&event).unwrap();
    }

    #[wasm_bindgen_test]
    async fn rtc_port_pauses_and_restores_stream_credit() {
        let ports = web_sys::MessageChannel::new().unwrap();
        let flow = RtcFlow::attach(
            "unused".into(),
            "peer".into(),
            ports.port1(),
            (
                Arc::new(Semaphore::new(1)),
                Arc::new(Semaphore::new(65_536)),
                Arc::new(Semaphore::new(65_536)),
            ),
        );
        let (_tx, rx) = futures::channel::mpsc::unbounded();
        let lane = crate::transport::DataLane::PostMessage {
            port: Arc::new(ports.port1()),
            payload_type: actr_web_common::PayloadType::StreamReliable,
            rx: Arc::new(parking_lot::Mutex::new(rx)),
            failure_notifier: None,
            flow: Some(flow.clone()),
        };
        grant(&ports.port2(), [WINDOW; 2]);
        for _ in 0..WINDOW.messages {
            lane.send(Bytes::from_static(b"credited")).await.unwrap();
        }
        let pending = lane.send(Bytes::from_static(b"blocked"));
        futures::pin_mut!(pending);
        assert!(futures::poll!(pending.as_mut()).is_pending());
        grant(
            &ports.port2(),
            [WINDOW, WINDOW.checked_add(Credit::message(7)).unwrap()],
        );
        pending.await.unwrap();
        flow.close();
    }
}
