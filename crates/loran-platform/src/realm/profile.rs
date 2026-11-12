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
use super::membership::{IssuedInvite, Signer, check_key_on, issue_owner_invite_in, require_owner};
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

    pub fn verify_for_update(
        &self,
        keys: &RealmSigningKeys,
    ) -> Result<RealmProfile, RealmProfileError> {
        RealmProfile::verify_stored_baseline(self.document.as_bytes(), &self.realm, keys)
    }

    /// Store `profile`, verified from `document`, as the accepted Realm
    /// Profile of its Realm. The caller has checked it against the previous
    /// one with [`RealmProfile::check_successor`]; `previous_digest` is that
    /// record's digest, or `None` when the Realm had none, and the store
    /// fails with [`RealmError::Conflict`] if another write came first.
    ///
    /// In the same transaction an active Profile activates a pending Realm
    /// and a retired Profile retires the Realm. When this store activates the
    /// Realm and `owner_invite_by` is set, an owner invite created by it
    /// comes back too. Returns the Realm and whether this store activated
    /// it.
    ///
    /// When `owner` requested the Profile, the same transaction rechecks that
    /// its key is unrevoked, its member active and still the owner: an
    /// ownership change or revocation that committed while the Profile was
    /// signed stops the store.
    pub async fn store(
        profile: &RealmProfile,
        document: &str,
        previous_digest: Option<&str>,
        owner_invite_by: Option<&str>,
        owner: Option<&Signer>,
    ) -> Result<(Realm, bool, Option<IssuedInvite>), RealmError> {
        Self::store_inner(
            profile,
            document,
            previous_digest,
            owner_invite_by,
            owner,
            false,
        )
        .await
    }

    /// Store a locally signed Profile only if its issuer keys still match the
    /// ledger in the commit transaction. A transition during the signing RPC
    /// must not republish removed keys or overwrite a newer key status.
    pub async fn store_issued(
        profile: &RealmProfile,
        document: &str,
        previous_digest: Option<&str>,
        owner: Option<&Signer>,
    ) -> Result<(Realm, bool, Option<IssuedInvite>), RealmError> {
        Self::store_inner(profile, document, previous_digest, None, owner, true).await
    }

    async fn store_inner(
        profile: &RealmProfile,
        document: &str,
        previous_digest: Option<&str>,
        owner_invite_by: Option<&str>,
        owner: Option<&Signer>,
        check_issuer_ledger: bool,
    ) -> Result<(Realm, bool, Option<IssuedInvite>), RealmError> {
        let name = profile.realm.name.as_str();
        let now = Utc::now().timestamp();
        let mut tx = get_database()
            .get_pool()
            .begin_with("BEGIN IMMEDIATE")
            .await?;

        let status: Option<String> = sqlx::query_scalar("SELECT status FROM realm WHERE name = ?")
            .bind(name)
            .fetch_optional(&mut *tx)
            .await?;
        match status.as_deref() {
            None => return Err(RealmError::NotFound),
            Some("retired") => return Err(RealmError::Retired),
            Some(_) => {}
        }
        if let Some(owner) = owner {
            require_owner(&check_key_on(&mut tx, name, &owner.key_id).await?)?;
        }
        if check_issuer_ledger {
            let keys = sqlx::query_as::<_, super::issuer_key::RealmIssuerKey>(
                "SELECT * FROM realm_issuer_key WHERE realm = ?",
            )
            .bind(name)
            .fetch_all(&mut *tx)
            .await?;
            if keys.len() != profile.issuer.keys.len()
                || keys.iter().any(|key| {
                    profile.issuer_key(key.key_id).is_none_or(|published| {
                        published.public_key != key.public_key
                            || published.status != key.profile_status()
                    })
                })
            {
                return Err(RealmError::Conflict(
                    "issuer keys changed while signing the Realm Profile".into(),
                ));
            }
        }

        if status.as_deref() == Some("pending") && profile.realm.status == RealmStatus::Active {
            let expected: Option<String> =
                sqlx::query_scalar("SELECT selection FROM realm_creation_selection WHERE realm=?")
                    .bind(name)
                    .fetch_optional(&mut *tx)
                    .await?;
            if let Some(expected) = expected {
                let expected = serde_json::from_str(&expected).map_err(|e| {
                    RealmError::DatabaseError(format!("invalid stored selection: {e}"))
                })?;
                if profile.selection.as_ref() != Some(&expected) {
                    return Err(RealmError::Conflict(
                        "first Profile differs from the create_realm selection".into(),
                    ));
                }
            }
        }
        if let Some(selection) = &profile.selection {
            for package in selection.packages.values() {
                let count = sqlx::query(
                    "INSERT INTO realm_package_digest (realm, actor_type, sha256) VALUES (?, ?, ?)
                     ON CONFLICT(realm, actor_type) DO UPDATE SET sha256 = excluded.sha256
                     WHERE realm_package_digest.sha256 = excluded.sha256",
                )
                .bind(name)
                .bind(&package.r#type)
                .bind(&package.sha256)
                .execute(&mut *tx)
                .await?
                .rows_affected();
                if count == 0 {
                    return Err(RealmError::Conflict(format!(
                        "immutable package {} changed digest",
                        package.r#type
                    )));
                }
            }
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
        let mut owner_invite = None;
        match profile.realm.status {
            RealmStatus::Retired => retire_in(&mut tx, name, now).await?,
            RealmStatus::Active => {
                super::issuer_key::RealmIssuerKey::mark_published(&mut tx, profile, now).await?;
                activated = sqlx::query(
                    "UPDATE realm SET status = 'active', pending_expires_at = NULL,
                        updated_at = ?
                     WHERE name = ? AND status = 'pending'",
                )
                .bind(now)
                .bind(name)
                .execute(&mut *tx)
                .await?
                .rows_affected()
                    == 1;
                if activated && let Some(created_by) = owner_invite_by {
                    owner_invite = Some(
                        issue_owner_invite_in(&mut tx, name, created_by, now)
                            .await
                            .map_err(|error| RealmError::DatabaseError(error.to_string()))?,
                    );
                }
            }
        }
        tx.commit().await?;
        crate::aid::credential::validator::AIdCredentialValidator::reconcile_profile_cache(profile)
            .await;
        super::changes::announce(name);
        let realm = Realm::get(name).await?.ok_or(RealmError::NotFound)?;
        Ok((realm, activated, owner_invite))
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
            "issuer": { "url": "https://provider.example/issuer", "keys": [{
                "id": "7", "algorithm": "Ed25519", "status": "active",
                "public_key": BASE64.encode([2u8; 32]),
            }] },
            "provider": { "id": "provider.example", "signaling": [], "ice": [] },
            "policy": { "membership": "invite-only" },
            "issued_at": "2026-09-25T00:00:00Z",
        });
        if status == "active" {
            profile["primary_package"] = json!("service");
            profile["front_package"] = json!("ui");
            profile["packages"] = json!({"ui":{"href":"https://cdn.example/front.actr","sha256":"cd".repeat(32),"type":"acme.com/ui@1.0.0"},"service":{
                "href": "https://cdn.example/notes.actr", "sha256": "ab".repeat(32),
                "type": "acme.com/notes@1.0.0",
            }});
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
        let (realm, activated, invite) =
            RealmProfileRecord::store(&profile, &first, None, Some("holder"), None).await?;
        assert_eq!(realm.status, Lifecycle::Active);
        assert!(activated);
        assert!(invite.unwrap().invite.starts_with("ri_"));
        // A second write that missed the first one conflicts.
        assert!(matches!(
            RealmProfileRecord::store(&profile, &first, None, None, None).await,
            Err(RealmError::Conflict(_))
        ));

        let stored = RealmProfileRecord::get(NAME).await?.unwrap();
        assert_eq!(stored.document, first);
        let accepted = stored.verify(&keys)?;

        let retired = document(2, "retired", &key);
        let profile = RealmProfile::verify(retired.as_bytes(), NAME, &keys)?;
        accepted.check_successor(&profile)?;
        assert!(matches!(
            RealmProfileRecord::store(&profile, &retired, Some("stale"), None, None).await,
            Err(RealmError::Conflict(_))
        ));
        let (realm, _, _) =
            RealmProfileRecord::store(&profile, &retired, Some(&stored.digest), None, None).await?;
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
            RealmProfileRecord::store(&profile, &later, None, None, None).await,
            Err(RealmError::Retired)
        ));
        let stored: Value =
            serde_json::from_str(&RealmProfileRecord::get(NAME).await?.unwrap().document)?;
        assert_eq!(stored["realm"]["status"], "retired");
        Ok(())
    }
}
