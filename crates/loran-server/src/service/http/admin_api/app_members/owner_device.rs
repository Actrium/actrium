use super::{
    AdminApiState, AppManagement, Error, Operation, Result, conflict, invalid, invitations,
    management, realm_writes_disabled_response, receipt, reply, unavailable,
};
use actr_protocol::realm_membership::{
    MembershipOp, MembershipRequest, Role, body_sha256, invite_secret_hash,
};
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::StatusCode,
    response::Response,
};
use chrono::{DateTime, Utc};
use platform::{
    realm::membership::{self, OwnerDevice},
    storage::db::get_database,
};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

const INVITE_SECS: i64 = 600;

fn checked(
    admin: &AppManagement,
    realm: &str,
    body: &[u8],
    op: MembershipOp,
) -> Result<MembershipRequest> {
    let request =
        MembershipRequest::parse(body).map_err(|_| invalid("Invalid signed membership request"))?;
    if request.realm != realm || request.op != op {
        return Err(invalid("Signed operation must match this App and route"));
    }
    request
        .check(&admin.provider, Utc::now())
        .map_err(|_| invalid("Invalid membership proof"))?;
    Ok(request)
}

pub(in crate::service::http::admin_api) async fn join_owner_device(
    State(state): State<Arc<AdminApiState>>,
    Path(realm): Path<String>,
    body: Bytes,
) -> Result<Response> {
    if !state.realm_writes_enabled {
        return Ok(realm_writes_disabled_response());
    }
    let admin = management(&state)?;
    let request = checked(admin, &realm, &body, MembershipOp::Join)?;
    let key = request
        .verify_join()
        .map_err(|_| invalid("Invalid device signature"))?;
    let hash = request
        .invite
        .as_deref()
        .and_then(|secret| invite_secret_hash(secret).ok())
        .ok_or_else(|| invalid("Invalid app invitation"))?;
    let expires = DateTime::parse_from_rfc3339(&request.expires_at)
        .map_err(|_| invalid("Invalid request expiration"))?
        .timestamp();
    if !platform::realm::management::use_nonce(&request.nonce, expires)
        .await
        .map_err(|_| unavailable())?
    {
        return Err(conflict("The device proof was already used"));
    }
    let owner = request
        .member_id
        .as_deref()
        .map_or(OwnerDevice::First, OwnerDevice::Existing);
    let label = request.label.as_deref().unwrap_or("AskAway Console");
    if label.chars().count() > actr_protocol::realm_membership::MAX_LABEL_CHARS {
        return Err(invalid("Device label is too long"));
    }
    Ok(reply(
        membership::join_owner_device(&realm, &hash, &key, label, admin.limits, owner).await?,
    ))
}

pub(in crate::service::http::admin_api) async fn owner_device_status(
    Path((realm, key_id)): Path<(String, String)>,
) -> Result<Response> {
    let key = membership::client_key(&realm, &key_id).await?;
    Ok(reply(json!({
        "member_id": key.as_ref().map(|key| &key.member_id),
        "key_id": key.as_ref().map(|key| &key.key_id),
        "connected": key.is_some_and(|key| key.member_active && !key.revoked && key.role == Role::Owner)
    })))
}

pub(in crate::service::http::admin_api) async fn issue_owner_device(
    State(state): State<Arc<AdminApiState>>,
    Path(realm): Path<String>,
    body: Bytes,
) -> Result<Response> {
    deliver(state, realm, body, false).await
}

pub(in crate::service::http::admin_api) async fn ack_owner_device(
    State(state): State<Arc<AdminApiState>>,
    Path(realm): Path<String>,
    body: Bytes,
) -> Result<Response> {
    deliver(state, realm, body, true).await
}

async fn deliver(
    state: Arc<AdminApiState>,
    realm: String,
    body: Bytes,
    ack: bool,
) -> Result<Response> {
    if !state.realm_writes_enabled {
        return Ok(realm_writes_disabled_response());
    }
    let admin = management(&state)?;
    let mut request = checked(admin, &realm, &body, MembershipOp::CreateInvite)?;
    let prefix = if ack {
        "console-ack:"
    } else {
        "console-issue:"
    };
    let id = request
        .nonce
        .strip_prefix(prefix)
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or_else(|| invalid("Invalid signed operation ID"))?
        .to_string();
    let denied = || {
        Error(
            StatusCode::FORBIDDEN,
            "An active Owner device must sign this request".into(),
        )
    };
    let key = membership::client_key(&realm, request.signer_key_id())
        .await?
        .ok_or_else(denied)?;
    request.verify(&key.public_key).map_err(|_| denied())?;
    membership::require_owner(&membership::authenticate(key.clone()).await?)?;
    if request.member_id.as_deref() != Some(&key.member_id)
        || request.role.is_some()
        || request.invite_expires_at.is_some()
    {
        return Err(invalid("Supply the signing device's Owner member ID"));
    }
    admin.ready().await?;
    let fingerprint = body_sha256(format!("console-owner-device/v1\0{}", key.member_id).as_bytes());
    let operation = Operation {
        id: &id,
        provider: &admin.provider,
        realm: &realm,
        key_id: &key.key_id,
        fingerprint: &fingerprint,
    };
    if ack {
        if let Some(stored) = admin.operation(&id).await? {
            operation.require_same(&stored)?;
            admin.clear_receipt(&realm, &key.key_id, &id).await?;
        }
        return Ok(reply(json!({"dismissed":true})));
    }
    if let Some(stored) = admin.operation(&id).await? {
        operation.require_same(&stored)?;
        return receipt(admin, &realm, &key.key_id, &id, stored).await;
    }
    // The signature authorizes only this device's own member, for the server's fixed TTL.
    request.invite_expires_at = Some(membership::rfc3339(Utc::now().timestamp() + INVITE_SECS));
    let db = get_database().get_pool();
    if let Some(value) = invitations::issue(
        db,
        &admin.delivery,
        &operation,
        &request,
        admin.limits,
        true,
    )
    .await?
    {
        return Ok(reply(value));
    }
    let stored = admin.operation(&id).await?.ok_or_else(unavailable)?;
    operation.require_same(&stored)?;
    receipt(admin, &realm, &key.key_id, &id, stored).await
}
