//! Actrix auxiliary server main program
//!
//! Starts and manages WebRTC-related auxiliary services, including Signaling, STUN, TURN, etc.
#![deny(clippy::disallowed_macros)]

mod cli;
// mod config; // migrated to standalone config crate
mod error;
mod process;
mod recording_pipeline;
mod service;

use anyhow::Context;
use clap::Parser;
use futures::stream::{FuturesUnordered, StreamExt};
use platform::config::ActrixConfig;
use recording_pipeline::init_recording_pipeline;
use service::{
    AisService, ServiceContainer, ServiceManager, SignalingService, StunService, TurnService,
};
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

macro_rules! bootstrap_info {
    ($($arg:tt)*) => {
        println!($($arg)*);
    };
}

macro_rules! bootstrap_error {
    ($($arg:tt)*) => {
        eprintln!($($arg)*);
    };
}

use cli::{Cli, Commands};
use error::{Error, Result};

/// Application launcher utilities
struct ApplicationLauncher;

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Test { config_file }) => {
            let config_path =
                ApplicationLauncher::find_config_file(config_file.as_ref().unwrap_or(&cli.config))?;
            ApplicationLauncher::test_config_file(&Some(config_path.clone()), &config_path)
        }
        None => {
            let config_path = ApplicationLauncher::find_config_file(&cli.config)?;

            // Create Tokio runtime (before running the application)
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;

            // Run the asynchronous application
            runtime.block_on(ApplicationLauncher::run_application(&config_path))
        }
    }
}

impl ApplicationLauncher {
    /// Find config file with fallback locations
    fn find_config_file(provided_path: &PathBuf) -> Result<PathBuf> {
        // If the provided path is not the default "config.toml", check if it exists
        if provided_path != Path::new("config.toml") {
            if provided_path.exists() {
                bootstrap_info!("Using provided config file: {:?}", provided_path);
                return Ok(provided_path.clone());
            } else {
                bootstrap_error!("Provided config file not found: {:?}", provided_path);
                return Err(Error::custom(format!(
                    "Config file not found: {provided_path:?}"
                )));
            }
        }

        // Otherwise, try fallback locations
        let fallback_paths = vec![
            // 1. Current working directory
            PathBuf::from("config.toml"),
            // 2. System config directory
            PathBuf::from("/etc/actrix/config.toml"),
        ];

        bootstrap_info!("Searching for config file in default locations...");

        for path in &fallback_paths {
            if path.exists() {
                bootstrap_info!("Found config file: {:?}", path);
                return Ok(path.clone());
            } else {
                bootstrap_info!("Config not found at: {:?}", path);
            }
        }

        // If no config file found, provide helpful error message
        bootstrap_error!("No configuration file found!");
        bootstrap_error!("Please create a config file in one of these locations:");
        for (i, path) in fallback_paths.iter().enumerate() {
            bootstrap_error!("  {}. {:?}", i + 1, path);
        }
        bootstrap_error!("Or specify a custom path with: actrix --config <path>");

        Err(Error::custom(
            "No configuration file found. Please create one or specify path with --config",
        ))
    }

    /// Test whether the configuration file is valid
    fn test_config_file(config_file: &Option<PathBuf>, default_config: &PathBuf) -> Result<()> {
        // Initialize basic logging for test command
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .init();

        let config_path = config_file.as_ref().unwrap_or(default_config);
        match ActrixConfig::from_file(config_path) {
            Ok(config) => {
                platform::recording::info!("Config file parsed successfully: {:?}", config_path);

                // Validate configuration
                match config.validate() {
                    Ok(()) => {
                        platform::recording::info!("Configuration validation passed");
                    }
                    Err(errors) => {
                        platform::recording::error!("Configuration validation found issues:");
                        for (i, err) in errors.iter().enumerate() {
                            if err.starts_with("Warning:") {
                                platform::recording::info!("  {}. ⚠️  {}", i + 1, err);
                            } else {
                                platform::recording::error!("  {}. ❌ {}", i + 1, err);
                            }
                        }
                        // Check for non-warning errors
                        let has_errors = errors.iter().any(|e| !e.starts_with("Warning:"));
                        if has_errors {
                            return Err(Error::service_validation(
                                "Configuration validation failed".to_string(),
                            ));
                        }
                    }
                }

                // No need to reinitialize the recording pipeline since basic logging is already initialized
                platform::recording::info!("Full configuration validation passed");
                Ok(())
            }
            Err(e) => {
                platform::recording::error!("Configuration file parse failed: {}", e);
                Err(Error::service_validation(format!(
                    "Configuration parse failed: {e}"
                )))
            }
        }
    }

