//! Durable idempotency journal for Issuer control-plane writes.
//!
//! `/renew` and `/reissue` carry a client-generated 16-byte `attempt_id`. The
//! first request with an id claims it; the identity-state change and the
//! response then commit in one transaction. A retry of the same attempt gets
//! the exact stored response, a different body under the same id `409`, a
//! retry after the response retention `410`, and a retry while another worker
//! holds the attempt `425` with `Retry-After`.
//!
//! Successful renew/reissue responses contain bearer renewal tokens and TURN
//! passwords. The journal therefore encrypts response bytes with an AES-256-GCM
//! key derived from the configured renewal-token secret before writing SQLite.

use actr_protocol::{ActrId, RegisterResponse, register_response};
use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use prost::Message;
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::issuer::PreparedReissue;
use crate::renewal::{PreparedRotation, REISSUE_BINDING_GRACE_SECS, RotationOutcome};

const ATTEMPT_ID_LEN: usize = 16;
const RESPONSE_CIPHERTEXT_VERSION: u8 = 1;
const SHORT_ATTEMPT_TTL_SECS: i64 = 5 * 60;
const ATTEMPT_LEASE_SECS: i64 = 90;
const STATE_IN_PROGRESS: i64 = 0;
const STATE_COMPLETED: i64 = 1;
const STATE_EXPIRED: i64 = 2;
/// How long a completed credential attempt stays a tombstone (`410`) after its
/// response expired: as long as its proof can still be presented to `/reissue`.
const RESPONSE_TOMBSTONE_TTL_SECS: i64 = REISSUE_BINDING_GRACE_SECS;

#[derive(Clone, Copy)]
pub(crate) enum Operation {
    Renew,
    Reissue,
}

impl Operation {
    fn as_str(self) -> &'static str {
        match self {
            Self::Renew => "renew",
            Self::Reissue => "reissue",
        }
    }
}

pub(crate) struct CachedResponse {
    pub(crate) status: u16,
    pub(crate) body: Vec<u8>,
}

pub(crate) enum ClaimOutcome {
    Acquired(AttemptLease),
    Replay(CachedResponse),
}

pub(crate) struct AttemptLease {
    operation: Operation,
    attempt_id: Vec<u8>,
    actor_id: String,
    request_hash: Vec<u8>,
    lease_id: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum AttemptError {
    #[error("attempt_id must contain exactly 16 UUID bytes")]
    InvalidAttemptId,
    #[error("attempt_id was already used with a different request")]
    Conflict,
    #[error("idempotency result retention has expired")]
    Expired,
    #[error("the same control-plane attempt is still in progress")]
    InProgress { retry_after_secs: u64 },
    #[error("control-plane attempt lease was lost")]
    LeaseLost,
    #[error("renewal token invalid or expired")]
    TokenRejected,
    #[error("actor identity possession proof is no longer valid")]
    PossessionNotProven,
    #[error("publisher authorization nonce was already consumed")]
    PublisherNonceRejected,
    #[error("idempotency store error: {0}")]
    Store(String),
    #[error("idempotency response encryption error: {0}")]
    Crypto(String),
}

#[derive(Clone)]
pub(crate) struct ControlAttemptStore {
    encryption_key: [u8; 32],
}

impl ControlAttemptStore {
    pub(crate) fn new(renewal_token_secret: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"actrix/issuer/control-attempt-response/v1");
        hasher.update(renewal_token_secret);
        Self {
            encryption_key: hasher.finalize().into(),
        }
    }

