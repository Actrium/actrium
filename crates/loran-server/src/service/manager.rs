//! Service Manager
//!
//! Implements service start, stop, and management logic
//! Service manager module - manages the lifecycle of multiple services

use super::{HttpRouterService, IceService, http::build_control_router};
use crate::service::container::ServiceContainer;
use crate::service::grpc::build_signer_grpc_router;
use crate::service::http::{IssuerService, PublisherRegistryService, SignalingService};
use crate::service::ice::{StunService, TurnService};
use anyhow::Result;
use axum::Router;
use axum::extract::connect_info::ConnectInfo;
use axum_server::tls_rustls::RustlsConfig;
use platform::{
    ServiceCollector, ServiceCounters, ServiceInfo, ServiceType, TlsConfigurer,
    config::ActrixConfig, config::config_store::ConfigOverrideStore, monitoring::MetricsStore,
};

/// ResourceType integer constants (from proto enum).
mod resource_type {
    pub const STUN: i32 = 1;
    pub const TURN: i32 = 2;
    pub const SIGNALING: i32 = 3;
    pub const ISSUER: i32 = 4;
    pub const SIGNER: i32 = 5;
    pub const PUBLISHER_REGISTRY: i32 = 6;

    /// Map a ServiceType to its proto ResourceType integer.
    pub fn from_service_type(st: &super::ServiceType) -> i32 {
        match st {
            super::ServiceType::Stun => STUN,
            super::ServiceType::Turn => TURN,
            super::ServiceType::Signaling => SIGNALING,
            super::ServiceType::Issuer => ISSUER,
            super::ServiceType::Signer => SIGNER,
            super::ServiceType::PublisherRegistry => PUBLISHER_REGISTRY,
        }
    }
}
use std::collections::HashMap;
use std::convert::Infallible;
use std::future::{Ready, ready};
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::sync::{Notify, watch};
use tokio::task::JoinHandle;
use tower_service::Service;
use url::Url;

/// Swappable make service for hot-reloading the Router.
///
/// Each new TCP connection clones the current Router from the watch channel.
/// SIGHUP triggers a new Router to be sent through the channel, so new
/// connections immediately use the updated routes while existing connections
/// continue with their previously obtained Router.
#[derive(Clone)]
struct SwappableMakeService {
    router_rx: watch::Receiver<Router>,
}

impl SwappableMakeService {
    fn new(router_rx: watch::Receiver<Router>) -> Self {
        Self { router_rx }
    }
}

impl Service<SocketAddr> for SwappableMakeService {
    type Response = Router;
    type Error = Infallible;
    type Future = Ready<Result<Router, Infallible>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, target: SocketAddr) -> Self::Future {
        let router = self.router_rx.borrow().clone();
        // Inject ConnectInfo<SocketAddr> so handlers (e.g. signaling WS) can extract it.
        let router = router.layer(axum::Extension(ConnectInfo(target)));
        ready(Ok(router))
    }
}

/// Service manager, responsible for managing the lifecycle of multiple services
#[derive(Debug)]
pub struct ServiceManager {
    services: Vec<ServiceContainer>,
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
    service_collector: ServiceCollector,
    config: ActrixConfig,
    config_path: PathBuf,
    /// Watch channel sender for hot-swapping the Router.
    router_tx: Option<watch::Sender<Router>>,
    /// TLS configuration handle for certificate hot-reload.
    tls_config: Option<RustlsConfig>,
    /// Independent shutdown channel for ICE services (allows restart without
    /// affecting the HTTP server).
    ice_shutdown_tx: Option<tokio::sync::broadcast::Sender<()>>,
    /// Join handles for running ICE service tasks.
    ice_handles: Vec<JoinHandle<()>>,
    /// axum_server Handle for graceful HTTP shutdown.
    http_handle: Option<axum_server::Handle>,
    /// JWT secret for Admin UI, persisted across reloads.
    jwt_secret: Option<Vec<u8>>,
    /// SQLite-backed config override store, reused across reloads.
    config_store: Option<Arc<ConfigOverrideStore>>,
    /// Cancellation token for background tasks spawned during router build.
    /// Cancelled on reload to stop old tasks before spawning new ones.
    bg_cancel: Option<tokio_util::sync::CancellationToken>,
    /// Cancellation token for the dedicated Signer listener's background
    /// cleanup tasks (SGN-04/07). Independent of `bg_cancel` so those tasks
    /// live for the lifetime of the listener (which is not hot-reloaded) and
    /// are cancelled only on shutdown.
    signer_bg_cancel: Option<tokio_util::sync::CancellationToken>,
    /// In-memory metrics ring buffer, reused across reloads.
    metrics_store: MetricsStore,
    /// Per-service-type atomic counters, keyed by ResourceType i32.
    service_counters: Arc<HashMap<i32, Arc<ServiceCounters>>>,
}

impl ServiceManager {
    /// Create a new service manager
    pub fn new(
        config: ActrixConfig,
        shutdown_tx: tokio::sync::broadcast::Sender<()>,
        config_path: PathBuf,
    ) -> Self {
        Self {
            services: Vec::new(),
            shutdown_tx,
            service_collector: ServiceCollector::new(),
            config,
            config_path,
            router_tx: None,
            tls_config: None,
            ice_shutdown_tx: None,
            ice_handles: Vec::new(),
            http_handle: None,
            jwt_secret: None,
            config_store: None,
            bg_cancel: None,
            signer_bg_cancel: None,
            metrics_store: MetricsStore::new(),
            service_counters: Arc::new(HashMap::new()),
        }
    }

