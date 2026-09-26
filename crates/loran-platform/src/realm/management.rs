//! State behind Realm management and membership requests
//! (`actr/protocols/realm-profile.md` section 8,
//! `actr/protocols/realm-membership.md`): request nonces and pending Realm
//! expiry.

use chrono::Utc;

use super::error::RealmError;
use super::model::Realm;
use crate::storage::db::get_database;

impl Realm {
    /// Remove pending Realms whose time to become active has passed. They
    /// were never served, so their names are free again.
    pub async fn discard_expired_pending() -> Result<u64, RealmError> {
        let discarded: Vec<String> = sqlx::query_scalar(
            "DELETE FROM realm
             WHERE status = 'pending' AND pending_expires_at IS NOT NULL
               AND pending_expires_at <= ?
             RETURNING name",
        )
        .bind(Utc::now().timestamp())
        .fetch_all(get_database().get_pool())
        .await?;
        for name in &discarded {
            super::changes::announce(name);
        }
        Ok(discarded.len() as u64)
    }
}

/// Record a Realm management request `nonce` until `expires_at` (Unix
/// seconds). Returns `false` if it was already used. Expired nonces are
/// dropped on the way.
pub async fn use_nonce(nonce: &str, expires_at: i64) -> Result<bool, RealmError> {
    let pool = get_database().get_pool();
    sqlx::query("DELETE FROM realm_mgmt_nonce WHERE expires_at < ?")
        .bind(Utc::now().timestamp())
        .execute(pool)
        .await?;
    let inserted = sqlx::query(
        "INSERT INTO realm_mgmt_nonce (nonce, expires_at) VALUES (?, ?)
         ON CONFLICT(nonce) DO NOTHING",
    )
    .bind(nonce)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok(inserted.rows_affected() == 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RealmConfig;
    use crate::realm::RealmStatus;
    use crate::util::test_utils::utils::setup_test_db;
    use serial_test::serial;

    async fn insert(name: &str, status: RealmStatus, pending_expires_at: Option<u64>) {
        Realm {
            name: name.to_string(),
            status,
            pending_expires_at,
            ..Default::default()
        }
        .insert(None)
        .await
        .unwrap();
    }

    #[tokio::test]
    #[serial]
    async fn nonces_are_single_use_until_they_expire() -> anyhow::Result<()> {
        setup_test_db().await?;
        let now = Utc::now().timestamp();
        assert!(use_nonce("n-1", now + 300).await?);
        assert!(!use_nonce("n-1", now + 300).await?);
        assert!(use_nonce("n-old", now - 1).await?);
        // The expired one is dropped by the next use.
        assert!(use_nonce("n-2", now + 300).await?);
        assert!(use_nonce("n-old", now + 300).await?);
        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn expired_pending_realms_are_discarded() -> anyhow::Result<()> {
        setup_test_db().await?;
        let now = Utc::now().timestamp() as u64;
        insert("late@acme.com", RealmStatus::Pending, Some(now - 1)).await;
        insert("soon@acme.com", RealmStatus::Pending, Some(now + 600)).await;
        insert("live@acme.com", RealmStatus::Active, None).await;

        assert_eq!(Realm::discard_expired_pending().await?, 1);
        assert!(Realm::get("late@acme.com").await?.is_none());
        assert!(Realm::get("soon@acme.com").await?.is_some());
        // The name is free again.
        insert("late@acme.com", RealmStatus::Pending, Some(now + 600)).await;
        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn requested_realms_stay_within_domain_quotas() -> anyhow::Result<()> {
        setup_test_db().await?;
        let config = RealmConfig {
            max_realms_per_domain: Some(2),
            max_pending_per_domain: Some(1),
            ..Default::default()
        };
        let pending = |name: &str| Realm {
            name: name.to_string(),
            status: RealmStatus::Pending,
            requester: "member-1".to_string(),
            ..Default::default()
        };
        let a = pending("a@acme.com").insert_requested(&config).await?;
        assert_eq!(
            Realm::get("a@acme.com").await?.unwrap().requester,
            a.requester
        );
        assert!(matches!(
            pending("b@acme.com").insert_requested(&config).await,
            Err(RealmError::QuotaExceeded(_))
        ));
        // Other domains have their own quota.
        pending("c@other.com").insert_requested(&config).await?;

        // No pending Realm left, but acme.com already has two Realms.
        Realm::discard_pending("a@acme.com").await?;
        insert("one@acme.com", RealmStatus::Active, None).await;
        insert("two@acme.com", RealmStatus::Active, None).await;
        assert!(matches!(
            pending("d@acme.com").insert_requested(&config).await,
            Err(RealmError::QuotaExceeded(_))
        ));
        Ok(())
    }
}
