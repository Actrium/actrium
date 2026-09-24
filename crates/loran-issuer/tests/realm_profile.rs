//! Loran-signed Realm Profiles and the refresh of other domains' Realms,
//! checked with the Actr Host rules (`RealmProfile::verify` and
//! `check_successor`).

use actr_protocol::RealmSigningKeys;
use actr_protocol::realm_profile::{
    IceEndpoint, RealmPackage, RealmProfile, RealmStatus as ProfileStatus, SignalingEndpoint, jcs,
};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use ed25519_dalek::{Signer as _, SigningKey};
use issuer::realm_profile::{ProfileChange, ProfileError, ProviderEndpoints, RealmProfiles};
use issuer::signer_client_wrapper::create_signer_client;
use nonce_auth::storage::MemoryStorage;
use platform::config::signer::SignerClientConfig;
use platform::net::FetchError;
use platform::realm::{Realm, RealmError, RealmProfileRecord, RealmStatus};
use serde_json::{Value, json};
use serial_test::serial;
use signer::{KeyStorage, SignerServiceConfig, create_grpc_service};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tonic::transport::Server;

const PROVIDER: &str = "provider.example";

fn package() -> RealmPackage {
    RealmPackage {
        href: "https://cdn.example/notes-1.0.actr".to_string(),
        sha256: "ab".repeat(32),
    }
}

