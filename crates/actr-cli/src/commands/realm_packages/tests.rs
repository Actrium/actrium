use super::*;
use actr_pack::{
    ActorDependency, ActorPackOptions, KEEL_GUEST_PROFILE_V1, PackageFileEntry, pack_actor_package,
    verify_actor_package,
};
use actr_protocol::realm_profile::sign_document;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use std::{cell::RefCell, collections::HashMap};

const REALM: &str = "echo@realm.example";
const REALM_URL: &str = "https://realm.example/realms/echo";
const PROTO: &[u8] = b"syntax = \"proto3\"; package echo; service Echo {}";
const OTHER_PROTO: &[u8] = b"syntax = \"proto3\"; package other; service Other {}";
const FRONT_CONTRACT: &str = "contracts/service.proto";
const SERVICE_EXPORT: &str = "contracts/export.proto";
const EXTRA_TYPE: &str = "publisher.example/extra@1.0.0";
type Documents = RefCell<HashMap<String, Vec<u8>>>;

fn key(id: &str, key: &SigningKey) -> Value {
    json!({"id":id,"status":"active","algorithm":"Ed25519","public_key":STANDARD.encode(key.verifying_key().as_bytes())})
}

fn package(workload: &str, proto: &[u8], publisher: &SigningKey) -> (Value, Vec<u8>) {
    let is_front = workload == KEEL_GUEST_PROFILE_V1;
    let template = if is_front {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/actrium/echo/app/manifest.toml"
        ))
    } else {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../actr-pack/tests/fixtures/registration-v2.toml"
        ))
    };
    let mut m = ActorPackageManifest::from_toml(template).unwrap();
    m.workload.profile = workload.into();
    m.actor.publisher = "publisher.example".into();
    m.actor.version = "1.0.0".into();
    let contract = if is_front {
        FRONT_CONTRACT
    } else {
        SERVICE_EXPORT
    };
    m.files.push(PackageFileEntry {
        path: contract.into(),
        role: "contract".into(),
        media_type: "text/plain".into(),
        size: 0,
        sha256: "0".repeat(64),
    });
    if is_front {
        m.dependencies.insert(
            "service".into(),
            ActorDependency {
                actor: "publisher.example/echo".into(),
                version: "^1.0".into(),
                contract: Some(contract.into()),
                required: true,
            },
        );
    }
    let entry = m.workload.entry.clone();
    let ty = m.actor_type();
    let href = format!("https://cdn.example/{}.actr", m.actor.name);
    let mut payloads = BTreeMap::from([
        (entry, b"workload".to_vec()),
        (contract.into(), proto.to_vec()),
    ]);
    if !is_front {
        // A file at the Front's path is not necessarily the Service's RPC export.
        if workload == "actr.component/1" {
            m.workload.rpc_contract = Some(SERVICE_EXPORT.into());
        } else {
            m.dependencies.insert(
                "upstream".into(),
                ActorDependency {
                    actor: "publisher.example/other".into(),
                    version: "^1.0".into(),
                    contract: Some(FRONT_CONTRACT.into()),
                    required: false,
                },
            );
        }
        m.files.push(PackageFileEntry {
            path: FRONT_CONTRACT.into(),
            role: "contract".into(),
            media_type: "text/plain".into(),
            size: 0,
            sha256: "0".repeat(64),
        });
        payloads.insert(FRONT_CONTRACT.into(), OTHER_PROTO.to_vec());
        let path = "contracts/privacy.toml";
        m.files.push(PackageFileEntry {
            path: path.into(),
            role: "contract".into(),
            media_type: "application/toml".into(),
            size: 0,
            sha256: "0".repeat(64),
        });
        payloads.insert(
            path.into(),
            format!("[privacy_contract]\nschema_version = 1\nrelease_id = {ty:?}\n").into_bytes(),
        );
    }
    let bytes = pack_actor_package(&ActorPackOptions {
        manifest: m,
        signing_key: publisher.clone(),
        payloads,
    })
    .unwrap();
    (
        json!({"type":ty,"sha256":hex::encode(Sha256::digest(&bytes)),"href":href}),
        bytes,
    )
}

