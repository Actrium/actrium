//! Realm members, Realm client keys and Realm invites
//! (`actr/protocols/realm-membership.md`).
//!
//! A member is a row with a role; each of its clients registers an Ed25519
//! Realm client key. Invites are single-use secrets of which only the
//! SHA-256 is stored. Every write runs in one `BEGIN IMMEDIATE`
//! transaction, which rechecks the signing key, its member and the Realm.
//!
//! Revoking a key revokes the unused invites it signed and disables the
//! Actors bound to it; [`super::changes::announce_actor`] tells Signaling and
//! TURN to drop their connections. The Actors are not revoked: another key
//! of the same member reissues them with their renewal token.

use actr_protocol::ActrId;
use actr_protocol::realm_membership::{RealmInvite, Role, invite_id, invite_secret_hash};
use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD as BASE64, URL_SAFE_NO_PAD};
use chrono::{TimeZone, Utc};
use rand::RngCore;
use serde_json::{Value, json};
use sqlx::{Sqlite, SqliteConnection, Transaction};

use super::changes;
use super::error::RealmError;
use super::model::Realm;
use crate::storage::db::get_database;

/// Lifetime of an owner invite.
pub const OWNER_INVITE_SECS: i64 = 7 * 24 * 3600;

/// `created_by` of an owner invite from the Realm signing key holder.
pub const BY_HOLDER: &str = "holder";
/// `created_by` of an owner invite from the operator.
pub const BY_OPERATOR: &str = "operator";

/// Stable membership error code (section 4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    InvalidRequest,
    ClientUnknown,
    ClientRevoked,
    MemberRemoved,
    Forbidden,
    RealmUnavailable,
    InviteUnknown,
    NotFound,
    InviteRealmMismatch,
    OwnerLastClient,
    OwnerMustTransfer,
    Conflict,
    InviteExpired,
    InviteUsed,
    InviteRevoked,
    Quota,
    Unavailable,
}

impl Code {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::ClientUnknown => "client_unknown",
            Self::ClientRevoked => "client_revoked",
            Self::MemberRemoved => "member_removed",
            Self::Forbidden => "forbidden",
            Self::RealmUnavailable => "realm_unavailable",
            Self::InviteUnknown => "invite_unknown",
            Self::NotFound => "not_found",
            Self::InviteRealmMismatch => "invite_realm_mismatch",
            Self::OwnerLastClient => "owner_last_client",
            Self::OwnerMustTransfer => "owner_must_transfer",
            Self::Conflict => "conflict",
            Self::InviteExpired => "invite_expired",
            Self::InviteUsed => "invite_used",
            Self::InviteRevoked => "invite_revoked",
            Self::Quota => "quota",
            Self::Unavailable => "unavailable",
        }
    }

    /// HTTP status of the code.
    pub fn status(self) -> u16 {
        match self {
            Self::InvalidRequest => 400,
            Self::ClientUnknown
            | Self::ClientRevoked
            | Self::MemberRemoved
            | Self::Forbidden
            | Self::RealmUnavailable => 403,
            Self::InviteUnknown | Self::NotFound => 404,
            Self::InviteRealmMismatch
            | Self::OwnerLastClient
            | Self::OwnerMustTransfer
            | Self::Conflict => 409,
            Self::InviteExpired | Self::InviteUsed | Self::InviteRevoked => 410,
            Self::Quota => 429,
            Self::Unavailable => 503,
        }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{}: {message}", code.as_str())]
pub struct MembershipError {
    pub code: Code,
    pub message: String,
}

impl MembershipError {
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

fn fail<T>(code: Code, message: impl Into<String>) -> Result<T, MembershipError> {
    Err(MembershipError::new(code, message))
}

impl From<sqlx::Error> for MembershipError {
    fn from(error: sqlx::Error) -> Self {
        Self::new(Code::Unavailable, format!("storage: {error}"))
    }
}

impl From<RealmError> for MembershipError {
    fn from(error: RealmError) -> Self {
        match error {
            RealmError::Membership(error) => error,
            error => Self::new(Code::Unavailable, error.to_string()),
        }
    }
}

/// Membership limits from `[realm]`.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_members: u32,
    pub max_clients_per_member: u32,
    pub max_open_invites: u32,
}

impl From<&crate::config::RealmConfig> for Limits {
    fn from(config: &crate::config::RealmConfig) -> Self {
        Self {
            max_members: config.max_members_per_realm(),
            max_clients_per_member: config.max_clients_per_member(),
            max_open_invites: config.max_open_invites_per_realm(),
        }
    }
}

/// A registered Realm client key with its member.
#[derive(Debug, Clone)]
pub struct ClientKey {
    pub realm: String,
    pub key_id: String,
    pub public_key: [u8; 32],
    pub member_id: String,
    pub role: Role,
    pub revoked: bool,
    pub member_active: bool,
}

/// The holder of an active Realm client key of an active member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signer {
    pub realm: String,
    pub key_id: String,
    pub member_id: String,
    pub role: Role,
}

