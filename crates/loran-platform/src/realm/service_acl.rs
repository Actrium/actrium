//! Service instance ACL (FR-SEC-013): which Realm members may attach to a
//! Service Actor, separate from Realm membership (`realm-membership.md`
//! section 4).
//!
//! The Service's owner is the member the Actor identity was issued to
//! (`issued_actor_identity`); the owner is implicit and a row never names
//! them. Only the owner grants, refreshes and revokes delegate rows. Grants
//! carry `editor` or `viewer` and an expiry; the attachee's effective
//! ceiling is its credential's expiry, and Signaling re-reads the ACL on
//! every route/spec request, so a revocation is live the moment it commits —
//! no broadcast is needed and none is sent (unlike a client-key revocation,
//! an ACL change must not drop the Service's connections).
//!
//! Enforcement is deliberately "enforce what is known": an Actor this
//! Provider did not issue has no binding, and the ACL layer stays silent
//! for it (the type-level ACL still applies).

use actr_protocol::ActrId;
use actr_protocol::realm_membership::Role;
use serde_json::{Value, json};
use sqlx::SqliteConnection;

use super::membership::{Code, MembershipError, Signer, actor_binding_on};
use crate::storage::db::get_database;

/// Longest a grant may run: just over a year.
pub const MAX_GRANT_SECS: i64 = 366 * 24 * 3600;

fn fail<T>(code: Code, message: impl Into<String>) -> Result<T, MembershipError> {
    Err(MembershipError::new(code, message))
}

fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

/// The binding of `service`, which must be an identity this Provider issued.
async fn service_binding_on(
    conn: &mut SqliteConnection,
    realm: &str,
    service_serial: u64,
) -> Result<String, MembershipError> {
    let service = ActrId {
        realm: realm.to_string(),
        serial_number: service_serial,
    };
    actor_binding_on(conn, &service)
        .await?
        .map(|binding| binding.member_id)
        .ok_or_else(|| {
            MembershipError::new(
                Code::NotFound,
                format!("no Service Actor at serial {service_serial} of {realm}"),
            )
        })
}

/// Whether `member_id` is an active member of `realm`.
async fn active_member_on(
    conn: &mut SqliteConnection,
    realm: &str,
    member_id: &str,
) -> Result<bool, MembershipError> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM realm_member
         WHERE realm = ? AND member_id = ? AND status = 'active')",
    )
    .bind(realm)
    .bind(member_id)
    .fetch_one(conn)
    .await?)
}

/// `service_acl_grant`: give `member_id` `role` on the Service Actor until
/// `expires_at` (unix seconds). The signer must be the Service's owner;
/// re-granting refreshes role, expiry and granter.
pub async fn acl_grant(
    signer: &Signer,
    service_serial: u64,
    member_id: &str,
    role: Role,
    expires_at: i64,
) -> Result<Value, MembershipError> {
    if role == Role::Owner {
        return fail(
            Code::InvalidRequest,
            "a grant is editor or viewer; the owner is implicit",
        );
    }
    let now = now_secs();
    if expires_at <= now {
        return fail(Code::InvalidRequest, "grant_expires_at is in the past");
    }
    if expires_at - now > MAX_GRANT_SECS {
        return fail(
            Code::InvalidRequest,
            format!("grant_expires_at is more than {MAX_GRANT_SECS} seconds out"),
        );
    }
    let mut tx = get_database()
        .get_pool()
        .begin()
        .await
        .map_err(|error| MembershipError::new(Code::Unavailable, error.to_string()))?;
    let owner = service_binding_on(&mut tx, &signer.realm, service_serial).await?;
    if owner != signer.member_id {
        return fail(
            Code::Forbidden,
            "only the Service's owner grants roles on it",
        );
    }
    if member_id == owner {
        return fail(
            Code::InvalidRequest,
            "the owner needs no grant on their own Service",
        );
    }
    if !active_member_on(&mut tx, &signer.realm, member_id).await? {
        return fail(Code::NotFound, "no such active member");
    }
    sqlx::query(
        "INSERT INTO service_acl
             (realm, serial_number, member_id, role, granted_by, expires_at,
              revoked_at, created_at)
         VALUES (?, ?, ?, ?, ?, ?, NULL, ?)
         ON CONFLICT(realm, serial_number, member_id) DO UPDATE SET
             role = excluded.role,
             granted_by = excluded.granted_by,
             expires_at = excluded.expires_at,
             revoked_at = NULL,
             created_at = excluded.created_at",
    )
    .bind(&signer.realm)
    .bind(service_serial as i64)
    .bind(member_id)
    // `Owner` returned above.
    .bind(role.as_str())
    .bind(&signer.key_id)
    .bind(expires_at)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    tx.commit()
        .await
        .map_err(|error| MembershipError::new(Code::Unavailable, error.to_string()))?;
    Ok(json!({
        "realm": signer.realm,
        "service_serial": service_serial,
        "member_id": member_id,
        "role": role.as_str(),
        "expires_at": super::membership::rfc3339(expires_at),
    }))
}

