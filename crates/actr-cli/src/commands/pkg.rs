//! `actr pkg` — local package operations (pack, sign, verify, keygen).
//!
//! ## Subcommands
//!
//! ```text
//! actr pkg pack     [--manifest-path FILE] [--payload-root DIR] --output FILE
//! actr pkg sign     [--manifest-path FILE] [--key FILE] [--binary FILE]
//! actr pkg verify   --package FILE [--pubkey FILE]
//! actr pkg keygen   [--output FILE] [--no-register] [--public-key-output FILE]
//! ```
//!
//! End-to-end build + package is a single top-level command: `actr build`.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use async_trait::async_trait;
use base64::Engine;
use clap::{Args, Subcommand};
use ed25519_dalek::SigningKey;

use crate::commands::package_build::{
    binary_kind_for_target, ensure_publisher_matches, load_signing_key_with_publisher,
    load_verifying_key, load_verifying_key_from_dev_key, resolve_key_path, validated_packaged_lock,
};
use crate::core::{Command, CommandContext, CommandResult, ComponentType};

#[derive(Args, Debug)]
pub struct PkgArgs {
    #[command(subcommand)]
    pub command: PkgCommand,
}

#[derive(Subcommand, Debug)]
pub enum PkgCommand {
    /// Pack and sign one actr-package/2 archive from prebuilt payloads.
    Pack(PkgPackArgs),
    /// Write and sign a flat package manifest from a workload project configuration.
    Sign(PkgSignArgs),
    /// Verify the signature and contents of one .actr archive.
    ///
    /// The result covers this archive and its applicable Workload Profile checks.
    /// Product v1 Service manifests supported here omit [workspace].
    Verify(PkgVerifyArgs),
    /// Generate an Ed25519 Publisher signing key pair.
    Keygen(PkgKeygenArgs),
}

#[derive(Args, Debug)]
pub struct PkgPackArgs {
    /// Path to a complete actr-package/2 package manifest.
    ///
    /// Include [signature] and each file's size and SHA-256 in [[files]].
    /// Packing recomputes file sizes, hashes, and signature.key-id.
    /// Product v1 Service manifests supported here omit [workspace].
    #[arg(
        long = "manifest-path",
        short = 'm',
        default_value = "manifest.toml",
        value_name = "FILE"
    )]
    pub manifest_path: PathBuf,

    /// Directory containing the payloads declared in [[files]].
    #[arg(long, default_value = ".", value_name = "DIR")]
    pub payload_root: PathBuf,

    /// Path to Publisher signing key file (overrides config publisher.keychain).
    #[arg(long, short = 'k', value_name = "FILE")]
    pub key: Option<PathBuf>,

    /// Output .actr package path.
    #[arg(long, short = 'o', value_name = "FILE")]
    pub output: PathBuf,
}

