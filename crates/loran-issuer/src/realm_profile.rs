//! Realm Profiles of the Realms this Loran serves as Realm Provider
//! (`actr/protocols/realm-profile.md`).
//!
//! For its own Provider domain Loran holds the Realm signing key in the
//! Signer (`realm-signing:<domain>`) and signs the Profiles itself. For other
//! domains it only keeps the Profiles their key holders signed, and refreshes
//! them together with the domain's pinned `realm_signing_keys`.

use actr_protocol::realm_profile::{
    IceEndpoint, IssuerKey, RealmIssuer, RealmPackage, RealmPolicy, RealmProfile,
    RealmProfileError, RealmProvider, SignalingEndpoint, jcs, realm_profile_urls,
};
use actr_protocol::{
    AuthorityDescriptor, DetachedSignature, KeyStatus, RealmSigningKey, RealmSigningKeys,
    SIGNATURE_ALGORITHM,
};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use platform::net::FetchError;
use platform::realm::domain::member_from_descriptor;
use platform::realm::{Realm, RealmDomain, RealmError, RealmProfileRecord, RealmStatus};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, OnceCell};

use crate::signer_client_wrapper::SignerClientWrapper;

/// The only membership policy of version 1.
const MEMBERSHIP: &str = "invite-only";

/// Signer key name, and descriptor key id, of a domain's Realm signing key.
pub fn realm_signing_key_id(domain: &str) -> String {
    format!("realm-signing:{domain}")
}

/// Signer key name, and Profile key id, of a Realm's issuer key.
pub fn issuer_key_id(realm: &str) -> String {
    format!("issuer:{realm}")
}

/// The Realm Provider endpoints a Loran-signed Realm Profile names.
#[derive(Debug, Clone)]
pub struct ProviderEndpoints {
    /// Realm Provider ID, also the domain whose Realm signing key Loran holds.
    pub provider_id: String,
    pub issuer_url: String,
    pub signaling: Vec<SignalingEndpoint>,
    pub ice: Vec<IceEndpoint>,
    /// `realms` location Loran serves, `<origin>/actr/realms/`.
    pub realms_base: String,
}

/// What a newly signed Realm Profile changes; unset members keep their
/// previous values.
#[derive(Debug, Clone, Default)]
pub struct ProfileChange {
    pub package: Option<RealmPackage>,
    pub display_name: Option<String>,
    pub retire: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Unavailable(String),
    #[error(transparent)]
    Realm(#[from] RealmError),
}

impl From<RealmProfileError> for ProfileError {
    fn from(error: RealmProfileError) -> Self {
        Self::Invalid(error.to_string())
    }
}

/// Signs and keeps Realm Profiles for the Realms this Loran serves.
pub struct RealmProfiles {
    endpoints: ProviderEndpoints,
    signer: SignerClientWrapper,
    /// Signer key id and the self-signed `realm_signing_keys` member.
    member: OnceCell<(u32, RealmSigningKeys)>,
    /// One Profile signed at a time, so sequences never collide.
    signing: Mutex<()>,
}

impl RealmProfiles {
    pub fn new(endpoints: ProviderEndpoints, signer: SignerClientWrapper) -> Self {
        Self {
            endpoints,
            signer,
            member: OnceCell::new(),
            signing: Mutex::new(()),
        }
    }

    pub fn provider_id(&self) -> &str {
        &self.endpoints.provider_id
    }

    /// Whether Loran holds the Realm signing key of `realm`'s domain.
    pub fn holds_key_for(&self, realm: &str) -> bool {
        realm
            .rsplit_once('@')
            .is_some_and(|(_, domain)| domain == self.endpoints.provider_id)
    }

    /// The Provider domain's `realm_signing_keys` member, once
    /// [`Self::ensure_signing_keys`] has produced it. Never calls the Signer.
    pub fn signing_keys(&self) -> Option<&RealmSigningKeys> {
        self.member.get().map(|(_, member)| member)
    }

    /// Generate (first time) the Realm signing key and self-sign the
    /// one-key `realm_signing_keys` member.
    pub async fn ensure_signing_keys(&self) -> anyhow::Result<&RealmSigningKeys> {
        let (_, member) = self
            .member
            .get_or_try_init(|| async {
                let key_id = realm_signing_key_id(&self.endpoints.provider_id);
                let (signer_key_id, public_key) = self.signer.ensure_named_key(&key_id).await?;
                let (mut member, payload) = unsigned_realm_signing_keys(&key_id, &public_key);
                let signature = self.signer.sign(signer_key_id, payload.as_bytes()).await?;
                member.signature.value = BASE64.encode(signature);
                anyhow::Ok((signer_key_id, member))
            })
            .await?;
        Ok(member)
    }