/// `service_acl_revoke`: remove `member_id`'s grant on the Service Actor.
pub async fn acl_revoke(
    signer: &Signer,
    service_serial: u64,
    member_id: &str,
) -> Result<Value, MembershipError> {
    let mut tx = get_database()
        .get_pool()
        .begin()
        .await
        .map_err(|error| MembershipError::new(Code::Unavailable, error.to_string()))?;
    let owner = service_binding_on(&mut tx, &signer.realm, service_serial).await?;
    if owner != signer.member_id {
        return fail(
            Code::Forbidden,
            "only the Service's owner revokes roles on it",
        );
    }
    let result = sqlx::query(
        "UPDATE service_acl SET revoked_at = ?
         WHERE realm = ? AND serial_number = ? AND member_id = ?
             AND revoked_at IS NULL",
    )
    .bind(now_secs())
    .bind(&signer.realm)
    .bind(service_serial as i64)
    .bind(member_id)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 0 {
        return fail(Code::NotFound, "no such grant");
    }
    tx.commit()
        .await
        .map_err(|error| MembershipError::new(Code::Unavailable, error.to_string()))?;
    Ok(json!({
        "realm": signer.realm,
        "service_serial": service_serial,
        "member_id": member_id,
        "revoked": true,
    }))
}

/// `service_acl_list`: the Service Actor's grants, active and history.
pub async fn acl_list(signer: &Signer, service_serial: u64) -> Result<Value, MembershipError> {
    let mut conn = get_database()
        .get_pool()
        .acquire()
        .await
        .map_err(|error| MembershipError::new(Code::Unavailable, error.to_string()))?;
    let owner = service_binding_on(&mut conn, &signer.realm, service_serial).await?;
    if owner != signer.member_id {
        return fail(Code::Forbidden, "only the Service's owner reads its ACL");
    }
    let rows: Vec<(String, String, String, i64, Option<i64>, i64)> = sqlx::query_as(
        "SELECT member_id, role, granted_by, expires_at, revoked_at, created_at
         FROM service_acl
         WHERE realm = ? AND serial_number = ?
         ORDER BY member_id",
    )
    .bind(&signer.realm)
    .bind(service_serial as i64)
    .fetch_all(&mut *conn)
    .await?;
    let now = now_secs();
    Ok(json!({
        "realm": signer.realm,
        "service_serial": service_serial,
        "owner": owner,
        "grants": rows
            .into_iter()
            .map(|(member_id, role, granted_by, expires_at, revoked_at, created_at)| {
                let state = if revoked_at.is_some() {
                    "revoked"
                } else if expires_at <= now {
                    "expired"
                } else {
                    "active"
                };
                json!({
                    "member_id": member_id,
                    "role": role,
                    "granted_by": granted_by,
                    "expires_at": super::membership::rfc3339(expires_at),
                    "created_at": super::membership::rfc3339(created_at),
                    "state": state,
                })
            })
            .collect::<Vec<_>>(),
    }))
}

/// The instance-ACL decision for one (Actor, member) pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attachee {
    /// No identity issued here for the Actor: not instance-guarded.
    Unissued,
    /// The member the identity was issued to: always allowed.
    Owner,
    /// An unrevoked, unexpired grant.
    Granted,
    /// Issued to another member, with no live grant.
    Denied,
}

impl Attachee {
    pub fn allowed(self) -> bool {
        !matches!(self, Self::Denied)
    }
}

/// The instance-ACL decision for (`service`, `member_id`): the binding and
/// the grant existence in a single round trip (one pool acquire, one query).
pub async fn attachee_status(service: &ActrId, member_id: &str) -> Result<Attachee, sqlx::Error> {
    let mut conn = get_database().get_pool().acquire().await?;
    attachee_status_on(&mut conn, service, member_id).await
}

