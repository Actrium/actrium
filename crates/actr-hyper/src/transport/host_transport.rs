//! HostTransport - Intra-process transport manager
//!
//! Manages mpsc channel communication between Guest and Shell
//!
//! # Usage Examples
//!
//! ## Guest Side (Subscribe to data streams)
//!
//! ```rust,ignore
//! use actr_hyper::transport::HostTransport;
//! use std::sync::Arc;
//!
//! struct MyGuest {
//!     host_transport: Arc<HostTransport>,
//! }
//!
//! impl MyGuest {
//!     pub async fn subscribe_metrics_stream(&self) -> NetworkResult<()> {
//!         // Create LatencyFirst channel
//!         let rx = self.host_transport
//!             .create_latency_first_channel("metrics-stream".to_string())
//!             .await;
//!
//!         // Start receive loop
//!         tokio::spawn(async move {
//!             loop {
//!                 let mut receiver = rx.lock().await;
//!                 if let Some(envelope) = receiver.recv().await {
//!                     // Process streaming data
//!                     println!("Received: {:?}", envelope);
//!                 }
//!             }
//!         });
//!
//!         Ok(())
//!     }
//! }
//! ```
//!
//! ## Shell Side (Send data)
//!
//! ```rust,ignore
//! // Get HostTransport from ActrNode
//! if let Some(host_transport) = node.host_transport() {
//!     // Send to LatencyFirst channel
//!     let envelope = RpcEnvelope { /* ... */ };
//!     host_transport.send_message(
//!         PayloadType::StreamLatencyFirst,
//!         Some("metrics-stream".to_string()),
//!         envelope
//!     ).await?;
//! }
//! ```

use super::correlation::{CompleteOutcome, RpcCorrelation};
use super::{DataLane, MpscLane, NetworkError, NetworkResult, RpcDeadline};
use actr_framework::Bytes;
use actr_protocol::{ActrError, PayloadType, RpcEnvelope};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock, mpsc};

/// Host Transport - manages intra-process transport (mpsc channels)
///
/// # Design Philosophy
/// - **Guest <-> Shell communication bridge** (not for arbitrary Actor-to-Actor communication)
/// - **Reliable is mandatory, others are created on-demand**
/// - **Dynamic multi-channel management**: HashMap<String, Channel>
/// - **Bi-directional sharing**: Shell and Guest share the same Manager
pub struct HostTransport {
    // ========== Mandatory base channel ==========
    /// Reliable channel (must exist)
    reliable_tx: mpsc::Sender<RpcEnvelope>,
    reliable_rx: Arc<Mutex<mpsc::Receiver<RpcEnvelope>>>,

    // ========== Optional specialized channels ==========
    /// Signal channel (optional, lazy creation)
    signal_channel: Arc<Mutex<Option<ChannelPair>>>,

    /// LatencyFirst channels (multi-instance, indexed by channel_id)
    latency_first_channels: Arc<RwLock<HashMap<String, ChannelPair>>>,

    /// MediaTrack channels (multi-instance, indexed by track_id)
    media_track_channels: Arc<RwLock<HashMap<String, ChannelPair>>>,

    // ========== Management data ==========
    /// Lane cache (avoid repeated creation)
    lane_cache: Arc<RwLock<HashMap<LaneKey, Arc<dyn DataLane>>>>,

    /// Pending requests (request/response matching); entries are removed on
    /// every send_request exit path via the RAII guard, and by complete_*.
    correlation: RpcCorrelation<()>,

    /// In-process scope handoff (#390): the sender side registers root
    /// scopes here and workload intake claims them through the shared
    /// `Arc<HostTransport>`. Never consulted for peer transports.
    scope_registry: crate::request_scope::ScopeHandoffRegistry,
}

/// Channel pair (tx + rx)
#[derive(Clone)]
struct ChannelPair {
    tx: mpsc::Sender<RpcEnvelope>,
    rx: Arc<Mutex<mpsc::Receiver<RpcEnvelope>>>,
}

/// Lane cache key
#[derive(Hash, Eq, PartialEq, Clone, Debug)]
struct LaneKey {
    payload_type: PayloadType,
    /// channel_id (LatencyFirst) or track_id (MediaTrack)
    identifier: Option<String>,
}

impl Default for HostTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl HostTransport {
    /// Create new instance (only creates Reliable channel, others are lazy-initialized)
    ///
    /// HostTransport manages intra-process communication channels between Guest and Shell.
    /// It does not need ActorId as all communication is within a single process.
    pub fn new() -> Self {
        let (reliable_tx, reliable_rx) = mpsc::channel(1024);

        tracing::debug!("Created HostTransport");
        tracing::debug!("Created Reliable channel");

        Self {
            reliable_tx,
            reliable_rx: Arc::new(Mutex::new(reliable_rx)),
            signal_channel: Arc::new(Mutex::new(None)),
            latency_first_channels: Arc::new(RwLock::new(HashMap::new())),
            media_track_channels: Arc::new(RwLock::new(HashMap::new())),
            lane_cache: Arc::new(RwLock::new(HashMap::new())),
            correlation: RpcCorrelation::new(),
            scope_registry: crate::request_scope::ScopeHandoffRegistry::new(),
        }
    }