    /// Build a counters map for all five service types.
    ///
    /// Called once during initial start. The same Arc'd counters are reused
    /// across hot-reloads so the sampler sees continuous data.
    fn build_service_counters() -> Arc<HashMap<i32, Arc<ServiceCounters>>> {
        let mut map = HashMap::new();
        map.insert(resource_type::STUN, Arc::new(ServiceCounters::new()));
        map.insert(resource_type::TURN, Arc::new(ServiceCounters::new()));
        map.insert(resource_type::SIGNALING, Arc::new(ServiceCounters::new()));
        map.insert(resource_type::ISSUER, Arc::new(ServiceCounters::new()));
        map.insert(resource_type::SIGNER, Arc::new(ServiceCounters::new()));
        Arc::new(map)
    }

    /// Add a service to the manager
    pub fn add_service(&mut self, service: ServiceContainer) {
        platform::recording::info!("Adding service '{}' to manager", service.info().name);
        self.services.push(service);
    }

    /// Register services to the management platform
    pub async fn register_services(&self, services: Vec<ServiceInfo>) -> Result<()> {
        // Current control plane is pull-based (/admin), no active registration.
        platform::recording::debug!(
            "Control plane is pull-based, skipping active service registration for {} services",
            services.len()
        );
        Ok(())
    }

    /// Rebuild ServiceCollector entries based on the current config.
    ///
    /// Removes entries for disabled services and ensures enabled services
    /// have entries. Called before building the router so the admin dashboard
    /// reflects the correct set of active services.
    async fn refresh_service_collector(&mut self) {
        let svc_types = [
            (
                "Signaling Service",
                ServiceType::Signaling,
                self.config.is_signaling_enabled(),
            ),
            (
                "Issuer Service",
                ServiceType::Issuer,
                self.config.is_issuer_enabled(),
            ),
            (
                "Signer Service",
                ServiceType::Signer,
                self.config.is_signer_enabled(),
            ),
        ];

        for (name, svc_type, enabled) in &svc_types {
            if *enabled {
                // Ensure entry exists (may already be there from initial start)
                if self.service_collector.get(name).await.is_none() {
                    let mut info = ServiceInfo::new(*name, svc_type.clone(), None, &self.config);
                    let rt_id = resource_type::from_service_type(svc_type);
                    if let Some(ctr) = self.service_counters.get(&rt_id) {
                        info.set_counters(ctr.clone());
                    }
                    self.service_collector.insert(name.to_string(), info).await;
                }
            } else {
                // Remove disabled services
                self.service_collector.remove(name).await;
            }
        }
    }

