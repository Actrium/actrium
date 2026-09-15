//! Actr-owned handles for the upstream v0.21 WebRTC objects.
//!
//! The v0.21 async crate seals its public `DataChannel` and `RtpSender` traits.
//! Keeping the narrow capabilities used by the Controller/Session boundary in
//! local traits preserves deterministic test doubles without weakening the
//! upstream API. Production objects are wrapped at this boundary.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::BytesMut;

use webrtc::data_channel::{
    DataChannel as UpstreamDataChannel, DataChannelEvent, RTCDataChannelState,
};
use webrtc::error::Error as RtcError;
use webrtc::rtp_transceiver::RtpSender as UpstreamRtpSender;

/// The DataChannel operations owned by Actr's lane/session boundary.
#[async_trait]
pub(crate) trait LaneDataChannel: Send + Sync + 'static {
    async fn label(&self) -> Result<String, RtcError>;
    async fn negotiated(&self) -> Result<bool, RtcError>;
    async fn ready_state(&self) -> Result<RTCDataChannelState, RtcError>;
    async fn outstanding_bytes(&self) -> Result<usize, RtcError>;
    async fn send(&self, data: BytesMut) -> Result<(), RtcError>;
    async fn send_text(&self, text: &str) -> Result<(), RtcError>;
    async fn try_send(&self, data: BytesMut) -> Result<(), RtcError>;
    async fn writable(&self) -> Result<(), RtcError>;
    async fn poll(&self) -> Option<DataChannelEvent>;
    async fn close(&self) -> Result<(), RtcError>;
}

/// Production adapter for an upstream sealed DataChannel trait object.
pub(crate) struct RealLaneDataChannel {
    inner: Arc<dyn UpstreamDataChannel>,
}

impl RealLaneDataChannel {
    pub(crate) fn new(inner: Arc<dyn UpstreamDataChannel>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl LaneDataChannel for RealLaneDataChannel {
    async fn label(&self) -> Result<String, RtcError> {
        self.inner.label().await
    }

    async fn negotiated(&self) -> Result<bool, RtcError> {
        self.inner.negotiated().await
    }

    async fn ready_state(&self) -> Result<RTCDataChannelState, RtcError> {
        self.inner.ready_state().await
    }

    async fn outstanding_bytes(&self) -> Result<usize, RtcError> {
        self.inner.outstanding_bytes().await
    }

    async fn send(&self, data: BytesMut) -> Result<(), RtcError> {
        self.inner.send(data).await
    }

    async fn send_text(&self, text: &str) -> Result<(), RtcError> {
        self.inner.send_text(text).await
    }

    async fn try_send(&self, data: BytesMut) -> Result<(), RtcError> {
        self.inner.try_send(data).await
    }

    async fn writable(&self) -> Result<(), RtcError> {
        self.inner.writable().await
    }

    async fn poll(&self) -> Option<DataChannelEvent> {
        self.inner.poll().await
    }

    async fn close(&self) -> Result<(), RtcError> {
        self.inner.close().await
    }
}

/// The RTP sender operations needed by Actr's media bookkeeping.
#[async_trait]
pub(crate) trait ActrRtpSender: Send + Sync + 'static {
    fn id(&self) -> rtc::rtp_transceiver::RTCRtpSenderId;
    async fn track_id(&self) -> String;
    fn upstream(&self) -> Option<Arc<dyn UpstreamRtpSender>>;
}

/// Production adapter for an upstream sealed RTP sender trait object.
pub(crate) struct RealRtpSender {
    inner: Arc<dyn UpstreamRtpSender>,
}

impl RealRtpSender {
    pub(crate) fn new(inner: Arc<dyn UpstreamRtpSender>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl ActrRtpSender for RealRtpSender {
    fn id(&self) -> rtc::rtp_transceiver::RTCRtpSenderId {
        self.inner.id()
    }

    async fn track_id(&self) -> String {
        self.inner.track().track_id().await
    }

    fn upstream(&self) -> Option<Arc<dyn UpstreamRtpSender>> {
        Some(self.inner.clone())
    }
}
