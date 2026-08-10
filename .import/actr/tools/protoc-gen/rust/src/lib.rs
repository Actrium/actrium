//! # actr-framework-protoc-codegen
//!
//! Protoc plugin for generating actr-framework code from protobuf service definitions.
//!
//! This crate generates:
//! - Handler traits for service implementations
//! - MessageDispatcher implementations for request routing
//! - Workload wrapper types
//! - Message trait implementations (for Context::call/tell)
//! - PayloadType-aware client code
//! - Unified workload-level `actr_dispatch.rs` sourced from DispatchSpec v1.
//!
//! The only external consumer of this library is the sibling binary
//! `protoc-gen-actrframework`, which consumes the re-exported subset below.
//! `payload_type_extractor` defines the `PayloadType` enum and the default
//! lane a per-service RPC method emits; the declared `(actr.payload_type)`
//! option itself is carried by the unified dispatch table (DispatchSpec).

pub(crate) mod dispatch_spec;
pub(crate) mod modern_generator;
pub(crate) mod option_extractor;
pub mod payload_type_extractor;
pub(crate) mod unified_dispatch;

pub use dispatch_spec::{DispatchRoute, DispatchSpec};
pub use modern_generator::{GeneratorRole, ModernGenerator, RemoteServiceInfo};
pub use option_extractor::extract_reply_modes;
pub use payload_type_extractor::{
    ExplicitPayloadTypeMap, MethodKey, PayloadType, build_explicit_payload_types,
};
pub use unified_dispatch::UnifiedDispatcherGenerator;