    /// Main entry point for running the application
    async fn run_application(config_path: &Path) -> Result<()> {
        bootstrap_info!("Loading config file: {:?}", config_path);

        // Load configuration file
        let config = match ActrixConfig::from_file(config_path) {
            Ok(config) => {
                bootstrap_info!("Config loaded successfully");

                // Validate configuration
                if let Err(errors) = config.validate() {
                    bootstrap_error!("Configuration validation found issues:");
                    let mut has_critical_errors = false;
                    for (i, err) in errors.iter().enumerate() {
                        if err.starts_with("Warning:") {
                            bootstrap_info!("  {}. ⚠️  {}", i + 1, err);
                        } else {
                            bootstrap_error!("  {}. ❌ {}", i + 1, err);
                            has_critical_errors = true;
                        }
                    }
                    if has_critical_errors {
                        return Err(Error::custom(
                            "Configuration validation failed, please fix the above errors"
                                .to_string(),
                        ));
                    }
                }

                config
            }
            Err(e) => {
                bootstrap_error!("Config load failed: {}", e);
                return Err(Error::custom(format!("Configuration load failed: {e}")));
            }
        };

        // ensure sqlite_path directory exists
        if !config.sqlite_path.exists() {
            std::fs::create_dir_all(&config.sqlite_path).with_context(|| {
                format!(
                    "Failed to create SQLite data directory: {}",
                    config.sqlite_path.display()
                )
            })?;
        }

        // Initialize recording pipeline (logging + tracing)
        let _recording_pipeline_guard = init_recording_pipeline(&config)?;

        // Write PID file (before binding ports, requires privileges)
        let pid_path = process::ProcessManager::write_pid_file(config.get_pid_path().as_deref())?;
        let _pid_guard = process::PidFileGuard::new(pid_path);

        // Clone config before creating services, as the service may need root privileges to bind ports
        let user = config.user.clone();
        let group = config.group.clone();

        // Run services
        Self::run_services_with_privilege_drop(config, user, group, config_path.to_path_buf()).await
    }

