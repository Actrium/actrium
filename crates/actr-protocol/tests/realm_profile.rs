//! Shared Realm Profile vectors exercise both the JSON Schema and the real
//! signature/successor verifier. Schema acceptance alone never grants trust.

use actr_protocol::realm_profile::{RealmPackageSelection, RealmProfile, jcs};
use actr_protocol::{KeyPinOutcome, RealmSigningKeys};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::{Value, json};

fn vectors() -> Value {
    serde_json::from_str(include_str!(
        "../../../contracts/actr/test-vectors/realm-profile.json"
    ))
    .unwrap()
}

fn schema() -> jsonschema::Validator {
    let schema: Value = serde_json::from_str(include_str!(
        "../../../contracts/actr/realm-profile.schema.json"
    ))
    .unwrap();
    assert!(jsonschema::meta::is_valid(&schema));
    jsonschema::draft202012::options()
        .should_validate_formats(true)
        .build(&schema)
        .unwrap()
}

fn keys(vectors: &Value, name: &str) -> RealmSigningKeys {
    // Each key set is a separately acquired, self-signed trust fixture. Key
    // rotation/pinning transitions have their own existing vector suite.
    let member = &vectors["key_sets"][name]["document"];
    let (keys, outcome) = RealmSigningKeys::accept(member, None).unwrap();
    assert!(matches!(outcome, KeyPinOutcome::Accepted(_)), "{name}");
    keys
}

fn profile(vectors: &Value, name: &str) -> RealmProfile {
    RealmProfile::verify(
        &serde_json::to_vec(&vectors["profiles"][name]["document"]).unwrap(),
        vectors["realm"].as_str().unwrap(),
        &keys(vectors, "initial"),
    )
    .unwrap()
}

#[test]
fn documents_match_their_published_jcs_bytes() {
    let vectors = vectors();
    for group in ["key_sets", "profiles"] {
        for (name, vector) in vectors[group].as_object().unwrap() {
            let mut payload = vector["document"].clone();
            payload.as_object_mut().unwrap().remove("signature");
            assert_eq!(jcs(&payload), vector["jcs"].as_str().unwrap(), "{name}");
        }
    }
}

#[test]
fn schema_and_protocol_give_the_expected_results() {
    let vectors = vectors();
    let schema = schema();
    for case in vectors["verification_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let document = &vectors["profiles"][case["profile"].as_str().unwrap()]["document"];
        assert_eq!(
            schema.is_valid(document),
            case["schema_valid"].as_bool().unwrap(),
            "{name}: {:?}",
            schema.iter_errors(document).collect::<Vec<_>>()
        );
        let result = RealmProfile::verify(
            &serde_json::to_vec(document).unwrap(),
            case["realm"]
                .as_str()
                .unwrap_or_else(|| vectors["realm"].as_str().unwrap()),
            &keys(&vectors, case["keys"].as_str().unwrap()),
        );
        let actual = result
            .as_ref()
            .map_or_else(|error| error.code(), |_| "accept");
        assert_eq!(
            actual,
            case["result"].as_str().unwrap(),
            "{name}: {result:?}"
        );
    }
}

#[test]
fn schema_rejects_malformed_shapes_but_allows_extensions() {
    let vectors = vectors();
    let schema = schema();
    for case in vectors["schema_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut document =
            vectors["profiles"][case["profile"].as_str().unwrap()]["document"].clone();
        for (pointer, value) in case["set"].as_object().unwrap() {
            // The vectors only add object members or replace existing ones.
            let (parent, member) = pointer.rsplit_once('/').unwrap();
            let parent = document.pointer_mut(parent).expect("existing parent");
            match parent {
                Value::Object(members) => {
                    members.insert(member.to_string(), value.clone());
                }
                Value::Array(items) => items[member.parse::<usize>().unwrap()] = value.clone(),
                _ => panic!("{name}: parent of {pointer} is not a container"),
            }
        }
        for pointer in case["remove"].as_array().unwrap() {
            let (parent, member) = pointer.as_str().unwrap().rsplit_once('/').unwrap();
            assert!(
                document
                    .pointer_mut(parent)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove(member)
                    .is_some(),
                "{name}: missing member to remove"
            );
        }
        assert_eq!(
            schema.is_valid(&document),
            case["valid"].as_bool().unwrap(),
            "{name}: {:?}",
            schema.iter_errors(&document).collect::<Vec<_>>()
        );
    }
}

