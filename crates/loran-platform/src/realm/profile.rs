//! Accepted Realm Profiles (`actr/protocols/realm-profile.md`).
//!
//! Each Realm keeps its latest accepted Realm Profile verbatim, so the served
//! bytes are the signed bytes. A retired Profile stays as served proof that
//! the Realm is gone.

use actr_protocol::RealmSigningKeys;
use actr_protocol::realm_profile::{RealmProfile, RealmProfileError, RealmStatus};
use chrono::Utc;

use super::domain::digest;
use super::error::RealmError;
use super::model::{Realm, retire_in};
use crate::storage::db::get_database;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RealmProfileRecord {
    pub realm: String,
    pub sequence: i64,
    pub digest: String,
    pub document: String,
    pub accepted_at: i64,
}

impl RealmProfileRecord {
    pub async fn get(realm: &str) -> Result<Option<Self>, RealmError> {
        Ok(sqlx::query_as::<_, Self>(
            "SELECT realm, sequence, digest, document, accepted_at
             FROM realm_profile WHERE realm = ?",
        )
        .bind(realm)
        .fetch_optional(get_database().get_pool())
        .await?)
    }

    /// Verify the stored document again under the domain's `keys`.
    pub fn verify(&self, keys: &RealmSigningKeys) -> Result<RealmProfile, RealmProfileError> {
        RealmProfile::verify(self.document.as_bytes(), &self.realm, keys)
    }

