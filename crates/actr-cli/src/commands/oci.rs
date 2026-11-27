//! Local administration for the rootless OCI backend.

use std::path::PathBuf;

use anyhow::{Result, bail};
use async_trait::async_trait;
use clap::{Args, Subcommand};

use crate::core::{Command, CommandContext, CommandResult, ComponentType};

#[derive(Args, Debug)]
pub struct OciArgs {
    #[command(subcommand)]
    pub command: OciCommand,
}

#[derive(Subcommand, Debug)]
pub enum OciCommand {
    /// Check the configured OCI tools, filesystem and cgroup delegation.
    Doctor {
        #[arg(short, long, default_value = "actr.toml")]
        config: PathBuf,
    },
    /// Reconcile abandoned containers and evict unused OCI images.
    Gc {
        #[arg(short, long, default_value = "actr.toml")]
        config: PathBuf,
    },
}

#[async_trait]
impl Command for OciArgs {
    async fn execute(&self, _: &CommandContext) -> Result<CommandResult> {
        if !cfg!(feature = "oci-engine") {
            bail!("OCI requires the oci-engine feature");
        }
        #[cfg(not(all(
            feature = "oci-engine",
            target_os = "linux",
            any(target_arch = "x86_64", target_arch = "aarch64")
        )))]
        bail!("OCI requires Linux x86_64 or aarch64");

        #[cfg(all(
            feature = "oci-engine",
            target_os = "linux",
            any(target_arch = "x86_64", target_arch = "aarch64")
        ))]
        {
            use anyhow::Context as _;
            use std::path::Path;

            let file = match &self.command {
                OciCommand::Doctor { config } | OciCommand::Gc { config } => config,
            };
            let file = std::fs::canonicalize(file)?;
            let raw: actr_config::RuntimeRawConfig = std::fs::read_to_string(&file)?.parse()?;
            let base = file.parent().unwrap_or(Path::new("/"));
            let config = raw
                .hyper
                .and_then(|section| section.oci)
                .context("OCI requires [hyper.oci] host configuration")?
                .resolve_paths(base);
            let output = match &self.command {
                OciCommand::Doctor { .. } => {
                    actr_hyper::oci::doctor(&config).await?;
                    "OCI host checks passed".into()
                }
                OciCommand::Gc { .. } => {
                    let (instances, images) = actr_hyper::oci::gc(&config).await?;
                    format!("Reconciled {instances} OCI instances and evicted {images} images")
                }
            };
            Ok(CommandResult::Success(output))
        }
    }

    fn required_components(&self) -> Vec<ComponentType> {
        Vec::new()
    }

    fn name(&self) -> &str {
        "oci"
    }

    fn description(&self) -> &str {
        "Inspect and reconcile the OCI execution environment"
    }
}
