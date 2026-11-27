//! Reconcile OCI resources whose supervisor no longer owns them.

use std::fs::{self, File, TryLockError};
use std::io;
use std::path::Path;

use actr_config::OciRuntimeConfig;
use anyhow::{Context, Result, ensure};

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

    if !evict {
        return Ok((instances, 0));
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
        if name.len() != 64 || !name.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            continue;
        }
        let path = images.join(format!("{name}.lock"));
        if protected_images.contains(&path) {
            continue;
        }
        if let Some(_lock) = exclusive(&path)? {
            fs::remove_dir_all(entry.path())?;
            removed_images += 1;
        }
    }
    Ok((instances, removed_images))
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
