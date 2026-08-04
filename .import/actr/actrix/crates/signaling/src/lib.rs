//! Actrix signaling service
//!
//! WebSocket signaling service based on the protobuf SignalingEnvelope protocol
//!
//! # Module structure
//!
//! ## Core modules
//! - [`server`]: WebSocket server and protocol handling
//! - [`service_registry`][]: Service registration and discovery
//!
//! ## Extension modules
//! - [`presence`] - Presence subscription management
//! - [`load_balancer`] - Load balancing algorithms
//! - [`geo`] - Geographic location and distance calculation
#![deny(clippy::disallowed_macros)]

pub mod geo;
pub mod load_balancer;
pub mod presence;
pub mod ratelimit;
pub mod server;
pub mod service_registry;
pub mod service_registry_storage;
#[cfg(feature = "opentelemetry")]
pub mod trace;

// Axum router integration
pub mod axum_router;

pub use axum_router::{
    create_signaling_router, create_signaling_router_with_config,
    create_signaling_router_with_config_and_counters,
};

// Re-export commonly used types
pub use load_balancer::LoadBalancer;
pub use presence::PresenceManager;
pub use server::{ClientConnection, SignalingServer, SignalingServerHandle};
pub use service_registry::{ServiceInfo, ServiceRegistry};

// Export WebSocket handler
pub use server::handle_websocket_connection;
