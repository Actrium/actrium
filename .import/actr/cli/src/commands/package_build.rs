use std::path::{Path, PathBuf};

use actr_config::{ConfigParser, LockFile, ManifestConfig};
use anyhow::{Context, Result};
use base64::Engine;
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};

pub struct PackageBuildInput {
    pub binary_path: PathBuf,
    pub config_path: PathBuf,
    pub key_path: PathBuf,
    pub output_path: PathBuf,
    pub target: String,
    pub resources: Vec<(String, PathBuf)>,
}

pub struct PackageBuildSummary {
    pub actr_type: String,
    pub target: String,
    pub binary_path: PathBuf,
    pub output_path: PathBuf,
    pub binary_hash: String,
    pub package_size: usize,
    pub public_key: String,
}

#[derive(Debug)]
struct ValidatedLock {
    parsed: LockFile,
    bytes: Vec<u8>,
}

pub(crate) fn binary_kind_for_target(target: &str) -> Option<actr_pack::BinaryKind> {
    match target {
        "wasm32-wasip2" => Some(actr_pack::BinaryKind::Component),
        "wasm32-unknown-unknown" => Some(actr_pack::BinaryKind::CoreModule),
        _ => None,
    }
}

fn read_validated_lock(lock_path: &Path) -> Result<Option<ValidatedLock>> {
    if !lock_path.exists() {
        return Ok(None);
    }

    let bytes = std::fs::read(lock_path)
        .with_context(|| format!("Failed to read {}", lock_path.display()))?;
    let raw = std::str::from_utf8(&bytes)
        .with_context(|| format!("{} is not valid UTF-8", lock_path.display()))?;
    let parsed: LockFile = raw
        .parse()
        .with_context(|| format!("Failed to parse {}", lock_path.display()))?;
    parsed.validate_aliases().with_context(|| {
        format!(
            "Invalid {}; run `actr deps install` before packaging",
            lock_path.display()
        )
    })?;
    Ok(Some(ValidatedLock { parsed, bytes }))
}

fn validate_packaged_dependencies(
    config: &ManifestConfig,
    lock: Option<&ValidatedLock>,
) -> Result<()> {
    if !config.dependencies.is_empty() && lock.is_none() {
        anyhow::bail!(
            "manifest declares remote dependencies but manifest.lock.toml is missing; run `actr deps install` before packaging"
        );
    }

    let Some(lock) = lock else {
        return Ok(());
    };
    if lock.parsed.dependencies.len() != config.dependencies.len() {
        anyhow::bail!(
            "manifest.lock.toml dependency set does not match manifest.toml; run `actr deps install` before packaging"
        );
    }
    for dependency in &config.dependencies {
        let locked = lock
            .parsed
            .get_dependency_by_alias(&dependency.alias)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "manifest dependency '{}' is missing from manifest.lock.toml; run `actr deps install` before packaging",
                    dependency.alias
                )
            })?;
        if let Some(expected_type) = &dependency.actr_type {
            let expected = expected_type.to_string_repr();
            if locked.actr_type != expected {
                anyhow::bail!(
                    "manifest dependency '{}' has ActrType {}, but manifest.lock.toml contains {}; run `actr deps install` before packaging",
                    dependency.alias,
                    expected,
                    locked.actr_type
                );
            }
        }
        if let Some(expected_service) = &dependency.service {
            if locked.name != expected_service.name
                || locked.fingerprint != expected_service.fingerprint
            {
                anyhow::bail!(
                    "manifest dependency '{}' service reference does not match manifest.lock.toml; run `actr deps install` before packaging",
                    dependency.alias
                );
            }
        }
    }
    Ok(())
}

pub(crate) fn validated_packaged_lock(config: &ManifestConfig) -> Result<Option<Vec<u8>>> {
    let lock = read_validated_lock(&config.config_dir.join("manifest.lock.toml"))?;
    validate_packaged_dependencies(config, lock.as_ref())?;
    Ok(lock.map(|lock| lock.bytes))
}

