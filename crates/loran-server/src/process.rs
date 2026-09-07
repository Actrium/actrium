//! Process management module
//!
//! Handles PID file management and user/group switching

use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Process management utilities
pub struct ProcessManager;

impl ProcessManager {
    /// Write PID file
    pub fn write_pid_file(pid_path: Option<&str>) -> Result<Option<PathBuf>> {
        if let Some(path_str) = pid_path {
            let path = Path::new(path_str);

            // Create parent directory if it doesn't exist
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create PID file directory: {parent:?}"))?;
            }

            // Get current process ID
            let pid = std::process::id();

            // Write PID to file
            let mut file = fs::File::create(path)
                .with_context(|| format!("Failed to create PID file: {path:?}"))?;

            writeln!(file, "{pid}")
                .with_context(|| format!("Failed to write PID to file: {path:?}"))?;

            platform::recording::info!("PID file written: {:?} (PID: {})", path, pid);
            Ok(Some(path.to_path_buf()))
        } else {
            Ok(None)
        }
    }

    /// Remove PID file
    pub fn remove_pid_file(pid_path: Option<&PathBuf>) {
        if let Some(path) = pid_path {
            if let Err(e) = fs::remove_file(path) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    platform::recording::warn!("Failed to remove PID file {:?}: {}", path, e);
                }
            } else {
                platform::recording::info!("PID file removed: {:?}", path);
            }
        }
    }

    /// Drop privileges by switching to specified user and group
    #[cfg(unix)]
    pub fn drop_privileges(user: Option<&str>, group: Option<&str>) -> Result<()> {
        #[cfg(not(any(
            target_os = "macos",
            target_os = "ios",
            target_os = "tvos",
            target_os = "watchos",
            target_os = "redox",
            target_os = "haiku"
        )))]
        use nix::unistd::setgroups;
        use nix::unistd::{Uid, setgid, setuid};

        // Get current user ID
        let current_uid = Uid::current();

        // Only root can switch users. If a drop target is configured but we are
        // not root, fail closed: silently continuing would run the daemon with
        // the current (possibly over-privileged) identity while the operator
        // believes privileges were dropped. main.rs aborts startup on this Err.
        if !current_uid.is_root() {
            if user.is_some() || group.is_some() {
                return Err(anyhow::anyhow!(
                    "user/group privilege drop is configured (user={user:?}, group={group:?}) \
                     but the process is not running as root; refusing to start without \
                     dropping privileges"
                ));
            }
            return Ok(());
        }

        // Switch group first (while we still have privileges)
        if let Some(group_name) = group {
            platform::recording::info!("Switching to group: {}", group_name);

            // Look up group by name
            let group_info = nix::unistd::Group::from_name(group_name)?
                .ok_or_else(|| anyhow::anyhow!("Group '{group_name}' not found"))?;

            // Set supplementary groups to empty
            #[cfg(not(any(
                target_os = "macos",
                target_os = "ios",
                target_os = "tvos",
                target_os = "watchos",
                target_os = "redox",
                target_os = "haiku"
            )))]
            setgroups(&[]).with_context(|| "Failed to clear supplementary groups")?;

            // Set group ID
            setgid(group_info.gid)
                .with_context(|| format!("Failed to set group ID to {group_name}"))?;

            platform::recording::info!(
                "Successfully switched to group: {} (GID: {})",
                group_name,
                group_info.gid
            );
        }

        // Switch user (this must be done last as it drops privileges)
        if let Some(user_name) = user {
            platform::recording::info!("Switching to user: {}", user_name);

            // Look up user by name
            let user_info = nix::unistd::User::from_name(user_name)?
                .ok_or_else(|| anyhow::anyhow!("User '{user_name}' not found"))?;

            // If no group was specified, use the user's primary group
            if group.is_none() {
                #[cfg(not(any(
                    target_os = "macos",
                    target_os = "ios",
                    target_os = "tvos",
                    target_os = "watchos",
                    target_os = "redox",
                    target_os = "haiku"
                )))]
                setgroups(&[]).with_context(|| "Failed to clear supplementary groups")?;

                setgid(user_info.gid)
                    .with_context(|| format!("Failed to set primary group for user {user_name}"))?;
            }

            // Set user ID (this drops privileges)
            setuid(user_info.uid)
                .with_context(|| format!("Failed to set user ID to {user_name}"))?;

            platform::recording::info!(
                "Successfully switched to user: {} (UID: {})",
                user_name,
                user_info.uid
            );
        }

        Ok(())
    }

    /// Tighten the process umask so every file created afterwards is owner-only.
    ///
    /// 0o077 => new files 0600, new dirs 0700. This keeps SQLite databases (incl.
    /// the Signer private-key store), their `-wal`/`-shm` sidecars, migration
    /// backups and the PID file unreadable by other local users — the in-scope
    /// "same-host unprivileged process" attacker. Applies for the whole process
    /// lifetime and survives the privilege drop.
    #[cfg(unix)]
    pub fn tighten_umask() {
        use nix::sys::stat::{Mode, umask};
        umask(Mode::from_bits_truncate(0o077));
    }

    /// Tighten umask on non-Unix systems (no-op).
    #[cfg(not(unix))]
    pub fn tighten_umask() {}

    /// Secure the SQLite data directory before dropping privileges.
    ///
    /// Files created while still root would otherwise stay owned by root:root and
    /// become unreadable to the target user after the drop. When a drop is
    /// imminent (running as root with a configured user), chown the directory and
    /// its files to the target user/group; in all cases tighten the modes to
    /// 0700 (dir) / 0600 (files) so existing files created before the umask was
    /// tightened are corrected too.
    #[cfg(unix)]
    pub fn secure_data_dir(
        path: &std::path::Path,
        user: Option<&str>,
        group: Option<&str>,
    ) -> Result<()> {
        use nix::unistd::{Uid, chown};
        use std::os::unix::fs::PermissionsExt;

        if !path.exists() {
            return Ok(());
        }

        let (uid, gid) = match (Uid::current().is_root(), user) {
            (true, Some(user_name)) => {
                let u = nix::unistd::User::from_name(user_name)?
                    .ok_or_else(|| anyhow::anyhow!("User '{user_name}' not found"))?;
                let gid = match group {
                    Some(group_name) => {
                        nix::unistd::Group::from_name(group_name)?
                            .ok_or_else(|| anyhow::anyhow!("Group '{group_name}' not found"))?
                            .gid
                    }
                    None => u.gid,
                };
                (Some(u.uid), Some(gid))
            }
            _ => (None, None),
        };

        let apply = |p: &std::path::Path, is_dir: bool| -> Result<()> {
            let mode = if is_dir { 0o700 } else { 0o600 };
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode))
                .with_context(|| format!("Failed to chmod {}", p.display()))?;
            if let Some(uid) = uid {
                chown(p, Some(uid), gid)
                    .with_context(|| format!("Failed to chown {}", p.display()))?;
            }
            Ok(())
        };

        apply(path, true)?;
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let child = entry.path();
            let is_dir = child.is_dir();
            apply(&child, is_dir)?;
        }
        Ok(())
    }

    /// Secure the data directory on non-Unix systems (no-op).
    #[cfg(not(unix))]
    pub fn secure_data_dir(
        _path: &std::path::Path,
        _user: Option<&str>,
        _group: Option<&str>,
    ) -> Result<()> {
        Ok(())
    }

    /// Drop privileges on non-Unix systems (no-op)
    #[cfg(not(unix))]
    pub fn drop_privileges(_user: Option<&str>, _group: Option<&str>) -> Result<()> {
        if _user.is_some() || _group.is_some() {
            platform::recording::warn!("User/group switching is not supported on this platform");
        }
        Ok(())
    }
}

/// Guard to ensure PID file is removed on drop
pub struct PidFileGuard {
    path: Option<PathBuf>,
}

impl PidFileGuard {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }
}

impl Drop for PidFileGuard {
    fn drop(&mut self) {
        ProcessManager::remove_pid_file(self.path.as_ref());
    }
}