    pub(crate) async fn claim(
        &self,
        operation: Operation,
        attempt_id: &[u8],
        actor_id: &ActrId,
        request_body: &[u8],
    ) -> Result<ClaimOutcome, AttemptError> {
        if attempt_id.len() != ATTEMPT_ID_LEN {
            return Err(AttemptError::InvalidAttemptId);
        }

        let now = crate::renewal::now_secs();
        let actor_id = actor_id.to_string_repr();
        let request_hash = Sha256::digest(request_body).to_vec();
        let lease_id = uuid::Uuid::new_v4().as_bytes().to_vec();
        let pool = platform::storage::db::get_database().get_pool();
        let mut tx = pool
            .begin()
            .await
            .map_err(|error| AttemptError::Store(format!("begin failed: {error}")))?;

        // Completed responses become non-sensitive tombstones at their explicit
        // response horizon. Tombstones make an aged-out retry distinguishable
        // from a never-seen attempt.
        sqlx::query(
            "UPDATE issuer_control_attempt
             SET state = ?1, response_ciphertext = NULL, lease_id = NULL,
                 lease_expires_at = NULL, updated_at = ?2
             WHERE state = ?3 AND response_expires_at <= ?2",
        )
        .bind(STATE_EXPIRED)
        .bind(now)
        .bind(STATE_COMPLETED)
        .execute(&mut *tx)
        .await
        .map_err(|error| AttemptError::Store(format!("expiry transition failed: {error}")))?;

        sqlx::query(
            "DELETE FROM issuer_control_attempt
             WHERE tombstone_expires_at <= ?1
               AND (state = ?2 OR (state = ?3 AND lease_expires_at <= ?1))",
        )
        .bind(now)
        .bind(STATE_EXPIRED)
        .bind(STATE_IN_PROGRESS)
        .execute(&mut *tx)
        .await
        .map_err(|error| AttemptError::Store(format!("attempt cleanup failed: {error}")))?;

        sqlx::query(
            "INSERT OR IGNORE INTO issuer_control_attempt (
                operation, attempt_id, actor_id, request_hash, state,
                lease_id, lease_expires_at, tombstone_expires_at,
                created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
        )
        .bind(operation.as_str())
        .bind(attempt_id)
        .bind(&actor_id)
        .bind(&request_hash)
        .bind(STATE_IN_PROGRESS)
        .bind(&lease_id)
        .bind(now.saturating_add(ATTEMPT_LEASE_SECS))
        .bind(now.saturating_add(SHORT_ATTEMPT_TTL_SECS))
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|error| AttemptError::Store(format!("attempt insert failed: {error}")))?;

        let row = sqlx::query_as::<
            _,
            (
                String,
                Vec<u8>,
                i64,
                Option<Vec<u8>>,
                Option<i64>,
                Option<i64>,
                Option<Vec<u8>>,
            ),
        >(
            "SELECT actor_id, request_hash, state, lease_id, lease_expires_at,
                    response_status, response_ciphertext
             FROM issuer_control_attempt
             WHERE operation = ?1 AND attempt_id = ?2",
        )
        .bind(operation.as_str())
        .bind(attempt_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| AttemptError::Store(format!("attempt lookup failed: {error}")))?;

        let (
            stored_actor_id,
            stored_request_hash,
            state,
            stored_lease_id,
            lease_expires_at,
            response_status,
            response_ciphertext,
        ) = row;

        if stored_actor_id != actor_id || stored_request_hash != request_hash {
            tx.commit()
                .await
                .map_err(|error| AttemptError::Store(format!("commit failed: {error}")))?;
            return Err(AttemptError::Conflict);
        }

        if state == STATE_COMPLETED {
            let status = response_status.ok_or_else(|| {
                AttemptError::Store("completed attempt missing response status".to_string())
            })?;
            let ciphertext = response_ciphertext.ok_or_else(|| {
                AttemptError::Store("completed attempt missing response body".to_string())
            })?;
            tx.commit()
                .await
                .map_err(|error| AttemptError::Store(format!("commit failed: {error}")))?;
            return Ok(ClaimOutcome::Replay(CachedResponse {
                status: u16::try_from(status).map_err(|_| {
                    AttemptError::Store("cached response status is out of range".to_string())
                })?,
                body: self.decrypt_response(&request_hash, &ciphertext)?,
            }));
        }

        if state == STATE_EXPIRED {
            tx.commit()
                .await
                .map_err(|error| AttemptError::Store(format!("commit failed: {error}")))?;
            return Err(AttemptError::Expired);
        }

        if stored_lease_id.as_deref() == Some(lease_id.as_slice()) {
            tx.commit()
                .await
                .map_err(|error| AttemptError::Store(format!("commit failed: {error}")))?;
            return Ok(ClaimOutcome::Acquired(AttemptLease {
                operation,
                attempt_id: attempt_id.to_vec(),
                actor_id,
                request_hash,
                lease_id,
            }));
        }

        if lease_expires_at.is_some_and(|expires_at| expires_at > now) {
            let retry_after_secs = lease_expires_at
                .unwrap_or(now.saturating_add(1))
                .saturating_sub(now)
                .max(1) as u64;
            tx.commit()
                .await
                .map_err(|error| AttemptError::Store(format!("commit failed: {error}")))?;
            return Err(AttemptError::InProgress { retry_after_secs });
        }

