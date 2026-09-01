//! Operation pipeline definitions
//!
//! Defines three core operation pipelines for cross-command logic reuse

use actr_config::{LockFile, LockedDependency, ProtoFileMeta, ServiceSpecMeta};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::components::*;

#[cfg(test)]
fn build_lock_snapshot(dependencies: &[ResolvedDependency]) -> Result<LockFile> {
    let mut lock_file = LockFile::new();

    for dep in dependencies {
        lock_file.add_dependency(locked_dependency_from_resolved(dep)?);
    }

    lock_file.validate_aliases()?;
    Ok(lock_file)
}

fn locked_dependency_from_resolved(dep: &ResolvedDependency) -> Result<LockedDependency> {
    let dependency_alias = dep.spec.alias.clone();
    let protobufs = dep
        .proto_files
        .iter()
        .map(|proto| {
            let file_name = if proto.name.ends_with(".proto") {
                proto.name.clone()
            } else {
                format!("{}.proto", proto.name)
            };
            ProtoFileMeta {
                path: format!("{dependency_alias}/{file_name}"),
                fingerprint: String::new(),
            }
        })
        .collect();
    let spec = ServiceSpecMeta {
        name: dep.spec.name.clone(),
        description: None,
        fingerprint: dep.fingerprint.clone(),
        protobufs,
        published_at: None,
        tags: Vec::new(),
    };
    let actr_type = dep.spec.actr_type.clone().ok_or_else(|| {
        anyhow::anyhow!("Actr type is required for dependency: {dependency_alias}")
    })?;
    Ok(LockedDependency::new(
        dependency_alias,
        actr_type.to_string_repr(),
        spec,
    ))
}

fn locked_dependency_matches_spec(locked: &LockedDependency, spec: &DependencySpec) -> bool {
    if locked.alias != spec.alias {
        return false;
    }
    if let Some(actr_type) = &spec.actr_type
        && locked.actr_type != actr_type.to_string_repr()
    {
        return false;
    }
    if let Some(fingerprint) = &spec.fingerprint
        && (locked.name != spec.name || locked.fingerprint != *fingerprint)
    {
        return false;
    }
    true
}

#[cfg(test)]
fn rebuild_lock_snapshot(
    existing: Option<&LockFile>,
    dependencies: &[ResolvedDependency],
) -> Result<LockFile> {
    if let Some(existing) = existing {
        validate_existing_lock_version(existing)?;
    }
    // The old dependency array is deliberately ignored. The manifest and
    // current discovery results are the complete source of the new snapshot.
    build_lock_snapshot(dependencies)
}

fn validate_existing_lock_version(lock: &LockFile) -> Result<()> {
    let version = lock.format_version();
    if version != 1 && version != actr_config::lock::LOCK_FILE_VERSION {
        anyhow::bail!(
            "unsupported existing lock file version {version}; this CLI can migrate version 1 and writes version {}",
            actr_config::lock::LOCK_FILE_VERSION
        );
    }
    Ok(())
}

fn validate_discovered_fingerprint(
    spec: &DependencySpec,
    discovered_fingerprint: &str,
) -> Result<()> {
    if let Some(expected) = spec.fingerprint.as_deref()
        && expected != discovered_fingerprint
    {
        return Err(anyhow::anyhow!(
            "manifest dependency '{}' fingerprint mismatch: expected {}, discovered {}",
            spec.alias,
            expected,
            discovered_fingerprint
        ));
    }
    Ok(())
}

struct InstallFilesystemBackup {
    project_root: PathBuf,
    temporary: tempfile::TempDir,
    had_lock: bool,
    had_cache: bool,
}

impl InstallFilesystemBackup {
    fn capture(project_root: &Path) -> Result<Self> {
        let temporary = tempfile::Builder::new()
            .prefix(".actr-install-backup-")
            .tempdir_in(project_root)?;
        let lock = project_root.join("manifest.lock.toml");
        let had_lock = lock.is_file();
        if had_lock {
            std::fs::copy(&lock, temporary.path().join("manifest.lock.toml"))?;
        }
        let cache = project_root.join("protos").join("remote");
        let had_cache = cache.is_dir();
        if had_cache {
            copy_directory_snapshot(&cache, &temporary.path().join("remote"))?;
        }
        Ok(Self {
            project_root: project_root.to_path_buf(),
            temporary,
            had_lock,
            had_cache,
        })
    }