/// [`attachee_status`] on an existing connection.
pub async fn attachee_status_on(
    conn: &mut SqliteConnection,
    service: &ActrId,
    member_id: &str,
) -> Result<Attachee, sqlx::Error> {
    let row: Option<(String, bool)> = sqlx::query_as(
        "SELECT i.member_id,
                EXISTS(SELECT 1 FROM service_acl a
                       JOIN realm_member m
                         ON m.realm = a.realm AND m.member_id = a.member_id
                            AND m.status = 'active'
                       WHERE a.realm = i.realm AND a.serial_number = i.serial_number
                         AND a.member_id = ? AND a.revoked_at IS NULL
                         AND a.expires_at > ?)
         FROM issued_actor_identity i
         WHERE i.realm = ? AND i.serial_number = ?",
    )
    .bind(member_id)
    .bind(now_secs())
    .bind(&service.realm)
    .bind(service.serial_number as i64)
    .fetch_optional(conn)
    .await?;
    Ok(match row {
        None => Attachee::Unissued,
        Some((owner, _)) if owner == member_id => Attachee::Owner,
        Some((_, true)) => Attachee::Granted,
        Some((_, false)) => Attachee::Denied,
    })
}

/// Whether `member_id` may attach to `service`: the owner always may, an
/// unrevoked unexpired grant may, and an Actor this Provider did not issue
/// is not ACL-guarded here.
pub async fn service_attachee_allowed(
    service: &ActrId,
    member_id: &str,
) -> Result<bool, sqlx::Error> {
    Ok(attachee_status(service, member_id).await?.allowed())
}

/// [`service_attachee_allowed`] on an existing connection.
pub async fn service_attachee_allowed_on(
    conn: &mut SqliteConnection,
    service: &ActrId,
    member_id: &str,
) -> Result<bool, sqlx::Error> {
    Ok(attachee_status_on(conn, service, member_id)
        .await?
        .allowed())
}

/// Whether `target` is this Realm's profile-package Service: the Actor the
/// Realm Profile names (any version). Only that Actor is instance-guarded at
/// the relay — Fronts and helpers must relay freely once the type-level ACL
/// passes, or the service -> front return path of an authorized session
/// would die on the first SDP/ICE answer.
pub async fn relay_target_guarded(
    target: &ActrId,
    target_type: &actr_protocol::ActrType,
) -> Result<bool, sqlx::Error> {
    let mut conn = get_database().get_pool().acquire().await?;
    relay_target_guarded_with(&mut conn, target, target_type).await
}

/// Relay-layer instance ACL (FR-SEC-013): the Realm's profile-package
/// Service relays only for its owner and grantees; every other Actor passes
/// once the type-level ACL accepted the hop. A caller the Actor was not
/// issued to only pays the profile lookup when it is otherwise denied.
pub async fn relay_attachee_allowed(
    target: &ActrId,
    target_type: &actr_protocol::ActrType,
    member_id: &str,
) -> Result<bool, sqlx::Error> {
    let mut conn = get_database().get_pool().acquire().await?;
    if attachee_status_on(&mut conn, target, member_id)
        .await?
        .allowed()
    {
        return Ok(true);
    }
    // Otherwise denied: only a target outside the Realm's profile-package
    // Service still relays (Fronts and helpers).
    Ok(!relay_target_guarded_with(&mut conn, target, target_type).await?)
}

