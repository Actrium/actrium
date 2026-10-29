//! Pinned Realm trust (FR-NET-015, AC-021): the accepted `realm_signing_keys`
//! of each Realm domain and the accepted Realm Profile baselines, persisted
//! as `~/.actr/realms/<domain>.json`.
//!
//! Joining a Realm accepts the domain's signing keys under the stored pins
//! (`actr/protocols/authority-discovery.md` section 4.4) instead of trusting
//! every fetch anew: a member that rolls the accepted `sequence` back or
//! changes it without a rotation chain is rejected, and a signing key that
//! does not chain to a pinned key is an identity change the user confirms
//! explicitly with `actr realm trust <domain>`. The accepted Realm Profile is
//! kept verbatim per Realm, so a later Profile must be a valid successor of
//! the stored baseline (`actr/protocols/realm-profile.md` section 5).

use actr_protocol::realm_profile::RealmProfile;
use actr_protocol::uri::ActrUri;
use actr_protocol::{KeyPinOutcome, KeyPins, RealmSigningKeys};
use anyhow::{Context, Result, bail};
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::realm_members::{discovery_client, domain_authority, fetch_public};

pub const REALM_PINS_SCHEMA: &str = "actr.realm-pins/v1";

/// The accepted Realm Profile baseline of one Realm.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfilePin {
    pub sequence: u64,
    /// SHA-256 hex of `document`.
    pub digest: String,
    /// The accepted Realm Profile, the verified bytes verbatim.
    pub document: String,
    pub provider_id: String,
    pub pinned_at: String,
}

/// Everything pinned for one Realm domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealmTrust {
    pub schema: String,
    pub domain: String,
    /// The accepted `realm_signing_keys` member, verbatim.
    pub member: Value,
    pub pins: KeyPins,
    pub pinned_at: String,
    pub checked_at: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub profiles: BTreeMap<String, ProfilePin>,
}

impl RealmTrust {
    fn new(domain: &str, member: &Value, pins: KeyPins) -> Self {
        let now = now();
        Self {
            schema: REALM_PINS_SCHEMA.to_string(),
            domain: domain.to_string(),
            member: member.clone(),
            pins,
            pinned_at: now.clone(),
            checked_at: now,
            profiles: BTreeMap::new(),
        }
    }
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Where pinned trust is kept. `load` fails closed: a corrupt or foreign
/// store is an error, never a silent re-pin.
pub trait TrustStore: Send {
    fn load(&mut self, domain: &str) -> Result<Option<RealmTrust>>;
    fn save(&mut self, trust: &RealmTrust) -> Result<()>;
    /// Drop everything pinned for `domain`; whether anything was pinned.
    fn reset(&mut self, domain: &str) -> Result<bool>;
}

/// [`TrustStore`] as `~/.actr/realms/<domain>.json` (or another directory).
pub struct FileTrustStore {
    dir: PathBuf,
}

impl FileTrustStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// The default store, `~/.actr/realms`.
    pub fn at_default() -> Result<Self> {
        let home = dirs::home_dir().context("Cannot locate the home directory")?;
        Ok(Self::new(home.join(".actr").join("realms")))
    }

    fn path(&self, domain: &str) -> Result<PathBuf> {
        let domain = ActrUri::from_domain(domain)
            .map_err(|error| anyhow::anyhow!("{error}"))?
            .domain()
            .to_string();
        Ok(self.dir.join(format!("{domain}.json")))
    }
}

impl TrustStore for FileTrustStore {
    fn load(&mut self, domain: &str) -> Result<Option<RealmTrust>> {
        let path = self.path(domain)?;
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Failed to read pinned trust: {}", path.display()));
            }
        };
        let trust: RealmTrust = serde_json::from_str(&text).with_context(|| {
            format!(
                "Pinned trust {} is corrupt; inspect it, or drop it with \
                 `actr realm trust {domain} --reset` and rejoin",
                path.display()
            )
        })?;
        if trust.schema != REALM_PINS_SCHEMA {
            bail!(
                "Pinned trust {} has schema '{}', expected '{REALM_PINS_SCHEMA}'; \
                 inspect it, or drop it with `actr realm trust {domain} --reset` and rejoin",
                path.display(),
                trust.schema
            );
        }
        if trust.domain != domain {
            bail!(
                "Pinned trust {} is for '{}', not '{domain}'",
                path.display(),
                trust.domain
            );
        }
        Ok(Some(trust))
    }

    fn save(&mut self, trust: &RealmTrust) -> Result<()> {
        let path = self.path(&trust.domain)?;
        std::fs::create_dir_all(&self.dir)
            .with_context(|| format!("Failed to create {}", self.dir.display()))?;
        write_atomic_private(&path, serde_json::to_string_pretty(trust)?.as_bytes())
            .with_context(|| format!("Failed to write pinned trust: {}", path.display()))
    }

    fn reset(&mut self, domain: &str) -> Result<bool> {
        let path = self.path(domain)?;
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => {
                Err(error).with_context(|| format!("Failed to remove {}", path.display()))
            }
        }
    }
}