    fn restore(&self) -> Result<()> {
        let lock = self.project_root.join("manifest.lock.toml");
        if self.had_lock {
            std::fs::copy(self.temporary.path().join("manifest.lock.toml"), &lock)?;
        } else if lock.exists() {
            std::fs::remove_file(&lock)?;
        }

        let cache = self.project_root.join("protos").join("remote");
        if cache.exists() {
            std::fs::remove_dir_all(&cache)?;
        }
        if self.had_cache {
            copy_directory_snapshot(&self.temporary.path().join("remote"), &cache)?;
        }
        Ok(())
    }
}

fn copy_directory_snapshot(source: &Path, destination: &Path) -> Result<()> {
    std::fs::create_dir_all(destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let metadata = std::fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() {
            anyhow::bail!(
                "refusing to snapshot symbolic link {}",
                entry.path().display()
            );
        }
        let target = destination.join(entry.file_name());
        if metadata.is_dir() {
            copy_directory_snapshot(&entry.path(), &target)?;
        } else if metadata.is_file() {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

// ============================================================================
// Pipeline result types
// ============================================================================

/// Install result
#[derive(Debug, Clone)]
pub struct InstallResult {
    pub installed_dependencies: Vec<ResolvedDependency>,
    pub updated_config: bool,
    pub updated_lock_file: bool,
    pub cache_updates: usize,
    pub warnings: Vec<String>,
}

impl InstallResult {
    pub fn success() -> Self {
        Self {
            installed_dependencies: Vec::new(),
            updated_config: false,
            updated_lock_file: false,
            cache_updates: 0,
            warnings: Vec::new(),
        }
    }

    pub fn summary(&self) -> String {
        format!(
            "Installed {} dependencies, updated {} cache entries",
            self.installed_dependencies.len(),
            self.cache_updates
        )
    }
}

/// Install plan
#[derive(Debug, Clone)]
pub struct InstallPlan {
    pub dependencies_to_install: Vec<DependencySpec>,
    pub resolved_dependencies: Vec<ResolvedDependency>,
    pub estimated_cache_size: u64,
    pub required_permissions: Vec<String>,
}

/// Generation options
#[derive(Debug, Clone)]
pub struct GenerationOptions {
    pub input_path: std::path::PathBuf,
    pub output_path: std::path::PathBuf,
    pub clean_before_generate: bool,
    pub generate_scaffold: bool,
    pub format_code: bool,
    pub run_checks: bool,
}

// ============================================================================
// 1. Validation Pipeline
// ============================================================================

/// Core validation pipeline - reused by multiple commands
#[derive(Clone)]
pub struct ValidationPipeline {
    config_manager: Arc<dyn ConfigManager>,
    dependency_resolver: Arc<dyn DependencyResolver>,
    service_discovery: Arc<dyn ServiceDiscovery>,
    network_validator: Arc<dyn NetworkValidator>,
    fingerprint_validator: Arc<dyn FingerprintValidator>,
}

impl ValidationPipeline {
    pub fn new(
        config_manager: Arc<dyn ConfigManager>,
        dependency_resolver: Arc<dyn DependencyResolver>,
        service_discovery: Arc<dyn ServiceDiscovery>,
        network_validator: Arc<dyn NetworkValidator>,
        fingerprint_validator: Arc<dyn FingerprintValidator>,
    ) -> Self {
        Self {
            config_manager,
            dependency_resolver,
            service_discovery,
            network_validator,
            fingerprint_validator,
        }
    }

    /// Get service discovery component
    pub fn service_discovery(&self) -> &Arc<dyn ServiceDiscovery> {
        &self.service_discovery
    }

    /// Get network validator component
    pub fn network_validator(&self) -> &Arc<dyn NetworkValidator> {
        &self.network_validator
    }

    /// Get config manager component
    pub fn config_manager(&self) -> &Arc<dyn ConfigManager> {
        &self.config_manager
    }

    /// Get dependency resolver component
    pub fn dependency_resolver(&self) -> &Arc<dyn DependencyResolver> {
        &self.dependency_resolver
    }

    fn dependency_lookup_key(spec: &DependencySpec) -> String {
        spec.actr_type
            .as_ref()
            .map(|actr_type| actr_type.to_string_repr())
            .unwrap_or_else(|| spec.name.clone())
    }

    /// Full project validation flow
    pub async fn validate_project(&self) -> Result<ValidationReport> {
        // 1. Config file validation
        let config_validation = self.config_manager.validate_config().await?;

        // If config file has issues, return immediately
        if !config_validation.is_valid {
            return Ok(ValidationReport {
                is_valid: false,
                config_validation,
                dependency_validation: vec![],
                network_validation: vec![],
                fingerprint_validation: vec![],
                conflicts: vec![],
            });
        }

        // 2. Dependency resolution and validation
        let config = self
            .config_manager
            .load_config(
                self.config_manager
                    .get_project_root()
                    .join("manifest.toml")
                    .as_path(),
            )
            .await?;
        let dependency_specs = self.dependency_resolver.resolve_spec(&config).await?;

        let mut service_details = Vec::new();
        for spec in &dependency_specs {
            let lookup_key = Self::dependency_lookup_key(spec);
            match self
                .service_discovery
                .get_service_details(&lookup_key)
                .await
            {
                Ok(details) => service_details.push(details),
                Err(_) => {
                    // Service might not be available, continue without details
                }
            }
        }

        let resolved_dependencies = self
            .dependency_resolver
            .resolve_dependencies(&dependency_specs, &service_details)
            .await?;

        // 3. Conflict check
        let conflicts = self
            .dependency_resolver
            .check_conflicts(&resolved_dependencies)
            .await?;

        let dependency_validation = self.validate_dependencies(&dependency_specs).await?;
        let network_validation = self
            .validate_network_connectivity(&resolved_dependencies, &NetworkCheckOptions::default())
            .await?;
        let fingerprint_validation = self.validate_fingerprints(&resolved_dependencies).await?;

        let is_valid = config_validation.is_valid
            && dependency_validation.iter().all(|d| d.is_available)
            && network_validation
                .iter()
                .all(|n| !n.is_applicable || n.is_reachable)
            && fingerprint_validation.iter().all(|f| f.is_valid)
            && conflicts.is_empty();

        Ok(ValidationReport {
            is_valid,
            config_validation,
            dependency_validation,
            network_validation,
            fingerprint_validation,
            conflicts,
        })
    }

    /// Validate a specific list of dependencies.
    /// Reuses availability checks that share the same canonical lookup key.
    pub async fn validate_dependencies(
        &self,
        specs: &[DependencySpec],
    ) -> Result<Vec<DependencyValidation>> {
        use std::collections::HashMap;

        let mut results = Vec::new();
        // Cache validation results by service name to avoid duplicate checks
        let mut validation_cache: HashMap<String, (bool, Option<String>)> = HashMap::new();

        for spec in specs {
            let lookup_key = Self::dependency_lookup_key(spec);
            // Check cache first - if we already validated this service name, reuse the result
            let (is_available, error) = if let Some(cached) = validation_cache.get(&lookup_key) {
                cached.clone()
            } else {
                // Perform validation
                let (available, err) = match self
                    .service_discovery
                    .check_service_availability(&lookup_key)
                    .await
                {
                    Ok(status) => {
                        if status.is_available {
                            (true, None)
                        } else {
                            // Provide meaningful error when service is not found
                            (
                                false,
                                Some(format!("Service '{}' not found in registry", lookup_key)),
                            )
                        }
                    }
                    Err(e) => (false, Some(e.to_string())),
                };

                // Cache the result for this service name
                validation_cache.insert(lookup_key, (available, err.clone()));
                (available, err)
            };

            results.push(DependencyValidation {
                dependency: spec.alias.clone(),
                is_available,
                error,
            });
        }

        Ok(results)
    }

    /// Network connectivity validation
    pub async fn validate_network_connectivity(
        &self,
        deps: &[ResolvedDependency],
        options: &NetworkCheckOptions,
    ) -> Result<Vec<NetworkValidation>> {
        let names = deps.iter().map(|d| d.spec.name.clone()).collect::<Vec<_>>();
        let network_results = self.network_validator.batch_check(&names, options).await?;

        Ok(network_results
            .into_iter()
            .map(|result| {
                let mut is_applicable = true;
                let mut error = result.connectivity.error;
                let mut health = result.health;
                let mut latency_ms = result.connectivity.response_time_ms;

                if let Some(ref message) = error
                    && message.starts_with("Address resolution failed: Invalid address format")
                {
                    is_applicable = false;
                    error =
                        Some("Network check skipped: no endpoint address available".to_string());
                    health = HealthStatus::Unknown;
                    latency_ms = None;
                }

                NetworkValidation {
                    is_reachable: result.connectivity.is_reachable,
                    health,
                    latency_ms,
                    error,
                    is_applicable,
                }
            })
            .collect())
    }

    /// Fingerprint validation
    pub async fn validate_fingerprints(
        &self,
        deps: &[ResolvedDependency],
    ) -> Result<Vec<FingerprintValidation>> {
        let mut results = Vec::new();

        for dep in deps {
            let expected_val = dep.spec.fingerprint.clone().unwrap_or_default();
            let expected = Fingerprint {
                algorithm: "sha256".to_string(),
                value: expected_val,
            };

            // Compute actual fingerprint (if resolved_dependencies has none, fetch from remote)
            let actual_fp = if dep.fingerprint.is_empty() {
                let lookup_key = Self::dependency_lookup_key(&dep.spec);
                match self
                    .service_discovery
                    .get_service_details(&lookup_key)
                    .await
                {
                    Ok(details) => {
                        let computed = self
                            .fingerprint_validator
                            .compute_service_fingerprint(&details.info)
                            .await?;
                        Some(computed)
                    }
                    Err(e) => {
                        results.push(FingerprintValidation {
                            dependency: dep.spec.alias.clone(),
                            expected,
                            actual: None,
                            is_valid: false,
                            error: Some(e.to_string()),
                        });
                        continue;
                    }
                }
            } else {
                // Already has fingerprint, no need to recompute
                None
            };

            let is_valid = if expected.value.is_empty() {
                true
            } else if let Some(ref computed) = actual_fp {
                self.fingerprint_validator
                    .verify_fingerprint(&expected, computed)
                    .await
                    .unwrap_or(false)
            } else {
                // Fingerprint already matched (from resolve_dependencies)
                true
            };

            results.push(FingerprintValidation {
                dependency: dep.spec.alias.clone(),
                expected,
                actual: actual_fp,
                is_valid,
                error: None,
            });
        }

        Ok(results)
    }
}

// ============================================================================
// 2. Install Pipeline
// ============================================================================

/// Install pipeline - built on top of ValidationPipeline
pub struct InstallPipeline {
    validation_pipeline: ValidationPipeline,
    config_manager: Arc<dyn ConfigManager>,
    cache_manager: Arc<dyn CacheManager>,
    #[allow(dead_code)]
    proto_processor: Arc<dyn ProtoProcessor>,
}

impl InstallPipeline {
    pub fn new(
        validation_pipeline: ValidationPipeline,
        config_manager: Arc<dyn ConfigManager>,
        cache_manager: Arc<dyn CacheManager>,
        proto_processor: Arc<dyn ProtoProcessor>,
    ) -> Self {
        Self {
            validation_pipeline,
            config_manager,
            cache_manager,
            proto_processor,
        }
    }

    /// Get validation pipeline reference
    pub fn validation_pipeline(&self) -> &ValidationPipeline {
        &self.validation_pipeline
    }

    /// Get config manager reference
    pub fn config_manager(&self) -> &Arc<dyn ConfigManager> {
        &self.config_manager
    }

    /// Check-first install flow
    pub async fn install_dependencies(&self, specs: &[DependencySpec]) -> Result<InstallResult> {
        if specs.is_empty() {
            return Ok(InstallResult::success());
        }
        // Phase 1: Full validation (reuse ValidationPipeline)
        let validation_report = self
            .validation_pipeline
            .validate_dependencies(specs)
            .await?;

        // Check validation results
        let failed_validations: Vec<_> = validation_report
            .iter()
            .filter(|v| !v.is_available)
            .collect();

        if !failed_validations.is_empty() {
            return Err(anyhow::anyhow!(
                "Dependency validation failed: {}",
                failed_validations
                    .iter()
                    .map(|v| format!(
                        "{}: {}",
                        v.dependency,
                        v.error.as_deref().unwrap_or("unknown error")
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        // Phase 2: Atomic install. The config manager protects manifest.toml;
        // this snapshot protects the lock and proto cache written by the same
        // transaction.
        let filesystem_backup =
            InstallFilesystemBackup::capture(self.config_manager.get_project_root())?;
        let backup = self.config_manager.backup_config().await?;

        match self.execute_atomic_install(specs).await {
            Ok(result) => {
                // The transaction is already committed. Backup cleanup is
                // best-effort and must not turn a successful install into a
                // reported failure with committed state.
                if let Err(error) = self.config_manager.remove_backup(backup).await {
                    tracing::warn!(%error, "failed to remove manifest backup after successful install");
                }
                Ok(result)
            }
            Err(e) => {
                let config_restore = self.config_manager.restore_backup(backup).await;
                let filesystem_restore = filesystem_backup.restore();
                match (config_restore, filesystem_restore) {
                    (Ok(()), Ok(())) => Err(e),
                    (config_result, filesystem_result) => Err(anyhow::anyhow!(
                        "{e}; rollback failed (manifest: {}; lock/cache: {})",
                        config_result
                            .err()
                            .map(|error| error.to_string())
                            .unwrap_or_else(|| "ok".to_string()),
                        filesystem_result
                            .err()
                            .map(|error| error.to_string())
                            .unwrap_or_else(|| "ok".to_string())
                    )),
                }
            }
        }
    }

    /// Commit an empty v2 dependency snapshot and prune all remote proto
    /// caches. Used when the manifest declares no dependencies.
    pub async fn install_empty_dependency_snapshot(&self) -> Result<InstallResult> {
        let project_root = self.config_manager.get_project_root();
        let lock_path = project_root.join("manifest.lock.toml");
        if lock_path.exists() {
            let existing = LockFile::from_file(&lock_path)?;
            validate_existing_lock_version(&existing)?;
        }
        let filesystem_backup = InstallFilesystemBackup::capture(project_root)?;
        let operation = async {
            let mut lock_file = LockFile::new();
            lock_file.update_timestamp();
            lock_file.save_to_file(&lock_path)?;
            self.cache_manager
                .retain_aliases(&std::collections::HashSet::new())
                .await?;
            Ok::<_, anyhow::Error>(())
        }
        .await;

        match operation {
            Ok(()) => Ok(InstallResult {
                updated_lock_file: true,
                ..InstallResult::success()
            }),
            Err(error) => {
                if let Err(restore_error) = filesystem_backup.restore() {
                    return Err(anyhow::anyhow!(
                        "{error}; lock/cache rollback failed: {restore_error}"
                    ));
                }
                Err(error)
            }
        }
    }

    /// Atomic install execution
    /// A valid manifest must not assign multiple aliases to the same canonical
    /// service lookup or `ActrType`, because v2 lock validation treats both
    /// alias and canonical actor type as unique identities.
    async fn execute_atomic_install(&self, specs: &[DependencySpec]) -> Result<InstallResult> {
        use std::collections::HashSet;

        let mut result = InstallResult::success();
        let mut installed_services: HashSet<String> = HashSet::new();

        for spec in specs {
            let lookup_key = ValidationPipeline::dependency_lookup_key(spec);
            if !installed_services.insert(lookup_key.clone()) {
                return Err(anyhow::anyhow!(
                    "dependency '{}' resolves the same service as another alias ({})",
                    spec.alias,
                    lookup_key
                ));
            }

            // 1. Get service details (before updating config, ensure we have the full actr_type)
            let service_details = self
                .validation_pipeline
                .service_discovery
                .get_service_details(&lookup_key)
                .await?;

            // Validate a manifest-pinned fingerprint before replacing it with
            // discovery metadata. Otherwise the subsequent manifest rewrite
            // would hide a registry mismatch.
            validate_discovered_fingerprint(spec, &service_details.info.fingerprint)?;

            // 2. Build resolved_spec using the canonical actr_type from discovery
            let mut resolved_spec = spec.clone();
            resolved_spec.actr_type = Some(service_details.info.actr_type.clone());
            resolved_spec.name = service_details.info.dependency_service_name().to_string();
            resolved_spec.fingerprint = Some(service_details.info.fingerprint.clone());

            // 3. Update config file (using resolved_spec with actr_type)
            self.config_manager
                .update_dependency(&resolved_spec)
                .await?;
            result.updated_config = true;

            // 4. Cache proto files
            self.cache_manager
                .cache_proto(&spec.alias, &service_details.proto_files)
                .await?;

            result.cache_updates += 1;

            // 5. Record installed dependency

            let resolved_dep = ResolvedDependency {
                spec: resolved_spec,
                fingerprint: service_details.info.fingerprint,
                proto_files: service_details.proto_files,
            };
            result.installed_dependencies.push(resolved_dep);
        }

        // 6. Update the lock file (deduplicated by alias and canonical ActrType).
        self.update_lock_file(&result.installed_dependencies)
            .await?;
        result.updated_lock_file = true;

        Ok(result)
    }

    /// Update lock file with new format (no embedded proto content)
    async fn update_lock_file(&self, installed: &[ResolvedDependency]) -> Result<()> {
        let project_root = self.config_manager.get_project_root();
        let lock_file_path = project_root.join("manifest.lock.toml");

        // A lock is a snapshot of the current manifest, not an append-only
        // install history. Rebuild it on every install so removed aliases are
        // pruned and v1 migration never has to infer identity from old fields.
        let existing = lock_file_path
            .exists()
            .then(|| LockFile::from_file(&lock_file_path))
            .transpose()?;
        if let Some(existing) = existing.as_ref() {
            validate_existing_lock_version(existing)?;
        }
        let reusable = existing
            .as_ref()
            .filter(|lock| lock.validate_aliases().is_ok());
        let manifest_path = project_root.join("manifest.toml");
        let config = self.config_manager.load_config(&manifest_path).await?;
        let specs = self
            .validation_pipeline
            .dependency_resolver()
            .resolve_spec(&config)
            .await?;
        let installed_by_alias = installed
            .iter()
            .map(|dependency| (dependency.spec.alias.as_str(), dependency))
            .collect::<std::collections::HashMap<_, _>>();
        let mut lock_file = LockFile::new();
        let mut aliases = std::collections::HashSet::with_capacity(specs.len());
        for spec in specs {
            aliases.insert(spec.alias.clone());
            if let Some(dependency) = installed_by_alias.get(spec.alias.as_str()) {
                lock_file.add_dependency(locked_dependency_from_resolved(dependency)?);
                continue;
            }
            if let Some(locked) = reusable
                .and_then(|lock| lock.get_dependency_by_alias(&spec.alias))
                .filter(|locked| locked_dependency_matches_spec(locked, &spec))
            {
                lock_file.add_dependency(locked.clone());
                continue;
            }

            let dependency = self.resolve_manifest_dependency(spec).await?;
            self.cache_manager
                .cache_proto(&dependency.spec.alias, &dependency.proto_files)
                .await?;
            lock_file.add_dependency(locked_dependency_from_resolved(&dependency)?);
        }
        lock_file.validate_aliases()?;

        // Update timestamp and save
        lock_file.update_timestamp();
        lock_file.save_to_file(&lock_file_path)?;
        self.cache_manager.retain_aliases(&aliases).await?;

        tracing::info!(
            "Updated lock file: {} dependencies",
            lock_file.dependencies.len()
        );
        Ok(())
    }

    /// Resolve every dependency declared by the current manifest.
    ///
    /// The manifest supplies the source-level alias and any pinned fingerprint;
    /// discovery supplies the canonical `ActrType` and current service data.
    async fn resolve_manifest_dependency(
        &self,
        spec: DependencySpec,
    ) -> Result<ResolvedDependency> {
        let lookup_key = ValidationPipeline::dependency_lookup_key(&spec);
        let service_details = self
            .validation_pipeline
            .service_discovery()
            .get_service_details(&lookup_key)
            .await?;
        validate_discovered_fingerprint(&spec, &service_details.info.fingerprint)?;
        let mut resolved_spec = spec;
        resolved_spec.actr_type = Some(service_details.info.actr_type.clone());
        resolved_spec.name = service_details.info.dependency_service_name().to_string();
        resolved_spec.fingerprint = Some(service_details.info.fingerprint.clone());
        Ok(ResolvedDependency {
            spec: resolved_spec,
            fingerprint: service_details.info.fingerprint,
            proto_files: service_details.proto_files,
        })
    }
}

// ============================================================================
// 3. Generation Pipeline
// ============================================================================

/// Code generation pipeline
pub struct GenerationPipeline {
    #[allow(dead_code)]
    config_manager: Arc<dyn ConfigManager>,
    proto_processor: Arc<dyn ProtoProcessor>,
    #[allow(dead_code)]
    cache_manager: Arc<dyn CacheManager>,
}

impl GenerationPipeline {
    pub fn new(
        config_manager: Arc<dyn ConfigManager>,
        proto_processor: Arc<dyn ProtoProcessor>,
        cache_manager: Arc<dyn CacheManager>,
    ) -> Self {
        Self {
            config_manager,
            proto_processor,
            cache_manager,
        }
    }

    /// Execute code generation
    pub async fn generate_code(&self, options: &GenerationOptions) -> Result<GenerationResult> {
        // 1. Clean output directory (if needed)
        if options.clean_before_generate {
            self.clean_output_directory(&options.output_path).await?;
        }

        // 2. Discover local proto files
        let local_protos = self
            .proto_processor
            .discover_proto_files(&options.input_path)
            .await?;

        // 3. Load dependency proto files
        let dependency_protos = self.load_dependency_protos().await?;

        // 4. Validate proto syntax
        let all_protos = [local_protos, dependency_protos].concat();
        let validation = self
            .proto_processor
            .validate_proto_syntax(&all_protos)
            .await?;

        if !validation.is_valid {
            return Err(anyhow::anyhow!("Proto file syntax validation failed"));
        }

        // 5. Execute code generation
        let mut generation_result = self
            .proto_processor
            .generate_code(&options.input_path, &options.output_path)
            .await?;

        // 6. Post-processing: format and check
        if options.format_code {
            self.format_generated_code(&generation_result.generated_files)
                .await?;
        }

        if options.run_checks {
            let check_result = self
                .run_code_checks(&generation_result.generated_files)
                .await?;
            generation_result.warnings.extend(check_result.warnings);
            generation_result.errors.extend(check_result.errors);
        }

        Ok(generation_result)
    }

    /// Clean the output directory
    async fn clean_output_directory(&self, output_path: &std::path::Path) -> Result<()> {
        if output_path.exists() {
            std::fs::remove_dir_all(output_path)?;
        }
        std::fs::create_dir_all(output_path)?;
        Ok(())
    }

    /// Load dependency proto files
    async fn load_dependency_protos(&self) -> Result<Vec<ProtoFile>> {
        // TODO: Load dependency proto files from cache
        Ok(Vec::new())
    }

    /// Format generated code
    async fn format_generated_code(&self, files: &[std::path::PathBuf]) -> Result<()> {
        for file in files {
            if file.extension().and_then(|s| s.to_str()) == Some("rs") {
                // Run rustfmt
                let output = std::process::Command::new("rustfmt").arg(file).output()?;

                if !output.status.success() {
                    eprintln!(
                        "rustfmt warning: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
            }
        }
        Ok(())
    }

    /// Run code checks
    async fn run_code_checks(&self, files: &[std::path::PathBuf]) -> Result<GenerationResult> {
        // TODO: Run cargo check or other code validation tools
        Ok(GenerationResult {
            generated_files: files.to_vec(),
            warnings: vec![],
            errors: vec![],
        })
    }
}

#[cfg(test)]
#[path = "pipelines_tests.rs"]
mod tests;
