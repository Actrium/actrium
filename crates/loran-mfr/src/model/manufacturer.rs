use crate::MfrError;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum MfrStatus {
    #[default]
    Pending,
    Active,
    Suspended,
    Revoked,
}

impl std::fmt::Display for MfrStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MfrStatus::Pending => write!(f, "pending"),
            MfrStatus::Active => write!(f, "active"),
            MfrStatus::Suspended => write!(f, "suspended"),
            MfrStatus::Revoked => write!(f, "revoked"),
        }
    }
}

impl std::str::FromStr for MfrStatus {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "pending" => Ok(MfrStatus::Pending),
            "active" => Ok(MfrStatus::Active),
            "suspended" => Ok(MfrStatus::Suspended),
            "revoked" => Ok(MfrStatus::Revoked),
            _ => Err(()),
        }
    }
}

/// Publisher record.
/// `name` is the canonical HTTPS publisher domain and is the identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manufacturer {
    pub id: i64,
    /// Canonical DNS domain, e.g. `apps.example.com`.
    pub name: String,
    pub public_key: String,
    pub key_id: String,
    /// Last verified `actr.publisher/v1` descriptor.
    pub descriptor_json: String,
    pub contact: Option<String>,
    pub status: MfrStatus,
    pub created_at: i64,
    pub updated_at: Option<i64>,
    pub verified_at: Option<i64>,
    pub suspended_at: Option<i64>,
    pub revoked_at: Option<i64>,
    /// Signing key expiration (unix timestamp). Set on activate(), checked on publish.
    pub key_expires_at: Option<i64>,
}

impl<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> for Manufacturer {
    fn from_row(row: &'r sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        let status_str: String = row.try_get("status")?;
        let status = status_str.parse().unwrap_or_default();
        Ok(Manufacturer {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            public_key: row.try_get("public_key")?,
            key_id: row.try_get("key_id").unwrap_or_default(),
            descriptor_json: row
                .try_get("descriptor_json")
                .unwrap_or_else(|_| "{}".to_string()),
            contact: row.try_get("contact")?,
            status,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
            verified_at: row.try_get("verified_at")?,
            suspended_at: row.try_get("suspended_at")?,
            revoked_at: row.try_get("revoked_at")?,
            key_expires_at: row.try_get("key_expires_at").unwrap_or(None),
        })
    }
}

impl Manufacturer {
    pub async fn create(
        pool: &SqlitePool,
        name: &str,
        contact: Option<&str>,
    ) -> Result<Self, MfrError> {
        let now = Utc::now().timestamp();
        let id = sqlx::query(
            "INSERT INTO mfr (name, contact, status, created_at) VALUES (?, ?, 'pending', ?)",
        )
        .bind(name)
        .bind(contact)
        .bind(now)
        .execute(pool)
        .await
        .map_err(|e| {
            if e.to_string().contains("UNIQUE") {
                MfrError::AlreadyExists(format!("'{}' already registered", name))
            } else {
                MfrError::Database(e)
            }
        })?
        .last_insert_rowid();

        Self::get(pool, id).await?.ok_or(MfrError::NotFound)
    }

    pub async fn get(pool: &SqlitePool, id: i64) -> Result<Option<Self>, MfrError> {
        Ok(
            sqlx::query_as::<_, Manufacturer>("SELECT * FROM mfr WHERE id = ?")
                .bind(id)
                .fetch_optional(pool)
                .await?,
        )
    }

    pub async fn get_by_name(pool: &SqlitePool, name: &str) -> Result<Option<Self>, MfrError> {
        Ok(
            sqlx::query_as::<_, Manufacturer>("SELECT * FROM mfr WHERE name = ?")
                .bind(name)
                .fetch_optional(pool)
                .await?,
        )
    }

    pub async fn list(pool: &SqlitePool, status: Option<MfrStatus>) -> Result<Vec<Self>, MfrError> {
        if let Some(s) = status {
            Ok(sqlx::query_as::<_, Manufacturer>(
                "SELECT * FROM mfr WHERE status = ? ORDER BY created_at DESC",
            )
            .bind(s.to_string())
            .fetch_all(pool)
            .await?)
        } else {
            Ok(
                sqlx::query_as::<_, Manufacturer>("SELECT * FROM mfr ORDER BY created_at DESC")
                    .fetch_all(pool)
                    .await?,
            )
        }
    }