#[test]
fn endpoint_vectors_match_schema_and_signed_verification() {
    let vectors = vectors();
    let schema = schema();
    // Sign each mutation with a fresh, separately accepted key so rejection
    // cannot be attributed to tampering with a published fixture's signature.
    let signer = SigningKey::from_bytes(&rand::random());
    let resign = |document: &mut Value, key_id: &str| {
        document.as_object_mut().unwrap().remove("signature");
        let signature = signer.sign(jcs(document).as_bytes());
        document["signature"] = json!({
            "key_id": key_id,
            "algorithm": "Ed25519",
            "value": BASE64.encode(signature.to_bytes()),
        });
    };
    let mut key_document = vectors["key_sets"]["initial"]["document"].clone();
    let key_id = key_document["keys"][0]["id"].as_str().unwrap().to_owned();
    key_document["keys"][0]["public_key"] = json!(BASE64.encode(signer.verifying_key().to_bytes()));
    resign(&mut key_document, &key_id);
    let (keys, _) = RealmSigningKeys::accept(&key_document, None).unwrap();

    for case in vectors["endpoint_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let valid = case["valid"].as_bool().unwrap();
        let mut document = vectors["profiles"]["active"]["document"].clone();
        for (pointer, value) in case["set"].as_object().unwrap() {
            *document.pointer_mut(pointer).expect("existing endpoint") = value.clone();
        }
        assert_eq!(
            schema.is_valid(&document),
            valid,
            "{name}: {:?}",
            schema.iter_errors(&document).collect::<Vec<_>>()
        );
        resign(&mut document, &key_id);
        let result = RealmProfile::verify(
            &serde_json::to_vec(&document).unwrap(),
            vectors["realm"].as_str().unwrap(),
            &keys,
        );
        assert_eq!(result.is_ok(), valid, "{name}: {result:?}");
        if let Err(error) = result {
            assert_eq!(error.code(), "REALM_PROFILE_INVALID", "{name}: {error}");
            assert!(
                error
                    .to_string()
                    .contains(case["error_field"].as_str().unwrap()),
                "{name}: {error}"
            );
        }
    }
}

#[test]
fn verified_successors_preserve_sequence_provider_and_retirement() {
    let vectors = vectors();
    for case in vectors["successor_cases"].as_array().unwrap() {
        let accepted = profile(&vectors, case["accepted"].as_str().unwrap());
        let candidate = profile(&vectors, case["candidate"].as_str().unwrap());
        let result = accepted.check_successor(&candidate);
        let actual = result
            .as_ref()
            .map_or_else(|error| error.code(), |_| "accept");
        assert_eq!(
            actual,
            case["result"].as_str().unwrap(),
            "{}: {result:?}",
            case["name"]
        );
    }
}

#[test]
fn json_whitespace_and_member_order_do_not_change_signed_content() {
    let vectors = vectors();
    let document = &vectors["profiles"]["active"]["document"];
    let compact = profile(&vectors, "active");
    // Sorting by reverse key order changes the wire form, not the JCS payload.
    let members = document.as_object().unwrap();
    let reordered = format!(
        "{{\n{}\n}}",
        members
            .iter()
            .rev()
            .map(|(key, value)| format!("  {}: {value}", serde_json::to_string(key).unwrap()))
            .collect::<Vec<_>>()
            .join(",\n")
    );
    let reordered = RealmProfile::verify(
        reordered.as_bytes(),
        vectors["realm"].as_str().unwrap(),
        &keys(&vectors, "initial"),
    )
    .unwrap();
    compact.check_successor(&reordered).unwrap();
}

#[test]
fn credential_key_ids_resolve_active_and_retired_issuer_keys() {
    let vectors = vectors();
    let lookup = &vectors["issuer_key_lookups"];
    let profile = profile(&vectors, lookup["profile"].as_str().unwrap());
    for (key_id, expected) in lookup["cases"].as_object().unwrap() {
        let status = profile
            .issuer_key(key_id.parse().unwrap())
            .map(|key| serde_json::to_value(key.status).unwrap())
            .unwrap_or(Value::Null);
        assert_eq!(&status, expected, "{key_id}");
    }
}