fn fixture(front_proto: &[u8]) -> (Documents, SigningKey, Value) {
    let rk = SigningKey::from_bytes(&rand::random());
    let pk = SigningKey::from_bytes(&rand::random());
    let root = SigningKey::from_bytes(&rand::random());
    let (service, sb) = package("actr.component/1", PROTO, &pk);
    let (front, fb) = package(KEEL_GUEST_PROFILE_V1, front_proto, &pk);
    let mut realm_keys = json!({"sequence":1,"keys":[key("realm-test",&rk)]});
    sign_document(&mut realm_keys, "realm-test", &rk);
    let rd = json!({"version":1,"authority":"actr://realm.example","services":[{"rel":"realms","href":"https://realm.example/realms","protocol":"actr-realm-profile/1"}],"realm_signing_keys":realm_keys});
    let now = chrono::Utc::now();
    let mut publisher = json!({"schema":"actr.publisher/v1","domain":"publisher.example","sequence":1,
        "profile_signing_keys":[key("pub-root",&root)],"keys":[key(&actr_protocol::package_key_id(pk.verifying_key().as_bytes()),&pk)],
        "profile_issued_at":(now-chrono::Duration::hours(1)).to_rfc3339(),"profile_expires_at":(now+chrono::Duration::days(1)).to_rfc3339()});
    sign_document(&mut publisher, "pub-root", &root);
    let pd = json!({"version":1,"authority":"actr://publisher.example","services":[{"rel":"contracts","href":"https://publisher.example/contracts"}],"publisher":publisher});
    let mut p = json!({"schema":"actr.realm/v1","sequence":1,"realm":{"name":REALM,"display_name":"Echo","status":"active"},
       "primary_package":"svc","front_package":"view","packages":{"svc":service,"view":front},
       "issuer":{"url":"https://afp.example/issuer","keys":[key("1",&rk)]},"provider":{"id":"afp.example","ice":[],"signaling":[]},
       "policy":{"membership":"invite-only"},"issued_at":"2026-01-01T00:00:00Z"});
    sign_document(&mut p, "realm-test", &rk);
    let docs = HashMap::from([
        (
            "https://realm.example/.well-known/actr".into(),
            serde_json::to_vec(&rd).unwrap(),
        ),
        (REALM_URL.into(), serde_json::to_vec(&p).unwrap()),
        (
            "https://publisher.example/.well-known/actr".into(),
            serde_json::to_vec(&pd).unwrap(),
        ),
        (service["href"].as_str().unwrap().into(), sb),
        (front["href"].as_str().unwrap().into(), fb),
    ]);
    (RefCell::new(docs), rk, p)
}

async fn from_fixture(args: &FetchArgs, docs: &Documents) -> Result<Value> {
    fetch_with(args, |url, _limit| {
        std::future::ready(
            docs.borrow()
                .get(&url)
                .cloned()
                .with_context(|| format!("unexpected fetch {url}")),
        )
    })
    .await
}

fn publish_profile(docs: &Documents, signer: &SigningKey, profile: &mut Value, sequence: u64) {
    profile["sequence"] = sequence.into();
    sign_document(profile, "realm-test", signer);
    docs.borrow_mut()
        .insert(REALM_URL.into(), serde_json::to_vec(profile).unwrap());
}

fn extra_package() -> Value {
    json!({"type": EXTRA_TYPE, "sha256": "a".repeat(64), "href": "https://cdn.example/extra.actr"})
}

#[test]
fn native_and_linked_services_match_exports_but_not_imported_contracts() {
    let signer = SigningKey::from_bytes(&rand::random());
    let manifest = |workload, proto: &[u8]| {
        let (_, bytes) = package(workload, proto, &signer);
        verify_actor_package(&bytes, &signer.verifying_key())
            .unwrap()
            .manifest
    };
    let front = manifest(KEEL_GUEST_PROFILE_V1, PROTO);
    let other_front = manifest(KEEL_GUEST_PROFILE_V1, OTHER_PROTO);
    for workload in ["actr.native/1", "actr.linked/1"] {
        let service = manifest(workload, PROTO);
        check_realm_pair(&service, &front).unwrap();
        assert!(check_realm_pair(&service, &other_front).is_err());
    }
}

#[tokio::test]
async fn fetches_a_signed_pair_and_rechecks_cached_archives() {
    let (docs, rk, mut profile) = fixture(PROTO);
    let dir = tempfile::tempdir().unwrap();
    let args = FetchArgs {
        realm: REALM.into(),
        additional: vec![],
        output: dir.path().into(),
    };
    let result = from_fixture(&args, &docs).await.unwrap();
    assert_eq!(result["packages"].as_object().unwrap().len(), 2);
    assert_eq!(
        result["privacy_contract_sha256"].as_str().unwrap().len(),
        64
    );
    let path = result["front"].as_str().unwrap();
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(
        hex::encode(Sha256::digest(&bytes)),
        profile["packages"]["view"]["sha256"]
    );
    docs.borrow_mut().retain(|name, _| !name.ends_with(".actr"));
    assert_eq!(result, from_fixture(&args, &docs).await.unwrap());
    std::fs::write(path, b"changed").unwrap();
    assert!(
        from_fixture(&args, &docs)
            .await
            .unwrap_err()
            .to_string()
            .contains("digest mismatch")
    );
    publish_profile(&docs, &rk, &mut profile, 0);
    assert!(
        from_fixture(&args, &docs)
            .await
            .unwrap_err()
            .to_string()
            .contains("sequence")
    );
}

