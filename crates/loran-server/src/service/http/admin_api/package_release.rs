use super::{AdminApiState, grpc_status_to_response, realm_writes_disabled_response};
use actr_protocol::realm_profile::{FrontUpdate, PackageSelectionChange, RealmPackageSelection};
use axum::{
    Json,
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use platform::realm::RealmPackageReleaseRecord;
use serde::Deserialize;
use serde_json::json;
use std::{sync::Arc, time::Duration};
use tokio::process::Command;
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
    if let Some(receipt) = RealmPackageReleaseRecord::get(realm, &operation, &selection)
        .await
        .map_err(|error| Status::aborted(error.to_string()))?
    {
        return receipt
            .reply(realm, &operation)
            .map_err(|error| Status::internal(error.to_string()));
    }
    let snapshot = state.service.realm_package_state(realm).await?;
    if snapshot.sequence != operation.expected_sequence {
        return Err(Status::aborted(
            "The Profile changed. Reload the selection before submitting again.",
        ));
    }
    if operation.rollback && snapshot.previous_selection.as_ref() != Some(&selection) {
        return Err(Status::aborted(
            "There is no matching previous package selection to restore.",
        ));
    }
    let kind = snapshot
        .selection
        .release_change(&selection)
        .map_err(Status::invalid_argument)?;
    let Some(kind) = kind else {
        return Ok(json!({"realm": realm, "sequence": snapshot.sequence, "changed": false}));
    };
    verify_packages(&selection, kind).await?;
    let mut result = state
        .service
        .publish_verified_packages(realm, selection, operation, kind)
        .await?;
    result["changed"] = json!(true);
    Ok(result)
}

async fn verify_packages(
    selection: &RealmPackageSelection,
    kind: PackageSelectionChange,
) -> Result<(), Status> {
    let executable = std::env::var_os("LORAN_PACKAGE_VERIFIER").ok_or_else(|| {
        Status::unavailable("Configure LORAN_PACKAGE_VERIFIER with the actr executable.")
    })?;
    let work = tempfile::tempdir().map_err(|error| Status::internal(error.to_string()))?;
    let selection_path = work.path().join("selection.json");
    let bytes = serde_json::to_vec(selection)
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
    tokio::fs::write(&selection_path, bytes)
        .await
        .map_err(|error| Status::internal(error.to_string()))?;
    let mut command = Command::new(executable);
    command
        .kill_on_drop(true)
        .args(["realm", "check-selection", "--selection"])
        .arg(selection_path)
        .arg("--output")
        .arg(work.path().join("verified"));
    let output = tokio::time::timeout(Duration::from_secs(120), command.output())
        .await
        .map_err(|_| Status::unavailable("Package verification timed out."))?
        .map_err(|error| {
            Status::unavailable(format!("Could not start the package verifier: {error}"))
        })?;
    if !output.status.success() {
        let details: String = String::from_utf8_lossy(&output.stderr)
            .chars()
            .take(1800)
            .collect();
        return Err(Status::invalid_argument(details));
    }
    let report: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| Status::unavailable("Invalid package verification result."))?;
    let actual = report
        .get("selection")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok());
    if actual.as_ref() != Some(selection) {
        return Err(Status::unavailable(
            "The package verifier returned another selection.",
        ));
    }
    if kind == PackageSelectionChange::Service && report["front"].as_str().is_none() {
        return Err(Status::invalid_argument(
            "A Service release needs a downloadable Front archive for RPC contract verification.",
        ));
    }
    Ok(())
}
