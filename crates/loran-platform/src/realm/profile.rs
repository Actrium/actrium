//! Accepted Realm Profiles (`actr/protocols/realm-profile.md`).
//!
//! Each Realm keeps its latest accepted Realm Profile verbatim, so the served
//! bytes are the signed bytes. A retired Profile stays as served proof that
//! the Realm is gone.

use actr_protocol::realm_profile::{
    FrontUpdate, PackageSelectionChange, RealmPackage, RealmPackageSelection, RealmProfile,
    RealmProfileError, RealmStatus,
};
use actr_protocol::{ActrId, RealmSigningKeys};
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
    pub fn update(&self) -> &FrontUpdate {
        match self {
            Self::Front(update) | Self::Service(update) => update,
        }
    }

    pub fn kind(&self) -> PackageSelectionChange {
        match self {
            Self::Front(_) => PackageSelectionChange::Front,
            Self::Service(_) => PackageSelectionChange::Service,
        }
    }
}

enum ProfileWrite<'a> {
    Publish,
    Fetch,
    Issue,
    Release(PackageRelease<'a>),
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

enum StoredSelection {
    Complete(RealmPackageSelection),
    Legacy(RealmPackage),
}

impl StoredSelection {
    fn primary(&self) -> Result<&RealmPackage, RealmError> {
        match self {
            Self::Complete(selection) => selection
                .selected()
                .map(|(service, _)| service)
                .map_err(RealmError::Conflict),
            Self::Legacy(primary) => Ok(primary),
        }
    }
}

fn read_stored_selection(document: &str) -> Result<StoredSelection, RealmError> {
    #[derive(serde::Deserialize)]
    struct StoredPackages {
        #[serde(
            flatten,
            deserialize_with = "RealmPackageSelection::deserialize_optional"
        )]
        selection: Option<RealmPackageSelection>,
        package: Option<serde_json::Value>,
    }

    let previous: StoredPackages = serde_json::from_str(document)
        .map_err(|e| RealmError::DatabaseError(format!("invalid stored Realm packages: {e}")))?;
    if let Some(selection) = previous.selection {
        return Ok(StoredSelection::Complete(selection));
    }
    let value = previous.package.ok_or_else(|| {
        RealmError::Conflict("the accepted Profile has no primary Service".into())
    })?;
    let primary = serde_json::from_value(value)
        .map_err(|e| RealmError::DatabaseError(format!("invalid stored primary Service: {e}")))?;
    Ok(StoredSelection::Legacy(primary))
}

