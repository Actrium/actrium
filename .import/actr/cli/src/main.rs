//! ACTR-CLI entry point.

use actr_cli::cli::{Cli, Commands};
use anyhow::Result;
use clap::Parser;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[cfg(windows)]
const WINDOWS_RUNTIME_STACK_SIZE: usize = 8 * 1024 * 1024;

#[cfg(windows)]
fn main() -> Result<()> {
    // The PE default reserves only 1 MiB for the main thread. WebRTC signaling
    // initialization can exceed that on Windows, so run the CLI runtime on a
    // deliberately sized thread instead.
    std::thread::Builder::new()
        .name("actr-cli-runtime".to_owned())
        .stack_size(WINDOWS_RUNTIME_STACK_SIZE)
        .spawn(run)
        .map_err(|error| anyhow::anyhow!("failed to start ACTR CLI runtime thread: {error}"))?
        .join()
        .map_err(|_| anyhow::anyhow!("ACTR CLI runtime thread panicked"))?
}

#[cfg(not(windows))]
fn main() -> Result<()> {
    run()
}

#[tokio::main]
async fn run() -> Result<()> {
    let cli = Cli::parse();

    // Skip CLI-level tracing init in detached child mode — the runtime
    // sets up its own tracing subscriber via init_observability().
    // If we init here first, the global subscriber with "off" filter
    // prevents init_observability()'s try_init() from succeeding,
    // and all runtime logs are silently dropped.
    let is_detached_child = matches!(
        &cli.command,
        Some(Commands::Run(command)) if command.internal_detached_child
    );
    // `actr run` also initializes the workload's own observability via
    // init_observability(). Skip CLI init for it too: a pre-set "off" global
    // subscriber would make init_observability()'s try_init() fail silently and
    // drop all workload logs/OTLP.
    let is_run = matches!(&cli.command, Some(Commands::Run(_)));
    if !is_detached_child && !is_run {
        init_tracing();
    }
    actr_cli::cli::run_with(cli).await
}

fn init_tracing() {
    let layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_level(true)
        .with_line_number(true)
        .with_file(true);
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("off"));
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(layer)
        .try_init();
}
