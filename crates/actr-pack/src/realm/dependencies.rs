use std::collections::BTreeMap;

use actr_protocol::realm_profile::RealmPackageSelection;
use semver::{Version, VersionReq};

use crate::{ActorPackageManifest, PackError, PackageFileEntry};

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
            return Err(PackError::InvalidPackage(format!(
                "{} needs {} {} for {alias}, but {} is selected.",
                reference.r#type, declared.actor, declared.version, selected.r#type
            )));
        }
        if let Some(locked) = locked_types.get(alias)
            && locked != &selected.r#type
        {
            return Err(PackError::InvalidPackage(format!(
                "{} is locked to {locked} for {alias}, but {} is selected.",
                reference.r#type, selected.r#type
            )));
        }
    }
    Ok(())
}

/// Check mapped edges between verified archives, including exported RPC contracts.
/// The scoped digests come from each verified source package's Privacy Contract and lock.
/// Callers obtain the dependency closure before calling; cycles need no ordering.
/// Unfetched, unrelated entries in the Profile are not asserted to be verified.
/// An unavailable local Front defers its export check to the user-authorized Host.
/// Its incoming aliases, selected types, locks and source contracts are still checked.
pub fn check_realm_dependencies(
    selection: &RealmPackageSelection,
    manifests: &BTreeMap<String, ActorPackageManifest>,
    locks: &BTreeMap<String, BTreeMap<String, String>>,
    scoped_digests: &BTreeMap<String, BTreeMap<String, String>>,
) -> Result<(), PackError> {
    selection.check().map_err(PackError::InvalidPackage)?;
    check_egress_cycles(selection, scoped_digests)?;
    let empty_lock = BTreeMap::new();
    for (source, manifest) in manifests {
        check_realm_package_dependencies(
            selection,
            source,
            manifest,
            locks.get(source).unwrap_or(&empty_lock),
        )?;
        for (alias, digest) in scoped_digests.get(source).unwrap_or(&empty_lock) {
            let selected = &selection.packages[source];
            match selected.dependencies.get(alias) {
                Some(target) if &selection.packages[target].sha256 != digest => {
                    return Err(PackError::InvalidPackage(format!(
                        "{} pins a different SHA-256 for {alias} than the one selected for {}.",
                        selection.packages[source].r#type, selection.packages[target].r#type
                    )));
                }
                None if manifest
                    .dependencies
                    .get(alias)
                    .is_some_and(|dep| dep.required) =>
                {
                    return Err(PackError::InvalidPackage(format!(
                        "{} needs {alias}. Choose a package for that dependency.",
                        selection.packages[source].r#type
                    )));
                }
                _ => {}
            }
        }
        for (alias, target) in &selection.packages[source].dependencies {
            let invalid = |message: &str| {
                PackError::InvalidPackage(format!("{source}.{alias} -> {target}: {message}"))
            };
            let contract = signed_dependency_contract(manifest, alias, &invalid)?;
            let Some(target_manifest) = manifests.get(target) else {
                if selection.packages[target].local_path.is_some() {
                    continue;
                }
                return Err(invalid("target archive has not been verified"));
            };
            check_verified_target(
                &selection.packages[target].r#type,
                target_manifest,
                contract,
                &invalid,
            )?;
        }
    }
    Ok(())
}

/// Complete dependencies from a verified source to a specific verified target.
/// Other mapped edges still have their source declarations checked.
pub fn check_realm_dependencies_to(
    selection: &RealmPackageSelection,
    source: &str,
    manifest: &ActorPackageManifest,
    target: &str,
    target_manifest: &ActorPackageManifest,
    locked_types: &BTreeMap<String, String>,
) -> Result<(), PackError> {
    check_realm_package_dependencies(selection, source, manifest, locked_types)?;
    for (alias, _) in selection.packages[source]
        .dependencies
        .iter()
        .filter(|(_, mapped)| mapped.as_str() == target)
    {
        let invalid = |message: &str| {
            PackError::InvalidPackage(format!("{source}.{alias} -> {target}: {message}"))
        };
        let contract = signed_dependency_contract(manifest, alias, &invalid)?;
        check_verified_target(
            &selection.packages[target].r#type,
            target_manifest,
            contract,
            &invalid,
        )?;
    }
    Ok(())
}

fn signed_dependency_contract<'a>(
    manifest: &'a ActorPackageManifest,
    alias: &str,
    invalid: &impl Fn(&str) -> PackError,
) -> Result<Option<&'a PackageFileEntry>, PackError> {
    manifest.dependencies[alias]
        .contract
        .as_ref()
        .map(|path| {
            manifest
                .files
                .iter()
                .find(|file| &file.path == path && file.role == "contract")
                .ok_or_else(|| invalid("dependency contract is not a signed contract file"))
        })
        .transpose()
}

