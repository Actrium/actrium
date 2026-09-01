//! Verified workload dependency aliases shared by browser guest contexts.
//!
//! Browser lifecycle hooks do not have a request-scoped
//! [`RuntimeContext`][crate::context::RuntimeContext],
//! so dependency lookup cannot use the per-dispatch context table. Instead,
//! package loading installs one immutable `alias -> ActrType` snapshot after
//! the package (including its lock file) has passed signature verification.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use actr_protocol::{ActorResult, ActrError, ActrType};
use serde::Deserialize;

// Keep this in sync with `actr_config::lock::LOCK_FILE_VERSION` without
// pulling the full native-oriented config crate into the web host.
const LOCK_FILE_VERSION: u32 = 2;

#[derive(Deserialize)]
struct LockFile {
    metadata: Option<LockMetadata>,
    #[serde(rename = "dependency", default)]
    dependencies: Vec<LockedDependency>,
}

#[derive(Deserialize)]
struct LockMetadata {
    version: u32,
    #[allow(dead_code)]
    generated_at: String,
}

#[derive(Deserialize)]
struct LockedDependency {
    name: String,
    #[serde(default)]
    alias: String,
    actr_type: String,
    #[allow(dead_code)]
    fingerprint: String,
    #[allow(dead_code)]
    cached_at: String,
    #[serde(default)]
    files: Vec<LockedProtoFile>,
}

#[derive(Deserialize)]
struct LockedProtoFile {
    #[allow(dead_code)]
    path: String,
    #[allow(dead_code)]
    fingerprint: String,
}

thread_local! {
    static DEPENDENCY_ALIASES: RefCell<HashMap<String, ActrType>> =
        RefCell::new(HashMap::new());
    static DEPENDENCY_FINGERPRINTS: RefCell<HashMap<ActrType, String>> =
        RefCell::new(HashMap::new());
}

/// Replace the currently installed dependency map.
///
/// The old map is cleared before parsing so a missing or malformed lock for a
/// replacement workload can never inherit aliases from the previous package.
pub(crate) fn install(lock_toml: Option<&str>) -> Result<(), String> {
    DEPENDENCY_ALIASES.with(|aliases| aliases.borrow_mut().clear());
    DEPENDENCY_FINGERPRINTS.with(|fingerprints| fingerprints.borrow_mut().clear());

    let Some(lock_toml) = lock_toml else {
        return Ok(());
    };

    let lock: LockFile = toml::from_str(lock_toml)
        .map_err(|error| format!("invalid manifest.lock.toml: {error}"))?;
    let version = lock
        .metadata
        .as_ref()
        .map_or(1, |metadata| metadata.version);
    if version != LOCK_FILE_VERSION {
        let migration = if version == 1 {
            "; run `actr deps install` to regenerate a v2 lock with explicit aliases"
        } else {
            ""
        };
        return Err(format!(
            "unsupported lock file version {version}; expected {LOCK_FILE_VERSION}{migration}"
        ));
    }
    let mut aliases = HashMap::with_capacity(lock.dependencies.len());
    let mut actr_types = HashSet::with_capacity(lock.dependencies.len());
    let mut fingerprints = HashMap::with_capacity(lock.dependencies.len());
    for dependency in &lock.dependencies {
        let alias = dependency.alias.trim();
        if alias.is_empty() {
            return Err(format!(
                "dependency `{}` is missing its alias",
                dependency.name
            ));
        }
        if alias != dependency.alias.as_str() {
            return Err(format!(
                "dependency alias `{}` contains leading or trailing whitespace",
                dependency.alias
            ));
        }
        let actr_type = ActrType::from_string_repr(&dependency.actr_type).map_err(|error| {
            format!(
                "dependency `{}` has invalid actr_type `{}`: {error}",
                dependency.name, dependency.actr_type
            )
        })?;
        if dependency.files.is_empty() {
            return Err(format!(
                "dependency `{}` declares no proto files",
                dependency.name
            ));
        }
        if aliases.contains_key(alias) {
            return Err(format!(
                "dependency alias `{alias}` is declared more than once"
            ));
        }
        let canonical_type = actr_type.to_string_repr();
        if !actr_types.insert(canonical_type.clone()) {
            return Err(format!(
                "dependency actr_type `{canonical_type}` is declared more than once"
            ));
        }
        fingerprints.insert(actr_type.clone(), dependency.fingerprint.clone());
        aliases.insert(alias.to_string(), actr_type);
    }

    DEPENDENCY_ALIASES.with(|installed| *installed.borrow_mut() = aliases);
    DEPENDENCY_FINGERPRINTS.with(|installed| *installed.borrow_mut() = fingerprints);
    Ok(())
}

