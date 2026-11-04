//! Publisher Registry HTTP Service Implementation
//!
//! Provides HTTP API service for Publisher domain verification, Actor package publishing and querying

use crate::service::HttpRouterService;
use anyhow::Result;
use async_trait::async_trait;
use axum::Router;
use loran_publisher_registry::{
    PublisherRegistryManager,
    handlers::{PublisherRegistryState, create_public_router},
};
use platform::config::LoranConfig;
use platform::{ServiceInfo, ServiceType};
use std::sync::Arc;

/// Publisher Registry HTTP service implementation
#[derive(Debug)]
pub struct PublisherRegistryService {
    info: ServiceInfo,
    #[allow(dead_code)]
    config: LoranConfig,
}

impl PublisherRegistryService {
    pub fn new(config: LoranConfig) -> Self {
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
        let manager = PublisherRegistryManager::new(pool).with_nonce_retain_secs(nonce_retain_secs);
        let state = PublisherRegistryState {
            manager: Arc::new(manager),
        };
        let publisher_registry_router = create_public_router(state);
        let router = Router::new().merge(publisher_registry_router);

        platform::recording::info!("Publisher Registry router built successfully");
        Ok(router)
    }

    fn route_prefix(&self) -> &str {
        "/registry"
    }
}