    /// Build the combined Router from configuration.
    ///
    /// This is used both for initial startup and for reload. It creates fresh
    /// service instances, builds their routers, merges the control router,
    /// adds /metrics, and applies global middleware layers.
    async fn build_router_from_config(&mut self) -> Result<Router> {
        use crate::service::trace::http_trace_layer;

        // Cancel any previously spawned background tasks (cleanup loops, key refresh)
        if let Some(old_cancel) = self.bg_cancel.take() {
            old_cancel.cancel();
        }
        let cancel = tokio_util::sync::CancellationToken::new();
        self.bg_cancel = Some(cancel.clone());

        let mut app = Router::new();

        // Refresh ServiceCollector to reflect currently enabled services.
        // This ensures the admin dashboard shows accurate service status after reload.
        self.refresh_service_collector().await;

        // Initialize or reuse the ConfigOverrideStore across reloads.
        let config_store = if let Some(ref store) = self.config_store {
            store.clone()
        } else {
            let pool = platform::storage::db::get_database().get_pool().clone();
            let store = Arc::new(
                ConfigOverrideStore::new(pool)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to create config override store: {e}"))?,
            );
            self.config_store = Some(store.clone());
            store
        };

        // Apply L2 overrides to the running config
        let overrides = config_store.list_all().await.unwrap_or_default();
        if !overrides.is_empty() {
            let toml_content = std::fs::read_to_string(&self.config_path).unwrap_or_default();
            match platform::config::resolver::apply_overrides(&toml_content, &overrides) {
                Ok(merged) => {
                    // CFG-02: The merged config (disk config + L2 SQLite overrides)
                    // has never been validated as a whole. A single override could
                    // silently relax a safety control (e.g. disable Signer KEK
                    // enforcement). Validate before adopting; on failure keep the
                    // file-only config and record a security-channel event.
                    match merged.validate() {
                        Ok(()) => {
                            platform::recording::info!(
                                "Applied {} config override(s) from database",
                                overrides.len()
                            );
                            self.config = merged;
                        }
                        Err(errors) => {
                            let non_warnings: Vec<_> = errors
                                .iter()
                                .filter(|e| !e.starts_with("Warning:"))
                                .collect();
                            if non_warnings.is_empty() {
                                for w in errors.iter().filter(|e| e.starts_with("Warning:")) {
                                    platform::recording::warn!("Config override warning: {}", w);
                                }
                                platform::recording::info!(
                                    "Applied {} config override(s) from database",
                                    overrides.len()
                                );
                                self.config = merged;
                            } else {
                                let joined = non_warnings
                                    .iter()
                                    .map(|s| s.as_str())
                                    .collect::<Vec<_>>()
                                    .join("; ");
                                platform::recording::error!(
                                    "Rejected config overrides: merged config failed \
                                     validation ({}). Falling back to file-only config.",
                                    joined
                                );
                                emit_config_override_rejected(&joined);
                                // Keep self.config as the file-only config.
                            }
                        }
                    }
                }
                Err(e) => {
                    platform::recording::warn!(
                        "Failed to apply config overrides, using file-only config: {}",
                        e
                    );
                }
            }
        }

        // Create the Issuer state (shared issuer) before the control router so
        // the control plane and the public /issuer router hold the same issuer.
        // On failure the /issuer routes are simply not mounted, matching the
        // previous behavior where IssuerService::build_router failed.
        let issuer_state = if self.config.is_issuer_enabled() {
            let issuer_config = self.config.services.issuer.clone();
            match issuer_config {
                Some(issuer_config) => {
                    let counters = self.service_counters.get(&(resource_type::ISSUER)).cloned();
                    match issuer::create_issuer_state(
                        &issuer_config,
                        &self.config,
                        cancel.clone(),
                        counters,
                    )
                    .await
                    {
                        Ok(state) => Some(state),
                        Err(e) => {
                            platform::recording::error!(
                                "Failed to build router for service 'Issuer Service': {:?}",
                                e
                            );
                            None
                        }
                    }
                }
                None => {
                    platform::recording::error!(
                        "Failed to build router for service 'Issuer Service': Issuer config not found"
                    );
                    None
                }
            }
        } else {
            None
        };

        // Control plane always present.
        // Pass existing JWT secret so admin sessions survive reloads.
        let (control_router, jwt_secret) = build_control_router(
            &self.config,
            self.service_collector.clone(),
            self.shutdown_tx.clone(),
            self.config_path.clone(),
            self.jwt_secret.clone(),
            config_store,
            self.metrics_store.clone(),
            issuer_state.as_ref().map(|state| state.issuer.clone()),
        )
        .await?;
        self.jwt_secret = Some(jwt_secret);
        platform::recording::info!(
            "Adding control routes: admin_ui={}, grpc_api={}",
            self.config.admin_ui_enabled(),
            self.config.grpc_api_enabled()
        );
        app = app.merge(control_router);

        // Public authority bootstrap is an instance-level concern. It stays
        // separate from the individual signaling, identity, and package roles
        // even when Loran serves them from the same process.
        app = app.merge(crate::service::http::build_authority_router(&self.config)?);

        // Business services based on enable bitmask
        if self.config.is_signaling_enabled() {
            let mut svc = SignalingService::new(self.config.clone(), cancel.clone());
            if let Some(ctr) = self.service_counters.get(&(resource_type::SIGNALING)) {
                svc.set_counters(ctr.clone());
            }
            match svc.build_router().await {
                Ok(router) => {
                    let prefix = svc.route_prefix();
                    platform::recording::info!(
                        "Adding route '{}' for service '{}'",
                        prefix,
                        svc.info().name
                    );
                    app = app.nest(prefix, router);
                }
                Err(e) => {
                    platform::recording::error!(
                        "Failed to build router for service '{}': {:?}",
                        svc.info().name,
                        e
                    );
                }
            }
        }

        if self.config.is_issuer_enabled() {
            if let Some(state) = issuer_state.clone() {
                let mut svc = IssuerService::with_state(&self.config, state);
                if let Some(ctr) = self.service_counters.get(&(resource_type::ISSUER)) {
                    svc.set_counters(ctr.clone());
                }
                match svc.build_router().await {
                    Ok(router) => {
                        let prefix = svc.route_prefix();
                        platform::recording::info!(
                            "Adding route '{}' for service '{}'",
                            prefix,
                            svc.info().name
                        );
                        app = app.nest(prefix, router);
                    }
                    Err(e) => {
                        platform::recording::error!(
                            "Failed to build router for service '{}': {:?}",
                            svc.info().name,
                            e
                        );
                    }
                }
            }
        }

        // SGN-02: the Signer gRPC signing oracle is no longer merged into this
        // shared listener. It is served on its own dedicated listener (loopback
        // by default, mTLS when configured) started via `start_signer_service`.

        // Publisher Registry service - always mounted (no config gate yet)
        {
            let mut svc = PublisherRegistryService::new(self.config.clone());
            match svc.build_router().await {
                Ok(router) => {
                    let prefix = svc.route_prefix();
                    platform::recording::info!(
                        "Adding route '{}' for service '{}'",
                        prefix,
                        svc.info().name
                    );
                    app = app.nest(prefix, router);
                }
                Err(e) => {
                    platform::recording::error!(
                        "Failed to build router for service '{}': {:?}",
                        svc.info().name,
                        e
                    );
                }
            }
        }

        // Observability endpoints: /health, /metrics, /{service}/health, /{service}/metrics
        {
            use crate::service::http::observability::{
                ObservabilityState, build_observability_router,
            };

            let obs_state = Arc::new(ObservabilityState {
                collector: self.service_collector.clone(),
                config: self.config.clone(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            });
            let obs_router = build_observability_router(obs_state);
            platform::recording::info!(
                "Adding observability routes: /health, /metrics, /{{service}}/health, /{{service}}/metrics"
            );
            app = app.merge(obs_router);
        }

        // Global middleware layers
        app = app.layer(http_trace_layer()); // HTTP tracing (with OpenTelemetry context propagation)

        Ok(app)
    }

    /// Start all services
    pub async fn start_all(&mut self) -> Result<Vec<JoinHandle<()>>> {
        platform::recording::info!(
            "Starting all {} types ({}) services.",
            self.services.len(),
            self.services
                .iter()
                .map(|s| s.info().service_type.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );

        // Build service counters (once) and start the background sampler.
        self.service_counters = Self::build_service_counters();
        self.metrics_store
            .clone()
            .start_sampler(self.service_counters.clone(), Duration::from_secs(60));

        let services = std::mem::take(&mut self.services);
        let mut ice_services = Vec::new();

        // Separate HTTP router services (used for on_start callbacks only) and ICE services.
        // Wire service counters into ICE services.
        let mut http_service_containers = Vec::new();
        for mut service in services {
            if service.is_http_router() {
                http_service_containers.push(service);
            } else if service.is_ice() {
                // Attach counters to ICE services
                match &mut service {
                    ServiceContainer::Stun(s) => {
                        if let Some(ctr) = self.service_counters.get(&resource_type::STUN) {
                            s.counters = Some(ctr.clone());
                        }
                    }
                    ServiceContainer::Turn(s) => {
                        if let Some(ctr) = self.service_counters.get(&resource_type::TURN) {
                            s.counters = Some(ctr.clone());
                        }
                    }
                    _ => {}
                }
                ice_services.push(service);
            }
        }

        let notify = Arc::new(Notify::new());
        let notify_clone = notify.clone();
        let mut handle_futs = Vec::new();

        // Start HTTP server (control plane /admin always on)
        let handle = self
            .start_http_services(http_service_containers, notify_clone)
            .await?;
        handle_futs.push(handle);
        notify.notified().await;

        // Start the dedicated Signer gRPC listener (SGN-02), separate from the
        // shared HTTP listener so the signing oracle is not exposed on the
        // public port. Loopback-only by default; mTLS when configured.
        if self.config.is_signer_enabled() {
            let handle = self.start_signer_service(notify.clone()).await?;
            handle_futs.push(handle);
            notify.notified().await;
        }
        let notify_clone = notify.clone();

        // Create ICE shutdown channel
        let (ice_shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(10);
        self.ice_shutdown_tx = Some(ice_shutdown_tx);

        // Start ICE services (handles tracked internally for restart)
        for service in ice_services {
            let handle = self
                .start_ice_service(service, notify_clone.clone())
                .await?;
            self.ice_handles.push(handle);
            notify.notified().await;
        }

        let services = self.service_collector.values().await;
        self.register_services(services).await?;

        Ok(handle_futs)
    }

    /// Start the HTTP server, merging all HTTP routing services
    async fn start_http_services(
        &mut self,
        mut service_containers: Vec<ServiceContainer>,
        notify: Arc<Notify>,
    ) -> Result<JoinHandle<()>> {
        let http_config = self
            .config
            .bind
            .http
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No bind.http configuration found"))?;

        let is_tls = http_config.is_tls();
        let protocol = if is_tls { "HTTPS" } else { "HTTP" };

        platform::recording::info!(
            "Starting {} server with {} route services (environment: {})",
            protocol,
            service_containers.len(),
            self.config.env
        );

        let bind_addr = parse_bind_socket_addr("bind.http.ip", &http_config.ip, http_config.port)?;
        let scheme = http_config.scheme();
        let public_url = Url::parse(&format!(
            "{}://{}:{}",
            scheme, http_config.domain_name, http_config.port
        ))
        .map_err(|e| anyhow::anyhow!("Failed to parse {} URL: {e}", scheme.to_uppercase()))?;

        let tls_config = if is_tls {
            let cert = http_config.cert.as_ref().unwrap();
            let key = http_config.key.as_ref().unwrap();
            TlsConfigurer::install_crypto_provider();
            Some(RustlsConfig::from_pem_file(cert, key).await?)
        } else {
            None
        };

        // Save TLS config handle for certificate hot-reload
        self.tls_config = tls_config.clone();

        // Build the initial router
        let initial_router = self.build_router_from_config().await?;

        // Fire on_start callbacks for HTTP services (for ServiceCollector registration)
        for service in &mut service_containers {
            let service_name = service.info().name.clone();
            if let Some(result) = service.on_start(public_url.clone()).await {
                self.service_collector
                    .insert(service_name.clone(), service.info().clone())
                    .await;
                if let Err(e) = result {
                    platform::recording::error!(
                        "Failed to start service '{}': {:?}",
                        service_name,
                        e
                    );
                }
            }
        }

        // The Signer gRPC service runs on its own dedicated listener (SGN-02);
        // its running URL is registered in `start_signer_service`, not here.

        // Create watch channel for router hot-swap
        let (router_tx, router_rx) = watch::channel(initial_router);
        self.router_tx = Some(router_tx);

        let make_svc = SwappableMakeService::new(router_rx);

        // Create axum_server Handle for graceful shutdown
        let handle = axum_server::Handle::new();
        self.http_handle = Some(handle.clone());

        // Spawn shutdown listener that triggers graceful shutdown on the Handle
        let handle_for_shutdown = handle.clone();
        let shutdown_tx = self.shutdown_tx.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        tokio::spawn(async move {
            let _ = shutdown_rx.recv().await;
            platform::recording::info!("{} server received shutdown signal", protocol);
            handle_for_shutdown.graceful_shutdown(Some(Duration::from_secs(10)));
        });

        platform::recording::info!("{} server listening on {}", protocol, bind_addr);
        notify.notify_one();

        let shutdown_tx_for_error = shutdown_tx.clone();
        let fut = if let Some(tls_config) = tls_config {
            // Start HTTPS server with axum_server
            tokio::spawn(async move {
                if let Err(e) = axum_server::bind_rustls(bind_addr, tls_config)
                    .handle(handle)
                    .serve(make_svc)
                    .await
                {
                    platform::recording::error!("HTTPS server error: {}", e);
                    let _ = shutdown_tx_for_error.send(());
                }
                platform::recording::info!("HTTPS server stopped");
            })
        } else {
            // Eagerly bind the TCP listener so port conflicts are detected
            // before spawning the server task.
            let listener = tokio::net::TcpListener::bind(bind_addr)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to bind to address '{bind_addr}': {e}"))?;
            let std_listener = listener.into_std()?;

            tokio::spawn(async move {
                if let Err(e) = axum_server::from_tcp(std_listener)
                    .handle(handle)
                    .serve(make_svc)
                    .await
                {
                    platform::recording::error!("HTTP server error: {}", e);
                    let _ = shutdown_tx_for_error.send(());
                }
                platform::recording::info!("HTTP server stopped");
            })
        };

        Ok(fut)
    }

    /// Start the dedicated Signer gRPC listener (SGN-02).
    ///
    /// The Signer signing oracle is served on its own listener instead of being
    /// merged into the shared primary HTTP listener. By default it binds
    /// loopback-only on `bind.http.port + 1`; operators can override the bind
    /// address via `[bind.signer]`. When `services.signer.mtls_enabled()` is
    /// true the listener terminates TLS with mutual client-certificate
    /// verification; otherwise it serves plaintext h2c for loopback callers.
    async fn start_signer_service(&mut self, notify: Arc<Notify>) -> Result<JoinHandle<()>> {
        let signer_ip = self.config.signer_bind_ip();
        let signer_port = self.config.signer_bind_port();
        let bind_addr = parse_bind_socket_addr("bind.signer.ip", &signer_ip, signer_port)?;
        let use_tls = self.config.signer_listener_tls();

        // Dedicated cancellation token for the Signer background cleanup tasks
        // (SGN-04/07), independent of the hot-reload token.
        let signer_cancel = tokio_util::sync::CancellationToken::new();
        self.signer_bg_cancel = Some(signer_cancel.clone());

        // Build the rate-limited Signer gRPC router.
        let signer_counters = self.service_counters.get(&(resource_type::SIGNER)).cloned();
        let router = build_signer_grpc_router(&self.config, signer_counters, signer_cancel)
            .await
            .map_err(|e| {
                anyhow::anyhow!("Failed to build gRPC router for service 'Signer Service': {e:?}")
            })?;
        let router = apply_signer_rate_limit(router);

        // Inject ConnectInfo<SocketAddr> per connection so the per-IP rate
        // limiter can read the source IP, mirroring the shared listener.
        let make_svc = router.into_make_service_with_connect_info::<SocketAddr>();

        // Build the mTLS server config when configured. install_crypto_provider
        // is idempotent and mirrors start_http_services.
        let tls_config =
            if use_tls {
                let signer_cfg = self
                    .config
                    .services
                    .signer
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Signer service configuration not found"))?;
                let ca = signer_cfg.mtls_ca_cert.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("Signer mTLS enabled but mtls_ca_cert is missing")
                })?;
                let cert = signer_cfg.mtls_cert.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("Signer mTLS enabled but mtls_cert is missing")
                })?;
                let key = signer_cfg.mtls_key.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("Signer mTLS enabled but mtls_key is missing")
                })?;
                TlsConfigurer::install_crypto_provider();
                let server_config = TlsConfigurer::create_mtls_server_config(ca, cert, key)
                    .map_err(|e| anyhow::anyhow!("Failed to build Signer mTLS config: {e}"))?;
                Some(RustlsConfig::from_config(Arc::new(server_config)))
            } else {
                None
            };

        let protocol = if use_tls { "gRPC/mTLS" } else { "gRPC/h2c" };
        platform::recording::info!(
            "Starting Signer {} listener on {} (loopback={})",
            protocol,
            bind_addr,
            bind_addr.ip().is_loopback()
        );

        // Register the Signer service as running on its dedicated endpoint.
        if let Some(mut info) = self.service_collector.get("Signer Service").await {
            match Url::parse(&self.config.signer_client_endpoint()) {
                Ok(url) => {
                    info.set_running(url);
                    self.service_collector
                        .insert("Signer Service".to_string(), info)
                        .await;
                }
                Err(e) => {
                    platform::recording::error!("Failed to parse Signer service URL: {e}");
                }
            }
        }

        // Create axum_server Handle for graceful shutdown.
        let handle = axum_server::Handle::new();
        let handle_for_shutdown = handle.clone();
        let shutdown_tx = self.shutdown_tx.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        tokio::spawn(async move {
            let _ = shutdown_rx.recv().await;
            platform::recording::info!("Signer listener received shutdown signal");
            handle_for_shutdown.graceful_shutdown(Some(Duration::from_secs(10)));
        });

        platform::recording::info!("Signer listener listening on {}", bind_addr);
        notify.notify_one();

        let shutdown_tx_for_error = shutdown_tx.clone();
        let fut = if let Some(tls_config) = tls_config {
            tokio::spawn(async move {
                if let Err(e) = axum_server::bind_rustls(bind_addr, tls_config)
                    .handle(handle)
                    .serve(make_svc)
                    .await
                {
                    platform::recording::error!("Signer mTLS listener error: {}", e);
                    let _ = shutdown_tx_for_error.send(());
                }
                platform::recording::info!("Signer listener stopped");
            })
        } else {
            // Eagerly bind the TCP listener so port conflicts surface early.
            let listener = tokio::net::TcpListener::bind(bind_addr)
                .await
                .map_err(|e| {
                    anyhow::anyhow!("Failed to bind Signer listener to '{bind_addr}': {e}")
                })?;
            let std_listener = listener.into_std()?;
            tokio::spawn(async move {
                if let Err(e) = axum_server::from_tcp(std_listener)
                    .handle(handle)
                    .serve(make_svc)
                    .await
                {
                    platform::recording::error!("Signer listener error: {}", e);
                    let _ = shutdown_tx_for_error.send(());
                }
                platform::recording::info!("Signer listener stopped");
            })
        };

        Ok(fut)
    }

    /// Start a single ICE service.
    async fn start_ice_service(
        &mut self,
        service: ServiceContainer,
        notify: Arc<Notify>,
    ) -> Result<JoinHandle<()>> {
        // ICE services use the independent ice_shutdown channel so they can be
        // restarted during reload without affecting the HTTP server.
        let shutdown_rx = self
            .ice_shutdown_tx
            .as_ref()
            .expect("ice_shutdown_tx must be initialized before starting ICE services")
            .subscribe();
        let shutdown_tx = self.shutdown_tx.clone();
        let service_name = service.info().name.clone();
        let bind_addr = self.config.bind.ice.ip.clone();
        let config = self.config.clone();

        match service {
            ServiceContainer::Stun(mut s) => {
                let (tx, rx) = tokio::sync::oneshot::channel::<ServiceInfo>();
                let handle = tokio::spawn(async move {
                    if let Err(e) = s.start(shutdown_rx, tx).await {
                        platform::recording::error!("Failed to start STUN service: {:?}", e);
                        let _ = shutdown_tx.send(());
                    }
                });
                let info = rx
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to receive STUN service info: {e}"))?;
                self.service_collector.insert(info.name.clone(), info).await;
                notify.notify_one();
                Ok(handle)
            }
            ServiceContainer::Turn(mut s) => {
                let (tx, rx) = tokio::sync::oneshot::channel::<ServiceInfo>();
                let handle = tokio::spawn(async move {
                    if let Err(e) = s.start(shutdown_rx, tx).await {
                        platform::recording::error!("Failed to start TURN service: {:?}", e);
                        let _ = shutdown_tx.send(());
                    }
                });
                let info = rx
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to receive TURN service info: {e}"))?;

                self.service_collector
                    .insert(info.name.clone(), info.clone())
                    .await;

                let mut stun_info =
                    ServiceInfo::new("STUN Server", ServiceType::Stun, None, &config);
                stun_info.set_running(
                    Url::parse(&format!("stun:{}:{}", bind_addr, info.port_info))
                        .map_err(|e| anyhow::anyhow!("Failed to parse STUN URL: {e}"))?,
                );
                if let Some(ctr) = self.service_counters.get(&(resource_type::STUN)) {
                    stun_info.set_counters(ctr.clone());
                }
                self.service_collector
                    .insert(stun_info.name.clone(), stun_info)
                    .await;
                notify.notify_one();
                Ok(handle)
            }
            _ => {
                platform::recording::error!(
                    "Invalid service type for ICE service: {}",
                    service_name
                );
                Err(anyhow::anyhow!(
                    "Invalid service type for ICE service: {service_name}"
                ))
            }
        }
    }

    /// Restart ICE services with updated configuration.
    async fn restart_ice_services(&mut self) -> Result<()> {
        // 1. Signal old ICE services to stop
        if let Some(ref old_tx) = self.ice_shutdown_tx {
            let _ = old_tx.send(());
        }

        // 2. Wait for old handles to finish (5s timeout)
        let old_handles = std::mem::take(&mut self.ice_handles);
        for handle in old_handles {
            let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
        }

        // 3. Create new ICE shutdown channel
        let (ice_shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(10);
        self.ice_shutdown_tx = Some(ice_shutdown_tx);

        // 4. Start new ICE services based on current config
        let notify = Arc::new(Notify::new());

        if self.config.is_ice_enabled() {
            if self.config.is_turn_enabled() {
                platform::recording::info!("Reloading TURN Server (UDP, includes STUN)");
                let mut turn_service = TurnService::new(self.config.clone());
                if let Some(ctr) = self.service_counters.get(&(resource_type::TURN)) {
                    turn_service = turn_service.with_counters(ctr.clone());
                }
                let handle = self
                    .start_ice_service(ServiceContainer::turn(turn_service), notify.clone())
                    .await?;
                self.ice_handles.push(handle);
                notify.notified().await;
            } else if self.config.is_stun_enabled() {
                platform::recording::info!("Reloading STUN Server (UDP)");
                let mut stun_service = StunService::new(self.config.clone());
                if let Some(ctr) = self.service_counters.get(&(resource_type::STUN)) {
                    stun_service = stun_service.with_counters(ctr.clone());
                }
                let handle = self
                    .start_ice_service(ServiceContainer::stun(stun_service), notify.clone())
                    .await?;
                self.ice_handles.push(handle);
                notify.notified().await;
            }
        }

        Ok(())
    }

    /// Check whether config changes require a full process restart.
    ///
    /// Returns a list of human-readable reasons. An empty list means
    /// hot-reload is sufficient.
    fn check_restart_required(old: &ActrixConfig, new: &ActrixConfig) -> Vec<String> {
        let mut reasons = Vec::new();

        if old.bind.http != new.bind.http {
            reasons.push("bind.http changed".into());
        }
        if old.sqlite_path != new.sqlite_path {
            reasons.push("sqlite_path changed".into());
        }
        if old.recording != new.recording {
            reasons.push("recording config changed".into());
        }
        if old.env != new.env {
            reasons.push("env changed".into());
        }
        if old.is_signaling_enabled() != new.is_signaling_enabled() {
            reasons.push("signaling service enablement changed".into());
        }
        if old.services.signaling.as_ref().map(|config| &config.server)
            != new.services.signaling.as_ref().map(|config| &config.server)
        {
            reasons.push("signaling server config changed".into());
        }

        reasons
    }

    /// Reload configuration and hot-swap services.
    ///
    /// On success, new TCP connections will use the updated Router, TLS
    /// certificates are refreshed, and ICE services are restarted if their
    /// configuration changed.
    ///
    /// On failure at any stage, the error is logged and the previous
    /// configuration continues to operate.
    pub async fn reload(&mut self) -> Result<()> {
        platform::recording::info!("Reloading configuration from {:?}", self.config_path);

        // 1. Read & validate new config
        let new_config = match ActrixConfig::from_file(&self.config_path) {
            Ok(cfg) => cfg,
            Err(e) => {
                platform::recording::error!(
                    "Config reload failed: parse error: {}. Keeping old config.",
                    e
                );
                return Err(anyhow::anyhow!("Config parse error: {e}"));
            }
        };

        if let Err(errors) = new_config.validate() {
            let non_warnings: Vec<_> = errors
                .iter()
                .filter(|e| !e.starts_with("Warning:"))
                .collect();
            if !non_warnings.is_empty() {
                platform::recording::error!(
                    "Config reload failed: validation errors: {:?}. Keeping old config.",
                    non_warnings
                );
                return Err(anyhow::anyhow!(
                    "Config validation failed: {}",
                    non_warnings
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join("; ")
                ));
            }
            // Warnings only — proceed
            for w in errors.iter().filter(|e| e.starts_with("Warning:")) {
                platform::recording::warn!("Config reload warning: {}", w);
            }
        }

        let old_config = self.config.clone();
        self.config = new_config;

        // 2. Warn about config changes that require a full restart
        let restart_reasons = Self::check_restart_required(&old_config, &self.config);
        if !restart_reasons.is_empty() {
            for reason in &restart_reasons {
                platform::recording::warn!("Config change ignored (requires restart): {}", reason);
            }
            platform::recording::warn!("Run `systemctl restart actrix` to apply these changes.");
            // Revert immutable fields to old values so runtime state stays consistent
            self.config.bind.http = old_config.bind.http.clone();
            self.config.sqlite_path = old_config.sqlite_path.clone();
            self.config.recording = old_config.recording.clone();
            self.config.env = old_config.env.clone();
            if old_config.is_signaling_enabled() {
                self.config.enable |= platform::config::ENABLE_SIGNALING;
            } else {
                self.config.enable &= !platform::config::ENABLE_SIGNALING;
            }
            self.config.services.signaling = old_config.services.signaling.clone();

            // CFG-01: Reverting restart-only fields above produces a hybrid config
            // (new file values for reloadable fields, old values for the reverted
            // ones) that has never been validated as a whole. Re-validate the final
            // config; if the mix now fails safety checks (e.g. a relaxed Signer KEK
            // enforcement combined with a reverted field), discard this reload
            // entirely and keep the previous config running.
            if let Err(errors) = self.config.validate() {
                let non_warnings: Vec<_> = errors
                    .iter()
                    .filter(|e| !e.starts_with("Warning:"))
                    .collect();
                if !non_warnings.is_empty() {
                    platform::recording::error!(
                        "Config reload discarded: merged config failed validation after \
                         reverting restart-only fields: {:?}. Keeping old config.",
                        non_warnings
                    );
                    self.config = old_config;
                    return Err(anyhow::anyhow!(
                        "Merged config validation failed after field revert: {}",
                        non_warnings
                            .iter()
                            .map(|s| s.as_str())
                            .collect::<Vec<_>>()
                            .join("; ")
                    ));
                }
            }
        }

        // 3. Build new Router
        match self.build_router_from_config().await {
            Ok(new_router) => {
                if let Some(ref tx) = self.router_tx {
                    let _ = tx.send(new_router);
                    platform::recording::info!(
                        "Router hot-swapped successfully. New connections will use updated routes."
                    );
                }
            }
            Err(e) => {
                platform::recording::error!(
                    "Failed to build new router: {}. Keeping old router.",
                    e
                );
                self.config = old_config;
                return Err(e);
            }
        }

        // 4. Reload TLS certificates if applicable
        if let Some(ref tls_config) = self.tls_config
            && let Some(ref http_config) = self.config.bind.http
            && let (Some(cert), Some(key)) = (&http_config.cert, &http_config.key)
        {
            match tls_config.reload_from_pem_file(cert, key).await {
                Ok(()) => {
                    platform::recording::info!("TLS certificates reloaded successfully");
                }
                Err(e) => {
                    platform::recording::error!(
                        "Failed to reload TLS certificates: {}. Old certificates remain active.",
                        e
                    );
                }
            }
        }

        // 5. Restart ICE services if their config changed
        let ice_changed = old_config.bind.ice != self.config.bind.ice
            || old_config.turn != self.config.turn
            || old_config.is_ice_enabled() != self.config.is_ice_enabled()
            || old_config.is_stun_enabled() != self.config.is_stun_enabled()
            || old_config.is_turn_enabled() != self.config.is_turn_enabled();

        if ice_changed {
            platform::recording::info!("ICE configuration changed, restarting ICE services...");
            if let Err(e) = self.restart_ice_services().await {
                platform::recording::error!(
                    "Failed to restart ICE services: {}. ICE in degraded state.",
                    e
                );
            }
        }

        platform::recording::info!("Configuration reload completed successfully");
        Ok(())
    }

    /// Stop all services
    pub async fn stop_all(&mut self) -> Result<()> {
        platform::recording::info!("Stopping all services");

        // Cancel background tasks (cleanup loops, key refresh)
        if let Some(cancel) = self.bg_cancel.take() {
            cancel.cancel();
        }

        // Cancel the dedicated Signer listener's background cleanup tasks.
        if let Some(cancel) = self.signer_bg_cancel.take() {
            cancel.cancel();
        }

        // Stop ICE services via independent channel
        if let Some(ref ice_tx) = self.ice_shutdown_tx {
            let _ = ice_tx.send(());
        }

        // Broadcast global shutdown
        let _ = self.shutdown_tx.send(());

        // Wait for ICE handles
        let ice_handles = std::mem::take(&mut self.ice_handles);
        for handle in ice_handles {
            let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
        }

        // Never unwrap here: one service failing to stop must not panic-abort the
        // whole shutdown and skip the remaining services' cleanup (WAL checkpoints,
        // file handles). Log and continue stopping the rest.
        for service in &mut self.services {
            let result = match service {
                ServiceContainer::Signaling(s) => s.on_stop().await.map_err(|e| e.to_string()),
                ServiceContainer::Issuer(s) => s.on_stop().await.map_err(|e| e.to_string()),
                ServiceContainer::PublisherRegistry(s) => {
                    s.on_stop().await.map_err(|e| e.to_string())
                }
                ServiceContainer::Stun(s) => s.stop().await.map_err(|e| e.to_string()),
                ServiceContainer::Turn(s) => s.stop().await.map_err(|e| e.to_string()),
            };
            if let Err(e) = result {
                platform::recording::error!("Service failed to stop cleanly: {e}");
            }
        }

        platform::recording::info!("All services stopped");
        Ok(())
    }
}