    // ========== Dynamic creation APIs ==========

    /// Ensure Signal channel exists
    async fn ensure_signal_channel(&self) -> ChannelPair {
        let mut opt = self.signal_channel.lock().await;
        if opt.is_none() {
            let (tx, rx) = mpsc::channel(1024);
            *opt = Some(ChannelPair {
                tx,
                rx: Arc::new(Mutex::new(rx)),
            });
            tracing::debug!("Created Signal channel");
        }
        // Safe: we just created it if it was None
        opt.as_ref()
            .expect("Signal channel must exist after ensure_signal_channel")
            .clone()
    }

    /// Create LatencyFirst channel
    #[cfg(feature = "test-utils")]
    pub async fn create_latency_first_channel(
        &self,
        channel_id: String,
    ) -> Arc<Mutex<mpsc::Receiver<RpcEnvelope>>> {
        let mut channels = self.latency_first_channels.write().await;

        if !channels.contains_key(&channel_id) {
            let (tx, rx) = mpsc::channel(1024);
            let pair = ChannelPair {
                tx,
                rx: Arc::new(Mutex::new(rx)),
            };
            let rx_clone = pair.rx.clone();
            channels.insert(channel_id.clone(), pair);

            tracing::debug!("Created LatencyFirst channel '{}'", channel_id);
            rx_clone
        } else {
            // Safe: we just checked contains_key
            channels
                .get(&channel_id)
                .expect("LatencyFirst channel must exist after contains_key check")
                .rx
                .clone()
        }
    }

    /// Create MediaTrack channel
    #[cfg(feature = "test-utils")]
    pub async fn create_media_track_channel(
        &self,
        track_id: String,
    ) -> Arc<Mutex<mpsc::Receiver<RpcEnvelope>>> {
        let mut channels = self.media_track_channels.write().await;

        if !channels.contains_key(&track_id) {
            let (tx, rx) = mpsc::channel(1024);
            let pair = ChannelPair {
                tx,
                rx: Arc::new(Mutex::new(rx)),
            };
            let rx_clone = pair.rx.clone();
            channels.insert(track_id.clone(), pair);

            tracing::debug!("Created MediaTrack channel '{}'", track_id);
            rx_clone
        } else {
            // Safe: we just checked contains_key
            channels
                .get(&track_id)
                .expect("MediaTrack channel must exist after contains_key check")
                .rx
                .clone()
        }
    }

    // ========== Lane retrieval APIs ==========

    /// Get Lane (with optional channel_id/track_id)
    ///
    /// # Arguments
    /// - `payload_type`: PayloadType
    /// - `identifier`:
    ///   - `None` for Reliable/Signal
    ///   - `Some(channel_id)` for LatencyFirst
    ///   - `Some(track_id)` for MediaTrack
    pub async fn get_lane(
        &self,
        payload_type: PayloadType,
        identifier: Option<String>,
    ) -> NetworkResult<Arc<dyn DataLane>> {
        let key = LaneKey {
            payload_type,
            identifier: identifier.clone(),
        };

        // 1. Check cache
        {
            let cache = self.lane_cache.read().await;
            if let Some(lane) = cache.get(&key) {
                tracing::debug!("Reusing cached Inproc DataLane: {:?}", key);
                return Ok(lane.clone());
            }
        }

        // 2. Get corresponding ChannelPair
        let pair = match payload_type {
            PayloadType::RpcReliable => ChannelPair {
                tx: self.reliable_tx.clone(),
                rx: self.reliable_rx.clone(),
            },

            PayloadType::RpcSignal => self.ensure_signal_channel().await,

            PayloadType::StreamReliable | PayloadType::StreamLatencyFirst => {
                let channel_id = identifier
                    .as_ref()
                    .ok_or_else(|| {
                        NetworkError::InvalidArgument("DataChunk requires channel_id".into())
                    })?
                    .clone();

                let channels = self.latency_first_channels.read().await;
                channels
                    .get(&channel_id)
                    .ok_or_else(|| NetworkError::ChannelNotFound(channel_id))?
                    .clone()
            }

            PayloadType::MediaRtp => {
                let track_id = identifier
                    .as_ref()
                    .ok_or_else(|| {
                        NetworkError::InvalidArgument("MediaRtp requires track_id".into())
                    })?
                    .clone();

                let channels = self.media_track_channels.read().await;
                channels
                    .get(&track_id)
                    .ok_or_else(|| NetworkError::ChannelNotFound(track_id))?
                    .clone()
            }
        };

        // 3. Create DataLane
        let lane: Arc<dyn DataLane> =
            Arc::new(MpscLane::new_shared(payload_type, pair.tx, pair.rx));

        // 4. Cache it
        self.lane_cache.write().await.insert(key, lane.clone());

        tracing::debug!(
            "Created Inproc DataLane: type={:?}, identifier={:?}",
            payload_type,
            identifier
        );

        Ok(lane)
    }

