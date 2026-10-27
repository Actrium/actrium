//! Service Container
//!
//! Manages the container and lifecycle of various services
//! Service container module - encapsulates different service types

use super::{HttpRouterService, IceService};
use super::{IssuerService, PublisherRegistryService, SignalingService, StunService, TurnService};
use axum::Router;
use platform::ServiceInfo;
use url::Url;

/// Service container for encapsulating different service types
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ServiceContainer {
    Signaling(SignalingService),
    Issuer(IssuerService),
    #[allow(dead_code)]
    PublisherRegistry(PublisherRegistryService),
    Stun(StunService),
    Turn(TurnService),
}

impl ServiceContainer {
    /// Create a Signaling service container
    pub fn signaling(service: SignalingService) -> Self {
        Self::Signaling(service)
    }

    /// Create an Issuer service container
    pub fn issuer(service: IssuerService) -> Self {
        Self::Issuer(service)
    }

    /// Create a Publisher Registry service container
    #[allow(dead_code)]
    pub fn publisher_registry(service: PublisherRegistryService) -> Self {
        Self::PublisherRegistry(service)
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
            ServiceContainer::Issuer(_) => "Issuer",
            ServiceContainer::PublisherRegistry(_) => "Publisher Registry",
            ServiceContainer::Stun(_) => "STUN",
            ServiceContainer::Turn(_) => "TURN",
        }
    }

    pub fn info(&self) -> &ServiceInfo {
        match self {
            ServiceContainer::Signaling(service) => service.info(),
            ServiceContainer::Issuer(service) => service.info(),
            ServiceContainer::PublisherRegistry(service) => service.info(),
            ServiceContainer::Stun(service) => service.info(),
            ServiceContainer::Turn(service) => service.info(),
        }
    }

    pub fn is_http_router(&self) -> bool {
        matches!(
            self,
            ServiceContainer::Signaling(_)
                | ServiceContainer::Issuer(_)
                | ServiceContainer::PublisherRegistry(_)
        )
    }

    pub fn is_ice(&self) -> bool {
        matches!(self, ServiceContainer::Stun(_) | ServiceContainer::Turn(_))
    }

    /// Get the route prefix (only for HTTP router services)
    #[allow(dead_code)]
    pub fn route_prefix(&self) -> Option<&str> {
        match self {
            ServiceContainer::Signaling(service) => Some(service.route_prefix()),
            ServiceContainer::Issuer(service) => Some(service.route_prefix()),
            ServiceContainer::PublisherRegistry(service) => Some(service.route_prefix()),
            _ => None,
        }
    }

    /// Build the router (only for HTTP router services)
    #[allow(dead_code)]
    pub async fn build_router(&mut self) -> Option<Result<Router, anyhow::Error>> {
        match self {
            ServiceContainer::Signaling(service) => Some(service.build_router().await),
            ServiceContainer::Issuer(service) => Some(service.build_router().await),
            ServiceContainer::PublisherRegistry(service) => Some(service.build_router().await),
            _ => None,
        }
    }

    /// Service start callback (only for HTTP router services)
    pub async fn on_start(&mut self, base_url: Url) -> Option<Result<(), anyhow::Error>> {
        match self {
            ServiceContainer::Signaling(service) => Some(service.on_start(base_url).await),
            ServiceContainer::Issuer(service) => Some(service.on_start(base_url).await),
            ServiceContainer::PublisherRegistry(service) => Some(service.on_start(base_url).await),
            _ => None,
        }
    }
}
