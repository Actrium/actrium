//! Application installation utilities

use anyhow::Result;
use dialoguer::{Confirm, Input, theme::ColorfulTheme};
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{SystemProvider, SystemProviderFactory};
use crate::config::InstallConfig;
use crate::template::SystemdServiceTemplate;

/// Install application files to system directories
pub fn install_application(config: &InstallConfig) -> Result<()> {
    // Detect system provider
    let provider = SystemProviderFactory::detect()?;

    println!("Creating directory structure...");

    // Create installation directories
    let directories = config.all_directories();
    for dir in &directories {
        provider.create_directory(dir, Some(0o755))?;
    }

    println!("✅ Directory structure created successfully");

    // Find and copy actrix binary
    println!("Locating actrix binary...");
    let source_binary = find_actrix_binary()?;
    println!("📦 Found source binary: {}", source_binary.display());

    let target_binary = config.binary_path();
    provider.copy_file(&source_binary, &target_binary)?;

    // Make binary executable
    provider.set_file_permissions(&target_binary, 0o755)?;
    println!("✅ Auxes binary installed to {}", target_binary.display());

    // Add symlink to PATH if requested
    if config.add_to_path {
        add_to_path(provider.as_ref(), &target_binary, &config.symlink_path())?;
    }

    println!();
    println!("📁 Installation directories:");
    for dir in &directories {
        println!("  - {}", dir.display());
    }
    println!();
    println!("✅ Application installation completed");

    Ok(())
}

/// Deploy application as systemd service
pub fn install_systemd_service() -> Result<()> {
    // Detect system provider
    let provider = SystemProviderFactory::detect()?;

    if !provider.has_systemd() {
        anyhow::bail!("systemd is not available on this system");
    }

    println!("✅ systemd is available");
    println!();

    // Configure configuration file path
    let config_path = configure_config_path()?;

    // Configure systemd service
    let install_config = configure_service_settings()?;
    let (service_user, service_group) = configure_service_user()?;

    // Verify critical files exist before creating service
    verify_deployment_files(&install_config, &config_path)?;

    // Create systemd service
    let service_template = SystemdServiceTemplate::new(install_config, config_path);
    service_template.generate_service_file(&service_user, &service_group)?;

    Ok(())
}

/// Configure configuration file path
fn configure_config_path() -> Result<PathBuf> {
    let theme = ColorfulTheme::default();

    println!("📁 Configuration File Path");
    println!("══════════════════════════");
    println!("Specify the configuration file path for the service:");
    println!();

    let config_path: String = Input::with_theme(&theme)
        .with_prompt("Configuration file path")
        .default("/etc/actor-rtc-actrix/config.toml".to_string())
        .interact_text()?;

    let path = PathBuf::from(config_path);

    // Check if file exists
    if !path.exists() {
        println!("⚠️  Configuration file does not exist: {}", path.display());
        let confirm = Confirm::with_theme(&theme)
            .with_prompt("Continue with deployment? (you'll need to create the config later)")
            .default(true)
            .interact()?;

        if !confirm {
            anyhow::bail!("Deployment cancelled - configuration file required");
        }
    } else {
        println!("✅ Configuration file found: {}", path.display());
    }

    println!();
    Ok(path)
}

/// Configure systemd service settings
fn configure_service_settings() -> Result<InstallConfig> {
    let theme = ColorfulTheme::default();

    println!("📋 Systemd Service Configuration");
    println!("═══════════════════════════════");
    println!("Configure service settings (press Enter for defaults):");
    println!();

    let default_config = InstallConfig::default();

    // Installation directory
    let install_dir: String = Input::with_theme(&theme)
        .with_prompt("Installation directory")
        .default(default_config.install_dir.to_string_lossy().to_string())
        .interact_text()?;

    // Binary name
    let binary_name: String = Input::with_theme(&theme)
        .with_prompt("Service/binary name")
        .default(default_config.binary_name.clone())
        .interact_text()?;

    println!();

    Ok(InstallConfig {
        install_dir: PathBuf::from(install_dir),
        binary_name,
        add_to_path: false, // Not relevant for systemd service
    })
}

/// Configure systemd service user and group
fn configure_service_user() -> Result<(String, String)> {
    let theme = ColorfulTheme::default();

    println!("👤 Service User Configuration");
    println!("════════════════════════════");
    println!();

    // Service user
    let service_user: String = Input::with_theme(&theme)
        .with_prompt("Service user")
        .default("actor-rtc".to_string())
        .interact_text()?;

    // Service group
    let service_group: String = Input::with_theme(&theme)
        .with_prompt("Service group")
        .default("actor-rtc".to_string())
        .interact_text()?;

    // Check if user exists
    if !user_exists(&service_user) {
        println!("⚠️  User '{}' does not exist", service_user);
        let create_user = Confirm::with_theme(&theme)
            .with_prompt("Create system user?")
            .default(true)
            .interact()?;

        if create_user {
            create_system_user(&service_user)?;
        } else {
            println!("❌ Service deployment requires the user to exist");
            anyhow::bail!(
                "User '{}' does not exist and creation was declined",
                service_user
            );
        }
    }

    // Check if group exists
    if !group_exists(&service_group) {
        println!("⚠️  Group '{}' does not exist", service_group);
        let create_group = Confirm::with_theme(&theme)
            .with_prompt("Create system group?")
            .default(true)
            .interact()?;

        if create_group {
            create_system_group(&service_group)?;
        } else {
            println!("❌ Service deployment requires the group to exist");
            anyhow::bail!(
                "Group '{}' does not exist and creation was declined",
                service_group
            );
        }
    }

    println!();
    Ok((service_user, service_group))
}