        let updated = sqlx::query(
            "UPDATE issuer_control_attempt
             SET lease_id = ?1, lease_expires_at = ?2, updated_at = ?3
             WHERE operation = ?4 AND attempt_id = ?5 AND state = ?6
               AND lease_expires_at <= ?3",
        )
        .bind(&lease_id)
        .bind(now.saturating_add(ATTEMPT_LEASE_SECS))
        .bind(now)
        .bind(operation.as_str())
        .bind(attempt_id)
        .bind(STATE_IN_PROGRESS)
        .execute(&mut *tx)
        .await
        .map_err(|error| AttemptError::Store(format!("lease takeover failed: {error}")))?;

        if updated.rows_affected() != 1 {
            tx.commit()
                .await
                .map_err(|error| AttemptError::Store(format!("commit failed: {error}")))?;
            return Err(AttemptError::InProgress {
                retry_after_secs: 1,
            });
        }

        tx.commit()
            .await
            .map_err(|error| AttemptError::Store(format!("commit failed: {error}")))?;
        Ok(ClaimOutcome::Acquired(AttemptLease {
            operation,
            attempt_id: attempt_id.to_vec(),
            actor_id,
            request_hash,
            lease_id,
        }))
    }

    pub(crate) async fn release(&self, lease: &AttemptLease) -> Result<(), AttemptError> {
        let now = crate::renewal::now_secs();
        let pool = platform::storage::db::get_database().get_pool();
        sqlx::query(
            "UPDATE issuer_control_attempt
             SET lease_expires_at = ?1, tombstone_expires_at = ?2, updated_at = ?3
             WHERE operation = ?4 AND attempt_id = ?5 AND state = ?6 AND lease_id = ?7",
        )
        .bind(now.saturating_sub(1))
        .bind(now.saturating_add(SHORT_ATTEMPT_TTL_SECS))
        .bind(now)
        .bind(lease.operation.as_str())
        .bind(&lease.attempt_id)
        .bind(STATE_IN_PROGRESS)
        .bind(&lease.lease_id)
        .execute(pool)
        .await
        .map_err(|error| AttemptError::Store(format!("lease release failed: {error}")))?;
        Ok(())
    }

    pub(crate) async fn complete_response(
        &self,
        lease: &AttemptLease,
        status: u16,
        body: &[u8],
        response_expires_at: i64,
    ) -> Result<(), AttemptError> {
        let encrypted = self.encrypt_response(&lease.request_hash, body)?;
        let now = crate::renewal::now_secs();
        let pool = platform::storage::db::get_database().get_pool();
        let mut tx = pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|error| AttemptError::Store(format!("begin immediate failed: {error}")))?;
        self.ensure_lease(&mut tx, lease).await?;
        self.mark_completed(
            &mut tx,
            lease,
            status,
            &encrypted,
            response_expires_at,
            SHORT_ATTEMPT_TTL_SECS,
            now,
        )
        .await?;
        tx.commit()
            .await
            .map_err(|error| AttemptError::Store(format!("commit failed: {error}")))?;
        Ok(())
    }

    /// Commit a prepared renewal and its response in one transaction.
    pub(crate) async fn complete_renewal(
        &self,
        lease: &AttemptLease,
        actor_id: &ActrId,
        prepared: &PreparedRotation,
        body: &[u8],
        response_expires_at: i64,
    ) -> Result<(), AttemptError> {
        let encrypted = self.encrypt_response(&lease.request_hash, body)?;
        let now = crate::renewal::now_secs();
        let actor_id_repr = actor_id.to_string_repr();
        let pool = platform::storage::db::get_database().get_pool();
        let mut tx = pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|error| AttemptError::Store(format!("begin immediate failed: {error}")))?;
        self.ensure_lease(&mut tx, lease).await?;

        let presented_exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1 FROM issuer_renewal_token
                WHERE actor_id = ?1 AND token_hash = ?2 AND expires_at > ?3
             )",
        )
        .bind(&actor_id_repr)
        .bind(&prepared.presented_hash)
        .bind(now)
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| AttemptError::Store(format!("token recheck failed: {error}")))?;
        if presented_exists == 0 {
            return Err(AttemptError::TokenRejected);
        }

        let renewal_expires_at = match &prepared.outcome {
            RotationOutcome::Unchanged { expires_at, .. }
            | RotationOutcome::Rotated { expires_at, .. } => *expires_at,
        };
        if let Some(successor_hash) = prepared.successor_hash.as_ref() {
            sqlx::query(
                "INSERT OR IGNORE INTO issuer_renewal_token
                    (actor_id, token_hash, expires_at, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
            )
            .bind(&actor_id_repr)
            .bind(successor_hash)
            .bind(renewal_expires_at)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|error| AttemptError::Store(format!("successor insert failed: {error}")))?;

            sqlx::query(
                "DELETE FROM issuer_renewal_token
                 WHERE actor_id = ?1 AND expires_at <= ?2",
            )
            .bind(&actor_id_repr)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                AttemptError::Store(format!("expired token cleanup failed: {error}"))
            })?;

            sqlx::query(
                "DELETE FROM issuer_renewal_token
                 WHERE actor_id = ?1
                   AND id NOT IN (
                       SELECT id FROM issuer_renewal_token
                       WHERE actor_id = ?1
                       ORDER BY expires_at DESC, created_at DESC
                       LIMIT 2
                   )",
            )
            .bind(&actor_id_repr)
            .execute(&mut *tx)
            .await
            .map_err(|error| AttemptError::Store(format!("token trim failed: {error}")))?;
        }

        crate::renewal::refresh_pending_registration(&mut tx, actor_id, renewal_expires_at)
            .await
            .map_err(|error| {
                AttemptError::Store(format!("pending registration refresh failed: {error}"))
            })?;
        self.mark_completed(
            &mut tx,
            lease,
            200,
            &encrypted,
            response_expires_at,
            RESPONSE_TOMBSTONE_TTL_SECS,
            now,
        )
        .await?;
        tx.commit()
            .await
            .map_err(|error| AttemptError::Store(format!("commit failed: {error}")))?;
        Ok(())
    }

    /// Commit a prepared reissue and its response in one transaction: consume
    /// the possession proof (its successor row), the publisher-auth nonce, and
    /// record the package target the reissue was authorized under.
    pub(crate) async fn complete_reissue(
        &self,
        lease: &AttemptLease,
        actor_id: &ActrId,
        mut prepared: PreparedReissue,
    ) -> Result<CachedResponse, AttemptError> {
        let now = crate::renewal::now_secs();
        let actor_id_repr = actor_id.to_string_repr();
        let pool = platform::storage::db::get_database().get_pool();
        let mut tx = pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|error| AttemptError::Store(format!("begin immediate failed: {error}")))?;
        self.ensure_lease(&mut tx, lease).await?;

        let binding_expires_at: Option<i64> = sqlx::query_scalar(
            "SELECT expires_at FROM issuer_renewal_token
             WHERE actor_id = ?1 AND token_hash = ?2 AND expires_at + ?3 > ?4
             LIMIT 1",
        )
        .bind(&actor_id_repr)
        .bind(&prepared.proof_hash)
        .bind(REISSUE_BINDING_GRACE_SECS)
        .bind(now)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| AttemptError::Store(format!("possession recheck failed: {error}")))?;
        if binding_expires_at != Some(prepared.binding_expires_at) {
            return Err(AttemptError::PossessionNotProven);
        }

        if let Some(binding) = prepared.publisher_nonce.as_ref() {
            crate::issuer::AIdIssuer::consume_publisher_auth_nonce(
                &mut tx,
                &binding.publisher,
                &binding.signing_key_id,
                &binding.nonce,
                now,
            )
            .await
            .map_err(|error| match error {
                platform::aid::AidError::PublisherNotVerified => {
                    AttemptError::PublisherNonceRejected
                }
                other => AttemptError::Store(other.to_string()),
            })?;
            if let Some(target) = prepared.target.as_deref().filter(|t| !t.is_empty()) {
                sqlx::query(
                    "UPDATE issued_actor_identity SET target = ?1, auth_mode = ?2
                     WHERE realm = ?3 AND serial_number = ?4",
                )
                .bind(target)
                .bind(actr_protocol::RegisterAuthMode::Package as i32)
                .bind(&actor_id.realm)
                .bind(actor_id.serial_number as i64)
                .execute(&mut *tx)
                .await
                .map_err(|error| AttemptError::Store(format!("target update failed: {error}")))?;
            }
        }

        // The successor row marks the proof as consumed. Another attempt that
        // presented the same proof and committed first owns it.
        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO issuer_renewal_token
                (actor_id, token_hash, expires_at, created_at)
             VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(&actor_id_repr)
        .bind(&prepared.successor_hash)
        .bind(prepared.successor_expires_at)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|error| AttemptError::Store(format!("successor insert failed: {error}")))?;
        if inserted.rows_affected() != 1 {
            return Err(AttemptError::PossessionNotProven);
        }

        prepared.register_ok.renewal_token = Some(prepared.successor_token.into());
        prepared.register_ok.renewal_token_expires_at = Some(prost_types::Timestamp {
            seconds: prepared.successor_expires_at,
            nanos: 0,
        });
        let response_expires_at = register_ok_response_expiry(&prepared.register_ok);
        let body = RegisterResponse {
            result: Some(register_response::Result::Success(prepared.register_ok)),
        }
        .encode_to_vec();
        let encrypted = self.encrypt_response(&lease.request_hash, &body)?;

        crate::renewal::refresh_pending_registration(
            &mut tx,
            actor_id,
            prepared.successor_expires_at,
        )
        .await
        .map_err(|error| {
            AttemptError::Store(format!("pending registration refresh failed: {error}"))
        })?;
        self.mark_completed(
            &mut tx,
            lease,
            200,
            &encrypted,
            response_expires_at,
            RESPONSE_TOMBSTONE_TTL_SECS,
            now,
        )
        .await?;
        tx.commit()
            .await
            .map_err(|error| AttemptError::Store(format!("commit failed: {error}")))?;

        Ok(CachedResponse { status: 200, body })
    }

    async fn ensure_lease(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        lease: &AttemptLease,
    ) -> Result<(), AttemptError> {
        let owns_lease: i64 = sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1 FROM issuer_control_attempt
                WHERE operation = ?1 AND attempt_id = ?2 AND actor_id = ?3
                  AND request_hash = ?4 AND state = ?5 AND lease_id = ?6
             )",
        )
        .bind(lease.operation.as_str())
        .bind(&lease.attempt_id)
        .bind(&lease.actor_id)
        .bind(&lease.request_hash)
        .bind(STATE_IN_PROGRESS)
        .bind(&lease.lease_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(|error| AttemptError::Store(format!("lease check failed: {error}")))?;
        if owns_lease == 0 {
            return Err(AttemptError::LeaseLost);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn mark_completed(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        lease: &AttemptLease,
        status: u16,
        ciphertext: &[u8],
        response_expires_at: i64,
        tombstone_ttl_secs: i64,
        now: i64,
    ) -> Result<(), AttemptError> {
        let updated = sqlx::query(
            "UPDATE issuer_control_attempt
             SET state = ?1, response_status = ?2, response_ciphertext = ?3,
                 response_expires_at = ?4, tombstone_expires_at = ?5,
                 lease_id = NULL, lease_expires_at = NULL, updated_at = ?6
             WHERE operation = ?7 AND attempt_id = ?8 AND actor_id = ?9
               AND request_hash = ?10 AND state = ?11 AND lease_id = ?12",
        )
        .bind(STATE_COMPLETED)
        .bind(i64::from(status))
        .bind(ciphertext)
        .bind(response_expires_at)
        .bind(response_expires_at.saturating_add(tombstone_ttl_secs))
        .bind(now)
        .bind(lease.operation.as_str())
        .bind(&lease.attempt_id)
        .bind(&lease.actor_id)
        .bind(&lease.request_hash)
        .bind(STATE_IN_PROGRESS)
        .bind(&lease.lease_id)
        .execute(&mut **tx)
        .await
        .map_err(|error| AttemptError::Store(format!("attempt completion failed: {error}")))?;
        if updated.rows_affected() != 1 {
            return Err(AttemptError::LeaseLost);
        }
        Ok(())
    }

    fn encrypt_response(
        &self,
        request_hash: &[u8],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, AttemptError> {
        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|error| AttemptError::Crypto(error.to_string()))?;
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce_bytes),
                Payload {
                    msg: plaintext,
                    aad: request_hash,
                },
            )
            .map_err(|error| AttemptError::Crypto(error.to_string()))?;
        let mut stored = Vec::with_capacity(1 + nonce_bytes.len() + ciphertext.len());
        stored.push(RESPONSE_CIPHERTEXT_VERSION);
        stored.extend_from_slice(&nonce_bytes);
        stored.extend_from_slice(&ciphertext);
        Ok(stored)
    }

    fn decrypt_response(
        &self,
        request_hash: &[u8],
        stored: &[u8],
    ) -> Result<Vec<u8>, AttemptError> {
        if stored.len() <= 13 {
            return Err(AttemptError::Crypto(
                "cached response ciphertext is truncated".to_string(),
            ));
        }
        if stored[0] != RESPONSE_CIPHERTEXT_VERSION {
            return Err(AttemptError::Crypto(format!(
                "unsupported cached response ciphertext version {}",
                stored[0]
            )));
        }
        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|error| AttemptError::Crypto(error.to_string()))?;
        cipher
            .decrypt(
                Nonce::from_slice(&stored[1..13]),
                Payload {
                    msg: &stored[13..],
                    aad: request_hash,
                },
            )
            .map_err(|error| AttemptError::Crypto(error.to_string()))
    }
}

