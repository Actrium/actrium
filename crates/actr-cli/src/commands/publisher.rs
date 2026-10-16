//! Offline Publisher Profile keys and signing for `/.well-known/actr`.
//!
//! Profile signing keys authorize the domain's metadata. Package signing keys
//! are generated separately by `actr pkg keygen` and listed by public key only.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use actr_protocol::realm_profile::sign_document;
use actr_protocol::{
    KeyStatus, PUBLISHER_PROFILE_SCHEMA, PackageSigningKey, PublisherProfile, RealmSigningKey,
    SIGNATURE_ALGORITHM, package_key_id, parse_authority_member,
};
use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use chrono::{SecondsFormat, TimeDelta, Utc};
use clap::{Args, Subcommand};
use ed25519_dalek::{SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::core::{Command, CommandContext, CommandResult, ComponentType};

#[derive(Args, Debug)]
pub struct PublisherArgs {
    #[command(subcommand)]
    pub command: PublisherCommand,
}

#[derive(Subcommand, Debug)]
pub enum PublisherCommand {
    /// Generate Profile signing keys and sign a domain's Publisher Profile
    Profile(ProfileArgs),
}

#[derive(Args, Debug)]
pub struct ProfileArgs {
    #[command(subcommand)]
    pub command: ProfileCommand,
}

#[derive(Subcommand, Debug)]
pub enum ProfileCommand {
    /// Write a private Profile signing key and print its public key entry
    Keygen(ProfileKeygenArgs),
    /// Print a self-signed Publisher Profile for the authority descriptor
    Sign(ProfileSignArgs),
}

#[derive(Args, Debug)]
pub struct ProfileKeygenArgs {
    /// ID of this key in the Publisher Profile's profile_signing_keys
    #[arg(long, value_name = "ID")]
    pub key_id: String,
    /// Private key file to write; compatible with realm register-publisher
    #[arg(long, short = 'o', value_name = "FILE")]
    pub output: PathBuf,
    /// Replace an existing key file
    #[arg(long)]
    pub force: bool,
}

#[derive(Args, Debug)]
pub struct ProfileSignArgs {
    /// Private Profile signing key from publisher profile keygen
    #[arg(long, short = 'k', value_name = "FILE")]
    pub key: PathBuf,
    /// Publisher domain, without a scheme, port or path
    #[arg(long, value_name = "DOMAIN")]
    pub domain: String,
    /// Package public-key JSON or pkg keygen keychain; repeat to list more keys.
    /// Only public material is used. An optional status may be active or retired.
    #[arg(long, value_name = "FILE", required = true)]
    pub package_key: Vec<PathBuf>,
    /// Security contact URI, for example mailto:security@example.com
    #[arg(long, value_name = "URI")]
    pub security_contact: String,
    /// Profile sequence; increase for every update of an accepted Profile
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u64).range(1..=(1 << 53) - 1))]
    pub sequence: u64,
    /// Validity from now, in days; renew the Profile before it expires
    #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u32).range(1..))]
    pub valid_for_days: u32,
}

// Keep the format accepted by `realm register-publisher`, without reusing the
// Realm keygen command or changing the global package-signing keychain setting.
#[derive(Serialize, Deserialize)]
struct ProfileKeyFile {
    key_id: String,
    private_key: String,
    public_key: String,
}

impl ProfileKeyFile {
    fn load(path: &Path) -> Result<(String, SigningKey)> {
        let text = actr_protocol::realm_membership::read_private_key_file(
            path,
            "Publisher Profile signing key",
        )
        .map_err(anyhow::Error::msg)?;
        let file: Self = serde_json::from_str(&text).with_context(|| {
            format!(
                "Not a Publisher Profile signing key file: {}",
                path.display()
            )
        })?;
        validate_key_id(&file.key_id)?;
        let bytes: [u8; 32] = BASE64
            .decode(&file.private_key)
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .context("private_key is not a base64 Ed25519 key")?;
        let key = SigningKey::from_bytes(&bytes);
        if BASE64.encode(key.verifying_key().to_bytes()) != file.public_key {
            bail!("Profile signing key's public_key does not match its private_key");
        }
        Ok((file.key_id, key))
    }
}

#[async_trait]
impl Command for PublisherArgs {
    async fn execute(&self, _ctx: &CommandContext) -> Result<CommandResult> {
        let PublisherCommand::Profile(args) = &self.command;
        let output = match &args.command {
            ProfileCommand::Keygen(args) => keygen(args)?,
            ProfileCommand::Sign(args) => sign_profile(args)?,
        };
        Ok(CommandResult::Success(serde_json::to_string_pretty(
            &output,
        )?))
    }

    fn required_components(&self) -> Vec<ComponentType> {
        vec![]
    }

    fn name(&self) -> &str {
        "publisher"
    }

    fn description(&self) -> &str {
        "Publisher Profile keys and signing"
    }
}

fn validate_key_id(id: &str) -> Result<()> {
    if id.trim().is_empty() || id.trim() != id || id.chars().any(char::is_control) {
        bail!("--key-id must be nonempty without surrounding whitespace or control characters");
    }
    Ok(())
}

