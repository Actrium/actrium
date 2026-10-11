//! Transport manager for a single destination.
//!
//! Manages all connections and message routing for one `Dest` using an
//! event-driven model with no polling.

use super::handover::HandoverBook;
use super::wire_handle::WireHandle;
use super::wire_pool::WirePool;
use actr_protocol::{prost::Message, transport_control};
use actr_web_common::{ConnType, Dest, PayloadType, WebError, WebResult};
use bytes::Bytes;
use futures::lock::Mutex;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

const RTC_STABLE: u8 = 0;
const RTC_RECOVERING: u8 = 1;
const RTC_RESUMED: u8 = 2;

#[derive(Default)]
struct ReliableRoute {
    current: Option<WireHandle>,
    retry_after: f64,
}

/// Transport manager for a single destination.
///
/// Responsibilities:
/// - Manage all connections to one `Dest` (`WebSocket` + `WebRTC`)
/// - Start connection attempts concurrently in the background
/// - Wait for connection readiness in an event-driven way
/// - Reuse cached lanes stored inside `WireHandle`
/// - Delegate connection priority selection to `WirePool`
pub struct DestTransport {
    /// Destination.
    dest: Dest,

    /// Connection manager.
    wire_pool: Arc<WirePool>,
    handover: Arc<HandoverBook>,
    reliable: Mutex<ReliableRoute>,
    rtc_recovery: AtomicU8,
}

impl DestTransport {
    /// Create a new `DestTransport`.
    ///
    /// # Parameters
    /// - `dest`: destination
    /// - `connections`: pre-created connection list (`WebSocket`/`WebRTC`), possibly empty
    pub async fn new(dest: Dest, connections: Vec<WireHandle>) -> WebResult<Self> {
        let wire_pool = Arc::new(WirePool::new());

        // Start all connection tasks concurrently in the background.
        log::info!("[{:?}] Starting connection tasks...", dest);
        for conn in connections {
            wire_pool.add_connection(conn);
        }

        Ok(Self {
            dest,
            wire_pool,
            handover: Arc::new(HandoverBook::default()),
            reliable: Mutex::new(ReliableRoute::default()),
            rtc_recovery: AtomicU8::new(RTC_STABLE),
        })
    }

    pub(crate) fn set_handover(&mut self, handover: Arc<HandoverBook>) {
        self.handover = handover;
    }

    async fn reliable_wire(
        &self,
        preferred: WireHandle,
        route: &mut ReliableRoute,
    ) -> WebResult<WireHandle> {
        let Some(mut previous) = route.current.clone() else {
            return Ok(preferred);
        };
        if previous.conn_type() == ConnType::WebRTC {
            let recovery = self.rtc_recovery.load(Ordering::Acquire);
            if recovery != RTC_STABLE {
                let recovered = if recovery == RTC_RESUMED {
                    self.wire_pool.get_connection(ConnType::WebRTC).await
                } else {
                    None
                };
                if let Some(wire) = recovered.filter(WireHandle::is_connected) {
                    previous = wire;
                    route.current = Some(previous.clone());
                    self.rtc_recovery.store(RTC_STABLE, Ordering::Release);
                } else {
                    return Err(WebError::Transport(
                        "Previous reliable RTC path is recovering".into(),
                    ));
                }
            }
        }
        if previous.same_connection(&preferred) {
            return Ok(preferred);
        }
        let still_current = self
            .wire_pool
            .get_connection(previous.conn_type())
            .await
            .is_some_and(|wire| previous.same_connection(&wire));
        let previous_lane = if still_current && previous.is_connected() {
            previous.get_lane(PayloadType::StreamReliable).await.ok()
        } else {
            None
        };
        let Some(lane) = previous_lane else {
            // A failed physical route retains delivery uncertainty; application
            // chunks are never automatically replayed on a replacement route.
            route.retry_after = 0.0;
            return Ok(preferred);
        };
        if route.retry_after > js_sys::Date::now() {
            return Ok(previous);
        }
        let Dest::Peer(peer) = &self.dest else {
            return Ok(preferred);
        };
        let Some(mut pending) = self.handover.begin(peer) else {
            return Ok(previous);
        };
        let barrier = transport_control::barrier(&pending.nonce).encode_to_vec();
        let acknowledgement = async {
            if lane.send(Bytes::from(barrier)).await.is_err() {
                return false;
            }
            (&mut pending.receiver).await.is_ok()
        };
        let acknowledged = matches!(
            futures::future::select(
                Box::pin(acknowledgement),
                Box::pin(gloo_timers::future::TimeoutFuture::new(3000)),
            )
            .await,
            futures::future::Either::Left((true, _))
        );
        let old_current = self
            .wire_pool
            .get_connection(previous.conn_type())
            .await
            .is_some_and(|wire| previous.same_connection(&wire) && wire.is_connected());
        let new_current = self
            .wire_pool
            .get_connection(preferred.conn_type())
            .await
            .is_some_and(|wire| preferred.same_connection(&wire) && wire.is_connected());
        if !old_current
            || !new_current
            || (previous.conn_type() == ConnType::WebRTC
                && self.rtc_recovery.load(Ordering::Acquire) != RTC_STABLE)
        {
            return Err(WebError::Transport(
                "Reliable route changed during handover".into(),
            ));
        }
        if acknowledged {
            route.retry_after = 0.0;
            Ok(preferred)
        } else {
            // Legacy receivers cannot acknowledge the control. Keep their
            // healthy ordered lane and avoid delaying every subsequent chunk.
            route.retry_after = js_sys::Date::now() + 30_000.0;
            Ok(previous)
        }
    }