/// A successful response is replayable until its credential or renewal token
/// expires, whichever comes first.
pub(crate) fn register_ok_response_expiry(ok: &register_response::RegisterOk) -> i64 {
    let credential = ok
        .credential_expires_at
        .map(|timestamp| timestamp.seconds)
        .unwrap_or_default();
    let renewal = ok
        .renewal_token_expires_at
        .map(|timestamp| timestamp.seconds)
        .unwrap_or(credential);
    credential.min(renewal)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The crate's test database, shared by every unit test in the process
    pub(crate) async fn setup_db() {
        static DB_DIR: tokio::sync::OnceCell<tempfile::TempDir> =
            tokio::sync::OnceCell::const_new();
        DB_DIR
            .get_or_init(|| async {
                let dir = tempfile::tempdir().expect("temp dir");
                platform::storage::db::set_db_path(dir.path())
                    .await
                    .expect("init database");
                dir
            })
            .await;
    }

    fn actor_id() -> ActrId {
        ActrId {
            realm: "test@example.com".to_string(),
            serial_number: 42,
        }
    }

    fn acquired(outcome: ClaimOutcome) -> AttemptLease {
        match outcome {
            ClaimOutcome::Acquired(lease) => lease,
            ClaimOutcome::Replay(_) => panic!("expected a fresh lease"),
        }
    }

    #[tokio::test]
    async fn completed_attempt_replays_and_rejects_changed_body() {
        setup_db().await;
        let store = ControlAttemptStore::new(&[7u8; 32]);
        let attempt_id = uuid::Uuid::new_v4().as_bytes().to_vec();
        let actor = actor_id();

        let lease = acquired(
            store
                .claim(Operation::Renew, &attempt_id, &actor, b"request")
                .await
                .unwrap(),
        );
        assert!(matches!(
            store
                .claim(Operation::Renew, &attempt_id, &actor, b"request")
                .await,
            Err(AttemptError::InProgress { .. })
        ));

        let expires_at = crate::renewal::now_secs() + 60;
        store
            .complete_response(&lease, 200, b"response", expires_at)
            .await
            .unwrap();

        match store
            .claim(Operation::Renew, &attempt_id, &actor, b"request")
            .await
            .unwrap()
        {
            ClaimOutcome::Replay(cached) => {
                assert_eq!(cached.status, 200);
                assert_eq!(cached.body, b"response");
            }
            ClaimOutcome::Acquired(_) => panic!("completed attempt must replay"),
        }
        assert!(matches!(
            store
                .claim(Operation::Renew, &attempt_id, &actor, b"other")
                .await,
            Err(AttemptError::Conflict)
        ));
        assert!(matches!(
            store
                .claim(Operation::Renew, &attempt_id[..8], &actor, b"request")
                .await,
            Err(AttemptError::InvalidAttemptId)
        ));
    }

    #[tokio::test]
    async fn released_attempt_can_be_retried_and_expired_response_is_gone() {
        setup_db().await;
        let store = ControlAttemptStore::new(&[7u8; 32]);
        let attempt_id = uuid::Uuid::new_v4().as_bytes().to_vec();
        let actor = actor_id();

        let lease = acquired(
            store
                .claim(Operation::Reissue, &attempt_id, &actor, b"request")
                .await
                .unwrap(),
        );
        store.release(&lease).await.unwrap();
        let lease = acquired(
            store
                .claim(Operation::Reissue, &attempt_id, &actor, b"request")
                .await
                .unwrap(),
        );

        // A response horizon already in the past turns into a tombstone on the
        // next claim.
        let expires_at = crate::renewal::now_secs() - 1;
        store
            .complete_response(&lease, 200, b"response", expires_at)
            .await
            .unwrap();
        assert!(matches!(
            store
                .claim(Operation::Reissue, &attempt_id, &actor, b"request")
                .await,
            Err(AttemptError::Expired)
        ));
    }
}
