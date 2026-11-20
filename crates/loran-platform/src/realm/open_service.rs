//! Realm management grants for package-backed Services using Open Join.

use super::{RealmError, changes};
use crate::storage::db::get_database;
use actr_protocol::{ActrId, ActrType, RegisterAuthMode};
use chrono::Utc;

/// Check the grant for the authenticated Realm client and exact selected type.
pub async fn allowed<'e>(
    db: impl sqlx::SqliteExecutor<'e>,
    realm: &str,
    key_id: &str,
    actor_type: &ActrType,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM realm_open_service g
            JOIN realm r ON r.name = g.realm
            JOIN realm_admission a ON a.realm = r.name
            JOIN realm_client_key k ON k.realm = g.realm AND k.public_key = g.public_key
            JOIN realm_profile p ON p.realm = r.name
            WHERE g.realm = ?1 AND k.key_id = ?2 AND g.actor_type = ?3
                AND r.status = 'active' AND r.enabled = 1
                AND (r.expires_at IS NULL OR r.expires_at > ?4)
                AND k.revoked_at IS NULL AND json_extract(a.policy, '$.membership') = 'open'
                AND EXISTS (
                    SELECT 1 FROM json_each(p.document, '$.packages') package
                    WHERE package.key != json_extract(p.document, '$.front_package')
                      AND json_extract(package.value, '$.type') = g.actor_type
                )
        )",
    )
    .bind(realm)
    .bind(key_id)
    .bind(actor_type.to_string_repr())
    .bind(Utc::now().timestamp())
    .fetch_one(db)
    .await
}

/// Change a grant on behalf of the verified Realm management signing key.
/// Revocation disconnects only Actors registered using this grant.
pub async fn set_authorization(
    realm: &str,
    actor_type: &ActrType,
    public_key: &[u8; 32],
    enabled: bool,
) -> Result<(), RealmError> {
    let mut tx = get_database()
        .get_pool()
        .begin_with("BEGIN IMMEDIATE")
        .await?;
    let now = Utc::now().timestamp();
    let actor_type = actor_type.to_string_repr();
    if enabled {
        let valid: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1 FROM realm r
                JOIN realm_admission a ON a.realm = r.name
                JOIN realm_profile p ON p.realm = r.name
                WHERE r.name = ?1 AND r.status = 'active' AND r.enabled = 1
                    AND (r.expires_at IS NULL OR r.expires_at > ?3)
                    AND json_extract(a.policy, '$.membership') = 'open'
                    AND EXISTS (
                        SELECT 1 FROM json_each(p.document, '$.packages') package
                        WHERE package.key != json_extract(p.document, '$.front_package')
                          AND json_extract(package.value, '$.type') = ?2
                    )
            )",
        )
        .bind(realm)
        .bind(&actor_type)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
        if !valid {
            return Err(RealmError::ValidationError(
                "Service type is not selected by an active open Realm".to_owned(),
            ));
        }
        sqlx::query(
            "INSERT OR IGNORE INTO realm_open_service (realm, actor_type, public_key)
             VALUES (?, ?, ?)",
        )
        .bind(realm)
        .bind(&actor_type)
        .bind(public_key.as_slice())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Ok(());
    }

    sqlx::query(
        "DELETE FROM realm_open_service WHERE realm = ? AND actor_type = ? AND public_key = ?",
    )
    .bind(realm)
    .bind(&actor_type)
    .bind(public_key.as_slice())
    .execute(&mut *tx)
    .await?;
    let serials: Vec<i64> = sqlx::query_scalar(
        "INSERT OR IGNORE INTO revoked_actor (realm, serial_number, revoked_at, reason)
         SELECT i.realm, i.serial_number, ?4, 'open Service authorization revoked'
         FROM issued_actor_identity i
         JOIN realm_client_key k ON k.realm = i.realm AND k.key_id = i.client_key_id
         WHERE i.realm = ?1 AND i.actor_type = ?2 AND k.public_key = ?3 AND i.auth_mode = ?5
         RETURNING serial_number",
    )
    .bind(realm)
    .bind(&actor_type)
    .bind(public_key.as_slice())
    .bind(now)
    .bind(RegisterAuthMode::OpenService as i32)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    for serial_number in serials {
        changes::announce_actor(&ActrId {
            realm: realm.to_owned(),
            serial_number: serial_number as u64,
        });
    }
    Ok(())
}
