//! Durable idempotency journal for Issuer control-plane writes.
//!
//! Successful renew/reissue responses contain bearer renewal tokens and TURN
//! passwords. The journal therefore encrypts response bytes with an AES-256-GCM
//! key derived from the configured renewal-token secret before writing SQLite.

use actr_protocol::ActrId;
use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use rand::RngCore;
use sha2::{Digest, Sha256};

const ATTEMPT_ID_LEN: usize = 16;
const RESPONSE_CIPHERTEXT_VERSION: u8 = 1;
const SHORT_ATTEMPT_TTL_SECS: i64 = 5 * 60;
const ATTEMPT_LEASE_SECS: i64 = 90;
const STATE_IN_PROGRESS: i64 = 0;
const STATE_COMPLETED: i64 = 1;
const STATE_EXPIRED: i64 = 2;

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

#[cfg(test)]
mod tests {
    use super::*;
    use actr_protocol::{ActrType, Realm};

    async fn setup_db() {
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
            realm: Realm { realm_id: 1 },
            serial_number: 42,
            r#type: ActrType {
                publisher: "example.com".to_string(),
                name: "worker".to_string(),
                version: "1.0.0".to_string(),
            },
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
