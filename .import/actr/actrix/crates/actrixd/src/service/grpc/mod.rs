//! gRPC service module
//!
//! Manages the implementation of various gRPC services

pub mod signer;

pub use signer::build_signer_grpc_router;