fn now_secs() -> i64 {
    Utc::now().timestamp()
}

fn role_of(text: &str) -> Role {
    Role::parse(text).unwrap_or(Role::Viewer)
}

type KeyRow = (String, Vec<u8>, String, String, Option<i64>, String);

async fn client_key_on(
    conn: &mut SqliteConnection,
    realm: &str,
    key_id: &str,
) -> Result<Option<ClientKey>, MembershipError> {
    let row: Option<KeyRow> = sqlx::query_as(
        "SELECT k.key_id, k.public_key, k.member_id, m.role, k.revoked_at, m.status
         FROM realm_client_key k
         JOIN realm_member m ON m.realm = k.realm AND m.member_id = k.member_id
         WHERE k.realm = ? AND k.key_id = ?",
    )
    .bind(realm)
    .bind(key_id)
    .fetch_optional(conn)
    .await?;
    Ok(row.and_then(
        |(key_id, public_key, member_id, role, revoked_at, status)| {
            Some(ClientKey {
                realm: realm.to_string(),
                key_id,
                public_key: public_key.try_into().ok()?,
                member_id,
                role: role_of(&role),
                revoked: revoked_at.is_some(),
                member_active: status == "active",
            })
        },
    ))
}

/// The registered Realm client key `key_id` of `realm`, whose public key
/// verifies the request before [`authenticate`].
pub async fn client_key(realm: &str, key_id: &str) -> Result<Option<ClientKey>, MembershipError> {
    let mut conn = get_database().get_pool().acquire().await?;
    client_key_on(&mut conn, realm, key_id).await
}

fn signer_of(key: ClientKey) -> Result<Signer, MembershipError> {
    if key.revoked {
        return fail(Code::ClientRevoked, "the Realm client key is revoked");
    }
    if !key.member_active {
        return fail(Code::MemberRemoved, "the member was removed");
    }
    Ok(Signer {
        realm: key.realm,
        key_id: key.key_id,
        member_id: key.member_id,
        role: key.role,
    })
}

/// Accept a key whose signature has been verified: it must not be revoked
/// and its member must be active. Records its use (at most once a minute).
pub async fn authenticate(key: ClientKey) -> Result<Signer, MembershipError> {
    let signer = signer_of(key)?;
    let now = now_secs();
    sqlx::query(
        "UPDATE realm_client_key SET last_used_at = ?1
         WHERE realm = ?2 AND key_id = ?3 AND (last_used_at IS NULL OR last_used_at < ?1 - 60)",
    )
    .bind(now)
    .bind(&signer.realm)
    .bind(&signer.key_id)
    .execute(get_database().get_pool())
    .await?;
    Ok(signer)
}

/// Check on `conn` (e.g. inside a transaction) that the key `key_id` of
/// `realm` is registered, unrevoked and of an active member, and return its
/// holder with the member's current role.
pub async fn check_key_on(
    conn: &mut SqliteConnection,
    realm: &str,
    key_id: &str,
) -> Result<Signer, MembershipError> {
    let key = client_key_on(conn, realm, key_id)
        .await?
        .ok_or_else(|| MembershipError::new(Code::ClientUnknown, "unknown Realm client key"))?;
    signer_of(key)
}

/// Check on `conn` that `actor` is bound to an unrevoked key of an active
/// member.
pub async fn check_actor_key_on(
    conn: &mut SqliteConnection,
    actor: &ActrId,
) -> Result<(), MembershipError> {
    let binding = actor_binding_on(conn, actor).await?.ok_or_else(|| {
        MembershipError::new(Code::ClientUnknown, "the Actor has no Realm client key")
    })?;
    check_key_on(conn, &actor.realm, &binding.client_key_id)
        .await
        .map(|_| ())
}

async fn begin() -> Result<Transaction<'static, Sqlite>, MembershipError> {
    Ok(get_database()
        .get_pool()
        .begin_with("BEGIN IMMEDIATE")
        .await?)
}

/// Recheck `signer` and its Realm inside `tx`; the member's role may have
/// changed since authentication.
async fn recheck(
    tx: &mut Transaction<'_, Sqlite>,
    signer: &Signer,
) -> Result<Signer, MembershipError> {
    let signer = check_key_on(tx, &signer.realm, &signer.key_id).await?;
    check_realm_active(tx, &signer.realm).await?;
    Ok(signer)
}

async fn check_realm_active(
    conn: &mut SqliteConnection,
    realm: &str,
) -> Result<(), MembershipError> {
    match Realm::get_on(conn, realm).await? {
        Some(realm) if realm.is_active() => Ok(()),
        _ => fail(
            Code::RealmUnavailable,
            format!("Realm {realm} is not active"),
        ),
    }
}

/// Fail with `forbidden` unless `signer`, as last rechecked, is the owner.
pub fn require_owner(signer: &Signer) -> Result<(), MembershipError> {
    if signer.role != Role::Owner {
        return fail(Code::Forbidden, "only the owner may do this");
    }
    Ok(())
}