/// Emit a security-channel record when L2 config overrides are rejected because
/// the merged configuration failed validation. This surfaces a potential attempt
/// to weaken a safety control through the dynamic override store.
fn emit_config_override_rejected(errors: &str) {
    use platform::recording::{
        CHANNEL_SECURITY, Outcome, Record, RecordLevel, SecurityPayload, SecuritySeverity,
        emit_with_callsite,
    };

    let mut record = Record::<{ CHANNEL_SECURITY }>::new("config.override.rejected");
    record.common.level = RecordLevel::Error;
    record.common.outcome = Outcome::Denied;
    record.security = Some(SecurityPayload {
        control: "config.override.validation".to_string(),
        severity: SecuritySeverity::High,
        category: Some("config".to_string()),
        subject: Some("config_overrides".to_string()),
        source_addr: None,
        destination_addr: None,
        evidence: Some(serde_json::json!({ "validation_errors": errors })),
    });
    let _ = emit_with_callsite(record, file!(), line!(), module_path!());
}

/// Upper bound on the number of per-IP token buckets kept in memory, so a flood
/// from many distinct source IPs cannot grow the map without bound.
const SIGNER_RL_MAX_BUCKETS: usize = 65_536;

/// Idle time-to-live for a per-IP bucket. Buckets not consulted within this
/// window become eligible for reclamation.
const SIGNER_RL_IDLE_TTL: Duration = Duration::from_secs(600);

