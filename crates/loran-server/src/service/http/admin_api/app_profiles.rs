//! Private Admin adapters for the existing domain protocol and package verifier.
use super::{AdminApiState, realm_writes_disabled_response};
use actr_protocol::{
    realm_management::{ManagementOp, ManagementRequest},
    realm_profile::RealmPackageSelection,
};
use axum::{
    Json,
    body::{Body, Bytes},
    extract::{ConnectInfo, Path, Request, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::{ffi::OsStr, net::SocketAddr, path::PathBuf, process::Stdio, sync::Arc, time::Duration};
use tokio::{process::Command, sync::Semaphore};
use tower_service::Service;
use uuid::Uuid;

fn message(status: StatusCode, message: &str) -> Response {
    (
        status,
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({"error":message})),
    )
        .into_response()
}

pub(super) async fn submit_signed(
    State(state): State<Arc<AdminApiState>>,
    Path(realm): Path<String>,
    peer: ConnectInfo<SocketAddr>,
    document: Bytes,
) -> Response {
    if !state.realm_writes_enabled {
        return realm_writes_disabled_response();
    }
    let Some(router) = &state.domain_management else {
        return message(
            StatusCode::SERVICE_UNAVAILABLE,
            "Realm authority is unavailable",
        );
    };
    if document.len() > 128 * 1024 {
        return message(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Signed request exceeds 128 KiB",
        );
    }
    let Ok(request) = ManagementRequest::parse(&document) else {
        return message(StatusCode::BAD_REQUEST, "Invalid signed Realm request");
    };
    if request.realm != realm
        || request.op == ManagementOp::RegisterPublisher
        || request.signature.is_none()
    {
        return message(
            StatusCode::BAD_REQUEST,
            "Supply a domain-signed app request for this App ID",
        );
    }
    // Forward the original bytes so the holder's signature covers every field.
    let mut req = Request::post("/actr/realm-management")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(document))
        .expect("static management route");
    req.extensions_mut().insert(peer);
    router
        .clone()
        .call(req)
        .await
        .expect("Axum router is infallible")
}

static CHECKS: Semaphore = Semaphore::const_new(1);

/// This is optional on Loran-only installs. Check the exact subcommand without
/// starting a download. A config reload rebuilds the Admin state and probes again.
pub(super) async fn package_verifier(state: &AdminApiState) -> Option<&OsStr> {
    state
        .package_verifier
        .get_or_init(|| async {
            let binary = std::env::var_os("LORAN_ADMIN_ACTR_BIN").unwrap_or_else(|| "actr".into());
            let mut command = Command::new(&binary);
            command
                .args(["realm", "check-selection", "--help"])
                .kill_on_drop(true)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            match tokio::time::timeout(Duration::from_secs(3), command.status()).await {
                Ok(Ok(status)) if status.success() => Some(binary),
                _ => None,
            }
        })
        .await
        .as_deref()
}

struct SelectionDirectory(PathBuf);
impl Drop for SelectionDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(super) async fn check_selection(
    State(state): State<Arc<AdminApiState>>,
    Json(selection): Json<RealmPackageSelection>,
) -> Response {
    if !state.realm_writes_enabled {
        return realm_writes_disabled_response();
    }
    if let Err(reason) = selection.check() {
        return message(StatusCode::BAD_REQUEST, &reason);
    }
    let Some(binary) = package_verifier(&state).await else {
        return message(
            StatusCode::SERVICE_UNAVAILABLE,
            "Install actr with realm check-selection on this node, or set LORAN_ADMIN_ACTR_BIN and restart Loran",
        );
    };
    let Ok(_slot) = CHECKS.try_acquire() else {
        return message(
            StatusCode::TOO_MANY_REQUESTS,
            "Another package check is in progress",
        );
    };
    let path = std::env::temp_dir().join(format!("loran-admin-packages-{}", Uuid::new_v4()));
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    if builder.create(&path).is_err() {
        return message(
            StatusCode::SERVICE_UNAVAILABLE,
            "Could not create package verification workspace",
        );
    }
    let directory = SelectionDirectory(path);
    let selection_file = directory.0.join("selection.json");
    if tokio::fs::write(
        &selection_file,
        serde_json::to_vec(&selection).expect("typed package selection"),
    )
    .await
    .is_err()
    {
        return message(
            StatusCode::SERVICE_UNAVAILABLE,
            "Could not prepare package verification",
        );
    }
    let mut cmd = Command::new(binary);
    cmd.args(["realm", "check-selection", "--selection"])
        .arg(selection_file)
        .arg("--output")
        .arg(&directory.0)
        .kill_on_drop(true)
        .stdin(Stdio::null());
    let output = match tokio::time::timeout(Duration::from_secs(90), cmd.output()).await {
        Err(_) => {
            return message(
                StatusCode::GATEWAY_TIMEOUT,
                "Package verification timed out",
            );
        }
        Ok(Err(_)) => {
            return message(
                StatusCode::SERVICE_UNAVAILABLE,
                "Install actr with realm check-selection or set LORAN_ADMIN_ACTR_BIN",
            );
        }
        Ok(Ok(output)) => output,
    };
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        let first = error
            .lines()
            .next()
            .unwrap_or("Package verification failed");
        return message(
            StatusCode::UNPROCESSABLE_ENTITY,
            &first.chars().take(350).collect::<String>(),
        );
    }
    let count = selection
        .packages
        .values()
        .filter(|package| package.local_path.is_none())
        .count();
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({"verified":true,"count":count})),
    )
        .into_response()
}
