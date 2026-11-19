//! AFP's local invitation delivery journal. A failed receipt write must roll
//! back the invitation, whose plaintext secret cannot be recovered from Issuer.
use super::{Error, Result, conflict, invalid, unavailable};
use actr_protocol::realm_membership::{
    MAX_INVITE_SECS, MembershipRequest, RealmInvite, Role, invite_id, invite_secret_hash,
};
use axum::http::StatusCode;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use platform::realm::{Realm, membership};
use serde_json::Value;
use signer::crypto::KeyEncryptor;
use sqlx::{SqliteConnection, SqlitePool};

pub(super) const RECEIPT_SECS: i64 = 120;

pub(super) async fn initialize(db: &SqlitePool) -> Result<()> {
    sqlx::raw_sql(
        "CREATE TABLE IF NOT EXISTS admin_operations (
            id TEXT PRIMARY KEY, provider TEXT NOT NULL, realm TEXT NOT NULL, key_id TEXT NOT NULL,
            fingerprint TEXT NOT NULL, expires INTEGER NOT NULL, receipt TEXT);
         CREATE INDEX IF NOT EXISTS admin_receipt_expiry
            ON admin_operations(expires) WHERE receipt IS NOT NULL;",
    )
    .execute(db)
    .await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
pub(super) struct StoredOperation {
    pub(super) provider: String,
    pub(super) realm: String,
    pub(super) key_id: String,
    pub(super) fingerprint: String,
    pub(super) expires: i64,
    pub(super) receipt: Option<String>,
}

impl StoredOperation {
    pub(super) fn belongs_to(&self, provider: &str, realm: &str, key_id: &str) -> bool {
        self.provider == provider && self.realm == realm && self.key_id == key_id
    }
}

pub(super) async fn load(db: &SqlitePool, id: &str) -> Result<Option<StoredOperation>> {
    Ok(sqlx::query_as(
        "SELECT provider,realm,key_id,fingerprint,expires,receipt FROM admin_operations WHERE id=?",
    )
    .bind(id)
    .fetch_optional(db)
    .await?)
}

pub(super) struct Operation<'a> {
    pub(super) id: &'a str,
    pub(super) provider: &'a str,
    pub(super) realm: &'a str,
    pub(super) key_id: &'a str,
    pub(super) fingerprint: &'a str,
}

impl Operation<'_> {
    pub(super) fn require_same(&self, stored: &StoredOperation) -> Result<()> {
        if !stored.belongs_to(self.provider, self.realm, self.key_id)
            || stored.fingerprint != self.fingerprint
        {
            return Err(conflict(
                "Operation ID already used for a different request",
            ));
        }
        Ok(())
    }

    pub(super) async fn reserve(&self, conn: &mut SqliteConnection, now: i64) -> Result<bool> {
        Ok(sqlx::query(
            "INSERT OR IGNORE INTO admin_operations(id,provider,realm,key_id,fingerprint,expires)
             VALUES(?,?,?,?,?,?)",
        )
        .bind(self.id)
        .bind(self.provider)
        .bind(self.realm)
        .bind(self.key_id)
        .bind(self.fingerprint)
        .bind(now + RECEIPT_SECS)
        .execute(conn)
        .await?
        .rows_affected()
            == 1)
    }
}

pub(super) async fn store_receipt(
    conn: &mut SqliteConnection,
    delivery: &KeyEncryptor,
    id: &str,
    result: &Value,
) -> Result<()> {
    let encrypted = delivery
        .encrypt(&result.to_string())
        .map_err(|_| unavailable())?;
    let stored = sqlx::query("UPDATE admin_operations SET receipt=? WHERE id=?")
        .bind(encrypted)
        .bind(id)
        .execute(conn)
        .await?;
    if stored.rows_affected() != 1 {
        return Err(unavailable());
    }
    Ok(())
}

