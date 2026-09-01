//! Root-secured per-actor storage location derivation.
//!
//! [`SecureStorageRoot`] owns a canonicalized, symlink-checked filesystem root
//! and derives the fixed per-actor layout
//! `{root}/actors/v1/<sha256-hex>/store.sqlite3` in exactly one place
//! ([`SecureStorageRoot::path_for`]). Because the only variable component is the
//! lowercase-hex identity hash, no manifest string, template, env var, or
//! hostname can influence the path (CWE-22 / CWE-73).
//!
//! Symlink protection is layered and is only claimed under the documented threat
//! model: the storage root and its parents are created by a trusted
//! administrator / Actr, and a non-privileged attacker cannot rewrite them
//! concurrently. A same-UID attacker who can swap directories between the
//! canonicalize check and the open is *out of scope* (that needs `openat2` /
//! handle-relative APIs / a custom SQLite VFS — see issue #350 doc §9).

use std::path::{Component, Path, PathBuf};

use actr_platform_traits::StorageNamespace;
use tracing::debug;

use crate::error::{HyperError, HyperResult};

/// A canonicalized, symlink-checked filesystem root for per-actor storage.
#[derive(Debug, Clone)]
pub struct SecureStorageRoot {
    canonical: PathBuf,
}

impl SecureStorageRoot {
    /// Create `root` (if missing) with process-private permissions, reject a
    /// symlink/reparse point at the root itself, and canonicalize it.
    pub async fn new(root: &Path) -> HyperResult<Self> {
        reject_if_symlink(root).await?;
        tokio::fs::create_dir_all(root).await.map_err(|e| {
            HyperError::Storage(format!(
                "failed to create storage root `{}`: {e}",
                root.display()
            ))
        })?;
        // Re-check after creation (same check/create/check pattern as
        // `prepare_database_path`): the path could have been swapped for a
        // symlink between the first check and `create_dir_all`.
        reject_if_symlink(root).await?;
        set_private_permissions(root).await?;
        let canonical = tokio::fs::canonicalize(root).await.map_err(|e| {
            HyperError::Storage(format!(
                "failed to canonicalize storage root `{}`: {e}",
                root.display()
            ))
        })?;
        debug!(root = %canonical.display(), "secure storage root ready");
        Ok(Self { canonical })
    }

    /// The canonical storage root.
    pub fn canonical_path(&self) -> &Path {
        &self.canonical
    }

    /// Pure (no-IO) derivation of the database path for `namespace`.
    ///
    /// Used for diagnostics; the real open path goes through
    /// [`Self::prepare_database_path`], which shares this single layout.
    pub fn path_for(&self, namespace: &StorageNamespace) -> HyperResult<PathBuf> {
        if namespace.version() != 1 {
            return Err(HyperError::Storage(format!(
                "unsupported storage namespace version: {}",
                namespace.version()
            )));
        }
        let component = hex::encode(namespace.digest());
        debug_assert_eq!(component.len(), 64);
        let candidate = self
            .canonical
            .join("actors")
            .join("v1")
            .join(component)
            .join("store.sqlite3");
        // Defense-in-depth invariant: candidate is fixed strings + hex only.
        if !candidate.starts_with(&self.canonical) {
            return Err(HyperError::Storage(
                "derived storage path escaped storage root".to_string(),
            ));
        }
        Ok(candidate)
    }

    /// Derive the database path AND create the namespace directory hierarchy,
    /// checking every level for symlinks/reparse points. This is the real
    /// open-time entry used by [`crate::storage::ActorStore::open_under`].
    pub async fn prepare_database_path(
        &self,
        namespace: &StorageNamespace,
    ) -> HyperResult<PathBuf> {
        let db_path = self.path_for(namespace)?;
        let ns_dir = db_path.parent().ok_or_else(|| {
            HyperError::Storage("derived database path has no parent".to_string())
        })?;

        // Walk the fixed relative layout (`actors/v1/<hex>`) from the canonical
        // root, creating each level and rejecting symlinks before and after.
        let rel = ns_dir.strip_prefix(&self.canonical).map_err(|_| {
            HyperError::Storage("namespace dir is outside storage root".to_string())
        })?;
        let mut cur = self.canonical.clone();
        for comp in rel.components() {
            match comp {
                Component::Normal(seg) => {
                    cur.push(seg);
                    reject_if_symlink(&cur).await?;
                    tokio::fs::create_dir_all(&cur).await.map_err(|e| {
                        HyperError::Storage(format!(
                            "failed to create namespace dir `{}`: {e}",
                            cur.display()
                        ))
                    })?;
                    reject_if_symlink(&cur).await?;
                }
                _ => {
                    return Err(HyperError::Storage(
                        "namespace path contained a non-normal component".to_string(),
                    ));
                }
            }
        }

        // After creation, confirm the namespace dir still resolves under the root.
        let canon_ns = tokio::fs::canonicalize(ns_dir).await.map_err(|e| {
            HyperError::Storage(format!(
                "failed to canonicalize namespace dir `{}`: {e}",
                ns_dir.display()
            ))
        })?;
        if !canon_ns.starts_with(&self.canonical) {
            return Err(HyperError::Storage(
                "namespace dir escaped storage root after creation".to_string(),
            ));
        }

        // Reject a symlink at the db file itself before SQLite opens it.
        reject_if_symlink(&db_path).await?;
        Ok(db_path)
    }
}

/// Reject `path` if it is a symlink (or, on Windows, a reparse point). A
/// non-existent path is accepted (it will be created by the caller).
async fn reject_if_symlink(path: &Path) -> HyperResult<()> {
    match tokio::fs::symlink_metadata(path).await {
        Ok(md) => {
            let ft = md.file_type();
            if ft.is_symlink() {
                return Err(HyperError::Storage(format!(
                    "refusing to use symlink `{}`",
                    path.display()
                )));
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
                if md.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                    return Err(HyperError::Storage(format!(
                        "refusing to use reparse point `{}`",
                        path.display()
                    )));
                }
            }
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(HyperError::Storage(format!(
            "failed to stat `{}`: {e}",
            path.display()
        ))),
    }
}

/// Set process-private (`0700`) permissions on Unix. No-op elsewhere.
async fn set_private_permissions(path: &Path) -> HyperResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .await
            .map_err(|e| {
                HyperError::Storage(format!(
                    "failed to set permissions on `{}`: {e}",
                    path.display()
                ))
            })?;
    }
    let _ = path;
    Ok(())
}