/// Revoke `key_ids` of `realm` and the unused invites they signed. The
/// Actors bound to them are disabled: every Issuer, Signaling and TURN check
/// refuses an Actor bound to a revoked key. Their renewal tokens stay, as
/// the proof with which another key of the member reissues them. Returns
/// those Actors, to announce once the transaction commits.
pub async fn revoke_keys_in(
    tx: &mut Transaction<'_, Sqlite>,
    realm: &str,
    key_ids: &[String],
    now: i64,
) -> Result<Vec<ActrId>, MembershipError> {
    let mut actors = Vec::new();
    for key_id in key_ids {
        sqlx::query(
            "UPDATE realm_client_key SET revoked_at = ?
             WHERE realm = ? AND key_id = ? AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(realm)
        .bind(key_id)
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            "UPDATE realm_invite SET revoked_at = ?
             WHERE realm = ? AND created_by = ? AND used_at IS NULL AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(realm)
        .bind(key_id)
        .execute(&mut **tx)
        .await?;
        let serials: Vec<i64> = sqlx::query_scalar(
            "SELECT serial_number FROM issued_actor_identity
             WHERE realm = ? AND client_key_id = ?",
        )
        .bind(realm)
        .bind(key_id)
        .fetch_all(&mut **tx)
        .await?;
        actors.extend(serials.into_iter().map(|serial| ActrId {
            realm: realm.to_string(),
            serial_number: serial as u64,
        }));
    }
    Ok(actors)
}

fn announce(actors: &[ActrId]) {
    for actor in actors {
        changes::announce_actor(actor);
    }
}

/// Unrevoked key ids of a member.
async fn member_keys(
    conn: &mut SqliteConnection,
    realm: &str,
    member_id: &str,
) -> Result<Vec<String>, MembershipError> {
    Ok(sqlx::query_scalar(
        "SELECT key_id FROM realm_client_key
         WHERE realm = ? AND member_id = ? AND revoked_at IS NULL ORDER BY key_id",
    )
    .bind(realm)
    .bind(member_id)
    .fetch_all(conn)
    .await?)
}

/// A member's role and whether it is active.
async fn member_on(
    conn: &mut SqliteConnection,
    realm: &str,
    member_id: &str,
) -> Result<Option<(Role, bool)>, MembershipError> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT role, status FROM realm_member WHERE realm = ? AND member_id = ?")
            .bind(realm)
            .bind(member_id)
            .fetch_optional(conn)
            .await?;
    Ok(row.map(|(role, status)| (role_of(&role), status == "active")))
}