pub fn resolve_key_path(custom: Option<&Path>, config_keychain: Option<&str>) -> Result<PathBuf> {
    if let Some(path) = custom {
        return Ok(path.to_path_buf());
    }

    if let Some(path) = config_keychain {
        if let Some(stripped) = path.strip_prefix("~/") {
            let home = dirs::home_dir()
                .ok_or_else(|| anyhow::anyhow!("Unable to determine home directory"))?;
            return Ok(home.join(stripped));
        }
        return Ok(PathBuf::from(path));
    }

    anyhow::bail!(
        "No signing key configured.\nSpecify --key, or set mfr.keychain in your CLI config."
    )
}

/// Load an MFR signing key from a keychain JSON file, returning the key
/// alongside the optional `manufacturer` binding recorded in the keychain.
///
/// A keychain that carries a `manufacturer` field is bound to that
/// manufacturer; callers must verify it matches the package manufacturer
/// before signing (see [`ensure_manufacturer_matches`]). Keychains without the
/// field remain unbound for backward compatibility.
pub fn load_signing_key_with_manufacturer(key_path: &Path) -> Result<(SigningKey, Option<String>)> {
    if !key_path.exists() {
        anyhow::bail!(
            "Key file not found: {}\nRun `actr pkg keygen` to generate a key first.",
            key_path.display()
        );
    }

    let content = std::fs::read_to_string(key_path)?;
    let json: serde_json::Value = serde_json::from_str(&content)?;
    let manufacturer = json
        .get("manufacturer")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let private_b64 = json["private_key"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Key file missing private_key field"))?;
    let private_bytes = base64::engine::general_purpose::STANDARD.decode(private_b64)?;
    let key_arr: [u8; 32] = private_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("private_key must be exactly 32 bytes"))?;
    Ok((SigningKey::from_bytes(&key_arr), manufacturer))
}

/// Enforce that a keychain's bound manufacturer matches the package
/// manufacturer. No-op when the keychain carries no `manufacturer` field
/// (backward-compatible: legacy keychains remain usable and only log a warning
/// advising that binding the keychain is recommended).
pub fn ensure_manufacturer_matches(
    keychain_manufacturer: Option<&str>,
    package_manufacturer: &str,
) -> Result<()> {
    match keychain_manufacturer {
        Some(kc_mfr) if kc_mfr == package_manufacturer => Ok(()),
        Some(kc_mfr) => anyhow::bail!(
            "Manufacturer mismatch: keychain is bound to '{}' but the package manufacturer is '{}'. \
             Rebind the keychain with `actr pkg keygen --manufacturer {} --force` or use a matching keychain.",
            kc_mfr,
            package_manufacturer,
            package_manufacturer,
        ),
        None => {
            tracing::warn!(
                "Keychain has no 'manufacturer' binding; the package manufacturer '{}' is not \
                 verified against the signing key. Run `actr pkg keygen --manufacturer {}` to \
                 bind the keychain and prevent cross-manufacturer signing.",
                package_manufacturer,
                package_manufacturer,
            );
            Ok(())
        }
    }
}

pub fn load_verifying_key(path: &Path) -> Result<ed25519_dalek::VerifyingKey> {
    let content = std::fs::read_to_string(path)?;
    let json: serde_json::Value = serde_json::from_str(&content)?;
    let public_b64 = json["public_key"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Key file missing public_key field"))?;
    let public_bytes = base64::engine::general_purpose::STANDARD.decode(public_b64)?;
    let key_arr: [u8; 32] = public_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("public_key must be exactly 32 bytes"))?;
    ed25519_dalek::VerifyingKey::from_bytes(&key_arr)
        .map_err(|e| anyhow::anyhow!("Invalid public key: {e}"))
}

pub fn load_verifying_key_from_dev_key(path: &Path) -> Result<ed25519_dalek::VerifyingKey> {
    if !path.exists() {
        anyhow::bail!(
            "No key file found at {}. Specify --pubkey or run `actr pkg keygen` first.",
            path.display()
        );
    }

    load_verifying_key(path)
}