    // ========== High-level APIs ==========

    /// Shared scope handoff registry for this transport direction (#390).
    pub(crate) fn scope_registry(&self) -> &crate::request_scope::ScopeHandoffRegistry {
        &self.scope_registry
    }

    /// Send request (with response waiting)
    #[cfg_attr(
        feature = "opentelemetry",
        tracing::instrument(skip_all, name = "HostTransport.send_request")
    )]
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn send_request(
        &self,
        payload_type: PayloadType,
        identifier: Option<String>,
        envelope: RpcEnvelope,
    ) -> actr_protocol::ActorResult<Bytes> {
        let deadline = RpcDeadline::from_timeout_ms(envelope.timeout_ms)?;
        self.send_request_until(payload_type, identifier, envelope, deadline)
            .await
    }

    /// Send a request using a deadline that was created by the API entry
    /// point.  Lane lookup, channel admission, the send, and response waiting
    /// all consume this same absolute budget.
    pub(crate) async fn send_request_until(
        &self,
        payload_type: PayloadType,
        identifier: Option<String>,
        envelope: RpcEnvelope,
        deadline: RpcDeadline,
    ) -> actr_protocol::ActorResult<Bytes> {
        deadline.ensure_remaining()?;
        let operation = async {
            // Register the pending entry before any awaited setup.  The RAII
            // guard removes it if lane lookup, channel admission, response
            // waiting, or cancellation exits this operation.
            let pending = self.correlation.register(envelope.request_id.clone(), ());

            let lane = self
                .get_lane(payload_type, identifier)
                .await
                .map_err(ActrError::from)?;
            deadline.ensure_remaining()?;
            lane.send_envelope(envelope)
                .await
                .map_err(ActrError::from)?;

            // The outer timeout owns the combined send + response budget;
            // do not start another relative timeout here.
            pending.recv().await
        };

        match crate::timer::timeout_at(
            crate::timer::ids::HOST_REQUEST_DEADLINE,
            deadline.instant(),
            operation,
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(ActrError::TimedOut),
        }
    }

    /// Send one-way message
    #[cfg_attr(
        feature = "opentelemetry",
        tracing::instrument(skip_all, name = "HostTransport.send_message")
    )]
    pub async fn send_message(
        &self,
        payload_type: PayloadType,
        identifier: Option<String>,
        envelope: RpcEnvelope,
    ) -> NetworkResult<()> {
        let lane = self.get_lane(payload_type, identifier).await?;
        lane.send_envelope(envelope).await
    }

    /// Send a one-way message under a caller-owned absolute deadline.
    pub(crate) async fn send_message_until(
        &self,
        payload_type: PayloadType,
        identifier: Option<String>,
        envelope: RpcEnvelope,
        deadline: RpcDeadline,
    ) -> NetworkResult<()> {
        deadline
            .ensure_remaining()
            .map_err(|error| NetworkError::TimeoutError(error.to_string()))?;
        let operation = async {
            let lane = self.get_lane(payload_type, identifier).await?;
            deadline
                .ensure_remaining()
                .map_err(|error| NetworkError::TimeoutError(error.to_string()))?;
            lane.send_envelope(envelope).await
        };

        match crate::timer::timeout_at(
            crate::timer::ids::OUTBOUND_TELL_DEADLINE,
            deadline.instant(),
            operation,
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(NetworkError::TimeoutError(
                "outbound tell timed out".to_string(),
            )),
        }
    }

    /// Receive one message (select first available from all channels)
    ///
    /// # Returns
    /// - `Some(envelope)`: received message (response matching already handled)
    /// - `None`: all channels closed
    #[cfg(feature = "test-utils")]
    pub async fn recv(&self) -> Option<RpcEnvelope> {
        loop {
            tokio::select! {
                biased;

                // Signal (highest priority)
                msg = Self::recv_from_channel_opt(&self.signal_channel) => {
                    if let Some(envelope) = msg {
                        if !self.try_complete_response(&envelope).await {
                            return Some(envelope);  // It's a request
                        }
                        // It's a response, already handled, continue loop
                    }
                }

                // Reliable
                msg = Self::recv_from_channel(&self.reliable_rx) => {
                    if let Some(envelope) = msg {
                        if !self.try_complete_response(&envelope).await {
                            return Some(envelope);
                        }
                    }
                }

                // TODO: LatencyFirst and MediaTrack reception
                // Need to implement receiving from all channels in HashMap
            }
        }
    }

    /// Drain one envelope from the Reliable channel **without** the
    /// response-matching step [`Self::recv`] performs.
    ///
    /// [`Self::recv`] calls `try_complete_response`, which — on a single
    /// bidirectional `HostTransport` where the guest both registers its pending
    /// (`send_request`) and the envelope is later drained on the same
    /// correlation map — would immediately self-complete the guest's own
    /// outbound request as if it were its response (an echo). A gate harness
    /// needs to instead *observe* the outbound request (its `route_key` +
    /// `request_id`), hold it suspended for as long as the test wants, and only
    /// then answer it via [`Self::complete_response`] / [`Self::complete_error`].
    ///
    /// This raw drain returns the request envelope untouched so the harness owns
    /// the release timing. It is the native-`Linked` mirror of the WASM
    /// `HostAbiFn` bridge's `CallRaw` interception used by the M6 isomorphism
    /// suite; it changes no production path (test-utils only).
    #[cfg(feature = "test-utils")]
    pub async fn recv_reliable_raw(&self) -> Option<RpcEnvelope> {
        self.reliable_rx.lock().await.recv().await
    }

    /// Complete a pending request with response payload
    ///
    /// # Arguments
    /// - `request_id`: The request ID to complete
    /// - `response_bytes`: Response payload
    ///
    /// # Returns
    /// - `Ok(())`: Successfully sent response to waiting sender
    /// - `Err(NetworkError)`: No pending request found with this ID
    pub async fn complete_response(
        &self,
        request_id: &str,
        response_bytes: Bytes,
    ) -> NetworkResult<()> {
        match self.correlation.complete(request_id, Ok(response_bytes)) {
            CompleteOutcome::Completed => {
                tracing::debug!("Completed pending request: {}", request_id);
                Ok(())
            }
            CompleteOutcome::Orphan => Err(NetworkError::InvalidArgument(format!(
                "No pending request found for id: {request_id}"
            ))),
        }
    }

    /// Complete a pending request with an error
    ///
    /// # Returns
    /// - `Ok(())`: Successfully sent error to waiting sender
    /// - `Err(NetworkError)`: No pending request found with this ID
    pub async fn complete_error(&self, request_id: &str, error: ActrError) -> NetworkResult<()> {
        match self.correlation.complete(request_id, Err(error)) {
            CompleteOutcome::Completed => {
                tracing::debug!("Completed pending request with error: {}", request_id);
                Ok(())
            }
            CompleteOutcome::Orphan => Err(NetworkError::InvalidArgument(format!(
                "No pending request found for id: {request_id}"
            ))),
        }
    }

    /// Handle response matching (returns true if it was a response)
    #[cfg(feature = "test-utils")]
    async fn try_complete_response(&self, envelope: &RpcEnvelope) -> bool {
        // Reconstructs the precise ActrError variant from the wire code so
        // test-utils tests observe the same classification production call
        // sites do — not a flat Unavailable.
        let result = super::correlation::envelope_response_to_result(
            envelope.payload.clone(),
            envelope.error.clone(),
        );
        match self.correlation.complete(&envelope.request_id, result) {
            CompleteOutcome::Completed => {
                tracing::debug!("Completed pending request: {}", envelope.request_id);
                true
            }
            // No pending entry: the envelope is a request, not a response.
            CompleteOutcome::Orphan => false,
        }
    }

    // ========== Helper methods ==========

    /// Number of in-flight pending requests (leak regression checks).
    #[cfg(test)]
    fn pending_len(&self) -> usize {
        self.correlation.len()
    }

    #[cfg(feature = "test-utils")]
    async fn recv_from_channel(
        rx: &Arc<Mutex<mpsc::Receiver<RpcEnvelope>>>,
    ) -> Option<RpcEnvelope> {
        rx.lock().await.recv().await
    }

    #[cfg(feature = "test-utils")]
    async fn recv_from_channel_opt(opt: &Arc<Mutex<Option<ChannelPair>>>) -> Option<RpcEnvelope> {
        let rx = {
            let guard = opt.lock().await;
            guard.as_ref().map(|pair| pair.rx.clone())
        };

        if let Some(rx) = rx {
            rx.lock().await.recv().await
        } else {
            std::future::pending().await // If doesn't exist, wait forever
        }
    }
}

#[cfg(test)]
#[path = "host_transport_tests.rs"]
mod tests;
