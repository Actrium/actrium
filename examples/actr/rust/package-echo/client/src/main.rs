//! Package Echo Client Host — package-backed host that loads the client-guest .actr package.
//!
//! Mirrors the server host pattern:
//! 1. Load signed client-guest .actr package
//! 2. Hyper verifies and attaches the package workload
//! 3. Register with Issuer to obtain credential
//! 4. Start ActrNode
//! 5. Expose the local guest to either an interactive terminal or Actrium's
//!    loopback-only Actor gateway.

/// Generated protobuf types from echo.proto
pub mod echo {
    include!(concat!(env!("OUT_DIR"), "/echo.rs"));
}

use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;

use actr_framework::Dest;
use actr_hyper::{
    ActrRef, Hyper, HyperConfig, RegistryTrust, StaticTrust, TrustProvider, WorkloadPackage,
    init_observability,
};
use actr_protocol::{PayloadType, RpcRequest};
use anyhow::{Context, Result, anyhow, ensure};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::Engine;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tracing::{error, info};

use crate::echo::{EchoRequest, EchoResponse};

impl RpcRequest for EchoRequest {
    type Response = EchoResponse;

    fn route_key() -> &'static str {
        "echo.EchoService.Echo"
    }

    fn payload_type() -> actr_protocol::PayloadType {
        actr_protocol::PayloadType::RpcReliable
    }
}

#[derive(Clone)]
struct GatewayState {
    actr: ActrRef,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GatewayCallRequest {
    route_key: String,
    payload_base64: String,
    #[serde(default = "default_timeout_ms")]
    timeout_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GatewayCallResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload_base64: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GatewayHealthResponse {
    status: &'static str,
    actor_id: String,
}

fn default_timeout_ms() -> i64 {
    30_000
}

fn package_path() -> PathBuf {
    env::var("CLIENT_GUEST_PACKAGE_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../client-guest/dist/acme-package-echo-client-guest-0.1.0-cdylib.actr")
        })
}

fn public_key_path() -> PathBuf {
    env::var("CLIENT_GUEST_PUBLIC_KEY_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../client-guest/dist/public-key.json")
        })
}

fn load_package_public_key() -> Result<Vec<u8>> {
    let key_path = public_key_path();
    let value: Value =
        serde_json::from_reader(std::fs::File::open(&key_path).with_context(|| {
            format!(
                "Failed to read client-guest public key at {}. Run `./start.sh` to build first.",
                key_path.display(),
            )
        })?)
        .context("Failed to parse client-guest public key JSON")?;
    let public_key_b64 = value["public_key"]
        .as_str()
        .ok_or_else(|| anyhow!("client-guest public key JSON missing `public_key` field"))?;
    let public_key = base64::engine::general_purpose::STANDARD.decode(public_key_b64)?;
    ensure!(
        public_key.len() == 32,
        "client-guest public key must be exactly 32 bytes"
    );
    Ok(public_key)
}

async fn gateway_health(State(state): State<GatewayState>) -> Json<GatewayHealthResponse> {
    Json(GatewayHealthResponse {
        status: "ready",
        actor_id: actr_protocol::ActrId::to_string_repr(&state.actr.actor_id()),
    })
}

async fn gateway_call(
    State(state): State<GatewayState>,
    Json(request): Json<GatewayCallRequest>,
) -> Response {
    if request.route_key.trim().is_empty() {
        return gateway_error(StatusCode::BAD_REQUEST, "routeKey must not be empty");
    }
    if request.timeout_ms <= 0 {
        return gateway_error(StatusCode::BAD_REQUEST, "timeoutMs must be positive");
    }

    let payload = match base64::engine::general_purpose::STANDARD.decode(&request.payload_base64) {
        Ok(payload) => payload,
        Err(err) => {
            return gateway_error(
                StatusCode::BAD_REQUEST,
                format!("payloadBase64 is invalid: {err}"),
            );
        }
    };

    let context = state.actr.app_context().await;
    match context
        .call_raw(
            &Dest::Workload,
            request.route_key,
            PayloadType::RpcReliable,
            Bytes::from(payload),
            request.timeout_ms,
        )
        .await
    {
        Ok(response) => (
            StatusCode::OK,
            Json(GatewayCallResponse {
                success: true,
                payload_base64: Some(base64::engine::general_purpose::STANDARD.encode(response)),
                error: None,
            }),
        )
            .into_response(),
        Err(err) => gateway_error(StatusCode::BAD_GATEWAY, err.to_string()),
    }
}

fn gateway_error(status: StatusCode, error: impl Into<String>) -> Response {
    (
        status,
        Json(GatewayCallResponse {
            success: false,
            payload_base64: None,
            error: Some(error.into()),
        }),
    )
        .into_response()
}

async fn run_gateway(actr: ActrRef, listen: &str) -> Result<()> {
    let address: SocketAddr = listen
        .parse()
        .with_context(|| format!("invalid ECHO_HTTP_LISTEN address: {listen}"))?;
    ensure!(
        address.ip().is_loopback(),
        "the Actrium Actor gateway must listen on a loopback address"
    );

    let app = Router::new()
        .route("/health", get(gateway_health))
        .route("/v1/call", post(gateway_call))
        .with_state(GatewayState { actr: actr.clone() });
    let listener = tokio::net::TcpListener::bind(address).await?;
    info!("Actrium Actor gateway ready at http://{address}");

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            actr.shutdown();
            actr.wait_for_shutdown().await;
        })
        .await?;
    Ok(())
}

