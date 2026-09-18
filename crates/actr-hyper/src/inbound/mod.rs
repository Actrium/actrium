//! Inbound Layer 3: Inbound dispatch layer
//!
//! Responsible for inbound message routing and dispatching:
//! - InboundDispatcher: shared RPC/DataChunk dispatch (extracted from WebRtcGate)
//! - DataChunkRegistry: LatencyFirst type message registration and callback (streaming data chunks)
//! - MediaFrameRegistry: MediaTrack type message registration and callback (media streams)

mod data_chunk_registry;
pub(crate) mod dispatcher;
mod media_frame_registry;

#[cfg(test)]
pub(crate) use data_chunk_registry::DataChunkCallback;
pub(crate) use data_chunk_registry::DataChunkRegistry;
pub use media_frame_registry::MediaFrameRegistry;
#[cfg(feature = "test-utils")]
pub use media_frame_registry::MediaTrackCallback;

// MediaSample and MediaType are now re-exported from actr-framework, not here
