#[path = "support/publisher_storage.rs"]
mod support;
use actr_protocol::{
    KeyRotation,
    KeyStatus::{Active, Retired},
    rotation_payload,
};
use actrium_core::GuestLocalStorage;
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{Signer, SigningKey};
use serde_json::{Value, json};
use support::{accepted, key, origin, profile, public, resolved, stored, unqualified};

#[test]
fn verified_package_and_root_rotations_keep_one_storage_identity_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("storage.sqlite");
    let path = path.to_str().unwrap().to_owned();
    let (root, next_root, package, next_package) = (key(1), key(2), key(3), key(4));
    let store = GuestLocalStorage::new(path.clone());
    let identity = accepted(&store, single_profile(1, "root", &root, &package));
    let canonical = resolved(&store, origin(&package, "counter"), &identity);
    store
        .set_item(canonical.clone(), "theme".into(), "dark".into())
        .unwrap();
    drop(store);
    let reopened = GuestLocalStorage::new(path);
    let second = profile(
        2,
        ("next", &next_root),
        &[("next", &next_root, Active), ("root", &root, Retired)],
        &[(&package, Retired), (&next_package, Active)],
        rotation(("root", &root), ("next", &next_root), 2),
    );
    assert_eq!(accepted(&reopened, second.clone()), identity);
    assert_eq!(
        resolved(&reopened, origin(&next_package, "counter"), &identity),
        canonical
    );
    assert_eq!(
        stored(&reopened, canonical.clone(), "theme").as_deref(),
        Some("dark")
    );
    accepted(&reopened, second);
    reopened
        .remove_item(canonical.clone(), "theme".into())
        .unwrap();
    assert_eq!(stored(&reopened, canonical, "theme"), None);
}

#[test]
fn rejected_profiles_do_not_change_identity_or_grant_another_key_storage_access() {
    let (root, other_root, package, other_package) = (key(11), key(12), key(13), key(14));
    let store = GuestLocalStorage::new(":memory:".into());
    let identity = accepted(&store, single_profile(2, "root", &root, &package));
    let canonical = resolved(&store, origin(&package, "counter"), &identity);
    store
        .set_item(canonical.clone(), "secret".into(), "original".into())
        .unwrap();
    for body in [
        single_profile(1, "root", &root, &other_package),
        single_profile(2, "root", &root, &other_package),
        single_profile(3, "other", &other_root, &other_package),
    ] {
        assert!(
            store
                .accept_publisher_profile(body, "publisher.example".into(), None)
                .is_err()
        );
        assert_eq!(
            resolved(&store, origin(&package, "counter"), &identity),
            canonical
        );
        assert_eq!(
            unqualified(&store, origin(&other_package, "counter")),
            origin(&other_package, "counter")
        );
        assert_eq!(
            stored(&store, origin(&other_package, "counter"), "secret"),
            None
        );
    }
}

#[test]
fn invalid_signatures_domains_and_expired_profiles_are_rejected() {
    let (root, package) = (key(21), key(22));
    let valid = single_profile(1, "root", &root, &package);
    let mut tampered: Value = serde_json::from_str(&valid).unwrap();
    tampered["publisher"]["sequence"] = json!(100);
    let store = GuestLocalStorage::new(":memory:".into());
    assert!(
        store
            .accept_publisher_profile(tampered.to_string(), "publisher.example".into(), None)
            .is_err()
    );
    assert!(
        store
            .accept_publisher_profile(valid.clone(), "other.example".into(), None)
            .is_err()
    );
    let mut expired: Value = serde_json::from_str(&valid).unwrap();
    expired["publisher"]["profile_expires_at"] = json!("2001-01-01T00:00:00Z");
    actr_protocol::realm_profile::sign_document(&mut expired["publisher"], "root", &root);
    assert!(
        store
            .accept_publisher_profile(expired.to_string(), "publisher.example".into(), None)
            .is_err()
    );
    accepted(&store, valid);
}

#[test]
fn a_profile_listing_an_independent_key_does_not_adopt_its_local_package() {
    let store = GuestLocalStorage::new(":memory:".into());
    let package = key(52);
    let raw = origin(&package, "counter");
    let root = key(51);
    let identity = accepted(&store, single_profile(1, "root", &root, &package));
    let network = resolved(&store, raw.clone(), &identity);
    assert_eq!(unqualified(&store, raw.clone()), raw);
    store
        .set_item(raw.clone(), "local".into(), "private".into())
        .unwrap();
    assert_eq!(resolved(&store, raw.clone(), &identity), network);
    assert_eq!(stored(&store, network, "local"), None);
    assert_eq!(stored(&store, raw, "local").as_deref(), Some("private"));
}

#[test]
fn non_network_host_origins_keep_their_existing_isolation() {
    let store = GuestLocalStorage::new(":memory:".into());
    for raw in [
        "dev:/some/app".to_owned(),
        format!("signed:{}:PrivatePublisher/counter", public(&key(50))),
    ] {
        assert_eq!(unqualified(&store, raw.clone()), raw);
    }
}

#[test]
fn initial_storage_enrollment_preserves_existing_host_pins() {
    let (root, package) = (key(71), key(72));
    let first = single_profile(1, "root", &root, &package);
    let previous = actrium_core::accept_realm_package_publisher(
        first.clone(),
        "publisher.example".into(),
        None,
    )
    .unwrap();
    let store = GuestLocalStorage::new(":memory:".into());
    let changed = single_profile(2, "other", &key(73), &key(74));
    assert!(
        store
            .accept_publisher_profile(
                changed,
                "publisher.example".into(),
                Some(previous.pins_json.clone())
            )
            .is_err()
    );
    let accepted = store
        .accept_publisher_profile(first, "publisher.example".into(), Some(previous.pins_json))
        .unwrap();
    assert!(accepted.profile.storage_identity.is_some());
}

fn single_profile(sequence: u64, id: &str, root: &SigningKey, package: &SigningKey) -> String {
    profile(
        sequence,
        (id, root),
        &[(id, root, Active)],
        &[(package, Active)],
        json!([]),
    )
}

fn rotation(old: (&str, &SigningKey), new: (&str, &SigningKey), sequence: u64) -> Value {
    let mut rotation = KeyRotation {
        from: old.0.into(),
        to: new.0.into(),
        sequence,
        signature: String::new(),
    };
    rotation.signature = BASE64.encode(
        old.1
            .sign(rotation_payload(&rotation, &public(new.1)).as_bytes())
            .to_bytes(),
    );
    json!([rotation])
}

#[test]
fn invalid_context_does_not_silently_select_another_space() {
    let store = GuestLocalStorage::new(":memory:".into());
    let (root, package) = (key(61), key(62));
    let identity = accepted(&store, single_profile(1, "root", &root, &package));
    assert!(
        store
            .resolve_signed_origin(origin(&package, "counter"), Some("forged".into()), None)
            .is_err()
    );
    assert!(
        store
            .resolve_signed_origin("dev:/app".into(), Some(identity), None)
            .is_err()
    );
}
