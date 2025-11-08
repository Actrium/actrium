//! Application uninstallation utilities

use anyhow::Result;
use dialoguer::{Confirm, theme::ColorfulTheme};
use std::process::Command;

#[cfg(unix)]
use users::{get_group_by_name, get_user_by_name};

/// Uninstall application with selective component removal
pub fn uninstall_application() -> Result<()> {
    let theme = ColorfulTheme::default();

    println!("🔍 Checking what's installed...");

    // Check what's currently installed
    let install_dir = "/opt/actor-rtc-actrix";
    let config_dir = "/etc/actor-rtc-actrix";
    let service_file = "/etc/systemd/system/actrix.service";

    let mut components_found = Vec::new();

    if std::path::Path::new(install_dir).exists() {
        components_found.push("Application files");
    }

    if std::path::Path::new(config_dir).exists() {
        components_found.push("Configuration files");
    }

    if std::path::Path::new(service_file).exists() {
        components_found.push("Systemd service");
    }

    #[cfg(unix)]
    {
        if get_user_by_name("actor-rtc").is_some() {
            components_found.push("System user (actor-rtc)");
        }

        if get_group_by_name("actor-rtc").is_some() {
            components_found.push("System group (actor-rtc)");
        }
    }

    if components_found.is_empty() {
        println!("✅ No actor-rtc-actrix components found on this system.");
        return Ok(());
    }

    println!();
    println!("Found the following components:");
    for component in &components_found {
        println!("  📦 {}", component);
    }

    println!();

    // Selective removal
    let mut removed_count = 0;

    // 1. Stop and remove systemd service
    if std::path::Path::new(service_file).exists() {
        if Confirm::with_theme(&theme)
            .with_prompt("Remove systemd service? (This will stop the service if running)")
            .default(true)
            .interact()?
        {
            if let Err(e) = remove_systemd_service() {
                println!("⚠️  Failed to remove systemd service: {}", e);
            } else {
                removed_count += 1;
            }
        }
    }

    // 2. Remove application files
    if std::path::Path::new(install_dir).exists() {
        if Confirm::with_theme(&theme)
            .with_prompt("Remove application files? (/opt/actor-rtc-actrix)")
            .default(true)
            .interact()?
        {
            if let Err(e) = remove_directory(install_dir) {
                println!("⚠️  Failed to remove application files: {}", e);
            } else {
                println!("✅ Application files removed");
                removed_count += 1;
            }
        }
    }

    // 3. Remove configuration files (optional)
    if std::path::Path::new(config_dir).exists() {
        if Confirm::with_theme(&theme)
            .with_prompt("Remove configuration files? (/etc/actor-rtc-actrix)")
            .default(false)
            .interact()?
        {
            if let Err(e) = remove_directory(config_dir) {
                println!("⚠️  Failed to remove configuration files: {}", e);
            } else {
                println!("✅ Configuration files removed");
                removed_count += 1;
            }
        } else {
            println!("ℹ️  Configuration files preserved");
        }
    }

    // 4. Remove system user and group
    #[cfg(unix)]
    {
        let user_exists = get_user_by_name("actor-rtc").is_some();
        let group_exists = get_group_by_name("actor-rtc").is_some();

        if user_exists {
            if Confirm::with_theme(&theme)
                .with_prompt("Remove system user 'actor-rtc'?")
                .default(true)
                .interact()?
            {
                if let Err(e) = remove_user("actor-rtc") {
                    println!("⚠️  Failed to remove user: {}", e);
                } else {
                    removed_count += 1;
                }
            } else {
                println!("ℹ️  System user preserved");
            }
        }

        if group_exists {
            if Confirm::with_theme(&theme)
                .with_prompt("Remove system group 'actor-rtc'?")
                .default(true)
                .interact()?
            {
                if let Err(e) = remove_group("actor-rtc") {
                    println!("⚠️  Failed to remove group: {}", e);
                } else {
                    removed_count += 1;
                }
            } else {
                println!("ℹ️  System group preserved");
            }
        }
    }

    // Summary
    println!();
    if removed_count > 0 {
        println!(
            "🎯 Uninstallation completed! Removed {} component(s).",
            removed_count
        );
    } else {
        println!("ℹ️  No components were removed.");
    }

    Ok(())
}

fn remove_systemd_service() -> Result<()> {
    let service_name = "actrix";
    let service_file = "/etc/systemd/system/actrix.service";

    // Stop the service if running
    let _ = Command::new("sudo")
        .args(["systemctl", "stop", service_name])
        .output();

    // Disable the service
    let _ = Command::new("sudo")
        .args(["systemctl", "disable", service_name])
        .output();

    // Remove service file
    let output = Command::new("sudo")
        .args(["rm", "-f", service_file])
        .output()?;

    if !output.status.success() {
        anyhow::bail!("Failed to remove systemd service file");
    }

    // Reload systemd
    let _ = Command::new("sudo")
        .args(["systemctl", "daemon-reload"])
        .output();

    println!("✅ Systemd service removed");
    Ok(())
}

fn remove_directory(path: &str) -> Result<()> {
    let output = Command::new("sudo").args(["rm", "-rf", path]).output()?;

    if !output.status.success() {
        anyhow::bail!("Failed to remove directory: {}", path);
    }

    Ok(())
}

#[cfg(unix)]
fn remove_user(username: &str) -> Result<()> {
    let output = Command::new("sudo").args(["userdel", username]).output()?;

    if output.status.success() {
        println!("✅ User '{}' removed successfully", username);
        Ok(())
    } else {
        let error = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to remove user '{}': {}", username, error);
    }
}

#[cfg(unix)]
fn remove_group(groupname: &str) -> Result<()> {
    let output = Command::new("sudo")
        .args(["groupdel", groupname])
        .output()?;

    if output.status.success() {
        println!("✅ Group '{}' removed successfully", groupname);
        Ok(())
    } else {
        let error = String::from_utf8_lossy(&output.stderr);

        // If group doesn't exist (maybe removed when user was deleted), treat as success
        if error.contains("does not exist") {
            println!(
                "ℹ️  Group '{}' was already removed (likely when user was deleted)",
                groupname
            );
            Ok(())
        } else {
            anyhow::bail!("Failed to remove group '{}': {}", groupname, error);
        }
    }
}
