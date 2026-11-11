use std::{
    collections::BTreeMap,
    io::{Cursor, Write},
    sync::OnceLock,
};

use actr_pack::{ActorPackOptions, ActorPackageManifest};
use actr_protocol::realm_profile::sign_document;
use actrium_core::{PackageSourceInfo, accept_package_publisher, inspect_downloaded_program};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use chrono::{Duration, Utc};
use ed25519_dalek::{Signer, SigningKey};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn component() -> Vec<u8> {
    static COMPONENT: OnceLock<Vec<u8>> = OnceLock::new();
    COMPONENT
        .get_or_init(|| {
            let mut resolve = wit_parser::Resolve::default();
            let pkg = resolve
                .push_str(
                    "workload.wit",
                    include_str!("../../actr-pack/wit/program-workload.wit"),
                )
                .unwrap();
            let world = resolve.packages[pkg].worlds["actr-workload-guest-v2"];
            // A source-generated stub with the complete real ABI. No opaque Wasm
            // fixture, native plugin or network/Registry service is needed.
            let mut module = wit_component::dummy_module(
                &resolve,
                world,
                wit_parser::ManglingAndAbi::Standard32,
            );
            wit_component::embed_component_metadata(
                &mut module,
                &resolve,
                world,
                wit_component::StringEncoding::UTF8,
            )
            .unwrap();
            wit_component::ComponentEncoder::default()
                .module(&module)
                .unwrap()
                .validate(true)
                .encode()
                .unwrap()
        })
        .clone()
}

fn fixture_with(service: Vec<u8>) -> ActorPackOptions {
    let proto = b"syntax = \"proto3\"; package echo; message Request { string text = 1; } service Echo { rpc Send(Request) returns (Request); }".to_vec();
    let contract = serde_json::to_vec(&json!({
        "schema":"actrium.front-contract/v1", "service_contract":"proto/service.proto",
        "service_contract_digest":format!("sha256:{}", hash(&proto)),
        "capabilities":["ui.render"], "components":["View","Text"]
    }))
    .unwrap();
    let payloads = BTreeMap::from([
        ("bin/service.wasm".into(), service),
        (
            "ui/guest.js".into(),
            b"globalThis.verifiedProgram = true;".to_vec(),
        ),
        ("ui/contract.json".into(), contract.clone()),
        ("proto/service.proto".into(), proto.clone()),
        ("assets/readme.txt".into(), b"signed asset".to_vec()),
    ]);
    let files: Vec<Value> = [
        ("bin/service.wasm", "workload", "application/wasm"),
        ("ui/guest.js", "guest", "text/javascript"),
        ("ui/contract.json", "front-contract", "application/json"),
        ("proto/service.proto", "service-contract", "text/x-protobuf"),
        ("assets/readme.txt", "asset", "text/plain"),
    ]
    .into_iter()
    .map(|(path, role, media)| {
        json!({"path":path,"role":role,"media-type":media,
        "size":payloads[path].len(), "sha256":hash(&payloads[path])})
    })
    .collect();
    let manifest: ActorPackageManifest = serde_json::from_value(json!({
        "format":"actr-package/2", "actor":{"publisher":"publisher.example.test","name":"echo","version":"1.0.0"},
        "workload":{"profile":"actrium.service-component/1","entry":"bin/service.wasm"},
        "host":{"profile":"actrium.service-host/1","api":"^1.0","required-capabilities":["actor.call","actor.discover","actor.stream","log"]},
        "signature":{"algorithm":"ed25519","key-id":"filled-by-pack"}, "files": files,
        "extensions":{"actrium":{"title":"Verified Program", "program":{
            "schema_version":1,"min_actrium_version":"1.0.0","front_host_api":"^1.0","service_host_api":"^1.0",
            "front":{"guest_runtime":"keel-js","guest_entry":"ui/guest.js","keel_api":"^1.0",
                "contract_manifest":"ui/contract.json","contract_digest":format!("sha256:{}",hash(&contract))},
            "service":{"abi":"actr:workload/workload@0.8.0","contract":"proto/service.proto",
                "protocol_fingerprint":format!("sha256:{}",hash(&proto)),
                "workspace":{"requirement":"none","provisioning":"none","access":"none","providers":[],"lease":"none"}},
            "capabilities":{"front":["ui.render"],"service":["actor.call","actor.discover","actor.stream","log"],"components":["View","Text"],"outbound_domains":[]},
            "quotas":{"guest_heap_bytes":33554432,"guest_execution_ms":1000,"render_ops":10000,
                "service_memory_bytes":67108864,"service_fuel":1000000,"service_execution_ms":5000,
                "table_elements":10000,"streams":0,"io_bytes":0,"storage_bytes":0,"network_bytes":0},
            "session":{"attachment_policy":"exclusive"}
        }}}
    })).unwrap();
    ActorPackOptions {
        manifest,
        payloads,
        signing_key: SigningKey::generate(&mut rand::rngs::OsRng),
    }
}
fn fixture() -> ActorPackOptions {
    fixture_with(component())
}

