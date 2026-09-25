//! Actor access control list
//!
//! Defines data structures for actor permission control
use anyhow::Result;

use serde::{Deserialize, Serialize};

use super::super::RealmError;
use crate::storage::db::get_database;

const ANONYMOUS_ACTOR_TYPE: &str = "ANONCLNT";
const VOICE_ACTOR_TYPE: &str = "VOICE";
const CHAT_ACTOR_TYPE: &str = "CHAT";
const MOCK_REALM: &str = "mock@example.com";

/// Actor access control list
///
/// Manages access permissions between different actor types
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ActorAcl {
    pub rowid: Option<i64>,
    /// Target (accessed party) Realm name
    pub realm: String,
    /// Source (accessing party) Realm name; None means any Realm
    pub source_realm: Option<String>,
    pub from_type: String,
    pub to_type: String,
    pub access: bool,
}

impl<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> for ActorAcl {
    fn from_row(row: &'r sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        Ok(Self {
            rowid: row.try_get("rowid")?,
            realm: row.try_get("realm")?,
            source_realm: row.try_get("source_realm")?,
            from_type: row.try_get("from_type")?,
            to_type: row.try_get("to_type")?,
            access: row.try_get::<i64, _>("access")? != 0,
        })
    }
}

impl ActorAcl {
    /// Create a new access control rule
    pub fn new(realm: String, from_type: String, to_type: String, access: bool) -> Self {
        // When no source realm is explicitly specified, defaults to same-realm only
        let source_realm = Some(realm.clone());
        Self::new_with_source_realm(realm, source_realm, from_type, to_type, access)
    }

    /// Create an access control rule with a source realm constraint
    pub fn new_with_source_realm(
        realm: String,
        source_realm: Option<String>,
        from_type: String,
        to_type: String,
        access: bool,
    ) -> Self {
        Self {
            rowid: None,
            realm,
            source_realm,
            from_type,
            to_type,
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
                 SET realm = ?, source_realm = ?, from_type = ?, to_type = ?, access = ?
                 WHERE rowid = ?",
            )
            .bind(&self.realm)
            .bind(&self.source_realm)
            .bind(&self.from_type)
            .bind(&self.to_type)
            .bind(if self.access { 1 } else { 0 })
            .bind(rowid)
            .execute(pool)
            .await?;

            Ok(rowid)
        } else {
            // A policy coordinate has exactly one effective rule. Repeated
            // registration replaces it instead of creating row-order semantics.
            let new_rowid: i64 = sqlx::query_scalar(
                "INSERT INTO actoracl (realm, source_realm, from_type, to_type, access)
                 VALUES (?, ?, ?, ?, ?)
                 ON CONFLICT DO UPDATE SET access = excluded.access
                 RETURNING rowid",
            )
            .bind(&self.realm)
            .bind(&self.source_realm)
            .bind(&self.from_type)
            .bind(&self.to_type)
            .bind(if self.access { 1 } else { 0 })
            .fetch_one(pool)
            .await?;

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
            "SELECT rowid, realm, source_realm, from_type, to_type, access
             FROM actoracl
             WHERE rowid = ?",
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;

        Ok(result)
    }

    /// Get all access control rules for the specified Realm
    pub async fn get_by_realm(realm: &str) -> Result<Vec<Self>, RealmError> {
        let db = get_database();
        let pool = db.get_pool();

        let acls = sqlx::query_as::<_, ActorAcl>(
            "SELECT rowid, realm, source_realm, from_type, to_type, access
             FROM actoracl
             WHERE realm = ?",
        )
        .bind(realm)
        .fetch_all(pool)
        .await?;

        Ok(acls)
    }