/// In-memory [`TrustStore`], for tests and one-off joins.
#[derive(Default)]
pub struct MemoryTrustStore {
    domains: BTreeMap<String, RealmTrust>,
}

impl TrustStore for MemoryTrustStore {
    fn load(&mut self, domain: &str) -> Result<Option<RealmTrust>> {
        Ok(self.domains.get(domain).cloned())
    }

    fn save(&mut self, trust: &RealmTrust) -> Result<()> {
        self.domains.insert(trust.domain.clone(), trust.clone());
        Ok(())
    }

    fn reset(&mut self, domain: &str) -> Result<bool> {
        Ok(self.domains.remove(domain).is_some())
    }
}

/// Write `contents` atomically (same-directory temporary file + rename),
/// readable by its owner alone.
fn write_atomic_private(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    let mut temporary = match parent {
        Some(parent) => tempfile::NamedTempFile::new_in(parent)?,
        None => tempfile::NamedTempFile::new()?,
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        temporary
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    temporary.write_all(contents)?;
    temporary
        .persist(path)
        .map(|_| ())
        .map_err(|error| error.error)
}

/// `keys` restricted to the pinned trust set, so a listed key without a
/// chain to a pinned key verifies nothing (`authority-discovery.md` 4.4 step
/// four; the server's `pinned_keys` does the same).
fn pinned_view(mut keys: RealmSigningKeys, pins: &KeyPins) -> RealmSigningKeys {
    keys.keys = pins.signing_keys();
    keys
}

/// Drop the baselines whose signing key is no longer listed: their chain of
/// custody is gone (`realm-profile.md` section 6). Returns their Realm names.
fn gc_profiles(trust: &mut RealmTrust, keys: &RealmSigningKeys) -> Vec<String> {
    let mut dropped = Vec::new();
    trust.profiles.retain(|name, pin| {
        let keep = RealmProfile::verify(pin.document.as_bytes(), name, keys).is_ok();
        if !keep {
            dropped.push(name.clone());
        }
        keep
    });
    dropped
}

/// Accept `member`, the freshly fetched `realm_signing_keys` of `domain`,
/// under the stored pins. Returns the key view to verify Realm Profiles with
/// and the updated, not yet saved trust record.
///
/// An identity change — the member's signing key does not chain to a pinned
/// key — is refused: accepting it takes the explicit `actr realm trust`.
pub fn accept_member(
    store: &mut dyn TrustStore,
    domain: &str,
    member: &Value,
) -> Result<(RealmSigningKeys, RealmTrust)> {
    let stored = store.load(domain)?;
    let (keys, outcome) = RealmSigningKeys::accept(member, stored.as_ref().map(|t| &t.pins))
        .with_context(|| format!("{domain}: realm_signing_keys"))?;
    let pins = match outcome {
        KeyPinOutcome::Accepted(pins) => pins,
        KeyPinOutcome::IdentityChange(_) => bail!(
            "{domain}: realm_signing_keys is not signed by a key chained to the pinned keys: \
             an identity change. If the Realm operator confirmed the new keys out of band, \
             accept them with `actr realm trust {domain}`."
        ),
    };
    let verify_keys = pinned_view(keys, &pins);
    let mut trust = stored.unwrap_or_else(|| RealmTrust::new(domain, member, pins.clone()));
    trust.member = member.clone();
    trust.pins = pins;
    trust.checked_at = now();
    Ok((verify_keys, trust))
}

/// Check `profile` against the stored baseline and save `trust` with it as
/// the new baseline. A served Profile that is not a valid successor of the
/// baseline — a rollback, a same-sequence change, another provider — is
/// rejected with the [`actr_protocol::realm_profile::RealmProfileError`]
/// code (AC-021).
pub fn accept_profile(
    store: &mut dyn TrustStore,
    realm: &str,
    trust: &mut RealmTrust,
    keys: &RealmSigningKeys,
    profile: &RealmProfile,
    document: &str,
) -> Result<()> {
    for name in gc_profiles(trust, keys) {
        eprintln!(
            "note: {name}: the pinned Realm Profile's signing key is no longer listed; \
             its baseline was discarded"
        );
    }
    if let Some(pin) = trust.profiles.get(realm) {
        let previous = RealmProfile::verify(pin.document.as_bytes(), realm, keys)
            .context("The pinned Realm Profile baseline failed re-verification")?;
        previous.check_successor(profile).map_err(|error| {
            anyhow::anyhow!(
                "{realm}: the Realm Profile is not a valid successor of the pinned baseline \
                 ({}): {error}",
                error.code()
            )
        })?;
    }
    trust.profiles.insert(
        realm.to_string(),
        ProfilePin {
            sequence: profile.sequence,
            digest: hex::encode(Sha256::digest(document.as_bytes())),
            document: document.to_string(),
            provider_id: profile.provider.id.clone(),
            pinned_at: now(),
        },
    );
    store.save(trust)
}

#[derive(Args, Debug)]
pub struct TrustArgs {
    /// Realm domain whose signing keys to confirm (see `actr realm join`)
    #[arg(value_name = "DOMAIN")]
    pub domain: String,
    /// Drop all pinned trust for the domain instead of confirming its keys
    #[arg(long)]
    pub reset: bool,
}

/// `actr realm trust <domain>`.
pub async fn trust(args: &TrustArgs) -> Result<Value> {
    if args.reset {
        let domain = ActrUri::from_domain(&args.domain)
            .map_err(|error| anyhow::anyhow!("{error}"))?
            .domain()
            .to_string();
        let mut store = FileTrustStore::at_default()?;
        let removed = store.reset(&domain)?;
        return Ok(json!({ "domain": domain, "reset": removed }));
    }
    let client = discovery_client()?;
    let (_, member) = domain_authority(&args.domain, move |url| {
        let client = client.clone();
        async move { fetch_public(&client, url).await }
    })
    .await?;
    let mut store = FileTrustStore::at_default()?;
    trust_with(&args.domain, member, &mut store)
}

/// [`trust`] with the domain's freshly fetched `realm_signing_keys` member:
/// the explicit confirmation of an identity change that `join` refuses.
/// Profile baselines anchored on the previous keys do not survive it.
pub fn trust_with(domain: &str, member: Value, store: &mut dyn TrustStore) -> Result<Value> {
    let domain = ActrUri::from_domain(domain)
        .map_err(|error| anyhow::anyhow!("{error}"))?
        .domain()
        .to_string();
    let stored = store.load(&domain)?;
    let first_pin = stored.is_none();
    let (keys, outcome) = RealmSigningKeys::accept(&member, stored.as_ref().map(|t| &t.pins))
        .with_context(|| format!("{domain}: realm_signing_keys"))?;
    let (pins, identity_change) = match outcome {
        KeyPinOutcome::Accepted(pins) => (pins, false),
        KeyPinOutcome::IdentityChange(pins) => (pins, true),
    };
    let status = match (first_pin, identity_change) {
        (true, _) => "pinned",
        (false, false) => "confirmed",
        (false, true) => "identity-change-confirmed",
    };
    let verify_keys = pinned_view(keys, &pins);
    let mut trust = stored.unwrap_or_else(|| RealmTrust::new(&domain, &member, pins.clone()));
    trust.member = member;
    trust.pins = pins.clone();
    trust.checked_at = now();
    if identity_change {
        // The old keys no longer vouch for anything: Profile baselines
        // anchored on them are dropped with the old pins.
        trust.profiles.clear();
    } else {
        gc_profiles(&mut trust, &verify_keys);
    }
    store.save(&trust)?;
    Ok(json!({
        "domain": domain,
        "status": status,
        "sequence": pins.sequence,
        "keys": pins.keys.len(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actr_protocol::realm_profile::sign_document;
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as BASE64;
    use ed25519_dalek::SigningKey;

    fn key() -> SigningKey {
        SigningKey::from_bytes(&rand::random::<[u8; 32]>())
    }

    fn member(key_id: &str, key: &SigningKey, sequence: u64) -> Value {
        let mut member = json!({
            "sequence": sequence,
            "keys": [{
                "id": key_id,
                "algorithm": "Ed25519",
                "status": "active",
                "public_key": BASE64.encode(key.verifying_key().to_bytes()),
            }],
        });
        sign_document(&mut member, key_id, key);
        member
    }

    #[test]
    fn memory_store_round_trips_and_resets() {
        let mut store = MemoryTrustStore::default();
        assert!(store.load("acme.com").unwrap().is_none());
        assert!(!store.reset("acme.com").unwrap());

        let key = key();
        let member = member("realm-key-1", &key, 1);
        let (keys, outcome) = RealmSigningKeys::accept(&member, None).unwrap();
        let KeyPinOutcome::Accepted(pins) = outcome else {
            panic!("first member must be accepted");
        };
        let trust = RealmTrust::new("acme.com", &member, pins);
        store.save(&trust).unwrap();

        let loaded = store.load("acme.com").unwrap().unwrap();
        assert_eq!(loaded.domain, "acme.com");
        assert_eq!(loaded.pins.sequence, 1);
        assert_eq!(loaded.member, member);
        assert_eq!(keys.keys.len(), 1);
        assert!(store.reset("acme.com").unwrap());
        assert!(store.load("acme.com").unwrap().is_none());
    }

    #[test]
    fn accept_member_refuses_identity_change() {
        let mut store = MemoryTrustStore::default();
        let first = member("realm-key-1", &key(), 1);
        let (_, trust) = accept_member(&mut store, "acme.com", &first).unwrap();
        store.save(&trust).unwrap();

        // A member signed by an unrelated key at a higher sequence is an
        // identity change, not a rotation.
        let rogue = member("realm-key-9", &key(), 2);
        let error = accept_member(&mut store, "acme.com", &rogue).unwrap_err();
        assert!(error.to_string().contains("identity change"), "{error:#}");

        // The stored pins are untouched.
        let trust = store.load("acme.com").unwrap().unwrap();
        assert_eq!(trust.pins.sequence, 1);
    }

    #[test]
    fn accept_member_refuses_sequence_rollback() {
        let mut store = MemoryTrustStore::default();
        let key = key();
        let (_, trust) =
            accept_member(&mut store, "acme.com", &member("realm-key-1", &key, 31)).unwrap();
        store.save(&trust).unwrap();
        let error =
            accept_member(&mut store, "acme.com", &member("realm-key-1", &key, 30)).unwrap_err();
        assert!(!error.to_string().contains("identity change"), "{error:#}");
    }

    #[test]
    fn trust_confirms_an_identity_change_and_drops_baselines() {
        let mut store = MemoryTrustStore::default();
        let first = member("realm-key-1", &key(), 1);
        let (_, mut trust) = accept_member(&mut store, "acme.com", &first).unwrap();
        trust.profiles.insert(
            "42@acme.com".to_string(),
            ProfilePin {
                sequence: 3,
                digest: "ab".repeat(32),
                document: "{}".to_string(),
                provider_id: "provider.example".to_string(),
                pinned_at: now(),
            },
        );
        store.save(&trust).unwrap();

        let rogue = member("realm-key-9", &key(), 2);
        let reply = trust_with("acme.com", rogue.clone(), &mut store).unwrap();
        assert_eq!(reply["status"], "identity-change-confirmed");
        let trust = store.load("acme.com").unwrap().unwrap();
        assert_eq!(trust.pins.sequence, 2);
        assert!(trust.profiles.is_empty());

        // The new keys are the baseline now.
        accept_member(&mut store, "acme.com", &rogue).unwrap();
    }

    #[test]
    fn trust_first_pin_and_reset() {
        let mut store = MemoryTrustStore::default();
        let reply = trust_with("acme.com", member("realm-key-1", &key(), 5), &mut store).unwrap();
        assert_eq!(reply["status"], "pinned");
        assert_eq!(reply["sequence"], 5);
        assert!(store.reset("acme.com").unwrap());
    }

    #[test]
    fn file_store_fails_closed_on_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = FileTrustStore::new(dir.path().to_path_buf());
        std::fs::write(dir.path().join("acme.com.json"), b"not json").unwrap();
        let error = store.load("acme.com").unwrap_err();
        assert!(format!("{error:#}").contains("--reset"), "{error:#}");
    }

    #[test]
    fn file_store_round_trip_is_owner_private() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = FileTrustStore::new(dir.path().to_path_buf());
        let key = key();
        let member = member("realm-key-1", &key, 7);
        let (_, trust) = accept_member(&mut store, "acme.com", &member).unwrap();
        store.save(&trust).unwrap();

        let loaded = store.load("acme.com").unwrap().unwrap();
        assert_eq!(loaded.pins.sequence, 7);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(dir.path().join("acme.com.json"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "{mode:o}");
        }
    }
}
