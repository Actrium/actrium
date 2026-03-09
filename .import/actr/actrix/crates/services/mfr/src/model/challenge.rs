use base64::Engine as _;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use crate::MfrError;

const CHALLENGE_TTL_SECS: i64 = 24 * 3600; // 24 hours

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainChallenge {
    pub id: i64,
    pub mfr_id: i64,
    pub token: String,
    pub dns_host: String,
    pub expires_at: i64,
    pub verified_at: Option<i64>,
    pub created_at: i64,
}

impl<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> for DomainChallenge {
    fn from_row(row: &'r sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        Ok(DomainChallenge {
            id: row.try_get("id")?,
            mfr_id: row.try_get("mfr_id")?,
            token: row.try_get("token")?,
            dns_host: row.try_get("dns_host")?,
            expires_at: row.try_get("expires_at")?,
            verified_at: row.try_get("verified_at")?,
            created_at: row.try_get("created_at")?,
        })
    }
}

impl DomainChallenge {
    pub async fn create(pool: &SqlitePool, mfr_id: i64, domain: &str) -> Result<Self, MfrError> {
        use rand::RngCore;
        let mut token_bytes = [0u8; 24];
        rand::thread_rng().fill_bytes(&mut token_bytes);
        let token = format!(
            "actrix-verify={}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token_bytes)
        );
        let dns_host = format!("_actrix-verify.{}", domain);
        let now = Utc::now().timestamp();
        let expires_at = now + CHALLENGE_TTL_SECS;

        let id = sqlx::query(
            "INSERT INTO mfr_challenge (mfr_id, token, dns_host, expires_at, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(mfr_id)
        .bind(&token)
        .bind(&dns_host)
        .bind(expires_at)
        .bind(now)
        .execute(pool)
        .await?
        .last_insert_rowid();

        Ok(DomainChallenge {
            id,
            mfr_id,
            token,
            dns_host,
            expires_at,
            verified_at: None,
            created_at: now,
        })
    }

    pub async fn get_active(pool: &SqlitePool, mfr_id: i64) -> Result<Option<Self>, MfrError> {
        let now = Utc::now().timestamp();
        Ok(sqlx::query_as::<_, DomainChallenge>(
            "SELECT * FROM mfr_challenge WHERE mfr_id = ? AND expires_at > ? AND verified_at IS NULL ORDER BY created_at DESC LIMIT 1",
        )
        .bind(mfr_id)
        .bind(now)
        .fetch_optional(pool)
        .await?)
    }

    pub async fn mark_verified(&mut self, pool: &SqlitePool) -> Result<(), MfrError> {
        let now = Utc::now().timestamp();
        sqlx::query("UPDATE mfr_challenge SET verified_at = ? WHERE id = ?")
            .bind(now)
            .bind(self.id)
            .execute(pool)
            .await?;
        self.verified_at = Some(now);
        Ok(())
    }
}
