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

impl VerificationReport {
    fn check(
        self,
        selection: &RealmPackageSelection,
        release: PackageRelease<'_>,
    ) -> Result<(), ProfileError> {
        if &self.selection != selection {
            return Err(ProfileError::Unavailable(
                "The package verifier returned another selection.".into(),
            ));
        }
        if matches!(release, PackageRelease::Service(_)) && self.front.is_none() {
            // The CLI validates a local Front declaration and its Publisher key.
            // The Host verifies the archive after the user grants file access.
            let (_, front) = selection.selected().map_err(ProfileError::Invalid)?;
            if front.local_path.is_none() {
                return Err(ProfileError::Invalid(
                    "The package verifier did not verify the selected Front archive.".into(),
                ));
            }
        }
        Ok(())
    }
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
        let stderr = String::from_utf8_lossy(&output.stderr);
        platform::recording::debug!("Package verifier rejected the selection: {stderr}");
        let reason = stderr
            .lines()
            .find_map(|line| line.trim().strip_prefix("Error: "))
            .unwrap_or("Could not verify the selected packages. See the server logs.");
        return Err(ProfileError::Invalid(reason.chars().take(1000).collect()));
    }
    let report: VerificationReport = serde_json::from_slice(&output.stdout)
        .map_err(|_| ProfileError::Unavailable("Invalid package verification result.".into()))?;
    report.check(selection, release)
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

#[cfg(test)]
mod tests {
    use super::*;
    use actr_protocol::realm_profile::FrontUpdate;
    use serde_json::json;

    #[test]
    fn service_release_accepts_a_verified_local_front_declaration() {
        let mut document = json!({
            "primary_package": "service",
            "front_package": "front",
            "packages": {
                "service": {
                    "href": "https://publisher.example/echo.actr",
                    "sha256": "a".repeat(64),
                    "type": "publisher.example/echo@1.1.0"
                },
                "front": {
                    "href": "/Users/example/front.actr",
                    "type": "publisher.example/echo-ui@1.0.0",
                    "local_path": {"signing_key_id": "actr-pub-0123456789abcdef", "allow_rebuild": true},
                    "dependencies": {"service": "service"}
                }
            }
        });
        let local: RealmPackageSelection = serde_json::from_value(document.clone()).unwrap();
        local.check().unwrap();
        let front = document["packages"]["front"].as_object_mut().unwrap();
        front.remove("local_path");
        front.insert("href".into(), json!("https://publisher.example/front.actr"));
        front.insert("sha256".into(), json!("b".repeat(64)));
        let downloadable: RealmPackageSelection = serde_json::from_value(document).unwrap();
        downloadable.check().unwrap();

        for rollback in [false, true] {
            let operation = FrontUpdate {
                operation_id: "1".repeat(32),
                expected_sequence: 3,
                rollback,
            };
            let release = PackageRelease::Service(&operation);
            let report = |selection: &RealmPackageSelection| VerificationReport {
                selection: selection.clone(),
                front: None,
            };
            report(&local).check(&local, release).unwrap();
            assert!(matches!(
                report(&downloadable).check(&downloadable, release),
                Err(ProfileError::Invalid(_))
            ));
        }
    }
}