#[tokio::test]
async fn pins_an_unrequested_package_across_removal_and_reintroduction() {
    let (docs, signer, mut profile) = fixture(PROTO);
    let dir = tempfile::tempdir().unwrap();
    let args = FetchArgs {
        realm: REALM.into(),
        additional: vec![],
        output: dir.path().into(),
    };
    let state = dir.path().join("state.json");
    let extra = extra_package();
    profile["packages"]["extra"] = extra.clone();
    publish_profile(&docs, &signer, &mut profile, 1);
    let result = from_fixture(&args, &docs).await.unwrap();
    assert!(result["packages"].get("extra").is_none());

    profile["packages"].as_object_mut().unwrap().remove("extra");
    publish_profile(&docs, &signer, &mut profile, 2);
    from_fixture(&args, &docs).await.unwrap();
    let accepted = std::fs::read(&state).unwrap();

    profile["packages"]["renamed"] = extra.clone();
    profile["packages"]["renamed"]["sha256"] = "b".repeat(64).into();
    publish_profile(&docs, &signer, &mut profile, 3);
    let error = from_fixture(&args, &docs).await.unwrap_err();
    assert_eq!(
        error.to_string(),
        format!("PACKAGE_EQUIVOCATION for {EXTRA_TYPE}")
    );
    assert_eq!(std::fs::read(&state).unwrap(), accepted);

    profile["packages"]["renamed"] = extra;
    publish_profile(&docs, &signer, &mut profile, 3);
    assert_eq!(from_fixture(&args, &docs).await.unwrap()["sequence"], 3);
}

#[tokio::test]
async fn recovers_missing_pins_before_replacing_a_cached_profile() {
    let (docs, signer, mut profile) = fixture(PROTO);
    let dir = tempfile::tempdir().unwrap();
    let args = FetchArgs {
        realm: REALM.into(),
        additional: vec![],
        output: dir.path().into(),
    };
    let state = dir.path().join("state.json");
    let read_cache = || serde_json::from_slice::<Cache>(&std::fs::read(&state).unwrap()).unwrap();
    let extra = extra_package();
    profile["packages"]["extra"] = extra.clone();
    publish_profile(&docs, &signer, &mut profile, 1);

    // Profile acceptance must survive a failed archive download.
    let url = profile["packages"]["svc"]["href"]
        .as_str()
        .unwrap()
        .to_owned();
    let archive = docs.borrow_mut().remove(&url).unwrap();
    let error = from_fixture(&args, &docs).await.unwrap_err();
    assert_eq!(error.to_string(), format!("unexpected fetch {url}"));
    let mut cache = read_cache();
    assert_eq!(
        cache.digests.remove(EXTRA_TYPE).as_deref(),
        extra["sha256"].as_str()
    );

    // Simulate a cache that retained the signed Profile but missed the auxiliary digest.
    std::fs::write(&state, serde_json::to_vec(&cache).unwrap()).unwrap();
    docs.borrow_mut().insert(url, archive);
    profile["packages"].as_object_mut().unwrap().remove("extra");
    publish_profile(&docs, &signer, &mut profile, 2);
    assert_eq!(from_fixture(&args, &docs).await.unwrap()["sequence"], 2);
    assert_eq!(read_cache().digests[EXTRA_TYPE], extra["sha256"]);
}

#[tokio::test]
async fn rejects_incompatible_signed_contracts_and_altered_package_bytes() {
    let (docs, _, profile) = fixture(OTHER_PROTO);
    let dir = tempfile::tempdir().unwrap();
    let args = FetchArgs {
        realm: REALM.into(),
        additional: vec![],
        output: dir.path().into(),
    };
    assert!(
        from_fixture(&args, &docs)
            .await
            .unwrap_err()
            .to_string()
            .contains("RPC contracts differ")
    );
    let dir = tempfile::tempdir().unwrap();
    let args = FetchArgs {
        output: dir.path().into(),
        ..args
    };
    let url = profile["packages"]["view"]["href"].as_str().unwrap();
    docs.borrow_mut().get_mut(url).unwrap().push(0);
    assert!(
        from_fixture(&args, &docs)
            .await
            .unwrap_err()
            .to_string()
            .contains("digest mismatch")
    );
}
