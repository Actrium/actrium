use super::*;
use crate::commands::realm_members::SendError;
use actr_pack::{
    ActorDependency, ActorPackOptions, KEEL_GUEST_PROFILE_V1, PackageFileEntry, pack_actor_package,
    verify_actor_package,
};
use actr_protocol::realm_profile::sign_document;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use std::{cell::RefCell, collections::HashMap};

mod front_e2e;

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

fn package(
    workload: &str,
    proto: &[u8],
    publisher: &SigningKey,
    name: Option<&str>,
) -> (Value, Vec<u8>) {
    package_artifact(workload, proto, publisher, "1.0.0", name)
}

fn package_version(
    workload: &str,
    proto: &[u8],
    publisher: &SigningKey,
    version: &str,
) -> (Value, Vec<u8>) {
    package_artifact(workload, proto, publisher, version, None)
}

fn package_artifact(
    workload: &str,
    proto: &[u8],
    publisher: &SigningKey,
    version: &str,
    name: Option<&str>,
) -> (Value, Vec<u8>) {
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
    m.actor.version = version.into();
    if let Some(name) = name {
        m.actor.name = name.into();
    }
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
            m.dependencies.insert(
                "upstream".into(),
                ActorDependency {
                    actor: "publisher.example/echo-ui".into(),
                    version: "^1.0".into(),
                    contract: None,
                    required: false,
                },
            );
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

fn fixture(front_proto: &[u8]) -> (Documents, SigningKey, Value, SigningKey) {
    let pk = SigningKey::from_bytes(&rand::random());
    let (documents, realm_key, profile) = fixture_with_publisher(front_proto, &pk);
    (documents, realm_key, profile, pk)
}

fn fixture_with_publisher(front_proto: &[u8], pk: &SigningKey) -> (Documents, SigningKey, Value) {
    let rk = SigningKey::from_bytes(&rand::random());
    let root = SigningKey::from_bytes(&rand::random());
    let (service, sb) = package("actr.component/1", PROTO, pk, None);
    let (front, fb) = package(KEEL_GUEST_PROFILE_V1, front_proto, pk, None);
    let mut realm_keys = json!({"sequence":1,"keys":[key("realm-test",&rk)]});
    sign_document(&mut realm_keys, "realm-test", &rk);
    let rd = json!({"version":1,"authority":"actr://realm.example","services":[{"rel":"realms","href":"https://realm.example/realms","protocol":"actr-realm-profile/1"}],"realm_signing_keys":realm_keys});
    let now = chrono::Utc::now();
    let mut publisher = json!({"schema":"actr.publisher/v1","domain":"publisher.example","sequence":1,
        "profile_signing_keys":[key("pub-root",&root)],"keys":[key(&actr_protocol::package_key_id(pk.verifying_key().as_bytes()),pk)],
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

#[tokio::test]
async fn front_release_resumes_after_a_lost_reply_and_rolls_back() -> Result<()> {
    use actr_protocol::realm_membership::{MembershipOp, MembershipRequest};
    use std::cell::Cell;

    let pk = SigningKey::from_bytes(&rand::random());
    let (docs, rk, old) = fixture_with_publisher(PROTO, &pk);
    let (mut front, bytes) = package_version(KEEL_GUEST_PROFILE_V1, PROTO, &pk, "1.1.0");
    let location = "https://cdn.example/front-v11.actr";
    front["href"] = json!(location);
    docs.borrow_mut().insert(location.to_owned(), bytes);
    let mut selection: RealmPackageSelection = serde_json::from_value(old.clone())?;
    selection
        .packages
        .insert("view".into(), serde_json::from_value(front)?);
    let output = tempfile::tempdir()?;
    let journal_path = output.path().join("package-release.json");
    let refuse_first = Cell::new(true);
    let lose_reply = Cell::new(true);
    let replies = RefCell::new(BTreeMap::<String, Value>::new());
    let get = |url, _| {
        std::future::ready(
            docs.borrow()
                .get(&url)
                .cloned()
                .with_context(|| format!("unexpected fetch {url}")),
        )
    };
    let send = |request: MembershipRequest| {
        if refuse_first.replace(false) {
            return std::future::ready(Err(SendError::Refused(anyhow::anyhow!(
                "Issuer rejected the selection"
            ))));
        }
        let result = (|| {
            assert_eq!(request.op, MembershipOp::UpdateFront);
            let update = request.front_update.unwrap();
            if let Some(reply) = replies.borrow().get(&update.operation_id) {
                return Ok(reply.clone());
            }
            let mut current: Value = serde_json::from_slice(&docs.borrow()[REALM_URL])?;
            assert_eq!(current["sequence"], update.expected_sequence);
            let previous: RealmPackageSelection = serde_json::from_value(current.clone())?;
            let target = request.selection.unwrap();
            previous
                .check_unchanged_service(&target)
                .map_err(anyhow::Error::msg)?;
            current["packages"] = serde_json::to_value(target.packages)?;
            current["primary_package"] = json!(target.primary_package);
            current["front_package"] = json!(target.front_package);
            let sequence = update.expected_sequence + 1;
            publish_profile(&docs, &rk, &mut current, sequence);
            let reply = json!({
                "operation_id": update.operation_id,
                "sequence": sequence,
                "previous_selection": previous,
            });
            replies
                .borrow_mut()
                .insert(update.operation_id, reply.clone());
            if lose_reply.replace(false) {
                bail!("reply lost after publication");
            }
            Ok(reply)
        })();
        std::future::ready(result.map_err(SendError::Unknown))
    };
    let update = |selection| {
        front::release_with(REALM, "afp.example", output.path(), selection, &get, &send)
    };
    let read_pending = || -> Result<Value> {
        let journal: Value = serde_json::from_slice(&std::fs::read(&journal_path)?)?;
        Ok(journal["pending"].clone())
    };
    let refused = update(Some(selection.clone())).await.unwrap_err();
    assert!(format!("{refused:#}").contains("Issuer rejected the selection"));
    assert!(read_pending()?.is_null());
    assert!(update(Some(selection.clone())).await.is_err());
    assert!(read_pending()?.is_object());
    assert_eq!(update(Some(selection)).await?["sequence"], 2);
    assert_eq!(replies.borrow().len(), 1);
    assert_eq!(update(None).await?["sequence"], 3);
    let published: Value = serde_json::from_slice(&docs.borrow()[REALM_URL])?;
    assert_eq!(published["packages"], old["packages"]);
    Ok(())
}

fn publish_profile(docs: &Documents, signer: &SigningKey, profile: &mut Value, sequence: u64) {
    profile["sequence"] = sequence.into();
    sign_document(profile, "realm-test", signer);
    docs.borrow_mut()
        .insert(REALM_URL.into(), serde_json::to_vec(profile).unwrap());
}

#[tokio::test]
async fn service_release_rejects_mismatched_rpc_contract() -> Result<()> {
    let publisher = SigningKey::from_bytes(&rand::random());
    let (docs, _, profile) = fixture_with_publisher(PROTO, &publisher);
    let mut selection: RealmPackageSelection = serde_json::from_value(profile)?;
    for (reference, workload, contract) in [
        ("svc", "actr.component/1", PROTO),
        ("view", KEEL_GUEST_PROFILE_V1, OTHER_PROTO),
    ] {
        let (mut entry, bytes) = package_version(workload, contract, &publisher, "1.1.0");
        let address = format!("https://cdn.example/{reference}-v11.actr");
        entry["href"] = json!(&address);
        docs.borrow_mut().insert(address, bytes);
        selection
            .packages
            .insert(reference.into(), serde_json::from_value(entry)?);
    }
    let output = tempfile::tempdir()?;
    let get =
        |url, _| std::future::ready(docs.borrow().get(&url).cloned().context("missing fixture"));
    let error = front::release_with(
        REALM,
        "afp.example",
        output.path(),
        Some(selection),
        &get,
        |_| async { panic!("an incompatible Service reached publication") },
    )
    .await
    .unwrap_err();
    assert!(
        error.to_string().contains("RPC contracts differ"),
        "{error:#}"
    );
    Ok(())
}

fn extra_package() -> Value {
    json!({"type": EXTRA_TYPE, "sha256": "a".repeat(64), "href": "https://cdn.example/extra.actr"})
}

#[test]
fn native_and_linked_services_match_exports_but_not_imported_contracts() {
    let signer = SigningKey::from_bytes(&rand::random());
    let manifest = |workload, proto: &[u8]| {
        let (_, bytes) = package(workload, proto, &signer, None);
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
    let (docs, rk, mut profile, _) = fixture(PROTO);
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
    let (docs, signer, mut profile, _) = fixture(PROTO);
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
    let (docs, signer, mut profile, _) = fixture(PROTO);
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
    let (docs, _, profile, _) = fixture(OTHER_PROTO);
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

#[tokio::test]
async fn previews_manual_selections_and_fetches_cyclic_dependency_closure() {
    let (docs, signer, mut profile, _) = fixture(PROTO);
    profile["packages"]["view"]["dependencies"] = json!({"service":"svc"});
    profile["packages"]["svc"]["dependencies"] = json!({"upstream":"view"});
    publish_profile(&docs, &signer, &mut profile, 2);
    let dir = tempfile::tempdir().unwrap();
    let fetched = from_fixture(
        &FetchArgs {
            realm: REALM.into(),
            additional: vec![],
            output: dir.path().join("fetch"),
        },
        &docs,
    )
    .await
    .unwrap();
    assert_eq!(fetched["packages"].as_object().unwrap().len(), 2);
    assert!(
        fetched["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| entry["status"] == "selected")
    );
    let selection_path = dir.path().join("selection.json");
    let mut selection = profile.clone();
    selection.as_object_mut().unwrap().retain(|key, _| {
        matches!(
            key.as_str(),
            "primary_package" | "front_package" | "packages"
        )
    });
    std::fs::write(&selection_path, serde_json::to_vec(&selection).unwrap()).unwrap();
    let trust_dir = dir.path().join("trust");
    let args = CheckArgs {
        selection: selection_path.clone(),
        output: dir.path().join("check"),
        trust_dir: Some(trust_dir.clone()),
        probe_realm: None,
        probe: ServiceProbeArgs::default(),
    };
    let check = |url: String, _limit| {
        std::future::ready(
            docs.borrow()
                .get(&url)
                .cloned()
                .with_context(|| format!("unexpected fetch {url}")),
        )
    };
    let result = check_selection_with(&args, check).await.unwrap();
    assert_eq!(result["selection"], selection);
    assert_eq!(result["dependencies"].as_array().unwrap().len(), 2);
    assert!(trust_dir.join("selection-state.json").is_file());
    assert!(!args.output.join("selection-state.json").exists());
    selection["packages"]["svc"]["dependencies"] = json!({"not-declared":"view"});
    std::fs::write(&selection_path, serde_json::to_vec(&selection).unwrap()).unwrap();
    let error = check_selection_with(&args, check).await.unwrap_err();
    assert!(
        error.to_string().contains("svc.not-declared -> view"),
        "{error}"
    );
}

#[tokio::test]
async fn traverses_local_front_dependencies_and_defers_its_incoming_export_check() {
    let (docs, realm_signer, mut profile, publisher_signer) = fixture(PROTO);
    let (extra, bytes) = package("actr.component/1", PROTO, &publisher_signer, Some("extra"));
    docs.borrow_mut()
        .insert(extra["href"].as_str().unwrap().into(), bytes);
    let front = &mut profile["packages"]["view"];
    front["href"] = json!("/Users/example/front.actr");
    front.as_object_mut().unwrap().remove("sha256");
    front["local_path"] = json!({
        "allow_rebuild": true,
        "signing_key_id": actr_protocol::package_key_id(publisher_signer.verifying_key().as_bytes())
    });
    front["dependencies"] = json!({"service":"svc","other":"extra"});
    profile["packages"]["extra"] = extra;
    profile["packages"]["svc"]["dependencies"] = json!({"upstream":"view"});
    publish_profile(&docs, &realm_signer, &mut profile, 2);

    let dir = tempfile::tempdir().unwrap();
    let result = from_fixture(
        &FetchArgs {
            realm: REALM.into(),
            output: dir.path().join("fetch"),
            additional: vec![],
        },
        &docs,
    )
    .await
    .unwrap();
    assert!(result["front"].is_null());
    assert!(result["packages"]["extra"].is_string());
    assert_eq!(result["packages"].as_object().unwrap().len(), 2);
    assert_eq!(
        result["dependencies"],
        json!([
            {"source":"extra","alias":"upstream","required":false,"target":null,"status":"unspecified"},
            {"source":"svc","alias":"upstream","required":false,"target":"view","status":"deferred"}
        ])
    );

    let selection_path = dir.path().join("selection.json");
    let write_selection = |profile: &Value| {
        let selection = json!({
            "primary_package":profile["primary_package"],
            "front_package":profile["front_package"],
            "packages":profile["packages"]
        });
        std::fs::write(&selection_path, serde_json::to_vec(&selection).unwrap()).unwrap();
    };
    write_selection(&profile);
    let args = CheckArgs {
        selection: selection_path.clone(),
        output: dir.path().join("check"),
        trust_dir: None,
        probe_realm: None,
        probe: ServiceProbeArgs::default(),
    };
    let check = |url: String, _limit| {
        std::future::ready(
            docs.borrow()
                .get(&url)
                .cloned()
                .with_context(|| format!("unexpected fetch {url}")),
        )
    };
    assert_eq!(
        check_selection_with(&args, check).await.unwrap()["dependencies"],
        result["dependencies"]
    );

    profile["packages"]["svc"]["dependencies"] = json!({"invalid":"view"});
    write_selection(&profile);
    let error = check_selection_with(&args, check).await.unwrap_err();
    assert!(error.to_string().contains("svc.invalid -> view"), "{error}");
}