    /// Sign the next Realm Profile of `realm`, a Realm under the Provider
    /// domain, and accept it. The first active Profile activates a pending
    /// Realm; a retired one retires it.
    pub async fn issue(&self, realm: &str, change: ProfileChange) -> Result<Realm, ProfileError> {
        if !self.holds_key_for(realm) {
            return Err(ProfileError::Invalid(format!(
                "Loran signs Realm Profiles only for Realms under {}",
                self.endpoints.provider_id
            )));
        }
        let Some((signer_key_id, keys)) = self.member.get() else {
            return Err(ProfileError::Unavailable(
                "Realm signing key is not ready".to_string(),
            ));
        };
        let _signing = self.signing.lock().await;

        let current = Realm::get(realm).await?.ok_or(RealmError::NotFound)?;
        if current.status == RealmStatus::Retired {
            return Err(RealmError::Retired.into());
        }
        let previous = RealmProfileRecord::get(realm)
            .await?
            .map(|record| record.verify(keys))
            .transpose()?;

        let package = change
            .package
            .or_else(|| previous.as_ref().and_then(|p| p.package.clone()));
        if package.is_none() && !change.retire {
            return Err(ProfileError::Invalid(
                "an active Realm Profile needs package {href, sha256}".to_string(),
            ));
        }
        let display_name = change
            .display_name
            .or_else(|| previous.as_ref().map(|p| p.realm.display_name.clone()))
            .unwrap_or_else(|| realm.split('@').next().unwrap_or_default().to_string());
        let issuer_key = issuer_key_id(realm);
        let (_, issuer_public_key) = self
            .signer
            .ensure_named_key(&issuer_key)
            .await
            .map_err(|error| ProfileError::Unavailable(format!("Signer: {error}")))?;

        let mut document = json!({
            "schema": actr_protocol::realm_profile::REALM_PROFILE_SCHEMA,
            "sequence": previous.as_ref().map_or(1, |p| p.sequence + 1),
            "realm": {
                "name": realm,
                "display_name": display_name,
                "status": if change.retire { "retired" } else { "active" },
            },
            "issuer": RealmIssuer {
                url: self.endpoints.issuer_url.clone(),
                keys: vec![IssuerKey {
                    id: issuer_key,
                    algorithm: SIGNATURE_ALGORITHM.to_string(),
                    public_key: BASE64.encode(issuer_public_key),
                    status: KeyStatus::Active,
                }],
            },
            "provider": RealmProvider {
                id: self.endpoints.provider_id.clone(),
                signaling: self.endpoints.signaling.clone(),
                ice: self.endpoints.ice.clone(),
            },
            "policy": RealmPolicy { membership: MEMBERSHIP.to_string() },
            "issued_at": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        });
        if let Some(package) = package {
            document["package"] = json!(package);
        }
        let signature = self
            .signer
            .sign(*signer_key_id, jcs(&document).as_bytes())
            .await
            .map_err(|error| ProfileError::Unavailable(format!("Signer: {error}")))?;
        document["signature"] = json!(DetachedSignature {
            key_id: keys.signature.key_id.clone(),
            algorithm: SIGNATURE_ALGORITHM.to_string(),
            value: BASE64.encode(signature),
        });
        let document = document.to_string();

        // Accept it exactly as a Realm Provider or Actr Host would.
        let profile = RealmProfile::verify(document.as_bytes(), realm, keys)?;
        if let Some(previous) = &previous {
            previous.check_successor(&profile)?;
        }
        Ok(RealmProfileRecord::store(&profile, &document).await?)
    }

    /// Refresh the Realms of other domains that have an accepted Profile:
    /// accept each domain's current `realm_signing_keys` under its pin, then
    /// take a newer Profile from the domain's `realms` locations.
    ///
    /// A Profile whose signing key the domain removed is dropped as a
    /// baseline; without a replacement the Realm is suspended.
    pub async fn refresh<F, Fut>(&self, fetch: F) -> Result<(), RealmError>
    where
        F: Fn(String) -> Fut,
        Fut: Future<Output = Result<Arc<[u8]>, FetchError>>,
    {
        let mut domains: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for realm in Realm::get_all().await? {
            if realm.status == RealmStatus::Retired || self.holds_key_for(&realm.name) {
                continue;
            }
            if RealmProfileRecord::get(&realm.name).await?.is_none() {
                continue;
            }
            if let Some((_, domain)) = realm.name.rsplit_once('@') {
                domains
                    .entry(domain.to_string())
                    .or_default()
                    .push(realm.name.clone());
            }
        }

        for (domain, realms) in domains {
            let descriptor = match fetch(format!("https://{domain}/.well-known/actr")).await {
                Ok(bytes) => bytes,
                Err(error) => {
                    platform::recording::warn!("Realm domain {domain} descriptor: {error}");
                    continue;
                }
            };
            let member = match member_from_descriptor(&domain, &descriptor) {
                Ok(member) => member,
                Err(error) => {
                    platform::recording::warn!("Realm domain {domain} descriptor: {error}");
                    continue;
                }
            };
            let keys = match RealmDomain::accept(&domain, &member).await {
                Ok(pinned) => pinned.keys,
                Err(error) => {
                    platform::recording::warn!("Realm domain {domain} keys not accepted: {error}");
                    match RealmDomain::get(&domain).await? {
                        Some(pinned) => pinned.keys,
                        None => continue,
                    }
                }
            };
            // Already validated by `member_from_descriptor`.
            let Ok(descriptor) = serde_json::from_slice::<AuthorityDescriptor>(&descriptor) else {
                continue;
            };
            for realm in realms {
                if let Err(error) = self.refresh_realm(&realm, &descriptor, &keys, &fetch).await {
                    platform::recording::warn!("Realm {realm} refresh: {error}");
                }
            }
        }
        Ok(())
    }

