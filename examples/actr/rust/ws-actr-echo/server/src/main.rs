//! Relay echo service. Peer sessions use the SDK outbound Loran relay path.
//! No application WebSocket listener or advertised peer address is required.

mod echo_service;
mod generated;

use echo_service::EchoService;
use generated::echo_service_actor::EchoServiceWorkload;

use actr_runtime::prelude::*;
use std::path::PathBuf;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 1. loadconfig
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    let config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("actr.toml");
    let config = actr_config::ConfigParser::from_file(&config_path)?;

    // initialize[...] (log/[...])
    let _obs_guard = actr_runtime::init_observability(&config.observability)?;

    info!("🚀 WS Echo Server start");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("Peer relay is discovered from the configured Loran signaling endpoint");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 2. create workload
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    info!("📦 create EchoService...");

    let echo_service = EchoService::new();
    let workload = EchoServiceWorkload::new(echo_service);
    let node = match unimplemented!(
        "source-defined workload examples were removed; migrate this example to a package-backed host"
    ) {
        Ok(node) => node,
        Err(e) => {
            error!("❌ ActrNode createfailed: {:?}", e);
            return Err(e.into());
        }
    };

    info!("✅ EchoService workload created");

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 3. start ActrNode (register with signaling and connect outbound to relay)
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    info!("🚀 start ActrNode...");
    info!("   Peer relay requires no application listener");

    let actr_ref = match node.start().await {
        Ok(actr) => actr,
        Err(e) => {
            error!("❌ ActrNode startfailed: {:?}", e);
            error!("💡 tip/hint: please ensure signaling-server already[...] realm configured");
            return Err(e.into());
        }
    };

    info!("✅ ActrNode startsuccess!");
    info!("🆔 Server ID: {:?}", actr_ref.actor_id());
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("🎉 WS Echo Server alreadyfully/completelystart[...]register");
    info!("Outbound relay discovery enabled");
    info!("Waiting for peer RPC requests");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 5. waiting/wait for Ctrl+C [...]close/shutdown
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    actr_ref.wait_for_ctrl_c_and_shutdown().await?;

    info!("✅ WS Echo Server closed");

    Ok(())
}
