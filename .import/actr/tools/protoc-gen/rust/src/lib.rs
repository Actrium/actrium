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
//!
//! The only external consumer of this library is the sibling binary
//! `protoc-gen-actrframework`, which consumes the re-exported subset below.
//! `payload_type_extractor` recovers the `(actr.payload_type)` method option
//! from the raw request bytes (prost drops it as an unknown field) and is
//! public so the binary can thread the extracted map into the generator.

pub(crate) mod modern_generator;
pub mod payload_type_extractor;

pub use modern_generator::{GeneratorRole, ModernGenerator, RemoteServiceInfo};
pub use payload_type_extractor::{
    ExplicitPayloadTypeMap, MethodKey, PayloadType, build_explicit_payload_types,
};
