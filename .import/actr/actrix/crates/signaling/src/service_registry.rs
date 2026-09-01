//! Service registry implementation
//!
//! Responsible for managing all registered services and providing service discovery
//!
//! ## Persistence strategy
//!
//! - **In-memory HashMap**: primary store, fast lookups
//! - **SQLite cache**: optional, used for restart recovery
//! - **Background writes**: non-blocking, async writes to the database

use actr_protocol::{ActrId, ActrType};
use platform::RealmError;
use platform::realm::acl::ActorAcl;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::service_registry_storage::ServiceRegistryStorage;

/// Service expiry threshold (seconds) - a service is considered expired if no heartbeat is received within this time
pub const SERVICE_EXPIRY_THRESHOLD_SECS: u64 = 60;

/// Cleanup task execution interval (seconds)
pub const CLEANUP_INTERVAL_SECS: u64 = 30;

/// Service capability description
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceCapabilities {
    /// Maximum concurrent requests
    pub max_concurrent_requests: Option<u32>,
    /// Supported version range
    pub version_range: Option<String>,
    /// Region
    pub region: Option<String>,
    /// Custom tags
    pub tags: Option<HashMap<String, String>>,
}

/// Service status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ServiceStatus {
    Available,
    Busy,
    Maintenance,
    Unavailable,
}

/// Service info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub actor_id: ActrId,
    pub service_name: String,
    pub message_types: Vec<String>,
    pub capabilities: Option<ServiceCapabilities>,
    pub status: ServiceStatus,
    pub last_heartbeat_time_secs: u64, // Unix timestamp

    // New field: protocol spec
    /// Service protocol spec (includes fingerprint, protobufs, tags, etc.)
    #[serde(skip)]
    pub service_spec: Option<actr_protocol::ServiceSpec>,

    // New field: access control list
    /// ACL rules
    #[serde(skip)]
    pub acl: Option<actr_protocol::Acl>,

    // New field: load metrics (from Ping messages)
    /// Service availability state (protobuf enum ServiceAvailabilityState, stored as i32)
    /// FULL=0, DEGRADED=1, OVERLOADED=2, UNAVAILABLE=3
    pub service_availability_state: Option<i32>,
    /// Remaining processing capacity (0.0 ~ 1.0)
    pub power_reserve: Option<f32>,
    /// Message backlog (0.0 ~ 1.0)
    pub mailbox_backlog: Option<f32>,

    // New field: dependency health state (protobuf enum, stored as i32)
    /// Worst dependency health state (aggregated across multiple dependencies, worst-case first strategy)
    /// protobuf enum ServiceDependencyState: HEALTHY=0, WARNING=1, BROKEN=2
    pub worst_dependency_health_state: Option<i32>,

    // New field: complex metrics for load-balancing sort
    /// Geographic location (region + lat/long)
    pub geo_location: Option<ServiceLocation>,
    /// Whether the client fingerprint matches exactly (marked by LoadBalancer)
    #[serde(skip)]
    pub is_exact_match: bool,
    /// Sticky client ID list (session affinity, obtained from Ping messages)
    pub sticky_client_ids: Vec<String>,

    // WebSocket direct connection address
    /// The WebSocket server address currently open for this service (e.g. "ws://192.168.1.10:9100").
    /// Comes from the `RegisterRequest::ws_address` field at registration time.
    /// `None` means this service does not support WebSocket direct connection.
    #[serde(default)]
    pub ws_address: Option<String>,

    /// Whether this entry was restored from SQLite persistence (loaded via restore_from_storage on actrix restart).
    ///
    /// When marked as true, this entry is a "preliminary candidate": it waits for the service process
    /// to re-establish a WebSocket connection and call `register_service_full()` to complete confirmation.
    /// If a new active registration arrives under the same service_name, the preliminary candidate
    /// will be cleared to prevent ghost candidates from affecting routing decisions.
    #[serde(default)]
    pub is_restored_from_storage: bool,
}

/// Service geographic location
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceLocation {
    /// Geographic region (e.g. "us-west", "cn-beijing")
    pub region: String,
    /// Longitude (optional)
    pub longitude: Option<f64>,
    /// Latitude (optional)
    pub latitude: Option<f64>,
}

/// Service requirement description
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceRequirements {
    /// Minimum version requirement
    pub min_version: Option<String>,
    /// Preferred regions
    pub preferred_regions: Option<Vec<String>>,
    /// Required tags
    pub required_tags: Option<HashMap<String, String>>,
}

/// Service performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceMetrics {
    /// Current active connection count
    pub active_connections: u32,
    /// Average response time (milliseconds)
    pub avg_response_time_ms: f64,
    /// Error rate
    pub error_rate: f64,
}

/// Service registry
#[derive(Debug, Default)]
pub struct ServiceRegistry {
    /// Service map: service_name -> list of service instances
    services: HashMap<String, Vec<ServiceInfo>>,
    /// Message type index: message_type -> list of service names
    message_type_index: HashMap<String, Vec<String>>,
    /// Actor ID index: actor_id -> list of services
    actor_index: HashMap<ActrId, Vec<String>>,
    /// SQLite persistence cache (optional)
    storage: Option<Arc<ServiceRegistryStorage>>,
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Rebuild the secondary indexes for one service name after replacing instances.
    fn rebuild_indexes_for_service(&mut self, service_name: &str) {
        let mut actor_ids = Vec::new();
        let mut message_types = Vec::new();

        if let Some(services) = self.services.get(service_name) {
            for service in services {
                if !actor_ids.contains(&service.actor_id) {
                    actor_ids.push(service.actor_id.clone());
                }
                for message_type in &service.message_types {
                    if !message_types.contains(message_type) {
                        message_types.push(message_type.clone());
                    }
                }
            }
        }

        self.message_type_index.retain(|_, service_names| {
            service_names.retain(|name| name != service_name);
            !service_names.is_empty()
        });
        for message_type in message_types {
            self.message_type_index
                .entry(message_type)
                .or_default()
                .push(service_name.to_string());
        }

        self.actor_index.retain(|_, service_names| {
            service_names.retain(|name| name != service_name);
            !service_names.is_empty()
        });
        for actor_id in actor_ids {
            self.actor_index
                .entry(actor_id)
                .or_default()
                .push(service_name.to_string());
        }
    }

    /// Set persistence storage (called at startup)
    pub fn set_storage(&mut self, storage: Arc<ServiceRegistryStorage>) {
        platform::recording::info!("ServiceRegistry enabling SQLite persistence cache");
        self.storage = Some(storage);
    }