/// Check if a user exists
fn user_exists(username: &str) -> bool {
    Command::new("id")
        .arg(username)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Check if a group exists  
fn group_exists(groupname: &str) -> bool {
    Command::new("getent")
        .args(["group", groupname])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Create a system user
fn create_system_user(username: &str) -> Result<()> {
    println!("👤 Creating system user: {}", username);

    let output = Command::new("sudo")
        .args([
            "useradd",
            "--system",
            "--home-dir",
            "/opt/actor-rtc-actrix",
            "--no-create-home",
            "--shell",
            "/usr/sbin/nologin",
            "--comment",
            "actor-rtc actrix service user",
            username,
        ])
        .output()?;

    if output.status.success() {
        println!("✅ System user '{}' created", username);
    } else {
        let error = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to create user '{}': {}", username, error);
    }

    Ok(())
}

/// Create a system group
fn create_system_group(groupname: &str) -> Result<()> {
    println!("👥 Creating system group: {}", groupname);

    let output = Command::new("sudo")
        .args(["groupadd", "--system", groupname])
        .output()?;

    if output.status.success() {
        println!("✅ System group '{}' created", groupname);

        // Add user to group if both exist
        // Note: This assumes the user was just created above
        let username = "actor-rtc"; // Default user name
        if user_exists(username) {
            let output = Command::new("sudo")
                .args(["usermod", "-a", "-G", groupname, username])
                .output()?;

            if output.status.success() {
                println!("✅ User '{}' added to group '{}'", username, groupname);
            }
        }
    } else {
        let error = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to create group '{}': {}", groupname, error);
    }

    Ok(())
}

/// Find the actrix binary in the project structure
fn find_actrix_binary() -> Result<PathBuf> {
    let search_bases = [
        std::env::current_exe()?.parent().unwrap().to_path_buf(),
        std::env::current_dir()?,
    ];

    let target_paths = ["target/release/auxes", "auxes/target/release/auxes"];
    let max_steps_up = 4; // Maximum directory levels to go up

    for base_dir in &search_bases {
        for target_path in &target_paths {
            for steps_up in 0..max_steps_up {
                let mut search_dir = base_dir.clone();

                // Go up the directory tree
                for _ in 0..steps_up {
                    if let Some(parent) = search_dir.parent() {
                        search_dir = parent.to_path_buf();
                    } else {
                        break; // Can't go up anymore
                    }
                }

                let candidate = search_dir.join(target_path);
                if candidate.exists() && candidate.is_file() {
                    return Ok(candidate.canonicalize()?);
                }
            }
        }
    }

    anyhow::bail!(
        "Could not find actrix binary. Please ensure it's built with:\n  \
         cargo build --release --bin actrix"
    )
}

fn add_to_path(
    _provider: &dyn SystemProvider,
    binary_path: &Path,
    symlink_path: &Path,
) -> Result<()> {
    // Remove existing symlink if it exists
    let _ = Command::new("sudo")
        .args(["rm", "-f", &symlink_path.to_string_lossy()])
        .output();

    // Create new symlink
    let output = Command::new("sudo")
        .args([
            "ln",
            "-s",
            &binary_path.to_string_lossy(),
            &symlink_path.to_string_lossy(),
        ])
        .output()?;

    if output.status.success() {
        println!(
            "✅ Created symlink: {} -> {}",
            symlink_path.display(),
            binary_path.display()
        );
        println!(
            "   The '{}' command is now available in your PATH",
            symlink_path.file_name().unwrap().to_string_lossy()
        );
    } else {
        let error = String::from_utf8_lossy(&output.stderr);
        println!("⚠️  Warning: Failed to create symlink to PATH: {}", error);
        println!(
            "   You can manually add {} to your PATH",
            binary_path.display()
        );
    }

    Ok(())
}

/// Verify that critical files exist before deploying service
fn verify_deployment_files(install_config: &InstallConfig, config_path: &PathBuf) -> Result<()> {
    println!("🔍 Verifying deployment files...");
    println!();

    let mut missing_files = Vec::new();

    // Check binary file
    let binary_path = install_config.binary_path();
    if binary_path.exists() {
        println!("✅ Binary file found: {}", binary_path.display());
    } else {
        println!("❌ Binary file missing: {}", binary_path.display());
        missing_files.push(format!("Binary: {}", binary_path.display()));
    }

    // Check configuration file
    if config_path.exists() {
        println!("✅ Configuration file found: {}", config_path.display());
    } else {
        println!("❌ Configuration file missing: {}", config_path.display());
        missing_files.push(format!("Config: {}", config_path.display()));
    }

    // Check if install directory exists
    let install_dir = &install_config.install_dir;
    if install_dir.exists() {
        println!("✅ Installation directory found: {}", install_dir.display());
    } else {
        println!(
            "❌ Installation directory missing: {}",
            install_dir.display()
        );
        missing_files.push(format!("Install dir: {}", install_dir.display()));
    }

    if !missing_files.is_empty() {
        println!();
        println!("⚠️  Missing critical files for service deployment:");
        for file in &missing_files {
            println!("   • {}", file);
        }
        println!();
        println!("Suggestions:");

        if !binary_path.exists() {
            println!("   • Run application installation first to copy the binary");
            println!("   • Or build the project: cargo build --release --bin actrix");
        }

        if !config_path.exists() {
            println!("   • Run configuration wizard first to generate config.toml");
            println!("   • Or manually create the configuration file");
        }

        if !install_dir.exists() {
            println!("   • Run application installation first to create directories");
        }

        anyhow::bail!("Cannot deploy service - critical files missing");
    }

    println!("✅ All critical files verified");
    println!();
    Ok(())
}
