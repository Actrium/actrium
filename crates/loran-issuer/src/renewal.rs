//! Issuer renewal token management — deterministic rotation, storage, and cleanup.
//!
//! # Design
//!
//! Renewal tokens are bound to a specific `ActrId` and stored only as
//! SHA-256(token) in SQLite. Successor tokens are derived deterministically
//! via HMAC-SHA256 so concurrent requests and retries produce the same token.
//!
//! `/renew` prepares a rotation without writing (see
//! [`prepare_renewal_token_rotation`]); the attempt journal then commits the
//! successor together with the response:
//! 1. Look up the unexpired token by `actor_id + token_hash`.
//! 2. If remaining lifetime > rotation window, return the current token unchanged.
//! 3. Otherwise derive the successor deterministically and `INSERT OR IGNORE`.
//! 4. Delete expired tokens for this actor; keep at most the 2 most recent tokens.
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
/// for `/issuer/reissue` (issuer-recorded actor binding).
///
/// Renewal (`/issuer/renew`) keeps requiring an unexpired token; this grace only
/// lets a long-offline node prove it once owned the exact serial so it can
/// recover its stable identity through the hard-reissue path. Records older
/// than expiry + grace are garbage-collected and the identity can no longer
/// be reissued (the node must register a fresh serial).
///
/// Accepting an up-to-30-day-expired possession proof extends the recovery
/// authority of a leaked old token; the attack surface, bounds, and mitigations
/// are analysed in `loran/docs/reissue-grace-threat-model.zh.md`.
pub const REISSUE_BINDING_GRACE_SECS: i64 = 30 * 86_400;

/// How long `/issuer/renew` still accepts a token whose successor already
/// exists (it was rotated, or consumed by `/issuer/reissue`). Within it the
/// caller gets that successor back, so a node that lost the response can
/// recover; afterwards the token is refused, so a leaked copy of it is useless.
pub const CONSUMED_TOKEN_RENEW_SECS: i64 = 5 * 60;

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
///   "actrix/issuer/renewal/v1" ||
///   old_token ||
///   ActrId::to_string_repr() ||
///   old_expires_at_be
/// )
/// ```
///
/// `old_expires_at` is encoded as big-endian u64 bytes for portability.
pub fn derive_successor_token(
    secret: &[u8],
    old_token: &[u8],
    actor_id: &ActrId,
    old_expires_at: i64,
) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC-SHA256 accepts any key length");
    mac.update(b"actrix/issuer/renewal/v1");
    mac.update(old_token);
    mac.update(actor_id.to_string_repr().as_bytes());
    mac.update(&old_expires_at.to_be_bytes());
    mac.finalize().into_bytes().to_vec()
}

/// Compute successor expiry: old_expires_at + ttl_secs.
///
/// Returned as Unix seconds (i64) for direct SQLite storage.
pub fn successor_expires_at(old_expires_at: i64, ttl_secs: u64) -> i64 {
    old_expires_at + ttl_secs as i64
}

/// Current Unix timestamp in seconds (truncated to i64 for SQLite).
pub fn now_secs() -> i64 {
    try_now_secs().unwrap_or_default() as i64
}

/// Current Unix timestamp in seconds, preserving a pre-epoch clock error for
/// callers that must reject invalid timestamps instead of falling back to zero.
pub fn try_now_secs() -> Result<u64, std::time::SystemTimeError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
}

// -------- database operations ------------------------------------------------