pub fn default_dist_output_path(config_path: &Path, target: &str) -> Result<PathBuf> {
    let file_name = default_package_file_name(config_path, target)?;
    let manifest_dir = config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    Ok(manifest_dir.join("dist").join(file_name))
}

fn default_package_file_name(config_path: &Path, target: &str) -> Result<String> {
    let config = ConfigParser::from_manifest_file(config_path).with_context(|| {
        format!(
            "Failed to parse manifest for default output path: {}",
            config_path.display()
        )
    })?;
    let file_name = format!(
        "{}-{}-{}-{}.actr",
        config.package.actr_type.manufacturer,
        config.package.actr_type.name,
        config.package.actr_type.version,
        target
    );
    // Defense-in-depth: `parse_package` now validates the identity (including
    // `version`), but require the generated filename to be exactly one normal
    // path component so it can never escape `dist/` regardless of how it was
    // produced (CWE-22 / CWE-73).
    let mut components = Path::new(&file_name).components();
    match (components.next(), components.next()) {
        (Some(std::path::Component::Normal(_)), None) => Ok(file_name),
        _ => anyhow::bail!(
            "generated package filename `{}` is not a single path component",
            file_name
        ),
    }
}

pub fn build_package(input: PackageBuildInput) -> Result<PackageBuildSummary> {
    let (signing_key, keychain_manufacturer) = load_signing_key_with_manufacturer(&input.key_path)?;
    let verifying_key = signing_key.verifying_key();

    let config = ConfigParser::from_manifest_file(&input.config_path).with_context(|| {
        format!(
            "Failed to parse manifest configuration: {}",
            input.config_path.display()
        )
    })?;

    // Reject cross-manufacturer signing before any packaging work: a keychain
    // that declares a `manufacturer` binding may only sign packages for that
    // same manufacturer (issue #425).
    ensure_manufacturer_matches(
        keychain_manufacturer.as_deref(),
        &config.package.actr_type.manufacturer,
    )?;

    let binary_bytes = std::fs::read(&input.binary_path)
        .with_context(|| format!("Failed to read binary: {}", input.binary_path.display()))?;

    let resources = input
        .resources
        .iter()
        .map(|(zip_path, local_path)| {
            let bytes = std::fs::read(local_path)
                .with_context(|| format!("Failed to read resource: {}", local_path.display()))?;
            Ok((zip_path.clone(), bytes))
        })
        .collect::<Result<Vec<_>>>()?;

    let proto_files = config
        .exports
        .iter()
        .map(|proto| {
            (
                proto.file_name().unwrap_or("unknown.proto").to_string(),
                proto.content.as_bytes().to_vec(),
            )
        })
        .collect::<Vec<_>>();

    let lock_file = validated_packaged_lock(&config)?;

    let manifest = actr_pack::PackageManifest {
        manufacturer: config.package.actr_type.manufacturer.clone(),
        name: config.package.actr_type.name.clone(),
        version: config.package.actr_type.version.clone(),
        binary: actr_pack::BinaryEntry {
            path: "bin/actor.wasm".to_string(),
            target: input.target.clone(),
            hash: String::new(),
            size: None,
            kind: binary_kind_for_target(&input.target),
        },
        signature_algorithm: "ed25519".to_string(),
        signing_key_id: Some(actr_pack::compute_key_id(&verifying_key.to_bytes())),
        resources: vec![],
        proto_files: vec![],
        lock_file: None,
        metadata: actr_pack::ManifestMetadata {
            description: config.package.description.clone(),
            license: config.package.license.clone(),
        },
    };

    let package_bytes = actr_pack::pack(&actr_pack::PackOptions {
        manifest,
        binary_bytes: binary_bytes.clone(),
        resources,
        proto_files,
        signing_key,
        lock_file,
    })?;

    if let Some(parent) = input.output_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create output directory: {}", parent.display()))?;
    }
    std::fs::write(&input.output_path, &package_bytes)
        .with_context(|| format!("Failed to write package: {}", input.output_path.display()))?;

    let binary_hash = hex::encode(Sha256::digest(&binary_bytes));
    let public_key = base64::engine::general_purpose::STANDARD.encode(verifying_key.to_bytes());
    let actr_type = config.package.actr_type.to_string_repr();

    Ok(PackageBuildSummary {
        actr_type,
        target: input.target,
        binary_path: input.binary_path,
        output_path: input.output_path,
        binary_hash,
        package_size: package_bytes.len(),
        public_key,
    })
}

