use std::collections::BTreeMap;

use actr_protocol::realm_profile::RealmPackageSelection;
use semver::{Version, VersionReq};

use crate::{ActorPackageManifest, PackError};

/// Check one verified source manifest against its Realm-selected dependencies.
/// `locked_types` comes from that source's verified dependency lock, when present.
/// Targets are package selections, not runtime instance or permission bindings.
pub fn check_realm_package_dependencies(
    selection: &RealmPackageSelection,
    source: &str,
    manifest: &ActorPackageManifest,
    locked_types: &BTreeMap<String, String>,
) -> Result<(), PackError> {
    selection.check().map_err(PackError::InvalidPackage)?;
    let reference = selection
        .packages
        .get(source)
        .ok_or_else(|| PackError::InvalidPackage(format!("unknown source package {source:?}")))?;
    if manifest.actor_type() != reference.r#type {
        return Err(PackError::InvalidPackage(format!(
            "package {source:?} manifest type differs from its Realm selection"
        )));
    }
    for (alias, target) in &reference.dependencies {
        let invalid = |message: &str| {
            PackError::InvalidPackage(format!("{source}.{alias} -> {target}: {message}"))
        };
        let declared = manifest
            .dependencies
            .get(alias)
            .ok_or_else(|| invalid("alias is not declared in the signed manifest"))?;
        let selected = &selection.packages[target];
        let actor = selected.actr_type().map_err(|e| invalid(&e))?;
        let version = Version::parse(&actor.version).map_err(|e| invalid(&e.to_string()))?;
        let constraint =
            VersionReq::parse(&declared.version).map_err(|e| invalid(&e.to_string()))?;
        if declared.actor != format!("{}/{}", actor.publisher, actor.name)
            || !constraint.matches(&version)
        {
            return Err(invalid(
                "selected type/version does not satisfy the signed manifest",
            ));
        }
        if locked_types
            .get(alias)
            .is_some_and(|locked| locked != &selected.r#type)
        {
            return Err(invalid(
                "selected type differs from the signed dependency lock",
            ));
        }
    }
    Ok(())
}