/// [`relay_target_guarded`] on an existing connection.
async fn relay_target_guarded_with(
    conn: &mut SqliteConnection,
    target: &ActrId,
    target_type: &actr_protocol::ActrType,
) -> Result<bool, sqlx::Error> {
    let Some(document) =
        sqlx::query_scalar::<_, String>("SELECT document FROM realm_profile WHERE realm = ?")
            .bind(&target.realm)
            .fetch_optional(conn)
            .await?
    else {
        return Ok(false);
    };
    let Ok(profile) = serde_json::from_str::<serde_json::Value>(&document) else {
        return Ok(false);
    };
    let Some(service) = profile["package"]["type"]
        .as_str()
        .and_then(|text| actr_protocol::ActrType::from_program_identity(text).ok())
    else {
        return Ok(false);
    };
    Ok(service.publisher == target_type.publisher && service.name == target_type.name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::realm::membership::{
        self, Limits, Signer, authenticate, client_key, create_invite, issue_owner_invite, join,
    };
    use crate::realm::model::Realm;
    use crate::util::test_utils::utils::setup_test_db;
    use actr_protocol::realm_membership::{RealmInvite, invite_secret_hash};
    use serial_test::serial;

    const REALM: &str = "k7m2q9xh@acme.com";
    const LIMITS: Limits = Limits {
        max_members: 5,
        max_clients_per_member: 3,
        max_open_invites: 6,
    };
    /// The serial of the fixture Service Actor.
    const SERVICE: u64 = 100;

    fn key(seed: u8) -> [u8; 32] {
        ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
            .verifying_key()
            .to_bytes()
    }

    async fn redeem(invite: &membership::IssuedInvite, seed: u8) -> Result<Value, MembershipError> {
        let secret = RealmInvite::decode(&invite.invite).unwrap().secret;
        join(
            REALM,
            &invite_secret_hash(&secret).unwrap(),
            &key(seed),
            "test",
            LIMITS,
        )
        .await
    }

    async fn signer_of(seed: u8) -> Signer {
        let key = client_key(
            REALM,
            &actr_protocol::realm_membership::client_key_id(&key(seed)),
        )
        .await
        .unwrap()
        .unwrap();
        authenticate(key).await.unwrap()
    }

    async fn grant_editor_invite_member(owner: &Signer) -> (membership::IssuedInvite, Signer) {
        let invite = create_invite(
            owner,
            Some(Role::Editor),
            None,
            chrono::Utc::now().timestamp() + 3600,
            LIMITS,
        )
        .await
        .unwrap();
        let joined = redeem(&invite, 2).await.unwrap();
        assert_eq!(joined["role"], "editor");
        (invite, signer_of(2).await)
    }

    /// A Realm whose owner holds the key of seed 1, a second member of seed
    /// 2, and a Service Actor bound to the owner.
    async fn fixture() -> (Signer, Signer) {
        setup_test_db().await.unwrap();
        Realm::create(REALM.to_string()).await.unwrap();
        let invite = issue_owner_invite(REALM, membership::BY_OPERATOR)
            .await
            .unwrap();
        redeem(&invite, 1).await.unwrap();
        let owner = signer_of(1).await;
        let (_, editor) = grant_editor_invite_member(&owner).await;
        sqlx::query(
            "INSERT INTO issued_actor_identity
                 (realm, serial_number, actor_type, member_id, client_key_id,
                  issued_at)
             VALUES (?, ?, 'acme.com/notes@1.0.0', ?, ?, ?)",
        )
        .bind(REALM)
        .bind(SERVICE as i64)
        .bind(&owner.member_id)
        .bind(&owner.key_id)
        .bind(now_secs())
        .execute(get_database().get_pool())
        .await
        .unwrap();
        (owner, editor)
    }

    fn service() -> ActrId {
        ActrId {
            realm: REALM.to_string(),
            serial_number: SERVICE,
        }
    }

    #[tokio::test]
    #[serial]
    async fn grant_revoke_and_enforce() {
        let (owner, editor) = fixture().await;
        let expires = now_secs() + 3600;
        let grant = acl_grant(&owner, SERVICE, &editor.member_id, Role::Editor, expires)
            .await
            .unwrap();
        assert_eq!(grant["role"], "editor");

        // Owner always attaches; a grantee attaches; a stranger does not.
        assert!(
            service_attachee_allowed(&service(), &owner.member_id)
                .await
                .unwrap()
        );
        assert!(
            service_attachee_allowed(&service(), &editor.member_id)
                .await
                .unwrap()
        );
        let invite = create_invite(&owner, Some(Role::Viewer), None, now_secs() + 3600, LIMITS)
            .await
            .unwrap();
        redeem(&invite, 3).await.unwrap();
        let stranger = signer_of(3).await;
        assert!(
            !service_attachee_allowed(&service(), &stranger.member_id)
                .await
                .unwrap()
        );

        acl_revoke(&owner, SERVICE, &editor.member_id)
            .await
            .unwrap();
        assert!(
            !service_attachee_allowed(&service(), &editor.member_id)
                .await
                .unwrap()
        );
        assert_eq!(
            code(acl_revoke(&owner, SERVICE, &editor.member_id).await),
            Code::NotFound
        );
    }

    #[tokio::test]
    #[serial]
    async fn only_the_owner_manages_the_acl() {
        let (owner, editor) = fixture().await;
        assert_eq!(
            code(
                acl_grant(
                    &editor,
                    SERVICE,
                    &owner.member_id,
                    Role::Viewer,
                    now_secs() + 60
                )
                .await
            ),
            Code::Forbidden
        );
        assert_eq!(
            code(acl_revoke(&editor, SERVICE, &editor.member_id).await),
            Code::Forbidden
        );
        assert_eq!(code(acl_list(&editor, SERVICE).await), Code::Forbidden);
    }

    #[tokio::test]
    #[serial]
    async fn grants_are_validated() {
        let (owner, editor) = fixture().await;
        // The owner needs no grant.
        assert_eq!(
            code(
                acl_grant(
                    &owner,
                    SERVICE,
                    &owner.member_id,
                    Role::Editor,
                    now_secs() + 60
                )
                .await
            ),
            Code::InvalidRequest
        );
        // No owner role on a grant.
        assert_eq!(
            code(
                acl_grant(
                    &owner,
                    SERVICE,
                    &editor.member_id,
                    Role::Owner,
                    now_secs() + 60
                )
                .await
            ),
            Code::InvalidRequest
        );
        // Expiry is required to be in the future.
        assert_eq!(
            code(
                acl_grant(
                    &owner,
                    SERVICE,
                    &editor.member_id,
                    Role::Viewer,
                    now_secs() - 1
                )
                .await
            ),
            Code::InvalidRequest
        );
        // ... and not absurdly far out.
        assert_eq!(
            code(
                acl_grant(
                    &owner,
                    SERVICE,
                    &editor.member_id,
                    Role::Viewer,
                    now_secs() + MAX_GRANT_SECS + 1
                )
                .await
            ),
            Code::InvalidRequest
        );
        // Unknown service serial.
        assert_eq!(
            code(
                acl_grant(
                    &owner,
                    999,
                    &editor.member_id,
                    Role::Viewer,
                    now_secs() + 60
                )
                .await
            ),
            Code::NotFound
        );
        // Unknown member.
        assert_eq!(
            code(
                acl_grant(
                    &owner,
                    SERVICE,
                    "0000000000000000",
                    Role::Viewer,
                    now_secs() + 60
                )
                .await
            ),
            Code::NotFound
        );
    }

    #[tokio::test]
    #[serial]
    async fn an_expired_grant_stops_allowing() {
        let (owner, editor) = fixture().await;
        acl_grant(
            &owner,
            SERVICE,
            &editor.member_id,
            Role::Viewer,
            now_secs() + 3600,
        )
        .await
        .unwrap();
        assert!(
            service_attachee_allowed(&service(), &editor.member_id)
                .await
                .unwrap()
        );
        sqlx::query("UPDATE service_acl SET expires_at = ? WHERE realm = ? AND serial_number = ?")
            .bind(now_secs() - 1)
            .bind(REALM)
            .bind(SERVICE as i64)
            .execute(get_database().get_pool())
            .await
            .unwrap();
        assert!(
            !service_attachee_allowed(&service(), &editor.member_id)
                .await
                .unwrap()
        );
        let listed = acl_list(&owner, SERVICE).await.unwrap();
        assert_eq!(listed["grants"][0]["state"], "expired");
    }

    #[tokio::test]
    #[serial]
    async fn unbound_actors_are_not_acl_guarded() {
        let (_owner, _editor) = fixture().await;
        let foreign = ActrId {
            realm: REALM.to_string(),
            serial_number: 999,
        };
        assert!(service_attachee_allowed(&foreign, "anyone").await.unwrap());
    }

    #[tokio::test]
    #[serial]
    async fn a_refresh_replaces_the_grant() {
        let (owner, editor) = fixture().await;
        acl_grant(
            &owner,
            SERVICE,
            &editor.member_id,
            Role::Viewer,
            now_secs() + 60,
        )
        .await
        .unwrap();
        acl_grant(
            &owner,
            SERVICE,
            &editor.member_id,
            Role::Editor,
            now_secs() + 3600,
        )
        .await
        .unwrap();
        let listed = acl_list(&owner, SERVICE).await.unwrap();
        assert_eq!(listed["grants"].as_array().unwrap().len(), 1);
        assert_eq!(listed["grants"][0]["role"], "editor");
        assert_eq!(listed["grants"][0]["state"], "active");
    }

    fn code<T: std::fmt::Debug>(result: Result<T, MembershipError>) -> Code {
        result.unwrap_err().code
    }
}
