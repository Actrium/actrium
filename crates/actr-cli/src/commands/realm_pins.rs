//! Pinned Realm trust (FR-NET-015, AC-021): the accepted `realm_signing_keys`
//! of each Realm domain and the accepted Realm Profile baselines, persisted
//! as `~/.actr/realms/<domain>.json`.
//!
//! Joining a Realm accepts the domain's signing keys under the stored pins
//! (`actr/protocols/authority-discovery.md` section 4.4) instead of trusting
//! every fetch anew: a member that rolls the accepted `sequence` back or
//! changes it without a rotation chain is rejected, and a signing key that
//! does not chain to a pinned key is an identity change the user confirms
//! explicitly with `actr realm trust <domain>`. Acceptance is persisted when
//! the member is accepted, so a first join that never reaches a Profile
//! still leaves a pin. The accepted Realm Profile is kept verbatim per
//! Realm, so a later Profile must be a valid successor of the stored
//! baseline (`actr/protocols/realm-profile.md` section 5).

use actr_protocol::realm_profile::RealmProfile;
use actr_protocol::uri::ActrUri;
use actr_protocol::{KeyPinOutcome, KeyPins, PinnedStatus, RealmSigningKeys};
use anyhow::{Context, Result, bail};
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::PathBuf;

use super::realm_members::{discovery_client, domain_authority, fetch_public};

pub const REALM_PINS_SCHEMA: &str = "actr.realm-pins/v1";

/// The accepted Realm Profile baseline of one Realm.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfilePin {
    pub sequence: u64,
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
    /// Compare-and-swap generation of the on-disk record: every save must
    /// find the revision its writer loaded, so a concurrent join/trust
    /// cannot be overwritten (and roll the pins back) unnoticed.
    #[serde(default)]
    pub revision: u64,
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
            revision: 0,
            profiles: BTreeMap::new(),
        }
    }
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// The domain in its normalized (IDNA, lowercase) form: the pin file name
/// and every store key.
fn normalized_domain(domain: &str) -> Result<String> {
    Ok(ActrUri::from_domain(domain)
        .map_err(|error| anyhow::anyhow!("{error}"))?
        .domain()
        .to_string())
}

/// Where pinned trust is kept. `load` fails closed: a corrupt or foreign
/// store is an error, never a silent re-pin.
pub trait TrustStore: Send {
    fn load(&mut self, domain: &str) -> Result<Option<RealmTrust>>;
    fn save(&mut self, trust: &RealmTrust) -> Result<()>;
    /// Drop everything pinned for `domain`; whether anything was pinned.
    fn reset(&mut self, domain: &str) -> Result<bool>;
}

/// [`TrustStore`] as `~/.actr/realms/<domain>.json` (or another directory),
/// with compare-and-swap saves across processes.
pub struct FileTrustStore {
    dir: PathBuf,
    /// The on-disk revision each domain's record was loaded at (0 = absent).
    expected: BTreeMap<String, u64>,
}

impl FileTrustStore {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            expected: BTreeMap::new(),
        }
    }

    /// The default store, `~/.actr/realms`. `ACTR_HOME`, when set, replaces
    /// `~/.actr`: test sandboxes and containers without a home directory.
    pub fn at_default() -> Result<Self> {
        let home = std::env::var_os("ACTR_HOME")
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|home| home.join(".actr")))
            .context("Cannot locate the home directory; set ACTR_HOME")?;
        Ok(Self::new(home.join("realms")))
    }

    fn path(&self, domain: &str) -> Result<PathBuf> {
        Ok(self
            .dir
            .join(format!("{}.json", normalized_domain(domain)?)))
    }

    /// The on-disk record and its revision, tolerating absence only.
    fn read_current(&self, domain: &str) -> Result<Option<(RealmTrust, u64)>> {
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
        let revision = trust.revision;
        Ok(Some((trust, revision)))
    }
}

impl TrustStore for FileTrustStore {
    fn load(&mut self, domain: &str) -> Result<Option<RealmTrust>> {
        let current = self.read_current(domain)?;
        self.expected.insert(
            domain.to_string(),
            current.as_ref().map(|(_, revision)| *revision).unwrap_or(0),
        );
        Ok(current.map(|(trust, _)| trust))
    }