    /// Run services and switch user privileges at the appropriate time
    async fn run_services_with_privilege_drop(
        config: ActrixConfig,
        user: Option<String>,
        group: Option<String>,
        config_path: PathBuf,
    ) -> Result<()> {
        platform::recording::info!("Starting WebRTC auxiliary server cluster");

        // First initialize the database,
        // ensure it is ready before any service that may access it starts
        platform::storage::db::set_db_path(&config.sqlite_path)
            .await
            .map_err(|e| Error::custom(format!("Database initialization failed: {e}")))?;
        platform::recording::info!("Database initialization complete");

        // Initialize global shutdown channel (shared across all services)
        let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(10);

        // Install Ctrl-C handler to ensure shutdown can be broadcast at any stage
        setup_ctrl_c_handler(shutdown_tx.clone()).await;

        // If KS is enabled, build gRPC service future
        let mut handle_futs: Vec<JoinHandle<()>> = Vec::new();

        let mut service_manager =
            Self::create_service_manager(config.clone(), shutdown_tx.clone(), config_path).await?;

        let handle_futures = service_manager.start_all().await?;
        handle_futs.extend(handle_futures);
        platform::recording::info!("Starting all services...");

        // After port binding is complete, switch user and group
        platform::recording::info!(
            "Service startup complete, preparing to switch user privileges..."
        );
        if let Err(e) = process::ProcessManager::drop_privileges(user.as_deref(), group.as_deref())
        {
            platform::recording::error!("Failed to drop privileges: {}", e);
            // Continue running, but log the error
        }

        // Display service info
        Self::display_service_info(&config);

        // Setup SIGHUP handler for hot reload
        let mut sighup_rx = setup_sighup_channel();

        // Main event loop: monitor services, SIGHUP, and shutdown
        let mut shutdown_rx = shutdown_tx.subscribe();
        let mut pending: FuturesUnordered<_> = handle_futs.into_iter().collect();
        loop {
            tokio::select! {
                Some(result) = pending.next() => {
                    if let Err(e) = result {
                        platform::recording::error!("Service task terminated unexpectedly: {}", e);
                        let _ = shutdown_tx.send(());
                        break;
                    }
                    // A task completed normally (e.g. shutdown in progress).
                    // If all tasks are done, next() returns None and the loop
                    // exits via the else branch.
                }
                Some(()) = sighup_rx.recv() => {
                    platform::recording::info!("Received SIGHUP, initiating configuration reload...");
                    if let Err(e) = service_manager.reload().await {
                        platform::recording::error!(
                            "Configuration reload failed: {}. Continuing with previous config.",
                            e
                        );
                    }
                }
                _ = shutdown_rx.recv() => {
                    platform::recording::info!("Shutdown signal received, exiting main loop");
                    break;
                }
                else => break,
            }
        }
        service_manager.stop_all().await?;

        platform::recording::info!("All services safely shut down");
        Ok(())
    }

    /// Create service manager
    async fn create_service_manager(
        config: ActrixConfig,
        shutdown_tx: tokio::sync::broadcast::Sender<()>,
        config_path: PathBuf,
    ) -> Result<ServiceManager> {
        platform::recording::info!("Planned services:");
        // Database is initialized early in run_services_with_privilege_drop
        // to ensure the control gRPC header can safely handle RPC callbacks

        // Initialize Prometheus metrics registry
        let registry = &platform::metrics::REGISTRY;
        if let Err(e) = platform::metrics::register_metrics() {
            platform::recording::warn!(
                "Prometheus metrics registration warning (may already be registered): {}",
                e
            );
        }

        // Register metrics for each service
        if config.is_signer_enabled()
            && let Err(e) = signer::register_signer_metrics(registry)
        {
            platform::recording::warn!(
                "KS metrics registration warning (may already be registered): {}",
                e
            );
        }

        platform::recording::info!("Prometheus metrics registry initialized successfully");

        let mut service_manager =
            ServiceManager::new(config.clone(), shutdown_tx.clone(), config_path);
        // Add ICE services - fine-grained control over STUN and TURN
        if config.is_ice_enabled() {
            if config.is_turn_enabled() {
                platform::recording::info!("  - TURN Server (UDP, with built-in STUN support)");
                let turn_service = TurnService::new(config.clone());
                service_manager.add_service(ServiceContainer::turn(turn_service));
            } else if config.is_stun_enabled() {
                platform::recording::info!("  - STUN Server (UDP)");
                let stun_service = StunService::new(config.clone());
                service_manager.add_service(ServiceContainer::stun(stun_service));
            }
        } else {
            platform::recording::info!("ICE service (STUN/TURN) is disabled");
        }

        // Add HTTP route services - each service controlled independently
        if config.is_signaling_enabled() {
            platform::recording::info!("  - Signaling WebSocket Service (/signaling)");
            let signaling_service =
                SignalingService::new(config.clone(), tokio_util::sync::CancellationToken::new());
            service_manager.add_service(ServiceContainer::signaling(signaling_service));
        }

        if config.is_ais_enabled() {
            platform::recording::info!("  - AIS Service (/ais)");
            let ais_service = AisService::new(&config);
            service_manager.add_service(ServiceContainer::ais(ais_service));
        }

        if config.is_signer_enabled() {
            platform::recording::info!("  - Signer Service (gRPC: /signer.v1.Signer/<Method>)");
        }

        platform::recording::info!(
            "  - Control Service (admin_ui={}, grpc_api={})",
            config.admin_ui_enabled(),
            config.grpc_api_enabled()
        );

        Ok(service_manager)
    }