    /// Store `profile`, verified from `document`, as the accepted Realm
    /// Profile of its Realm. The caller has checked it against the previous
    /// one with [`RealmProfile::check_successor`]; `previous_digest` is that
    /// record's digest, or `None` when the Realm had none, and the store
    /// fails with [`RealmError::Conflict`] if another write came first.
    ///
    /// In the same transaction an active Profile activates a pending Realm,
    /// setting `admin_token_hash` as its Realm admin token, and a retired
    /// Profile retires the Realm. Returns the Realm and whether this store
    /// activated it.
    pub async fn store(
        profile: &RealmProfile,
        document: &str,
        previous_digest: Option<&str>,
        admin_token_hash: Option<&str>,
    ) -> Result<(Realm, bool), RealmError> {
        let name = profile.realm.name.as_str();
        let now = Utc::now().timestamp();
        let mut tx = get_database().get_pool().begin().await?;

        let status: Option<String> = sqlx::query_scalar("SELECT status FROM realm WHERE name = ?")
            .bind(name)
            .fetch_optional(&mut *tx)
            .await?;
        match status.as_deref() {
            None => return Err(RealmError::NotFound),
            Some("retired") => return Err(RealmError::Retired),
            Some(_) => {}
        }

        let digest = digest(document.as_bytes());
        let written = match previous_digest {
            Some(previous) => sqlx::query(
                "UPDATE realm_profile SET sequence = ?, digest = ?, document = ?, accepted_at = ?
                 WHERE realm = ? AND digest = ?",
            )
            .bind(profile.sequence as i64)
            .bind(&digest)
            .bind(document)
            .bind(now)
            .bind(name)
            .bind(previous)
            .execute(&mut *tx)
            .await?
            .rows_affected(),
            None => sqlx::query(
                "INSERT INTO realm_profile (realm, sequence, digest, document, accepted_at)
                 VALUES (?, ?, ?, ?, ?) ON CONFLICT(realm) DO NOTHING",
            )
            .bind(name)
            .bind(profile.sequence as i64)
            .bind(&digest)
            .bind(document)
            .bind(now)
            .execute(&mut *tx)
            .await?
            .rows_affected(),
        };
        if written == 0 {
            return Err(RealmError::Conflict(format!(
                "the Realm Profile of {name} changed concurrently"
            )));
        }

        let mut activated = false;
        match profile.realm.status {
            RealmStatus::Retired => retire_in(&mut tx, name, now).await?,
            RealmStatus::Active => {
                activated = sqlx::query(
                    "UPDATE realm SET status = 'active', pending_expires_at = NULL,
                        admin_token_hash = ?, updated_at = ?
                     WHERE name = ? AND status = 'pending'",
                )
                .bind(admin_token_hash)
                .bind(now)
                .bind(name)
                .execute(&mut *tx)
                .await?
                .rows_affected()
                    == 1;
            }
        }
        tx.commit().await?;
        super::changes::announce(name);
        let realm = Realm::get(name).await?.ok_or(RealmError::NotFound)?;
        Ok((realm, activated))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::realm::RealmStatus as Lifecycle;
    use crate::realm::domain::tests::signed_member;
    use crate::realm::domain::verify_member;
    use crate::util::test_utils::utils::setup_test_db;
    use actr_protocol::KeyStatus;
    use actr_protocol::realm_profile::jcs;
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as BASE64;
    use ed25519_dalek::{Signer, SigningKey};
    use serde_json::{Value, json};
    use serial_test::serial;

    const NAME: &str = "k7m2q9xh@acme.com";

    fn document(sequence: u64, status: &str, key: &SigningKey) -> String {
        let mut profile = json!({
            "schema": "actr.realm/v1",
            "sequence": sequence,
            "realm": { "name": NAME, "display_name": "Notes", "status": status },
            "issuer": { "url": "https://provider.example/issuer", "keys": [] },
            "provider": { "id": "provider.example", "signaling": [], "ice": [] },
            "policy": { "membership": "invite-only" },
            "issued_at": "2026-09-25T00:00:00Z",
        });
        if status == "active" {
            profile["package"] = json!({
                "href": "https://cdn.example/notes.actr", "sha256": "ab".repeat(32)
            });
        }
        let signature = key.sign(jcs(&profile).as_bytes());
        profile["signature"] = json!({
            "key_id": "rs-1", "algorithm": "Ed25519",
            "value": BASE64.encode(signature.to_bytes()),
        });
        serde_json::to_string(&profile).unwrap()
    }

    #[tokio::test]
    #[serial]
    async fn stores_profiles_and_retires_with_the_retired_one() -> anyhow::Result<()> {
        setup_test_db().await?;
        let key = SigningKey::from_bytes(&[1; 32]);
        let keys = verify_member(&signed_member(1, &[("rs-1", &key, KeyStatus::Active)]))?;
        let pool = get_database().get_pool();

        Realm {
            name: NAME.to_string(),
            status: Lifecycle::Pending,
            secret_current: "hash".to_string(),
            ..Default::default()
        }
        .insert(None)
        .await?;
        sqlx::query(
            "INSERT INTO actoracl (realm, source_realm, from_type, to_type, access)
             VALUES (?1, ?1, 'acme/a@1', 'acme/b@1', 1)",
        )
        .bind(NAME)
        .execute(pool)
        .await?;

        let first = document(1, "active", &key);
        let profile = RealmProfile::verify(first.as_bytes(), NAME, &keys)?;
        let (realm, activated) =
            RealmProfileRecord::store(&profile, &first, None, Some("token-hash")).await?;
        assert_eq!(realm.status, Lifecycle::Active);
        assert!(activated);
        // A second write that missed the first one conflicts.
        assert!(matches!(
            RealmProfileRecord::store(&profile, &first, None, None).await,
            Err(RealmError::Conflict(_))
        ));

        let stored = RealmProfileRecord::get(NAME).await?.unwrap();
        assert_eq!(stored.document, first);
        let accepted = stored.verify(&keys)?;

        let retired = document(2, "retired", &key);
        let profile = RealmProfile::verify(retired.as_bytes(), NAME, &keys)?;
        accepted.check_successor(&profile)?;
        assert!(matches!(
            RealmProfileRecord::store(&profile, &retired, Some("stale"), None).await,
            Err(RealmError::Conflict(_))
        ));
        let (realm, _) =
            RealmProfileRecord::store(&profile, &retired, Some(&stored.digest), None).await?;
        assert_eq!(realm.status, Lifecycle::Retired);
        assert_eq!(RealmProfileRecord::get(NAME).await?.unwrap().sequence, 2);
        let acl: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM actoracl")
            .fetch_one(pool)
            .await?;
        assert_eq!(acl, 0);

        // The retired Profile is final.
        let later = document(3, "active", &key);
        let profile = RealmProfile::verify(later.as_bytes(), NAME, &keys)?;
        assert!(matches!(
            RealmProfileRecord::store(&profile, &later, None, None).await,
            Err(RealmError::Retired)
        ));
        let stored: Value =
            serde_json::from_str(&RealmProfileRecord::get(NAME).await?.unwrap().document)?;
        assert_eq!(stored["realm"]["status"], "retired");
        Ok(())
    }
}