    /// Restore service list from storage (called at startup)
    pub async fn restore_from_storage(&mut self) -> Result<usize, String> {
        let storage = match &self.storage {
            Some(s) => s,
            None => {
                platform::recording::warn!("No storage configured, skipping recovery");
                return Ok(0);
            }
        };

        match storage.load_all_services().await {
            Ok(services) => {
                let count = services.len();
                platform::recording::info!("Restored {} services from cache", count);

                // Load services into memory, mark as storage-restored (awaiting WebSocket reconnect confirmation)
                for mut service in services {
                    service.is_restored_from_storage = true;
                    let actor_id = service.actor_id.clone();
                    let service_name = service.service_name.clone();
                    let message_types = service.message_types.clone();

                    // Add to service map
                    self.services
                        .entry(service_name.clone())
                        .or_default()
                        .push(service);

                    // Update message type index
                    for message_type in &message_types {
                        self.message_type_index
                            .entry(message_type.clone())
                            .or_default()
                            .push(service_name.clone());
                    }

                    // Update Actor index
                    self.actor_index
                        .entry(actor_id)
                        .or_default()
                        .push(service_name);
                }

                Ok(count)
            }
            Err(e) => {
                platform::recording::error!("Failed to restore services from cache: {}", e);
                Err(format!("Recovery failed: {e}"))
            }
        }
    }

    /// Register service (full version, supports ServiceSpec and ACL)
    #[allow(clippy::too_many_arguments)]
    pub fn register_service_full(
        &mut self,
        actor_id: ActrId,
        service_name: String,
        message_types: Vec<String>,
        capabilities: Option<ServiceCapabilities>,
        service_spec: Option<actr_protocol::ServiceSpec>,
        acl: Option<actr_protocol::Acl>,
        ws_address: Option<String>,
    ) -> Result<(), String> {
        platform::recording::info!(
            "Register service: {} (Actor {}), has_spec={}, has_acl={}, ws_address={:?}",
            service_name,
            actor_id.to_string_repr(),
            service_spec.is_some(),
            acl.is_some(),
            ws_address
        );

        let service_info = ServiceInfo {
            actor_id: actor_id.clone(),
            service_name: service_name.clone(),
            message_types: message_types.clone(),
            capabilities,
            status: ServiceStatus::Available,
            last_heartbeat_time_secs: current_timestamp(),
            service_spec,
            acl,
            service_availability_state: None,
            power_reserve: None,
            mailbox_backlog: None,
            worst_dependency_health_state: None,
            geo_location: None,
            is_exact_match: false,
            sticky_client_ids: Vec::new(),
            ws_address,
            is_restored_from_storage: false,
        };

        // Async write to SQLite cache (background task, non-blocking)
        if let Some(storage) = self.storage.clone() {
            let service_to_save = service_info.clone();
            tokio::spawn(async move {
                if let Err(e) = storage.save_service(&service_to_save).await {
                    platform::recording::error!("Failed to save service to cache: {}", e);
                }
            });
        }

        // Add to service map. Clear storage-restored ghost candidates and replace the same Actor's
        // existing registration for the same service. Other active Actor instances are retained
        // to support horizontal scaling.
        let entry = self.services.entry(service_name.clone()).or_default();
        let ghost_actor_ids: Vec<ActrId> = entry
            .iter()
            .filter(|s| s.is_restored_from_storage)
            .map(|s| s.actor_id.clone())
            .collect();
        entry.retain(|s| !s.is_restored_from_storage && s.actor_id != actor_id);
        entry.push(service_info);

        for ghost_id in &ghost_actor_ids {
            platform::recording::info!(
                "Cleared storage-restored ghost candidate: service={}, actor={}",
                service_name,
                ghost_id.to_string_repr()
            );
        }

        self.rebuild_indexes_for_service(&service_name);

        Ok(())
    }

    /// Register service (simplified version, backward compatible)
    pub fn register_service(
        &mut self,
        actor_id: ActrId,
        service_name: String,
        message_types: Vec<String>,
        capabilities: Option<ServiceCapabilities>,
    ) -> Result<(), String> {
        self.register_service_full(
            actor_id,
            service_name,
            message_types,
            capabilities,
            None,
            None,
            None,
        )
    }

