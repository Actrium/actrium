//! Accepted Realm Profiles (`actr/protocols/realm-profile.md`).
//!
//! Each Realm keeps its latest accepted Realm Profile verbatim, so the served
//! bytes are the signed bytes. A retired Profile stays as served proof that
//! the Realm is gone.

use actr_protocol::RealmSigningKeys;
use actr_protocol::realm_profile::{
    FrontUpdate, PackageSelectionChange, RealmPackageSelection, RealmProfile, RealmProfileError,
    RealmStatus,
};
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

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RealmPackageReleaseRecord {
    pub sequence: i64,
    previous_document: String,
    document: String,
    rollback: bool,
}

#[derive(Clone, Copy)]
pub enum PackageRelease<'a> {
    Front(&'a FrontUpdate),
    Service(&'a FrontUpdate),
}

impl PackageRelease<'_> {
    fn update(&self) -> &FrontUpdate {
        match self {
            Self::Front(update) | Self::Service(update) => update,
        }
    }
}

impl RealmPackageReleaseRecord {
    pub async fn get(
        realm: &str,
        update: &FrontUpdate,
        selection: &RealmPackageSelection,
    ) -> Result<Option<Self>, RealmError> {
        update.check().map_err(RealmError::Conflict)?;
        let receipt = sqlx::query_as::<_, Self>(
            "SELECT sequence, previous_document, document, rollback FROM realm_package_release
             WHERE realm = ? AND operation_id = ?",
        )
        .bind(realm)
        .bind(&update.operation_id)
        .fetch_optional(get_database().get_pool())
        .await?;
        if let Some(receipt) = &receipt
            && (receipt.sequence as u64 != update.expected_sequence + 1
                || read_selection(&receipt.document)? != *selection)
        {
            return Err(RealmError::Conflict(
                "Package release operation ID already used".into(),
            ));
        }
        Ok(receipt)
    }

    pub async fn rollback_selection(
        realm: &str,
        current: &RealmPackageSelection,
    ) -> Result<Option<RealmPackageSelection>, RealmError> {
        let mut connection = get_database().get_pool().acquire().await?;
        Self::rollback_selection_on(&mut connection, realm, current).await
    }

    async fn rollback_selection_on(
        connection: &mut sqlx::SqliteConnection,
        realm: &str,
        current: &RealmPackageSelection,
    ) -> Result<Option<RealmPackageSelection>, RealmError> {
        let latest = sqlx::query_as::<_, Self>(
            "SELECT sequence, previous_document, document, rollback FROM realm_package_release
             WHERE realm = ? ORDER BY sequence DESC LIMIT 1",
        )
        .bind(realm)
        .fetch_optional(connection)
        .await?;
        let Some(latest) = latest else {
            return Ok(None);
        };
        if latest.rollback || read_selection(&latest.document)? != *current {
            return Ok(None);
        }
        read_selection(&latest.previous_document).map(Some)
    }

    pub fn reply(
        &self,
        realm: &str,
        update: &FrontUpdate,
    ) -> Result<serde_json::Value, RealmError> {
        let profile: serde_json::Value = serde_json::from_str(&self.document).map_err(|e| {
            RealmError::DatabaseError(format!("invalid package release receipt: {e}"))
        })?;
        Ok(serde_json::json!({
            "realm": realm,
            "operation_id": update.operation_id,
            "sequence": self.sequence,
            "previous_selection": read_selection(&self.previous_document)?,
            "profile": profile,
        }))
    }
}

