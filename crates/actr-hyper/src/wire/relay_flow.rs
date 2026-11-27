//! Native adapter for the transport-independent peer window.
use crate::transport::NetworkError;
use actr_relay_security::peer_flow::FlowError;
pub(crate) use actr_relay_security::peer_flow::{
    INBOUND_SLOTS, MAX_CHANNELS, PeerFlow as RelayFlow, ReceiveCredit, SHARED_HEADROOM,
};

impl From<FlowError> for NetworkError {
    fn from(error: FlowError) -> Self {
        match error {
            FlowError::Closed => Self::WebSocketClosed(error.to_string()),
            FlowError::Protocol => Self::DeserializationError(error.to_string()),
            FlowError::TooLarge => Self::InvalidArgument(error.to_string()),
            FlowError::Busy(_) => Self::SendError(error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actr_relay_security::flow_control::{Class, Control};
    use actr_relay_security::peer_flow::WINDOW;
    use std::sync::Arc;
    use tokio::sync::{Semaphore, mpsc};
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn waiting_for_credit_has_not_started_a_transport_write() {
        use std::sync::atomic::{AtomicBool, Ordering};

        use actr_protocol::PayloadType;
        use bytes::Bytes;

        use crate::transport::lane::{DataLane, DeliveryProgress, WebSocketDataLane, WsSink};

        let cancel = CancellationToken::new();
        let budget = Arc::new(Semaphore::new(1024 * 1024));
        let (flow, _controls) = RelayFlow::new(cancel.clone(), budget.clone());
        flow.activate();
        let sink = futures_util::sink::unfold((), |(), _| async {
            Ok::<_, tokio_tungstenite::tungstenite::Error>(())
        });
        let sink: WsSink = Arc::new(tokio::sync::Mutex::new(Some(Box::pin(sink))));
        let (_tx, rx) = mpsc::channel(1);
        let lane = WebSocketDataLane::new(sink, PayloadType::StreamReliable, rx)
            .with_relay_flow(Some(flow.clone()));
        let started = AtomicBool::new(false);
        let on_write = || started.store(true, Ordering::Relaxed);

        let mut waiting = Box::pin(lane.send_tracked(Bytes::from_static(b"wait"), &on_write));
        assert!(futures_util::poll!(waiting.as_mut()).is_pending());
        drop(waiting);
        assert!(!started.load(Ordering::Relaxed));
        assert!(!cancel.is_cancelled());

        flow.receive_control(Control::Window([WINDOW; 2]), &budget)
            .unwrap();
        let attempt = lane
            .send_tracked(Bytes::from_static(b"submit"), &on_write)
            .await;
        assert!(started.load(Ordering::Relaxed));
        assert_eq!(attempt.progress, DeliveryProgress::SubmittedLocally);
        attempt.into_result().unwrap();
    }

    #[tokio::test]
    async fn retiring_a_channel_keeps_its_window_reserved_until_data_is_consumed() {
        let slots = Arc::new(Semaphore::new(1));
        let (flow, _) = RelayFlow::new(CancellationToken::new(), Arc::new(Semaphore::new(1)));
        flow.activate();
        flow.open_window(slots.clone().acquire_owned().await.unwrap());
        assert_eq!(flow.window_update().await, Control::Window([WINDOW; 2]));
        let credit = flow.admit(Class::Stream, 42).unwrap();
        flow.retire();
        assert_eq!(slots.available_permits(), 0);
        assert!(flow.admit(Class::Stream, 42).is_none());
        drop(credit);
        assert_eq!(slots.available_permits(), 1);
    }

    #[tokio::test]
    async fn cancelling_a_large_send_returns_its_reservation() {
        let cancel = CancellationToken::new();
        let pending = Arc::new(Semaphore::new(1024 * 1024));
        let (sender, mut sender_controls) = RelayFlow::new(cancel.child_token(), pending.clone());
        let (receiver, mut receiver_controls) = RelayFlow::new(cancel.child_token(), pending);
        let shared = Arc::new(Semaphore::new(1024 * 1024));
        sender.activate();
        receiver.activate();

        let size = 64 * 1024;
        let outbound = {
            let sender = sender.clone();
            tokio::spawn(async move { sender.reserve(Class::Stream, size).await.unwrap() })
        };
        let request = sender_controls.recv().await.unwrap();
        tokio::spawn(receiver.receive_control(request, &shared).unwrap().unwrap());
        let granted = receiver_controls.recv().await.unwrap();
        sender.receive_control(granted, &shared).unwrap();
        let reservation = outbound.await.unwrap().unwrap();
        assert!(shared.available_permits() < 1024 * 1024);

        // Cancelling before the write must not consume the peer's reservation.
        drop(reservation);
        let release = sender_controls.recv().await.unwrap();
        receiver.receive_control(release, &shared).unwrap();
        assert_eq!(shared.available_permits(), 1024 * 1024);
        assert!(receiver.admit(Class::Stream, size).is_none());
    }
}
