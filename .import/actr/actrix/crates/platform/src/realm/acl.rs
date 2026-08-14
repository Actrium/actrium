//! Actor access control list
//!
//! Defines the data structures for actor permission control.
use anyhow::Result;

use serde::{Deserialize, Serialize};

use super::super::RealmError;
use crate::storage::db::get_database;

const ANONYMOUS_ACTOR_TYPE: &str = "ANONCLNT";
const VOICE_ACTOR_TYPE: &str = "VOICE";
const CHAT_ACTOR_TYPE: &str = "CHAT";

/// Actor access control list
///
/// Manages access permissions between different actor types.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ActorAcl {
    pub rowid: Option<i64>,
    /// Target (accessed party) realm
    pub realm_id: u32,
    /// Source (accessing party) realm; None means no source scope set
    pub source_realm_id: Option<u32>,
    pub from_type: String,
    pub to_type: String,
    pub access: bool,
}

impl<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> for ActorAcl {
    fn from_row(row: &'r sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        let source_realm_id = row
            .try_get::<Option<i64>, _>("source_realm_id")?
            .and_then(|v| u32::try_from(v).ok());
        Ok(Self {
            rowid: row.try_get("rowid")?,
            realm_id: row.try_get::<i64, _>("realm_id")?.try_into().unwrap(),
            source_realm_id,
            from_type: row.try_get("from_type")?,
            to_type: row.try_get("to_type")?,
            access: row.try_get::<i64, _>("access")? != 0,
        })
    }
}

impl ActorAcl {
    /// Create a new access control rule
    pub fn new(realm_id: u32, from_type: String, to_type: String, access: bool) -> Self {
        // Legacy semantics: when no source realm is explicitly specified, default to same-realm only
        Self::new_with_source_realm(realm_id, Some(realm_id), from_type, to_type, access)
    }

    /// Create an access control rule with a source realm constraint
    pub fn new_with_source_realm(
        realm_id: u32,
        source_realm_id: Option<u32>,
        from_type: String,
        to_type: String,
        access: bool,
    ) -> Self {
        Self {
            rowid: None,
            realm_id,
            source_realm_id,
            from_type: from_type.to_string(),
            to_type: to_type.to_string(),
            access,
        }
    }