fn read_selection(document: &str) -> Result<RealmPackageSelection, RealmError> {
    serde_json::from_str(document)
        .map_err(|e| RealmError::DatabaseError(format!("invalid stored package selection: {e}")))
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
            None,
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
        Self::store_inner(profile, document, previous_digest, None, owner, true, None).await
    }

    pub async fn store_front_update(
        profile: &RealmProfile,
        document: &str,
        previous_digest: Option<&str>,
        owner: Option<&Signer>,
        update: &FrontUpdate,
    ) -> Result<RealmPackageReleaseRecord, RealmError> {
        Self::store_package_release(
            profile,
            document,
            previous_digest,
            owner,
            PackageRelease::Front(update),
        )
        .await
    }

    pub async fn store_package_release(
        profile: &RealmProfile,
        document: &str,
        previous_digest: Option<&str>,
        owner: Option<&Signer>,
        release: PackageRelease<'_>,
    ) -> Result<RealmPackageReleaseRecord, RealmError> {
        let update = release.update();
        let realm = &profile.realm.name;
        let selection = profile.selection.as_ref().ok_or_else(|| {
            RealmError::Conflict("a package release needs an active selection".into())
        })?;
        // Both the current Profile and the receipt are committed together. If
        // concurrent retries race, only the winner gets to change the Profile.
        let error = match Self::store_inner(
            profile,
            document,
            previous_digest,
            None,
            owner,
            owner.is_some() || matches!(release, PackageRelease::Service(_)),
            Some(release),
        )
        .await
        {
            Ok(_) => RealmError::DatabaseError("Front update receipt missing".into()),
            Err(error @ RealmError::Conflict(_)) => error,
            Err(error) => return Err(error),
        };
        RealmPackageReleaseRecord::get(realm, update, selection)
            .await?
            .ok_or(error)
    }

    async fn store_inner(
        profile: &RealmProfile,
        document: &str,
        previous_digest: Option<&str>,
        owner_invite_by: Option<&str>,
        owner: Option<&Signer>,
        check_issuer_ledger: bool,
        release: Option<PackageRelease<'_>>,
    ) -> Result<(Realm, bool, Option<IssuedInvite>), RealmError> {
        let name = profile.realm.name.as_str();
        let now = Utc::now().timestamp();
        let mut tx = get_database()
            .get_pool()
            .begin_with("BEGIN IMMEDIATE")
            .await?;

        let current = Realm::get_on(&mut tx, name)
            .await?
            .ok_or(RealmError::NotFound)?;
        if current.status == super::model::RealmStatus::Retired {
            return Err(RealmError::Retired);
        }
        if let Some(owner) = owner {
            require_owner(&check_key_on(&mut tx, name, &owner.key_id).await?)?;
        }
        if let Some(release) = release {
            let update = release.update();
            update.check().map_err(RealmError::Conflict)?;
            if current.status != super::model::RealmStatus::Active
                || profile.realm.status != RealmStatus::Active
                || profile.sequence != update.expected_sequence + 1
            {
                return Err(RealmError::Conflict(
                    "invalid package release revision".into(),
                ));
            }
            let previous: Option<String> = sqlx::query_scalar(
                "SELECT document FROM realm_profile WHERE realm = ? AND sequence = ? AND digest = ?",
            )
            .bind(name)
            .bind(update.expected_sequence as i64)
            .bind(previous_digest)
            .fetch_optional(&mut *tx)
            .await?;
            let previous = previous
                .ok_or_else(|| RealmError::Conflict("package release baseline changed".into()))?;
            let old = read_selection(&previous)?;
            let new = profile.selection.as_ref().ok_or_else(|| {
                RealmError::Conflict("a package release needs a complete selection".into())
            })?;
            let change = old.release_change(new).map_err(RealmError::Conflict)?;
            let expected = match release {
                PackageRelease::Front(_) => PackageSelectionChange::Front,
                PackageRelease::Service(_) => PackageSelectionChange::Service,
            };
            if change != Some(expected) {
                return Err(RealmError::Conflict(
                    "package release does not match the selection change".into(),
                ));
            }
            if update.rollback
                && RealmPackageReleaseRecord::rollback_selection_on(&mut tx, name, &old)
                    .await?
                    .as_ref()
                    != Some(new)
            {
                return Err(RealmError::Conflict(
                    "the last package release is no longer available for rollback".into(),
                ));
            }
            if expected == PackageSelectionChange::Service
                && let Some(owner) = owner
            {
                let service_type = &new.selected().map_err(RealmError::Conflict)?.0.r#type;
                let registered: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM issued_actor_identity
                         WHERE realm = ? AND actor_type = ?
                           AND member_id = ? AND auth_mode = 1 AND development = 0)",
                )
                .bind(name)
                .bind(service_type)
                .bind(&owner.member_id)
                .fetch_one(&mut *tx)
                .await?;
                if !registered {
                    return Err(RealmError::Conflict(
                        "the owner has not registered this Service type".into(),
                    ));
                }
            }
            let inserted = sqlx::query(
                "INSERT INTO realm_package_release
                 (realm, operation_id, sequence, previous_document, document, rollback)
                 VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT DO NOTHING",
            )
            .bind(name)
            .bind(&update.operation_id)
            .bind(profile.sequence as i64)
            .bind(previous)
            .bind(document)
            .bind(update.rollback)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if inserted != 1 {
                return Err(RealmError::Conflict(
                    "Package release operation ID already used".into(),
                ));
            }
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

        if current.status == super::model::RealmStatus::Pending
            && profile.realm.status == RealmStatus::Active
        {
            if current.policy != profile.policy {
                return Err(RealmError::Conflict(
                    "first Profile differs from the create_realm admission policy".into(),
                ));
            }
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
                Realm::store_policy(&mut tx, name, profile.policy).await?;
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
    async fn front_release_is_conditional_and_rollback_preserves_service() -> anyhow::Result<()> {
        use actr_protocol::realm_profile::sign_document;

        setup_test_db().await?;
        let key = SigningKey::from_bytes(&[1; 32]);
        let keys = verify_member(&signed_member(1, &[("rs-1", &key, KeyStatus::Active)]))?;
        Realm {
            name: NAME.into(),
            status: Lifecycle::Pending,
            ..Default::default()
        }
        .insert(None)
        .await?;

        let first = document(1, "active", &key);
        let initial = RealmProfile::verify(first.as_bytes(), NAME, &keys)?;
        RealmProfileRecord::store(&initial, &first, None, None, None).await?;
        let original = RealmProfileRecord::get(NAME).await?.unwrap();

        let operation = FrontUpdate {
            expected_sequence: 1,
            operation_id: "a".repeat(32),
            rollback: false,
        };
        let mut second: Value = serde_json::from_str(&first)?;
        second["sequence"] = json!(2);
        second["packages"]["ui"]["type"] = json!("acme.com/ui@1.1.0");
        second["packages"]["ui"]["sha256"] = json!("12".repeat(32));

        let mut changed_service = second.clone();
        changed_service["packages"]["service"]["type"] = json!("acme.com/notes@2.0.0");
        sign_document(&mut changed_service, "rs-1", &key);
        let changed = RealmProfile::verify(changed_service.to_string().as_bytes(), NAME, &keys)?;
        assert!(
            RealmProfileRecord::store_front_update(
                &changed,
                &changed_service.to_string(),
                Some(&original.digest),
                None,
                &operation,
            )
            .await
            .is_err()
        );
        assert_eq!(RealmProfileRecord::get(NAME).await?.unwrap().sequence, 1);

        sign_document(&mut second, "rs-1", &key);
        let candidate = RealmProfile::verify(second.to_string().as_bytes(), NAME, &keys)?;
        let receipt = RealmProfileRecord::store_front_update(
            &candidate,
            &second.to_string(),
            Some(&original.digest),
            None,
            &operation,
        )
        .await?;
        assert_eq!(receipt.sequence, 2);
        let retry = RealmProfileRecord::store_front_update(
            &candidate,
            &second.to_string(),
            Some(&original.digest),
            None,
            &operation,
        )
        .await?;
        assert_eq!(
            retry.reply(NAME, &operation)?,
            receipt.reply(NAME, &operation)?
        );

        let prior: RealmPackageSelection = read_selection(&first)?;
        let mut old: Value = serde_json::from_str(&first)?;
        old["sequence"] = json!(3);
        sign_document(&mut old, "rs-1", &key);
        let rollback = RealmProfile::verify(old.to_string().as_bytes(), NAME, &keys)?;
        let current = RealmProfileRecord::get(NAME).await?.unwrap();
        let operation = FrontUpdate {
            expected_sequence: 2,
            operation_id: "b".repeat(32),
            rollback: true,
        };
        RealmProfileRecord::store_front_update(
            &rollback,
            &old.to_string(),
            Some(&current.digest),
            None,
            &operation,
        )
        .await?;
        let restored = RealmProfileRecord::get(NAME)
            .await?
            .unwrap()
            .verify(&keys)?;
        assert_eq!(restored.sequence, 3);
        assert_eq!(restored.selection, Some(prior));
        Ok(())
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