    fn save(&mut self, trust: &RealmTrust) -> Result<()> {
        // Compare-and-swap: only the writer who loaded the record at this
        // revision may replace it. A concurrent join/trust bumps it and the
        // loser is told to re-run instead of silently overwriting newer pins.
        let current = self.read_current(&trust.domain)?;
        let current_revision = current.map(|(_, revision)| revision).unwrap_or(0);
        let expected = self.expected.get(&trust.domain).copied().unwrap_or(0);
        if current_revision != expected {
            bail!(
                "Pinned trust for {} changed concurrently (revision {expected} -> \
                 {current_revision}); re-run the command",
                trust.domain
            );
        }
        let mut trust = trust.clone();
        trust.revision = current_revision + 1;
        let path = self.path(&trust.domain)?;
        std::fs::create_dir_all(&self.dir)
            .with_context(|| format!("Failed to create {}", self.dir.display()))?;
        super::realm::write_private(&path, serde_json::to_string_pretty(&trust)?.as_bytes())
            .with_context(|| format!("Failed to write pinned trust: {}", path.display()))?;
        self.expected.insert(trust.domain.clone(), trust.revision);
        Ok(())
    }

    fn reset(&mut self, domain: &str) -> Result<bool> {
        let path = self.path(domain)?;
        match std::fs::remove_file(&path) {
            Ok(()) => {
                self.expected.insert(domain.to_string(), 0);
                Ok(true)
            }
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

/// `keys` restricted to the pinned trust set, so a listed key without a
/// chain to a pinned key verifies nothing (`authority-discovery.md` 4.4 step
/// four; the server's `pinned_keys` does the same).
fn pinned_view(mut keys: RealmSigningKeys, pins: &KeyPins) -> RealmSigningKeys {
    keys.keys = pins.signing_keys();
    keys
}

/// Drop the baselines whose signing key is no longer listed: their chain of
/// custody is gone (`realm-profile.md` section 6). Returns the baselines
/// that still verify, already re-verified for the caller to reuse.
fn gc_profiles(trust: &mut RealmTrust, keys: &RealmSigningKeys) -> BTreeMap<String, RealmProfile> {
    let mut kept = BTreeMap::new();
    let mut dropped = Vec::new();
    trust.profiles.retain(|name, pin| {
        match RealmProfile::verify(pin.document.as_bytes(), name, keys) {
            Ok(profile) => {
                kept.insert(name.clone(), profile);
                true
            }
            Err(_) => {
                dropped.push(name.clone());
                false
            }
        }
    });
    for name in dropped {
        eprintln!(
            "note: {name}: the pinned Realm Profile's signing key is no longer listed; \
             its baseline was discarded"
        );
    }
    kept
}

/// What accepting a member produced: the key view for Profile verification,
/// the refreshed (not yet saved) trust record, and the baselines that were
/// re-verified while pruning (`realm-profile.md` section 6).
#[derive(Debug)]
pub struct AcceptedMember {
    pub keys: RealmSigningKeys,
    pub trust: RealmTrust,
    pub verified_profiles: BTreeMap<String, RealmProfile>,
    pub identity_change: bool,
}

/// Shared acceptance core: accept `member` under the stored pins, refresh
/// the trust record, and prune baselines when the member changed. Saves
/// nothing: the caller decides when to persist (join saves on acceptance,
/// `trust` saves on confirmation).
fn accept_into(store: &mut dyn TrustStore, domain: &str, member: &Value) -> Result<AcceptedMember> {
    let stored = store.load(domain)?;
    let member_changed = stored.as_ref().is_some_and(|t| t.member != *member);
    let (keys, outcome) = RealmSigningKeys::accept(member, stored.as_ref().map(|t| &t.pins))
        .with_context(|| format!("{domain}: realm_signing_keys"))?;
    let (pins, identity_change) = match outcome {
        KeyPinOutcome::Accepted(pins) => (pins, false),
        KeyPinOutcome::IdentityChange(pins) => (pins, true),
    };
    let verify_keys = pinned_view(keys, &pins);
    let mut trust = stored.unwrap_or_else(|| RealmTrust::new(domain, member, pins.clone()));
    if trust.pins.digest != pins.digest {
        // `pinned_at` is when the current keys were accepted, not the first
        // contact: a rotation or an identity confirmation replaces it.
        trust.pinned_at = now();
    }
    trust.member = member.clone();
    trust.pins = pins;
    trust.checked_at = now();
    let verified_profiles = if member_changed {
        gc_profiles(&mut trust, &verify_keys)
    } else {
        BTreeMap::new()
    };
    Ok(AcceptedMember {
        keys: verify_keys,
        trust,
        verified_profiles,
        identity_change,
    })
}

/// Accept `member`, the freshly fetched `realm_signing_keys` of `domain`,
/// under the stored pins, and persist the acceptance immediately: a first
/// join that fails to fetch any Profile still leaves the pin, so retrying
/// in another network cannot re-pin an attacker's keys as first trust.
///
/// An identity change — the member's signing key does not chain to a pinned
/// key — is refused without saving: accepting it takes the explicit
/// `actr realm trust`.
pub fn accept_member(
    store: &mut dyn TrustStore,
    domain: &str,
    member: &Value,
) -> Result<AcceptedMember> {
    let accepted = accept_into(store, domain, member)?;
    if accepted.identity_change {
        bail!(
            "{domain}: realm_signing_keys is not signed by a key chained to the pinned keys: \
             an identity change. If the Realm operator confirmed the new keys out of band, \
             accept them with `actr realm trust {domain}`."
        );
    }
    store.save(&accepted.trust)?;
    Ok(accepted)
}

/// Check `profile` against the stored baseline and save `trust` with it as
/// the new baseline. A served Profile that is not a valid successor of the
/// baseline — a rollback, a same-sequence change, another provider — is
/// rejected with the [`actr_protocol::realm_profile::RealmProfileError`]
/// code (AC-021).
pub fn accept_profile(
    store: &mut dyn TrustStore,
    realm: &str,
    accepted: &mut AcceptedMember,
    profile: &RealmProfile,
    document: &str,
) -> Result<()> {
    // The baseline was already re-verified when the member changed; only
    // verify it here when it was not (one verification per join at most).
    let previous: Option<RealmProfile> = match accepted.verified_profiles.get(realm) {
        Some(previous) => Some(previous.clone()),
        None => match accepted.trust.profiles.get(realm) {
            Some(pin) => Some(
                RealmProfile::verify(pin.document.as_bytes(), realm, &accepted.keys)
                    .context("The pinned Realm Profile baseline failed re-verification")?,
            ),
            None => None,
        },
    };
    if let Some(previous) = previous {
        previous.check_successor(profile).map_err(|error| {
            anyhow::anyhow!(
                "{realm}: the Realm Profile is not a valid successor of the pinned baseline \
                 ({}): {error}",
                error.code()
            )
        })?;
    }
    accepted.trust.profiles.insert(
        realm.to_string(),
        ProfilePin {
            sequence: profile.sequence,
            document: document.to_string(),
            provider_id: profile.provider.id.clone(),
            pinned_at: now(),
        },
    );
    store.save(&accepted.trust)
}

/// SHA-256 hex fingerprint of a base64 Ed25519 public key, for out-of-band
/// comparison (the issuer prints the same digest).
fn fingerprint(public_key: &str) -> String {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(public_key)
        .unwrap_or_default();
    hex::encode(Sha256::digest(&bytes))
}

/// The verifiable content of a pin: its digest and per-key fingerprints,
/// including each key's status so Removed keys cannot confuse a comparison.
fn trust_json(domain: &str, pins: &KeyPins) -> Value {
    let keys: Vec<Value> = pins
        .keys
        .iter()
        .filter(|key| key.status != PinnedStatus::Removed)
        .map(|key| {
            json!({
                "id": key.id,
                "status": match key.status {
                    PinnedStatus::Active => "active",
                    PinnedStatus::Retired => "retired",
                    PinnedStatus::Removed => unreachable!("filtered above"),
                },
                "fingerprint": fingerprint(&key.public_key),
            })
        })
        .collect();
    json!({
        "domain": domain,
        "sequence": pins.sequence,
        "digest": pins.digest,
        "keys": keys,
    })
}

#[derive(Args, Debug)]
pub struct TrustArgs {
    /// Realm domain whose signing keys to confirm (see `actr realm join`)
    #[arg(value_name = "DOMAIN")]
    pub domain: String,
    /// Drop all pinned trust for the domain instead of confirming its keys
    #[arg(long)]
    pub reset: bool,
    /// Show the pinned keys and their fingerprints, changing nothing
    #[arg(long, conflicts_with = "reset")]
    pub show: bool,
    /// Only pin the served keys if their digest (SHA-256 hex, see --show)
    /// matches this value: the out-of-band verification of a confirmation
    #[arg(long, value_name = "SHA256", conflicts_with = "reset")]
    pub expect: Option<String>,
}

/// `actr realm trust <domain>`.
pub async fn trust(args: &TrustArgs) -> Result<Value> {
    let domain = normalized_domain(&args.domain)?;
    let mut store = FileTrustStore::at_default()?;
    if args.reset {
        let removed = store.reset(&domain)?;
        return Ok(json!({ "domain": domain, "reset": removed }));
    }
    if args.show {
        let stored = store.load(&domain)?;
        return Ok(match stored {
            Some(trust) => {
                let mut reply = trust_json(&domain, &trust.pins);
                reply["pinned"] = json!(true);
                reply
            }
            None => json!({ "domain": domain, "pinned": false }),
        });
    }
    let client = discovery_client()?;
    let (_, member) = domain_authority(&args.domain, move |url| {
        let client = client.clone();
        async move { fetch_public(&client, url).await }
    })
    .await?;
    trust_with(&args.domain, member, &mut store, args.expect.as_deref())
}

/// [`trust`] with the domain's freshly fetched `realm_signing_keys` member:
/// the explicit confirmation of an identity change that `join` refuses.
/// Baselines are pruned only when their signing key is gone from the new
/// member (`realm-profile.md` section 6) — baselines signed by a key that
/// stays listed survive the confirmation.
pub fn trust_with(
    domain: &str,
    member: Value,
    store: &mut dyn TrustStore,
    expect: Option<&str>,
) -> Result<Value> {
    let domain = normalized_domain(domain)?;
    let first_pin = store.load(&domain)?.is_none();
    let accepted = accept_into(store, &domain, &member)?;
    let pins = &accepted.trust.pins;
    if let Some(expected) = expect
        && pins.digest != expected
    {
        bail!(
            "{domain}: the served keys' digest {} does not match the expected {expected}; \
             nothing was pinned",
            pins.digest
        );
    }
    let status = match (first_pin, accepted.identity_change) {
        (true, _) => "pinned",
        (false, false) => "confirmed",
        (false, true) => "identity-change-confirmed",
    };
    store.save(&accepted.trust)?;
    let mut reply = trust_json(&domain, pins);
    reply["status"] = json!(status);
    Ok(reply)
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

    /// A member listing `extra` alongside the signing key.
    fn member_with(
        key_id: &str,
        key: &SigningKey,
        sequence: u64,
        extra: (&str, &SigningKey),
    ) -> Value {
        let mut member = member(key_id, key, sequence);
        member["keys"].as_array_mut().unwrap().push(json!({
            "id": extra.0,
            "algorithm": "Ed25519",
            "status": "active",
            "public_key": BASE64.encode(extra.1.verifying_key().to_bytes()),
        }));
        sign_document(&mut member, key_id, key);
        member
    }

    /// A valid Realm Profile document signed by `key` as `key_id`.
    fn profile(key: &SigningKey, key_id: &str, sequence: u64) -> Value {
        let mut profile = json!({
            "schema": "actr.realm/v1",
            "sequence": sequence,
            "realm": { "name": "42@acme.com", "display_name": "Notes", "status": "active" },
            "package": {
                "href": "https://cdn.example/a.actr",
                "sha256": "ab".repeat(32),
                "type": "acme.com/notes@1.0.0",
            },
            "issuer": { "url": "https://issuer.example", "keys": [{
                "id": "7", "algorithm": "Ed25519", "status": "active",
                "public_key": BASE64.encode([2u8; 32]),
            }] },
            "provider": { "id": "provider.example", "signaling": [], "ice": [] },
            "policy": { "membership": "invite-only" },
            "issued_at": "2026-09-25T00:00:00Z",
        });
        sign_document(&mut profile, key_id, key);
        profile
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
        accept_member(&mut store, "acme.com", &first).unwrap();

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
        accept_member(&mut store, "acme.com", &member("realm-key-1", &key, 31)).unwrap();
        let error =
            accept_member(&mut store, "acme.com", &member("realm-key-1", &key, 30)).unwrap_err();
        assert!(!error.to_string().contains("identity change"), "{error:#}");
    }

    #[test]
    fn accept_member_persists_immediately() {
        let mut store = MemoryTrustStore::default();
        accept_member(&mut store, "acme.com", &member("realm-key-1", &key(), 1)).unwrap();
        // No Profile was fetched, yet the pin is on record: a retry in a
        // hostile network sees the identity change, not a fresh first pin.
        assert!(store.load("acme.com").unwrap().is_some());
    }

    #[test]
    fn identity_change_prunes_only_baselines_whose_key_is_gone() {
        let mut store = MemoryTrustStore::default();
        let first = key();
        let second = key();
        accept_member(&mut store, "acme.com", &member("realm-key-1", &first, 1)).unwrap();
        // A baseline signed by the first key, at sequence 3.
        let document = profile(&first, "realm-key-1", 3).to_string();
        let keys = pinned_view(
            serde_json::from_value(store.load("acme.com").unwrap().unwrap().member).unwrap(),
            &store.load("acme.com").unwrap().unwrap().pins,
        );
        let verified = RealmProfile::verify(document.as_bytes(), "42@acme.com", &keys).unwrap();
        let mut accepted = AcceptedMember {
            keys,
            trust: store.load("acme.com").unwrap().unwrap(),
            verified_profiles: BTreeMap::new(),
            identity_change: false,
        };
        accept_profile(
            &mut store,
            "42@acme.com",
            &mut accepted,
            &verified,
            &document,
        )
        .unwrap();

        // A member signed by an unrelated second key, still listing the
        // first: an identity change, but the first key still vouches for
        // the baseline, which must survive the confirmation.
        let rogue = member_with("realm-key-9", &second, 2, ("realm-key-1", &first));
        let reply = trust_with("acme.com", rogue, &mut store, None).unwrap();
        assert_eq!(reply["status"], "identity-change-confirmed");
        let trust = store.load("acme.com").unwrap().unwrap();
        assert!(trust.profiles.contains_key("42@acme.com"));

        // And the baseline still refuses a rollback afterwards.
        let older = profile(&first, "realm-key-1", 2).to_string();
        let mut accepted = AcceptedMember {
            keys: pinned_view(
                serde_json::from_value(trust.member.clone()).unwrap(),
                &trust.pins,
            ),
            trust,
            verified_profiles: BTreeMap::new(),
            identity_change: false,
        };
        let older_profile =
            RealmProfile::verify(older.as_bytes(), "42@acme.com", &accepted.keys).unwrap();
        let error = accept_profile(
            &mut store,
            "42@acme.com",
            &mut accepted,
            &older_profile,
            &older,
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("REALM_PROFILE_ROLLBACK"),
            "{error:#}"
        );
    }

    #[test]
    fn trust_expect_binds_the_confirmation_to_a_digest() {
        let mut store = MemoryTrustStore::default();
        let key = key();
        let member = member("realm-key-1", &key, 5);
        let error = trust_with("acme.com", member.clone(), &mut store, Some("00")).unwrap_err();
        assert!(error.to_string().contains("does not match"), "{error:#}");
        assert!(store.load("acme.com").unwrap().is_none());

        // With the right digest the pin is accepted.
        let (_, outcome) = RealmSigningKeys::accept(&member, None).unwrap();
        let KeyPinOutcome::Accepted(pins) = outcome else {
            panic!()
        };
        let reply = trust_with("acme.com", member, &mut store, Some(&pins.digest)).unwrap();
        assert_eq!(reply["status"], "pinned");
        assert_eq!(reply["digest"], pins.digest.as_str());
        assert_eq!(reply["keys"][0]["fingerprint"].as_str().unwrap().len(), 64);
    }

    #[test]
    fn trust_first_pin_and_reset() {
        let mut store = MemoryTrustStore::default();
        let reply = trust_with(
            "acme.com",
            member("realm-key-1", &key(), 5),
            &mut store,
            None,
        )
        .unwrap();
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
        accept_member(&mut store, "acme.com", &member).unwrap();

        let loaded = store.load("acme.com").unwrap().unwrap();
        assert_eq!(loaded.pins.sequence, 7);
        assert_eq!(loaded.revision, 1);
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

    #[test]
    fn file_store_refuses_a_stale_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let key = key();
        // Two independent stores, as two processes would have.
        let mut first = FileTrustStore::new(dir.path().to_path_buf());
        let mut second = FileTrustStore::new(dir.path().to_path_buf());
        let first_member = member("realm-key-1", &key, 1);
        accept_member(&mut first, "acme.com", &first_member).unwrap();
        accept_member(&mut second, "acme.com", &first_member).unwrap();

        // The second store read the record at revision 2; while it was
        // away, the first store advanced it. The stale save must fail, not
        // roll the pins back.
        let stale = second.load("acme.com").unwrap().unwrap();
        accept_member(&mut first, "acme.com", &member("realm-key-1", &key, 2)).unwrap();
        let error = second.save(&stale).unwrap_err();
        assert!(error.to_string().contains("concurrently"), "{error:#}");
        assert_eq!(second.load("acme.com").unwrap().unwrap().pins.sequence, 2);
    }
}