/// Persist a renewal token hash for the given actor.
///
/// Uses the shared `actrix.db` pool. Token hash uniqueness enforces that
/// duplicate inserts are silently ignored (`INSERT OR IGNORE`).
pub async fn insert_renewal_token(
    actor_id: &ActrId,
    token_hash: &[u8],
    expires_at: i64,
) -> Result<(), sqlx::Error> {
    let pool = platform::storage::db::get_database().get_pool();
    let now = now_secs();

    // Use a separate connection for this write — the caller handles
    // the enclosing transaction for rotation.
    sqlx::query(
        "INSERT OR IGNORE INTO issuer_renewal_token (actor_id, token_hash, expires_at, created_at)
         VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(actor_id.to_string_repr())
    .bind(token_hash)
    .bind(expires_at)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(())
}

/// Look up the issuer-recorded binding backing an `/issuer/reissue` possession
/// proof, returning its recorded `expires_at`.
///
/// Unlike `/renew`, this accepts tokens that expired up to
/// [`REISSUE_BINDING_GRACE_SECS`] ago: the reissue path exists precisely for
/// nodes whose renewal token can no longer be used for `/renew`.
///
/// The returned expiry is a stable property of the presented proof; with the
/// proof it seeds the deterministic successor whose row marks the proof as
/// consumed.
pub async fn find_binding_token_expiry(
    actor_id: &ActrId,
    token_hash: &[u8],
) -> Result<Option<i64>, sqlx::Error> {
    let pool = platform::storage::db::get_database().get_pool();
    let now = now_secs();

    let expires_at: Option<i64> = sqlx::query_scalar(
        "SELECT expires_at FROM issuer_renewal_token
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

/// Return whether an actor identity has been explicitly revoked.
///
/// A hit is terminal: the caller must refuse to renew or reissue credentials for
/// this `(realm, serial_number)` regardless of any renewal token it still
/// holds.
pub async fn is_actor_revoked(
    conn: &mut sqlx::SqliteConnection,
    actor_id: &ActrId,
) -> Result<bool, sqlx::Error> {
    let found: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM revoked_actor
         WHERE realm = ?1 AND serial_number = ?2
         LIMIT 1",
    )
    .bind(&actor_id.realm)
    .bind(actor_id.serial_number as i64)
    .fetch_optional(conn)
    .await?;

    Ok(found.is_some())
}

/// Record an actor identity revocation. Idempotent: a repeated call for the same
/// `(realm, serial_number)` keeps the original `revoked_at`/`reason`.
pub async fn revoke_actor(actor_id: &ActrId, reason: &str) -> Result<(), sqlx::Error> {
    let pool = platform::storage::db::get_database().get_pool();
    let now = now_secs();

    sqlx::query(
        "INSERT OR IGNORE INTO revoked_actor (realm, serial_number, revoked_at, reason)
         VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(&actor_id.realm)
    .bind(actor_id.serial_number as i64)
    .bind(now)
    .bind(reason)
    .execute(pool)
    .await?;

    Ok(())
}

/// Delete all expired tokens for a given actor.
pub async fn delete_expired_tokens_for_actor(actor_id: &ActrId) -> Result<u64, sqlx::Error> {
    let pool = platform::storage::db::get_database().get_pool();
    let now = now_secs();

    let result =
        sqlx::query("DELETE FROM issuer_renewal_token WHERE actor_id = ?1 AND expires_at <= ?2")
            .bind(actor_id.to_string_repr())
            .bind(now)
            .execute(pool)
            .await?;

    Ok(result.rows_affected())
}

/// Delete all but the 2 most recent tokens for a given actor.
pub async fn trim_oldest_tokens_for_actor(actor_id: &ActrId) -> Result<u64, sqlx::Error> {
    let pool = platform::storage::db::get_database().get_pool();

    // SQLite doesn't support LIMIT in subqueries for DELETE directly,
    // so we use a rowid-based approach.
    let result = sqlx::query(
        "DELETE FROM issuer_renewal_token
         WHERE actor_id = ?1
           AND id NOT IN (
               SELECT id FROM issuer_renewal_token
               WHERE actor_id = ?1
               ORDER BY expires_at DESC, created_at DESC
               LIMIT 2
           )",
    )
    .bind(actor_id.to_string_repr())
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

/// Global GC: delete tokens across all actors once they are expired beyond
/// [`REISSUE_BINDING_GRACE_SECS`].
///
/// Called periodically (low-frequency) and at the start of each
/// register/renew transaction. Rows inside the grace window are kept on
/// purpose: they are the issuer-recorded binding that `/issuer/reissue` uses to
/// verify actor possession after the token stopped being renewable.
pub async fn gc_expired_tokens() -> Result<u64, sqlx::Error> {
    let pool = platform::storage::db::get_database().get_pool();
    let now = now_secs();

    let result = sqlx::query("DELETE FROM issuer_renewal_token WHERE expires_at + ?1 <= ?2")
        .bind(REISSUE_BINDING_GRACE_SECS)
        .bind(now)
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}

/// Extend an actor's pending registration metadata to the retention of its
/// current renewal token, matching what `/issuer/register` stored.
pub(crate) async fn refresh_pending_registration(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    actor_id: &ActrId,
    renewal_expires_at: i64,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE pending_registration SET expires_at = ?1
         WHERE realm = ?2 AND serial_number = ?3",
    )
    .bind(renewal_expires_at.saturating_add(REISSUE_BINDING_GRACE_SECS))
    .bind(&actor_id.realm)
    .bind(actor_id.serial_number as i64)
    .execute(&mut **tx)
    .await?;
    Ok(result.rows_affected())
}

// -------- rotation logic -----------------------------------------------------

/// Outcome of a renewal token rotation attempt.
pub enum RotationOutcome {
    /// Token was still fresh — return the same token and expiry.
    Unchanged { token: Bytes, expires_at: i64 },
    /// Entered the rotation window — return the successor token and its expiry.
    Rotated { token: Bytes, expires_at: i64 },
}

/// A validated rotation, not yet written. The attempt journal commits it
/// together with the response (`ControlAttemptStore::complete_renewal`).
pub(crate) struct PreparedRotation {
    pub(crate) presented_hash: Vec<u8>,
    /// The token's successor, stored when `outcome` is `Rotated`; its row
    /// marks the presented token as consumed.
    pub(crate) successor_hash: Vec<u8>,
    pub(crate) outcome: RotationOutcome,
}

/// Validate the presented token and work out the rotation without writing.
///
/// 1. Verify the token exists and is unexpired (401 otherwise).
/// 2. If its successor already exists, return that successor, but only within
///    [`CONSUMED_TOKEN_RENEW_SECS`] of its creation (401 afterwards).
/// 3. If remaining lifetime > rotation_window_secs, the token stays.
/// 4. Otherwise derive the successor and its expiry.
pub(crate) async fn prepare_renewal_token_rotation(
    actor_id: &ActrId,
    old_token: &[u8],
    secret: &[u8],
    rotation_window_secs: u64,
    ttl_secs: u64,
) -> Result<PreparedRotation, RenewalError> {
    let presented_hash = hash_token(old_token);
    let now = now_secs();
    let pool = platform::storage::db::get_database().get_pool();

    let current_expires_at: Option<i64> = sqlx::query_scalar(
        "SELECT expires_at FROM issuer_renewal_token
         WHERE actor_id = ?1 AND token_hash = ?2 AND expires_at > ?3
         LIMIT 1",
    )
    .bind(actor_id.to_string_repr())
    .bind(&presented_hash)
    .bind(now)
    .fetch_optional(pool)
    .await
    .map_err(|e| RenewalError::StoreError(format!("token lookup failed: {e}")))?;

    let Some(current_expires_at) = current_expires_at else {
        return Err(RenewalError::TokenRejected);
    };

    let successor = derive_successor_token(secret, old_token, actor_id, current_expires_at);
    let successor_hash = hash_token(&successor);
    let consumed: Option<(i64, i64)> = sqlx::query_as(
        "SELECT expires_at, created_at FROM issuer_renewal_token
         WHERE actor_id = ?1 AND token_hash = ?2",
    )
    .bind(actor_id.to_string_repr())
    .bind(&successor_hash)
    .fetch_optional(pool)
    .await
    .map_err(|e| RenewalError::StoreError(format!("successor lookup failed: {e}")))?;
    if let Some((successor_expires, consumed_at)) = consumed {
        if now - consumed_at > CONSUMED_TOKEN_RENEW_SECS {
            return Err(RenewalError::TokenRejected);
        }
        return Ok(PreparedRotation {
            presented_hash,
            successor_hash,
            outcome: RotationOutcome::Rotated {
                token: Bytes::from(successor),
                expires_at: successor_expires,
            },
        });
    }

    if current_expires_at - now > rotation_window_secs as i64 {
        return Ok(PreparedRotation {
            presented_hash,
            successor_hash,
            outcome: RotationOutcome::Unchanged {
                token: Bytes::copy_from_slice(old_token),
                expires_at: current_expires_at,
            },
        });
    }

    Ok(PreparedRotation {
        presented_hash,
        successor_hash,
        outcome: RotationOutcome::Rotated {
            token: Bytes::from(successor),
            expires_at: successor_expires_at(current_expires_at, ttl_secs),
        },
    })
}

/// Errors specific to renewal token management.
#[derive(Debug, Clone)]
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
            realm: "test@example.com".to_string(),
            serial_number: 42,
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
            realm: "test@example.com".to_string(),
            serial_number: 42,
        };

        let s1 = derive_successor_token(secret, old, &actor_id, 1_000_000);
        let s2 = derive_successor_token(secret, old, &actor_id, 1_000_001);
        assert_ne!(s1, s2);
    }
}