fn random_member_id() -> String {
    let mut bytes = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn new_secret() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// A new invite: its id, the text form, and when it expires.
#[derive(Debug, Clone)]
pub struct IssuedInvite {
    pub invite_id: String,
    pub invite: String,
    pub expires_at: i64,
}

impl IssuedInvite {
    pub fn to_json(&self) -> Value {
        json!({
            "invite_id": self.invite_id,
            "invite": self.invite,
            "expires_at": rfc3339(self.expires_at),
        })
    }
}

/// RFC 3339 form of a Unix time.
pub fn rfc3339(secs: i64) -> String {
    Utc.timestamp_opt(secs, 0)
        .single()
        .unwrap_or_default()
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

async fn insert_invite(
    tx: &mut Transaction<'_, Sqlite>,
    realm: &str,
    target: (Option<Role>, Option<&str>),
    created_by: &str,
    now: i64,
    expires_at: i64,
) -> Result<IssuedInvite, MembershipError> {
    let secret = new_secret();
    let id = invite_id(&secret).map_err(|e| MembershipError::new(Code::Unavailable, e.0))?;
    let hash =
        invite_secret_hash(&secret).map_err(|e| MembershipError::new(Code::Unavailable, e.0))?;
    sqlx::query(
        "INSERT INTO realm_invite
            (realm, invite_id, secret_hash, role, member_id, created_by, created_at, expires_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(realm)
    .bind(&id)
    .bind(&hash)
    .bind(target.0.map(Role::as_str))
    .bind(target.1)
    .bind(created_by)
    .bind(now)
    .bind(expires_at)
    .execute(&mut **tx)
    .await?;
    Ok(IssuedInvite {
        invite_id: id,
        invite: RealmInvite::new(realm, &secret).encode(),
        expires_at,
    })
}

/// Issue an owner invite inside `tx`, revoking the unused owner invites of
/// the Realm (section 5). `created_by` is [`BY_HOLDER`] or [`BY_OPERATOR`].
pub async fn issue_owner_invite_in(
    tx: &mut Transaction<'_, Sqlite>,
    realm: &str,
    created_by: &str,
    now: i64,
) -> Result<IssuedInvite, MembershipError> {
    sqlx::query(
        "UPDATE realm_invite SET revoked_at = ?
         WHERE realm = ? AND role = 'owner' AND used_at IS NULL AND revoked_at IS NULL",
    )
    .bind(now)
    .bind(realm)
    .execute(&mut **tx)
    .await?;
    insert_invite(
        tx,
        realm,
        (Some(Role::Owner), None),
        created_by,
        now,
        now + OWNER_INVITE_SECS,
    )
    .await
}

/// Issue an owner invite for an `active` Realm.
pub async fn issue_owner_invite(
    realm: &str,
    created_by: &str,
) -> Result<IssuedInvite, MembershipError> {
    let mut tx = begin().await?;
    let status: Option<String> = sqlx::query_scalar("SELECT status FROM realm WHERE name = ?")
        .bind(realm)
        .fetch_optional(&mut *tx)
        .await?;
    match status.as_deref() {
        None => return fail(Code::NotFound, format!("Realm {realm} not found")),
        Some("active") => {}
        Some(status) => return fail(Code::Conflict, format!("Realm {realm} is {status}")),
    }
    let invite = issue_owner_invite_in(&mut tx, realm, created_by, now_secs()).await?;
    tx.commit().await?;
    Ok(invite)
}

/// The Realm of the invite whose secret hashes to `secret_hash`.
pub async fn invite_realm(secret_hash: &str) -> Result<Option<String>, MembershipError> {
    Ok(
        sqlx::query_scalar("SELECT realm FROM realm_invite WHERE secret_hash = ?")
            .bind(secret_hash)
            .fetch_optional(get_database().get_pool())
            .await?,
    )
}

type InviteRow = (
    String,
    Option<String>,
    Option<String>,
    i64,
    Option<i64>,
    Option<String>,
    Option<i64>,
);

/// Redeem the invite whose secret hashes to `secret_hash` with a new Realm
/// client key (section 4.1). The request signature has been verified with
/// `public_key` and its nonce recorded.
pub async fn join(
    realm: &str,
    secret_hash: &str,
    public_key: &[u8; 32],
    label: &str,
    limits: Limits,
) -> Result<Value, MembershipError> {
    let key_id = actr_protocol::realm_membership::client_key_id(public_key);
    let now = now_secs();
    let mut tx = begin().await?;
    let invite: Option<InviteRow> = sqlx::query_as(
        "SELECT invite_id, role, member_id, expires_at, used_at, used_by_key, revoked_at
         FROM realm_invite WHERE realm = ? AND secret_hash = ?",
    )
    .bind(realm)
    .bind(secret_hash)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((invite_id, role, target, expires_at, used_at, used_by_key, revoked_at)) = invite
    else {
        return fail(Code::InviteUnknown, "no such invite");
    };
    if revoked_at.is_some() {
        return fail(Code::InviteRevoked, "the invite was revoked");
    }
    if used_at.is_some() {
        // A retry of the same redemption returns its result.
        if used_by_key.as_deref() == Some(key_id.as_str())
            && let Some(key) = client_key_on(&mut tx, realm, &key_id).await?
            && !key.revoked
            && key.member_active
        {
            return Ok(json!({
                "realm": realm, "member_id": key.member_id,
                "role": key.role.as_str(), "key_id": key_id,
            }));
        }
        return fail(Code::InviteUsed, "the invite was used");
    }
    if expires_at <= now {
        return fail(Code::InviteExpired, "the invite expired");
    }
    check_realm_active(&mut tx, realm).await?;

    let mut revoked_actors = Vec::new();
    let (member_id, role) = match (role.as_deref().and_then(Role::parse), target) {
        (Some(Role::Owner), _) => {
            let owner: Option<String> = sqlx::query_scalar(
                "SELECT member_id FROM realm_member
                 WHERE realm = ? AND role = 'owner' AND status = 'active'",
            )
            .bind(realm)
            .fetch_optional(&mut *tx)
            .await?;
            match owner {
                // Owner recovery: the new key replaces all the owner's keys.
                Some(owner) => {
                    let keys = member_keys(&mut tx, realm, &owner).await?;
                    revoked_actors = revoke_keys_in(&mut tx, realm, &keys, now).await?;
                    (owner, Role::Owner)
                }
                None => (
                    insert_member(&mut tx, realm, Role::Owner, now, limits).await?,
                    Role::Owner,
                ),
            }
        }
        (Some(role), None) => (
            insert_member(&mut tx, realm, role, now, limits).await?,
            role,
        ),
        (None, Some(member_id)) => match member_on(&mut tx, realm, &member_id).await? {
            Some((role, true)) => (member_id, role),
            _ => return fail(Code::MemberRemoved, "the invited member was removed"),
        },
        _ => {
            return fail(
                Code::Unavailable,
                format!("invite {invite_id} is malformed"),
            );
        }
    };

    let clients: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM realm_client_key
         WHERE realm = ? AND member_id = ? AND revoked_at IS NULL",
    )
    .bind(realm)
    .bind(&member_id)
    .fetch_one(&mut *tx)
    .await?;
    if clients >= i64::from(limits.max_clients_per_member) {
        return fail(Code::Quota, "the member has the most clients allowed");
    }
    let inserted = sqlx::query(
        "INSERT INTO realm_client_key (realm, key_id, public_key, member_id, label, registered_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(realm)
    .bind(&key_id)
    .bind(public_key.as_slice())
    .bind(&member_id)
    .bind(label)
    .bind(now)
    .execute(&mut *tx)
    .await;
    match inserted {
        Ok(_) => {}
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            return fail(Code::Conflict, "the Realm client key is already registered");
        }
        Err(error) => return Err(error.into()),
    }
    sqlx::query(
        "UPDATE realm_invite SET used_at = ?, used_by_key = ? WHERE realm = ? AND invite_id = ?",
    )
    .bind(now)
    .bind(&key_id)
    .bind(realm)
    .bind(&invite_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    announce(&revoked_actors);
    Ok(json!({
        "realm": realm, "member_id": member_id, "role": role.as_str(), "key_id": key_id,
    }))
}

async fn insert_member(
    tx: &mut Transaction<'_, Sqlite>,
    realm: &str,
    role: Role,
    now: i64,
    limits: Limits,
) -> Result<String, MembershipError> {
    let members: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM realm_member WHERE realm = ? AND status = 'active'",
    )
    .bind(realm)
    .fetch_one(&mut **tx)
    .await?;
    if members >= i64::from(limits.max_members) {
        return fail(Code::Quota, "the Realm has the most members allowed");
    }
    let member_id = random_member_id();
    sqlx::query(
        "INSERT INTO realm_member (realm, member_id, role, status, created_at)
         VALUES (?, ?, ?, 'active', ?)",
    )
    .bind(realm)
    .bind(&member_id)
    .bind(role.as_str())
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(member_id)
}

/// `create_invite`: a new member (`role`) or a new client of `member_id`,
/// valid until `expires_at` (Unix seconds).
pub async fn create_invite(
    signer: &Signer,
    role: Option<Role>,
    member_id: Option<&str>,
    expires_at: i64,
    limits: Limits,
) -> Result<IssuedInvite, MembershipError> {
    let now = now_secs();
    let mut tx = begin().await?;
    let signer = recheck(&mut tx, signer).await?;
    match (role, member_id) {
        (Some(_), None) => require_owner(&signer)?,
        (None, Some(target)) => {
            if signer.role != Role::Owner && target != signer.member_id {
                return fail(
                    Code::Forbidden,
                    "members invite clients only for themselves",
                );
            }
            match member_on(&mut tx, &signer.realm, target).await? {
                None => return fail(Code::NotFound, "no such member"),
                Some((_, false)) => return fail(Code::Conflict, "the member was removed"),
                Some(_) => {}
            }
        }
        _ => {
            return fail(
                Code::InvalidRequest,
                "create_invite needs either role or member_id",
            );
        }
    }
    if role == Some(Role::Owner) {
        return fail(Code::InvalidRequest, "role must be editor or viewer");
    }
    if expires_at <= now || expires_at > now + actr_protocol::realm_membership::MAX_INVITE_SECS {
        return fail(
            Code::InvalidRequest,
            "invite_expires_at must be in the next 30 days",
        );
    }
    let open: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM realm_invite
         WHERE realm = ? AND used_at IS NULL AND revoked_at IS NULL AND expires_at > ?",
    )
    .bind(&signer.realm)
    .bind(now)
    .fetch_one(&mut *tx)
    .await?;
    if open >= i64::from(limits.max_open_invites) {
        return fail(Code::Quota, "the Realm has the most open invites allowed");
    }
    let invite = insert_invite(
        &mut tx,
        &signer.realm,
        (role, member_id),
        &signer.key_id,
        now,
        expires_at,
    )
    .await?;
    tx.commit().await?;
    Ok(invite)
}

/// `revoke_invite`: by the owner or a key of the member that signed it.
pub async fn revoke_invite(signer: &Signer, invite_id: &str) -> Result<Value, MembershipError> {
    let now = now_secs();
    let mut tx = begin().await?;
    let signer = recheck(&mut tx, signer).await?;
    type Row = (String, Option<i64>, Option<i64>, Option<String>);
    let row: Option<Row> = sqlx::query_as(
        "SELECT i.created_by, i.used_at, i.revoked_at, k.member_id
         FROM realm_invite i
         LEFT JOIN realm_client_key k ON k.realm = i.realm AND k.key_id = i.created_by
         WHERE i.realm = ? AND i.invite_id = ?",
    )
    .bind(&signer.realm)
    .bind(invite_id)
    .fetch_optional(&mut *tx)
    .await?;
    let own = row
        .as_ref()
        .is_some_and(|row| row.3.as_deref() == Some(signer.member_id.as_str()));
    if signer.role != Role::Owner && !own {
        return fail(
            Code::Forbidden,
            "only the owner or its issuer may revoke an invite",
        );
    }
    let Some((_, used_at, revoked_at, _)) = row else {
        return fail(Code::NotFound, "no such invite");
    };
    if used_at.is_some() {
        return fail(Code::InviteUsed, "the invite was used");
    }
    if revoked_at.is_none() {
        sqlx::query("UPDATE realm_invite SET revoked_at = ? WHERE realm = ? AND invite_id = ?")
            .bind(now)
            .bind(&signer.realm)
            .bind(invite_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(json!({ "realm": signer.realm, "invite_id": invite_id }))
}

/// `revoke_client`: a member revokes its own clients, the owner any.
pub async fn revoke_client(signer: &Signer, key_id: &str) -> Result<Value, MembershipError> {
    let now = now_secs();
    let mut tx = begin().await?;
    let signer = recheck(&mut tx, signer).await?;
    let target = client_key_on(&mut tx, &signer.realm, key_id).await?;
    let own = target
        .as_ref()
        .is_some_and(|key| key.member_id == signer.member_id);
    if signer.role != Role::Owner && !own {
        return fail(Code::Forbidden, "members revoke only their own clients");
    }
    let Some(target) = target else {
        return fail(Code::NotFound, "no such client");
    };
    let mut actors = Vec::new();
    if !target.revoked {
        if own && signer.role == Role::Owner {
            let keys = member_keys(&mut tx, &signer.realm, &signer.member_id).await?;
            if keys.len() <= 1 {
                return fail(
                    Code::OwnerLastClient,
                    "the owner cannot revoke its last client; transfer ownership first",
                );
            }
        }
        actors = revoke_keys_in(
            &mut tx,
            &signer.realm,
            std::slice::from_ref(&target.key_id),
            now,
        )
        .await?;
    }
    tx.commit().await?;
    announce(&actors);
    Ok(json!({ "realm": signer.realm, "key_id": key_id }))
}

/// Remove `member_id`: revoke all its keys and the invites for it.
async fn remove_in(
    tx: &mut Transaction<'_, Sqlite>,
    realm: &str,
    member_id: &str,
    now: i64,
) -> Result<Vec<ActrId>, MembershipError> {
    sqlx::query(
        "UPDATE realm_member SET status = 'removed', removed_at = ?
         WHERE realm = ? AND member_id = ? AND status = 'active'",
    )
    .bind(now)
    .bind(realm)
    .bind(member_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE realm_invite SET revoked_at = ?
         WHERE realm = ? AND member_id = ? AND used_at IS NULL AND revoked_at IS NULL",
    )
    .bind(now)
    .bind(realm)
    .bind(member_id)
    .execute(&mut **tx)
    .await?;
    let keys = member_keys(tx, realm, member_id).await?;
    revoke_keys_in(tx, realm, &keys, now).await
}

/// `remove_member`: only the owner, never itself.
pub async fn remove_member(signer: &Signer, member_id: &str) -> Result<Value, MembershipError> {
    let now = now_secs();
    let mut tx = begin().await?;
    let signer = recheck(&mut tx, signer).await?;
    require_owner(&signer)?;
    if member_id == signer.member_id {
        return fail(
            Code::OwnerMustTransfer,
            "the owner cannot remove itself; transfer ownership first",
        );
    }
    if member_on(&mut tx, &signer.realm, member_id)
        .await?
        .is_none()
    {
        return fail(Code::NotFound, "no such member");
    }
    let actors = remove_in(&mut tx, &signer.realm, member_id, now).await?;
    tx.commit().await?;
    announce(&actors);
    Ok(json!({ "realm": signer.realm, "member_id": member_id }))
}

/// `leave`: a member other than the owner removes itself.
pub async fn leave(signer: &Signer) -> Result<Value, MembershipError> {
    let now = now_secs();
    let mut tx = begin().await?;
    let signer = recheck(&mut tx, signer).await?;
    if signer.role == Role::Owner {
        return fail(
            Code::OwnerMustTransfer,
            "the owner cannot leave; transfer ownership first",
        );
    }
    let actors = remove_in(&mut tx, &signer.realm, &signer.member_id, now).await?;
    tx.commit().await?;
    announce(&actors);
    Ok(json!({ "realm": signer.realm, "member_id": signer.member_id }))
}

/// `set_role`: only the owner; `editor` or `viewer`.
pub async fn set_role(
    signer: &Signer,
    member_id: &str,
    role: Role,
) -> Result<Value, MembershipError> {
    let mut tx = begin().await?;
    let signer = recheck(&mut tx, signer).await?;
    require_owner(&signer)?;
    if role == Role::Owner {
        return fail(
            Code::InvalidRequest,
            "role must be editor or viewer; use transfer_owner",
        );
    }
    if member_id == signer.member_id {
        return fail(
            Code::OwnerMustTransfer,
            "use transfer_owner to change the owner",
        );
    }
    match member_on(&mut tx, &signer.realm, member_id).await? {
        None => return fail(Code::NotFound, "no such member"),
        Some((_, false)) => return fail(Code::Conflict, "the member was removed"),
        Some(_) => {}
    }
    sqlx::query("UPDATE realm_member SET role = ? WHERE realm = ? AND member_id = ?")
        .bind(role.as_str())
        .bind(&signer.realm)
        .bind(member_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(json!({ "realm": signer.realm, "member_id": member_id, "role": role.as_str() }))
}

/// `transfer_owner` to an active member with at least one client.
pub async fn transfer_owner(
    signer: &Signer,
    member_id: &str,
    previous_owner_role: Role,
) -> Result<Value, MembershipError> {
    let mut tx = begin().await?;
    let signer = recheck(&mut tx, signer).await?;
    require_owner(&signer)?;
    if previous_owner_role == Role::Owner {
        return fail(
            Code::InvalidRequest,
            "previous_owner_role must be editor or viewer",
        );
    }
    if member_id == signer.member_id {
        return fail(Code::Conflict, "the member is already the owner");
    }
    match member_on(&mut tx, &signer.realm, member_id).await? {
        None => return fail(Code::NotFound, "no such member"),
        Some((_, false)) => return fail(Code::Conflict, "the member was removed"),
        Some(_) => {}
    }
    if member_keys(&mut tx, &signer.realm, member_id)
        .await?
        .is_empty()
    {
        return fail(Code::Conflict, "the member has no client to sign with");
    }
    for (member, role) in [
        (signer.member_id.as_str(), previous_owner_role),
        (member_id, Role::Owner),
    ] {
        sqlx::query("UPDATE realm_member SET role = ? WHERE realm = ? AND member_id = ?")
            .bind(role.as_str())
            .bind(&signer.realm)
            .bind(member)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(json!({
        "realm": signer.realm, "owner": member_id,
        "previous_owner": signer.member_id, "previous_owner_role": previous_owner_role.as_str(),
    }))
}

type MemberRow = (String, String, String, i64, Option<i64>);
type ClientRow = (
    String,
    Vec<u8>,
    String,
    String,
    i64,
    Option<i64>,
    Option<i64>,
);
type OpenInviteRow = (String, Option<String>, Option<String>, String, i64, i64);

fn time(secs: Option<i64>) -> Value {
    secs.map_or(Value::Null, |secs| Value::String(rfc3339(secs)))
}

/// Members, their clients and the open invites of `realm`. `viewer` limits
/// a non-owner to its own member, clients and the invites its keys signed;
/// `None` (the operator) and the owner see everything.
pub async fn list_members(realm: &str, viewer: Option<&Signer>) -> Result<Value, MembershipError> {
    let mut conn = get_database().get_pool().acquire().await?;
    let viewer = match viewer {
        Some(signer) => {
            let key = client_key_on(&mut conn, realm, &signer.key_id)
                .await?
                .ok_or_else(|| MembershipError::new(Code::ClientUnknown, "unknown client"))?;
            let signer = signer_of(key)?;
            check_realm_active(&mut conn, realm).await?;
            (signer.role != Role::Owner).then_some(signer.member_id)
        }
        None => None,
    };
    let members: Vec<MemberRow> = sqlx::query_as(
        "SELECT member_id, role, status, created_at, removed_at FROM realm_member
         WHERE realm = ?1 AND (?2 IS NULL OR member_id = ?2) ORDER BY created_at, member_id",
    )
    .bind(realm)
    .bind(viewer.as_deref())
    .fetch_all(&mut *conn)
    .await?;
    let clients: Vec<ClientRow> = sqlx::query_as(
        "SELECT key_id, public_key, member_id, label, registered_at, last_used_at, revoked_at
         FROM realm_client_key
         WHERE realm = ?1 AND (?2 IS NULL OR member_id = ?2) ORDER BY registered_at, key_id",
    )
    .bind(realm)
    .bind(viewer.as_deref())
    .fetch_all(&mut *conn)
    .await?;
    let invites: Vec<OpenInviteRow> = sqlx::query_as(
        "SELECT i.invite_id, i.role, i.member_id, i.created_by, i.created_at, i.expires_at
         FROM realm_invite i
         LEFT JOIN realm_client_key k ON k.realm = i.realm AND k.key_id = i.created_by
         WHERE i.realm = ?1 AND i.used_at IS NULL AND i.revoked_at IS NULL AND i.expires_at > ?3
           AND (?2 IS NULL OR k.member_id = ?2)
         ORDER BY i.created_at, i.invite_id",
    )
    .bind(realm)
    .bind(viewer.as_deref())
    .bind(now_secs())
    .fetch_all(&mut *conn)
    .await?;

    let members: Vec<Value> = members
        .into_iter()
        .map(|(member_id, role, status, created_at, removed_at)| {
            let clients: Vec<Value> = clients
                .iter()
                .filter(|client| client.2 == member_id)
                .map(
                    |(key_id, public_key, _, label, registered, used, revoked)| {
                        json!({
                            "key_id": key_id,
                            "public_key": BASE64.encode(public_key),
                            "label": label,
                            "registered_at": time(Some(*registered)),
                            "last_used_at": time(*used),
                            "revoked_at": time(*revoked),
                        })
                    },
                )
                .collect();
            json!({
                "member_id": member_id, "role": role, "status": status,
                "created_at": time(Some(created_at)), "removed_at": time(removed_at),
                "clients": clients,
            })
        })
        .collect();
    let invites: Vec<Value> = invites
        .into_iter()
        .map(
            |(invite_id, role, member_id, created_by, created_at, expires_at)| {
                let mut invite = json!({
                    "invite_id": invite_id, "created_by": created_by,
                    "created_at": time(Some(created_at)), "expires_at": time(Some(expires_at)),
                });
                if let Some(role) = role {
                    invite["role"] = json!(role);
                }
                if let Some(member_id) = member_id {
                    invite["member_id"] = json!(member_id);
                }
                invite
            },
        )
        .collect();
    Ok(json!({ "realm": realm, "members": members, "invites": invites }))
}

/// The member and client key an Actor is bound to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActorBinding {
    pub member_id: String,
    pub client_key_id: String,
}

/// The binding of `actor`, if it was issued here.
pub async fn actor_binding_on(
    conn: &mut SqliteConnection,
    actor: &ActrId,
) -> Result<Option<ActorBinding>, sqlx::Error> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT member_id, client_key_id FROM issued_actor_identity
         WHERE realm = ? AND serial_number = ?",
    )
    .bind(&actor.realm)
    .bind(actor.serial_number as i64)
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|(member_id, client_key_id)| ActorBinding {
        member_id,
        client_key_id,
    }))
}

/// Whether `actor` is disabled: revoked, or bound to a revoked Realm client
/// key (section 4.3 step 5).
pub async fn actor_disabled_on(
    conn: &mut SqliteConnection,
    actor: &ActrId,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM revoked_actor WHERE realm = ?1 AND serial_number = ?2)
             OR EXISTS(
                SELECT 1 FROM issued_actor_identity i
                JOIN realm_client_key k ON k.realm = i.realm AND k.key_id = i.client_key_id
                WHERE i.realm = ?1 AND i.serial_number = ?2 AND k.revoked_at IS NOT NULL)",
    )
    .bind(&actor.realm)
    .bind(actor.serial_number as i64)
    .fetch_one(conn)
    .await
}