/// Check existing Actor ownership before changing the primary program and
/// reserve the outgoing program's registration and ACLs for the owner.
async fn record_service_switch(
    tx: &mut sqlx::SqliteConnection,
    realm: &str,
    previous: &RealmPackage,
    next: &RealmPackage,
    check_rollback_policy: bool,
) -> Result<(), RealmError> {
    if previous.same_program(next).map_err(RealmError::Conflict)? {
        return Ok(());
    }
    let next = next.actr_type().map_err(RealmError::Conflict)?;
    let program = format!("{}/{}", next.publisher, next.name);
    let serial: Option<i64> = sqlx::query_scalar(
        "SELECT i.serial_number FROM issued_actor_identity i
         JOIN realm_member m ON m.realm = i.realm AND m.member_id = i.member_id
         WHERE i.realm = ?1 AND substr(i.actor_type, 1, length(?2) + 1) = (?2 || '@')
           AND m.status = 'active' AND m.role <> 'owner'
           AND NOT EXISTS (
               SELECT 1 FROM revoked_actor r
               WHERE r.realm = i.realm AND r.serial_number = i.serial_number
           )
         ORDER BY i.serial_number LIMIT 1",
    )
    .bind(realm)
    .bind(&program)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(serial) = serial {
        let actor = ActrId {
            realm: realm.to_owned(),
            serial_number: serial as u64,
        };
        return Err(RealmError::Conflict(format!(
            "Actor {} for {program} belongs to a non-owner. Revoke it before switching the Service.",
            actor.to_string_repr()
        )));
    }
    super::acl::ActorAcl::require_owner_policy(tx, realm, &program).await?;
    let previous = previous.actr_type().map_err(RealmError::Conflict)?;
    let outgoing_program = format!("{}/{}", previous.publisher, previous.name);
    if check_rollback_policy {
        // A forward release records this program as the rollback target.
        super::acl::ActorAcl::require_owner_policy(tx, realm, &outgoing_program).await?;
    }
    sqlx::query(
        "INSERT OR IGNORE INTO realm_previous_service_program (realm, program) VALUES (?, ?)",
    )
    .bind(realm)
    .bind(outgoing_program)
    .execute(tx)
    .await?;
    Ok(())
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
            ProfileWrite::Publish,
        )
        .await
    }

    /// Store an already verified Profile from the external Realm authority
    /// against the accepted record the refresh read. Returns the digest of
    /// the Profile that was written.
    pub async fn store_fetched(
        profile: &RealmProfile,
        document: &str,
        previous_digest: &str,
    ) -> Result<String, RealmError> {
        Self::store_inner(
            profile,
            document,
            Some(previous_digest),
            None,
            None,
            ProfileWrite::Fetch,
        )
        .await?;
        Ok(digest(document.as_bytes()))
    }

    /// Suspend a Realm only while the Profile that caused the suspension is
    /// still current. A concurrent replacement keeps its own enabled state.
    pub async fn suspend_if_current(
        realm: &str,
        expected_digest: &str,
    ) -> Result<bool, RealmError> {
        let suspended = sqlx::query(
            "UPDATE realm SET enabled = FALSE, updated_at = ?3
             WHERE name = ?1 AND enabled AND status <> 'retired'
               AND EXISTS (SELECT 1 FROM realm_profile WHERE realm = ?1 AND digest = ?2)",
        )
        .bind(realm)
        .bind(expected_digest)
        .bind(Utc::now().timestamp())
        .execute(get_database().get_pool())
        .await?
        .rows_affected()
            != 0;
        if suspended {
            super::changes::announce(realm);
        }
        Ok(suspended)
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
        Self::store_inner(
            profile,
            document,
            previous_digest,
            None,
            owner,
            ProfileWrite::Issue,
        )
        .await
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
            ProfileWrite::Release(release),
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
        write: ProfileWrite<'_>,
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
        if let ProfileWrite::Release(release) = write {
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
            let previous = previous.ok_or_else(|| {
                RealmError::Conflict(
                    "The Profile changed. Reload the selection and try again.".into(),
                )
            })?;
            let old = read_selection(&previous)?;
            let new = profile.selection.as_ref().ok_or_else(|| {
                RealmError::Conflict("a package release needs a complete selection".into())
            })?;
            let change = old.release_change(new).map_err(RealmError::Conflict)?;
            if change != Some(release.kind()) {
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
                    "There is no previous selection to roll back to.".into(),
                ));
            }
            record_service_switch(
                &mut tx,
                name,
                old.selected().map_err(RealmError::Conflict)?.0,
                new.selected().map_err(RealmError::Conflict)?.0,
                !update.rollback,
            )
            .await?;
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
        } else if let (Some(digest), Some(selection)) = (previous_digest, &profile.selection) {
            // The accepted row also anchors ordinary writes when its original
            // signing key is no longer listed.
            let previous: Option<String> = sqlx::query_scalar(
                "SELECT document FROM realm_profile WHERE realm = ? AND digest = ?",
            )
            .bind(name)
            .bind(digest)
            .fetch_optional(&mut *tx)
            .await?;
            let previous = previous
                .ok_or_else(|| RealmError::Conflict("Realm Profile baseline changed".into()))?;
            let previous = read_stored_selection(&previous)?;
            let old = previous.primary()?;
            let new = selection.selected().map_err(RealmError::Conflict)?.0;
            if let (ProfileWrite::Fetch, StoredSelection::Complete(accepted)) = (&write, &previous)
            {
                // Signer removal resets the refresh's sequence baseline.
                // A stored selection still constrains a Service program change.
                accepted
                    .check_successor(selection)
                    .map_err(RealmError::Conflict)?;
                record_service_switch(&mut tx, name, old, new, false).await?;
            } else {
                old.check_primary_successor(new).map_err(|_| {
                    RealmError::Conflict(
                        "Use a package release to change the Service publisher or name.".into(),
                    )
                })?;
            }
        }
        let check_issuer_ledger = match write {
            ProfileWrite::Issue | ProfileWrite::Release(PackageRelease::Service(_)) => true,
            ProfileWrite::Release(PackageRelease::Front(_)) => owner.is_some(),
            _ => false,
        };
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
            for (package, digest) in selection
                .packages
                .values()
                .filter_map(|package| package.fixed_digest().map(|digest| (package, digest)))
            {
                let count = sqlx::query(
                    "INSERT INTO realm_package_digest (realm, actor_type, sha256) VALUES (?, ?, ?)
                     ON CONFLICT(realm, actor_type) DO UPDATE SET sha256 = excluded.sha256
                     WHERE realm_package_digest.sha256 = excluded.sha256",
                )
                .bind(name)
                .bind(&package.r#type)
                .bind(digest)
                .execute(&mut *tx)
                .await?
                .rows_affected();
                if count == 0 {
                    return Err(RealmError::Conflict(format!(
                        "{} is already published with different contents. Publish a new version.",
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

        let mut unguarded = second.clone();
        unguarded["packages"]["service"]["type"] = json!("acme.com/new-notes@1.0.0");
        sign_document(&mut unguarded, "rs-1", &key);
        let unguarded = unguarded.to_string();
        let switched = RealmProfile::verify(unguarded.as_bytes(), NAME, &keys)?;
        initial.check_successor(&switched)?;
        assert!(matches!(
            RealmProfileRecord::store(&switched, &unguarded, Some(&original.digest), None, None)
                .await,
            Err(RealmError::Conflict(message)) if message.contains("Use a package release")
        ));

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
    async fn service_switch_and_rollback_check_existing_actor_owners() -> anyhow::Result<()> {
        use super::super::acl::ActorAcl;
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
        let pool = get_database().get_pool();
        sqlx::query(
            "INSERT INTO realm_issuer_key
                (realm, key_id, signer_name, public_key, state, created_at)
             VALUES (?, 7, 'test-issuer', ?, 'current', 0)",
        )
        .bind(NAME)
        .bind(BASE64.encode([2u8; 32]))
        .execute(pool)
        .await?;
        sqlx::query(
            "INSERT INTO realm_member (realm, member_id, role, status, created_at)
             VALUES (?1, 'owner', 'owner', 'active', 0),
                    (?1, 'viewer', 'viewer', 'active', 0)",
        )
        .bind(NAME)
        .execute(pool)
        .await?;
        sqlx::query(
            "INSERT INTO realm_client_key (realm, key_id, public_key, member_id, registered_at)
             VALUES (?1, 'owner-key', zeroblob(32), 'owner', 0),
                    (?1, 'viewer-key', randomblob(32), 'viewer', 0)",
        )
        .bind(NAME)
        .execute(pool)
        .await?;

        let first = document(1, "active", &key);
        let initial = RealmProfile::verify(first.as_bytes(), NAME, &keys)?;
        RealmProfileRecord::store(&initial, &first, None, None, None).await?;
        let mut candidate: Value = serde_json::from_str(&first)?;
        candidate["packages"]["service"]["type"] = "acme.com/notes-next@1.0.0".into();
        candidate["packages"]["ui"]["type"] = "acme.com/ui@1.1.0".into();
        sqlx::query(
            "INSERT INTO issued_actor_identity
                (realm, serial_number, actor_type, member_id, client_key_id, issued_at)
             VALUES (?1, 10, 'acme.com/notes-next@1.0.0', 'owner', 'owner-key', 0),
                    (?1, 11, 'acme.com/notes-next-other@1.0.0', 'viewer', 'viewer-key', 0)",
        )
        .bind(NAME)
        .execute(pool)
        .await?;

        for (expected_sequence, serial, actor_type, mut target) in [
            (1, 12, "acme.com/notes-next@2.0.0", candidate),
            (2, 13, "acme.com/notes@1.0.0", serde_json::from_str(&first)?),
        ] {
            // The Viewer registered another version before the switch, or
            // the original type after it was no longer the selected Service.
            sqlx::query(
                "INSERT INTO issued_actor_identity
                    (realm, serial_number, actor_type, member_id, client_key_id, issued_at)
                 VALUES (?, ?, ?, 'viewer', 'viewer-key', 0)",
            )
            .bind(NAME)
            .bind(serial)
            .bind(actor_type)
            .execute(pool)
            .await?;
            if expected_sequence == 1 {
                let rules = [ActorAcl::new(
                    NAME.into(),
                    "acme.com/ui@1.1.0".into(),
                    actor_type.into(),
                    true,
                )];
                let mut tx = pool.begin().await?;
                let viewer = check_key_on(&mut tx, NAME, "viewer-key").await?;
                ActorAcl::replace_registration_policy(&mut tx, actor_type, &rules, &viewer).await?;
                tx.commit().await?;
            }

            target["sequence"] = json!(expected_sequence + 1);
            sign_document(&mut target, "rs-1", &key);
            let target = target.to_string();
            let profile = RealmProfile::verify(target.as_bytes(), NAME, &keys)?;
            let previous = RealmProfileRecord::get(NAME).await?.unwrap();
            let update = FrontUpdate {
                operation_id: format!("{serial:032x}"),
                expected_sequence,
                rollback: expected_sequence == 2,
            };
            let release = PackageRelease::Service(&update);
            assert!(matches!(
                RealmProfileRecord::store_package_release(
                    &profile, &target, Some(&previous.digest), None, release,
                ).await,
                Err(RealmError::Conflict(message)) if message.contains("belongs to a non-owner")
            ));
            assert_eq!(
                RealmProfileRecord::get(NAME).await?.unwrap().digest,
                previous.digest
            );

            sqlx::query(
                "INSERT INTO revoked_actor (realm, serial_number, revoked_at) VALUES (?, ?, 1)",
            )
            .bind(NAME)
            .bind(serial)
            .execute(pool)
            .await?;
            if expected_sequence == 1 {
                assert!(matches!(
                    RealmProfileRecord::store_package_release(
                        &profile, &target, Some(&previous.digest), None, release,
                    ).await,
                    Err(RealmError::Conflict(message))
                        if message.contains(actor_type) && message.contains("needs approval from the current Realm owner")
                ));
                assert_eq!(
                    RealmProfileRecord::get(NAME).await?.unwrap().digest,
                    previous.digest
                );
                let rules = [ActorAcl::new(
                    NAME.into(),
                    "acme.com/ui@1.1.0".into(),
                    actor_type.into(),
                    true,
                )];
                let mut tx = pool.begin().await?;
                let owner = check_key_on(&mut tx, NAME, "owner-key").await?;
                ActorAcl::replace_registration_policy(&mut tx, actor_type, &rules, &owner).await?;
                tx.commit().await?;
            }
            RealmProfileRecord::store_package_release(
                &profile,
                &target,
                Some(&previous.digest),
                None,
                release,
            )
            .await?;
        }

        let restored = RealmProfileRecord::get(NAME)
            .await?
            .unwrap()
            .verify(&keys)?;
        assert_eq!(restored.sequence, 3);
        assert_eq!(restored.selection, initial.selection);
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