    /// Get access control rules by type
    pub async fn get_by_types(
        target_realm: &str,
        source_realm: &str,
        from_type: &str,
        to_type: &str,
    ) -> Result<Option<Self>, RealmError> {
        let db = get_database();
        let pool = db.get_pool();

        let result = sqlx::query_as::<_, ActorAcl>(
            "SELECT rowid, realm, source_realm, from_type, to_type, access
             FROM actoracl
             WHERE realm = ?
               AND (source_realm = ? OR source_realm IS NULL)
               AND from_type = ?
               AND to_type = ?
             ORDER BY CASE WHEN source_realm = ? THEN 0 ELSE 1 END
             LIMIT 1",
        )
        .bind(target_realm)
        .bind(source_realm)
        .bind(from_type)
        .bind(to_type)
        .bind(source_realm)
        .fetch_optional(pool)
        .await?;

        Ok(result)
    }

    /// Atomically replace every ACL rule owned by one target actor type.
    pub async fn replace_for_target(
        realm: &str,
        to_type: &str,
        rules: &[Self],
    ) -> Result<(), RealmError> {
        let pool = get_database().get_pool();
        let mut transaction = pool.begin().await?;
        Self::replace_for_target_in_transaction(&mut transaction, realm, to_type, rules).await?;
        transaction.commit().await?;
        Ok(())
    }

    /// Replace a target type's ACL inside a caller-owned SQLite transaction.
    ///
    /// This lets registration commit the ACL policy and pending signaling
    /// metadata as one all-or-nothing state change.
    pub async fn replace_for_target_in_transaction(
        transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        realm: &str,
        to_type: &str,
        rules: &[Self],
    ) -> Result<(), RealmError> {
        if rules
            .iter()
            .any(|rule| rule.realm != realm || rule.to_type != to_type)
        {
            return Err(RealmError::ValidationError(
                "ACL replacement contains a rule outside its target scope".to_string(),
            ));
        }

        sqlx::query("DELETE FROM actoracl WHERE realm = ? AND to_type = ?")
            .bind(realm)
            .bind(to_type)
            .execute(&mut **transaction)
            .await?;

        for rule in rules {
            sqlx::query(
                "INSERT INTO actoracl
                    (realm, source_realm, from_type, to_type, access)
                 VALUES (?, ?, ?, ?, ?)
                 ON CONFLICT DO UPDATE SET access = excluded.access",
            )
            .bind(&rule.realm)
            .bind(&rule.source_realm)
            .bind(&rule.from_type)
            .bind(&rule.to_type)
            .bind(if rule.access { 1 } else { 0 })
            .execute(&mut **transaction)
            .await?;
        }

        Ok(())
    }

