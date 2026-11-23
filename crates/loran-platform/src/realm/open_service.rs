//! Admission of package-backed Services in an Open Realm.

use actr_protocol::ActrType;
use chrono::Utc;

/// Check the current Realm policy and the exact Service type in its accepted Profile.
pub async fn allows_type<'e>(
    db: impl sqlx::SqliteExecutor<'e>,
    realm: &str,
    actor_type: &ActrType,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM realm r
            JOIN realm_admission a ON a.realm = r.name
            JOIN realm_profile p ON p.realm = r.name
            JOIN json_each(p.document, '$.packages') front
              ON front.key = json_extract(p.document, '$.front_package')
            WHERE r.name = ?1 AND r.status = 'active' AND r.enabled = 1
                AND (r.expires_at IS NULL OR r.expires_at > ?3)
                AND json_extract(a.policy, '$.membership') = 'open'
                AND json_extract(front.value, '$.type') != ?2
                AND EXISTS (
                    SELECT 1 FROM json_each(p.document, '$.packages') package
                    WHERE json_extract(package.value, '$.type') = ?2
                )
        )",
    )
    .bind(realm)
    .bind(actor_type.to_string_repr())
    .bind(Utc::now().timestamp())
    .fetch_one(db)
    .await
}
