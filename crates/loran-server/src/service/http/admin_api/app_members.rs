//! The Admin acts as a registered device of one app's Owner. The Signer holds
//! its private key. The AFP signs membership operations with that device.
//! Local invitation delivery and strict redemption use the membership store.

mod invitations;
use super::{AdminApiState, realm_writes_disabled_response};
use actr_protocol::realm_membership::{
    MembershipOp, MembershipRequest, RealmInvite, Role, body_sha256, client_key_id, invite_id,
    invite_secret_hash,
};
use actr_protocol::realm_profile::{RealmPackageSelection, jcs};
use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::{ConnectInfo, Path, Request, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::Utc;
use invitations::{Operation, StoredOperation};
use issuer::{
    AIdIssuer,
    handlers::IssuerState,
    realm_profile::RealmProfiles,
    signer_client_wrapper::{SignerClientWrapper, create_signer_client},
};
use platform::{
    config::{LoranConfig, realm::DevJoinMode},
    realm::membership,
    storage::db::get_database,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use signer::crypto::KeyEncryptor;
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::{Mutex, OnceCell};
use tower_service::Service;
use uuid::Uuid;

pub(crate) struct AppManagement {
    provider: String,
    signer: SignerClientWrapper,
    issuer: Router,
    limits: membership::Limits,
    invite_mode: DevJoinMode,
    db: SqlitePool,
    schema: OnceCell<()>,
    delivery: KeyEncryptor,
    attach_lock: Mutex<()>,
}

#[derive(Debug)]
pub(super) struct Error(StatusCode, String);
type Result<T> = std::result::Result<T, Error>;

fn invalid(message: &str) -> Error {
    Error(StatusCode::BAD_REQUEST, message.into())
}
fn conflict(message: &str) -> Error {
    Error(StatusCode::CONFLICT, message.into())
}
fn unavailable() -> Error {
    Error(
        StatusCode::SERVICE_UNAVAILABLE,
        "App management storage or signing service is unavailable".into(),
    )
}
impl From<sqlx::Error> for Error {
    fn from(_: sqlx::Error) -> Self {
        unavailable()
    }
}
impl From<membership::MembershipError> for Error {
    fn from(error: membership::MembershipError) -> Self {
        if error.code == membership::Code::Unavailable {
            return unavailable();
        }
        Self(
            StatusCode::from_u16(error.code.status()).expect("membership HTTP status"),
            error.message,
        )
    }
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (
            self.0,
            [(header::CACHE_CONTROL, "no-store")],
            Json(json!({"error": self.1})),
        )
            .into_response()
    }
}
fn reply(value: Value) -> Response {
    ([(header::CACHE_CONTROL, "no-store")], Json(value)).into_response()
}
fn management(state: &AdminApiState) -> Result<&AppManagement> {
    state.app_management.as_deref().ok_or_else(|| {
        Error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Owner management requires a configured Issuer and Signer on this node".into(),
        )
    })
}

impl AppManagement {
    pub(crate) async fn configured(
        config: &LoranConfig,
        issuer: Option<Arc<AIdIssuer>>,
        profiles: Option<Arc<RealmProfiles>>,
    ) -> anyhow::Result<Option<Self>> {
        let Some((issuer, signer_config)) = issuer.zip(
            config
                .services
                .issuer
                .as_ref()
                .and_then(|s| s.get_signer_client_config(config)),
        ) else {
            return Ok(None);
        };
        let Some(provider) = profiles
            .as_ref()
            .map(|p| p.provider_id().to_owned())
            .or_else(|| config.authority.provider_id().ok())
        else {
            return Ok(None);
        };
        let signer = create_signer_client(&signer_config, &config.loran_shared_key).await?;
        let issuer = issuer::create_router_from_state(
            IssuerState::from_shared_issuer(issuer).with_membership(&config.realm, profiles),
        );
        let db = SqlitePool::connect_lazy_with(
            SqliteConnectOptions::new()
                .filename(config.sqlite_path.join("admin-apps.sqlite3"))
                .create_if_missing(true)
                .busy_timeout(std::time::Duration::from_secs(10)),
        );
        // Domain-separated from all protocol keys. Only the short-lived delivery
        // receipt uses this key. The device key always stays inside the Signer.
        let kek = body_sha256(
            format!(
                "loran-admin/invite-delivery/v1\0{}",
                config.loran_shared_key
            )
            .as_bytes(),
        );
        Ok(Some(Self {
            provider,
            signer,
            issuer,
            limits: membership::Limits::from(&config.realm),
            invite_mode: config.realm.dev_join.mode,
            db,
            schema: OnceCell::new(),
            delivery: KeyEncryptor::from_kek(&kek)?,
            attach_lock: Mutex::new(()),
        }))
    }

