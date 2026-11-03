//! Base infrastructure library
//!
//! Provides infrastructure components for Loran auxiliary services, including identity management, encryption, monitoring, storage, Realm management, and other core functionality
#![deny(clippy::disallowed_macros)]

pub mod aid;
pub mod error;
pub mod metrics;
pub mod monitoring;
pub mod net;
pub mod realm;
pub mod recording;
pub mod storage;
pub mod types;

pub mod config;
pub mod util;

// Re-export commonly used types for convenience
pub use aid::{AIdCredential, AIdCredentialValidator, AidError, IdentityClaims};
pub use error::{
    BaseError, ConfigError, DatabaseError, NetworkError, Result, SerializationError, StorageError,
    ValidationError,
};
pub use monitoring::{
    MetricSample, MetricsStore, ServiceCollector, ServiceCounters, ServiceInfo, ServiceState,
    ServiceType,
};
pub use realm::{ActorAcl, Realm, RealmError};
pub use storage::SqliteNonceStorage;
pub use types::{ActrId, PeerId};
pub use util::TlsConfigurer;

// Simplified credential module for backward compatibility
pub mod token {
    pub use crate::aid::credential::{AIdCredentialValidator, AidError};
}

// Create a status module for backward compatibility
pub mod status {
    pub mod services {
        pub use crate::monitoring::ServiceState;
    }
}
