//! `actr registry` — remote Actor Registry interactions (Issuer / signaling).
//!
//! Subcommands:
//!   - `discover`    — find services available on the network
//!   - `publish`     — push a signed `.actr` package to the Publisher Registry
//!   - `fingerprint` — compute / verify / lock service semantic fingerprints

use std::path::PathBuf;

use anyhow::{Context, Result};
use async_trait::async_trait;
use base64::Engine;
use clap::{Args, Subcommand};
use serde::Serialize;

use super::discovery::DiscoveryCommand;
use super::fingerprint::FingerprintCommand;
use crate::core::{Command, CommandContext, CommandResult, ComponentType, ConfigRequirement};

#[derive(Args, Debug)]
pub struct RegistryArgs {
    #[command(subcommand)]
    pub command: RegistryCommand,
}

#[derive(Subcommand, Debug)]
pub enum RegistryCommand {
    /// Discover available Actor services on the network
    Discover(DiscoveryCommand),
    /// Publish a signed .actr package to the Publisher Registry
    Publish(RegistryPublishArgs),
    /// Compute / verify / lock service semantic fingerprints
    Fingerprint(FingerprintCommand),
}

#[async_trait]
impl Command for RegistryArgs {
    async fn execute(&self, ctx: &CommandContext) -> Result<CommandResult> {
        match &self.command {
            RegistryCommand::Discover(cmd) => {
                let command = DiscoveryCommand::from_args(cmd);
                {
                    let container = ctx.container.lock().unwrap();
                    container.validate(&command.required_components())?;
                }
                command.execute(ctx).await
            }
            RegistryCommand::Fingerprint(cmd) => cmd.execute(ctx).await,
            RegistryCommand::Publish(args) => {
                execute_publish(args).await?;
                Ok(CommandResult::Success(String::new()))
            }
        }
    }

    fn required_components(&self) -> Vec<ComponentType> {
        match &self.command {
            RegistryCommand::Discover(cmd) => {
                DiscoveryCommand::from_args(cmd).required_components()
            }
            RegistryCommand::Fingerprint(_) | RegistryCommand::Publish(_) => vec![],
        }
    }

    fn config_requirement(&self) -> ConfigRequirement {
        match &self.command {
            // Standalone discover (--endpoint --realm --realm-client-key) needs
            // no local project files; otherwise discover needs manifest.toml or
            // actr.toml to resolve network endpoints.
            RegistryCommand::Discover(cmd) => {
                if cmd.standalone_config().is_some() {
                    ConfigRequirement::None
                } else {
                    ConfigRequirement::RuntimeConfig
                }
            }
            RegistryCommand::Fingerprint(_) | RegistryCommand::Publish(_) => {
                ConfigRequirement::None
            }
        }
    }

    fn name(&self) -> &str {
        "registry"
    }

    fn description(&self) -> &str {
        "Interact with the remote Actor Registry (discover, publish, fingerprint)"
    }
}

// ── publish ──────────────────────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct RegistryPublishArgs {
    /// .actr package file to publish
    #[arg(long, short = 'p', value_name = "FILE")]
    pub package: PathBuf,

    /// Path to Publisher keychain JSON file
    #[arg(long, short = 'k', value_name = "FILE")]
    pub keychain: PathBuf,

    /// Publisher Registry endpoint URL (e.g. http://localhost:8081)
    #[arg(long, short = 'e', value_name = "URL")]
    pub endpoint: String,
}

#[derive(Serialize)]
struct SignablePublishBody<'a> {
    publisher: &'a str,
    name: &'a str,
    version: &'a str,
    target: &'a str,
    manifest: &'a str,
    signature: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    proto_files: Option<&'a serde_json::Value>,
    nonce: &'a str,
}

#[derive(Serialize)]
struct FinalPublishBody<'a> {
    publisher: &'a str,
    name: &'a str,
    version: &'a str,
    target: &'a str,
    manifest: &'a str,
    signature: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    proto_files: Option<&'a serde_json::Value>,
    nonce: &'a str,
    nonce_sig: &'a str,
}

