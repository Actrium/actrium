//! Deploy helper for actor-rtc auxiliary services

use anyhow::Result;
use clap::Parser;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

mod cli;
mod config;
mod docker;
mod menu;
mod services;
mod system;
mod template;

use cli::{Cli, Commands};
use config::{InstallConfig, UnifiedConfigWizard};
use menu::{MenuApplication, framework::screen::Screen};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Set up Ctrl+C handler for menu mode only
    let interrupted = Arc::new(AtomicBool::new(false));
    if matches!(&cli.command, Some(Commands::Menu) | None) {
        let interrupted_clone = interrupted.clone();
        ctrlc::set_handler(move || {
            interrupted_clone.store(true, Ordering::SeqCst);
        })?;
    }

    // Clear screen for better initial presentation (except for non-interactive commands)
    match &cli.command {
        Some(Commands::Menu) | None => {
            // Clear screen for interactive menu
            Screen::clear();
        }
        _ => {} // Don't clear for CLI commands
    }

    match cli.command {
        Some(Commands::Config { output }) => {
            let mut wizard = UnifiedConfigWizard::new(cli.debug);
            if let Some(config_path) = output {
                // If output path is specified via CLI, use it directly (skip interactive selection)
                // TODO: Add method to wizard to use specific path
                println!("CLI-specified config path not yet implemented, using interactive mode");
            }
            let _config_path = wizard.run()?;
            Ok(())
        }
        Some(Commands::Deps) => system::check_dependencies(),
        Some(Commands::Install {
            install_dir,
            binary_name,
            no_path,
        }) => {
            let install_config = InstallConfig {
                install_dir,
                binary_name,
                add_to_path: !no_path,
            };
            system::install_application(&install_config)
        }
        Some(Commands::Service) => system::install_systemd_service(),
        Some(Commands::Uninstall) => system::uninstall_application(),
        Some(Commands::Docker {
            config,
            output,
            run,
            legacy,
        }) => {
            // 检查 Docker 是否可用
            if run {
                if !docker::check_docker_available(legacy).await? {
                    let cmd = if legacy {
                        "docker-compose"
                    } else {
                        "docker compose"
                    };
                    anyhow::bail!("{} 命令不可用，请先安装 Docker", cmd);
                }
            }

            // 生成 docker-compose.yml
            println!("📝 从配置文件生成 Docker Compose 配置...");
            let generator = docker::DockerComposeGenerator::from_config_file(&config)?;
            generator.save_to_file(&output)?;

            // 可选执行 docker-compose up
            if run {
                docker::docker_compose_up(&output, legacy).await?;
            } else {
                println!("\n💡 提示：使用以下命令启动服务：");
                if legacy {
                    println!("   docker-compose -f {} up -d", output.display());
                } else {
                    println!("   docker compose -f {} up -d", output.display());
                }
            }

            Ok(())
        }
        Some(Commands::Menu) | None => {
            let mut app = MenuApplication::new(cli.debug, interrupted);
            app.run()
        }
    }
}
