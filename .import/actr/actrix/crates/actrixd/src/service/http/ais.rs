//! AIS (Actor Identity Service) HTTP service implementation
//!
//! Provides HTTP API services for ActrId registration and Token issuance

use crate::service::HttpRouterService;
use ais::handlers::AISState;
use anyhow::Result;
use async_trait::async_trait;
use axum::Router;
use platform::config::ActrixConfig;
use platform::monitoring::ServiceCounters;
use platform::{ServiceInfo, ServiceType};
use std::sync::Arc;

/// AIS HTTP service implementation
///
/// The serving router is built from an [`AISState`] created once per router
/// build by the service manager, so the control plane can hold the same
/// issuer for authenticated key rotation. Instances registered with the
/// service collector for display purposes carry no state and never build
/// a router.
pub struct AisService {
    info: ServiceInfo,
    state: Option<AISState>,
}

impl std::fmt::Debug for AisService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AisService")
            .field("info", &self.info)
            .field("state", &self.state.is_some())
            .finish()
    }
}

impl AisService {
    /// Display-only instance for the service collector.
    pub fn new(config: &ActrixConfig) -> Self {
        Self {
            info: ServiceInfo::new(
                "AIS Service",
                ServiceType::Ais,
                Some(
                    "Actor Identity Service - ActrId registration and credential issuance service"
                        .to_string(),
                ),
                config,
            ),
            state: None,
        }
    }

    /// Serving instance around a shared AIS state.
    pub fn with_state(config: &ActrixConfig, state: AISState) -> Self {
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
impl HttpRouterService for AisService {
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
            .ok_or_else(|| anyhow::anyhow!("AIS state not attached to this service instance"))?;
        platform::recording::info!("Building AIS router");
        let router = Router::new().merge(ais::create_router_from_state(state));
        platform::recording::info!("AIS router built successfully");
        Ok(router)
    }

    fn route_prefix(&self) -> &str {
        "/ais"
    }
}
