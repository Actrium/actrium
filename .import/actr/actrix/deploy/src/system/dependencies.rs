//! System dependency checking utilities

use super::check_result::{CheckItem, DependencyCheckResult};
use anyhow::Result;
use colored::*;
use std::process::Command;

/// Check system dependencies and compatibility
pub fn check_dependencies() -> Result<()> {
    let mut all_good = true;

    // Check system type
    if cfg!(unix) {
        println!("✅ System: Unix-like (Linux/macOS)");
    } else {
        println!("❌ System: Windows (not fully supported)");
        all_good = false;
    }

    // Check for systemd
    if has_systemd() {
        println!("✅ Init system: systemd");
    } else {
        println!("⚠️  Init system: non-systemd (manual service management required)");
    }

    // Check for required commands
    let required_commands = ["sudo", "mkdir", "tee"];
    for cmd in required_commands {
        if command_exists(cmd) {
            println!("✅ Command: {}", cmd);
        } else {
            println!("❌ Command: {} (missing)", cmd);
            all_good = false;
        }
    }

    // Check user management commands
    if has_user_management() {
        println!("✅ User management: useradd/groupadd available");
    } else {
        println!(
            "⚠️  User management: useradd/groupadd not available (manual user creation required)"
        );
    }

    println!();
    if all_good {
        println!(
            "{}",
            "🎉 All essential dependencies are satisfied!"
                .bright_green()
                .bold()
        );
    } else {
        println!(
            "{}",
            "⚠️  Some dependencies are missing. The tool will work with reduced functionality."
                .bright_yellow()
                .bold()
        );
    }

    Ok(())
}

pub(super) fn command_exists(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub(super) fn has_systemd() -> bool {
    command_exists("systemctl") && std::path::Path::new("/run/systemd/system").exists()
}

pub(super) fn has_user_management() -> bool {
    command_exists("useradd") && command_exists("groupadd")
}

/// Check system dependencies and return structured data
pub fn check_dependencies_data() -> Result<DependencyCheckResult> {
    let mut result = DependencyCheckResult::new();

    // Check system type
    if cfg!(unix) {
        result.add_item(CheckItem::ok("System", "Unix-like (Linux/macOS)"));
    } else {
        result.add_item(CheckItem::error("System", "Windows (not fully supported)"));
    }

    // Check for systemd
    if has_systemd() {
        result.add_item(CheckItem::ok("Init system", "systemd"));
    } else {
        result.add_item(CheckItem::warning(
            "Init system",
            "non-systemd (manual service management required)",
        ));
    }

    // Check for required commands
    let required_commands = ["sudo", "mkdir", "tee"];
    for cmd in required_commands {
        if command_exists(cmd) {
            result.add_item(CheckItem::ok("Command", cmd));
        } else {
            result.add_item(CheckItem::error("Command", &format!("{} (missing)", cmd)));
        }
    }

    // Check user management commands
    if has_user_management() {
        result.add_item(CheckItem::ok(
            "User management",
            "useradd/groupadd available",
        ));
    } else {
        result.add_item(CheckItem::warning(
            "User management",
            "useradd/groupadd not available (manual user creation required)",
        ));
    }

    Ok(result)
}