    /// Delete every policy rule owned by one target actor type.
    pub async fn delete_by_target(realm: &str, to_type: &str) -> Result<u64, RealmError> {
        let db = get_database();
        let result = sqlx::query("DELETE FROM actoracl WHERE realm = ? AND to_type = ?")
            .bind(realm)
            .bind(to_type)
            .execute(db.get_pool())
            .await?;
        Ok(result.rows_affected())
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
    /// - `source_realm`: Source actor's Realm name
    /// - `target_realm`: Target actor's Realm name
    /// - `from_type`: Source actor type
    /// - `to_type`: Target actor type
    ///
    /// # Returns
    ///
    /// Returns true if discovery is allowed, false otherwise.
    /// Actors in different Realms never see each other, whatever the rules
    /// say. Within a Realm the default is deny when no rule exists.
    pub async fn can_discover(
        source_realm: &str,
        target_realm: &str,
        from_type: &str,
        to_type: &str,
    ) -> Result<bool, RealmError> {
        if source_realm != target_realm {
            crate::recording::debug!(
                "Cross-Realm discovery denied: source_realm={}, target_realm={}",
                source_realm,
                target_realm
            );
            return Ok(false);
        }
        match Self::get_by_types(target_realm, source_realm, from_type, to_type).await? {
            Some(acl) => {
                crate::recording::debug!(
                    "ACL rule found: source_realm={}, target_realm={}, from_type={}, to_type={}, access={}",
                    source_realm,
                    target_realm,
                    from_type,
                    to_type,
                    acl.access()
                );
                Ok(acl.access())
            }
            None => {
                // Default policy: deny if no rule exists
                crate::recording::debug!(
                    "No ACL rule found, denying discovery (default policy): source_realm={}, target_realm={}, from_type={}, to_type={}",
                    source_realm,
                    target_realm,
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
            realm: MOCK_REALM.to_string(),
            source_realm: Some(MOCK_REALM.to_string()),
            from_type: ANONYMOUS_ACTOR_TYPE.to_string(),
            to_type: VOICE_ACTOR_TYPE.to_string(),
            access: true,
        },
        ActorAcl {
            rowid: None,
            realm: MOCK_REALM.to_string(),
            source_realm: Some(MOCK_REALM.to_string()),
            from_type: ANONYMOUS_ACTOR_TYPE.to_string(),
            to_type: CHAT_ACTOR_TYPE.to_string(),
            access: true,
        },
        ActorAcl {
            rowid: None,
            realm: MOCK_REALM.to_string(),
            source_realm: Some(MOCK_REALM.to_string()),
            from_type: ANONYMOUS_ACTOR_TYPE.to_string(),
            to_type: ANONYMOUS_ACTOR_TYPE.to_string(),
            access: false,
        },
        ActorAcl {
            rowid: None,
            realm: MOCK_REALM.to_string(),
            source_realm: Some(MOCK_REALM.to_string()),
            from_type: CHAT_ACTOR_TYPE.to_string(),
            to_type: ANONYMOUS_ACTOR_TYPE.to_string(),
            access: true,
        },
        ActorAcl {
            rowid: None,
            realm: MOCK_REALM.to_string(),
            source_realm: Some(MOCK_REALM.to_string()),
            from_type: CHAT_ACTOR_TYPE.to_string(),
            to_type: VOICE_ACTOR_TYPE.to_string(),
            access: true,
        },
        ActorAcl {
            rowid: None,
            realm: MOCK_REALM.to_string(),
            source_realm: Some(MOCK_REALM.to_string()),
            from_type: CHAT_ACTOR_TYPE.to_string(),
            to_type: CHAT_ACTOR_TYPE.to_string(),
            access: true,
        },
        ActorAcl {
            rowid: None,
            realm: MOCK_REALM.to_string(),
            source_realm: Some(MOCK_REALM.to_string()),
            from_type: VOICE_ACTOR_TYPE.to_string(),
            to_type: ANONYMOUS_ACTOR_TYPE.to_string(),
            access: true,
        },
        ActorAcl {
            rowid: None,
            realm: MOCK_REALM.to_string(),
            source_realm: Some(MOCK_REALM.to_string()),
            from_type: VOICE_ACTOR_TYPE.to_string(),
            to_type: CHAT_ACTOR_TYPE.to_string(),
            access: true,
        },
        ActorAcl {
            rowid: None,
            realm: MOCK_REALM.to_string(),
            source_realm: Some(MOCK_REALM.to_string()),
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
        let realm = Realm::create("acl-test@example.com".to_string()).await?;
        let realm = realm.name;

        // Test create
        let mut acl = ActorAcl::new(
            realm.clone(),
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
        assert_eq!(fetched.realm, realm);
        assert_eq!(fetched.source_realm.as_deref(), Some(realm.as_str()));
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
        let acls_for_realm = ActorAcl::get_by_realm(&realm).await?;
        assert_eq!(acls_for_realm.len(), 1);
        assert_eq!(acls_for_realm[0].rowid, Some(acl_id));

        // Test get_by_types
        let acl_by_types_opt = ActorAcl::get_by_types(
            &realm,
            &realm,
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
    async fn test_source_scoped_and_wildcard_acl_precedence() -> anyhow::Result<()> {
        setup_test_db().await?;

        let target = Realm::create("acl-target@example.com".to_string()).await?;
        let target_realm = target.name.as_str();

        let wildcard = ActorAcl::new_with_source_realm(
            target_realm.to_string(),
            None,
            "acme:client".to_string(),
            "acme:worker".to_string(),
            false,
        );
        let exact = ActorAcl::new_with_source_realm(
            target_realm.to_string(),
            Some(target_realm.to_string()),
            "acme:client".to_string(),
            "acme:worker".to_string(),
            true,
        );
        ActorAcl::replace_for_target(target_realm, "acme:worker", &[wildcard, exact]).await?;

        let allowed =
            ActorAcl::can_discover(target_realm, target_realm, "acme:client", "acme:worker")
                .await?;
        assert!(allowed, "the source-scoped rule wins over the wildcard");

        let denied_other_type =
            ActorAcl::can_discover(target_realm, target_realm, "acme:other", "acme:worker").await?;
        assert!(!denied_other_type);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_cross_realm_is_denied_even_with_a_rule() -> anyhow::Result<()> {
        setup_test_db().await?;

        let target_realm = "acl-target@example.com";
        let source_realm = "acl-source@acme.com";
        for name in [target_realm, source_realm] {
            Realm::create(name.to_string()).await?;
        }
        for source in [Some(source_realm.to_string()), None] {
            let rule = ActorAcl::new_with_source_realm(
                target_realm.to_string(),
                source,
                "acme:client".to_string(),
                "acme:worker".to_string(),
                true,
            );
            ActorAcl::replace_for_target(target_realm, "acme:worker", &[rule]).await?;
            let allowed =
                ActorAcl::can_discover(source_realm, target_realm, "acme:client", "acme:worker")
                    .await?;
            assert!(!allowed, "cross-Realm discovery must be denied");
        }

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_replace_for_target_replaces_only_the_selected_type() -> anyhow::Result<()> {
        setup_test_db().await?;

        let realm = Realm::create("acl-replace@example.com".to_string()).await?;
        let realm = realm.name;
        let target = "acme:worker";
        let untouched_target = "acme:other";

        let mut old = ActorAcl::new(
            realm.clone(),
            "acme:old-client".to_string(),
            target.to_string(),
            true,
        );
        old.save().await?;
        let mut untouched = ActorAcl::new(
            realm.clone(),
            "acme:client".to_string(),
            untouched_target.to_string(),
            true,
        );
        untouched.save().await?;

        let replacements = vec![
            ActorAcl::new(
                realm.clone(),
                "acme:new-client".to_string(),
                target.to_string(),
                true,
            ),
            ActorAcl::new_with_source_realm(
                realm.clone(),
                None,
                "acme:federated-client".to_string(),
                target.to_string(),
                false,
            ),
        ];
        ActorAcl::replace_for_target(&realm, target, &replacements).await?;

        let rules = ActorAcl::get_by_realm(&realm).await?;
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

    #[tokio::test]
    #[serial]
    async fn test_replace_for_target_in_transaction_rolls_back_with_caller() -> anyhow::Result<()> {
        setup_test_db().await?;

        let realm = Realm::create("acl-rollback@example.com".to_string()).await?;
        let realm = realm.name;
        let target = "acme:worker";
        let mut old = ActorAcl::new(
            realm.clone(),
            "acme:old-client".to_string(),
            target.to_string(),
            true,
        );
        old.save().await?;

        let replacement = [ActorAcl::new(
            realm.clone(),
            "acme:new-client".to_string(),
            target.to_string(),
            true,
        )];
        let mut transaction = get_database().get_pool().begin().await?;
        ActorAcl::replace_for_target_in_transaction(&mut transaction, &realm, target, &replacement)
            .await?;
        transaction.rollback().await?;

        let rules = ActorAcl::get_by_realm(&realm).await?;
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].from_type, "acme:old-client");

        Ok(())
    }
}