async fn run_interactive(actr_ref: ActrRef) -> Result<()> {
    println!("===== Package Echo Client =====");
    println!("Type messages to send to the echo server (type 'quit' to exit):");

    use std::io::Write;
    use tokio::io::{AsyncBufReadExt, BufReader};

    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin).lines();

    print!("> ");
    std::io::stdout().flush().unwrap();

    while let Ok(Some(line)) = reader.next_line().await {
        let line = line.trim().to_string();

        if line == "quit" || line == "exit" {
            info!("[App] User requested exit");
            break;
        }

        if line.is_empty() {
            print!("> ");
            std::io::stdout().flush().unwrap();
            continue;
        }

        info!("[App] Dispatching to local guest: {}", line);

        match actr_ref
            .call(
                EchoRequest {
                    message: line.clone(),
                },
                30_000,
            )
            .await
        {
            Ok(response) => {
                let response: EchoResponse = response;
                println!("\n[Received reply] {}", response.reply);
            }
            Err(e) => {
                error!("[App] Guest dispatch failed: {:?}", e);
                println!("\n[Error] {}", e);
            }
        }

        print!("> ");
        std::io::stdout().flush().unwrap();
    }

    actr_ref.shutdown();
    actr_ref.wait_for_shutdown().await;

    info!("Client shut down");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 1. Load WorkloadPackage and extract manifest
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    let package_path = package_path();
    let package_bytes = std::fs::read(&package_path).inspect_err(|e| {
        error!(
            "❌ Failed to read client-guest package at {:?}: {}",
            package_path, e
        );
        error!("💡 Run ./start.sh to build the client-guest package first");
    })?;
    info!(
        "📦 Loaded client-guest package: {} bytes",
        package_bytes.len()
    );
    let package = WorkloadPackage::new(package_bytes.clone());

    // Parse the PackageManifest from inside .actr (flat structure, no [package] section)
    let manifest = actr_pack::read_manifest(&package_bytes)?;
    let package_info = actr_config::PackageInfo {
        name: manifest.name.clone(),
        actr_type: actr_protocol::ActrType {
            publisher: manifest.publisher.clone(),
            name: manifest.name,
            version: manifest.version,
        },
        description: manifest.metadata.description,
        authors: vec![],
        license: manifest.metadata.license,
    };

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 2. Load runtime configuration
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    let config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("actr.toml");
    let config = actr_config::ConfigParser::from_runtime_file(&config_path, package_info)?;

    let _obs_guard = init_observability(&config.observability)?;

    info!("🚀 Package Echo Client Host starting");
    info!("📡 Signaling server: {:?}", config.signaling_url);

    let hyper_data_dir = actr_config::user_config::resolve_hyper_data_dir()?;

    let trust: Arc<dyn TrustProvider> = if env::var("TRUST_MODE")
        .map(|v| v == "production")
        .unwrap_or(false)
    {
        let issuer_endpoint =
            env::var("ISSUER_ENDPOINT").unwrap_or_else(|_| "http://localhost:8081/issuer".to_string());
        let base_endpoint = issuer_endpoint.trim_end_matches("/issuer").to_string();
        info!("🔐 Using RegistryTrust (base endpoint: {})", base_endpoint);
        Arc::new(RegistryTrust::new(base_endpoint))
    } else {
        info!("🔐 Using StaticTrust (local public key)");
        Arc::new(StaticTrust::new(load_package_public_key()?).context("invalid pubkey")?)
    };

    let hyper = Hyper::new(HyperConfig::new(&hyper_data_dir, trust))
        .await
        .inspect_err(|e| {
            error!("❌ Hyper initialization failed: {:?}", e);
        })?;
    info!(
        "✅ Hyper initialized, data_dir={}",
        hyper_data_dir.display()
    );

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 3. Attach package workload
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    info!("📦 Attaching client-guest package workload...");
    let attached = actr_hyper::Node::from_hyper(hyper, config)
        .attach(&package)
        .await
        .inspect_err(|e| error!("❌ Node attach failed: {:?}", e))?;

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 4. Register with Issuer
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    let issuer_endpoint =
        env::var("ISSUER_ENDPOINT").unwrap_or_else(|_| "http://localhost:8081/issuer".to_string());
    info!("🔐 Registering with Issuer at {}", issuer_endpoint);

    let registered = attached
        .register(&issuer_endpoint)
        .await
        .inspect_err(|e| error!("❌ Issuer registration failed: {:?}", e))?;
    info!("✅ Issuer registration successful");

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 5. Start ActrNode
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    info!("🚀 Starting ActrNode...");
    let actr_ref = registered.start().await.inspect_err(|e| {
        error!("❌ ActrNode start failed: {:?}", e);
    })?;
    info!(
        "✅ ActrNode started with ID: {}",
        actr_protocol::ActrId::to_string_repr(&actr_ref.actor_id())
    );

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 6. Select the Host boundary.
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    if let Ok(listen) = env::var("ECHO_HTTP_LISTEN") {
        run_gateway(actr_ref, &listen).await
    } else {
        run_interactive(actr_ref).await
    }
}
