use super::{AdminApiState, grpc_status_to_response, realm_writes_disabled_response};
use actr_protocol::realm_profile::{FrontUpdate, RealmPackageSelection};
use axum::{
    Json,
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tonic::Status;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReleaseRequest {
    operation: FrontUpdate,
    selection: RealmPackageSelection,
    // Older admin tabs may still send this deployment setting.
    #[serde(rename = "readiness")]
    _legacy_readiness: Option<serde::de::IgnoredAny>,
}

pub(super) async fn current(
    State(state): State<Arc<AdminApiState>>,
    Path(realm): Path<String>,
) -> Response {
    match state.service.realm_package_state(&realm).await {
        Ok(snapshot) => Json(snapshot).into_response(),
        Err(error) => grpc_status_to_response(error),
    }
}

pub(super) async fn submit(
    State(state): State<Arc<AdminApiState>>,
    Path(realm): Path<String>,
    Json(request): Json<ReleaseRequest>,
) -> Response {
    if !state.realm_writes_enabled {
        return realm_writes_disabled_response();
    }
    match release(&state, &realm, request).await {
        Ok(result) => Json(result).into_response(),
        Err(error) => grpc_status_to_response(error),
    }
}

async fn release(
    state: &AdminApiState,
    realm: &str,
    request: ReleaseRequest,
) -> Result<serde_json::Value, Status> {
    let ReleaseRequest {
        operation,
        selection,
        ..
    } = request;
    operation.check().map_err(Status::invalid_argument)?;
    selection.check().map_err(Status::invalid_argument)?;
    if let Some(receipt) = state
        .service
        .package_release_receipt(realm, &operation, &selection)
        .await?
    {
        return Ok(receipt);
    }
    let snapshot = state.service.realm_package_state(realm).await?;
    let conflict = if snapshot.sequence != operation.expected_sequence {
        Some("The Profile changed. Reload the selection and try again.")
    } else if operation.rollback && snapshot.previous_selection.as_ref() != Some(&selection) {
        Some("There is no previous selection to roll back to.")
    } else {
        None
    };
    if let Some(message) = conflict {
        // An identical request may have committed while the snapshot loaded.
        if let Some(receipt) = state
            .service
            .package_release_receipt(realm, &operation, &selection)
            .await?
        {
            return Ok(receipt);
        }
        return Err(Status::aborted(message));
    }
    let kind = snapshot
        .selection
        .release_change(&selection)
        .map_err(Status::invalid_argument)?;
    let Some(kind) = kind else {
        return Ok(json!({"realm": realm, "sequence": snapshot.sequence, "changed": false}));
    };
    let mut result = state
        .service
        .publish_packages(realm, selection, operation, kind)
        .await?;
    result["changed"] = json!(true);
    Ok(result)
}
