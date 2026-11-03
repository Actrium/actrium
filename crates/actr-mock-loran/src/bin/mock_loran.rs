//! Standalone `mock-loran` binary — starts a mock loran server on a given
//! port and blocks until Ctrl+C. Used by `packages/actr-web/examples/echo`'s
//! `start-mock.sh` to run integration e2e tests without a real loran.

use actr_mock_loran::MockLoranServer;
use clap::Parser;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(
    author,
    version,
    about = "Mock loran server for Actor-RTC integration tests"
)]
struct Args {
    /// TCP port to bind. Defaults to 8081 to match the echo e2e config.
    #[arg(long, default_value_t = 8081)]
    port: u16,

    /// Bind address (default 0.0.0.0 for accessibility from emulators).
    #[arg(long, default_value = "0.0.0.0")]
    bind: String,

    /// Log filter (defaults to `info` for mock-loran, `warn` elsewhere).
    #[arg(long, default_value = "actr_mock_loran=info,info")]
    log: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(&args.log))
        .with_target(true)
        .init();

    let listener = TcpListener::bind(format!("{}:{}", args.bind, args.port)).await?;
    let server = MockLoranServer::start_with_listener(listener).await?;

    // This log line is the readiness marker scripts grep for.
    println!("mock-loran listening on 127.0.0.1:{}", server.port());
    println!("  http: {}", server.http_url());
    println!("  ws:   {}", server.ws_url());

    tokio::signal::ctrl_c().await?;
    println!("mock-loran shutting down");
    Ok(())
}