/// Check mapped edges between verified archives, including exported RPC contracts.
/// Callers obtain the dependency closure before calling; cycles need no ordering.
/// Unfetched, unrelated entries in the Profile are not asserted to be verified.
pub fn check_realm_dependencies(
    selection: &RealmPackageSelection,
    manifests: &BTreeMap<String, ActorPackageManifest>,
    locks: &BTreeMap<String, BTreeMap<String, String>>,
) -> Result<(), PackError> {
    selection.check().map_err(PackError::InvalidPackage)?;
    let empty_lock = BTreeMap::new();
    for (source, manifest) in manifests {
        check_realm_package_dependencies(
            selection,
            source,
            manifest,
            locks.get(source).unwrap_or(&empty_lock),
        )?;
        for (alias, target) in &selection.packages[source].dependencies {
            let invalid = |message: &str| {
                PackError::InvalidPackage(format!("{source}.{alias} -> {target}: {message}"))
            };
            let target_manifest = manifests
                .get(target)
                .ok_or_else(|| invalid("target archive has not been verified"))?;
            if target_manifest.actor_type() != selection.packages[target].r#type {
                return Err(invalid(
                    "target manifest type differs from its Realm selection",
                ));
            }
            if let Some(path) = &manifest.dependencies[alias].contract {
                let contract = manifest
                    .files
                    .iter()
                    .find(|file| &file.path == path && file.role == "contract")
                    .ok_or_else(|| invalid("dependency contract is not a signed contract file"))?;
                let matches_export = if target_manifest.workload.profile == "actr.component/1" {
                    let exported = target_manifest.component_rpc_contract()?;
                    target_manifest.files.iter().any(|file| {
                        Some(file.path.as_str()) == exported && file.sha256 == contract.sha256
                    })
                } else {
                    target_manifest
                        .local_rpc_contracts()
                        .any(|file| file.sha256 == contract.sha256)
                };
                if !matches_export {
                    return Err(invalid(
                        "dependency contract differs from the target RPC export",
                    ));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActorDependency, PackageFileEntry};
    use actr_protocol::realm_profile::RealmPackage;

    fn fixture() -> (
        RealmPackageSelection,
        BTreeMap<String, ActorPackageManifest>,
    ) {
        let manifest = |name: &str| {
            let mut manifest = ActorPackageManifest::from_toml(include_str!(
                "../../tests/fixtures/registration-v2.toml"
            ))
            .unwrap();
            manifest.actor.name = name.into();
            manifest.workload.rpc_contract = Some("contracts/rpc.proto".into());
            manifest.files.push(PackageFileEntry {
                path: "contracts/rpc.proto".into(),
                role: "contract".into(),
                media_type: "text/plain".into(),
                size: 1,
                sha256: "a".repeat(64),
            });
            manifest
        };
        let mut manifests: BTreeMap<String, ActorPackageManifest> = BTreeMap::from([
            ("source".into(), manifest("source")),
            ("target".into(), manifest("target")),
            ("front".into(), manifest("front")),
        ]);
        manifests.get_mut("source").unwrap().dependencies.insert(
            "index".into(),
            ActorDependency {
                actor: "v2-publisher.example/target".into(),
                version: "^1.0".into(),
                contract: Some("contracts/rpc.proto".into()),
                required: true,
            },
        );
        let mut selection = RealmPackageSelection {
            primary_package: "source".into(),
            front_package: "front".into(),
            packages: manifests
                .iter()
                .map(|(name, manifest)| {
                    (
                        name.clone(),
                        RealmPackage {
                            href: format!("https://publisher.example/{name}.actr"),
                            sha256: "a".repeat(64),
                            r#type: manifest.actor_type(),
                            dependencies: BTreeMap::new(),
                            extensions: BTreeMap::new(),
                        },
                    )
                })
                .collect(),
        };
        selection.packages.get_mut("source").unwrap().dependencies =
            BTreeMap::from([("index".into(), "target".into())]);
        (selection, manifests)
    }

    #[test]
    fn checks_alias_version_contract_and_exact_lock() {
        let (selection, manifests) = fixture();
        check_realm_dependencies(&selection, &manifests, &BTreeMap::new()).unwrap();
        for failure in ["alias", "version", "contract", "lock", "missing"] {
            let mut selection = selection.clone();
            let mut manifests = manifests.clone();
            let mut locks = BTreeMap::new();
            match failure {
                "alias" => {
                    manifests.get_mut("source").unwrap().dependencies.clear();
                }
                "version" => {
                    manifests
                        .get_mut("source")
                        .unwrap()
                        .dependencies
                        .get_mut("index")
                        .unwrap()
                        .version = "^2.0".into();
                }
                "contract" => {
                    manifests
                        .get_mut("target")
                        .unwrap()
                        .files
                        .last_mut()
                        .unwrap()
                        .sha256 = "b".repeat(64);
                }
                "lock" => {
                    locks.insert(
                        "source".into(),
                        BTreeMap::from([(
                            "index".into(),
                            "v2-publisher.example/target@1.1.0".into(),
                        )]),
                    );
                }
                "missing" => {
                    selection
                        .packages
                        .get_mut("source")
                        .unwrap()
                        .dependencies
                        .insert("index".into(), "absent".into());
                }
                _ => unreachable!(),
            }
            assert!(
                check_realm_dependencies(&selection, &manifests, &locks).is_err(),
                "{failure}"
            );
        }
    }

    #[test]
    fn accepts_partial_mappings_and_cycles_but_requires_verified_targets() {
        let (mut selection, mut manifests) = fixture();
        manifests.get_mut("target").unwrap().dependencies.insert(
            "back".into(),
            ActorDependency {
                actor: "v2-publisher.example/source".into(),
                version: "^1.0".into(),
                contract: None,
                required: false,
            },
        );
        selection
            .packages
            .get_mut("target")
            .unwrap()
            .dependencies
            .insert("back".into(), "source".into());
        check_realm_dependencies(&selection, &manifests, &BTreeMap::new()).unwrap();
        manifests.remove("target");
        assert!(check_realm_dependencies(&selection, &manifests, &BTreeMap::new()).is_err());
        selection
            .packages
            .get_mut("source")
            .unwrap()
            .dependencies
            .clear();
        check_realm_dependencies(&selection, &manifests, &BTreeMap::new()).unwrap();
    }
}