    /// Display service info
    fn display_service_info(config: &ActrixConfig) {
        // Determine which URLs are available
        let mut urls = Vec::new();

        if let Some(ref http_config) = config.bind.http {
            let scheme = http_config.scheme();
            let ws_scheme = http_config.ws_scheme();
            let protocol = if http_config.is_tls() {
                "HTTPS"
            } else {
                "HTTP"
            };
            let http_url = format!("{}://{}:{}", scheme, http_config.ip, http_config.port);
            let ws_url = format!("{}://{}:{}", ws_scheme, http_config.ip, http_config.port);
            urls.push((protocol, http_url, ws_url));
        }

        platform::recording::info!("All services started");

        if !urls.is_empty() {
            for (protocol, http_url, _ws_url) in &urls {
                platform::recording::info!("{} server listening on: {}", protocol, http_url);
                platform::recording::info!("Available API endpoints:");
                if config.is_signaling_enabled() {
                    platform::recording::info!("  - {}/signaling/ws", _ws_url);
                }
                if config.is_signer_enabled() {
                    platform::recording::info!("  - {}/signer.v1.Signer/<Method>", http_url);
                }
                if config.is_ais_enabled() {
                    platform::recording::info!("  - {}/ais/health", http_url);
                    platform::recording::info!("  - {}/ais/register (POST protobuf)", http_url);
                }
                if config.admin_ui_enabled() {
                    platform::recording::info!("  - {}/admin", http_url);
                    platform::recording::info!("  - {}/admin/health", http_url);
                }
                if config.grpc_api_enabled() {
                    platform::recording::info!(
                        "  - {}/admin.v1.NodeAdminService/<Method>",
                        http_url
                    );
                }
            }
        } else {
            platform::recording::info!("No HTTP/HTTPS server configured");
        }

        // KS gRPC reuses the main HTTP listener port, no standalone gRPC port.
    }
}

/// Set up Ctrl-C signal handler
async fn setup_ctrl_c_handler(shutdown_tx: tokio::sync::broadcast::Sender<()>) {
    tokio::spawn(async move {
        if let Err(e) = tokio::signal::ctrl_c().await {
            platform::recording::error!("Failed to listen for Ctrl-C signal: {}", e);
            return;
        }
        platform::recording::info!("Ctrl-C signal received, starting graceful shutdown...");
        let _ = shutdown_tx.send(());
    });
}

/// Setup SIGHUP signal handler for hot-reload.
///
/// Returns an mpsc receiver that yields `()` each time SIGHUP is received.
#[cfg(unix)]
fn setup_sighup_channel() -> mpsc::Receiver<()> {
    use tokio::signal::unix::{SignalKind, signal};

    let (tx, rx) = mpsc::channel::<()>(1);
    tokio::spawn(async move {
        let mut sighup = signal(SignalKind::hangup()).expect("Failed to install SIGHUP handler");
        loop {
            sighup.recv().await;
            if tx.send(()).await.is_err() {
                break;
            }
        }
    });
    rx
}

/// On non-Unix platforms, SIGHUP is not available. Return a receiver that
/// never yields.
#[cfg(not(unix))]
fn setup_sighup_channel() -> mpsc::Receiver<()> {
    let (_tx, rx) = mpsc::channel::<()>(1);
    rx
}