pub fn print_build_summary(summary: &PackageBuildSummary) {
    println!("Package built successfully");
    println!();
    println!("  type:        {}", summary.actr_type);
    println!("  target:      {}", summary.target);
    println!("  binary:      {}", summary.binary_path.display());
    println!("  binary_hash: {}...", &summary.binary_hash[..16]);
    println!("  output:      {}", summary.output_path.display());
    println!("  size:        {} bytes", summary.package_size);
    println!();
    println!("Public key (for verification):");
    println!("  {}", summary.public_key);
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD as B64;
    use ed25519_dalek::SigningKey;
    use std::fs;
    use tempfile::TempDir;

    fn write_manifest(dir: &TempDir, version: &str) -> PathBuf {
        let path = dir.path().join("manifest.toml");
        fs::write(
            &path,
            format!(
                "edition = 1\n\n[package]\nname = \"Echo\"\nmanufacturer = \"acme\"\nversion = \"{version}\"\n"
            ),
        )
        .unwrap();
        path
    }

    fn write_keychain(path: &Path, manufacturer: Option<&str>) -> SigningKey {
        let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let json = match manufacturer {
            Some(mfr) => serde_json::json!({
                "private_key": B64.encode(signing_key.to_bytes()),
                "public_key": B64.encode(signing_key.verifying_key().to_bytes()),
                "manufacturer": mfr,
            }),
            None => serde_json::json!({
                "private_key": B64.encode(signing_key.to_bytes()),
                "public_key": B64.encode(signing_key.verifying_key().to_bytes()),
            }),
        };
        fs::write(path, json.to_string()).unwrap();
        signing_key
    }

    #[test]
    fn load_signing_key_with_manufacturer_reads_optional_binding() {
        let dir = TempDir::new().unwrap();
        let key_path = dir.path().join("k.json");

        // Bound keychain → manufacturer is returned.
        write_keychain(&key_path, Some("acme"));
        let (_, mfr) = load_signing_key_with_manufacturer(&key_path).unwrap();
        assert_eq!(mfr.as_deref(), Some("acme"));

        // Unbound keychain → None (backward compatible).
        write_keychain(&key_path, None);
        let (_, mfr) = load_signing_key_with_manufacturer(&key_path).unwrap();
        assert!(mfr.is_none());
    }

    #[test]
    fn ensure_manufacturer_matches_passes_when_equal_or_unbound() {
        // Bound + matching → ok.
        assert!(ensure_manufacturer_matches(Some("acme"), "acme").is_ok());
        // Unbound → ok (legacy keychain, warns only).
        assert!(ensure_manufacturer_matches(None, "acme").is_ok());
    }

    #[test]
    fn ensure_manufacturer_matches_rejects_cross_manufacturer() {
        let err = ensure_manufacturer_matches(Some("acme"), "other").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("Manufacturer mismatch"), "got: {msg}");
        assert!(msg.contains("acme"), "got: {msg}");
        assert!(msg.contains("other"), "got: {msg}");
    }

    #[test]
    fn build_package_rejects_manufacturer_mismatch_before_packaging() {
        let dir = TempDir::new().unwrap();
        let manifest_path = write_manifest(&dir, "1.0.0");
        let key_path = dir.path().join("bound-other.json");
        // Keychain bound to "rival" but manifest says "acme".
        write_keychain(&key_path, Some("rival"));

        let err = match build_package(PackageBuildInput {
            binary_path: dir.path().join("nonexistent.wasm"),
            config_path: manifest_path,
            key_path,
            output_path: dir.path().join("dist/out.actr"),
            target: "wasm32-wasip1".to_string(),
            resources: vec![],
        }) {
            Ok(_) => panic!("expected a manufacturer mismatch error"),
            Err(e) => e,
        };
        let msg = format!("{err}");
        assert!(msg.contains("Manufacturer mismatch"), "got: {msg}");
        // Must fail before touching the (non-existent) binary.
        assert!(
            !dir.path().join("dist").exists(),
            "no packaging should occur"
        );
    }

    #[test]
    fn default_package_file_name_rejects_traversal_version() {
        let dir = TempDir::new().unwrap();
        // `../../../outside` would escape `dist/` if interpolated unchecked; it is
        // now rejected at manifest parsing before any filename is produced.
        let path = write_manifest(&dir, "../../../outside");
        let err = default_package_file_name(&path, "wasm32-wasip2").unwrap_err();
        let chain = format!("{:#}", err);
        assert!(
            chain.contains("version") || chain.contains("Invalid"),
            "expected a version-validation error, got: {chain}"
        );
    }

    #[test]
    fn default_package_file_name_is_single_normal_component() {
        let dir = TempDir::new().unwrap();
        let path = write_manifest(&dir, "1.0.0");
        let name = default_package_file_name(&path, "wasm32-wasip2").unwrap();
        assert_eq!(name, "acme-Echo-1.0.0-wasm32-wasip2.actr");
        // The generated filename is always exactly one normal path component.
        let mut components = Path::new(&name).components();
        assert!(matches!(
            components.next(),
            Some(std::path::Component::Normal(_))
        ));
        assert!(components.next().is_none());
    }

    #[test]
    fn package_binary_kind_distinguishes_native_components_and_browser_modules() {
        assert_eq!(
            binary_kind_for_target("wasm32-wasip2"),
            Some(actr_pack::BinaryKind::Component)
        );
        assert_eq!(
            binary_kind_for_target("wasm32-unknown-unknown"),
            Some(actr_pack::BinaryKind::CoreModule)
        );
    }

    #[test]
    fn package_lock_validation_rejects_v1_and_accepts_empty_v2() {
        let dir = TempDir::new().unwrap();
        let lock_path = dir.path().join("manifest.lock.toml");
        fs::write(
            &lock_path,
            "[metadata]\nversion = 1\ngenerated_at = \"2026-01-01T00:00:00Z\"\n",
        )
        .unwrap();
        let error = read_validated_lock(&lock_path).unwrap_err().to_string();
        assert!(error.contains("actr deps install"), "{error}");

        fs::write(
            &lock_path,
            "[metadata]\nversion = 2\ngenerated_at = \"2026-01-01T00:00:00Z\"\n",
        )
        .unwrap();
        assert!(read_validated_lock(&lock_path).unwrap().is_some());
    }

    #[test]
    fn packaging_requires_lock_to_match_manifest_dependencies() {
        let dir = TempDir::new().unwrap();
        let manifest_path = dir.path().join("manifest.toml");
        fs::write(
            &manifest_path,
            r#"edition = 1

[package]
name = "EchoClient"
manufacturer = "acme"
version = "1.0.0"

[dependencies]
echo = { actr_type = "acme:EchoService:1.0.0", service = "EchoService:service_semantic:echo" }
"#,
        )
        .unwrap();
        let config = ConfigParser::from_manifest_file(&manifest_path).unwrap();
        let missing_error = validate_packaged_dependencies(&config, None)
            .unwrap_err()
            .to_string();
        assert!(
            missing_error.contains("actr deps install"),
            "{missing_error}"
        );

        let lock_path = dir.path().join("manifest.lock.toml");
        fs::write(
            &lock_path,
            r#"[metadata]
version = 2
generated_at = "2026-08-06T00:00:00Z"

[[dependency]]
name = "EchoService"
alias = "echo"
actr_type = "acme:EchoService:1.0.0"
fingerprint = "service_semantic:echo"
cached_at = "2026-08-06T00:00:00Z"

[[dependency.files]]
path = "echo/echo.proto"
fingerprint = "semantic:echo"
"#,
        )
        .unwrap();
        let lock = read_validated_lock(&lock_path).unwrap().unwrap();
        validate_packaged_dependencies(&config, Some(&lock)).unwrap();
    }
}
