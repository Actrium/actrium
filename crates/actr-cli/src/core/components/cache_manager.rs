//! Default CacheManager implementation
//!
//! Proto files are cached to the project's `protos/remote/` folder (not ~/.actr/cache)
//! following the documentation spec for dependency management.

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::{Component, Path, PathBuf};

use super::{CacheManager, CacheStats, CachedProto, Fingerprint, ProtoFile};

/// Default cache manager (file-based, project-local)
///
/// Caches proto files to `{project_root}/protos/remote/{dependency_alias}/` directory
/// following the documentation spec.
pub struct DefaultCacheManager {
    /// Project root directory (where manifest.toml is located)
    project_root: PathBuf,
}

impl DefaultCacheManager {
    pub fn new() -> Self {
        Self {
            project_root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        }
    }

    pub fn with_project_root(project_root: PathBuf) -> Self {
        Self { project_root }
    }

    /// Get the proto cache directory for a source-level dependency alias.
    /// Returns: {project_root}/protos/remote/{dependency_alias}/
    fn validate_component<'a>(value: &'a str, label: &str) -> Result<&'a str> {
        if value.is_empty() || value.contains(['/', '\\', '\0']) {
            anyhow::bail!("{label} `{value}` is not a safe path component");
        }
        let mut components = Path::new(value).components();
        if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
            anyhow::bail!("{label} `{value}` is not a safe path component");
        }
        Ok(value)
    }

    fn remote_root(&self) -> Result<PathBuf> {
        let project_root = std::fs::canonicalize(&self.project_root).with_context(|| {
            format!("canonicalize project root {}", self.project_root.display())
        })?;
        let remote_root = self.project_root.join("protos").join("remote");
        std::fs::create_dir_all(&remote_root)?;
        let canonical_remote = std::fs::canonicalize(&remote_root)?;
        if !canonical_remote.starts_with(&project_root) {
            anyhow::bail!(
                "proto cache root {} escapes project root {}",
                canonical_remote.display(),
                project_root.display()
            );
        }
        Ok(canonical_remote)
    }

    fn get_service_proto_dir(&self, dependency_alias: &str) -> Result<PathBuf> {
        let alias = Self::validate_component(dependency_alias, "dependency alias")?;
        Ok(self.remote_root()?.join(alias))
    }
}

impl Default for DefaultCacheManager {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CacheManager for DefaultCacheManager {
    async fn get_cached_proto(&self, service_name: &str) -> Result<Option<CachedProto>> {
        let cache_path = self.get_service_proto_dir(service_name)?;

        if !cache_path.exists() {
            return Ok(None);
        }

        let mut files = Vec::new();
        for entry in std::fs::read_dir(&cache_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "proto").unwrap_or(false) {
                let content = std::fs::read_to_string(&path)?;
                files.push(ProtoFile {
                    name: path.file_name().unwrap().to_string_lossy().to_string(),
                    path,
                    content,
                    services: Vec::new(),
                });
            }
        }

        if files.is_empty() {
            Ok(None)
        } else {
            Ok(Some(CachedProto {
                files,
                fingerprint: Fingerprint {
                    algorithm: "sha256".to_string(),
                    value: "cached".to_string(),
                },
                cached_at: std::time::SystemTime::now(),
                expires_at: None,
            }))
        }
    }

    async fn cache_proto(&self, service_name: &str, files: &[ProtoFile]) -> Result<()> {
        if files.is_empty() {
            anyhow::bail!("refusing to replace cache for `{service_name}` with an empty snapshot");
        }
        let cache_path = self.get_service_proto_dir(service_name)?;
        if cache_path.exists()
            && std::fs::symlink_metadata(&cache_path)?
                .file_type()
                .is_symlink()
        {
            anyhow::bail!(
                "proto cache path is a symbolic link: {}",
                cache_path.display()
            );
        }
        let remote_root = self.remote_root()?;
        let staging = tempfile::Builder::new()
            .prefix(".actr-cache-")
            .tempdir_in(&remote_root)?;

        for file in files {
            // Use the proto file name directly (e.g., echo.v1.proto)
            let file_name = if file.name.ends_with(".proto") {
                file.name.clone()
            } else {
                format!("{}.proto", file.name)
            };
            let file_name = Self::validate_component(&file_name, "proto file name")?;
            let file_path = staging.path().join(file_name);
            std::fs::write(&file_path, &file.content)?;
            tracing::debug!(
                "Cached proto file: {} -> {}",
                file.name,
                file_path.display()
            );
        }

        let staged_path = staging.keep();
        let backup_path = remote_root.join(format!(".actr-cache-backup-{}", uuid::Uuid::new_v4()));
        let had_existing = cache_path.exists();
        if had_existing {
            std::fs::rename(&cache_path, &backup_path)?;
        }
        if let Err(error) = std::fs::rename(&staged_path, &cache_path) {
            if had_existing {
                let _ = std::fs::rename(&backup_path, &cache_path);
            }
            let _ = std::fs::remove_dir_all(&staged_path);
            return Err(error.into());
        }
        if had_existing {
            std::fs::remove_dir_all(&backup_path)?;
        }

        tracing::info!(
            "Cached {} proto files to protos/remote/{}/",
            files.len(),
            service_name
        );
        Ok(())
    }

    async fn invalidate_cache(&self, service_name: &str) -> Result<()> {
        let cache_path = self.get_service_proto_dir(service_name)?;
        if cache_path.exists() {
            std::fs::remove_dir_all(&cache_path)?;
        }
        Ok(())
    }

    async fn retain_aliases(&self, aliases: &std::collections::HashSet<String>) -> Result<()> {
        for alias in aliases {
            Self::validate_component(alias, "dependency alias")?;
        }
        let remote_root = self.remote_root()?;
        for entry in std::fs::read_dir(&remote_root)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(".actr-cache-") || aliases.contains(&name) {
                continue;
            }
            let metadata = std::fs::symlink_metadata(entry.path())?;
            if metadata.file_type().is_symlink() || metadata.is_file() {
                std::fs::remove_file(entry.path())?;
            } else if metadata.is_dir() {
                std::fs::remove_dir_all(entry.path())?;
            }
        }
        Ok(())
    }

    async fn clear_cache(&self) -> Result<()> {
        let proto_dir = self.project_root.join("protos");
        if proto_dir.exists() {
            std::fs::remove_dir_all(&proto_dir)?;
        }
        Ok(())
    }

    async fn get_cache_stats(&self) -> Result<CacheStats> {
        let proto_dir = self.project_root.join("protos");
        let mut total_size = 0u64;
        let mut entry_count = 0usize;

        if proto_dir.exists() {
            for entry in std::fs::read_dir(&proto_dir)? {
                entry_count += 1;
                let entry = entry?;
                if entry.path().is_dir() {
                    for file in std::fs::read_dir(entry.path())? {
                        let file = file?;
                        total_size += file.metadata()?.len();
                    }
                }
            }
        }

        Ok(CacheStats {
            total_entries: entry_count,
            total_size_bytes: total_size,
            hit_rate: 0.0,
            miss_rate: 0.0,
        })
    }
}

#[cfg(test)]
#[path = "cache_manager_tests.rs"]
mod tests;
