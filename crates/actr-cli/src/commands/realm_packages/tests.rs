use super::*;
use actr_pack::{ActorDependency, ActorPackOptions, PackageFileEntry, pack_actor_package};
use actr_protocol::realm_profile::sign_document;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use std::{cell::RefCell, collections::HashMap};

const REALM: &str = "echo@realm.example";
const REALM_URL: &str = "https://realm.example/realms/echo";
const PROTO: &[u8] = b"syntax = \"proto3\"; package echo; service Echo {}";
type Documents = RefCell<HashMap<String, Vec<u8>>>;

fn key(id: &str, key: &SigningKey) -> Value {
    json!({"id":id,"status":"active","algorithm":"Ed25519","public_key":STANDARD.encode(key.verifying_key().as_bytes())})
}

fn package(is_front: bool, proto: &[u8], publisher: &SigningKey) -> (Value, Vec<u8>) {
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
    m.actor.publisher = "publisher.example".into();
    m.actor.version = "1.0.0".into();
    let contract = "contracts/echo.proto";
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
    let (service, sb) = package(false, PROTO, &pk);
    let (front, fb) = package(true, front_proto, &pk);
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
    profile["sequence"] = 0.into();
    sign_document(&mut profile, "realm-test", &rk);
    docs.borrow_mut()
        .insert(REALM_URL.into(), serde_json::to_vec(&profile).unwrap());
    assert!(
        from_fixture(&args, &docs)
            .await
            .unwrap_err()
            .to_string()
            .contains("sequence")
    );
}

#[tokio::test]
async fn rejects_incompatible_signed_contracts_and_altered_package_bytes() {
    let (docs, _, profile) = fixture(b"syntax = \"proto3\"; package other; service Other {}");
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
