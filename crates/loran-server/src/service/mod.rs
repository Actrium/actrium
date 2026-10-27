//! Service management module
//!
//! Manages the lifecycle of various auxiliary services
//! # Service Management Abstraction
//!
//! Provides a generic service management abstraction for managing Signer, Issuer, Signaling, STUN/TURN, and other services.
//!
//! ## Core Concepts
//!
//! - `HttpRouterService`: core trait for HTTP routing services, provides an axum router
//! - `IceService`: core trait for ICE services, standalone UDP server
//! - `ServiceInfo`: basic service information
//! - `ServiceManager`: service manager, responsible for managing the lifecycle of multiple services

pub mod container;
pub mod grpc;
pub mod http;
pub mod ice;
pub mod manager;
#[cfg(test)]
mod publisher_fixture;
pub mod trace;

use anyhow::Result;
use async_trait::async_trait;
use axum::Router;
use platform::{ServiceInfo, ServiceState};
use std::fmt::Debug;
use url::Url;

// Re-export service implementations
pub use http::{IssuerService, PublisherRegistryService, SignalingService};
pub use ice::{StunService, TurnService};

// Re-export core components
pub use container::ServiceContainer;
pub use manager::ServiceManager;

/// Core trait for HTTP routing services - provides a router for axum
#[async_trait]
pub trait HttpRouterService: Send + Sync + Debug {
    /// Get service information
    fn info(&self) -> &ServiceInfo;

    /// Get mutable service information
    fn info_mut(&mut self) -> &mut ServiceInfo;

    /// Build the axum router
    async fn build_router(&mut self) -> Result<Router>;

    /// Service start callback (called after the router is built and started)
    async fn on_start(&mut self, base_url: Url) -> Result<()> {
        self.info_mut().set_running(base_url);
        Ok(())
    }

    /// Service stop callback
    async fn on_stop(&mut self) -> Result<()> {
        platform::recording::info!("HTTP router service '{}' stopped", self.info().name);
        self.info_mut().status = ServiceState::Unknown;
        Ok(())
    }

    /// Get the route prefix (e.g. "/admin", "/authority", etc.)
    fn route_prefix(&self) -> &str;
}

/// Core trait for ICE services - standalone UDP server
#[async_trait]
pub trait IceService: Send + Sync + Debug {
    /// Get service information
    fn info(&self) -> &ServiceInfo;

    /// Get mutable service information
    fn info_mut(&mut self) -> &mut ServiceInfo;

    /// Start the ICE service
    async fn start(
        &mut self,
        shutdown_rx: tokio::sync::broadcast::Receiver<()>,
        oneshot_tx: tokio::sync::oneshot::Sender<ServiceInfo>,
    ) -> Result<()>;

    /// Stop the ICE service
    async fn stop(&mut self) -> Result<()> {
        platform::recording::info!("ICE service '{}' stopped", self.info().name);
        self.info_mut().status = ServiceState::Unknown;
        Ok(())
    }

    /// Get service health status
    #[allow(dead_code)]
    async fn health_check(&self) -> Result<bool> {
        Ok(self.info().is_running())
    }
}