#[derive(Args, Debug)]
pub struct PkgSignArgs {
    /// Path to a workload project manifest.toml containing [package].
    ///
    /// The signed flat manifest is written as manifest.toml in the input
    /// directory, replacing any file at that path.
    #[arg(
        long = "manifest-path",
        short = 'm',
        default_value = "manifest.toml",
        value_name = "FILE"
    )]
    pub manifest_path: PathBuf,

    /// Path to Publisher signing key file
    #[arg(long, short = 'k', value_name = "FILE")]
    pub key: Option<PathBuf>,

    /// Path to actor binary (for hash computation)
    #[arg(long, short = 'b', value_name = "FILE")]
    pub binary: Option<PathBuf>,

    /// Target platform (e.g. wasm32-wasip1, x86_64-unknown-linux-gnu)
    #[arg(long, short = 't', default_value = "wasm32-wasip1")]
    pub target: String,

    /// Output detached signature file (default: manifest.sig in the input directory).
    #[arg(long, short = 'o', value_name = "FILE")]
    pub output: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct PkgVerifyArgs {
    /// Path to the .actr archive to verify.
    #[arg(long, short = 'p', value_name = "FILE")]
    pub package: PathBuf,

    /// Public key file (default: derive from the configured Publisher keychain)
    #[arg(long, value_name = "FILE")]
    pub pubkey: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct PkgKeygenArgs {
    /// Key output file path (default: ~/.actr/dev-key.json)
    #[arg(long, short = 'o', value_name = "FILE")]
    pub output: Option<PathBuf>,
    /// Force overwrite existing key
    #[arg(long)]
    pub force: bool,
    /// Bind this keychain to a publisher; signing packages for
    /// any other publisher will then be rejected (issue #425).
    #[arg(long, value_name = "NAME")]
    pub publisher: Option<String>,
    /// With --output, keep the generated key out of the global CLI configuration.
    #[arg(long, requires = "output")]
    pub no_register: bool,
    /// Write a separate verification-only JSON file.
    #[arg(long, value_name = "FILE")]
    pub public_key_output: Option<PathBuf>,
}

#[async_trait]
impl Command for PkgArgs {
    async fn execute(&self, _ctx: &CommandContext) -> Result<CommandResult> {
        match &self.command {
            PkgCommand::Pack(a) => execute_pack(a, keychain_fallback(a.key.is_some())?.as_deref())?,
            PkgCommand::Sign(a) => {
                execute_sign(a, keychain_fallback(a.key.is_some())?.as_deref()).await?
            }
            PkgCommand::Verify(a) => {
                execute_verify(a, keychain_fallback(a.pubkey.is_some())?.as_deref()).await?
            }
            PkgCommand::Keygen(a) => execute_keygen(a)?,
        }
        Ok(CommandResult::Success(String::new()))
    }

    fn required_components(&self) -> Vec<ComponentType> {
        vec![]
    }

    fn name(&self) -> &str {
        "pkg"
    }

    fn description(&self) -> &str {
        "Local package operations (pack, sign, verify, keygen)"
    }
}

fn keychain_fallback(explicit: bool) -> Result<Option<String>> {
    if explicit {
        return Ok(None);
    }
    Ok(crate::config::resolver::resolve_effective_cli_config()?
        .publisher
        .keychain)
}

fn execute_pack(args: &PkgPackArgs, config_keychain: Option<&str>) -> Result<()> {
    let key_path = resolve_key_path(args.key.as_deref(), config_keychain)?;
    let (signing_key, keychain_publisher) = load_signing_key_with_publisher(&key_path)?;
    let manifest_source = std::fs::read_to_string(&args.manifest_path)
        .with_context(|| format!("Failed to read manifest: {}", args.manifest_path.display()))?;
    let manifest = actr_pack::ActorPackageManifest::from_toml(&manifest_source)?;
    ensure_publisher_matches(keychain_publisher.as_deref(), &manifest.actor.publisher)?;
    let output = output_target(&args.output)?;
    ensure!(
        output != key_path.canonicalize()? && output != args.manifest_path.canonicalize()?,
        "package output must differ from the signing key and manifest paths"
    );

    let canonical_root = args.payload_root.canonicalize().with_context(|| {
        format!(
            "Failed to resolve payload root: {}",
            args.payload_root.display()
        )
    })?;
    let mut payloads = BTreeMap::new();
    let mut remaining = actr_pack::DEFAULT_MAX_VERIFIED_ENTRY_BYTES;
    for file in &manifest.files {
        let path = &file.path;
        let canonical_path = canonical_root
            .join(path)
            .canonicalize()
            .with_context(|| format!("Failed to resolve payload {path}"))?;
        ensure!(
            canonical_path.starts_with(&canonical_root) && canonical_path.is_file(),
            "Payload {path} must be a regular file below {}",
            canonical_root.display()
        );
        ensure!(
            output != canonical_path,
            "package output must differ from payload {path}"
        );
        let mut bytes = Vec::new();
        std::fs::File::open(&canonical_path)?
            .take((remaining + 1) as u64)
            .read_to_end(&mut bytes)
            .with_context(|| format!("Failed to read payload: {}", canonical_path.display()))?;
        ensure!(
            bytes.len() <= remaining,
            "package payloads exceed the {}-byte verification limit",
            actr_pack::DEFAULT_MAX_VERIFIED_ENTRY_BYTES
        );
        remaining -= bytes.len();
        payloads.insert(path.clone(), bytes);
    }

    let public_key = signing_key.verifying_key();
    let package = actr_pack::pack_actor_package(&actr_pack::ActorPackOptions {
        manifest,
        payloads,
        signing_key,
    })?;
    actr_pack::verify_actor_package(&package, &public_key)?
        .manifest
        .validate_workload_payload(&package)?;
    write_atomic(&output, &package, true, OutputPermissions::Package)?;
    println!("Actor package created: {}", args.output.display());
    Ok(())
}

fn output_target(path: &Path) -> Result<PathBuf> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().context("expected an output file path")?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("Failed to create {}", parent.display()))?;
    Ok(parent.canonicalize()?.join(file_name))
}

