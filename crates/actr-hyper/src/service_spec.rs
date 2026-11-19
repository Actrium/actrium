//! Derive [`actr_protocol::ServiceSpec`] from a verified `.actr` package.
//!
//! Moved out of `actr-pack` because it pulls in `actr-service-compat` (and
//! therefore `proto-fingerprint`), which is not wasm-friendly. `actr-pack`
//! stays wasm-compatible for the SW runtime. Specs are derived here during
//! verified v2 package loading and legacy package registration.
//!
//! Spec assembly lives in `actr-service-compat`: v2 uses its self-contained
//! signed-source builder, while legacy packages use its batch builder. This
//! module selects declared contracts and extracts their archive contents.

use actr_pack::{ActorPackageManifest, PackageManifest};
use actr_protocol::ServiceSpec;
use actr_service_compat::{
    ProtoFile, ServiceSpecInput, build_self_contained_service_spec,
    build_self_contained_service_spec_from_files, build_service_spec,
};
use std::io::Read;

use crate::error::{HyperError, HyperResult};

/// Build the v2 spec from its signed, self-contained RPC Proto exports.
///
/// Native packages publish every local export, excluding dependency contracts.
/// Components retain their explicit selector and ambiguity rules.
pub(crate) fn calculate_v2_service_spec(
    bytes: &[u8],
    manifest: &ActorPackageManifest,
    max_bytes: usize,
) -> HyperResult<Option<ServiceSpec>> {
    // Also reject workload.rpc-contract on non-Component profiles.
    let selected = manifest
        .component_rpc_contract()
        .map_err(|e| HyperError::InvalidManifest(e.to_string()))?;
    let spec = if manifest.workload.profile == "actr.native/1" {
        let proto_files = manifest
            .local_rpc_contracts()
            .map(|file| read_v2_rpc_proto(bytes, manifest, &file.path, max_bytes))
            .collect::<HyperResult<Vec<_>>>()?;
        if proto_files.is_empty() {
            return Ok(None);
        }
        build_self_contained_service_spec_from_files(&manifest.actor.name, proto_files)
    } else {
        let Some(path) = selected else {
            return Ok(None);
        };
        let proto = read_v2_rpc_proto(bytes, manifest, path, max_bytes)?;
        build_self_contained_service_spec(&manifest.actor.name, proto)
    };
    spec.map(Some)
        .map_err(|e| HyperError::InvalidManifest(format!("build v2 service spec: {e}")))
}

fn read_v2_rpc_proto(
    bytes: &[u8],
    manifest: &ActorPackageManifest,
    path: &str,
    max_bytes: usize,
) -> HyperResult<ProtoFile> {
    let content = manifest
        .read_declared_file(bytes, path, max_bytes)
        .map_err(|e| HyperError::InvalidManifest(e.to_string()))?;
    let content = String::from_utf8(content)
        .map_err(|e| HyperError::InvalidManifest(format!("read Proto `{path}` as UTF-8: {e}")))?;
    Ok(ProtoFile {
        name: path.rsplit('/').next().unwrap_or(path).to_owned(),
        content,
        path: None,
    })
}

/// Build a [`ServiceSpec`] from the package's proto exports.
///
/// Returns `Ok(None)` when the manifest declares no proto files — such
/// packages register with Issuer without a service spec.
pub(crate) fn calculate_service_spec_from_package(
    package_bytes: &[u8],
    manifest: &PackageManifest,
) -> HyperResult<Option<ServiceSpec>> {
    if manifest.proto_files.is_empty() {
        return Ok(None);
    }

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(package_bytes))
        .map_err(|e| HyperError::Runtime(format!("open package ZIP: {e}")))?;

    let mut proto_files = Vec::with_capacity(manifest.proto_files.len());
    for proto_entry in &manifest.proto_files {
        let mut file = archive.by_name(&proto_entry.path).map_err(|e| {
            HyperError::Runtime(format!(
                "proto file {} not found in package: {e}",
                proto_entry.path
            ))
        })?;
        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|e| HyperError::Runtime(format!("read proto {}: {e}", proto_entry.path)))?;
        proto_files.push(ProtoFile {
            name: proto_entry.name.clone(),
            content,
            path: None,
        });
    }

    let spec = build_service_spec(ServiceSpecInput {
        name: &manifest.name,
        description: manifest.metadata.description.clone(),
        tags: vec![],
        proto_files,
    })
    .map_err(|e| HyperError::Runtime(format!("build service spec: {e}")))?;

    Ok(Some(spec))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actr_pack::{ActorDependency, ActorPackOptions, PackageFileEntry};
    use std::collections::BTreeMap;

    const NATIVE: &str = "actr.native/1";
    const COMPONENT: &str = "actr.component/1";
    const LIMIT: usize = actr_pack::DEFAULT_MAX_VERIFIED_ENTRY_BYTES;
    const RPC_PATH: &str = "contracts/echo.proto";
    const DEPENDENCY_PATH: &str = "contracts/dependency.proto";
    const CONTRACT: &str = r#"syntax = "proto3";
