//! Admin control-plane library for actrix nodes.
//!
//! This crate is the canonical implementation for node-side control-plane
//! behavior (node_admin gRPC API server).
#![deny(clippy::disallowed_macros)]

pub mod auth;
pub mod error;
pub mod metrics;
pub mod nonce_auth;
pub mod realm;
pub mod service;

pub use auth::{AuthService, CredentialPayload};
pub use error::{AdminError, Result as AdminResult};
pub use realm::realm_to_proto;
pub use service::{
    AdminApiService, ConfigFileContent, HealthInfo, KeyInfo, PlatformDetail, ServiceDetail,
    SignerKeysResult,
};

// Re-export commonly used proto types from actrix-proto.
pub use actrix_proto::{
    ConfigOverrideEntry as ProtoConfigOverrideEntry, ConfigType,
    ControlHealthCheckRequest as HealthCheckRequest,
    ControlHealthCheckResponse as HealthCheckResponse, ControlService, ControlServiceClient,
    ControlServiceServer, CreateRealmRequest, CreateRealmResponse, DeleteConfigOverrideRequest,
    DeleteConfigOverrideResponse, Directive, DirectiveType, GetConfigRequest, GetConfigResponse,
    GetNodeInfoRequest, GetNodeInfoResponse, GetRealmRequest, GetRealmResponse,
    ListConfigOverridesRequest, ListConfigOverridesResponse, ListRealmsRequest, ListRealmsResponse,
    NodeAdminService, NodeAdminServiceClient, NodeAdminServiceServer, NonceCredential, RealmInfo,
    RegisterNodeRequest, RegisterNodeResponse, ReportRequest, ReportResponse, ResourceType,
    RetireRealmRequest, RetireRealmResponse, ServiceAdvertisement, ServiceAdvertisementStatus,
    ServiceStatus, SetConfigOverrideRequest, SetConfigOverrideResponse, ShutdownRequest,
    ShutdownResponse, SystemMetrics, UpdateConfigRequest, UpdateConfigResponse, UpdateRealmRequest,
    UpdateRealmResponse,
};