enum OutputPermissions {
    Private,
    Package,
}

fn write_atomic(
    path: &Path,
    bytes: &[u8],
    overwrite: bool,
    permissions: OutputPermissions,
) -> Result<()> {
    #[cfg_attr(not(unix), allow(unused_mut))]
    let mut builder = tempfile::Builder::new();
    let existing_permissions = if matches!(permissions, OutputPermissions::Package) {
        // New packages use ordinary file permissions, restricted by the umask.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(std::fs::Permissions::from_mode(0o666));
        }
        match std::fs::metadata(path) {
            Ok(metadata) => Some(metadata.permissions()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(error).with_context(|| format!("Failed to inspect {}", path.display()));
            }
        }
    } else {
        None
    };
    let mut file = builder.tempfile_in(path.parent().context("missing output directory")?)?;
    file.write_all(bytes)?;
    if let Some(permissions) = existing_permissions {
        file.as_file().set_permissions(permissions)?;
    }
    let result = if overwrite {
        file.persist(path)
    } else {
        file.persist_noclobber(path)
    };
    result.with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

// ── keygen ───────────────────────────────────────────────────────────────────

fn execute_keygen(args: &PkgKeygenArgs) -> Result<()> {
    let requested_key = match args.output {
        Some(ref path) => path.clone(),
        None => {
            let home = dirs::home_dir()
                .ok_or_else(|| anyhow::anyhow!("Unable to determine home directory"))?;
            home.join(".actr").join("dev-key.json")
        }
    };

    let key_path = output_target(&requested_key)?;
    let public_path = args
        .public_key_output
        .as_deref()
        .map(output_target)
        .transpose()?;
    ensure!(
        public_path.as_ref() != Some(&key_path),
        "the public and private key outputs must be different files"
    );
    for path in std::iter::once(&key_path).chain(public_path.iter()) {
        ensure!(
            !path.is_dir(),
            "Key output path is a directory: {}\nSpecify a file name inside it.",
            path.display()
        );
        ensure!(
            !path.exists() || args.force,
            "Key file already exists: {}\nUse --force to overwrite, or choose a different output path.",
            path.display()
        );
    }
    let global = if args.no_register {
        None
    } else {
        let path = crate::config::loader::global_config_path()?;
        let config = crate::config::loader::load_cli_config(&path)?.unwrap_or_default();
        let target = output_target(&path)?;
        ensure!(
            target != key_path && public_path.as_ref() != Some(&target),
            "key outputs must differ from the global CLI configuration"
        );
        Some((target, config))
    };

    let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
    let verifying_key = signing_key.verifying_key();

    let private_b64 = base64::engine::general_purpose::STANDARD.encode(signing_key.to_bytes());
    let public_b64 = base64::engine::general_purpose::STANDARD.encode(verifying_key.to_bytes());

    let now = chrono::Utc::now().to_rfc3339();
    let mut key_json = serde_json::json!({
        "private_key": private_b64,
        "public_key": public_b64,
        "created_at": now,
        "note": "Development signing key, for StaticTrust use only, not for production use"
    });
    if let Some(ref publisher) = args.publisher {
        if publisher.trim().is_empty() {
            anyhow::bail!("--publisher cannot be empty");
        }
        key_json["publisher"] = serde_json::Value::String(publisher.clone());
    }

    let json_str = serde_json::to_string_pretty(&key_json)?;
    write_atomic(
        &key_path,
        json_str.as_bytes(),
        args.force,
        OutputPermissions::Private,
    )?;
    if let Some(path) = &public_path {
        let public_json = serde_json::to_vec_pretty(&serde_json::json!({
            "public_key": public_b64
        }))?;
        write_atomic(path, &public_json, args.force, OutputPermissions::Private)?;
    }

    println!("Key pair generated: {}", key_path.display());
    println!();
    println!("Public key (base64, Ed25519 verifying key — 32 bytes):");
    println!("  {}", public_b64);
    println!();
    println!("Use it as a StaticTrust anchor in your actr.toml:");
    println!("  [[trust]]");
    println!("  kind = \"static\"");
    println!("  pubkey_b64 = \"{}\"", public_b64);
    if let Some(path) = public_path.as_deref().and_then(Path::to_str) {
        println!();
        println!("Or reference the exported public key file in your actr.toml:");
        println!("  [[trust]]");
        println!("  kind = \"static\"");
        println!("  pubkey_file = {}", toml::Value::String(path.to_owned()));
    }

    if let Some((global_path, mut global_config)) = global {
        global_config.publisher.keychain = Some(key_path.display().to_string());
        let content =
            toml::to_string_pretty(&global_config).with_context(|| "Failed to serialize config")?;
        write_atomic(
            &global_path,
            content.as_bytes(),
            true,
            OutputPermissions::Private,
        )?;
        println!(
            "\nGlobal config updated: publisher.keychain = {}",
            key_path.display()
        );
    }

    Ok(())
}

