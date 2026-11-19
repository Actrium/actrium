use super::{AdminApiState, app_members, grpc_status_to_response, realm_writes_disabled_response};
use actr_protocol::realm_profile::{FrontUpdate, PackageSelectionChange, RealmPackageSelection};
use actr_protocol::uri::parse_realm_name;
use axum::{
    Json,
    extract::{ConnectInfo, Path, State},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
use std::{net::SocketAddr, path::PathBuf, process::Stdio, sync::Arc, time::Duration};
use tokio::{process::Command, sync::Semaphore};
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
    peer: ConnectInfo<SocketAddr>,
    Json(request): Json<ReleaseRequest>,
) -> Response {
    if !state.realm_writes_enabled {
        return realm_writes_disabled_response();
    }
    match release(&state, &realm, peer, request).await {
        Ok(result) => Json(result).into_response(),
        Err(response) => *response,
    }
}

async fn release(
    state: &AdminApiState,
    realm: &str,
    peer: ConnectInfo<SocketAddr>,
    request: ReleaseRequest,
) -> Result<serde_json::Value, Box<Response>> {
    let ReleaseRequest {
        operation,
        selection,
        ..
    } = request;
    let bad_request = |reason| grpc_status_to_response(Status::invalid_argument(reason));
    operation.check().map_err(bad_request)?;
    selection.check().map_err(bad_request)?;
    if let Some(receipt) = state
        .service
        .package_release_receipt(realm, &operation, &selection)
        .await
        .map_err(grpc_status_to_response)?
    {
        app_members::package_release_owner(state, realm)
            .await
            .map_err(IntoResponse::into_response)?;
        return Ok(receipt);
    }
    let snapshot = state
        .service
        .realm_package_state(realm)
        .await
        .map_err(grpc_status_to_response)?;
    let conflict = if snapshot.sequence != operation.expected_sequence {
        Some("The Profile changed. Reload the selection before submitting again.")
    } else if operation.rollback && snapshot.previous_selection.as_ref() != Some(&selection) {
        Some("There is no matching previous package selection to restore.")
    } else {
        None
    };
    if let Some(message) = conflict {
        // An identical request may have committed while the snapshot loaded.
        if let Some(receipt) = state
            .service
            .package_release_receipt(realm, &operation, &selection)
            .await
            .map_err(grpc_status_to_response)?
        {
            app_members::package_release_owner(state, realm)
                .await
                .map_err(IntoResponse::into_response)?;
            return Ok(receipt);
        }
        return Err(grpc_status_to_response(Status::aborted(message)).into());
    }
    let kind = snapshot
        .selection
        .release_change(&selection)
        .map_err(bad_request)?;
    let Some(kind) = kind else {
        return Ok(json!({"realm": realm, "sequence": snapshot.sequence, "changed": false}));
    };
    let owner = app_members::package_release_owner(state, realm)
        .await
        .map_err(IntoResponse::into_response)?;
    if kind == PackageSelectionChange::Service {
        check_service_readiness(realm, &selection)
            .await
            .map_err(grpc_status_to_response)?;
    }
    // The Issuer verifies archives again with persistent Publisher pins and
    // compares the sequence and the Owner when storing the signed Profile.
    let mut result = owner
        .publish(realm, selection, operation, kind, peer)
        .await
        .map_err(IntoResponse::into_response)?;
    result["changed"] = json!(true);
    Ok(result)
}

static SERVICE_PROBES: Semaphore = Semaphore::const_new(1);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProbeConfig {
    route: String,
}

async fn check_service_readiness(
    realm: &str,
    selection: &RealmPackageSelection,
) -> Result<(), Status> {
    let (_, front) = selection.selected().map_err(Status::invalid_argument)?;
    if front.local_path.is_some() {
        return Err(Status::invalid_argument(
            "Use a downloadable Front for a Service release so AFP can verify its RPC contract.",
        ));
    }
    // The probe configuration and its Realm credential belong to the deployment.
    // The HTTP request supplies only the signed selection and its baseline.
    let missing =
        || Status::failed_precondition("Service readiness probe is not configured for this App.");
    let root = std::env::var_os("LORAN_ADMIN_READINESS_DIR").ok_or_else(missing)?;
    if parse_realm_name(realm)
        .ok()
        .and_then(|uri| uri.realm_name())
        .as_deref()
        != Some(realm)
    {
        return Err(Status::invalid_argument(
            "Invalid App ID for Service readiness",
        ));
    }
    let dir = PathBuf::from(root).join(realm);
    let config = dir.join("actr.toml");
    let probe: ProbeConfig = serde_json::from_slice(
        &tokio::fs::read(dir.join("probe.json"))
            .await
            .map_err(|_| missing())?,
    )
    .map_err(|_| missing())?;
    if probe.route.trim().is_empty()
        || !tokio::fs::metadata(&config)
            .await
            .is_ok_and(|metadata| metadata.is_file())
    {
        return Err(missing());
    }
    let executable = std::env::var_os("LORAN_PACKAGE_VERIFIER").ok_or_else(|| {
        Status::unavailable("Configure LORAN_PACKAGE_VERIFIER with the actr executable.")
    })?;
    let _slot = SERVICE_PROBES.try_acquire().map_err(|_| {
        Status::unavailable("Another Service readiness check is running. Try again.")
    })?;
    let work = tempfile::tempdir()
        .map_err(|_| Status::unavailable("Could not create the Service readiness workspace."))?;
    let selection_file = work.path().join("selection.json");
    tokio::fs::write(
        &selection_file,
        serde_json::to_vec(selection)
            .map_err(|error| Status::invalid_argument(error.to_string()))?,
    )
    .await
    .map_err(|_| Status::unavailable("Could not prepare the Service readiness check."))?;
    let mut command = Command::new(executable);
    command
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .env_remove("RUST_LOG")
        .args(["realm", "check-selection", "--selection"])
        .arg(selection_file)
        .arg("--output")
        .arg(work.path().join("verified"))
        .args(["--probe-realm", realm, "--probe-config"])
        .arg(config)
        .arg("--probe-route")
        .arg(probe.route);
    let request = dir.join("request.bin");
    if tokio::fs::try_exists(&request)
        .await
        .map_err(|_| missing())?
    {
        command.arg("--probe-request").arg(request);
    }
    let output = tokio::time::timeout(Duration::from_secs(180), command.output())
        .await
        .map_err(|_| Status::unavailable("Service readiness verification timed out."))?
        .map_err(|_| Status::unavailable("Could not run the Service readiness verifier."))?;
    if !output.status.success() {
        return Err(Status::failed_precondition(
            "The target Service failed package verification or did not pass the readiness RPC.",
        ));
    }
    let verified: ReadinessReport = serde_json::from_slice(&output.stdout)
        .map_err(|_| Status::unavailable("Invalid Service readiness verification result."))?;
    if &verified.selection != selection || verified.front.is_none() {
        return Err(Status::unavailable(
            "Service readiness verifier did not confirm the submitted selection.",
        ));
    }
    Ok(())
}

#[derive(Deserialize)]
struct ReadinessReport {
    selection: RealmPackageSelection,
    front: Option<String>,
}