package echo;
message Request { string text = 1; }
service Echo { rpc Echo(Request) returns (Request); }
"#;
    const OTHER_CONTRACT: &str = r#"syntax = "proto3";
package other;
message Request { uint64 value = 1; }
service Other { rpc Call(Request) returns (Request); }
"#;

    fn add_dependency(manifest: &mut ActorPackageManifest) {
        manifest.dependencies.insert(
            "upstream".into(),
            ActorDependency {
                actor: "example.com/Upstream".into(),
                version: "^1.0.0".into(),
                contract: Some(DEPENDENCY_PATH.into()),
                required: true,
            },
        );
    }

    fn signed_fixture(
        profile: &str,
        contracts: &[(&str, &[u8])],
        configure: impl FnOnce(&mut ActorPackageManifest),
    ) -> (Vec<u8>, ActorPackageManifest) {
        let mut manifest = ActorPackageManifest::from_toml(include_str!(
            "../../actr-pack/tests/fixtures/registration-v2.toml"
        ))
        .unwrap();
        manifest.workload.profile = profile.into();
        if profile == NATIVE {
            manifest.workload.entry = "workload/echo.so".into();
            manifest.files[0].path = manifest.workload.entry.clone();
            manifest.files[0].media_type = "application/octet-stream".into();
        }
        let mut payloads = BTreeMap::from([(manifest.workload.entry.clone(), Vec::new())]);
        for &(path, content) in contracts {
            manifest.files.push(PackageFileEntry {
                path: path.into(),
                role: "contract".into(),
                media_type: "text/plain".into(),
                size: 0,
                sha256: "0".repeat(64),
            });
            payloads.insert(path.into(), content.to_vec());
        }
        configure(&mut manifest);
        let key = ed25519_dalek::SigningKey::from_bytes(&[7; 32]);
        let bytes = actr_pack::pack_actor_package(&ActorPackOptions {
            manifest,
            payloads,
            signing_key: key.clone(),
        })
        .unwrap();
        let verified = actr_pack::verify_actor_package(&bytes, &key.verifying_key()).unwrap();
        (bytes, verified.manifest)
    }

    #[test]
    fn native_single_signed_export_uses_the_self_contained_spec_builder() {
        let (bytes, manifest) = signed_fixture(NATIVE, &[(RPC_PATH, CONTRACT.as_bytes())], |_| {});
        let spec = calculate_v2_service_spec(&bytes, &manifest, LIMIT)
            .unwrap()
            .unwrap();
        let expected = build_self_contained_service_spec(
            &manifest.actor.name,
            ProtoFile {
                name: "echo.proto".into(),
                content: CONTRACT.into(),
                path: None,
            },
        )
        .unwrap();
        assert_eq!(spec.name, "echo");
        assert_eq!(spec.fingerprint, expected.fingerprint);
        assert_eq!(spec.protobufs, expected.protobufs);
        assert_eq!(spec.protobufs.len(), 1);
        assert_eq!(spec.protobufs[0].content, CONTRACT);
    }

    #[test]
    fn no_local_exports_produce_no_spec() {
        for profile in [NATIVE, COMPONENT] {
            let (bytes, manifest) = signed_fixture(profile, &[], |_| {});
            assert!(
                calculate_v2_service_spec(&bytes, &manifest, LIMIT)
                    .unwrap()
                    .is_none()
            );

            // Neither an imported dependency nor a non-Proto contract is an export.
            let (bytes, manifest) = signed_fixture(
                profile,
                &[
                    (DEPENDENCY_PATH, &[0xff]),
                    ("contracts/schema.json", &[0xff]),
                ],
                add_dependency,
            );
            assert!(
                calculate_v2_service_spec(&bytes, &manifest, LIMIT)
                    .unwrap()
                    .is_none()
            );
        }
    }

    #[test]
    fn dependency_contracts_are_excluded_unless_a_component_explicitly_reexports_one() {
        for (profile, selected, expected) in [
            (NATIVE, None, CONTRACT),
            (COMPONENT, None, CONTRACT),
            (COMPONENT, Some(DEPENDENCY_PATH), OTHER_CONTRACT),
        ] {
            let (bytes, manifest) = signed_fixture(
                profile,
                &[
                    (DEPENDENCY_PATH, OTHER_CONTRACT.as_bytes()),
                    (RPC_PATH, CONTRACT.as_bytes()),
                ],
                |manifest| {
                    add_dependency(manifest);
                    manifest.workload.rpc_contract = selected.map(str::to_owned);
                },
            );
            let spec = calculate_v2_service_spec(&bytes, &manifest, LIMIT)
                .unwrap()
                .unwrap();
            assert_eq!(spec.protobufs.len(), 1);
            assert_eq!(spec.protobufs[0].content, expected);
        }
    }

    #[test]
    fn multiple_native_exports_are_all_published_with_an_order_independent_fingerprint() {
        let mut contracts = [
            (RPC_PATH, CONTRACT.as_bytes()),
            ("contracts/other.proto", OTHER_CONTRACT.as_bytes()),
        ];
        let mut previous_fingerprint = None;
        for _ in 0..2 {
            let (bytes, manifest) = signed_fixture(NATIVE, &contracts, |_| {});
            let spec = calculate_v2_service_spec(&bytes, &manifest, LIMIT)
                .unwrap()
                .unwrap();
            let mut published = spec
                .protobufs
                .iter()
                .map(|proto| (proto.package.as_str(), proto.content.as_str()))
                .collect::<Vec<_>>();
            published.sort();
            assert_eq!(
                published,
                vec![("echo", CONTRACT), ("other", OTHER_CONTRACT)]
            );
            assert!(
                spec.protobufs
                    .iter()
                    .all(|proto| proto.fingerprint.starts_with("semantic:"))
            );
            if let Some(expected) = &previous_fingerprint {
                assert_eq!(&spec.fingerprint, expected);
            }
            previous_fingerprint = Some(spec.fingerprint.clone());
            contracts.reverse();
        }
    }

    #[test]
    fn native_exports_with_conflicting_published_basenames_are_rejected() {
        for [first, second] in [
            ["echo.proto", "echo.proto"],
            ["echo.proto", "Echo.proto"],
            ["echo.proto", "ECHO.proto"],
            ["echo.proto", "echo.proto.proto"],
            ["\u{e9}cho.proto", "e\u{301}cho.proto"],
            ["\u{c9}cho.proto.proto", "e\u{301}cho.proto"],
            ["\u{3a3}.proto", "\u{3c2}.proto"],
            ["Stra\u{df}e.proto", "STRASSE.proto"],
        ] {
            let first_path = format!("contracts/first/{first}");
            let second_path = format!("contracts/second/{second}");
            let (bytes, manifest) = signed_fixture(
                NATIVE,
                &[
                    (&first_path, CONTRACT.as_bytes()),
                    (&second_path, OTHER_CONTRACT.as_bytes()),
                ],
                |_| {},
            );
            let error = calculate_v2_service_spec(&bytes, &manifest, LIMIT).unwrap_err();
            assert!(matches!(&error, HyperError::InvalidManifest(_)));
            let error = error.to_string();
            assert!(error.contains(second), "{error}");
            assert!(error.contains("duplicate published Proto name"), "{error}");
        }
    }

    #[test]
    fn component_selection_and_native_selector_restrictions_are_preserved() {
        let (bytes, mut manifest) = signed_fixture(
            COMPONENT,
            &[
                (RPC_PATH, CONTRACT.as_bytes()),
                ("contracts/other.proto", OTHER_CONTRACT.as_bytes()),
            ],
            |manifest| manifest.workload.rpc_contract = Some("contracts/other.proto".into()),
        );
        let spec = calculate_v2_service_spec(&bytes, &manifest, LIMIT)
            .unwrap()
            .unwrap();
        assert_eq!(spec.protobufs.len(), 1);
        assert_eq!(spec.protobufs[0].content, OTHER_CONTRACT);

        // Invalid selectors must fail before any archive entry is read.
        manifest.workload.rpc_contract = None;
        let error = calculate_v2_service_spec(&[], &manifest, LIMIT)
            .unwrap_err()
            .to_string();
        assert!(error.contains("multiple RPC Proto exports"), "{error}");
        manifest.workload.profile = NATIVE.into();
        manifest.workload.rpc_contract = Some(RPC_PATH.into());
        let error = manifest.validate().unwrap_err().to_string();
        assert!(
            error.contains("requires the actr.component/1 profile"),
            "{error}"
        );
        let error = calculate_v2_service_spec(&[], &manifest, LIMIT)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("requires the actr.component/1 profile"),
            "{error}"
        );
    }

    #[test]
    fn signed_rpc_exports_must_be_utf8_and_fit_the_read_limit() {
        for profile in [NATIVE, COMPONENT] {
            let (bytes, manifest) = signed_fixture(profile, &[(RPC_PATH, &[0xff])], |_| {});
            let error = calculate_v2_service_spec(&bytes, &manifest, LIMIT)
                .unwrap_err()
                .to_string();
            assert!(error.contains(RPC_PATH), "{error}");
            assert!(error.contains("UTF-8"), "{error}");

            let (bytes, manifest) =
                signed_fixture(profile, &[(RPC_PATH, CONTRACT.as_bytes())], |_| {});
            assert!(
                calculate_v2_service_spec(&bytes, &manifest, CONTRACT.len())
                    .unwrap()
                    .is_some()
            );
            let error = calculate_v2_service_spec(&bytes, &manifest, CONTRACT.len() - 1)
                .unwrap_err()
                .to_string();
            assert!(error.contains(RPC_PATH), "{error}");
            assert!(error.contains("exceeds limit"), "{error}");
        }
    }

    #[test]
    fn signed_rpc_exports_reject_imports_and_unresolved_types() {
        for profile in [NATIVE, COMPONENT] {
            for import in [
                r#"import "unsigned.proto";"#,
                r#"import public "unsigned.proto";"#,
                r#"import weak "unsigned.proto";"#,
            ] {
                let source = format!("syntax = \"proto3\"; {import}\nmessage Request {{}}\n");
                let (bytes, manifest) =
                    signed_fixture(profile, &[(RPC_PATH, source.as_bytes())], |_| {});
                let error = calculate_v2_service_spec(&bytes, &manifest, LIMIT)
                    .unwrap_err()
                    .to_string();
                assert!(error.contains("imports are unsupported"), "{error}");
            }
            let source = r#"syntax = "proto3"; message Request { Missing value = 1; }"#;
            let (bytes, manifest) =
                signed_fixture(profile, &[(RPC_PATH, source.as_bytes())], |_| {});
            assert!(calculate_v2_service_spec(&bytes, &manifest, LIMIT).is_err());
        }
    }

    #[test]
    fn import_like_comments_preserve_the_signed_source_and_fingerprint() {
        let source = format!("/*\nimport \"not-an-import.proto\";\n*/\n{CONTRACT}");
        for profile in [NATIVE, COMPONENT] {
            let (bytes, manifest) =
                signed_fixture(profile, &[(RPC_PATH, source.as_bytes())], |_| {});
            let spec = calculate_v2_service_spec(&bytes, &manifest, LIMIT)
                .unwrap()
                .unwrap();
            let expected = build_self_contained_service_spec(
                &manifest.actor.name,
                ProtoFile {
                    name: "echo.proto".into(),
                    content: CONTRACT.into(),
                    path: None,
                },
            )
            .unwrap();
            assert_eq!(spec.fingerprint, expected.fingerprint);
            assert_eq!(spec.protobufs[0].content, source);
        }
    }
}
