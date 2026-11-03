//! Error-fixture server for binding error-mapping integration tests.
//!
//! Starts mock-actrix + a minimal echo service in-process, then writes
//! a single JSON line to stdout:
//!
//! ```json
//! {"port":PORT,"publisher_pubkey_b64":"BASE64","realm":"test@example.com"}
//! ```
//!
//! The calling test process reads that line, writes temp config files,
//! starts a binding client, and exercises the three error kinds:
//!
//! - `UnknownRoute`       — call any route the server does not handle
//! - `PermissionDenied`   — run with `--deny-all` to block every caller
//! - `TimedOut`           — call `fixture.Fixture.SlowEcho`; the handler
//!   never returns, so the caller times out
//!
//! The server blocks until stdin reaches EOF (parent process exited).

use std::io::BufRead;

use actr_config::ConfigParser;
use actr_framework::{Bytes, Context as RtContext, MessageDispatcher, Workload as RtWorkload};
use actr_hyper::Node;
use actr_mock_actrix::MockActrixServer;
use actr_protocol::{ActorResult, ActrError, DataChunk, PayloadType, RpcEnvelope};
use async_trait::async_trait;
use base64::Engine;
use clap::Parser;
use tracing::{info, warn};

// ── Workload ─────────────────────────────────────────────────────────────────

struct FixtureWorkload;

#[async_trait]
impl RtWorkload for FixtureWorkload {
    type Dispatcher = FixtureDispatcher;
}

struct FixtureDispatcher;

#[async_trait]
impl MessageDispatcher for FixtureDispatcher {
    type Workload = FixtureWorkload;

    async fn dispatch<C: RtContext>(
        _workload: &Self::Workload,
        envelope: RpcEnvelope,
        ctx: &C,
    ) -> ActorResult<Bytes> {
        match envelope.route_key.as_str() {
            "fixture.Fixture.Echo" => {
                let payload = envelope.payload.unwrap_or_default();
                info!(route = "Echo", bytes = payload.len(), "fixture: echo");
                Ok(payload)
            }
            "fixture.Fixture.SlowEcho" => {
                // Never returns — caller will time out.
                info!("fixture: SlowEcho — blocking forever (caller timeout expected)");
                std::future::pending::<ActorResult<Bytes>>().await
            }
            "fixture.Fixture.ProbePeer" => {
                let peer = ctx.caller_id().cloned().ok_or_else(|| {
                    ActrError::InvalidArgument("ProbePeer requires a caller".into())
                })?;
                let payload = envelope.payload.unwrap_or_default();
                let reply = ctx
                    .call_raw(&peer, "browser.Echo", payload.clone(), 5_000)
                    .await?;
                for (sequence, lane) in [
                    (1, PayloadType::StreamReliable),
                    (2, PayloadType::StreamLatencyFirst),
                ] {
                    ctx.send_data_chunk(
                        &actr_framework::Dest::Peer(peer.clone()),
                        DataChunk {
                            stream_id: "relay-interop".into(),
                            sequence,
                            payload: payload.clone(),
                            ..Default::default()
                        },
                        lane,
                    )
                    .await?;
                }
                Ok(reply)
            }
            route => {
                warn!(route, "fixture: unknown route");
                Err(ActrError::UnknownRoute(route.to_string()))
            }
        }
    }
}

// ── Args ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Parser)]
#[command(about = "Fixture server for binding error-mapping tests")]
struct Args {
    /// Deny all inbound calls (ACL deny-all mode for PermissionDenied tests).
    #[arg(long)]
    deny_all: bool,
    /// Register stream echo callbacks for browser relay interoperability tests.
    #[arg(long)]
    relay_test: bool,
    /// Disable peer signaling forwarding so data must use the shared relay.
    #[arg(long)]
    pause_rtc: bool,
}

