//! # Loran
//!
//! Collection of WebRTC auxiliary servers including Signaling, STUN and TURN services

pub mod service;

// Re-export commonly used types
pub use platform::config::LoranConfig;
pub use service::{ServiceContainer, ServiceManager};
