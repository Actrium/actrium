//! Systemd service template processing

use anyhow::Result;
use std::collections::HashMap;
use std::process::Command;

use crate::config::InstallConfig;

/// Systemd service template processor
pub struct SystemdServiceTemplate {
    install_config: InstallConfig,
    config_path: std::path::PathBuf,
}

impl SystemdServiceTemplate {
    pub fn new(install_config: InstallConfig, config_path: std::path::PathBuf) -> Self {
        Self {
            install_config,
            config_path,
        }
    }

    /// Generate systemd service file
    pub fn generate_service_file(&self, service_user: &str, service_group: &str) -> Result<()> {
        let service_name = &self.install_config.binary_name;
        let service_file = format!("/etc/systemd/system/{}.service", service_name);

        println!("📄 Creating systemd service: {}", service_name);

        // Create service content
        let service_content = self.create_service_content(service_user, service_group)?;

        // Write service file using sudo
        self.write_service_file(&service_content, &service_file)?;

        // Reload systemd daemon
        self.reload_systemd()?;

        // Enable service
        self.enable_service(service_name)?;

        // Start service
        self.start_service(service_name)?;

        // Show service status
        self.show_service_status(service_name)?;

        println!(
            "✅ Systemd service '{}' deployed successfully",
            service_name
        );
        println!("   • Service file: {}", service_file);
        println!("   • Status: systemctl status {}", service_name);
        println!("   • Logs: journalctl -u {} -f", service_name);

        Ok(())
    }

    fn create_service_content(&self, service_user: &str, service_group: &str) -> Result<String> {
        let template = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tpl/actrix.service"));

        let install_dir_str = self.install_config.install_dir.to_string_lossy();
        let config_path_str = self.config_path.to_string_lossy();

        let mut placeholders = HashMap::new();
        placeholders.insert("SERVICE_USER", service_user);
        placeholders.insert("SERVICE_GROUP", service_group);
        placeholders.insert("INSTALL_DIR", &install_dir_str);
        placeholders.insert("CONFIG_PATH", &config_path_str);

        let mut result = template.to_string();
        for (key, value) in placeholders {
            let placeholder = format!("{{{{{}}}}}", key);
            result = result.replace(&placeholder, value);
        }

        Ok(result)
    }

    fn write_service_file(&self, content: &str, service_file: &str) -> Result<()> {
        let mut output = Command::new("sudo")
            .arg("tee")
            .arg(service_file)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        if let Some(ref mut stdin) = output.stdin {
            use std::io::Write;
            stdin.write_all(content.as_bytes())?;
        }

        let result = output.wait_with_output()?;
        if !result.status.success() {
            let error = String::from_utf8_lossy(&result.stderr);
            anyhow::bail!("Failed to write service file: {}", error);
        }

        println!("✅ Service file created: {}", service_file);
        Ok(())
    }

    fn reload_systemd(&self) -> Result<()> {
        println!("🔄 Reloading systemd daemon...");
        let output = Command::new("sudo")
            .args(["systemctl", "daemon-reload"])
            .output()?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to reload systemd: {}", error);
        }

        println!("✅ Systemd daemon reloaded");
        Ok(())
    }

    fn enable_service(&self, service_name: &str) -> Result<()> {
        println!("⚡ Enabling service for auto-start...");
        let output = Command::new("sudo")
            .args(["systemctl", "enable", service_name])
            .output()?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to enable service: {}", error);
        }

        println!("✅ Service enabled for auto-start");
        Ok(())
    }

    fn start_service(&self, service_name: &str) -> Result<()> {
        println!("🚀 Starting service...");
        let output = Command::new("sudo")
            .args(["systemctl", "start", service_name])
            .output()?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to start service: {}", error);
        }

        // Check if service is actually running
        let status_output = Command::new("systemctl")
            .args(["is-active", service_name])
            .output()?;

        if status_output.status.success() {
            let status_str = String::from_utf8_lossy(&status_output.stdout);
            let status = status_str.trim();
            if status == "active" {
                println!("✅ Service started successfully");
            } else {
                println!("⚠️  Service status: {}", status);
            }
        }

        Ok(())
    }

    fn show_service_status(&self, service_name: &str) -> Result<()> {
        println!();
        println!("📊 Service Status");
        println!("════════════════");

        let output = Command::new("systemctl")
            .args(["status", service_name, "--no-pager", "--lines=10"])
            .output()?;

        if output.status.success() {
            let status_output = String::from_utf8_lossy(&output.stdout);
            println!("{}", status_output);
        } else {
            let error = String::from_utf8_lossy(&output.stderr);
            println!("⚠️  Failed to get service status: {}", error);
        }

        Ok(())
    }
}
