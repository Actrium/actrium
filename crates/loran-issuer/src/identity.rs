//! Operator re-confirmation after an identity change
//! (`actr/protocols/authority-discovery.md` section 4.4 step 6).
//!
//! A Realm domain's `realm_signing_keys`, or a publisher's Publisher Profile
//! signing keys, that do not chain to the pinned keys are refused. The
//! operator compares the pinned keys with the ones the domain serves now and
//! may take the new ones as the baseline. Nothing here runs on its own.

use actr_protocol::{KeyPinOutcome, KeyPins, PinnedStatus, PublisherProfile, RealmSigningKeys};
use actrix_publisher_registry::PublisherRegistryManager;
use actrix_publisher_registry::model::Publisher;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use chrono::Utc;
use platform::net::FetchError;
use platform::realm::RealmDomain;
use platform::realm::domain::member_from_descriptor;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::future::Future;
use std::sync::Arc;

use crate::realm_management::{ManagementError, publisher_from_descriptor};

/// SHA-256 hex of a base64 public key's bytes, for the operator to compare
/// out of band.
fn fingerprint(public_key: &str) -> String {
    let bytes = BASE64
        .decode(public_key)
        .unwrap_or_else(|_| public_key.as_bytes().to_vec());
    hex::encode(Sha256::digest(bytes))
}

fn keys_view(pins: &KeyPins) -> Value {
    json!({
        "sequence": pins.sequence,
        "digest": pins.digest,
        "keys": pins.keys.iter().map(|key| json!({
            "id": key.id,
            "status": match key.status {
                PinnedStatus::Active => "active",
                PinnedStatus::Retired => "retired",
                PinnedStatus::Removed => "removed",
            },
            "fingerprint": fingerprint(&key.public_key),
        })).collect::<Vec<_>>(),
    })
}

/// The pins a document would get as a first accept.
fn own_pins(outcome: KeyPinOutcome) -> KeyPins {
    match outcome {
        KeyPinOutcome::Accepted(pins) | KeyPinOutcome::IdentityChange(pins) => pins,
    }
}

/// `outcome` of the served document under the pins, and why when refused.
fn outcome_view<T, E: std::fmt::Display>(outcome: &Result<(T, KeyPinOutcome), E>) -> Value {
    match outcome {
        Ok((_, KeyPinOutcome::Accepted(_))) => json!({ "outcome": "accepted" }),
        Ok((_, KeyPinOutcome::IdentityChange(_))) => json!({ "outcome": "identity_change" }),
        Err(error) => json!({ "outcome": "rejected", "reason": error.to_string() }),
    }
}

fn merge(mut view: Value, extra: Value) -> Value {
    if let (Some(view), Value::Object(extra)) = (view.as_object_mut(), extra) {
        view.extend(extra);
    }
    view
}

async fn descriptor<F, Fut>(domain: &str, fetch: F) -> Result<Arc<[u8]>, ManagementError>
where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = Result<Arc<[u8]>, FetchError>>,
{
    fetch(format!("https://{domain}/.well-known/actr"))
        .await
        .map_err(|error| ManagementError::Unavailable(format!("{domain}: {error}")))
}

/// The pinned and the served `realm_signing_keys` of `domain`, and how the
/// served member fares under the pins: `accepted`, `identity_change` or
/// `rejected`. Only an `identity_change` or `rejected` member that is valid
/// on its own can be confirmed, by its `fetched.digest`.
pub async fn realm_domain_identity<F, Fut>(domain: &str, fetch: F) -> Result<Value, ManagementError>
where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = Result<Arc<[u8]>, FetchError>>,
{
    let pinned = RealmDomain::get(domain)
        .await?
        .ok_or_else(|| ManagementError::NotFound(format!("no keys are pinned for {domain}")))?;
    let member = member_from_descriptor(domain, &descriptor(domain, fetch).await?)
        .map_err(|error| ManagementError::Unavailable(error.to_string()))?;
    let fetched = RealmSigningKeys::accept(&member, None)
        .map_err(|error| ManagementError::Unavailable(format!("{domain}: {error}")))?;
    let view = json!({
        "domain": domain,
        "pinned": keys_view(&pinned.pins),
        "pinned_at": pinned.pinned_at,
        "fetched": keys_view(&own_pins(fetched.1)),
    });
    Ok(merge(
        view,
        outcome_view(&RealmSigningKeys::accept(&member, Some(&pinned.pins))),
    ))
}