fn public_entry(id: &str, key: &SigningKey) -> RealmSigningKey {
    RealmSigningKey {
        id: id.to_string(),
        algorithm: SIGNATURE_ALGORITHM.to_string(),
        public_key: BASE64.encode(key.verifying_key().to_bytes()),
        status: KeyStatus::Active,
    }
}

fn keygen(args: &ProfileKeygenArgs) -> Result<Value> {
    validate_key_id(&args.key_id)?;
    let parent = args
        .output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)
        .with_context(|| format!("Failed to create {}", parent.display()))?;

    let key = SigningKey::generate(&mut rand::rngs::OsRng);
    let entry = public_entry(&args.key_id, &key);
    let file = ProfileKeyFile {
        key_id: args.key_id.clone(),
        private_key: BASE64.encode(key.to_bytes()),
        public_key: entry.public_key.clone(),
    };
    // NamedTempFile is owner-only on Unix from creation. Atomic persistence
    // prevents a failed write from truncating an old key; no-clobber also
    // protects against a destination created after our command started.
    let mut output = tempfile::NamedTempFile::new_in(parent)?;
    output.write_all(serde_json::to_string_pretty(&file)?.as_bytes())?;
    output.as_file().sync_all()?;
    if args.force {
        output.persist(&args.output)
    } else {
        output.persist_noclobber(&args.output)
    }
    .map_err(|error| error.error)
    .with_context(|| {
        format!(
            "Failed to write key file {}; use --force to replace an existing file",
            args.output.display()
        )
    })?;
    eprintln!(
        "Publisher Profile signing key written to {}",
        args.output.display()
    );
    serde_json::to_value(entry).context("Failed to serialize public key entry")
}

#[derive(Deserialize)]
struct PackageKeyInput {
    public_key: String,
    publisher: Option<String>,
    id: Option<String>,
    algorithm: Option<String>,
    status: Option<KeyStatus>,
    not_before: Option<String>,
    not_after: Option<String>,
}

fn package_key(path: &Path, domain: &str) -> Result<PackageSigningKey> {
    let input: PackageKeyInput = serde_json::from_str(
        &std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read package public key {}", path.display()))?,
    )
    .with_context(|| format!("Invalid package public-key JSON: {}", path.display()))?;
    if input
        .publisher
        .as_deref()
        .is_some_and(|name| name != domain)
    {
        bail!(
            "Package key {} is bound to another publisher",
            path.display()
        );
    }
    let bytes: [u8; 32] = BASE64
        .decode(&input.public_key)
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .context("Package public_key is not a base64 Ed25519 key")?;
    VerifyingKey::from_bytes(&bytes).context("Invalid package Ed25519 public key")?;
    let id = package_key_id(&bytes);
    if input.id.as_deref().is_some_and(|listed| listed != id) {
        bail!(
            "Package key id in {} is not derived from its public key",
            path.display()
        );
    }
    if input
        .algorithm
        .as_deref()
        .is_some_and(|name| name != SIGNATURE_ALGORITHM)
    {
        bail!("Package key {} must use Ed25519", path.display());
    }
    for time in [&input.not_before, &input.not_after].into_iter().flatten() {
        chrono::DateTime::parse_from_rfc3339(time).context("Invalid package key validity time")?;
    }
    Ok(PackageSigningKey {
        id,
        algorithm: SIGNATURE_ALGORITHM.to_string(),
        public_key: BASE64.encode(bytes),
        status: input.status.unwrap_or(KeyStatus::Active),
        not_before: input.not_before,
        not_after: input.not_after,
    })
}

fn sign_profile(args: &ProfileSignArgs) -> Result<Value> {
    let authority = parse_authority_member(&format!("actr://{}", args.domain))?;
    let domain = authority.domain();
    url::Url::parse(&args.security_contact)
        .context("--security-contact must be an absolute URI")?;
    let (id, key) = ProfileKeyFile::load(&args.key)?;
    let package_keys = args
        .package_key
        .iter()
        .map(|path| package_key(path, domain))
        .collect::<Result<Vec<_>>>()?;
    let now = Utc::now();
    let expires = now
        .checked_add_signed(TimeDelta::days(i64::from(args.valid_for_days)))
        .context("Profile validity exceeds the supported date range")?;
    let mut profile = json!({
        "schema": PUBLISHER_PROFILE_SCHEMA,
        "sequence": args.sequence,
        "domain": domain,
        "profile_signing_keys": [public_entry(&id, &key)],
        "keys": package_keys,
        "security_contact": args.security_contact,
        "profile_issued_at": now.to_rfc3339_opts(SecondsFormat::Secs, true),
        "profile_expires_at": expires.to_rfc3339_opts(SecondsFormat::Secs, true),
    });
    sign_document(&mut profile, &id, &key);
    // Use the consumer's acceptance path, including safe JCS sequences, key
    // counts, derived package IDs and the self-signature, before printing.
    serde_json::from_value::<PublisherProfile>(profile.clone())?
        .accept(domain, now, None)
        .context("Publisher Profile is not acceptable")?;
    Ok(profile)
}