fn check_verified_target(
    expected_type: &str,
    manifest: &ActorPackageManifest,
    contract: Option<&PackageFileEntry>,
    invalid: &impl Fn(&str) -> PackError,
) -> Result<(), PackError> {
    if manifest.actor_type() != expected_type {
        return Err(invalid(
            "target manifest type differs from its Realm selection",
        ));
    }
    if let Some(contract) = contract {
        let matches_export = if manifest.workload.profile == "actr.component/1" {
            let exported = manifest.component_rpc_contract()?;
            manifest
                .files
                .iter()
                .any(|file| Some(file.path.as_str()) == exported && file.sha256 == contract.sha256)
        } else {
            manifest
                .local_rpc_contracts()
                .any(|file| file.sha256 == contract.sha256)
        };
        if !matches_export {
            return Err(invalid("The RPC definitions do not match."));
        }
    }
    Ok(())
}

fn check_egress_cycles(
    selection: &RealmPackageSelection,
    scoped_digests: &BTreeMap<String, BTreeMap<String, String>>,
) -> Result<(), PackError> {
    let mut pending: BTreeMap<_, Vec<_>> = scoped_digests
        .iter()
        .filter_map(|(source, digests)| {
            let selected = selection.packages.get(source)?;
            let edges: Vec<_> = selected
                .dependencies
                .iter()
                .filter(|(alias, _)| digests.contains_key(*alias))
                .collect();
            Some((source, edges))
        })
        .collect();
    while !pending.is_empty() {
        let leaves: Vec<_> = pending
            .iter()
            .filter(|(_, edges)| {
                edges
                    .iter()
                    .all(|(_, target)| !pending.contains_key(target))
            })
            .map(|(source, _)| *source)
            .collect();
        if leaves.is_empty() {
            let mut source = *pending.keys().next().expect("pending graph is nonempty");
            let mut position = BTreeMap::new();
            let mut trail = Vec::new();
            loop {
                if let Some(&start) = position.get(source) {
                    return Err(PackError::InvalidPackage(format!(
                        "cyclic egress digest dependencies: {}",
                        trail[start..].join(", "),
                    )));
                }
                position.insert(source, trail.len());
                let &(alias, target) = pending[source]
                    .iter()
                    .find(|(_, target)| pending.contains_key(target))
                    .expect("every pending package has a pending dependency");
                trail.push(format!("{source}.{alias} -> {target}"));
                source = target;
            }
        }
        for leaf in leaves {
            pending.remove(leaf);
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
                            local_path: None,
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
    fn rejects_digest_dependency_cycles_without_rejecting_legacy_realm_cycles() {
        let (mut selection, _) = fixture();
        selection
            .packages
            .get_mut("target")
            .unwrap()
            .dependencies
            .insert("notebook".into(), "source".into());
        assert!(check_egress_cycles(&selection, &BTreeMap::new()).is_ok());
        let scopes = BTreeMap::from([
            (
                "source".into(),
                BTreeMap::from([("index".into(), "a".repeat(64))]),
            ),
            (
                "target".into(),
                BTreeMap::from([("notebook".into(), "b".repeat(64))]),
            ),
        ]);
        let error = check_egress_cycles(&selection, &scopes)
            .unwrap_err()
            .to_string();
        assert!(error.contains("source.index -> target"), "{error}");
        assert!(error.contains("target.notebook -> source"), "{error}");
    }

    #[test]
    fn checks_alias_version_contract_and_exact_lock() {
        let (selection, manifests) = fixture();
        check_realm_dependencies(&selection, &manifests, &BTreeMap::new(), &BTreeMap::new())
            .unwrap();
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
                check_realm_dependencies(&selection, &manifests, &locks, &BTreeMap::new()).is_err(),
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
        check_realm_dependencies(&selection, &manifests, &BTreeMap::new(), &BTreeMap::new())
            .unwrap();
        manifests.remove("target");
        assert!(
            check_realm_dependencies(&selection, &manifests, &BTreeMap::new(), &BTreeMap::new())
                .is_err()
        );
        selection
            .packages
            .get_mut("source")
            .unwrap()
            .dependencies
            .clear();
        check_realm_dependencies(&selection, &manifests, &BTreeMap::new(), &BTreeMap::new())
            .unwrap();
    }
}