/// Burst capacity (maximum tokens) for a single source IP.
///
/// Chosen generously so ordinary callers and the integration test-suite (which
/// issue only a handful of requests from loopback) are never throttled, while a
/// sustained per-IP flood is still capped.
const SIGNER_RL_BURST: f64 = 200.0;

/// Sustained refill rate, in tokens per second, for a single source IP.
const SIGNER_RL_REFILL_PER_SEC: f64 = 100.0;

/// A single source IP's token bucket.
struct SignerTokenBucket {
    /// Current number of available tokens.
    tokens: f64,
    /// Last time tokens were refilled.
    last_refill: Instant,
    /// Last time this bucket was consulted (for idle-TTL / LRU reclamation).
    last_seen: Instant,
}

/// Per-IP token-bucket rate limiter for the Signer gRPC routes.
///
/// Deliberately dependency-free (no governor / tower rate-limit crate) so it can
/// live entirely inside actrixd without pulling in a new dependency. The
/// critical section is a short, non-async map update guarded by a std mutex.
struct SignerRateLimiter {
    buckets: Mutex<HashMap<IpAddr, SignerTokenBucket>>,
}

impl SignerRateLimiter {
    fn new() -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Returns true if a request from `ip` is allowed (consuming one token),
    /// false if the source IP has exhausted its bucket.
    fn check(&self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let mut buckets = match self.buckets.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        // Only reclaim when a brand-new IP is about to be inserted, so
        // steady-state traffic pays no eviction cost.
        if !buckets.contains_key(&ip) {
            Self::reclaim(&mut buckets, now);
        }

        let bucket = buckets.entry(ip).or_insert_with(|| SignerTokenBucket {
            tokens: SIGNER_RL_BURST,
            last_refill: now,
            last_seen: now,
        });

        let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * SIGNER_RL_REFILL_PER_SEC).min(SIGNER_RL_BURST);
        bucket.last_refill = now;
        bucket.last_seen = now;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Reclaim per-IP buckets to keep the map bounded: first drop entries idle
    /// longer than the TTL, then, if still at capacity, evict the
    /// least-recently-seen entries (LRU) until there is room for one more.
    fn reclaim(map: &mut HashMap<IpAddr, SignerTokenBucket>, now: Instant) {
        map.retain(|_ip, bucket| now.duration_since(bucket.last_seen) < SIGNER_RL_IDLE_TTL);

        while map.len() >= SIGNER_RL_MAX_BUCKETS {
            let Some(oldest_ip) = map
                .iter()
                .min_by_key(|(_, bucket)| bucket.last_seen)
                .map(|(ip, _)| *ip)
            else {
                break;
            };
            map.remove(&oldest_ip);
        }
    }
}