/// An embedded Signer, the global database, and `RealmProfiles` for
/// `provider.example` with its signing key ready.
async fn setup() -> (Arc<RealmProfiles>, TempDir) {
    // The database is process-global; keep its directory for the process.
    static DB_DIR: std::sync::OnceLock<TempDir> = std::sync::OnceLock::new();
    let db_dir = DB_DIR.get_or_init(|| TempDir::new().unwrap());
    if !platform::storage::db::is_database_initialized() {
        platform::storage::db::set_db_path(db_dir.path())
            .await
            .unwrap();
    }
    let dir = TempDir::new().unwrap();
    let pool = platform::storage::db::get_database().get_pool();
    for table in ["actoracl", "realm_profile", "realm_domain", "realm"] {
        sqlx::query(&format!("DELETE FROM {table}"))
            .execute(pool)
            .await
            .unwrap();
    }

    let shared_key = "test-shared-key";
    let service_config = SignerServiceConfig::default();
    let storage = KeyStorage::from_config(
        &service_config.storage,
        signer::KeyEncryptor::no_encryption(),
        dir.path(),
    )
    .await
    .unwrap();
    let service = create_grpc_service(
        storage,
        MemoryStorage::new(),
        shared_key.to_string(),
        service_config.tolerance_seconds,
    );
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    tokio::spawn(Server::builder().add_service(service).serve(addr));

    let signer = create_signer_client(
        &SignerClientConfig {
            endpoint: format!("http://{addr}"),
            timeout_seconds: 10,
            enable_tls: false,
            tls_domain: None,
            ca_cert: None,
            client_cert: None,
            client_key: None,
        },
        shared_key,
    )
    .await
    .unwrap();
    let profiles = Arc::new(RealmProfiles::new(
        ProviderEndpoints {
            provider_id: PROVIDER.to_string(),
            issuer_url: format!("https://{PROVIDER}/issuer"),
            signaling: vec![SignalingEndpoint {
                url: format!("wss://{PROVIDER}/signaling/ws"),
                priority: 0,
                control_plane_id: format!("{PROVIDER}/v1"),
                ticket_audience: "actr.signaling".to_string(),
            }],
            ice: vec![IceEndpoint {
                urls: vec!["turn:198.51.100.1:3478".to_string()],
                credential_audience: Some("actr.turn".to_string()),
            }],
            realms_base: format!("https://{PROVIDER}/actr/realms/"),
        },
        signer,
    ));
    for _ in 0..50 {
        if profiles.ensure_signing_keys().await.is_ok() {
            return (profiles, dir);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("Signer did not come up");
}

async fn insert(name: &str, status: RealmStatus) {
    Realm {
        name: name.to_string(),
        status,
        secret_current: "hash".to_string(),
        ..Default::default()
    }
    .insert(None)
    .await
    .unwrap();
}

#[tokio::test]
#[serial]
async fn signs_verifiable_profiles_for_the_provider_domain() {
    let (profiles, _dir) = setup().await;
    let keys = profiles.signing_keys().unwrap().clone();
    let name = format!("k7m2q9xh@{PROVIDER}");
    insert(&name, RealmStatus::Pending).await;

    // An active Profile needs a package.
    assert!(matches!(
        profiles.issue(&name, ProfileChange::default()).await,
        Err(ProfileError::Invalid(_))
    ));
    let realm = profiles
        .issue(
            &name,
            ProfileChange {
                package: Some(package()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(realm.status, RealmStatus::Active);

    let record = RealmProfileRecord::get(&name).await.unwrap().unwrap();
    let first = RealmProfile::verify(record.document.as_bytes(), &name, &keys).unwrap();
    assert_eq!(first.sequence, 1);
    assert_eq!(first.realm.display_name, "k7m2q9xh");
    assert_eq!(first.package, Some(package()));
    assert_eq!(first.provider.id, PROVIDER);
    assert_eq!(
        first.provider.signaling[0].control_plane_id,
        "provider.example/v1"
    );
    assert_eq!(first.policy.membership, "invite-only");
    assert_eq!(first.issuer.keys[0].id, format!("issuer:{name}"));
    assert_eq!(first.signature.key_id, "realm-signing:provider.example");

    profiles
        .issue(
            &name,
            ProfileChange {
                display_name: Some("Notes".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let record = RealmProfileRecord::get(&name).await.unwrap().unwrap();
    let second = RealmProfile::verify(record.document.as_bytes(), &name, &keys).unwrap();
    first.check_successor(&second).unwrap();
    assert_eq!(
        (second.sequence, second.realm.display_name.as_str()),
        (2, "Notes")
    );
    assert_eq!(second.package, Some(package()));
    assert_eq!(second.issuer.keys, first.issuer.keys);

    let realm = profiles
        .issue(
            &name,
            ProfileChange {
                retire: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(realm.status, RealmStatus::Retired);
    let record = RealmProfileRecord::get(&name).await.unwrap().unwrap();
    let retired = RealmProfile::verify(record.document.as_bytes(), &name, &keys).unwrap();
    second.check_successor(&retired).unwrap();
    assert_eq!(retired.realm.status, ProfileStatus::Retired);
    assert!(matches!(
        profiles.issue(&name, ProfileChange::default()).await,
        Err(ProfileError::Realm(RealmError::Retired))
    ));

    // Other domains' Realms are signed by their own key holders.
    insert("other@acme.com", RealmStatus::Active).await;
    assert!(matches!(
        profiles
            .issue(
                "other@acme.com",
                ProfileChange {
                    package: Some(package()),
                    ..Default::default()
                }
            )
            .await,
        Err(ProfileError::Invalid(_))
    ));
}

/// `acme.com` as its key holder: a signed `realm_signing_keys` member and
/// Realm Profiles for `provider.example`.
struct Domain {
    keys: Vec<(&'static str, SigningKey)>,
}

impl Domain {
    fn member(&self, sequence: u64, listed: &[&str]) -> Value {
        let keys: Vec<Value> = listed
            .iter()
            .map(|id| {
                let (_, key) = self.keys.iter().find(|(k, _)| k == id).unwrap();
                json!({
                    "id": id, "algorithm": "Ed25519", "status": "active",
                    "public_key": BASE64.encode(key.verifying_key().to_bytes()),
                })
            })
            .collect();
        let member = json!({ "sequence": sequence, "keys": keys });
        self.sign(member, listed[0])
    }

    fn descriptor(&self, member: &Value) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "version": 1,
            "authority": "actr://acme.com",
            "services": [{
                "rel": "realms",
                "href": "https://acme.com/realms/",
                "protocol": "actr-realm-profile/1"
            }],
            "realm_signing_keys": member,
        }))
        .unwrap()
    }

    fn profile(&self, name: &str, sequence: u64, status: &str, key_id: &str) -> Vec<u8> {
        let mut profile = json!({
            "schema": "actr.realm/v1",
            "sequence": sequence,
            "realm": { "name": name, "display_name": "Acme", "status": status },
            "issuer": { "url": format!("https://{PROVIDER}/issuer"), "keys": [] },
            "provider": { "id": PROVIDER, "signaling": [], "ice": [] },
            "policy": { "membership": "invite-only" },
            "issued_at": "2026-09-25T00:00:00Z",
        });
        if status == "active" {
            profile["package"] = json!(package());
        }
        serde_json::to_vec(&self.sign(profile, key_id)).unwrap()
    }

    fn sign(&self, mut value: Value, key_id: &str) -> Value {
        let (_, key) = self.keys.iter().find(|(k, _)| *k == key_id).unwrap();
        let signature = key.sign(jcs(&value).as_bytes());
        value["signature"] = json!({
            "key_id": key_id, "algorithm": "Ed25519",
            "value": BASE64.encode(signature.to_bytes()),
        });
        value
    }
}

/// Accept `document` as the Realm's Profile, as `publish_profile` will.
async fn accept(name: &str, document: &[u8], member: &Value) {
    let keys: RealmSigningKeys = serde_json::from_value(member.clone()).unwrap();
    platform::realm::RealmDomain::accept("acme.com", member)
        .await
        .unwrap();
    let profile = RealmProfile::verify(document, name, &keys).unwrap();
    let previous = RealmProfileRecord::get(name).await.unwrap();
    RealmProfileRecord::store(
        &profile,
        std::str::from_utf8(document).unwrap(),
        previous.as_ref().map(|record| record.digest.as_str()),
        None,
    )
    .await
    .unwrap();
}

type Documents = HashMap<String, Vec<u8>>;

async fn refresh(profiles: &RealmProfiles, documents: &Documents) {
    let fetch = |url: String| {
        let found = documents.get(&url).cloned();
        async move {
            found
                .map(Arc::from)
                .ok_or_else(|| FetchError::new(format!("404 {url}")))
        }
    };
    profiles.refresh(fetch).await.unwrap();
}

#[tokio::test]
#[serial]
async fn refreshes_other_domains_profiles_under_the_key_pin() {
    let (profiles, _dir) = setup().await;
    let acme = Domain {
        keys: vec![
            ("rs-1", SigningKey::from_bytes(&[1; 32])),
            ("rs-2", SigningKey::from_bytes(&[2; 32])),
        ],
    };
    let member = acme.member(1, &["rs-1", "rs-2"]);
    let (one, two) = ("one@acme.com", "two@acme.com");
    insert(one, RealmStatus::Active).await;
    insert(two, RealmStatus::Active).await;
    accept(one, &acme.profile(one, 1, "active", "rs-1"), &member).await;
    accept(two, &acme.profile(two, 5, "active", "rs-1"), &member).await;

    // A newer Profile is taken from the domain's realms location.
    let mut documents = Documents::new();
    documents.insert(
        "https://acme.com/.well-known/actr".into(),
        acme.descriptor(&member),
    );
    documents.insert(
        "https://acme.com/realms/one".into(),
        acme.profile(one, 2, "active", "rs-1"),
    );
    documents.insert(
        "https://acme.com/realms/two".into(),
        acme.profile(two, 4, "active", "rs-1"),
    );
    refresh(&profiles, &documents).await;
    assert_eq!(
        RealmProfileRecord::get(one)
            .await
            .unwrap()
            .unwrap()
            .sequence,
        2
    );
    // Rollback is refused.
    assert_eq!(
        RealmProfileRecord::get(two)
            .await
            .unwrap()
            .unwrap()
            .sequence,
        5
    );

    // A retired Profile retires the Realm.
    documents.insert(
        "https://acme.com/realms/one".into(),
        acme.profile(one, 3, "retired", "rs-1"),
    );
    refresh(&profiles, &documents).await;
    assert_eq!(
        Realm::get(one).await.unwrap().unwrap().status,
        RealmStatus::Retired
    );

    // rs-1 is revoked: its Profiles lose their baseline. A Profile signed by
    // a still-listed key replaces it, even at a lower sequence.
    documents.insert(
        "https://acme.com/.well-known/actr".into(),
        acme.descriptor(&acme.member(2, &["rs-2"])),
    );
    documents.insert(
        "https://acme.com/realms/two".into(),
        acme.profile(two, 1, "active", "rs-2"),
    );
    refresh(&profiles, &documents).await;
    let record = RealmProfileRecord::get(two).await.unwrap().unwrap();
    assert_eq!(record.sequence, 1);
    assert!(Realm::get(two).await.unwrap().unwrap().enabled);

    // Without any valid Profile the Realm is suspended.
    let stranger = Domain {
        keys: vec![("rs-2", SigningKey::from_bytes(&[9; 32]))],
    };
    documents.insert(
        "https://acme.com/realms/two".into(),
        stranger.profile(two, 9, "active", "rs-2"),
    );
    documents.insert(
        "https://acme.com/.well-known/actr".into(),
        acme.descriptor(&acme.member(2, &["rs-2"])),
    );
    // Make the accepted Profile unverifiable: sign it with a key the domain
    // no longer lists.
    let three = "three@acme.com";
    insert(three, RealmStatus::Active).await;
    let old: RealmSigningKeys = serde_json::from_value(member.clone()).unwrap();
    let profile =
        RealmProfile::verify(&acme.profile(three, 1, "active", "rs-1"), three, &old).unwrap();
    RealmProfileRecord::store(
        &profile,
        std::str::from_utf8(&acme.profile(three, 1, "active", "rs-1")).unwrap(),
        None,
        None,
    )
    .await
    .unwrap();
    refresh(&profiles, &documents).await;
    let suspended = Realm::get(three).await.unwrap().unwrap();
    assert_eq!(suspended.status, RealmStatus::Active);
    assert!(!suspended.enabled);
    // `two` still verifies under rs-2 and keeps serving.
    assert!(Realm::get(two).await.unwrap().unwrap().enabled);
}