    async fn ready(&self) -> Result<()> {
        self.schema
            .get_or_try_init(|| async {
                sqlx::query(
                    "CREATE TABLE IF NOT EXISTS admin_clients (
                        provider TEXT NOT NULL, realm TEXT NOT NULL, invite_id TEXT NOT NULL,
                        signer_id INTEGER NOT NULL, key_id TEXT NOT NULL, PRIMARY KEY(provider, realm))",
                )
                .execute(&self.db)
                .await?;
                // Existing displays can finish retrieving or dismissing a receipt
                // from the old journal. All new operations use loran.db.
                invitations::initialize(&self.db).await?;
                invitations::initialize(get_database().get_pool()).await
            })
            .await?;
        for pool in [get_database().get_pool(), &self.db] {
            sqlx::query(
                "UPDATE admin_operations SET receipt=NULL WHERE receipt IS NOT NULL AND expires<=?",
            )
            .bind(Utc::now().timestamp())
            .execute(pool)
            .await?;
        }
        Ok(())
    }

    async fn operation(&self, id: &str) -> Result<Option<StoredOperation>> {
        let active = invitations::load(get_database().get_pool(), id).await?;
        if active.is_some() {
            return Ok(active);
        }
        invitations::load(&self.db, id).await
    }

    async fn clear_receipt(&self, realm: &str, key: &str, id: &str) -> Result<()> {
        for db in [get_database().get_pool(), &self.db] {
            sqlx::query("UPDATE admin_operations SET receipt=NULL WHERE id=? AND provider=? AND realm=? AND key_id=?")
                .bind(id)
                .bind(&self.provider)
                .bind(realm)
                .bind(key)
                .execute(db)
                .await?;
        }
        Ok(())
    }

    async fn binding(&self, realm: &str) -> Result<Option<(String, u32, String)>> {
        Ok(sqlx::query_as(
            "SELECT invite_id,signer_id,key_id FROM admin_clients WHERE provider=? AND realm=?",
        )
        .bind(&self.provider)
        .bind(realm)
        .fetch_optional(&self.db)
        .await?)
    }

    async fn owner(&self, realm: &str) -> Result<Option<(u32, membership::ClientKey)>> {
        let Some((_, signer_id, id)) = self.binding(realm).await? else {
            return Ok(None);
        };
        let client = membership::client_key(realm, &id)
            .await
            .map_err(|_| unavailable())?;
        Ok(client
            .filter(|k| k.member_active && !k.revoked && k.role == Role::Owner)
            .map(|k| (signer_id, k)))
    }

    fn request(&self, op: MembershipOp, realm: &str) -> MembershipRequest {
        MembershipRequest::new(
            op,
            realm,
            &self.provider,
            &Uuid::new_v4().simple().to_string(),
        )
    }

    async fn signed_document(
        &self,
        request: &MembershipRequest,
        signer_id: u32,
        key: &str,
    ) -> Result<Value> {
        let mut doc = serde_json::to_value(request).map_err(|_| unavailable())?;
        let signature = self
            .signer
            .sign(signer_id, jcs(&doc).as_bytes())
            .await
            .map_err(|_| unavailable())?;
        doc["signature"] =
            json!({"algorithm":"Ed25519","key_id":key,"value":STANDARD.encode(signature)});
        Ok(doc)
    }

    async fn checked_request(
        &self,
        request: &MembershipRequest,
        signer_id: u32,
        key: &str,
    ) -> Result<MembershipRequest> {
        let doc = self.signed_document(request, signer_id, key).await?;
        let checked =
            MembershipRequest::parse(doc.to_string().as_bytes()).map_err(|_| unavailable())?;
        checked
            .check(&self.provider, Utc::now())
            .map_err(|_| unavailable())?;
        Ok(checked)
    }

    async fn send(
        &self,
        request: MembershipRequest,
        signer_id: u32,
        key: &str,
        peer: ConnectInfo<SocketAddr>,
    ) -> Result<Value> {
        let doc = self.signed_document(&request, signer_id, key).await?;
        let mut req = Request::post("/membership")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(doc.to_string()))
            .map_err(|_| unavailable())?;
        req.extensions_mut().insert(peer);
        let res = self
            .issuer
            .clone()
            .call(req)
            .await
            .expect("Axum router is infallible");
        let status = res.status();
        let bytes = to_bytes(res.into_body(), 256 * 1024)
            .await
            .map_err(|_| unavailable())?;
        if !status.is_success() {
            let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
            return Err(Error(
                status,
                value["message"]
                    .as_str()
                    .unwrap_or("Issuer rejected the operation")
                    .to_owned(),
            ));
        }
        serde_json::from_slice(&bytes).map_err(|_| unavailable())
    }
}