/// Wrap the Signer gRPC router with a per-source-IP token-bucket rate limit.
///
/// The source IP is read from the `ConnectInfo<SocketAddr>` extension injected
/// per-connection in [`SwappableMakeService::call`]. If it is unavailable the
/// request is allowed through (fail-open) rather than dropped. Over-limit
/// requests get HTTP 429, which gRPC clients surface as an `UNAVAILABLE`
/// transport error.
fn apply_signer_rate_limit(router: Router) -> Router {
    use axum::response::IntoResponse;

    let limiter = Arc::new(SignerRateLimiter::new());
    router.layer(axum::middleware::from_fn(
        move |req: axum::extract::Request, next: axum::middleware::Next| {
            let limiter = limiter.clone();
            async move {
                let ip = req
                    .extensions()
                    .get::<ConnectInfo<SocketAddr>>()
                    .map(|ci| ci.0.ip());
                if let Some(ip) = ip
                    && !limiter.check(ip)
                {
                    platform::recording::warn!(
                        "Signer gRPC request from {} rejected by per-IP rate limit",
                        ip
                    );
                    return axum::http::StatusCode::TOO_MANY_REQUESTS.into_response();
                }
                next.run(req).await
            }
        },
    ))
}

fn parse_bind_socket_addr(field_name: &str, ip: &str, port: u16) -> Result<SocketAddr> {
    let parsed_ip = ip
        .parse::<IpAddr>()
        .map_err(|e| anyhow::anyhow!("Invalid {field_name} '{ip}': {e}"))?;
    Ok(SocketAddr::new(parsed_ip, port))
}