/// [`actor_disabled_on`] on a pooled connection.
pub async fn actor_disabled(actor: &ActrId) -> Result<bool, sqlx::Error> {
    let mut conn = get_database().get_pool().acquire().await?;
    actor_disabled_on(&mut conn, actor).await
}

/// Whether Signaling refuses a credential of `actor` issued to the Realm
/// client key `client_key`: the Actor is disabled, or a reissue has since
/// moved it to another key, so a credential of the old key (perhaps revoked
/// by now) does not connect.
pub async fn credential_refused(actor: &ActrId, client_key: &str) -> Result<bool, sqlx::Error> {
    let mut conn = get_database().get_pool().acquire().await?;
    if actor_disabled_on(&mut conn, actor).await? {
        return Ok(true);
    }
    Ok(actor_binding_on(&mut conn, actor)
        .await?
        .is_some_and(|binding| binding.client_key_id != client_key))
}

/// Current TURN bindings of active members and unrevoked client keys.
pub async fn turn_actor_bindings() -> Result<Vec<(ActrId, String)>, sqlx::Error> {
    let rows: Vec<(String, i64, String)> = sqlx::query_as(
        "SELECT i.realm, i.serial_number, i.client_key_id FROM issued_actor_identity i
         JOIN realm_client_key k ON k.realm = i.realm AND k.key_id = i.client_key_id
         JOIN realm_member m ON m.realm = k.realm AND m.member_id = k.member_id
         WHERE k.revoked_at IS NULL AND m.status = 'active'",
    )
    .fetch_all(get_database().get_pool())
    .await?;
    Ok(rows
        .into_iter()
        .map(|(realm, serial, key)| {
            (
                ActrId {
                    realm,
                    serial_number: serial as u64,
                },
                key,
            )
        })
        .collect())
}

/// Every disabled Actor.
pub async fn disabled_actors() -> Result<Vec<ActrId>, sqlx::Error> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT realm, serial_number FROM revoked_actor
         UNION
         SELECT i.realm, i.serial_number FROM issued_actor_identity i
         JOIN realm_client_key k ON k.realm = i.realm AND k.key_id = i.client_key_id
         WHERE k.revoked_at IS NOT NULL",
    )
    .fetch_all(get_database().get_pool())
    .await?;
    Ok(rows
        .into_iter()
        .map(|(realm, serial)| ActrId {
            realm,
            serial_number: serial as u64,
        })
        .collect())
}

#[cfg(test)]
mod tests;
