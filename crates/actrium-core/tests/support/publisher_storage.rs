// Shared signed fixtures are linked into independent test binaries.
#![allow(dead_code)]
use actr_protocol::{KeyStatus, package_key_id};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::SigningKey;
use serde_json::{Value, json};

pub fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

pub fn public(key: &SigningKey) -> String {
    BASE64.encode(key.verifying_key().to_bytes())
}

pub fn origin(key: &SigningKey, program: &str) -> String {
    format!("signed:{}:publisher.example/{program}", public(key))
}

pub fn profile(
    sequence: u64,
    signer: (&str, &SigningKey),
    roots: &[(&str, &SigningKey, KeyStatus)],
    packages: &[(&SigningKey, KeyStatus)],
    rotations: Value,
) -> String {
    let entry = |id: String, key: &SigningKey, status: KeyStatus| json!({"id":id,"algorithm":"Ed25519","public_key":public(key),"status":status});
    let mut profile = json!({
        "schema":"actr.publisher/v1", "domain":"publisher.example", "sequence":sequence,
        "profile_issued_at":"2020-01-01T00:00:00Z", "profile_expires_at":"2100-01-01T00:00:00Z",
        "profile_signing_keys": roots.iter().map(|(id,key,status)|entry((*id).into(),key,*status)).collect::<Vec<_>>(),
        "keys":packages.iter().map(|(key,status)|entry(package_key_id(key.verifying_key().as_bytes()),key,*status)).collect::<Vec<_>>(),
        "rotations": rotations,
    });
    actr_protocol::realm_profile::sign_document(&mut profile, signer.0, signer.1);
    json!({"version":1,"authority":"actr://publisher.example","services":[{"rel":"contracts","href":"https://publisher.example/contracts"}],"publisher":profile}).to_string()
}

pub fn accepted(store: &actrium_core::GuestLocalStorage, body: String) -> String {
    store
        .accept_publisher_profile(body, "publisher.example".into(), None, None)
        .unwrap()
        .profile
        .storage_identity
        .unwrap()
}

pub fn resolved(store: &actrium_core::GuestLocalStorage, origin: String, identity: &str) -> String {
    store
        .resolve_signed_origin(origin, Some(identity.into()), None)
        .unwrap()
}

pub fn unqualified(store: &actrium_core::GuestLocalStorage, origin: String) -> String {
    store.resolve_signed_origin(origin, None, None).unwrap()
}

pub fn adopted(
    store: &actrium_core::GuestLocalStorage,
    origin: String,
    identity: &str,
) -> Result<String, actrium_core::GuestLocalStorageError> {
    let approval = store
        .storage_migration_token(origin.clone(), identity.into())
        .unwrap();
    store.resolve_signed_origin(origin, Some(identity.into()), approval)
}

pub fn stored(
    store: &actrium_core::GuestLocalStorage,
    origin: String,
    key: &str,
) -> Option<String> {
    store.get_item(origin, key.into()).unwrap()
}