    /// Update service load metrics (from Ping messages)
    pub fn update_load_metrics(
        &mut self,
        actor_id: &ActrId,
        service_availability_state: i32, // ServiceAvailabilityState as i32
        power_reserve: f32,
        mailbox_backlog: f32,
    ) -> Result<(), String> {
        platform::recording::debug!(
            "Update Actor {} load metrics: service_availability_state={}, power={:.2}, backlog={:.2}",
            actor_id.to_string_repr(),
            service_availability_state,
            power_reserve,
            mailbox_backlog
        );

        // Find all services for this Actor
        if let Some(service_names) = self.actor_index.get(actor_id) {
            for service_name in service_names {
                if let Some(services) = self.services.get_mut(service_name) {
                    for service in services {
                        if service.actor_id == *actor_id {
                            service.service_availability_state = Some(service_availability_state);
                            service.power_reserve = Some(power_reserve);
                            service.mailbox_backlog = Some(mailbox_backlog);
                            service.last_heartbeat_time_secs = current_timestamp();
                            platform::recording::debug!(
                                "Load metrics updated successfully: {}",
                                service_name
                            );

                            // Async update heartbeat time in SQLite cache (background task, non-blocking)
                            if let Some(storage) = self.storage.clone() {
                                let actor_id_clone = actor_id.clone();
                                let service_name_clone = service_name.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = storage
                                        .update_heartbeat(&actor_id_clone, &service_name_clone)
                                        .await
                                    {
                                        platform::recording::error!(
                                            "Failed to update cache heartbeat: {}",
                                            e
                                        );
                                    }
                                });
                            }
                        }
                    }
                }
            }
            Ok(())
        } else {
            Err(format!(
                "No service registration found for Actor {}",
                actor_id.serial_number
            ))
        }
    }

    /// Get service's ServiceSpec (for compatibility checking)
    pub fn get_service_spec(&self, actor_id: &ActrId) -> Option<&actr_protocol::ServiceSpec> {
        // Find service name from actor_index, then find the instance from services
        self.actor_index.get(actor_id).and_then(|service_names| {
            service_names.first().and_then(|service_name| {
                self.services.get(service_name).and_then(|services| {
                    services
                        .iter()
                        .find(|s| &s.actor_id == actor_id)
                        .and_then(|s| s.service_spec.as_ref())
                })
            })
        })
    }

    /// Get service's ACL (for access control)
    pub fn get_acl(&self, actor_id: &ActrId) -> Option<&actr_protocol::Acl> {
        self.actor_index.get(actor_id).and_then(|service_names| {
            service_names.first().and_then(|service_name| {
                self.services.get(service_name).and_then(|services| {
                    services
                        .iter()
                        .find(|s| &s.actor_id == actor_id)
                        .and_then(|s| s.acl.as_ref())
                })
            })
        })
    }

    /// Get ServiceRegistryStorage reference (for querying proto specs during compatibility negotiation)
    pub fn get_storage(&self) -> Option<Arc<ServiceRegistryStorage>> {
        self.storage.clone()
    }

    /// Discover services by message type
    pub fn discover_by_message_type(&self, message_type: &str) -> Vec<&ServiceInfo> {
        platform::recording::debug!("Discovering services by message type: {}", message_type);

        if let Some(service_names) = self.message_type_index.get(message_type) {
            let mut services = Vec::new();
            for service_name in service_names {
                if let Some(service_instances) = self.services.get(service_name) {
                    // Only return available service instances
                    services.extend(
                        service_instances
                            .iter()
                            .filter(|s| s.status == ServiceStatus::Available),
                    );
                }
            }
            services
        } else {
            platform::recording::debug!(
                "No services found supporting message type {}",
                message_type
            );
            Vec::new()
        }
    }

    /// Discover services by service name
    pub fn discover_by_service_name(&self, service_name: &str) -> Vec<&ServiceInfo> {
        platform::recording::debug!("Discovering services by name: {}", service_name);

        if let Some(services) = self.services.get(service_name) {
            services
                .iter()
                .filter(|s| s.status == ServiceStatus::Available)
                .collect()
        } else {
            platform::recording::debug!("Service not found: {}", service_name);
            Vec::new()
        }
    }

    /// Discover services by ServiceSpec name (search across all service types)
    pub fn discover_by_spec_name(&self, spec_name: &str) -> Vec<&ServiceInfo> {
        self.services
            .values()
            .flatten()
            .filter(|s| {
                s.status == ServiceStatus::Available
                    && s.service_spec
                        .as_ref()
                        .is_some_and(|spec| spec.name == spec_name)
            })
            .collect()
    }

    /// Discover services by requirements
    pub fn discover_by_requirements(
        &self,
        requirements: &ServiceRequirements,
    ) -> Vec<&ServiceInfo> {
        platform::recording::debug!("Discovering services by requirements: {:?}", requirements);

        let mut matching_services = Vec::new();

        for services in self.services.values() {
            for service in services {
                if service.status != ServiceStatus::Available {
                    continue;
                }

                // Check version requirements
                if let (Some(min_version), Some(capabilities)) =
                    (&requirements.min_version, &service.capabilities)
                    && let Some(version_range) = &capabilities.version_range
                {
                    // Simple version comparison; should use semantic versioning in practice
                    if version_range < min_version {
                        continue;
                    }
                }

                // Check region preference
                if let (Some(preferred_regions), Some(capabilities)) =
                    (&requirements.preferred_regions, &service.capabilities)
                    && let Some(region) = &capabilities.region
                    && !preferred_regions.contains(region)
                {
                    continue;
                }

                // Check required tags
                if let (Some(required_tags), Some(capabilities)) =
                    (&requirements.required_tags, &service.capabilities)
                {
                    if let Some(service_tags) = &capabilities.tags {
                        let mut all_tags_match = true;
                        for (key, value) in required_tags {
                            if service_tags.get(key) != Some(value) {
                                all_tags_match = false;
                                break;
                            }
                        }
                        if !all_tags_match {
                            continue;
                        }
                    } else {
                        // Service has no tags but tags are required
                        continue;
                    }
                }

                matching_services.push(service);
            }
        }

        matching_services
    }

    /// Update service status
    pub fn update_service_status(
        &mut self,
        actor_id: &ActrId,
        service_name: &str,
        status: ServiceStatus,
        metrics: Option<ServiceMetrics>,
    ) -> Result<(), String> {
        platform::recording::debug!(
            "Update service status: {} (Actor {}) -> {:?}",
            service_name,
            actor_id.to_string_repr(),
            status
        );

        if let Some(services) = self.services.get_mut(service_name) {
            for service in services {
                if service.actor_id == *actor_id {
                    service.status = status;
                    service.last_heartbeat_time_secs = current_timestamp();

                    if let Some(_metrics) = metrics {
                        // Performance metrics could be stored here; ignored for now
                        platform::recording::debug!("Received service performance metrics data");
                    }

                    return Ok(());
                }
            }
        }

        Err(format!(
            "Service instance not found: {} (Actor {})",
            service_name, actor_id.serial_number
        ))
    }

    /// Unregister service
    pub fn unregister_service(
        &mut self,
        actor_id: &ActrId,
        service_name: &str,
    ) -> Result<(), String> {
        platform::recording::info!(
            "Unregister service: {} (Actor {})",
            service_name,
            actor_id.to_string_repr()
        );

        // Remove from service map
        if let Some(services) = self.services.get_mut(service_name) {
            let original_len = services.len();
            services.retain(|s| s.actor_id != *actor_id);

            if services.len() == original_len {
                return Err(format!(
                    "Service instance to unregister not found: {} (Actor {})",
                    service_name, actor_id.serial_number
                ));
            }

            // If this is the last instance, clean up the message type index
            if services.is_empty() {
                self.services.remove(service_name);

                // Remove from message type index
                self.message_type_index.retain(|_, service_names| {
                    service_names.retain(|name| name != service_name);
                    !service_names.is_empty()
                });
            }
        }

        // Remove from Actor index
        if let Some(actor_services) = self.actor_index.get_mut(actor_id) {
            actor_services.retain(|name| name != service_name);

            if actor_services.is_empty() {
                self.actor_index.remove(actor_id);
            }
        }

        // Async delete from SQLite cache (background task, non-blocking)
        if let Some(storage) = self.storage.clone() {
            let actor_id_clone = actor_id.clone();
            let service_name_owned = service_name.to_string();
            tokio::spawn(async move {
                if let Err(e) = storage
                    .delete_service(&actor_id_clone, &service_name_owned)
                    .await
                {
                    platform::recording::error!("Failed to delete service from cache: {}", e);
                }
            });
        }

        Ok(())
    }

    /// Unregister all services for an Actor
    pub fn unregister_actor(&mut self, actor_id: &ActrId) {
        platform::recording::info!(
            "Unregistering all services for Actor {}",
            actor_id.to_string_repr()
        );

        if let Some(service_names) = self.actor_index.remove(actor_id) {
            for service_name in &service_names {
                let _ = self.unregister_service(actor_id, service_name);
            }
        }
    }

    /// Unregister all services for an Actor from memory only, without deleting the database cache.
    ///
    /// Used for transient offline scenarios such as WebSocket disconnection. Database data is
    /// preserved so the same Actor can restore its service registration via heartbeat after
    /// reconnecting.
    pub fn unregister_actor_memory_only(&mut self, actor_id: &ActrId) {
        platform::recording::info!(
            "Unregistering all services for Actor {} (memory only)",
            actor_id.to_string_repr()
        );

        if let Some(service_names) = self.actor_index.get(actor_id).cloned() {
            for service_name in &service_names {
                let _ = self.unregister_service_memory_only(actor_id, service_name);
            }
        }
    }

    /// Clean up expired services (not updated within the specified time)
    ///
    /// Note: This method only cleans up in-memory services; it does not delete database data.
    /// This allows services to be restored from the database via heartbeat after a 5-60 minute
    /// network outage. Database cleanup is handled by a separate scheduled task (TTL = 1 hour).
    pub fn cleanup_expired_services(&mut self) {
        let current_time = current_timestamp();
        let expiry_threshold = SERVICE_EXPIRY_THRESHOLD_SECS;

        let mut services_to_remove = Vec::new();

        for (service_name, services) in &self.services {
            for service in services {
                if current_time - service.last_heartbeat_time_secs > expiry_threshold {
                    services_to_remove.push((service.actor_id.clone(), service_name.clone()));
                }
            }
        }

        for (actor_id, service_name) in services_to_remove {
            platform::recording::warn!(
                "Cleaning expired service from memory: {} (Actor {}) [database retained for recovery]",
                service_name,
                actor_id.to_string_repr()
            );
            // Only clean up memory; do not delete from database
            let _ = self.unregister_service_memory_only(&actor_id, &service_name);
        }
    }

    /// Unregister service from memory only, without deleting database data
    ///
    /// Used for expired service cleanup; preserves database data for later recovery.
    fn unregister_service_memory_only(
        &mut self,
        actor_id: &ActrId,
        service_name: &str,
    ) -> Result<(), String> {
        // Remove from service map
        if let Some(services) = self.services.get_mut(service_name) {
            let original_len = services.len();
            services.retain(|s| s.actor_id != *actor_id);

            if services.len() == original_len {
                return Err(format!(
                    "Service instance to unregister not found: {} (Actor {})",
                    service_name, actor_id.serial_number
                ));
            }

            // If this is the last instance, clean up the message type index
            if services.is_empty() {
                self.services.remove(service_name);

                // Remove from message type index
                self.message_type_index.retain(|_, service_names| {
                    service_names.retain(|name| name != service_name);
                    !service_names.is_empty()
                });
            }
        }

        // Remove from Actor index
        if let Some(actor_services) = self.actor_index.get_mut(actor_id) {
            actor_services.retain(|name| name != service_name);

            if actor_services.is_empty() {
                self.actor_index.remove(actor_id);
            }
        }

        // Note: do not delete database data; preserve for later recovery

        Ok(())
    }

    /// Restore service from database (used during heartbeat recovery)
    ///
    /// When a heartbeat is received but the service is not found in memory, attempt to
    /// restore from the database. This typically occurs when the network has been down
    /// for more than 5 minutes (memory cleanup threshold) but less than 1 hour (database TTL).
    ///
    /// # Arguments
    ///
    /// * `actor_id` - The Actor ID to restore
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - Successfully restored at least one service from the database
    /// * `Ok(false)` - No services found in the database for this Actor (possibly expired or never registered)
    /// * `Err(String)` - Error during the recovery process
    pub async fn restore_service_from_storage(
        &mut self,
        actor_id: &ActrId,
    ) -> Result<bool, String> {
        // Check if a storage backend is available
        let storage = match &self.storage {
            Some(s) => s,
            None => {
                platform::recording::debug!("No storage backend available for service recovery");
                return Ok(false);
            }
        };

        // Load this Actor's services from the database
        let services = storage
            .load_services_by_actor_id(actor_id)
            .await
            .map_err(|e| format!("Failed to load services from storage: {}", e))?;

        if services.is_empty() {
            platform::recording::debug!(
                "No services found in storage for Actor {}",
                actor_id.to_string_repr()
            );
            return Ok(false);
        }

        platform::recording::info!(
            "🔄 Restoring {} service(s) from storage for Actor {}",
            services.len(),
            actor_id.to_string_repr()
        );

        let mut restored_count = 0;

        // Re-register each service into memory
        for mut service in services {
            if service.actor_id != *actor_id {
                platform::recording::warn!(
                    "Skipping mismatched cached service for Actor {}: cached Actor {}",
                    actor_id.to_string_repr(),
                    service.actor_id.to_string_repr()
                );
                continue;
            }

            service.is_restored_from_storage = true;

            let service_name = service.service_name.clone();
            let service_entries = self.services.entry(service_name.clone()).or_default();
            let inserted = if service_entries
                .iter()
                .any(|entry| entry.actor_id == service.actor_id)
            {
                false
            } else {
                service_entries.push(service.clone());
                true
            };

            // The message type index stores service_name; ensure each mapping appears only once.
            let mut message_index_updated = false;
            for message_type in &service.message_types {
                let service_names = self
                    .message_type_index
                    .entry(message_type.clone())
                    .or_default();
                if !service_names.contains(&service_name) {
                    service_names.push(service_name.clone());
                    message_index_updated = true;
                }
            }

            // Update Actor index
            let actor_services = self
                .actor_index
                .entry(service.actor_id.clone())
                .or_default();
            let actor_index_updated = if actor_services.contains(&service_name) {
                false
            } else {
                actor_services.push(service_name.clone());
                true
            };

            if !inserted && !message_index_updated && !actor_index_updated {
                platform::recording::debug!(
                    "Service already present in memory for Actor {}: {}",
                    service.actor_id.to_string_repr(),
                    service_name
                );
                continue;
            }

            restored_count += 1;

            platform::recording::info!(
                "  ✅ Restored service: {} (Actor {})",
                service_name,
                service.actor_id.to_string_repr()
            );
        }

        Ok(restored_count > 0)
    }

    /// Get all service statistics
    pub fn get_service_stats(&self) -> HashMap<String, usize> {
        self.services
            .iter()
            .map(|(name, instances)| {
                let available_count = instances
                    .iter()
                    .filter(|s| s.status == ServiceStatus::Available)
                    .count();
                (name.clone(), available_count)
            })
            .collect()
    }

    /// Get message type mapping statistics
    pub fn get_message_type_stats(&self) -> HashMap<String, usize> {
        self.message_type_index
            .iter()
            .map(|(msg_type, services)| (msg_type.clone(), services.len()))
            .collect()
    }

    /// Get all services (for service discovery)
    ///
    /// # Arguments
    /// - `manufacturer`: Optional manufacturer filter
    ///
    /// # Returns
    /// List of all matching service instances
    pub fn discover_all(&self, manufacturer: Option<&str>) -> Vec<&ServiceInfo> {
        let mut results = Vec::new();

        for services in self.services.values() {
            for service in services {
                // Only return available services
                if service.status != ServiceStatus::Available {
                    continue;
                }

                // Filter by manufacturer
                if let Some(mfr) = manufacturer
                    && service.actor_id.r#type.manufacturer != mfr
                {
                    continue;
                }

                results.push(service);
            }
        }

        results
    }

    /// Find service instances by ActrType (for load-balancing routing)
    ///
    /// # Arguments
    /// - `target_type`: Target Actor type
    ///
    /// # Returns
    /// All available service instances matching the type (cloned)
    pub fn find_by_actr_type(&self, target_type: &ActrType) -> Vec<ServiceInfo> {
        let mut results = Vec::new();

        for services in self.services.values() {
            for service in services {
                // Only return available services
                if service.status != ServiceStatus::Available {
                    continue;
                }

                // Exact match ActrType (manufacturer + name + version)
                if service.actor_id.r#type.manufacturer == target_type.manufacturer
                    && service.actor_id.r#type.name == target_type.name
                    && service.actor_id.r#type.version == target_type.version
                {
                    results.push(service.clone());
                }
            }
        }

        results
    }

    /// Discover services by ActrType with ACL filtering
    ///
    /// Returns only services that the requester is allowed to discover
    /// based on ACL rules
    ///
    /// # Arguments
    ///
    /// - `requester_id`: Actor requesting discovery
    /// - `target_type`: Target service type
    ///
    /// # Returns
    ///
    /// List of ServiceInfo that match the service type and pass ACL check
    pub async fn discover_with_acl(
        &self,
        requester_id: &ActrId,
        target_type: &ActrType,
    ) -> Result<Vec<ServiceInfo>, RealmError> {
        let all_services = self.find_by_actr_type(target_type);
        let total_count = all_services.len(); // Save count before moving

        let mut allowed_services = Vec::new();

        for service in all_services {
            // Skip self
            if &service.actor_id == requester_id {
                continue;
            }

            // ACL check: can requester discover this service?
            let can_discover = Self::check_discovery_acl(requester_id, &service.actor_id).await?;

            if can_discover {
                allowed_services.push(service);
            } else {
                platform::recording::debug!(
                    "ACL denied service discovery: requester={}, service={}",
                    requester_id.to_string_repr(),
                    service.actor_id.to_string_repr()
                );
            }
        }

        platform::recording::info!(
            "Service discovery completed with ACL filtering: requester={}, target_type={:?}, total_services={}, allowed_services={}",
            requester_id.to_string_repr(),
            target_type,
            total_count,
            allowed_services.len()
        );

        Ok(allowed_services)
    }

    /// Check if discovery is allowed between two actors based on ACL rules
    ///
    /// # Arguments
    ///
    /// - `from_actor`: Actor requesting discovery
    /// - `to_actor`: Target actor
    ///
    /// # Returns
    ///
    /// Returns true if discovery is allowed based on ACL rules
    async fn check_discovery_acl(
        from_actor: &ActrId,
        to_actor: &ActrId,
    ) -> Result<bool, RealmError> {
        // Extract realm and actor types
        let from_realm = from_actor.realm.realm_id;
        let to_realm = to_actor.realm.realm_id;

        let from_type = format!(
            "{}:{}:{}",
            from_actor.r#type.manufacturer, from_actor.r#type.name, from_actor.r#type.version
        );
        let to_type = format!(
            "{}:{}:{}",
            to_actor.r#type.manufacturer, to_actor.r#type.name, to_actor.r#type.version
        );

        ActorAcl::can_discover(from_realm, to_realm, &from_type, &to_type).await
    }

    /// Simulate restore_from_storage scenario: mark all entries under the given service_name as storage-restored.
    ///
    /// Test-only helper for verifying ghost candidate cleanup logic.
    #[cfg(test)]
    pub fn mark_all_as_restored(&mut self, service_name: &str) {
        if let Some(entries) = self.services.get_mut(service_name) {
            for entry in entries.iter_mut() {
                entry.is_restored_from_storage = true;
            }
        }
    }
}

