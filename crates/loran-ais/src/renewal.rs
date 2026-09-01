//! AIS renewal token management — deterministic rotation, storage, and cleanup.
//!
//! # Design
//!
//! Renewal tokens are bound to a specific `ActrId` and stored only as
//! SHA-256(token) in SQLite. Successor tokens are derived deterministically
//! via HMAC-SHA256 so concurrent requests and retries produce the same token.
//!
//! Rotation is first prepared without mutation so credential signing does not
//! hold a database lock. The completion transaction then revalidates the
//! presented token, inserts the deterministic successor, removes expired/old
//! rows, and records the idempotent response atomically.
//!
//! # Security
//!
//! - The database never stores raw tokens.
//! - Logging must never emit tokens, token hashes, or realm secrets.

use actr_protocol::ActrId;
use hmac::{Hmac, Mac};
use prost::bytes::Bytes;
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

/// How long an expired renewal-token record remains a valid possession proof
/// for `/ais/reissue` (issuer-recorded actor binding).
///
/// Renewal (`/ais/renew`) keeps requiring an unexpired token; this grace only
/// lets a long-offline node prove it once owned the exact serial so it can
/// recover its stable identity through the hard-reissue path. Records older
/// than expiry + grace are garbage-collected and the identity can no longer
/// be reissued (the node must register a fresh serial).
///
/// Accepting an up-to-30-day-expired possession proof extends the recovery
/// authority of a leaked old token; the attack surface, bounds, and mitigations
/// are analysed in `loran/docs/reissue-grace-threat-model.zh.md`.
pub const REISSUE_BINDING_GRACE_SECS: i64 = 30 * 86_400;

// -------- public API ---------------------------------------------------------

/// Compute SHA-256(token) for storage lookup.
pub fn hash_token(token: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(token);
    hasher.finalize().to_vec()
}

/// Deterministically derive a successor renewal token.
///
/// ```text
/// HMAC-SHA256(
///   renewal_token_secret,
///   "actrix/ais/renewal/v1" ||
///   old_token ||
///   ActrId::to_string_repr() ||
///   old_expires_at_be
/// )
/// ```
///
/// `old_expires_at` is encoded as big-endian u64 bytes for portability.
pub(crate) fn derive_successor_token(
    secret: &[u8],
    old_token: &[u8],
    actor_id: &ActrId,
    old_expires_at: i64,
) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC-SHA256 accepts any key length");
    mac.update(b"actrix/ais/renewal/v1");
    mac.update(old_token);
    mac.update(actor_id.to_string_repr().as_bytes());
    mac.update(&old_expires_at.to_be_bytes());
    mac.finalize().into_bytes().to_vec()
}

/// Compute successor expiry: old_expires_at + ttl_secs.
///
/// Returned as Unix seconds (i64) for direct SQLite storage.
pub(crate) fn successor_expires_at(old_expires_at: i64, ttl_secs: u64) -> i64 {
    old_expires_at + ttl_secs as i64
}

/// Current Unix timestamp in seconds (truncated to i64 for SQLite).
pub(crate) fn now_secs() -> i64 {
    try_now_secs().unwrap_or_default() as i64
}

/// Current Unix timestamp in seconds, preserving a pre-epoch clock error for
/// callers that must reject invalid timestamps instead of falling back to zero.
pub(crate) fn try_now_secs() -> Result<u64, std::time::SystemTimeError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
}

// -------- database operations ------------------------------------------------

/// Look up the issuer-recorded binding backing an `/ais/reissue` possession
/// proof, returning its recorded `expires_at`.
///
/// This accepts tokens that expired up to [`REISSUE_BINDING_GRACE_SECS`] ago:
/// the reissue path exists precisely for nodes whose renewal token can no
/// longer be used for `/renew`.
///
/// The returned expiry is what makes reissue replay-safe: it is a stable
/// property of the presented proof, so it can seed a deterministic successor
/// derivation that yields the same token for every retry of the same attempt.
pub(crate) async fn find_binding_token_expiry(
    actor_id: &ActrId,
    token_hash: &[u8],
) -> Result<Option<i64>, sqlx::Error> {
    let pool = platform::storage::db::get_database().get_pool();
    let now = now_secs();

    let expires_at: Option<i64> = sqlx::query_scalar(
        "SELECT expires_at FROM ais_renewal_token
         WHERE actor_id = ?1 AND token_hash = ?2 AND expires_at + ?3 > ?4
         LIMIT 1",
    )
    .bind(actor_id.to_string_repr())
    .bind(token_hash)
    .bind(REISSUE_BINDING_GRACE_SECS)
    .bind(now)
    .fetch_optional(pool)
    .await?;

    Ok(expires_at)
}

/// Global GC: delete tokens across all actors once they are expired beyond
/// [`REISSUE_BINDING_GRACE_SECS`].
///
/// Called periodically (low-frequency) and at the start of each
/// register/renew transaction. Rows inside the grace window are kept on
/// purpose: they are the issuer-recorded binding that `/ais/reissue` uses to
/// verify actor possession after the token stopped being renewable.
pub(crate) async fn gc_expired_tokens() -> Result<u64, sqlx::Error> {
    let pool = platform::storage::db::get_database().get_pool();
    let now = now_secs();

    let result = sqlx::query("DELETE FROM ais_renewal_token WHERE expires_at + ?1 <= ?2")
        .bind(REISSUE_BINDING_GRACE_SECS)
        .bind(now)
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}