// ── sign ─────────────────────────────────────────────────────────────────────
//
// Parses manifest.toml, reads binary + proto files, builds a PackageManifest,
// serializes via to_toml(), and signs with Ed25519.
//
// Output:
//   1. Canonical manifest.toml — the exact bytes that were signed
//   2. manifest.sig — 64 bytes raw Ed25519 signature
//   3. A signed manifest.lock.toml hash when the source manifest has dependencies
//
// The output uses the same PackageManifest schema and lock validation as
// `actr build`; an external packager must include the referenced sibling lock.

async fn execute_sign(args: &PkgSignArgs, config_keychain: Option<&str>) -> Result<()> {
    use ed25519_dalek::Signer;
    use sha2::{Digest, Sha256};

    let key_path = resolve_key_path(args.key.as_deref(), config_keychain)?;
    let (signing_key, keychain_publisher) = load_signing_key_with_publisher(&key_path)?;
    let verifying_key = signing_key.verifying_key();
    let key_id = actr_pack::compute_key_id(&verifying_key.to_bytes());

    let config_path = &args.manifest_path;
    if !config_path.exists() {
        return Err(anyhow::anyhow!(
            "manifest.toml not found: {}",
            config_path.display()
        ));
    }
    let config_bytes = std::fs::read(config_path)?;
    let config_value: toml::Value =
        toml::from_slice(&config_bytes).with_context(|| "Invalid manifest.toml")?;
    let pkg = config_value
        .get("package")
        .ok_or_else(|| anyhow::anyhow!("manifest.toml missing [package] section"))?;

    let get_str = |key: &str| -> Result<String> {
        pkg.get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("manifest.toml [package].{key} missing"))
    };

    let publisher = get_str("publisher")?;
    let name = get_str("name")?;
    let version = get_str("version")?;
    let parsed_config = actr_config::ConfigParser::from_manifest_file(config_path)
        .with_context(|| "Invalid manifest.toml package configuration")?;
    let lock_file = validated_packaged_lock(&parsed_config)?;

    // Reject cross-publisher signing before computing hashes or writing the
    // signature (issue #425).
    ensure_publisher_matches(keychain_publisher.as_deref(), &publisher)?;

    let (binary_hash, binary_size) = if let Some(binary_path) = &args.binary {
        let binary_data = std::fs::read(binary_path)
            .with_context(|| format!("Failed to read binary: {}", binary_path.display()))?;
        let hash = Sha256::digest(&binary_data);
        println!(
            "  binary:    {} ({} bytes)",
            binary_path.display(),
            binary_data.len()
        );
        (hex::encode(hash), Some(binary_data.len() as u64))
    } else {
        (String::new(), None)
    };

    let config_dir = args
        .manifest_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let mut proto_entries = vec![];
    let exports = pkg
        .get("exports")
        .and_then(|e| e.as_array())
        .or_else(|| config_value.get("exports").and_then(|e| e.as_array()));
    if let Some(exports) = exports {
        for export_entry in exports {
            if let Some(proto_path_str) = export_entry.as_str() {
                let proto_path = config_dir.join(proto_path_str);
                match std::fs::read(&proto_path) {
                    Ok(content) => {
                        let filename = proto_path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown.proto")
                            .to_string();
                        let hash = hex::encode(Sha256::digest(&content));
                        println!("  proto:     {} (hash: {}...)", filename, &hash[..16]);
                        proto_entries.push(actr_pack::ProtoFileEntry {
                            name: filename.clone(),
                            path: format!("proto/{}", filename),
                            hash,
                        });
                    }
                    Err(e) => {
                        tracing::warn!("Failed to read proto file {:?}: {}", proto_path, e);
                    }
                }
            }
        }
    }

    let manifest = actr_pack::PackageManifest {
        publisher: publisher.clone(),
        name: name.clone(),
        version: version.clone(),
        binary: actr_pack::BinaryEntry {
            path: "bin/actor.wasm".to_string(),
            target: args.target.clone(),
            hash: binary_hash,
            size: binary_size,
            kind: binary_kind_for_target(&args.target),
        },
        signature_algorithm: "ed25519".to_string(),
        signing_key_id: Some(key_id.clone()),
        resources: vec![],
        proto_files: proto_entries,
        lock_file: lock_file.as_ref().map(|bytes| actr_pack::LockFileEntry {
            path: "manifest.lock.toml".to_string(),
            hash: hex::encode(Sha256::digest(bytes)),
        }),
        metadata: actr_pack::ManifestMetadata {
            description: pkg
                .get("description")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            license: pkg
                .get("license")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        },
    };

    let manifest_toml = manifest
        .to_toml()
        .map_err(|e| anyhow::anyhow!("Failed to serialize manifest: {e}"))?;
    let manifest_bytes = manifest_toml.as_bytes();

    let signature = signing_key.sign(manifest_bytes);
    let sig_bytes = signature.to_bytes();

    let manifest_path = {
        let mut p = args.manifest_path.clone();
        p.set_file_name("manifest.toml");
        p
    };
    std::fs::write(&manifest_path, manifest_bytes)
        .with_context(|| format!("Failed to write manifest: {}", manifest_path.display()))?;

    let sig_path = args.output.clone().unwrap_or_else(|| {
        let mut p = args.manifest_path.clone();
        p.set_file_name("manifest.sig");
        p
    });
    {
        let mut f = std::fs::File::create(&sig_path)?;
        f.write_all(&sig_bytes)?;
    }

    println!("✅ Manifest signed successfully");
    println!(
        "  manifest:  {} (signed flat package manifest)",
        manifest_path.display()
    );
    println!("  sig file:  {} (64 bytes raw Ed25519)", sig_path.display());
    println!("  key_id:    {}", key_id);
    println!("  actr_type: {}/{}@{}", publisher, name, version);
    println!("  target:    {}", args.target);

    Ok(())
}

