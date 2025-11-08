mod peer;

// Re-export ActrId from actr-protocol (new naming convention)
pub use actr_protocol::ActrId;
pub use peer::PeerId;

/// Tenant ID type - simple u32 wrapper
pub type TenantId = u32;