#[test]
fn published_echo_example_selects_service_and_front() {
    let bytes = include_bytes!("../../../actr/protocols/examples/realm-profile.json");
    let key = actr_protocol::RealmSigningKey {
        id: "rs-demo-minimal-1".into(),
        algorithm: "Ed25519".into(),
        status: actr_protocol::KeyStatus::Active,
        public_key: "MwhxEhQQQtS1V6jX0bfHvy+wPND44QXNlEFg9frdHlc=".into(),
    };
    // As in a host with a preconfigured Realm trust root.
    let mut trusted = keys(&vectors(), "initial");
    trusted.signature.key_id = key.id.clone();
    trusted.keys = vec![key];
    let checked = RealmProfile::verify(bytes, "echo@realm.example", &trusted).unwrap();
    let (service, front) = checked.selection.as_ref().unwrap().selected().unwrap();
    assert_eq!(service.r#type, "publisher.example/echo@0.1.1");
    assert_eq!(front.r#type, "publisher.example/echo-ui@0.1.1");
    assert!(service.dependencies.is_empty());
    assert!(front.dependencies.is_empty());
    assert!(schema().is_valid(&serde_json::from_slice(bytes).unwrap()));
    let text = std::str::from_utf8(bytes).unwrap();
    for invalid in [
        text.replace(
            "\"primary_package\":",
            "\"primary_package\":\"ui\",\"primary_package\":",
        ),
        text.replace(
            "\"href\":",
            "\"href\":\"https://other.example/x\",\"href\":",
        ),
        format!("{}{}", " ".repeat(65536), text),
    ] {
        assert!(RealmProfile::verify(invalid.as_bytes(), "echo@realm.example", &trusted).is_err());
    }
}

#[test]
fn dependency_selections_match_schema_and_reject_missing_targets() {
    let vectors = vectors();
    let schema = schema();
    for (name, bindings, schema_valid, valid) in [
        ("empty", json!({}), true, true),
        ("selected service", json!({"service": "notes"}), true, true),
        (
            "case-sensitive alias",
            json!({"EchoService": "notes"}),
            true,
            true,
        ),
        ("self reference", json!({"self": "ui"}), true, true),
        ("wrong Front service", json!({"service": "ui"}), true, false),
        ("missing target", json!({"service": "missing"}), true, false),
        (
            "invalid reference",
            json!({"service": "Notes"}),
            false,
            false,
        ),
        ("empty reference", json!({"service": ""}), false, false),
        ("empty alias", json!({"": "notes"}), false, false),
        (
            "untrimmed alias",
            json!({" service": "notes"}),
            false,
            false,
        ),
        (
            "trailing newline",
            json!({"service\n": "notes"}),
            false,
            false,
        ),
        ("null map", Value::Null, false, false),
        ("array map", json!(["notes"]), false, false),
        (
            "non-string target",
            json!({"service": ["notes"]}),
            false,
            false,
        ),
    ] {
        let mut document = vectors["profiles"]["active"]["document"].clone();
        document["packages"]["ui"]["dependencies"] = bindings;
        assert_eq!(schema.is_valid(&document), schema_valid, "{name}");
        let result = serde_json::from_value::<RealmPackageSelection>(document)
            .map_err(|error| error.to_string())
            .and_then(|selection| selection.check());
        assert_eq!(result.is_ok(), valid, "{name}: {result:?}");
    }
}

#[test]
fn dependency_selection_example_round_trips_and_allows_cycles() {
    let mut document = vectors()["profiles"]["active"]["document"].clone();
    document.as_object_mut().unwrap().retain(|key, _| {
        matches!(
            key.as_str(),
            "primary_package" | "front_package" | "packages"
        )
    });
    let mut index = document["packages"]["notes"].clone();
    index["type"] = json!("publisher.example/index@1.0.0");
    document["packages"]["index"] = index;
    document["packages"]["ui"]["dependencies"] = json!({"service": "notes"});
    document["packages"]["notes"]["dependencies"] = json!({"index": "index"});
    let mut selection: RealmPackageSelection = serde_json::from_value(document.clone()).unwrap();
    selection.check().unwrap();
    assert_eq!(selection.packages["ui"].dependencies["service"], "notes");
    assert_eq!(selection.packages["notes"].dependencies["index"], "index");
    assert_eq!(serde_json::to_value(&selection).unwrap(), document);

    selection.packages.get_mut("index").unwrap().dependencies =
        [("notes".into(), "notes".into())].into();
    selection.check().unwrap();
}