// ── verify ───────────────────────────────────────────────────────────────────

async fn execute_verify(args: &PkgVerifyArgs, config_keychain: Option<&str>) -> Result<()> {
    let package_bytes = std::fs::read(&args.package)
        .with_context(|| format!("Failed to read package: {}", args.package.display()))?;

    let pubkey = if let Some(pubkey_path) = &args.pubkey {
        load_verifying_key(pubkey_path)?
    } else {
        let key_path = resolve_key_path(None, config_keychain)?;
        load_verifying_key_from_dev_key(&key_path)?
    };

    let verified = actr_pack::verify_runtime_package_bounded(
        &package_bytes,
        &pubkey,
        actr_pack::DEFAULT_MAX_VERIFIED_ENTRY_BYTES,
    )?;
    let manifest = match verified.manifest {
        actr_pack::RuntimePackageManifest::V2(manifest) => {
            manifest.validate_workload_payload(&package_bytes)?;
            println!("Package verification passed (single archive)");
            println!();
            println!("  publisher:   {}", manifest.actor.publisher);
            println!("  type:        {}", manifest.actor_type());
            println!("  workload_entry: {}", manifest.workload.entry);
            println!("  profile:     {}", manifest.workload.profile);
            println!("  signing_key: {}", manifest.signature.key_id);
            println!("  files:       {}", manifest.files.len());
            return Ok(());
        }
        actr_pack::RuntimePackageManifest::Legacy(manifest) => manifest,
    };

    if let Some(ref manifest_key_id) = manifest.signing_key_id {
        let expected_key_id = actr_pack::compute_key_id(&pubkey.to_bytes());
        if manifest_key_id != &expected_key_id {
            anyhow::bail!(
                "signing_key_id mismatch: manifest says '{}' but the provided public key fingerprint is '{}'. \
                 This package will fail verification in Production mode. \
                 Rebuild with 'actr build' using the correct signing key.",
                manifest_key_id,
                expected_key_id,
            );
        }
    } else {
        anyhow::bail!(
            "Package manifest has no 'signing_key_id'. \
             This package will be rejected in Production mode. \
             Rebuild with the latest 'actr build' to embed a signing_key_id."
        );
    }

    println!("Package verification passed (single archive)");
    println!();
    println!("  publisher: {}", manifest.publisher);
    println!("  type:         {}", manifest.actr_type_str());
    println!("  binary:       {}", manifest.binary.path);
    println!("  binary_hash:  {}...", &manifest.binary.hash[..16]);
    println!("  target:       {}", manifest.binary.target);
    if let Some(ref key_id) = manifest.signing_key_id {
        println!("  signing_key:  {}", key_id);
    }
    if !manifest.resources.is_empty() {
        println!("  resources:    {}", manifest.resources.len());
    }

    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::{OutputPermissions, write_atomic};
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn atomic_package_output_uses_regular_modes_and_preserves_existing_permissions() {
        let directory = tempfile::tempdir().unwrap();
        let ordinary = directory.path().join("ordinary");
        std::fs::write(&ordinary, b"ordinary").unwrap();
        let package = directory.path().join("package.actr");
        write_atomic(&package, b"first", true, OutputPermissions::Package).unwrap();
        assert_eq!(
            std::fs::metadata(&package).unwrap().permissions().mode(),
            std::fs::metadata(&ordinary).unwrap().permissions().mode()
        );
        for mode in [0o640, 0o644] {
            std::fs::set_permissions(&package, std::fs::Permissions::from_mode(mode)).unwrap();
            write_atomic(&package, b"replacement", true, OutputPermissions::Package).unwrap();
            assert_eq!(std::fs::read(&package).unwrap(), b"replacement");
            assert_eq!(
                std::fs::metadata(&package).unwrap().permissions().mode() & 0o777,
                mode
            );
        }

        let key = directory.path().join("key.json");
        std::fs::write(&key, b"old").unwrap();
        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o644)).unwrap();
        write_atomic(&key, b"private", true, OutputPermissions::Private).unwrap();
        assert_eq!(
            std::fs::metadata(&key).unwrap().permissions().mode() & 0o077,
            0
        );
    }
}