    /// Save ACL rule to database
    ///
    /// Inserts a new rule or updates existing one based on rowid.
    ///
    /// # Returns
    ///
    /// Returns the rowid of the saved ACL rule
    pub async fn save(&mut self) -> Result<i64, RealmError> {
        let db = get_database();
        let pool = db.get_pool();

        if let Some(rowid) = self.rowid {
            // Update existing record
            sqlx::query(
                "UPDATE actoracl
                 SET realm_id = ?, source_realm_id = ?, from_type = ?, to_type = ?, access = ?
                 WHERE rowid = ?",
            )
            .bind(self.realm_id)
            .bind(self.source_realm_id.map(i64::from))
            .bind(&self.from_type)
            .bind(&self.to_type)
            .bind(if self.access { 1 } else { 0 })
            .bind(rowid)
            .execute(pool)
            .await?;

            Ok(rowid)
        } else {
            // Insert new record
            let result = sqlx::query(
                "INSERT INTO actoracl (realm_id, source_realm_id, from_type, to_type, access)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(self.realm_id)
            .bind(self.source_realm_id.map(i64::from))
            .bind(&self.from_type)
            .bind(&self.to_type)
            .bind(if self.access { 1 } else { 0 })
            .execute(pool)
            .await?;

            let new_rowid = result.last_insert_rowid();
            self.rowid = Some(new_rowid);
            Ok(new_rowid)
        }
    }

    /// Delete ACL rule by ID
    ///
    /// # Arguments
    ///
    /// - `id`: ACL rule rowid
    ///
    /// # Returns
    ///
    /// Returns number of deleted rows (0 or 1)
    pub async fn delete_by_id(id: i64) -> Result<u64, RealmError> {
        let db = get_database();
        let pool = db.get_pool();

        let result = sqlx::query("DELETE FROM actoracl WHERE rowid = ?")
            .bind(id)
            .execute(pool)
            .await?;

        let changes = result.rows_affected();
        if changes > 0 {
            Ok(changes)
        } else {
            Err(RealmError::NotFound)
        }
    }

    /// Get ACL rule by ID
    ///
    /// # Arguments
    ///
    /// - `id`: ACL rule rowid
    ///
    /// # Returns
    ///
    /// Returns the ACL rule if found, None otherwise
    pub async fn get(id: i64) -> Result<Option<Self>, RealmError> {
        let db = get_database();
        let pool = db.get_pool();

        let result = sqlx::query_as::<_, ActorAcl>(
            "SELECT rowid, realm_id, source_realm_id, from_type, to_type, access
             FROM actoracl
             WHERE rowid = ?",
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;

        Ok(result)
    }

    /// Get all access control rules for the specified realm
    pub async fn get_by_realm(realm_id: u32) -> Result<Vec<Self>, RealmError> {
        let db = get_database();
        let pool = db.get_pool();

        let acls = sqlx::query_as::<_, ActorAcl>(
            "SELECT rowid, realm_id, source_realm_id, from_type, to_type, access
             FROM actoracl
             WHERE realm_id = ?",
        )
        .bind(realm_id)
        .fetch_all(pool)
        .await?;

        Ok(acls)
    }

    /// Get access control rule by type
    pub async fn get_by_types(
        target_realm_id: u32,
        source_realm_id: u32,
        from_type: &str,
        to_type: &str,
    ) -> Result<Option<Self>, RealmError> {
        let db = get_database();
        let pool = db.get_pool();

        let result = sqlx::query_as::<_, ActorAcl>(
            "SELECT rowid, realm_id, source_realm_id, from_type, to_type, access
             FROM actoracl
             WHERE realm_id = ? AND source_realm_id = ? AND from_type = ? AND to_type = ?
             ORDER BY rowid DESC
             LIMIT 1",
        )
        .bind(target_realm_id)
        .bind(source_realm_id)
        .bind(from_type)
        .bind(to_type)
        .fetch_optional(pool)
        .await?;

        Ok(result)
    }

    /// Delete all ACL rules for the specified target type (used when re-registering a service to replace old ACLs)
    pub async fn delete_by_target(realm_id: u32, to_type: &str) -> Result<u64, RealmError> {
        let db = get_database();
        let pool = db.get_pool();

        let result = sqlx::query("DELETE FROM actoracl WHERE realm_id = ? AND to_type = ?")
            .bind(realm_id)
            .bind(to_type)
            .execute(pool)
            .await?;

        Ok(result.rows_affected())
    }

    /// Atomically replace every ACL rule for one realm + target actor type.
    ///
    /// Registration ACLs are type-level policy. Deleting the old policy and
    /// inserting its replacement in separate statements can expose an empty or
    /// partially-written policy to concurrent readers; this transaction makes
    /// the replacement all-or-nothing.
    pub async fn replace_for_target(
        realm_id: u32,
        to_type: &str,
        rules: &[Self],
    ) -> Result<(), RealmError> {
        let pool = get_database().get_pool();
        let mut tx = pool.begin().await?;
        Self::replace_for_target_in_transaction(&mut tx, realm_id, to_type, rules).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Replace a target type's ACL inside a caller-owned SQLite transaction.
    ///
    /// This lets registration commit the renewal-token binding, ACL policy,
    /// and pending signaling metadata as one all-or-nothing state change.
    pub async fn replace_for_target_in_transaction(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        realm_id: u32,
        to_type: &str,
        rules: &[Self],
    ) -> Result<(), RealmError> {
        if rules
            .iter()
            .any(|rule| rule.realm_id != realm_id || rule.to_type != to_type)
        {
            return Err(RealmError::ValidationError(
                "replacement ACL contains a different realm or target type".to_string(),
            ));
        }

        sqlx::query("DELETE FROM actoracl WHERE realm_id = ?1 AND to_type = ?2")
            .bind(realm_id)
            .bind(to_type)
            .execute(&mut **tx)
            .await?;

        for rule in rules {
            sqlx::query(
                "INSERT INTO actoracl
                    (realm_id, source_realm_id, from_type, to_type, access)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .bind(rule.realm_id)
            .bind(rule.source_realm_id.map(i64::from))
            .bind(&rule.from_type)
            .bind(&rule.to_type)
            .bind(if rule.access { 1 } else { 0 })
            .execute(&mut **tx)
            .await?;
        }

        Ok(())
    }

    pub fn access(&self) -> bool {
        self.access
    }

    /// Check if discovery is allowed between two actor types
    ///
    /// Used for Presence notification filtering and service discovery
    ///
    /// # Arguments
    ///
    /// - `source_realm_id`: Source actor's realm
    /// - `target_realm_id`: Target actor's realm
    /// - `from_type`: Source actor type
    /// - `to_type`: Target actor type
    ///
    /// # Returns
    ///
    /// Returns true if discovery is allowed, false otherwise.
    /// Default policy: deny if no rule exists (secure by default)
    pub async fn can_discover(
        source_realm_id: u32,
        target_realm_id: u32,
        from_type: &str,
        to_type: &str,
    ) -> Result<bool, RealmError> {
        match Self::get_by_types(target_realm_id, source_realm_id, from_type, to_type).await? {
            Some(acl) => {
                crate::recording::debug!(
                    "ACL rule found: source_realm_id={}, target_realm_id={}, from_type={}, to_type={}, access={}",
                    source_realm_id,
                    target_realm_id,
                    from_type,
                    to_type,
                    acl.access()
                );
                Ok(acl.access())
            }
            None => {
                // Default policy: deny if no rule exists
                crate::recording::debug!(
                    "No ACL rule found, denying discovery (default policy): source_realm_id={}, target_realm_id={}, from_type={}, to_type={}",
                    source_realm_id,
                    target_realm_id,
                    from_type,
                    to_type
                );
                Ok(false)
            }
        }
    }
}

pub fn mock_actor_acl() -> Vec<ActorAcl> {
    vec![
        ActorAcl {
            rowid: None,
            realm_id: 2,
            source_realm_id: Some(2),
            from_type: ANONYMOUS_ACTOR_TYPE.to_string(),
            to_type: VOICE_ACTOR_TYPE.to_string(),
            access: true,
        },
        ActorAcl {
            rowid: None,
            realm_id: 2,
            source_realm_id: Some(2),
            from_type: ANONYMOUS_ACTOR_TYPE.to_string(),
            to_type: CHAT_ACTOR_TYPE.to_string(),
            access: true,
        },
        ActorAcl {
            rowid: None,
            realm_id: 2,
            source_realm_id: Some(2),
            from_type: ANONYMOUS_ACTOR_TYPE.to_string(),
            to_type: ANONYMOUS_ACTOR_TYPE.to_string(),
            access: false,
        },
        ActorAcl {
            rowid: None,
            realm_id: 2,
            source_realm_id: Some(2),
            from_type: CHAT_ACTOR_TYPE.to_string(),
            to_type: ANONYMOUS_ACTOR_TYPE.to_string(),
            access: true,
        },
        ActorAcl {
            rowid: None,
            realm_id: 2,
            source_realm_id: Some(2),
            from_type: CHAT_ACTOR_TYPE.to_string(),
            to_type: VOICE_ACTOR_TYPE.to_string(),
            access: true,
        },
        ActorAcl {
            rowid: None,
            realm_id: 2,
            source_realm_id: Some(2),
            from_type: CHAT_ACTOR_TYPE.to_string(),
            to_type: CHAT_ACTOR_TYPE.to_string(),
            access: true,
        },
        ActorAcl {
            rowid: None,
            realm_id: 2,
            source_realm_id: Some(2),
            from_type: VOICE_ACTOR_TYPE.to_string(),
            to_type: ANONYMOUS_ACTOR_TYPE.to_string(),
            access: true,
        },
        ActorAcl {
            rowid: None,
            realm_id: 2,
            source_realm_id: Some(2),
            from_type: VOICE_ACTOR_TYPE.to_string(),
            to_type: CHAT_ACTOR_TYPE.to_string(),
            access: true,
        },
        ActorAcl {
            rowid: None,
            realm_id: 2,
            source_realm_id: Some(2),
            from_type: VOICE_ACTOR_TYPE.to_string(),
            to_type: VOICE_ACTOR_TYPE.to_string(),
            access: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::realm::Realm;
    use crate::util::test_utils::utils::setup_test_db;
    use serial_test::serial;

    #[tokio::test]
    #[serial]
    async fn test_actor_acl_crud() -> anyhow::Result<()> {
        setup_test_db().await?;

        // Create a realm first
        let realm = Realm::create("acl_test_realm".to_string(), String::new()).await?;
        let realm_id = realm.id;

        // Test create
        let mut acl = ActorAcl::new(
            realm_id,
            "identified_client_user".to_string(),
            "identified_client_room".to_string(),
            true,
        );
        let acl_id = acl.save().await?;
        assert!(acl.rowid.is_some());
        assert_eq!(acl.rowid.unwrap(), acl_id);

        // Test get
        let fetched_opt = ActorAcl::get(acl_id).await?;
        assert!(fetched_opt.is_some());
        let fetched = fetched_opt.unwrap();
        assert_eq!(fetched.realm_id, realm_id);
        assert_eq!(fetched.source_realm_id, Some(realm_id));
        assert_eq!(fetched.from_type, "identified_client_user");
        assert_eq!(fetched.to_type, "identified_client_room");
        assert!(fetched.access);

        // Test update
        acl.access = false;
        let updated_acl_id = acl.save().await?;
        assert_eq!(updated_acl_id, acl_id);

        let reloaded_opt = ActorAcl::get(acl_id).await?;
        assert!(reloaded_opt.is_some());
        let reloaded = reloaded_opt.unwrap();
        assert!(!reloaded.access);

        // Test get_by_realm
        let acls_for_realm = ActorAcl::get_by_realm(realm_id).await?;
        assert_eq!(acls_for_realm.len(), 1);
        assert_eq!(acls_for_realm[0].rowid, Some(acl_id));

        // Test get_by_types
        let acl_by_types_opt = ActorAcl::get_by_types(
            realm_id,
            realm_id,
            "identified_client_user",
            "identified_client_room",
        )
        .await?;
        assert!(acl_by_types_opt.is_some());
        let acl_by_types = acl_by_types_opt.unwrap();
        assert!(!acl_by_types.access);

        // Test delete
        ActorAcl::delete_by_id(acl_id).await?;
        let deleted_acl_opt = ActorAcl::get(acl_id).await?;
        assert!(deleted_acl_opt.is_none());

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_cross_realm_acl_allow_and_deny() -> anyhow::Result<()> {
        setup_test_db().await?;

        let source = Realm::create("acl_source_realm".to_string(), String::new()).await?;
        let target = Realm::create("acl_target_realm".to_string(), String::new()).await?;
        let source_realm = source.id;
        let target_realm = target.id;

        let mut acl = ActorAcl::new_with_source_realm(
            target_realm,
            Some(source_realm),
            "acme:client".to_string(),
            "acme:worker".to_string(),
            true,
        );
        let _ = acl.save().await?;

        let allowed =
            ActorAcl::can_discover(source_realm, target_realm, "acme:client", "acme:worker")
                .await?;
        assert!(allowed);

        let denied_same_realm =
            ActorAcl::can_discover(target_realm, target_realm, "acme:client", "acme:worker")
                .await?;
        assert!(!denied_same_realm);

        let denied_other_type =
            ActorAcl::can_discover(source_realm, target_realm, "acme:other", "acme:worker").await?;
        assert!(!denied_other_type);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_replace_for_target_replaces_only_the_selected_type() -> anyhow::Result<()> {
        setup_test_db().await?;

        let realm = Realm::create("acl_replace_realm".to_string(), String::new()).await?;
        let realm_id = realm.id;
        let target = "acme:worker";
        let untouched_target = "acme:other";

        let mut old = ActorAcl::new(
            realm_id,
            "acme:old-client".to_string(),
            target.to_string(),
            true,
        );
        old.save().await?;
        let mut untouched = ActorAcl::new(
            realm_id,
            "acme:client".to_string(),
            untouched_target.to_string(),
            true,
        );
        untouched.save().await?;

        let replacements = vec![
            ActorAcl::new(
                realm_id,
                "acme:new-client".to_string(),
                target.to_string(),
                true,
            ),
            ActorAcl::new_with_source_realm(
                realm_id,
                None,
                "acme:federated-client".to_string(),
                target.to_string(),
                false,
            ),
        ];
        ActorAcl::replace_for_target(realm_id, target, &replacements).await?;

        let rules = ActorAcl::get_by_realm(realm_id).await?;
        assert_eq!(
            rules.iter().filter(|rule| rule.to_type == target).count(),
            2
        );
        assert!(
            rules
                .iter()
                .all(|rule| rule.to_type != target || rule.from_type != "acme:old-client")
        );
        assert!(
            rules
                .iter()
                .any(|rule| rule.to_type == untouched_target && rule.from_type == "acme:client")
        );

        Ok(())
    }
}
