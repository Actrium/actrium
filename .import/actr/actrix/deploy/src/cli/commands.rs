//! CLI command definitions

use clap::Subcommand;
use std::path::PathBuf;

/// Available subcommands for the deployment helper
#[derive(Subcommand)]
pub enum Commands {
    /// Run the configuration wizard
    Config {
        /// Output configuration file path
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Check system dependencies
    Deps,
    /// Install application files
    Install {
        /// Installation directory
        #[arg(long, default_value = "/opt/actor-rtc-actrix")]
        install_dir: PathBuf,
        /// Binary name
        #[arg(long, default_value = "actrix")]
        binary_name: String,
        /// Skip creating symlink in /usr/local/bin
        #[arg(long)]
        no_path: bool,
    },
    /// Deploy systemd service
    Service,
    /// Uninstall the application
    Uninstall,
    /// Generate and optionally run Docker Compose configuration
    Docker {
        /// Path to actrix config file
        #[arg(short, long, default_value = "config.toml")]
        config: PathBuf,
        /// Output docker-compose.yml path
        #[arg(short, long, default_value = "docker-compose.yml")]
        output: PathBuf,
        /// Automatically run docker-compose up -d after generation
        #[arg(long)]
        run: bool,
        /// Use docker-compose instead of docker compose
        #[arg(long)]
        legacy: bool,
    },
    /// Run interactive menu
    Menu,
}