pub(super) async fn status(
    State(state): State<Arc<AdminApiState>>,
    Path(realm): Path<String>,
) -> Result<Response> {
    let Some(admin) = &state.app_management else {
        return Ok(reply(json!({"available":false,"connected":false})));
    };
    admin.ready().await?;
    let owner = admin.owner(&realm).await?;
    Ok(reply(
        json!({"available":true, "connected":owner.is_some(), "member_id":owner.as_ref().map(|(_,c)|&c.member_id), "key_id":owner.as_ref().map(|(_,c)|&c.key_id)}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Attach {
    invite: String,
}

pub(super) async fn attach(
    State(state): State<Arc<AdminApiState>>,
    Path(realm): Path<String>,
    peer: ConnectInfo<SocketAddr>,
    Json(body): Json<Attach>,
) -> Result<Response> {
    if !state.realm_writes_enabled {
        return Ok(realm_writes_disabled_response());
    }
    let admin = management(&state)?;
    let invite =
        RealmInvite::decode(&body.invite).map_err(|_| invalid("Invalid app invitation"))?;
    if invite.realm != realm {
        return Err(invalid("This invitation belongs to another app"));
    }
    let id = invite_id(&invite.secret).map_err(|_| invalid("Invalid app invitation"))?;
    let _guard = admin.attach_lock.lock().await;
    admin.ready().await?;
    if let Some((_, client)) = admin.owner(&realm).await? {
        return Ok(reply(
            json!({"connected":true,"member_id":client.member_id,"key_id":client.key_id}),
        ));
    }
    let members = state
        .service
        .list_realm_members_direct(&realm)
        .await
        .map_err(|s| Error(StatusCode::CONFLICT, s.message().into()))?;
    let owner = members["members"]
        .as_array()
        .and_then(|list| {
            list.iter()
                .find(|m| m["role"] == "owner" && m["status"] == "active")
        })
        .and_then(|m| m["member_id"].as_str())
        .ok_or_else(|| {
            conflict("The Owner must redeem the first invite before authorizing Admin")
        })?;
    let authorized = members["invites"].as_array().is_some_and(|list| {
        list.iter()
            .any(|i| i["invite_id"] == id && i["member_id"] == owner && i["role"].is_null())
    });
    if !authorized {
        return Err(conflict(
            "Use an unused add-device invite issued for the current Owner",
        ));
    }
    let name = format!(
        "loran-admin-device/{}",
        body_sha256(format!("{}\0{realm}\0{id}", admin.provider).as_bytes())
    );
    let (signer_id, public_key) = admin
        .signer
        .ensure_named_key(&name)
        .await
        .map_err(|_| unavailable())?;
    let key = client_key_id(&public_key);
    // Persist the binding before redemption. Losing the reply cannot strand a
    // registered device, and retries always use the same named signing key.
    sqlx::query("INSERT INTO admin_clients(provider,realm,invite_id,signer_id,key_id) VALUES(?,?,?,?,?) ON CONFLICT(provider,realm) DO UPDATE SET invite_id=excluded.invite_id,signer_id=excluded.signer_id,key_id=excluded.key_id")
        .bind(&admin.provider)
        .bind(&realm)
        .bind(&id)
        .bind(signer_id)
        .bind(&key)
        .execute(&admin.db)
        .await?;
    let mut req = admin.request(MembershipOp::Join, &realm);
    req.label = Some("Loran Admin".into());
    req.invite = Some(invite.secret);
    req.public_key = Some(STANDARD.encode(public_key));
    let checked = admin.checked_request(&req, signer_id, &key).await?;
    let verified_key = checked.verify_join().map_err(|_| unavailable())?;
    let expires = chrono::DateTime::parse_from_rfc3339(&checked.expires_at)
        .map_err(|_| unavailable())?
        .timestamp();
    if !platform::realm::management::use_nonce(&checked.nonce, expires)
        .await
        .map_err(|_| unavailable())?
    {
        return Err(conflict("The device proof was already used"));
    }
    let hash = invite_secret_hash(checked.invite.as_deref().ok_or_else(unavailable)?)
        .map_err(|_| invalid("Invalid app invitation"))?;
    // The verified invite is for an existing Owner's device. Redeem it
    // strictly: the public Join route can otherwise admit a new Viewer.
    let joined = membership::join_with_mode(
        &realm,
        &hash,
        &verified_key,
        "Loran Admin",
        admin.limits,
        admin.invite_mode,
    )
    .await?;
    if joined["role"] != "owner" || joined["member_id"] != owner || joined["key_id"] != key {
        let mut remove = admin.request(MembershipOp::RevokeClient, &realm);
        remove.key_id = Some(key.clone());
        admin.send(remove, signer_id, &key, peer).await?;
        return Err(conflict("The device did not join the current Owner"));
    }
    Ok(reply(
        json!({"connected":true,"member_id":owner,"key_id":key}),
    ))
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Action {
    InviteMember {
        role: Role,
        days: u8,
    },
    InviteOwnerDevice {
        days: u8,
    },
    RevokeInvite {
        invite_id: String,
    },
    RevokeClient {
        key_id: String,
    },
    SetRole {
        member_id: String,
        role: Role,
    },
    RemoveMember {
        member_id: String,
    },
    TransferOwner {
        member_id: String,
    },
    UpdateApp {
        display_name: Option<String>,
        selection: Option<RealmPackageSelection>,
    },
}

impl Action {
    fn request(
        &self,
        admin: &AppManagement,
        realm: &str,
        owner: &str,
    ) -> Result<MembershipRequest> {
        use MembershipOp as Op;
        let mut req = admin.request(
            match self {
                Self::InviteMember { .. } | Self::InviteOwnerDevice { .. } => Op::CreateInvite,
                Self::RevokeInvite { .. } => Op::RevokeInvite,
                Self::RevokeClient { .. } => Op::RevokeClient,
                Self::SetRole { .. } => Op::SetRole,
                Self::RemoveMember { .. } => Op::RemoveMember,
                Self::TransferOwner { .. } => Op::TransferOwner,
                Self::UpdateApp { .. } => Op::UpdateRealm,
            },
            realm,
        );
        match self {
            Self::InviteMember { role, .. } | Self::SetRole { role, .. }
                if *role == Role::Owner =>
            {
                return Err(invalid("Choose Editor or Viewer"));
            }
            Self::InviteMember { role, .. } => req.role = Some(*role),
            Self::InviteOwnerDevice { .. } => req.member_id = Some(owner.to_owned()),
            Self::RevokeInvite { invite_id } => req.invite_id = Some(invite_id.clone()),
            Self::RevokeClient { key_id } => req.key_id = Some(key_id.clone()),
            Self::SetRole { member_id, role } => {
                req.member_id = Some(member_id.clone());
                req.role = Some(*role);
            }
            Self::RemoveMember { member_id } | Self::TransferOwner { member_id } => {
                req.member_id = Some(member_id.clone())
            }
            Self::UpdateApp {
                display_name,
                selection,
            } => {
                if let Some(selection) = selection {
                    selection.check().map_err(|msg| invalid(&msg))?;
                }
                req.display_name = display_name.clone();
                req.selection = selection.clone();
            }
        }
        if let Self::InviteMember { days, .. } | Self::InviteOwnerDevice { days } = self {
            if !(1..=30).contains(days) {
                return Err(invalid("Choose 1 to 30 days"));
            }
            req.invite_expires_at =
                Some((Utc::now() + chrono::Duration::days(i64::from(*days))).to_rfc3339());
        }
        Ok(req)
    }
}

#[derive(Deserialize)]
pub(super) struct Command {
    request_id: Uuid,
    #[serde(flatten)]
    action: Action,
}

pub(super) async fn action(
    State(state): State<Arc<AdminApiState>>,
    Path(realm): Path<String>,
    peer: ConnectInfo<SocketAddr>,
    Json(cmd): Json<Command>,
) -> Result<Response> {
    if !state.realm_writes_enabled {
        return Ok(realm_writes_disabled_response());
    }
    let admin = management(&state)?;
    admin.ready().await?;
    let (signer, key) = admin
        .owner(&realm)
        .await?
        .ok_or_else(|| conflict("An active Owner must authorize the Admin device"))?;
    let fingerprint = body_sha256(
        serde_json::to_string(&cmd.action)
            .map_err(|_| unavailable())?
            .as_bytes(),
    );
    let id = cmd.request_id.to_string();
    let operation = Operation {
        id: &id,
        provider: &admin.provider,
        realm: &realm,
        key_id: &key.key_id,
        fingerprint: &fingerprint,
    };
    if let Some(stored) = admin.operation(&id).await? {
        operation.require_same(&stored)?;
        return receipt(admin, &realm, &key.key_id, &id, stored).await;
    }

    let req = cmd.action.request(admin, &realm, &key.member_id)?;
    let db = get_database().get_pool();
    let issued = if req.op == MembershipOp::CreateInvite {
        let request = admin.checked_request(&req, signer, &key.key_id).await?;
        request.verify(&key.public_key).map_err(|_| unavailable())?;
        invitations::issue(db, &admin.delivery, &operation, &request, admin.limits).await?
    } else {
        let first = {
            let mut conn = db.acquire().await?;
            operation.reserve(&mut conn, Utc::now().timestamp()).await?
        };
        if first {
            let result = admin.send(req, signer, &key.key_id, peer).await?;
            let mut conn = db.acquire().await?;
            invitations::store_receipt(&mut conn, &admin.delivery, &id, &result).await?;
            Some(result)
        } else {
            None
        }
    };
    if let Some(result) = issued {
        return Ok(reply(result));
    }
    // Another request won the operation ID. Never issue a second invite.
    let stored = admin.operation(&id).await?.ok_or_else(unavailable)?;
    operation.require_same(&stored)?;
    receipt(admin, &realm, &key.key_id, &id, stored).await
}

async fn receipt(
    admin: &AppManagement,
    realm: &str,
    key: &str,
    id: &str,
    stored: StoredOperation,
) -> Result<Response> {
    let none = || {
        conflict(
            "No delivery receipt is available. Refresh members and open invites before issuing another invitation.",
        )
    };
    if !stored.belongs_to(&admin.provider, realm, key) || stored.expires <= Utc::now().timestamp() {
        return Err(none());
    }
    let encrypted = stored.receipt.ok_or_else(none)?;
    let plain = admin
        .delivery
        .decrypt(&encrypted)
        .map_err(|_| unavailable())?;
    let result: Value = serde_json::from_str(&plain).map_err(|_| unavailable())?;
    if let Some(invite_id) = result["invite_id"]
        .as_str()
        .filter(|_| result["invite"].is_string())
    {
        let open: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM realm_invite WHERE realm=? AND invite_id=?
                            AND used_at IS NULL AND revoked_at IS NULL AND expires_at>?)",
        )
        .bind(realm)
        .bind(invite_id)
        .bind(Utc::now().timestamp())
        .fetch_one(get_database().get_pool())
        .await?;
        if !open {
            admin.clear_receipt(realm, key, id).await?;
            return Err(conflict("This invitation is no longer open"));
        }
    }
    Ok(reply(result))
}

pub(super) async fn reveal(
    State(state): State<Arc<AdminApiState>>,
    Path((realm, id)): Path<(String, Uuid)>,
) -> Result<Response> {
    if !state.realm_writes_enabled {
        return Ok(realm_writes_disabled_response());
    }
    let admin = management(&state)?;
    admin.ready().await?;
    let (_, key) = admin
        .owner(&realm)
        .await?
        .ok_or_else(|| conflict("Owner authorization is required"))?;
    let id = id.to_string();
    let stored = admin
        .operation(&id)
        .await?
        .ok_or_else(|| conflict("No delivery receipt is available"))?;
    receipt(admin, &realm, &key.key_id, &id, stored).await
}

pub(super) async fn dismiss(
    State(state): State<Arc<AdminApiState>>,
    Path((realm, id)): Path<(String, Uuid)>,
) -> Result<Response> {
    if !state.realm_writes_enabled {
        return Ok(realm_writes_disabled_response());
    }
    let admin = management(&state)?;
    admin.ready().await?;
    let (_, key) = admin
        .owner(&realm)
        .await?
        .ok_or_else(|| conflict("Owner authorization is required"))?;
    admin
        .clear_receipt(&realm, &key.key_id, &id.to_string())
        .await?;
    Ok(reply(json!({"dismissed":true})))
}
