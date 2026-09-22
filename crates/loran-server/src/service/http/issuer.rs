//! Issuer HTTP service implementation
//!
//! Provides HTTP API services for ActrId registration and Token issuance

use crate::service::HttpRouterService;
use anyhow::Result;
use async_trait::async_trait;
use axum::Router;
use issuer::handlers::IssuerState;
use platform::config::ActrixConfig;
use platform::monitoring::ServiceCounters;
use platform::{ServiceInfo, ServiceType};
use std::sync::Arc;

/// Issuer HTTP service implementation
///
/// The serving router is built from an [`IssuerState`] created once per router
/// build by the service manager, so the control plane can hold the same
/// issuer for authenticated key rotation. Instances registered with the
/// service collector for display purposes carry no state and never build
/// a router.
pub struct IssuerService {
    info: ServiceInfo,
    state: Option<IssuerState>,
}

impl std::fmt::Debug for IssuerService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssuerService")
            .field("info", &self.info)
            .field("state", &self.state.is_some())
            .finish()
    }
}

impl IssuerService {
    /// Display-only instance for the service collector.
    pub fn new(config: &ActrixConfig) -> Self {
        Self {
            info: ServiceInfo::new(
                "Issuer Service",
                ServiceType::Issuer,
                Some(
                    "Actor Identity Service - ActrId registration and credential issuance service"
                        .to_string(),
                ),
                config,
            ),
            state: None,
        }
    }

    /// Serving instance around a shared Issuer state.
    pub fn with_state(config: &ActrixConfig, state: IssuerState) -> Self {
        let mut svc = Self::new(config);
        svc.state = Some(state);
        svc
    }

    /// Attach service-level counters.
    pub fn set_counters(&mut self, counters: Arc<ServiceCounters>) {
        self.info.set_counters(counters.clone());
        self.state = self.state.take().map(|state| state.with_counters(counters));
    }
}

#[async_trait]
impl HttpRouterService for IssuerService {
    fn info(&self) -> &ServiceInfo {
        &self.info
    }

    fn info_mut(&mut self) -> &mut ServiceInfo {
        &mut self.info
    }

    async fn build_router(&mut self) -> Result<Router> {
        let state = self
            .state
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Issuer state not attached to this service instance"))?;
        platform::recording::info!("Building Issuer router");
        let router = Router::new().merge(issuer::create_router_from_state(state));
        platform::recording::info!("Issuer router built successfully");
        Ok(router)
    }

    fn route_prefix(&self) -> &str {
        "/issuer"
    }
}