pub(crate) fn fingerprint_for(actr_type: &ActrType) -> Option<String> {
    DEPENDENCY_FINGERPRINTS.with(|fingerprints| fingerprints.borrow().get(actr_type).cloned())
}

pub(crate) fn resolve(alias: &str) -> ActorResult<ActrType> {
    DEPENDENCY_ALIASES.with(|aliases| {
        aliases
            .borrow()
            .get(alias)
            .cloned()
            .ok_or_else(|| ActrError::DependencyNotFound {
                service_name: alias.to_string(),
                message: format!("dependency alias `{alias}` is not present in manifest.lock.toml"),
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIRST_LOCK: &str = r#"
[metadata]
version = 2
generated_at = "2026-08-06T00:00:00Z"

[[dependency]]
name = "echo-service"
alias = "echo"
actr_type = "acme:EchoService:1.2.3"
fingerprint = "service_semantic:first"
cached_at = "2026-08-06T00:00:00Z"

[[dependency.files]]
path = "echo/echo.proto"
fingerprint = "semantic:first"
"#;

    const REPLACEMENT_LOCK: &str = r#"
[metadata]
version = 2
generated_at = "2026-08-06T00:00:00Z"

[[dependency]]
name = "profile-service"
alias = "profile"
actr_type = "acme:ProfileService:2.0.0"
fingerprint = "service_semantic:second"
cached_at = "2026-08-06T00:00:00Z"

[[dependency.files]]
path = "profile/profile.proto"
fingerprint = "semantic:second"
"#;

    #[test]
    fn install_replaces_and_clears_previous_aliases() {
        install(Some(FIRST_LOCK)).expect("install first lock");
        assert_eq!(
            resolve("echo")
                .expect("resolve first alias")
                .to_string_repr(),
            "acme:EchoService:1.2.3"
        );
        assert_eq!(
            fingerprint_for(&resolve("echo").unwrap()).as_deref(),
            Some("service_semantic:first")
        );

        install(Some(REPLACEMENT_LOCK)).expect("install replacement lock");
        assert!(matches!(
            resolve("echo"),
            Err(ActrError::DependencyNotFound { .. })
        ));
        assert!(
            fingerprint_for(&ActrType::from_string_repr("acme:EchoService:1.2.3").unwrap())
                .is_none()
        );
        assert_eq!(
            resolve("profile")
                .expect("resolve replacement alias")
                .to_string_repr(),
            "acme:ProfileService:2.0.0"
        );

        install(None).expect("clear lock");
        assert!(matches!(
            resolve("profile"),
            Err(ActrError::DependencyNotFound { .. })
        ));
    }

    #[test]
    fn invalid_replacement_does_not_retain_previous_aliases() {
        install(Some(FIRST_LOCK)).expect("install first lock");
        assert!(install(Some("not valid toml = [")).is_err());
        assert!(matches!(
            resolve("echo"),
            Err(ActrError::DependencyNotFound { .. })
        ));
    }

    #[test]
    fn unsupported_version_does_not_retain_previous_aliases() {
        install(Some(FIRST_LOCK)).expect("install first lock");
        let future_lock = FIRST_LOCK.replacen("version = 2", "version = 3", 1);

        let error = install(Some(&future_lock)).expect_err("reject future lock version");
        assert!(error.contains("unsupported lock file version 3"), "{error}");
        assert!(matches!(
            resolve("echo"),
            Err(ActrError::DependencyNotFound { .. })
        ));
    }

    #[test]
    fn v1_name_is_not_used_as_an_alias() {
        install(Some(FIRST_LOCK)).expect("install first lock");
        let v1_lock = FIRST_LOCK
            .replacen("version = 2", "version = 1", 1)
            .replace("alias = \"echo\"\n", "");

        let error = install(Some(&v1_lock)).expect_err("reject v1 lock");
        assert!(error.contains("unsupported lock file version 1"), "{error}");
        assert!(error.contains("actr deps install"), "{error}");
        assert!(matches!(
            resolve("echo"),
            Err(ActrError::DependencyNotFound { .. })
        ));
        assert!(matches!(
            resolve("echo-service"),
            Err(ActrError::DependencyNotFound { .. })
        ));
    }

    #[test]
    fn incomplete_v2_schema_is_rejected_like_native() {
        let missing_metadata_timestamp =
            FIRST_LOCK.replace("generated_at = \"2026-08-06T00:00:00Z\"\n", "");
        assert!(install(Some(&missing_metadata_timestamp)).is_err());

        let missing_fingerprint =
            FIRST_LOCK.replace("fingerprint = \"service_semantic:first\"\n", "");
        assert!(install(Some(&missing_fingerprint)).is_err());

        let missing_cached_at = FIRST_LOCK.replace("cached_at = \"2026-08-06T00:00:00Z\"\n", "");
        assert!(install(Some(&missing_cached_at)).is_err());
    }
}