// ── Main ─────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    let args = Args::parse();

    // ── 1. Start mock-actrix with the shared outbound relay ──────────────────
    let mock = MockActrixServer::start_with_relay().await?;
    if args.pause_rtc {
        mock.pause_forwarding();
    }
    let port = mock.port();
    let publisher_key = mock.publisher_signing_key();
    let publisher_pubkey_b64 =
        base64::engine::general_purpose::STANDARD.encode(publisher_key.verifying_key().as_bytes());

    info!(port, "mock-actrix ready");

    // ── 2. Write a temp manifest + runtime config ────────────────────────────
    let tmp = tempfile::tempdir()?;
    let manifest_path = tmp.path().join("manifest.toml");
    let runtime_path = tmp.path().join("actr.toml");

    // The ACL section: deny-all mode returns PermissionDenied for every call.
    //
    // `[acl]` with an empty `rules = []` array triggers the "empty rules list
    // — deny all" path in `check_acl_permission`.  Omitting the `[acl]`
    // section entirely falls back to "no ACL — allow all".
    let acl_section = if args.deny_all {
        "[acl]\nrules = []\n"
    } else {
        // Omit the `[acl]` section entirely so the runtime falls back to the
        // "no ACL configured — allow by default" path.
        ""
    };

    let manifest_toml = r#"edition = 1
exports = []

[package]
name = "ErrorFixture"
publisher = "test"
version = "0.2.0"
description = "Fixture service for error-mapping tests"
"#;

    std::fs::write(&manifest_path, manifest_toml)?;

    let runtime_toml = format!(
        r#"edition = 1

[package]
path = "{manifest_path}"

[signaling]
url = "ws://127.0.0.1:{port}/signaling/ws"

[issuer_endpoint]
url = "http://127.0.0.1:{port}/issuer"

[deployment]
realm = "test@example.com"

[hyper]
data_dir = "{data_dir}"

[discovery]
visible = true

[observability]
filter_level = "warn"
tracing_enabled = false

[webrtc]
force_relay = false
stun_urls = []
turn_urls = []

{acl_section}
[[trust]]
kind = "static"
pubkey_b64 = "{publisher_pubkey_b64}"
"#,
        manifest_path = manifest_path.display(),
        data_dir = tmp.path().join("hyper").display(),
        port = port,
        acl_section = acl_section,
        publisher_pubkey_b64 = publisher_pubkey_b64,
    );

    std::fs::write(&runtime_path, &runtime_toml)?;

    // ── 3. Start the fixture actor ───────────────────────────────────────────
    let manifest = ConfigParser::from_manifest_file(&manifest_path)?;
    let init = Node::from_config_file(&runtime_path)
        .await?
        .with_actor_type(manifest.package.actr_type.clone());
    let issuer_endpoint = init.runtime_config().issuer_endpoint.to_string();

    let attached = init.link(FixtureWorkload).await?;
    let registered = attached.register(&issuer_endpoint).await?;
    let actr_ref = registered.start().await?;

    let actor_id = actr_ref.actor_id();
    info!(?actor_id, "fixture actor ready");
    if args.relay_test {
        let context = actr_ref.app_context().await;
        let echo = context.clone();
        context
            .register_stream("relay-interop".into(), move |chunk, peer| {
                let echo = echo.clone();
                Box::pin(async move {
                    let lane = if chunk.sequence == 3 {
                        PayloadType::StreamLatencyFirst
                    } else {
                        PayloadType::StreamReliable
                    };
                    echo.send_data_chunk(&actr_framework::Dest::Peer(peer), chunk, lane)
                        .await
                })
            })
            .await?;
    }

    // ── 4. Emit readiness JSON ───────────────────────────────────────────────
    // Use println! so it goes to stdout; tracing goes to stderr.
    let ready = serde_json::json!({
        "port": port,
        "publisher_pubkey_b64": publisher_pubkey_b64,
        "realm": "test@example.com",
        "actor_id": actor_id.to_string_repr(),
        "actr_type": {
            "publisher": "test",
            "name": "ErrorFixture",
            "version": "0.2.0"
        }
    });
    println!("{}", ready);

    // ── 5. Block until stdin closes (parent exited) ──────────────────────────
    let (commands, mut incoming) = tokio::sync::mpsc::unbounded_channel();
    let input = tokio::task::spawn_blocking(move || {
        for line in std::io::stdin().lock().lines() {
            match line {
                Ok(line) => {
                    if commands.send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    while let Some(command) = incoming.recv().await {
        match command.trim() {
            "disconnect-relay" => mock.disconnect_relay_connections(),
            "resume-rtc" => mock.resume_forwarding(),
            _ => {}
        }
    }
    input.await?;

    info!("fixture: stdin closed, shutting down");
    actr_ref.shutdown();
    actr_ref.wait_for_shutdown().await;
    Ok(())
}