/// Get current timestamp
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use actr_protocol::ActrType;
    use platform::realm::Realm as RealmEntity;
    use platform::realm::acl::ActorAcl;
    use platform::storage::db::set_db_path;
    use serial_test::serial;
    use std::path::Path;
    use tokio::sync::OnceCell;

    fn create_test_actor_id(serial: u64) -> ActrId {
        ActrId {
            serial_number: serial,
            r#type: ActrType {
                manufacturer: "test".to_string(),
                name: "test".to_string(),
                version: "1.0.0".to_string(),
            },
            realm: actr_protocol::Realm { realm_id: 0 },
        }
    }

    fn create_actor_id(serial: u64, realm_id: u32, manufacturer: &str, name: &str) -> ActrId {
        ActrId {
            serial_number: serial,
            r#type: ActrType {
                manufacturer: manufacturer.to_string(),
                name: name.to_string(),
                version: "1.0.0".to_string(),
            },
            realm: actr_protocol::Realm { realm_id },
        }
    }

    async fn setup_test_db() -> anyhow::Result<()> {
        static INIT: OnceCell<()> = OnceCell::const_new();

        INIT.get_or_init(|| async {
            // nextest runs each test in its own process, so a fixed path would be
            // concurrently deleted and rewritten; suffix with the PID to isolate
            // each process's test database.
            let db_dir = std::env::temp_dir().join(format!(
                "actrix_signaling_registry_test_db_{}",
                std::process::id()
            ));
            std::fs::create_dir_all(&db_dir).expect("Failed to create signaling test db dir");

            let db_file = db_dir.join("actrix.db");
            if db_file.exists() {
                let _ = std::fs::remove_file(&db_file);
            }

            let db_dir_str = db_dir
                .to_str()
                .expect("Failed to convert signaling test db path to string");

            match set_db_path(Path::new(db_dir_str)).await {
                Ok(()) => {}
                Err(e) => {
                    let err_msg = e.to_string();
                    if !err_msg.contains("already initialized")
                        && !err_msg.contains("Database already")
                    {
                        panic!("Failed to initialize signaling test database: {e}");
                    }
                }
            }
        })
        .await;

        Ok(())
    }

    #[test]
    fn test_service_registration() {
        let mut registry = ServiceRegistry::new();
        let actor_id = create_test_actor_id(1);

        let result = registry.register_service(
            actor_id.clone(),
            "test_service".to_string(),
            vec!["TestMessage".to_string()],
            None,
        );

        assert!(result.is_ok());

        let services = registry.discover_by_message_type("TestMessage");
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].actor_id, actor_id);
    }

    #[test]
    fn test_service_discovery() {
        let mut registry = ServiceRegistry::new();
        let actor_id1 = create_test_actor_id(1);
        let actor_id2 = create_test_actor_id(2);

        registry
            .register_service(
                actor_id1,
                "service1".to_string(),
                vec!["Message1".to_string()],
                None,
            )
            .unwrap();

        registry
            .register_service(
                actor_id2,
                "service2".to_string(),
                vec!["Message1".to_string(), "Message2".to_string()],
                None,
            )
            .unwrap();

        let services = registry.discover_by_message_type("Message1");
        assert_eq!(services.len(), 2);

        let services = registry.discover_by_message_type("Message2");
        assert_eq!(services.len(), 1);

        let services = registry.discover_by_message_type("Message3");
        assert_eq!(services.len(), 0);
    }

    #[test]
    fn test_service_unregister() {
        let mut registry = ServiceRegistry::new();
        let actor_id = create_test_actor_id(1);

        registry
            .register_service(
                actor_id.clone(),
                "test_service".to_string(),
                vec!["TestMessage".to_string()],
                None,
            )
            .unwrap();

        // Verify service is registered
        let services = registry.discover_by_message_type("TestMessage");
        assert_eq!(services.len(), 1);

        // Unregister service
        let result = registry.unregister_service(&actor_id, "test_service");
        assert!(result.is_ok());

        // Verify service has been removed
        let services = registry.discover_by_message_type("TestMessage");
        assert_eq!(services.len(), 0);
    }

    #[test]
    fn test_unregister_nonexistent_service() {
        let mut registry = ServiceRegistry::new();
        let actor_id = create_test_actor_id(1);

        // Note: in the current implementation, unregistering a nonexistent service name returns Ok(())
        // Only when the service name exists but the actor_id doesn't match does it return Err
        let result = registry.unregister_service(&actor_id, "nonexistent");
        assert!(result.is_ok());

        // Test case: service name exists but actor_id doesn't match
        let actor_id1 = create_test_actor_id(1);
        let actor_id2 = create_test_actor_id(2);

        registry
            .register_service(
                actor_id1.clone(),
                "test_service".to_string(),
                vec!["Test".to_string()],
                None,
            )
            .unwrap();

        // Attempt to unregister with wrong actor_id
        let result = registry.unregister_service(&actor_id2, "test_service");
        assert!(result.is_err()); // Only this case returns an error
    }

    #[test]
    fn test_service_status_update() {
        let mut registry = ServiceRegistry::new();
        let actor_id = create_test_actor_id(1);

        registry
            .register_service(
                actor_id.clone(),
                "test_service".to_string(),
                vec!["TestMessage".to_string()],
                None,
            )
            .unwrap();

        // Update service status to Busy
        let result =
            registry.update_service_status(&actor_id, "test_service", ServiceStatus::Busy, None);
        assert!(result.is_ok());

        // Verify status updated (Busy services should not appear in discovery results)
        let services = registry.discover_by_message_type("TestMessage");
        assert_eq!(
            services.len(),
            0,
            "Services in Busy state should not be discovered"
        );

        // Update back to Available
        registry
            .update_service_status(&actor_id, "test_service", ServiceStatus::Available, None)
            .unwrap();
        let services = registry.discover_by_message_type("TestMessage");
        assert_eq!(services.len(), 1);
    }

    #[test]
    fn test_load_metrics_update() {
        let mut registry = ServiceRegistry::new();
        let actor_id = create_test_actor_id(1);

        registry
            .register_service(
                actor_id.clone(),
                "test_service".to_string(),
                vec!["TestMessage".to_string()],
                None,
            )
            .unwrap();

        // Update load metrics
        let result = registry.update_load_metrics(&actor_id, 0, 0.8, 0.3);
        assert!(result.is_ok());

        // Verify metrics have been updated
        let services = registry.discover_by_service_name("test_service");
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].service_availability_state, Some(0));
        assert_eq!(services[0].power_reserve, Some(0.8));
        assert_eq!(services[0].mailbox_backlog, Some(0.3));
    }

    #[test]
    fn test_discover_by_service_name() {
        let mut registry = ServiceRegistry::new();
        let actor_id1 = create_test_actor_id(1);
        let actor_id2 = create_test_actor_id(2);

        // Register two instances with the same service name
        registry
            .register_service(
                actor_id1,
                "api_service".to_string(),
                vec!["ApiMessage".to_string()],
                None,
            )
            .unwrap();

        registry
            .register_service(
                actor_id2,
                "api_service".to_string(),
                vec!["ApiMessage".to_string()],
                None,
            )
            .unwrap();

        let services = registry.discover_by_service_name("api_service");
        assert_eq!(services.len(), 2);

        let services = registry.discover_by_service_name("nonexistent");
        assert_eq!(services.len(), 0);
    }

    #[test]
    fn test_unregister_actor() {
        let mut registry = ServiceRegistry::new();
        let actor_id = create_test_actor_id(1);

        // Register multiple services
        registry
            .register_service(
                actor_id.clone(),
                "service1".to_string(),
                vec!["Message1".to_string()],
                None,
            )
            .unwrap();

        registry
            .register_service(
                actor_id.clone(),
                "service2".to_string(),
                vec!["Message2".to_string()],
                None,
            )
            .unwrap();

        // Verify service is registered
        assert_eq!(registry.discover_by_message_type("Message1").len(), 1);
        assert_eq!(registry.discover_by_message_type("Message2").len(), 1);

        // Unregister all services for the Actor
        registry.unregister_actor(&actor_id);

        // Verify all services have been removed
        assert_eq!(registry.discover_by_message_type("Message1").len(), 0);
        assert_eq!(registry.discover_by_message_type("Message2").len(), 0);
    }

    #[test]
    fn test_service_with_capabilities() {
        let mut registry = ServiceRegistry::new();
        let actor_id = create_test_actor_id(1);

        let mut tags = HashMap::new();
        tags.insert("region".to_string(), "us-west".to_string());

        let capabilities = ServiceCapabilities {
            max_concurrent_requests: Some(100),
            version_range: Some("1.0.0".to_string()),
            region: Some("us-west".to_string()),
            tags: Some(tags),
        };

        registry
            .register_service(
                actor_id,
                "test_service".to_string(),
                vec!["TestMessage".to_string()],
                Some(capabilities),
            )
            .unwrap();

        let services = registry.discover_by_message_type("TestMessage");
        assert_eq!(services.len(), 1);
        assert!(services[0].capabilities.is_some());
        assert_eq!(
            services[0].capabilities.as_ref().unwrap().region,
            Some("us-west".to_string())
        );
    }

    #[test]
    fn test_discover_by_requirements() {
        let mut registry = ServiceRegistry::new();
        let actor_id1 = create_test_actor_id(1);
        let actor_id2 = create_test_actor_id(2);

        // Register services with region tags
        let mut tags_us = HashMap::new();
        tags_us.insert("env".to_string(), "prod".to_string());

        registry
            .register_service(
                actor_id1,
                "service_us".to_string(),
                vec!["Message1".to_string()],
                Some(ServiceCapabilities {
                    max_concurrent_requests: None,
                    version_range: Some("2.0.0".to_string()),
                    region: Some("us-west".to_string()),
                    tags: Some(tags_us.clone()),
                }),
            )
            .unwrap();

        let mut tags_eu = HashMap::new();
        tags_eu.insert("env".to_string(), "dev".to_string());

        registry
            .register_service(
                actor_id2,
                "service_eu".to_string(),
                vec!["Message1".to_string()],
                Some(ServiceCapabilities {
                    max_concurrent_requests: None,
                    version_range: Some("1.0.0".to_string()),
                    region: Some("eu-west".to_string()),
                    tags: Some(tags_eu),
                }),
            )
            .unwrap();

        // Query by region
        let requirements = ServiceRequirements {
            min_version: None,
            preferred_regions: Some(vec!["us-west".to_string()]),
            required_tags: None,
        };

        let services = registry.discover_by_requirements(&requirements);
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].service_name, "service_us");

        // Query by tags
        let requirements = ServiceRequirements {
            min_version: None,
            preferred_regions: None,
            required_tags: Some(tags_us),
        };

        let services = registry.discover_by_requirements(&requirements);
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].service_name, "service_us");
    }

    #[test]
    fn test_service_stats() {
        let mut registry = ServiceRegistry::new();
        let actor_id1 = create_test_actor_id(1);
        let actor_id2 = create_test_actor_id(2);

        // Two different service names, both support Message1
        registry
            .register_service(
                actor_id1,
                "service1".to_string(),
                vec!["Message1".to_string()],
                None,
            )
            .unwrap();

        registry
            .register_service(
                actor_id2,
                "service2".to_string(),
                vec!["Message1".to_string()],
                None,
            )
            .unwrap();

        let stats = registry.get_service_stats();
        assert_eq!(stats.get("service1"), Some(&1)); // service1 has 1 instance
        assert_eq!(stats.get("service2"), Some(&1)); // service2 has 1 instance

        // get_message_type_stats() returns the number of service names per message type
        // Note: the current implementation may add duplicate service names to the index for each registered service instance
        let msg_stats = registry.get_message_type_stats();
        assert_eq!(msg_stats.get("Message1"), Some(&2)); // Message1 is supported by 2 service names
    }

    #[test]
    fn test_register_service_full_with_spec_and_acl() {
        let mut registry = ServiceRegistry::new();
        let actor_id = create_test_actor_id(1);

        let service_spec = actr_protocol::ServiceSpec {
            name: "secure_service".to_string(),
            fingerprint: "sha256:test123".to_string(),
            description: Some("Test service".to_string()),
            protobufs: vec![],
            published_at: None,
            tags: vec![],
        };

        let acl = actr_protocol::Acl { rules: vec![] };

        let result = registry.register_service_full(
            actor_id.clone(),
            "secure_service".to_string(),
            vec!["SecureMessage".to_string()],
            None,
            Some(service_spec.clone()),
            Some(acl),
            None,
        );

        assert!(result.is_ok());

        // Verify ServiceSpec can be retrieved
        let spec = registry.get_service_spec(&actor_id);
        assert!(spec.is_some());
        assert_eq!(spec.unwrap().fingerprint, "sha256:test123");

        // Verify ACL can be retrieved
        let acl = registry.get_acl(&actor_id);
        assert!(acl.is_some());
        assert_eq!(acl.unwrap().rules.len(), 0);
    }

    #[test]
    fn test_find_by_actr_type() {
        let mut registry = ServiceRegistry::new();

        let actor_id1 = ActrId {
            serial_number: 1,
            r#type: ActrType {
                manufacturer: "acme".to_string(),
                name: "worker".to_string(),
                version: "1.0.0".to_string(),
            },
            realm: actr_protocol::Realm { realm_id: 0 },
        };

        let actor_id2 = ActrId {
            serial_number: 2,
            r#type: ActrType {
                manufacturer: "acme".to_string(),
                name: "worker".to_string(),
                version: "1.0.0".to_string(),
            },
            realm: actr_protocol::Realm { realm_id: 0 },
        };

        let actor_id3 = ActrId {
            serial_number: 3,
            r#type: ActrType {
                manufacturer: "other".to_string(),
                name: "service".to_string(),
                version: "1.0.0".to_string(),
            },
            realm: actr_protocol::Realm { realm_id: 0 },
        };

        registry
            .register_service(
                actor_id1,
                "worker1".to_string(),
                vec!["Work".to_string()],
                None,
            )
            .unwrap();

        registry
            .register_service(
                actor_id2,
                "worker2".to_string(),
                vec!["Work".to_string()],
                None,
            )
            .unwrap();

        registry
            .register_service(
                actor_id3,
                "other_service".to_string(),
                vec!["Other".to_string()],
                None,
            )
            .unwrap();

        let target_type = ActrType {
            manufacturer: "acme".to_string(),
            name: "worker".to_string(),
            version: "1.0.0".to_string(),
        };

        let results = registry.find_by_actr_type(&target_type);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(
            |s| s.actor_id.r#type.manufacturer == "acme" && s.actor_id.r#type.name == "worker"
        ));
    }

    #[test]
    fn test_discover_all_with_manufacturer_filter() {
        let mut registry = ServiceRegistry::new();

        let actor_id1 = ActrId {
            serial_number: 1,
            r#type: ActrType {
                manufacturer: "acme".to_string(),
                name: "service1".to_string(),
                version: "1.0.0".to_string(),
            },
            realm: actr_protocol::Realm { realm_id: 0 },
        };

        let actor_id2 = ActrId {
            serial_number: 2,
            r#type: ActrType {
                manufacturer: "vendor".to_string(),
                name: "service2".to_string(),
                version: "1.0.0".to_string(),
            },
            realm: actr_protocol::Realm { realm_id: 0 },
        };

        registry
            .register_service(actor_id1, "s1".to_string(), vec!["M1".to_string()], None)
            .unwrap();

        registry
            .register_service(actor_id2, "s2".to_string(), vec!["M2".to_string()], None)
            .unwrap();

        // No filter
        let all = registry.discover_all(None);
        assert_eq!(all.len(), 2);

        // Filter by manufacturer
        let acme_only = registry.discover_all(Some("acme"));
        assert_eq!(acme_only.len(), 1);
        assert_eq!(acme_only[0].actor_id.r#type.manufacturer, "acme");
    }

    #[tokio::test]
    #[serial]
    async fn test_check_discovery_acl_cross_realm_and_full_type() -> anyhow::Result<()> {
        setup_test_db().await?;

        let source_realm = 4_100_001;
        let target_realm = 4_100_002;
        let other_realm = 4_100_003;

        let mut source_realm_entity = RealmEntity {
            id: source_realm,
            name: "source-realm".to_string(),
            ..Default::default()
        };
        let mut target_realm_entity = RealmEntity {
            id: target_realm,
            name: "target-realm".to_string(),
            ..Default::default()
        };
        let mut other_realm_entity = RealmEntity {
            id: other_realm,
            name: "other-realm".to_string(),
            ..Default::default()
        };
        let _ = source_realm_entity.save().await;
        let _ = target_realm_entity.save().await;
        let _ = other_realm_entity.save().await;

        let _ = ActorAcl::delete_by_target(target_realm, "acme:worker:1.0.0").await?;

        let mut acl = ActorAcl::new_with_source_realm(
            target_realm,
            Some(source_realm),
            "acme:edge:1.0.0".to_string(),
            "acme:worker:1.0.0".to_string(),
            true,
        );
        let _ = acl.save().await?;

        let from_allowed = create_actor_id(1, source_realm, "acme", "edge");
        let to_target = create_actor_id(2, target_realm, "acme", "worker");
        let allowed = ServiceRegistry::check_discovery_acl(&from_allowed, &to_target).await?;
        assert!(allowed);

        let from_wrong_realm = create_actor_id(3, other_realm, "acme", "edge");
        let denied_wrong_realm =
            ServiceRegistry::check_discovery_acl(&from_wrong_realm, &to_target).await?;
        assert!(!denied_wrong_realm);

        let from_wrong_manufacturer = create_actor_id(4, source_realm, "other", "edge");
        let denied_wrong_manufacturer =
            ServiceRegistry::check_discovery_acl(&from_wrong_manufacturer, &to_target).await?;
        assert!(!denied_wrong_manufacturer);

        Ok(())
    }

    /// Reproduce Ghost Candidates Bug:
    ///
    /// Real scenario: after actrix restart, restore_from_storage() loads ActorId A (serial=xxx1)
    /// into memory. Then a new service process starts, AIS assigns a new ActorId B (serial=xxx2),
    /// and calls register_service_full(). Without deduplication, both A (ghost) and B (real)
    /// coexist in memory under the same service_name.
    ///
    /// Expected (with bug): find_by_actr_type returns 2 candidates (A + B).
    /// Expected (after fix): find_by_actr_type returns 1 candidate (only B).
    #[test]
    fn test_ghost_candidates_double_register_same_actor_id() {
        let mut registry = ServiceRegistry::new();

        let actr_type = ActrType {
            manufacturer: "acme".to_string(),
            name: "EchoService".to_string(),
            version: "0.0.1".to_string(),
        };

        // ActorId A: from SQLite restore (loaded by restore_from_storage during actrix restart)
        let actor_id_a = ActrId {
            serial_number: 11111, // old serial
            realm: actr_protocol::Realm { realm_id: 1 },
            r#type: actr_type.clone(),
        };

        // ActorId B: new service process registration, new serial assigned by AIS
        let actor_id_b = ActrId {
            serial_number: 22222, // new serial, different from A
            realm: actr_protocol::Realm { realm_id: 1 },
            r#type: actr_type.clone(),
        };

        // First registration: simulate actrix restart loading ActorId A via restore_from_storage()
        // Immediately mark as storage-restored (matching the real restore_from_storage behavior)
        registry
            .register_service_full(
                actor_id_a.clone(),
                "EchoService".to_string(),
                vec!["echo.EchoRequest".to_string()],
                None,
                None,
                None,
                None,
            )
            .unwrap();
        registry.mark_all_as_restored("EchoService");

        let candidates = registry.find_by_actr_type(&actr_type);
        assert_eq!(
            candidates.len(),
            1,
            "After first registration (ActorId A) there should be only 1 candidate"
        );

        // Second registration: simulate a new service process starting, AIS assigns new ActorId B, calls register_service_full
        // Memory already has A (ghost), B is pushed directly -> produces 2 entries
        registry
            .register_service_full(
                actor_id_b.clone(),
                "EchoService".to_string(),
                vec!["echo.EchoRequest".to_string()],
                None,
                None,
                None,
                None,
            )
            .unwrap();

        let candidates_after = registry.find_by_actr_type(&actr_type);

        // With bug: memory [ActorId A (ghost), ActorId B (real)] -> returns 2 candidates -> FAIL
        // After fix: registering B clears old entry -> only B retained -> returns 1 candidate -> PASS
        assert_eq!(
            candidates_after.len(),
            1,
            "[BUG DETECTED] After registering ActorId B (serial=22222), memory contains both ActorId A (serial=11111, ghost) and B, \
             find_by_actr_type returned {} candidates, expected 1.\
             \nCause: register_service_full() did not clear old entries under the same service_name (different actor_id).",
            candidates_after.len()
        );
    }

    #[test]
    fn test_find_by_actr_type_requires_exact_version() {
        let mut registry = ServiceRegistry::new();

        let actor_id = ActrId {
            serial_number: 1,
            r#type: ActrType {
                manufacturer: "acme".to_string(),
                name: "worker".to_string(),
                version: "1.0.0".to_string(),
            },
            realm: actr_protocol::Realm { realm_id: 0 },
        };

        registry
            .register_service(
                actor_id,
                "worker1".to_string(),
                vec!["Work".to_string()],
                None,
            )
            .unwrap();

        let target_type = ActrType {
            manufacturer: "acme".to_string(),
            name: "worker".to_string(),
            version: "2.0.0".to_string(),
        };

        let results = registry.find_by_actr_type(&target_type);
        assert!(results.is_empty(), "version mismatch must not match");
    }

    #[tokio::test]
    async fn test_restore_service_from_storage_preserves_actor_version() {
        let actor_id = ActrId {
            serial_number: 914_562_612_010_752,
            realm: actr_protocol::Realm {
                realm_id: 33_554_433,
            },
            r#type: ActrType {
                manufacturer: "goaskaway".to_string(),
                name: "BotService".to_string(),
                version: "4.0.0".to_string(),
            },
        };
        let service = ServiceInfo {
            actor_id: actor_id.clone(),
            service_name: "goaskaway:BotService".to_string(),
            message_types: vec!["goaskaway.Ask".to_string()],
            capabilities: None,
            status: ServiceStatus::Available,
            last_heartbeat_time_secs: current_timestamp(),
            service_spec: None,
            acl: None,
            service_availability_state: None,
            power_reserve: None,
            mailbox_backlog: None,
            worst_dependency_health_state: None,
            geo_location: None,
            is_exact_match: false,
            sticky_client_ids: vec![],
            ws_address: None,
            is_restored_from_storage: false,
        };

        let storage = Arc::new(
            ServiceRegistryStorage::new(":memory:", Some(3600))
                .await
                .unwrap(),
        );
        storage.save_service(&service).await.unwrap();

        let mut registry = ServiceRegistry::new();
        registry.set_storage(storage);

        assert!(
            registry
                .restore_service_from_storage(&actor_id)
                .await
                .unwrap()
        );
        registry
            .update_load_metrics(&actor_id, 0, 0.75, 0.1)
            .unwrap();

        registry.unregister_actor_memory_only(&actor_id);
        assert!(registry.find_by_actr_type(&actor_id.r#type).is_empty());

        assert!(
            registry
                .restore_service_from_storage(&actor_id)
                .await
                .unwrap()
        );
        registry
            .update_load_metrics(&actor_id, 0, 0.75, 0.1)
            .unwrap();

        let candidates = registry.find_by_actr_type(&actor_id.r#type);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].actor_id, actor_id);
        assert_eq!(candidates[0].power_reserve, Some(0.75));

        assert!(
            !registry
                .restore_service_from_storage(&actor_id)
                .await
                .unwrap(),
            "a repeated restore with no index changes must report false"
        );
        let candidates_after_retry = registry.find_by_actr_type(&actor_id.r#type);
        assert_eq!(candidates_after_retry.len(), 1);
        assert_eq!(candidates_after_retry[0].power_reserve, Some(0.75));

        let services_by_message = registry.discover_by_message_type("goaskaway.Ask");
        assert_eq!(services_by_message.len(), 1);
        assert_eq!(
            registry.get_message_type_stats().get("goaskaway.Ask"),
            Some(&1)
        );

        registry
            .register_service_full(
                actor_id.clone(),
                service.service_name.clone(),
                service.message_types.clone(),
                None,
                None,
                None,
                None,
            )
            .unwrap();

        let candidates_after_registration = registry.find_by_actr_type(&actor_id.r#type);
        assert_eq!(candidates_after_registration.len(), 1);
        assert_eq!(candidates_after_registration[0].actor_id, actor_id);
        assert!(!candidates_after_registration[0].is_restored_from_storage);

        let services_by_message = registry.discover_by_message_type("goaskaway.Ask");
        assert_eq!(services_by_message.len(), 1);
        assert_eq!(
            registry.get_message_type_stats().get("goaskaway.Ask"),
            Some(&1)
        );
        assert_eq!(
            registry.actor_index.get(&actor_id),
            Some(&vec![service.service_name])
        );
    }
}