// -------- rotation logic -----------------------------------------------------

/// Outcome of a renewal token rotation attempt.
#[derive(Clone)]
pub(crate) enum RotationOutcome {
    /// Token was still fresh — return the same token and expiry.
    Unchanged { token: Bytes, expires_at: i64 },
    /// Entered the rotation window — return the successor token and its expiry.
    Rotated { token: Bytes, expires_at: i64 },
}

/// Read-only renewal decision that can later be committed together with an
/// idempotency response record in one SQLite transaction.
pub(crate) struct PreparedRotation {
    pub(crate) presented_hash: Vec<u8>,
    pub(crate) successor_hash: Option<Vec<u8>>,
    pub(crate) outcome: RotationOutcome,
}

/// Validate a renewal token and prepare its deterministic successor without
/// mutating the database.
///
/// The handler signs the new credential after this step, then atomically
/// commits this plan and the encrypted response through the control-attempt
/// journal. Keeping signing outside the SQLite transaction avoids holding a
/// write lock across the Signer RPC.
pub(crate) async fn prepare_renewal_token_rotation(
    actor_id: &ActrId,
    old_token: &[u8],
    secret: &[u8],
    rotation_window_secs: u64,
    ttl_secs: u64,
) -> Result<PreparedRotation, RenewalError> {
    let presented_hash = hash_token(old_token);
    let now = now_secs();
    let actor_id_repr = actor_id.to_string_repr();
    let pool = platform::storage::db::get_database().get_pool();

    let current_expires_at: Option<i64> = sqlx::query_scalar(
        "SELECT expires_at
         FROM ais_renewal_token
         WHERE actor_id = ?1 AND token_hash = ?2 AND expires_at > ?3
         LIMIT 1",
    )
    .bind(&actor_id_repr)
    .bind(&presented_hash)
    .bind(now)
    .fetch_optional(pool)
    .await
    .map_err(|e| RenewalError::StoreError(format!("token lookup failed: {e}")))?;

    let Some(current_expires_at) = current_expires_at else {
        return Err(RenewalError::TokenRejected);
    };

    if current_expires_at - now > rotation_window_secs as i64 {
        return Ok(PreparedRotation {
            presented_hash,
            successor_hash: None,
            outcome: RotationOutcome::Unchanged {
                token: Bytes::copy_from_slice(old_token),
                expires_at: current_expires_at,
            },
        });
    }

    let successor = derive_successor_token(secret, old_token, actor_id, current_expires_at);
    let successor_hash = hash_token(&successor);
    let successor_expires_at = successor_expires_at(current_expires_at, ttl_secs);

    Ok(PreparedRotation {
        presented_hash,
        successor_hash: Some(successor_hash),
        outcome: RotationOutcome::Rotated {
            token: Bytes::from(successor),
            expires_at: successor_expires_at,
        },
    })
}

/// Errors specific to renewal token management.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum RenewalError {
    /// Token not found or expired — maps to HTTP 401.
    TokenRejected,
    /// Database error — maps to HTTP 500.
    StoreError(String),
}

impl std::fmt::Display for RenewalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TokenRejected => write!(f, "renewal token rejected"),
            Self::StoreError(msg) => write!(f, "renewal store error: {msg}"),
        }
    }
}

impl std::error::Error for RenewalError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_token_deterministic() {
        let token = b"test-token-32-bytes-xxxxxxxxxxx";
        let h1 = hash_token(token);
        let h2 = hash_token(token);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 32); // SHA-256
    }

    #[test]
    fn test_hash_token_different_inputs() {
        let h1 = hash_token(b"token-a");
        let h2 = hash_token(b"token-b");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_successor_expiry_calculation() {
        let old = 1_000_000;
        let ttl = 86_400;
        assert_eq!(successor_expires_at(old, ttl), 1_086_400);
    }

    #[test]
    fn test_derive_successor_deterministic() {
        let secret = b"test-secret-at-least-32-bytes!";
        let old = b"old-renewal-token-32-bytes-xx";
        let actor_id = ActrId {
            realm: actr_protocol::Realm { realm_id: 1 },
            serial_number: 42,
            r#type: actr_protocol::ActrType {
                manufacturer: "test".to_string(),
                name: "actor".to_string(),
                version: "1.0.0".to_string(),
            },
        };

        let s1 = derive_successor_token(secret, old, &actor_id, 1_000_000);
        let s2 = derive_successor_token(secret, old, &actor_id, 1_000_000);
        assert_eq!(s1, s2);
        assert_eq!(s1.len(), 32);
    }

    #[test]
    fn test_derive_successor_different_old_expiry() {
        let secret = b"test-secret-at-least-32-bytes!";
        let old = b"old-renewal-token-32-bytes-xx";
        let actor_id = ActrId {
            realm: actr_protocol::Realm { realm_id: 1 },
            serial_number: 42,
            r#type: actr_protocol::ActrType {
                manufacturer: "test".to_string(),
                name: "actor".to_string(),
                version: "1.0.0".to_string(),
            },
        };

        let s1 = derive_successor_token(secret, old, &actor_id, 1_000_000);
        let s2 = derive_successor_token(secret, old, &actor_id, 1_000_001);
        assert_ne!(s1, s2);
    }
}
