//! Command line argument parsing

use clap::Parser;

use super::Commands;

/// Interactive deployment helper for actrix WebRTC services
#[derive(Parser)]
#[command(name = "deploy")]
#[command(about = "Interactive deployment helper for actrix WebRTC services")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Enable debug mode
    #[arg(long, global = true)]
    pub debug: bool,
}