/// Take the `realm_signing_keys` `domain` serves now as its new baseline;
/// `digest` is the `fetched.digest` the operator reviewed. Returns the
/// `previous` and the new `pinned` keys.
pub async fn confirm_realm_domain_identity<F, Fut>(
    domain: &str,
    digest: &str,
    fetch: F,
) -> Result<Value, ManagementError>
where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = Result<Arc<[u8]>, FetchError>>,
{
    let previous = RealmDomain::get(domain)
        .await?
        .ok_or_else(|| ManagementError::NotFound(format!("no keys are pinned for {domain}")))?;
    let member = member_from_descriptor(domain, &descriptor(domain, fetch).await?)
        .map_err(|error| ManagementError::Unavailable(error.to_string()))?;
    let confirmed = RealmDomain::confirm_identity(domain, &member, digest).await?;
    Ok(json!({
        "domain": domain,
        "previous": keys_view(&previous.pins),
        "pinned": keys_view(&confirmed.pins),
        "pinned_at": confirmed.pinned_at,
    }))
}

async fn registered_publisher(publisher: &str) -> Result<(Publisher, KeyPins), ManagementError> {
    let pool = platform::storage::db::get_database().get_pool().clone();
    let record = Publisher::get_by_name(&pool, publisher)
        .await?
        .ok_or_else(|| ManagementError::NotFound(format!("publisher {publisher} not found")))?;
    let pins = record
        .profile_pins
        .as_deref()
        .map(serde_json::from_str::<KeyPins>)
        .transpose()
        .map_err(|error| ManagementError::Internal(format!("stored key pins: {error}")))?
        .ok_or_else(|| {
            ManagementError::NotFound(format!("no keys are pinned for publisher {publisher}"))
        })?;
    Ok((record, pins))
}

async fn served_profile<F, Fut>(
    publisher: &str,
    fetch: F,
) -> Result<PublisherProfile, ManagementError>
where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = Result<Arc<[u8]>, FetchError>>,
{
    publisher_from_descriptor(publisher, &descriptor(publisher, fetch).await?)
        .map_err(ManagementError::Unavailable)
}

/// The pinned and the served Publisher Profile signing keys of a registered
/// `publisher`, as [`realm_domain_identity`].
pub async fn publisher_identity<F, Fut>(publisher: &str, fetch: F) -> Result<Value, ManagementError>
where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = Result<Arc<[u8]>, FetchError>>,
{
    let (_, pinned) = registered_publisher(publisher).await?;
    let profile = served_profile(publisher, fetch).await?;
    let now = Utc::now();
    let fetched = profile
        .accept(publisher, now, None)
        .map_err(|error| ManagementError::Unavailable(format!("{publisher}: {error}")))?;
    let view = json!({
        "publisher": publisher,
        "pinned": keys_view(&pinned),
        "fetched": keys_view(&own_pins(fetched.1)),
    });
    Ok(merge(
        view,
        outcome_view(&profile.accept(publisher, now, Some(&pinned))),
    ))
}

/// Take the Publisher Profile `publisher` serves now as its new baseline;
/// `digest` is the `fetched.digest` the operator reviewed. Returns the
/// `previous` and the new `pinned` keys.
pub async fn confirm_publisher_identity<F, Fut>(
    publisher: &str,
    digest: &str,
    fetch: F,
) -> Result<Value, ManagementError>
where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = Result<Arc<[u8]>, FetchError>>,
{
    let (_, previous) = registered_publisher(publisher).await?;
    let profile = served_profile(publisher, fetch).await?;
    let keys = profile
        .verify(publisher, Utc::now())
        .map_err(|error| ManagementError::Unavailable(format!("{publisher}: {error}")))?;
    let pool = platform::storage::db::get_database().get_pool().clone();
    PublisherRegistryManager::new(pool)
        .confirm_publisher_identity(publisher, &profile, &keys, digest)
        .await?;
    let (_, pins) = registered_publisher(publisher).await?;
    Ok(json!({
        "publisher": publisher,
        "previous": keys_view(&previous),
        "pinned": keys_view(&pins),
    }))
}
