//! Command line argument parsing

use clap::Parser;

use super::Commands;

/// Deployment bootstrap helper for loran WebRTC services
#[derive(Parser)]
#[command(name = "deploy")]
#[command(version)]
#[command(about = "Deployment bootstrap helper for loran WebRTC services")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Enable debug mode
    #[arg(long, global = true)]
    pub debug: bool,
}