    pub async fn activate(
        &mut self,
        pool: &SqlitePool,
        public_key: String,
    ) -> Result<(), MfrError> {
        if self.status != MfrStatus::Pending {
            return Err(MfrError::InvalidStatus(format!(
                "cannot activate from status: {}",
                self.status
            )));
        }
        let now = Utc::now().timestamp();
        let new_key_id = crate::crypto::compute_key_id_from_b64(&public_key)?;
        let key_expires_at = now + 365 * 24 * 3600; // 1 year
        sqlx::query(
            "UPDATE mfr SET status = 'active', public_key = ?, key_id = ?, verified_at = ?, updated_at = ?, key_expires_at = ? WHERE id = ?",
        )
        .bind(&public_key)
        .bind(&new_key_id)
        .bind(now)
        .bind(now)
        .bind(key_expires_at)
        .bind(self.id)
        .execute(pool)
        .await?;
        self.status = MfrStatus::Active;
        self.public_key = public_key;
        self.key_id = new_key_id;
        self.verified_at = Some(now);
        self.updated_at = Some(now);
        self.key_expires_at = Some(key_expires_at);
        Ok(())
    }

    /// Rotate the signing key: archive current key to history, set new key.
    /// Accepts the new public key and optionally generates a new key_id.
    pub async fn renew_key(
        &mut self,
        pool: &SqlitePool,
        new_public_key: String,
    ) -> Result<String, MfrError> {
        if self.status != MfrStatus::Active {
            return Err(MfrError::InvalidStatus(format!(
                "cannot renew key from status: {}",
                self.status
            )));
        }

        // Archive current key to history (only if there is one)
        if !self.public_key.is_empty() && !self.key_id.is_empty() {
            let created_at = self.verified_at.unwrap_or(self.created_at);
            super::key_history::MfrKeyHistory::archive(
                pool,
                self.id,
                &self.key_id,
                &self.public_key,
                created_at,
            )
            .await?;
        }

        let now = chrono::Utc::now().timestamp();
        let new_key_id = crate::crypto::compute_key_id_from_b64(&new_public_key)?;
        let key_expires_at = now + 365 * 24 * 3600; // 1 year

        sqlx::query(
            "UPDATE mfr SET public_key = ?, key_id = ?, key_expires_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&new_public_key)
        .bind(&new_key_id)
        .bind(key_expires_at)
        .bind(now)
        .bind(self.id)
        .execute(pool)
        .await?;

        self.public_key = new_public_key;
        self.key_id = new_key_id.clone();
        self.key_expires_at = Some(key_expires_at);
        self.updated_at = Some(now);

        Ok(new_key_id)
    }

    /// Emergency-revoke the CURRENTLY ACTIVE signing key (private key compromised).
    ///
    /// `MfrKeyHistory::revoke` only touches keys that were already rotated away;
    /// this closes the gap where the live key itself is compromised and must be
    /// killed immediately. The operation is atomic:
    ///   1. the live key is archived into `mfr_key_history` with status `revoked`
    ///      (not `retired`), so both exact-key resolution (`is_key_id_revoked`,
    ///      `get_by_key_id`) and the rollback blacklist (`key_id_ever_used`)
    ///      reject it forever, and
    ///   2. the key material and expiry are cleared from the publisher record.
    ///
    /// Coherent post-state: the publisher is deliberately LEFT in `active` status
    /// but WITHOUT a usable key. With `public_key`/`key_id` blanked and
    /// `key_expires_at` set to NULL, every current-key path fails closed -
    /// `resolve_by_name` and `validate_publish_authority` return
    /// `CertificateExpired`, and `resolve_key_by_id` for the old key_id returns
    /// `KeyRevoked`. Recovery is only possible via `renew_key`, which requires
    /// `active` status and, thanks to the blacklist, cannot reinstate the revoked
    /// key - the publisher must rotate to a brand-new key.
    ///
    /// Doing both writes in one transaction is required for safety: a half-applied
    /// state where the key is archived as revoked but still present on the `mfr`
    /// row would let `resolve_by_name` (which does not consult the revoked set)
    /// keep serving the compromised key.
    pub async fn revoke_active_key(&mut self, pool: &SqlitePool) -> Result<(), MfrError> {
        if self.status != MfrStatus::Active {
            return Err(MfrError::InvalidStatus(format!(
                "cannot revoke active key from status: {}",
                self.status
            )));
        }
        if self.public_key.is_empty() || self.key_id.is_empty() {
            return Err(MfrError::InvalidStatus(
                "publisher has no active key to revoke".to_string(),
            ));
        }

        let now = Utc::now().timestamp();
        let created_at = self.verified_at.unwrap_or(self.created_at);
        let mut tx = pool.begin().await?;

        // Archive the live key directly as `revoked`: a compromised key must be
        // rejected by all future verification, not merely retired. The UNIQUE
        // (mfr_id, key_id) index cannot conflict here because the current key has
        // never been archived while it was in use.
        sqlx::query(
            "INSERT INTO mfr_key_history
             (mfr_id, key_id, public_key, status, created_at, retired_at)
             VALUES (?, ?, ?, 'revoked', ?, ?)",
        )
        .bind(self.id)
        .bind(&self.key_id)
        .bind(&self.public_key)
        .bind(created_at)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        // Blank the live key material and expiry so no resolution path can serve
        // it and publish fails closed until the publisher renews to a fresh key.
        sqlx::query(
            "UPDATE mfr SET public_key = '', key_id = '', key_expires_at = NULL,
                 updated_at = ? WHERE id = ?",
        )
        .bind(now)
        .bind(self.id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        self.public_key = String::new();
        self.key_id = String::new();
        self.key_expires_at = None;
        self.updated_at = Some(now);
        Ok(())
    }

    pub async fn suspend(&mut self, pool: &SqlitePool) -> Result<(), MfrError> {
        if self.status != MfrStatus::Active {
            return Err(MfrError::InvalidStatus(format!(
                "cannot suspend from status: {}",
                self.status
            )));
        }
        let now = Utc::now().timestamp();
        sqlx::query(
            "UPDATE mfr SET status = 'suspended', suspended_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(now)
        .bind(now)
        .bind(self.id)
        .execute(pool)
        .await?;
        self.status = MfrStatus::Suspended;
        self.suspended_at = Some(now);
        Ok(())
    }

    pub async fn reinstate(&mut self, pool: &SqlitePool) -> Result<(), MfrError> {
        if self.status != MfrStatus::Suspended {
            return Err(MfrError::InvalidStatus(format!(
                "cannot reinstate from status: {}",
                self.status
            )));
        }
        let now = Utc::now().timestamp();
        sqlx::query("UPDATE mfr SET status = 'active', updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(self.id)
            .execute(pool)
            .await?;
        self.status = MfrStatus::Active;
        self.updated_at = Some(now);
        Ok(())
    }

    pub async fn revoke(&mut self, pool: &SqlitePool) -> Result<(), MfrError> {
        let now = Utc::now().timestamp();
        sqlx::query(
            "UPDATE mfr SET status = 'revoked', revoked_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(now)
        .bind(now)
        .bind(self.id)
        .execute(pool)
        .await?;
        self.status = MfrStatus::Revoked;
        self.revoked_at = Some(now);
        Ok(())
    }

    pub async fn delete(pool: &SqlitePool, id: i64) -> Result<(), MfrError> {
        sqlx::query("DELETE FROM mfr_key_history WHERE mfr_id = ?")
            .bind(id)
            .execute(pool)
            .await?;
        sqlx::query("DELETE FROM mfr_package WHERE mfr_id = ?")
            .bind(id)
            .execute(pool)
            .await?;
        sqlx::query("DELETE FROM mfr_challenge WHERE mfr_id = ?")
            .bind(id)
            .execute(pool)
            .await?;
        sqlx::query("DELETE FROM mfr WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }
}
