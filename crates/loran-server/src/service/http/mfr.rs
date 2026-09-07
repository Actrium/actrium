//! Publisher Registry HTTP Service Implementation
//!
//! Provides HTTP API service for Publisher domain verification, Actor package publishing and querying

use crate::service::HttpRouterService;
use actrix_mfr::{
    MfrManager,
    handlers::{MfrState, create_public_router},
};
use anyhow::Result;
use async_trait::async_trait;
use axum::Router;
use platform::config::ActrixConfig;
use platform::{ServiceInfo, ServiceType};
use std::sync::Arc;

/// Publisher Registry HTTP service implementation
#[derive(Debug)]
pub struct PublisherRegistryService {
    info: ServiceInfo,
    #[allow(dead_code)]
    config: ActrixConfig,
}

impl PublisherRegistryService {
    pub fn new(config: ActrixConfig) -> Self {
        Self {
            info: ServiceInfo::new(
                "Publisher Registry",
                ServiceType::PublisherRegistry,
                Some("HTTPS Publisher identity and Actor package registry".to_string()),
                &config,
            ),
            config,
        }
    }
}

#[async_trait]
impl HttpRouterService for PublisherRegistryService {
    fn info(&self) -> &ServiceInfo {
        &self.info
    }

    fn info_mut(&mut self) -> &mut ServiceInfo {
        &mut self.info
    }

    async fn build_router(&mut self) -> Result<Router> {
        platform::recording::info!("Building Publisher Registry router");

        let pool = platform::storage::db::get_database().get_pool().clone();
        let nonce_retain_secs = self.config.services.publisher_registry.nonce_retain_secs;
        let manager = MfrManager::new(pool).with_nonce_retain_secs(nonce_retain_secs);
        let state = MfrState {
            manager: Arc::new(manager),
        };
        let mfr_router = create_public_router(state);
        let router = Router::new().merge(mfr_router);

        platform::recording::info!("Publisher Registry router built successfully");
        Ok(router)
    }

    fn route_prefix(&self) -> &str {
        "/registry"
    }
}
