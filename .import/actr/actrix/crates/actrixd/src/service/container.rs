//\! Service container
//\!
//\! Manages the container and lifecycle of various services
//! Service container module - encapsulates different service types

use super::{AisService, MfrService, SignalingService, StunService, TurnService};
use super::{HttpRouterService, IceService};
use axum::Router;
use platform::ServiceInfo;
use url::Url;

/// Service container for encapsulating different service types
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ServiceContainer {
    Signaling(SignalingService),
    Ais(AisService),
    #[allow(dead_code)]
    Mfr(MfrService),
    Stun(StunService),
    Turn(TurnService),
}

impl ServiceContainer {
    /// Create a Signaling service container
    pub fn signaling(service: SignalingService) -> Self {
        Self::Signaling(service)
    }

    /// Create an AIS service container
    pub fn ais(service: AisService) -> Self {
        Self::Ais(service)
    }

    /// Create an MFR service container
    #[allow(dead_code)]
    pub fn mfr(service: MfrService) -> Self {
        Self::Mfr(service)
    }

    /// Create a STUN service container
    pub fn stun(service: StunService) -> Self {
        Self::Stun(service)
    }

    /// Create a TURN service container
    pub fn turn(service: TurnService) -> Self {
        Self::Turn(service)
    }

    #[allow(dead_code)]
    pub fn service_type(&self) -> &'static str {
        match self {
            ServiceContainer::Signaling(_) => "Signaling",
            ServiceContainer::Ais(_) => "AIS",
            ServiceContainer::Mfr(_) => "MFR",
            ServiceContainer::Stun(_) => "STUN",
            ServiceContainer::Turn(_) => "TURN",
        }
    }

    pub fn info(&self) -> &ServiceInfo {
        match self {
            ServiceContainer::Signaling(service) => service.info(),
            ServiceContainer::Ais(service) => service.info(),
            ServiceContainer::Mfr(service) => service.info(),
            ServiceContainer::Stun(service) => service.info(),
            ServiceContainer::Turn(service) => service.info(),
        }
    }

    pub fn is_http_router(&self) -> bool {
        matches!(
            self,
            ServiceContainer::Signaling(_) | ServiceContainer::Ais(_) | ServiceContainer::Mfr(_)
        )
    }

    pub fn is_ice(&self) -> bool {
        matches!(self, ServiceContainer::Stun(_) | ServiceContainer::Turn(_))
    }

    /// Get route prefix (only applies to HTTP route services)
    #[allow(dead_code)]
    pub fn route_prefix(&self) -> Option<&str> {
        match self {
            ServiceContainer::Signaling(service) => Some(service.route_prefix()),
            ServiceContainer::Ais(service) => Some(service.route_prefix()),
            ServiceContainer::Mfr(service) => Some(service.route_prefix()),
            _ => None,
        }
    }

    /// Build router (only applies to HTTP route services)
    #[allow(dead_code)]
    pub async fn build_router(&mut self) -> Option<Result<Router, anyhow::Error>> {
        match self {
            ServiceContainer::Signaling(service) => Some(service.build_router().await),
            ServiceContainer::Ais(service) => Some(service.build_router().await),
            ServiceContainer::Mfr(service) => Some(service.build_router().await),
            _ => None,
        }
    }

    /// Service start callback (only applies to HTTP route services)
    pub async fn on_start(&mut self, base_url: Url) -> Option<Result<(), anyhow::Error>> {
        match self {
            ServiceContainer::Signaling(service) => Some(service.on_start(base_url).await),
            ServiceContainer::Ais(service) => Some(service.on_start(base_url).await),
            ServiceContainer::Mfr(service) => Some(service.on_start(base_url).await),
            _ => None,
        }
    }
}