/// The AFP has verified its own registered device's signature before calling
/// this. Recheck the key, role, Realm and quota under the same SQLite write
/// lock as the invitation and its encrypted delivery receipt.
pub(super) async fn issue(
    db: &SqlitePool,
    delivery: &KeyEncryptor,
    operation: &Operation<'_>,
    request: &MembershipRequest,
    limits: membership::Limits,
) -> Result<Option<Value>> {
    let mut tx = db.begin_with("BEGIN IMMEDIATE").await?;
    let now = Utc::now().timestamp();
    if !operation.reserve(&mut tx, now).await? {
        return Ok(None);
    }
    let signer = membership::check_key_on(&mut tx, operation.realm, operation.key_id).await?;
    membership::require_owner(&signer)?;
    if !Realm::get_on(&mut tx, operation.realm)
        .await
        .map_err(|_| unavailable())?
        .is_some_and(|realm| realm.is_active())
    {
        return Err(conflict("The App is not active"));
    }
    match (request.role, request.member_id.as_deref()) {
        (Some(Role::Editor | Role::Viewer), None) => {}
        (None, Some(member_id)) if member_id == signer.member_id => {}
        _ => {
            return Err(invalid(
                "Choose an Editor, Viewer or the current Owner's device",
            ));
        }
    }
    let expires_at = request
        .invite_expires_at
        .as_deref()
        .and_then(|time| DateTime::parse_from_rfc3339(time).ok())
        .map(|time| time.timestamp())
        .filter(|&time| now < time && time <= now + MAX_INVITE_SECS)
        .ok_or_else(|| invalid("The invitation must expire within 30 days"))?;
    let open: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM realm_invite
         WHERE realm=? AND used_at IS NULL AND revoked_at IS NULL AND expires_at>?",
    )
    .bind(operation.realm)
    .bind(now)
    .fetch_one(&mut *tx)
    .await?;
    if open >= i64::from(limits.max_open_invites) {
        return Err(Error(
            StatusCode::TOO_MANY_REQUESTS,
            "The Realm has the most open invites allowed".into(),
        ));
    }
    let secret = URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>());
    let invite_id = invite_id(&secret).map_err(|_| unavailable())?;
    let hash = invite_secret_hash(&secret).map_err(|_| unavailable())?;
    sqlx::query(
        "INSERT INTO realm_invite
            (realm,invite_id,secret_hash,role,member_id,created_by,created_at,expires_at)
         VALUES(?,?,?,?,?,?,?,?)",
    )
    .bind(operation.realm)
    .bind(&invite_id)
    .bind(hash)
    .bind(request.role.map(Role::as_str))
    .bind(&request.member_id)
    .bind(operation.key_id)
    .bind(now)
    .bind(expires_at)
    .execute(&mut *tx)
    .await?;

    let invitation = membership::IssuedInvite {
        invite_id,
        invite: RealmInvite::new(operation.realm, &secret).encode(),
        expires_at,
    }
    .to_json();
    store_receipt(&mut tx, delivery, operation.id, &invitation).await?;
    tx.commit().await?;
    Ok(Some(invitation))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actr_protocol::realm_membership::{MembershipOp, client_key_id};
    use ed25519_dalek::SigningKey;
    use platform::storage::db::Database;

    #[tokio::test]
    async fn receipt_write_failure_rolls_back_invite_and_same_request_can_retry() {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::new(dir.path()).await.unwrap();
        let db = database.get_pool();
        initialize(db).await.unwrap();

        let realm = "receipt-test@localhost";
        let key = SigningKey::from_bytes(&rand::random());
        let public_key = key.verifying_key().to_bytes();
        let key_id = client_key_id(&public_key);
        sqlx::query("INSERT INTO realm (name,created_at) VALUES(?,0)")
            .bind(realm)
            .execute(db)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO realm_member (realm, member_id, role, status, created_at)
             VALUES(?,'owner','owner','active',0)",
        )
        .bind(realm)
        .execute(db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO realm_client_key (realm,key_id,public_key,member_id,registered_at)
             VALUES(?,?,?,'owner',0)",
        )
        .bind(realm)
        .bind(&key_id)
        .bind(public_key.as_slice())
        .execute(db)
        .await
        .unwrap();

        let operation = Operation {
            id: "atomic-invite",
            provider: "localhost",
            realm,
            key_id: &key_id,
            fingerprint: "viewer",
        };
        let mut request =
            MembershipRequest::new(MembershipOp::CreateInvite, realm, "localhost", "test");
        request.role = Some(Role::Viewer);
        request.invite_expires_at = Some(membership::rfc3339(Utc::now().timestamp() + 3600));
        let request = MembershipRequest::parse(request.sign(&key).to_string().as_bytes()).unwrap();
        request.verify(&public_key).unwrap();
        let limits = membership::Limits::from(&platform::config::RealmConfig::default());
        let delivery = KeyEncryptor::from_kek(&actr_protocol::realm_membership::body_sha256(
            &rand::random::<[u8; 32]>(),
        ))
        .unwrap();

        sqlx::query(
            "CREATE TRIGGER fail_receipt BEFORE UPDATE OF receipt ON admin_operations
             BEGIN SELECT RAISE(ABORT, 'test receipt write failure'); END",
        )
        .execute(db)
        .await
        .unwrap();
        assert!(
            issue(db, &delivery, &operation, &request, limits)
                .await
                .is_err()
        );
        let count =
            || sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM realm_invite").fetch_one(db);
        assert_eq!(count().await.unwrap(), 0);
        assert!(load(db, operation.id).await.unwrap().is_none());

        sqlx::query("DROP TRIGGER fail_receipt")
            .execute(db)
            .await
            .unwrap();
        let result = issue(db, &delivery, &operation, &request, limits)
            .await
            .unwrap()
            .unwrap();
        assert!(
            issue(db, &delivery, &operation, &request, limits)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(count().await.unwrap(), 1);
        let stored = load(db, operation.id).await.unwrap().unwrap();
        let plain = delivery.decrypt(&stored.receipt.unwrap()).unwrap();
        assert!(serde_json::from_str::<Value>(&plain).unwrap() == result);
    }
}