    pub(crate) async fn rtc_disconnected(&self) -> WebResult<()> {
        self.rtc_recovery.store(RTC_RECOVERING, Ordering::Release);
        if let Some(connection) = self.wire_pool.get_connection(ConnType::WebRTC).await {
            connection.close().await?;
        }
        self.wire_pool.mark_connection_failed(ConnType::WebRTC);
        Ok(())
    }

    pub(crate) fn rtc_connected(&self) {
        let _ = self.rtc_recovery.compare_exchange(
            RTC_RECOVERING,
            RTC_RESUMED,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    pub(crate) async fn rtc_terminated(&self) -> WebResult<()> {
        self.rtc_recovery.store(RTC_STABLE, Ordering::Release);
        if let Some(connection) = self.wire_pool.get_connection(ConnType::WebRTC).await {
            connection.close().await?;
        }
        self.wire_pool.mark_connection_failed(ConnType::WebRTC);
        Ok(())
    }

    /// Get the WirePool reference.
    ///
    /// This allows external code to inject a new `WireHandle` after a
    /// connection becomes available, for example when the DOM passes in a
    /// `MessagePort`.
    pub fn wire_pool(&self) -> &Arc<WirePool> {
        &self.wire_pool
    }

    /// Send a message.
    ///
    /// Event-driven behavior:
    /// - If a connection is available, send immediately
    /// - Otherwise wait for connection state changes through the watcher channel
    /// - `WirePool` already handles priority, so this method only tries lane types in order
    pub async fn send(&self, payload_type: PayloadType, data: &[u8]) -> WebResult<()> {
        let mut reliable = if payload_type == PayloadType::StreamReliable {
            Some(self.reliable.lock().await)
        } else {
            None
        };
        log::debug!(
            "[{:?}] Sending message: type={:?}, size={}",
            self.dest,
            payload_type,
            data.len()
        );

        // 1. Determine candidate connection types (simplified: WebRTC first).
        let mut conn_types = self.get_conn_types_for(payload_type);

        if conn_types.is_empty() {
            return Err(WebError::Transport(format!(
                "No route for: {:?}",
                payload_type
            )));
        }

        // 2. Subscribe to connection state changes.
        let mut watcher = self.wire_pool.subscribe_changes();

        loop {
            if payload_type != PayloadType::MediaRtp {
                let rtc = self.wire_pool.get_connection(ConnType::WebRTC).await;
                let relay = self.wire_pool.get_connection(ConnType::WebSocket).await;
                if relay.as_ref().is_some_and(|relay| {
                    rtc.as_ref()
                        .is_none_or(|rtc| relay.priority() > rtc.priority())
                }) {
                    conn_types = vec![ConnType::WebSocket, ConnType::WebRTC];
                } else {
                    conn_types = vec![ConnType::WebRTC, ConnType::WebSocket];
                }
            }
            // 3. Check the current ready connections snapshot.
            let ready_set = watcher.borrow_and_update();

            log::trace!("[{:?}] Available connections: {:?}", self.dest, ready_set);

            // 4. Try each connection type in priority order.
            for &conn_type in &conn_types {
                // Skip connections that are not ready yet.
                if !ready_set.contains(&conn_type) {
                    log::trace!("[{:?}] {:?} not ready, trying next", self.dest, conn_type);
                    continue;
                }

                // Fetch the connection and build or reuse its lane.
                if let Some(conn) = self.wire_pool.get_connection(conn_type).await {
                    let conn = if let Some(route) = reliable.as_deref_mut() {
                        self.reliable_wire(conn, route).await?
                    } else {
                        conn
                    };
                    match conn.get_lane(payload_type).await {
                        Ok(lane) => {
                            log::debug!(
                                "[{:?}] Using connection: {:?} (type={:?})",
                                self.dest,
                                conn_type,
                                payload_type
                            );

                            if let Some(route) = reliable.as_deref_mut() {
                                route.current = Some(conn.clone());
                            }
                            // Convert to `Bytes`.
                            return lane.send(Bytes::copy_from_slice(data)).await;
                        }
                        Err(e) => {
                            log::warn!(
                                "[{:?}] Failed to get DataLane: {:?}: {}",
                                self.dest,
                                conn_type,
                                e
                            );
                            continue;
                        }
                    }
                }
            }

            // 5. All attempts failed, so wait for a connection state change.
            log::info!("[{:?}] Waiting for connection status...", self.dest);

            // Event-driven wait.
            if watcher.changed().await.is_err() {
                return Err(WebError::Transport("connection manager closed".to_string()));
            }

            log::debug!("[{:?}] Connection status updated, retrying...", self.dest);
        }
    }

    /// Close the transport and release all connection resources.
    pub async fn close(&self) -> WebResult<()> {
        log::info!("[{:?}] Closing DestTransport", self.dest);

        // Close all connections.
        for conn_type in [ConnType::WebSocket, ConnType::WebRTC] {
            if let Some(conn) = self.wire_pool.get_connection(conn_type).await {
                if let Err(e) = conn.close().await {
                    log::warn!(
                        "[{:?}] Failed to close {:?} connection: {}",
                        self.dest,
                        conn_type,
                        e
                    );
                } else {
                    log::debug!("[{:?}] Closed {:?} connection", self.dest, conn_type);
                }
            }
        }

        Ok(())
    }

    /// Check whether there is at least one healthy connection.
    ///
    /// Used by health checks to detect complete transport failure.
    ///
    /// # Returns
    /// - `true` if at least one connection is healthy
    /// - `false` if all connections are unhealthy or missing
    pub async fn has_healthy_connection(&self) -> bool {
        for conn_type in [ConnType::WebRTC, ConnType::WebSocket] {
            if let Some(conn) = self.wire_pool.get_connection(conn_type).await {
                if conn.is_connected() {
                    return true;
                }
            }
        }
        false
    }

    /// Get connection types for a payload in priority order.
    ///
    /// Simplified rules:
    /// - `MEDIA_RTP`: WebRTC only
    /// - Others: WebRTC first, WebSocket as fallback
    fn get_conn_types_for(&self, payload_type: PayloadType) -> Vec<ConnType> {
        match payload_type {
            PayloadType::MediaRtp => vec![ConnType::WebRTC],
            _ => vec![ConnType::WebRTC, ConnType::WebSocket],
        }
    }

    /// Get the destination.
    pub fn dest(&self) -> &Dest {
        &self.dest
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{WebRtcConnection, WebSocketConnection};
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    async fn reliable_handover_requires_ack_and_does_not_hold_rpc() {
        use std::cell::RefCell;
        use std::rc::Rc;
        use wasm_bindgen::{JsCast, closure::Closure};

        let relay_frames = Rc::new(RefCell::new(Vec::<Vec<u8>>::new()));
        let recorded = relay_frames.clone();
        let relay_send = Closure::wrap(Box::new(move |frame: js_sys::Uint8Array| {
            recorded.borrow_mut().push(frame.to_vec());
        }) as Box<dyn FnMut(js_sys::Uint8Array)>);
        let relay = WireHandle::Relay(crate::transport::RelayConnection::new(
            relay_send
                .as_ref()
                .unchecked_ref::<js_sys::Function>()
                .clone(),
        ));

        let channel = web_sys::MessageChannel::new().unwrap();
        let rtc_frames = Rc::new(RefCell::new(Vec::<Vec<u8>>::new()));
        let recorded = rtc_frames.clone();
        let on_message = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
            recorded
                .borrow_mut()
                .push(js_sys::Uint8Array::new(&event.data()).to_vec());
        }) as Box<dyn FnMut(web_sys::MessageEvent)>);
        channel
            .port2()
            .set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        channel.port2().start();
        let mut rtc = WebRtcConnection::new("peer".into());
        rtc.set_datachannel_port(channel.port1());
        let rtc_path = rtc.clone();
        let transport = Arc::new(
            DestTransport::new(
                Dest::Peer("peer".into()),
                vec![relay, WireHandle::WebRTC(rtc)],
            )
            .await
            .unwrap(),
        );
        let mut ready = transport.wire_pool().subscribe_changes();
        while ready.borrow_and_update().len() != 2 {
            ready.changed().await.unwrap();
        }
        transport
            .send(PayloadType::StreamReliable, b"before")
            .await
            .unwrap();
        assert_eq!(relay_frames.borrow().len(), 1);
        rtc_path.set_network_path(Some(false));

        let (sent, mut received) = futures::channel::oneshot::channel();
        let sending = transport.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let _ = sent.send(sending.send(PayloadType::StreamReliable, b"after").await);
        });
        for _ in 0..100 {
            if relay_frames.borrow().len() == 2 {
                break;
            }
            gloo_timers::future::TimeoutFuture::new(5).await;
        }
        assert_eq!(relay_frames.borrow().len(), 2);
        let barrier = actr_protocol::DataChunk::decode(&relay_frames.borrow()[1][5..]).unwrap();
        let nonce = transport_control::barrier_nonce(&barrier)
            .unwrap()
            .to_owned();
        assert!(rtc_frames.borrow().is_empty());
        transport
            .send(PayloadType::RpcReliable, b"rpc")
            .await
            .unwrap();
        transport.handover.acknowledge("other peer", &nonce);
        assert!(received.try_recv().unwrap().is_none());
        transport.handover.acknowledge("peer", &nonce);
        received.await.unwrap().unwrap();
        for _ in 0..100 {
            if rtc_frames.borrow().len() == 2 {
                break;
            }
            gloo_timers::future::TimeoutFuture::new(5).await;
        }
        {
            let frames = rtc_frames.borrow();
            assert_eq!(frames.len(), 2);
            assert_eq!(&frames[0][5..], b"rpc");
            assert_eq!(&frames[1][5..], b"after");
        }
        transport.close().await.unwrap();
        channel.port2().close();
        drop(on_message);
        drop(relay_send);
    }

    #[wasm_bindgen_test]
    async fn reliable_rtc_disconnect_must_not_bypass_the_handover_barrier() {
        use std::cell::RefCell;
        use std::rc::Rc;
        use wasm_bindgen::{JsCast, closure::Closure};

        let relay_frames = Rc::new(RefCell::new(Vec::<Vec<u8>>::new()));
        let recorded = relay_frames.clone();
        let relay_send = Closure::wrap(Box::new(move |frame: js_sys::Uint8Array| {
            recorded.borrow_mut().push(frame.to_vec());
        }) as Box<dyn FnMut(js_sys::Uint8Array)>);
        let relay = WireHandle::Relay(crate::transport::RelayConnection::new(
            relay_send
                .as_ref()
                .unchecked_ref::<js_sys::Function>()
                .clone(),
        ));

        // Hold the first message in the simulated DOM bridge: closing the SW
        // handle does not close the physical RTC peer or its buffered data.
        let channel = web_sys::MessageChannel::new().unwrap();
        let rtc_frames = Rc::new(RefCell::new(Vec::<Vec<u8>>::new()));
        let recorded = rtc_frames.clone();
        let on_message = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
            recorded
                .borrow_mut()
                .push(js_sys::Uint8Array::new(&event.data()).to_vec());
        }) as Box<dyn FnMut(web_sys::MessageEvent)>);
        channel
            .port2()
            .set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        channel.port2().start();
        let mut rtc = WebRtcConnection::new("peer".into());
        rtc.set_datachannel_port(channel.port1());
        rtc.set_network_path(Some(false));
        let transport = Arc::new(
            DestTransport::new(
                Dest::Peer("peer".into()),
                vec![relay, WireHandle::WebRTC(rtc.clone())],
            )
            .await
            .unwrap(),
        );
        let mut ready = transport.wire_pool().subscribe_changes();
        while ready.borrow_and_update().len() != 2 {
            ready.changed().await.unwrap();
        }
        transport
            .send(PayloadType::StreamReliable, b"before")
            .await
            .unwrap();
        for _ in 0..100 {
            if !rtc_frames.borrow().is_empty() {
                break;
            }
            gloo_timers::future::TimeoutFuture::new(5).await;
        }
        assert_eq!(rtc_frames.borrow().len(), 1);
        assert!(relay_frames.borrow().is_empty());

        // This is the SW action taken for a recoverable DOM disconnected
        // event; it leaves the physical connection and its old queue alive.
        transport.rtc_disconnected().await.unwrap();
        let sending = transport.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let _ = sending.send(PayloadType::StreamReliable, b"after").await;
        });
        gloo_timers::future::TimeoutFuture::new(50).await;
        assert!(
            relay_frames.borrow().is_empty(),
            "new reliable data reached relay while the old RTC path could still deliver"
        );
        transport.rtc_terminated().await.unwrap();
        transport
            .send(PayloadType::StreamReliable, b"after termination")
            .await
            .unwrap();
        assert_eq!(relay_frames.borrow().len(), 1);
        transport.close().await.unwrap();
        channel.port2().close();
        drop(on_message);
        drop(relay_send);
    }

    #[wasm_bindgen_test]
    async fn recovered_rtc_must_acknowledge_before_switching_to_relay() {
        use std::cell::RefCell;
        use std::rc::Rc;
        use wasm_bindgen::{JsCast, closure::Closure};

        let relay_frames = Rc::new(RefCell::new(Vec::<Vec<u8>>::new()));
        let recorded = relay_frames.clone();
        let relay_send = Closure::wrap(Box::new(move |frame: js_sys::Uint8Array| {
            recorded.borrow_mut().push(frame.to_vec());
        }) as Box<dyn FnMut(js_sys::Uint8Array)>);
        let relay = WireHandle::Relay(crate::transport::RelayConnection::new(
            relay_send
                .as_ref()
                .unchecked_ref::<js_sys::Function>()
                .clone(),
        ));
        let old_channel = web_sys::MessageChannel::new().unwrap();
        let mut rtc = WebRtcConnection::new("peer".into());
        rtc.set_datachannel_port(old_channel.port1());
        rtc.set_network_path(Some(false));
        let transport = Arc::new(
            DestTransport::new(
                Dest::Peer("peer".into()),
                vec![relay, WireHandle::WebRTC(rtc)],
            )
            .await
            .unwrap(),
        );
        let mut ready = transport.wire_pool().subscribe_changes();
        while ready.borrow_and_update().len() != 2 {
            ready.changed().await.unwrap();
        }
        transport
            .send(PayloadType::StreamReliable, b"before")
            .await
            .unwrap();
        transport.rtc_disconnected().await.unwrap();
        assert!(
            transport
                .send(PayloadType::StreamReliable, b"blocked")
                .await
                .is_err()
        );
        transport.rtc_connected();
        assert!(
            transport
                .send(PayloadType::StreamReliable, b"not rebound")
                .await
                .is_err()
        );

        let channel = web_sys::MessageChannel::new().unwrap();
        let rtc_frames = Rc::new(RefCell::new(Vec::<Vec<u8>>::new()));
        let recorded = rtc_frames.clone();
        let on_message = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
            recorded
                .borrow_mut()
                .push(js_sys::Uint8Array::new(&event.data()).to_vec());
        }) as Box<dyn FnMut(web_sys::MessageEvent)>);
        channel
            .port2()
            .set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        channel.port2().start();
        let mut recovered = WebRtcConnection::new("peer".into());
        recovered.set_datachannel_port(channel.port1());
        // Relay is preferred over a TURN-mediated RTC path, but the recovered
        // RTC lane must first acknowledge all earlier reliable messages.
        recovered.set_network_path(Some(true));
        transport
            .wire_pool()
            .reconnect(WireHandle::WebRTC(recovered));
        while !ready.borrow_and_update().contains(&ConnType::WebRTC) {
            ready.changed().await.unwrap();
        }
        let (sent, received) = futures::channel::oneshot::channel();
        let sending = transport.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let _ = sent.send(sending.send(PayloadType::StreamReliable, b"after").await);
        });
        for _ in 0..100 {
            if !rtc_frames.borrow().is_empty() {
                break;
            }
            gloo_timers::future::TimeoutFuture::new(5).await;
        }
        assert!(relay_frames.borrow().is_empty());
        let barrier = actr_protocol::DataChunk::decode(&rtc_frames.borrow()[0][5..]).unwrap();
        let nonce = transport_control::barrier_nonce(&barrier).unwrap();
        transport.handover.acknowledge("peer", nonce);
        received.await.unwrap().unwrap();
        assert_eq!(relay_frames.borrow().len(), 1);
        transport.close().await.unwrap();
        channel.port2().close();
        old_channel.port2().close();
        drop(on_message);
        drop(relay_send);
    }

    #[wasm_bindgen_test]
    async fn rtc_disconnect_during_barrier_does_not_submit_the_new_chunk() {
        use std::cell::RefCell;
        use std::rc::Rc;
        use wasm_bindgen::{JsCast, closure::Closure};

        let relay_frames = Rc::new(RefCell::new(Vec::<Vec<u8>>::new()));
        let recorded = relay_frames.clone();
        let relay_send = Closure::wrap(Box::new(move |frame: js_sys::Uint8Array| {
            recorded.borrow_mut().push(frame.to_vec());
        }) as Box<dyn FnMut(js_sys::Uint8Array)>);
        let relay = WireHandle::Relay(crate::transport::RelayConnection::new(
            relay_send
                .as_ref()
                .unchecked_ref::<js_sys::Function>()
                .clone(),
        ));
        let channel = web_sys::MessageChannel::new().unwrap();
        let rtc_frames = Rc::new(RefCell::new(Vec::<Vec<u8>>::new()));
        let recorded = rtc_frames.clone();
        let on_message = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
            recorded
                .borrow_mut()
                .push(js_sys::Uint8Array::new(&event.data()).to_vec());
        }) as Box<dyn FnMut(web_sys::MessageEvent)>);
        channel
            .port2()
            .set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        channel.port2().start();
        let mut rtc = WebRtcConnection::new("peer".into());
        rtc.set_datachannel_port(channel.port1());
        rtc.set_network_path(Some(false));
        let transport = Arc::new(
            DestTransport::new(
                Dest::Peer("peer".into()),
                vec![relay, WireHandle::WebRTC(rtc.clone())],
            )
            .await
            .unwrap(),
        );
        let mut ready = transport.wire_pool().subscribe_changes();
        while ready.borrow_and_update().len() != 2 {
            ready.changed().await.unwrap();
        }
        transport
            .send(PayloadType::StreamReliable, b"before")
            .await
            .unwrap();
        rtc.set_network_path(Some(true));
        let (sent, received) = futures::channel::oneshot::channel();
        let sending = transport.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let _ = sent.send(sending.send(PayloadType::StreamReliable, b"after").await);
        });
        for _ in 0..100 {
            if rtc_frames.borrow().len() >= 2 {
                break;
            }
            gloo_timers::future::TimeoutFuture::new(5).await;
        }
        assert_eq!(rtc_frames.borrow().len(), 2, "old payload and barrier");
        transport.rtc_disconnected().await.unwrap();
        assert!(received.await.unwrap().is_err());
        assert_eq!(
            rtc_frames.borrow().len(),
            2,
            "new payload reached the stale RTC port"
        );
        assert!(
            relay_frames.borrow().is_empty(),
            "new payload bypassed the barrier"
        );
        transport.rtc_terminated().await.unwrap();
        transport
            .send(PayloadType::StreamReliable, b"after termination")
            .await
            .unwrap();
        assert_eq!(relay_frames.borrow().len(), 1);
        transport.close().await.unwrap();
        channel.port2().close();
        drop(on_message);
        drop(relay_send);
    }

    #[test]
    fn test_get_conn_types_for_media_rtp() {
        let dt = create_test_transport();
        let conn_types = dt.get_conn_types_for(PayloadType::MediaRtp);

        assert_eq!(conn_types.len(), 1);
        assert_eq!(conn_types[0], ConnType::WebRTC);
    }

    #[test]
    fn test_get_conn_types_for_rpc_reliable() {
        let dt = create_test_transport();
        let conn_types = dt.get_conn_types_for(PayloadType::RpcReliable);

        assert_eq!(conn_types.len(), 2);
        assert_eq!(conn_types[0], ConnType::WebRTC);
        assert_eq!(conn_types[1], ConnType::WebSocket);
    }

    #[test]
    fn test_get_conn_types_for_rpc_signal() {
        let dt = create_test_transport();
        let conn_types = dt.get_conn_types_for(PayloadType::RpcSignal);

        assert_eq!(conn_types.len(), 2);
        assert_eq!(conn_types[0], ConnType::WebRTC);
        assert_eq!(conn_types[1], ConnType::WebSocket);
    }

    #[test]
    fn test_get_conn_types_for_stream_reliable() {
        let dt = create_test_transport();
        let conn_types = dt.get_conn_types_for(PayloadType::StreamReliable);

        assert_eq!(conn_types.len(), 2);
        assert_eq!(conn_types[0], ConnType::WebRTC);
        assert_eq!(conn_types[1], ConnType::WebSocket);
    }

    #[wasm_bindgen_test]
    async fn test_dest_transport_new_server() {
        let dest = Dest::Server("wss://server.com".to_string());
        let ws = WebSocketConnection::new("wss://server.com");
        let handle = WireHandle::WebSocket(ws);

        let dt = DestTransport::new(dest.clone(), vec![handle]).await;
        assert!(dt.is_ok());

        let transport = dt.unwrap();
        assert_eq!(transport.dest(), &dest);
    }

    #[wasm_bindgen_test]
    async fn test_dest_transport_new_peer() {
        let dest = Dest::Peer("peer-123".to_string());
        let rtc = WebRtcConnection::new("peer-123".to_string());
        let handle = WireHandle::WebRTC(rtc);

        let dt = DestTransport::new(dest.clone(), vec![handle]).await;
        assert!(dt.is_ok());

        let transport = dt.unwrap();
        assert_eq!(transport.dest(), &dest);
    }

    #[wasm_bindgen_test]
    async fn test_dest_transport_new_empty_connections() {
        let dest = Dest::Server("wss://test.com".to_string());

        let dt = DestTransport::new(dest.clone(), vec![]).await;
        assert!(dt.is_ok());

        let transport = dt.unwrap();
        assert_eq!(transport.dest(), &dest);
    }

    #[wasm_bindgen_test]
    async fn test_dest_transport_new_multiple_connections() {
        let dest = Dest::Peer("peer-abc".to_string());

        let ws = WebSocketConnection::new("wss://fallback.com");
        let ws_handle = WireHandle::WebSocket(ws);

        let rtc = WebRtcConnection::new("peer-abc".to_string());
        let rtc_handle = WireHandle::WebRTC(rtc);

        let dt = DestTransport::new(dest.clone(), vec![ws_handle, rtc_handle]).await;
        assert!(dt.is_ok());

        let transport = dt.unwrap();
        assert_eq!(transport.dest(), &dest);
    }

    #[wasm_bindgen_test]
    async fn test_dest_transport_dest_accessor() {
        let dest1 = Dest::Server("wss://test1.com".to_string());
        let dest2 = Dest::Peer("peer-456".to_string());

        let dt1 = DestTransport::new(dest1.clone(), vec![]).await.unwrap();
        let dt2 = DestTransport::new(dest2.clone(), vec![]).await.unwrap();

        assert_eq!(dt1.dest(), &dest1);
        assert_eq!(dt2.dest(), &dest2);
        assert_ne!(dt1.dest(), dt2.dest());
    }

    #[wasm_bindgen_test]
    async fn test_has_healthy_connection_no_connections() {
        let dest = Dest::Server("wss://test.com".to_string());
        let dt = DestTransport::new(dest, vec![]).await.unwrap();

        let healthy = dt.has_healthy_connection().await;
        assert!(!healthy);
    }

    #[wasm_bindgen_test]
    async fn test_has_healthy_connection_with_connected() {
        let dest = Dest::Server("wss://test.com".to_string());
        let ws = WebSocketConnection::new("wss://test.com");

        // Connect first.
        ws.connect().await.unwrap();

        let handle = WireHandle::WebSocket(ws);
        let dt = DestTransport::new(dest, vec![handle]).await.unwrap();

        // Need to wait for the connection-state update.
        gloo_timers::future::TimeoutFuture::new(100).await;

        let healthy = dt.has_healthy_connection().await;
        assert!(healthy);
    }

    #[test]
    fn test_conn_types_priority_order() {
        let dt = create_test_transport();

        // Non-RTP types should prefer WebRTC first.
        let types = dt.get_conn_types_for(PayloadType::RpcReliable);
        assert_eq!(types[0], ConnType::WebRTC);
        assert_eq!(types[1], ConnType::WebSocket);
    }

    #[test]
    fn test_media_rtp_only_webrtc() {
        let dt = create_test_transport();

        let types = dt.get_conn_types_for(PayloadType::MediaRtp);
        assert_eq!(types.len(), 1);
        assert!(!types.contains(&ConnType::WebSocket));
        assert!(types.contains(&ConnType::WebRTC));
    }

    #[test]
    fn test_dest_server_variant() {
        let dest = Dest::Server("wss://example.com".to_string());
        match dest {
            Dest::Server(url) => assert_eq!(url, "wss://example.com"),
            _ => panic!("Expected Server variant"),
        }
    }

    #[test]
    fn test_dest_peer_variant() {
        let dest = Dest::Peer("peer-xyz".to_string());
        match dest {
            Dest::Peer(id) => assert_eq!(id, "peer-xyz"),
            _ => panic!("Expected Peer variant"),
        }
    }

    // Helper function
    fn create_test_transport() -> DestTransport {
        let dest = Dest::Server("wss://test.com".to_string());
        let wire_pool = Arc::new(WirePool::new());
        DestTransport {
            dest,
            wire_pool,
            handover: Arc::new(HandoverBook::default()),
            reliable: Mutex::new(ReliableRoute::default()),
            rtc_recovery: AtomicU8::new(RTC_STABLE),
        }
    }
}