async fn execute_publish(args: &RegistryPublishArgs) -> Result<()> {
    tracing::debug!("reading .actr package: {:?}", args.package);
    let package_bytes = std::fs::read(&args.package)
        .with_context(|| format!("Failed to read package: {}", args.package.display()))?;

    let manifest_str = actr_pack::read_manifest_raw(&package_bytes)
        .with_context(|| "Failed to read manifest from .actr package")?;
    let manifest = actr_pack::PackageManifest::from_toml(&manifest_str)
        .with_context(|| "Failed to parse manifest TOML")?;
    let sig_raw = actr_pack::read_signature(&package_bytes)
        .with_context(|| "Failed to read manifest.sig from .actr package")?;

    tracing::debug!(
        "loading keychain for identity verification: {:?}",
        args.keychain
    );
    let keychain_content = std::fs::read_to_string(&args.keychain)
        .with_context(|| format!("Failed to read keychain: {}", args.keychain.display()))?;
    let keychain: serde_json::Value =
        serde_json::from_str(&keychain_content).with_context(|| "Invalid keychain JSON")?;

    let kc_privkey_b64 = keychain["private_key"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Keychain missing 'private_key' field"))?;
    let kc_privkey_bytes = base64::engine::general_purpose::STANDARD
        .decode(kc_privkey_b64)
        .with_context(|| "Invalid private key in keychain")?;
    let kc_arr: [u8; 32] = kc_privkey_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("Private key must be 32 bytes"))?;
    let kc_signing_key = ed25519_dalek::SigningKey::from_bytes(&kc_arr);
    let kc_key_id = actr_pack::compute_key_id(&kc_signing_key.verifying_key().to_bytes());

    // Reject cross-publisher publishing (issue #425): a keychain that
    // declares a `publisher` binding may only publish packages for that
    // same publisher.
    crate::commands::package_build::ensure_publisher_matches(
        keychain.get("publisher").and_then(|v| v.as_str()),
        &manifest.publisher,
    )?;

    match manifest.signing_key_id {
        Some(ref manifest_key_id) if manifest_key_id == &kc_key_id => {}
        Some(ref manifest_key_id) => {
            anyhow::bail!(
                "Key mismatch: package was built with '{}' but keychain key is '{}'. \
                 Rebuild with the correct Publisher key, or use the matching keychain.",
                manifest_key_id,
                kc_key_id
            );
        }
        None => {
            anyhow::bail!(
                "Package manifest has no 'signing_key_id'. \
                 Rebuild with the latest 'actr build' to embed a signing_key_id."
            );
        }
    }

    let sig_b64 = base64::engine::general_purpose::STANDARD.encode(&sig_raw);

    println!("📦 Publishing package: {}", manifest.actr_type_str());
    println!("   publisher:    {}", manifest.publisher);
    println!("   name:         {}", manifest.name);
    println!("   version:      {}", manifest.version);
    println!("   target:       {}", manifest.binary.target);
    println!("   signing_key:  {}", kc_key_id);
    println!("✅ Identity verified (keychain matches package)");
    println!("🔐 Forwarding original signature (no re-signing)");

    let proto_files = actr_pack::read_proto_files(&package_bytes).unwrap_or_default();
    let proto_filing = if !proto_files.is_empty() {
        let proto_entries: Vec<serde_json::Value> = proto_files
            .iter()
            .map(|(name, content)| {
                serde_json::json!({
                    "name": name,
                    "content": String::from_utf8_lossy(content),
                })
            })
            .collect();
        println!("📋 Proto files for filing: {} file(s)", proto_entries.len());
        Some(serde_json::json!({ "protobufs": proto_entries }))
    } else {
        None
    };

    let endpoint = args
        .endpoint
        .trim_end_matches("/issuer")
        .trim_end_matches('/')
        .to_string();

    let base_url = endpoint.trim_end_matches('/');
    let nonce_url = format!("{}/registry/packages/nonce", base_url);
    let client = reqwest::Client::new();

    println!("🔑 Requesting publish nonce...");
    let nonce_resp = client
        .post(&nonce_url)
        .json(&serde_json::json!({ "publisher": manifest.publisher }))
        .send()
        .await
        .with_context(|| format!("Failed to request nonce from: {}", nonce_url))?;

    if !nonce_resp.status().is_success() {
        let status = nonce_resp.status();
        let body = nonce_resp.text().await.unwrap_or_default();
        anyhow::bail!("Nonce request failed (HTTP {}): {}", status, body);
    }

    let nonce_json: serde_json::Value = nonce_resp
        .json()
        .await
        .with_context(|| "Failed to parse nonce response")?;
    let nonce_b64 = nonce_json["nonce"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Nonce response missing 'nonce' field"))?
        .to_string();

    let nonce_bytes = base64::engine::general_purpose::STANDARD
        .decode(&nonce_b64)
        .with_context(|| "Invalid nonce base64 from server")?;

    let signable_body = SignablePublishBody {
        publisher: &manifest.publisher,
        name: &manifest.name,
        version: &manifest.version,
        target: &manifest.binary.target,
        manifest: &manifest_str,
        signature: &sig_b64,
        proto_files: proto_filing.as_ref(),
        nonce: &nonce_b64,
    };
    let signable_body_bytes = serde_json::to_vec(&signable_body)
        .with_context(|| "Failed to serialize signable publish body")?;

    let nonce_sig_b64 = {
        use ed25519_dalek::Signer;
        use sha2::{Digest, Sha256};

        let body_hash = hex::encode(Sha256::digest(&signable_body_bytes));
        let nonce_hex = hex::encode(&nonce_bytes);
        let payload = format!(
            "ACTR-PUBLISH-V2\npublisher={}\nmethod=POST\npath=/registry/packages/publish\nnonce={}\nbody_sha256={}",
            manifest.publisher, nonce_hex, body_hash
        );
        let sig = kc_signing_key.sign(payload.as_bytes());
        base64::engine::general_purpose::STANDARD.encode(sig.to_bytes())
    };
    println!("✅ Nonce signed (challenge-response)");

    let publish_url = format!("{}/registry/packages/publish", base_url);
    println!("📡 Publishing to: {}", publish_url);

    let publish_body_bytes = serde_json::to_vec(&FinalPublishBody {
        publisher: &manifest.publisher,
        name: &manifest.name,
        version: &manifest.version,
        target: &manifest.binary.target,
        manifest: &manifest_str,
        signature: &sig_b64,
        proto_files: proto_filing.as_ref(),
        nonce: &nonce_b64,
        nonce_sig: &nonce_sig_b64,
    })
    .with_context(|| "Failed to serialize publish request body")?;

    let resp = client
        .post(&publish_url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(publish_body_bytes)
        .send()
        .await
        .with_context(|| format!("Failed to connect to Publisher Registry: {}", publish_url))?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        eprintln!("❌ Publish failed (HTTP {})", status);
        eprintln!("   Response: {}", body);
        anyhow::bail!(
            "Publisher Registry publish failed with status {}: {}",
            status,
            body
        );
    }

    if let Ok(result) = serde_json::from_str::<serde_json::Value>(&body) {
        let type_str = result["type_str"].as_str().unwrap_or("unknown");
        let pkg_id = result["id"].as_i64().unwrap_or(0);
        println!();
        println!("✅ Package published successfully!");
        println!("   type_str:  {}", type_str);
        println!("   pkg_id:    {}", pkg_id);
        println!("   status:    active");
    } else {
        println!();
        println!("✅ Package published successfully!");
        println!("   Response: {}", body);
    }

    Ok(())
}