    async fn refresh_realm<F, Fut>(
        &self,
        realm: &str,
        descriptor: &AuthorityDescriptor,
        keys: &RealmSigningKeys,
        fetch: &F,
    ) -> Result<(), ProfileError>
    where
        F: Fn(String) -> Fut,
        Fut: Future<Output = Result<Arc<[u8]>, FetchError>>,
    {
        let Some(record) = RealmProfileRecord::get(realm).await? else {
            return Ok(());
        };
        let accepted = match record.verify(keys) {
            Ok(profile) => Some(profile),
            Err(error) => {
                platform::recording::warn!(
                    "Accepted Realm Profile of {realm} no longer verifies: {error}"
                );
                None
            }
        };

        let mut fetched = None;
        for url in realm_profile_urls(descriptor, realm)? {
            // Loran already holds what it serves itself.
            if url.starts_with(&self.endpoints.realms_base) {
                continue;
            }
            let document = match fetch(url.clone()).await {
                Ok(document) => document,
                Err(error) => {
                    platform::recording::warn!("Realm Profile {url}: {error}");
                    continue;
                }
            };
            match RealmProfile::verify(&document, realm, keys) {
                Ok(profile) => {
                    fetched = Some((profile, document));
                    break;
                }
                Err(error) => platform::recording::warn!("Realm Profile {url}: {error}"),
            }
        }

        match (accepted, fetched) {
            (Some(accepted), Some((profile, document))) => {
                accepted.check_successor(&profile)?;
                if profile.sequence > accepted.sequence {
                    store_fetched(&profile, &document).await?;
                }
            }
            (None, Some((profile, document))) if profile.provider.id == self.provider_id() => {
                store_fetched(&profile, &document).await?;
            }
            (None, _) => {
                platform::recording::warn!(
                    "Suspending Realm {realm}: no Realm Profile signed by a listed key"
                );
                Realm::set_enabled(realm, false).await?;
            }
            (Some(_), None) => {}
        }
        Ok(())
    }

    /// Run in the background until `cancel`: generate the Realm signing key
    /// (retrying while the Signer comes up), then refresh other domains'
    /// Realms every `interval`.
    pub fn spawn(
        self: &Arc<Self>,
        interval: Duration,
        cancel: tokio_util::sync::CancellationToken,
    ) {
        let profiles = self.clone();
        tokio::spawn(async move {
            while let Err(error) = profiles.ensure_signing_keys().await {
                platform::recording::warn!("Realm signing key not ready: {error:#}");
                tokio::select! {
                    _ = cancel.cancelled() => return,
                    _ = tokio::time::sleep(Duration::from_secs(2)) => {}
                }
            }
            platform::recording::info!(
                "Realm signing key ready: {}",
                realm_signing_key_id(profiles.provider_id())
            );

            // Every round fetches one descriptor per domain and one Profile per
            // Realm, often from a single host. A per-host limit would leave the
            // Realms past it unrefreshed, and suspend them when a key is revoked.
            let fetcher = &platform::net::CachedFetcher::new(Duration::from_secs(300), u32::MAX);
            loop {
                let fetch = |url: String| async move { fetcher.get(&url).await };
                if let Err(error) = profiles.refresh(fetch).await {
                    platform::recording::warn!("Realm Profile refresh failed: {error}");
                }
                tokio::select! {
                    _ = cancel.cancelled() => return,
                    _ = tokio::time::sleep(interval) => {}
                }
            }
        });
    }
}

async fn store_fetched(profile: &RealmProfile, document: &[u8]) -> Result<(), ProfileError> {
    let document = std::str::from_utf8(document)
        .map_err(|_| ProfileError::Invalid("Realm Profile is not UTF-8".to_string()))?;
    RealmProfileRecord::store(profile, document).await?;
    Ok(())
}

/// A one-key `realm_signing_keys` member with an empty signature value, and
/// the RFC 8785 JCS payload that the key must sign (the member without
/// `signature`).
pub fn unsigned_realm_signing_keys(
    key_id: &str,
    public_key: &[u8; 32],
) -> (RealmSigningKeys, String) {
    let member = RealmSigningKeys {
        sequence: 1,
        keys: vec![RealmSigningKey {
            id: key_id.to_string(),
            algorithm: SIGNATURE_ALGORITHM.to_string(),
            public_key: BASE64.encode(public_key),
            status: KeyStatus::Active,
        }],
        rotations: None,
        signature: DetachedSignature {
            key_id: key_id.to_string(),
            algorithm: SIGNATURE_ALGORITHM.to_string(),
            value: String::new(),
        },
    };
    let mut payload: Value = serde_json::to_value(&member).expect("member serializes");
    if let Some(members) = payload.as_object_mut() {
        members.remove("signature");
    }
    (member, jcs(&payload))
}
