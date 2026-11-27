//! Reconcile OCI resources whose supervisor no longer owns them.

use std::fs::{self, File, TryLockError};
use std::io::{self, Read};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use actr_config::OciRuntimeConfig;
use anyhow::{Context, Result, ensure};

use super::image::{IMAGE_STAGE_PREFIX, UNPACK_PREFIX, cleanup_unpack_cgroup, remove_image_tree};
use super::runtime::{private_dir, validate_cgroup};
use super::supervisor::{Launch, cleanup};

/// Reconcile stopped instances and evict unleased OCI cache entries.
pub async fn gc(config: &OciRuntimeConfig) -> Result<(usize, usize)> {
    let config = config.clone();
    tokio::task::spawn_blocking(move || collect(&config, true)).await?
}

pub(super) async fn reconcile(config: &OciRuntimeConfig) -> Result<()> {
    let config = config.clone();
    tokio::task::spawn_blocking(move || collect(&config, false)).await??;
    Ok(())
}

fn collect(config: &OciRuntimeConfig, evict: bool) -> Result<(usize, usize)> {
    let state = private_dir(&config.state_dir)?;
    let images = private_dir(&config.image_dir)?;
    let cgroup = validate_cgroup(&config.cgroup_parent)?;
    reconcile_unpack_cgroups(&state, &cgroup)?;
    let mut instances = 0;
    let mut protected_images = Vec::new();
    for entry in fs::read_dir(&state)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let instance = entry.file_name();
        let Some(name) = instance.to_str() else {
            continue;
        };
        if !name
            .strip_prefix("actr-")
            .is_some_and(|id| id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit()))
        {
            continue;
        }
        let dir = entry.path();
        let owner = match exclusive(&dir.join("owner.lock")) {
            Ok(owner) => owner,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        // The starter publishes this record by rename, so even when the
        // lock is busy it is safe to read the image lease for this pass.
        let launch_file = match File::open(dir.join("launch.json")) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                // Another start may still be preparing the directory.
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        let launch: Launch = serde_json::from_reader(launch_file)
            .context("cannot establish OCI bundle ownership")?;
        ensure!(
            launch.bundle == dir
                && launch.instance == name
                && launch.cgroup == cgroup.join(name)
                && launch.runtime == config.syd_oci
                && launch.image_lock.parent() == Some(images.as_path()),
            "OCI bundle does not belong to this deployment"
        );
        let Some(_owner) = owner else {
            protected_images.push(launch.image_lock);
            continue;
        };
        if !dir.join("stopped").is_file() {
            cleanup(&launch).context("OCI container still requires recovery")?;
        }
        fs::remove_dir_all(&dir)?;
        instances += 1;
    }

    let mut removed_images = 0;
    for entry in fs::read_dir(&images)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        if let Some(key) = image_stage_key(name) {
            if let Err(error) = remove_abandoned_image_stage(
                &entry.path(),
                &images.join(format!("{key}.build.lock")),
            ) {
                if evict {
                    return Err(error);
                }
                tracing::warn!(%error, "could not reclaim an abandoned OCI image stage");
            }
            continue;
        }
        if !evict || !is_image_key(name) {
            continue;
        }
        let path = images.join(format!("{name}.lock"));
        if protected_images.contains(&path) {
            continue;
        }
        if let Some(_lock) = exclusive(&path)? {
            remove_image_tree(&entry.path())?;
            removed_images += 1;
        }
    }
    Ok((instances, removed_images))
}

fn is_image_key(name: &str) -> bool {
    name.len() == 64 && name.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn image_stage_key(name: &str) -> Option<&str> {
    let (key, suffix) = name.strip_prefix(IMAGE_STAGE_PREFIX)?.split_once('-')?;
    (is_image_key(key) && !suffix.is_empty()).then_some(key)
}

fn remove_abandoned_image_stage(stage: &Path, build_lock: &Path) -> Result<()> {
    // The builder holds this lock until the stage is dropped. Keep the lock
    // file itself so future builders and collectors always synchronize on it.
    let _lock = match exclusive(build_lock) {
        Ok(Some(lock)) => lock,
        Ok(None) => return Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    match remove_image_tree(stage) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("reclaim OCI image stage {}", stage.display()))
        }
    }
}

fn reconcile_unpack_cgroups(state: &Path, parent: &Path) -> Result<()> {
    for entry in fs::read_dir(state)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let file_name = entry.file_name();
        let Some(name) = file_name
            .to_str()
            .and_then(|name| name.strip_suffix(".lock"))
        else {
            continue;
        };
        if !name
            .strip_prefix(UNPACK_PREFIX)
            .is_some_and(|id| id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit()))
        {
            continue;
        }
        let record = entry.path();
        let mut owner = match exclusive(&record) {
            Ok(Some(owner)) => owner,
            Ok(None) => continue,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        let mut deployment = Vec::new();
        (&mut owner).take(4096).read_to_end(&mut deployment)?;
        if deployment != parent.as_os_str().as_bytes() {
            continue;
        }
        cleanup_unpack_cgroup(&parent.join(name))
            .context("reconcile interrupted OCI image unpack")?;
        fs::remove_file(&record)?;
    }
    Ok(())
}

fn exclusive(path: &Path) -> io::Result<Option<File>> {
    let lock = File::options().read(true).write(true).open(path)?;
    match lock.try_lock() {
        Ok(()) => Ok(Some(lock)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
        Err(TryLockError::Error(error)) => Err(error),
    }
}