fn descriptor(f: &ActorPackOptions, root: &SigningKey, sequence: u64, listed: bool) -> String {
    let now = Utc::now();
    let public = f.signing_key.verifying_key().to_bytes();
    let root_id = format!("profile-{}", &hash(&root.verifying_key().to_bytes())[..16]);
    let mut profile = json!({
        "schema":"actr.publisher/v1", "domain":"publisher.example.test", "sequence":sequence,
        "profile_signing_keys":[{"id":root_id, "algorithm":"Ed25519","public_key":BASE64.encode(root.verifying_key().to_bytes()),"status":"active"}],
        "keys": if listed {json!([{"id":actr_pack::compute_key_id(&public),"algorithm":"Ed25519","public_key":BASE64.encode(public),"status":"active"}])} else {json!([])},
        "profile_issued_at":(now-Duration::hours(1)).to_rfc3339(),
        "profile_expires_at":(now+Duration::hours(24)).to_rfc3339()
    });
    sign_document(&mut profile, &root_id, root);
    json!({"version":1,"authority":"actr://publisher.example.test",
        "services":[{"rel":"realms","href":"https://realm.example.test/realms","protocol":"actr-realm-profile/1"}],
        "publisher":profile}).to_string()
}
fn source(bytes: &[u8]) -> PackageSourceInfo {
    PackageSourceInfo {
        href: "https://cdn.example.test/echo.actr".into(),
        expected_sha256: Some(hash(bytes)),
        expected_actor_type: Some("publisher.example.test/echo@1.0.0".into()),
    }
}
fn hostile(f: &ActorPackOptions) -> Vec<u8> {
    let raw = toml::to_string(&f.manifest).unwrap();
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let store =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, data) in [
        ("manifest.toml", raw.as_bytes().to_vec()),
        (
            "manifest.sig",
            f.signing_key.sign(raw.as_bytes()).to_bytes().to_vec(),
        ),
    ]
    .into_iter()
    .chain(f.payloads.iter().map(|(p, b)| (p.as_str(), b.clone())))
    {
        zip.start_file(name, store).unwrap();
        zip.write_all(&data).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn trusted_program_returns_exact_local_bytes_without_registry_or_private_publisher_key() {
    let f = fixture();
    let bytes = actr_pack::pack_actor_package(&f).unwrap();
    let root = SigningKey::generate(&mut rand::rngs::OsRng);
    let identity = inspect_downloaded_program(source(&bytes), bytes.clone()).unwrap();
    assert_eq!(
        identity.bootstrap_url,
        "https://publisher.example.test/.well-known/actr"
    );
    let descriptor = descriptor(&f, &root, 1, true);
    if let Ok(path) = std::env::var("ACTRIUM_EXPORT_TEST_FIXTURE") {
        let path = std::path::Path::new(&path);
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(path.join("program.actr"), &bytes).unwrap();
        std::fs::write(path.join("publisher.json"), &descriptor).unwrap();
        let mut invalid = fixture();
        invalid.manifest = f.manifest.clone();
        invalid.manifest.signature.key_id =
            actr_pack::compute_key_id(&f.signing_key.verifying_key().to_bytes());
        std::fs::write(path.join("invalid-signature.actr"), hostile(&invalid)).unwrap();
    }
    let publisher = accept_package_publisher(identity.domain, descriptor, None).unwrap();
    let verified = publisher.verify_program(source(&bytes), bytes).unwrap();
    assert_eq!(verified.guest_code.as_bytes(), f.payloads["ui/guest.js"]);
    assert_eq!(verified.service_bytes, f.payloads["bin/service.wasm"]);
    assert_eq!(verified.publisher, "publisher.example.test");
    assert!(verified.metadata_json.contains("guest_heap_bytes"));
    assert_eq!(
        verified
            .resources
            .iter()
            .find(|r| r.path == "assets/readme.txt")
            .unwrap()
            .bytes,
        b"signed asset"
    );
}

#[test]
fn accepted_publisher_history_survives_rejected_packages_and_prevents_rollback_or_unapproved_roots()
{
    let f = fixture();
    let root = SigningKey::generate(&mut rand::rngs::OsRng);
    let first = accept_package_publisher(
        "publisher.example.test".into(),
        descriptor(&f, &root, 1, true),
        None,
    )
    .unwrap();
    let revoked = accept_package_publisher(
        "publisher.example.test".into(),
        descriptor(&f, &root, 2, false),
        Some(first.pins_json()),
    )
    .unwrap();
    let bytes = actr_pack::pack_actor_package(&f).unwrap();
    assert!(
        revoked
            .verify_program(source(&bytes), bytes)
            .unwrap_err()
            .to_string()
            .contains("PACKAGE_KEY_REVOKED")
    );
    assert!(
        accept_package_publisher(
            "publisher.example.test".into(),
            descriptor(&f, &root, 1, true),
            Some(revoked.pins_json())
        )
        .is_err()
    );
    let other = SigningKey::generate(&mut rand::rngs::OsRng);
    let error = accept_package_publisher(
        "publisher.example.test".into(),
        descriptor(&f, &other, 3, true),
        Some(revoked.pins_json()),
    )
    .err()
    .unwrap();
    assert!(error.to_string().contains("PUBLISHER_IDENTITY_CHANGED"));
    assert!(
        accept_package_publisher(
            "publisher.example.test".into(),
            descriptor(&f, &root, 3, true),
            Some("corrupted".into())
        )
        .is_err()
    );
}

#[test]
fn tampering_wrong_selection_signature_and_executable_policy_never_produce_a_result() {
    for case in 0..11 {
        let mut f = fixture();
        let root = SigningKey::generate(&mut rand::rngs::OsRng);
        let publisher = accept_package_publisher(
            "publisher.example.test".into(),
            descriptor(&f, &root, 1, true),
            None,
        )
        .unwrap();
        match case {
            0 => f.payloads.get_mut("ui/guest.js").unwrap().push(b'!'),
            1 => f.manifest.signature.key_id = "unlisted".into(),
            2 => {
                f.manifest
                    .extensions
                    .actrium
                    .as_mut()
                    .unwrap()
                    .program
                    .as_mut()
                    .unwrap()
                    .front_host_api = "^99.0".into()
            }
            3 => {
                f.payloads.insert("../escape".into(), vec![1]);
            }
            4 => {
                f.signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
            }
            5 => {
                f.manifest
                    .extensions
                    .actrium
                    .as_mut()
                    .unwrap()
                    .program
                    .as_mut()
                    .unwrap()
                    .service
                    .abi = "unsupported".into()
            }
            6 => f.manifest.workload.entry = "missing.wasm".into(),
            7 => f.manifest.actor.publisher = "other.example.test".into(),
            _ => {}
        }
        let bytes = hostile(&f);
        let mut selection = source(&bytes);
        if case == 8 {
            selection.expected_sha256 = Some("0".repeat(64));
        }
        if case == 9 {
            selection.expected_actor_type = Some("publisher.example.test/other@1.0.0".into());
        }
        if case == 10 {
            selection.href = "http://cdn.example.test/echo.actr".into();
        }
        assert!(
            publisher.verify_program(selection, bytes).is_err(),
            "accepted case {case}"
        );
    }
}

#[test]
#[ignore = "requires a source-built SDK Service Component; see trusted-package-loading.md"]
fn real_source_built_service_is_accepted_when_requested() {
    let path = std::env::var("ACTRIUM_TEST_SERVICE_COMPONENT")
        .expect("set ACTRIUM_TEST_SERVICE_COMPONENT");
    let f = fixture_with(std::fs::read(path).unwrap());
    let bytes = actr_pack::pack_actor_package(&f).unwrap();
    let root = SigningKey::generate(&mut rand::rngs::OsRng);
    let publisher = accept_package_publisher(
        "publisher.example.test".into(),
        descriptor(&f, &root, 1, true),
        None,
    )
    .unwrap();
    assert_eq!(
        publisher
            .verify_program(source(&bytes), bytes)
            .unwrap()
            .service_bytes,
        f.payloads["bin/service.wasm"]
    );
}
