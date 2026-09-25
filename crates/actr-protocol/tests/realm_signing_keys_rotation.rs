//! `contracts/actr/test-vectors/realm-signing-keys-rotation.json` run through
//! the `realm_signing_keys` acceptance procedure
//! (`actr/protocols/authority-discovery.md` section 4.4).

use actr_protocol::realm_profile::jcs;
use actr_protocol::{KeyPinOutcome, KeyPins, PinnedStatus, RealmSigningKeys, rotation_payload};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::Value;

fn vectors() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../contracts/actr/test-vectors/realm-signing-keys-rotation.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn pins_of(vectors: &Value, name: &str) -> KeyPins {
    accepted_in_order(vectors, &[name])
}

/// The pins after accepting `names` in order, the first over HTTPS.
fn accepted_in_order(vectors: &Value, names: &[&str]) -> KeyPins {
    let mut pins = None;
    for name in names {
        let member = &vectors["members"][name]["member"];
        let (_, outcome) = RealmSigningKeys::accept(member, pins.as_ref()).unwrap();
        match outcome {
            KeyPinOutcome::Accepted(accepted) => pins = Some(accepted),
            other => panic!("{name}: {other:?}"),
        }
    }
    pins.expect("at least one member")
}

#[test]
fn keys_derive_from_their_seeds() {
    let vectors = vectors();
    for (id, key) in vectors["keys"].as_object().unwrap() {
        let seed: [u8; 32] = hex::decode(key["seed"].as_str().unwrap())
            .unwrap()
            .try_into()
            .unwrap();
        let public_key = ed25519_dalek::SigningKey::from_bytes(&seed).verifying_key();
        assert_eq!(
            BASE64.encode(public_key.to_bytes()),
            key["public_key"].as_str().unwrap(),
            "{id}"
        );
    }
}

#[test]
fn members_serialize_to_their_jcs_form() {
    let vectors = vectors();
    for (name, member) in vectors["members"].as_object().unwrap() {
        let mut payload = member["member"].clone();
        payload.as_object_mut().unwrap().remove("signature");
        assert_eq!(jcs(&payload), member["jcs"].as_str().unwrap(), "{name}");
    }

    let keys = &vectors["keys"];
    let rotations = [
        ("m2", "rs-a->rs-b"),
        ("retired_signer", "rs-a->rs-x"),
        ("stale_rotation", "rs-b->rs-x"),
    ];
    for (member, label) in rotations {
        let parsed: RealmSigningKeys =
            serde_json::from_value(vectors["members"][member]["member"].clone()).unwrap();
        let rotation = &parsed.rotations.unwrap()[0];
        let public_key = keys[rotation.to.as_str()]["public_key"].as_str().unwrap();
        assert_eq!(
            rotation_payload(rotation, public_key),
            vectors["rotation_payloads"][label].as_str().unwrap()
        );
    }
}

#[test]
fn cases_give_the_expected_result() {
    let vectors = vectors();
    for case in vectors["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        // `pinned`: null, a member, or members accepted in that order.
        let pinned = match &case["pinned"] {
            Value::Null => None,
            Value::String(name) => Some(pins_of(&vectors, name)),
            Value::Array(names) => {
                let names: Vec<&str> = names.iter().map(|name| name.as_str().unwrap()).collect();
                Some(accepted_in_order(&vectors, &names))
            }
            other => panic!("{name}: pinned {other}"),
        };
        let member = &vectors["members"][case["member"].as_str().unwrap()]["member"];
        let result = RealmSigningKeys::accept(member, pinned.as_ref());
        match (case["result"].as_str().unwrap(), result) {
            ("accept", Ok((_, KeyPinOutcome::Accepted(pins)))) => {
                assert_eq!(pins.sequence, case["sequence"].as_u64().unwrap(), "{name}");
                let mut kept: Vec<&str> = pins
                    .keys
                    .iter()
                    .filter(|key| key.status != PinnedStatus::Removed)
                    .map(|key| key.id.as_str())
                    .collect();
                kept.sort();
                let expected: Vec<&str> = case["pinned_after"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|id| id.as_str().unwrap())
                    .collect();
                assert_eq!(kept, expected, "{name}");
            }
            ("reject", Err(_)) => {}
            ("identity_change", Ok((_, KeyPinOutcome::IdentityChange(_)))) => {}
            (expected, result) => panic!("{name}: expected {expected}, got {result:?}"),
        }
    }
}

#[test]
fn removed_keys_stay_removed() {
    let vectors = vectors();
    // m3 -> m4 removes rs-a; m3 listing it again is then rejected, even
    // with a higher sequence it would roll back anyway.
    let (_, outcome) = RealmSigningKeys::accept(
        &vectors["members"]["m4"]["member"],
        Some(&pins_of(&vectors, "m3")),
    )
    .unwrap();
    let KeyPinOutcome::Accepted(pins) = outcome else {
        panic!("m4 not accepted");
    };
    assert_eq!(
        pins.keys
            .iter()
            .find(|key| key.id == "rs-a")
            .unwrap()
            .status,
        PinnedStatus::Removed
    );
    assert_eq!(pins.signing_keys().len(), 1);
    assert!(RealmSigningKeys::accept(&vectors["members"]["m3"]["member"], Some(&pins)).is_err());
    // The same member again is accepted and changes nothing.
    let (_, again) =
        RealmSigningKeys::accept(&vectors["members"]["m4"]["member"], Some(&pins)).unwrap();
    assert_eq!(again, KeyPinOutcome::Accepted(pins));
}
