use super::ProfileError;
use actr_protocol::realm_profile::RealmPackageSelection;
use platform::realm::PackageRelease;
use serde::Deserialize;
use std::{io, path::Path, time::Duration};
use tokio::process::Command;

#[derive(Deserialize)]
struct VerificationReport {
    selection: RealmPackageSelection,
    front: Option<String>,
}

pub(super) async fn verify(
    data_dir: &Path,
    selection: &RealmPackageSelection,
    release: PackageRelease<'_>,
) -> Result<(), ProfileError> {
    let executable = std::env::var_os("LORAN_PACKAGE_VERIFIER").ok_or_else(|| {
        ProfileError::Unavailable(
            "Configure LORAN_PACKAGE_VERIFIER with the actr executable.".into(),
        )
    })?;
    let trust_dir = data_dir.join("realm-package-trust");
    private_directory(&trust_dir).map_err(|error| {
        ProfileError::Unavailable(format!("Could not open Publisher trust directory: {error}"))
    })?;
    let work = tempfile::tempdir().map_err(|error| {
        ProfileError::Unavailable(format!("Package verification workspace: {error}"))
    })?;
    let selection_path = work.path().join("selection.json");
    let bytes =
        serde_json::to_vec(selection).map_err(|error| ProfileError::Invalid(error.to_string()))?;
    tokio::fs::write(&selection_path, bytes)
        .await
        .map_err(|error| {
            ProfileError::Unavailable(format!("Package verification input: {error}"))
        })?;

    let mut command = Command::new(executable);
    command
        .kill_on_drop(true)
        .env_remove("RUST_LOG")
        .args(["realm", "check-selection", "--selection"])
        .arg(selection_path)
        .arg("--output")
        .arg(work.path().join("verified"))
        .arg("--trust-dir")
        .arg(trust_dir);
    let output = tokio::time::timeout(Duration::from_secs(120), command.output())
        .await
        .map_err(|_| ProfileError::Unavailable("Package verification timed out.".into()))?
        .map_err(|error| {
            ProfileError::Unavailable(format!("Could not start the package verifier: {error}"))
        })?;
    if !output.status.success() {
        let details: String = String::from_utf8_lossy(&output.stderr)
            .chars()
            .take(1800)
            .collect();
        return Err(ProfileError::Invalid(format!(
            "Package verification failed: {details}"
        )));
    }
    let report: VerificationReport = serde_json::from_slice(&output.stdout)
        .map_err(|_| ProfileError::Unavailable("Invalid package verification result.".into()))?;
    if &report.selection != selection {
        return Err(ProfileError::Unavailable(
            "The package verifier returned another selection.".into(),
        ));
    }
    if matches!(release, PackageRelease::Service(_)) && report.front.is_none() {
        return Err(ProfileError::Invalid(
            "A Service release needs a downloadable Front archive for RPC contract verification."
                .into(),
        ));
    }
    Ok(())
}

fn private_directory(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(false);
    #[cfg(unix)]
    builder.mode(0o700);
    match builder.create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() {
        return Err(io::Error::other("Publisher trust path must be a directory"));
    }
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(io::Error::other(format!(
            "Publisher trust directory {} must use mode 0700",
            path.display()
        )));
    }
    Ok(())
}
