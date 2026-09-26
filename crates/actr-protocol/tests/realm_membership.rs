//! `contracts/actr/test-vectors/realm-membership.json`
//! (`actr/protocols/realm-membership.md`).

use actr_protocol::realm_membership::{
    ClientProof, MembershipRequest, ProofOp, RealmInvite, client_key_id, decode_public_key,
    invite_id, invite_secret_hash,
};
use actr_protocol::realm_profile::jcs;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use chrono::{DateTime, Utc};
use serde_json::Value;

fn vectors() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../contracts/actr/test-vectors/realm-membership.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn now(vectors: &Value) -> DateTime<Utc> {
    vectors["now"].as_str().unwrap().parse().unwrap()
}

fn public_key(vectors: &Value, name: &str) -> [u8; 32] {
    decode_public_key(vectors["keys"][name]["public_key"].as_str().unwrap()).unwrap()
}

fn without_signature(document: &Value) -> Value {
    let mut payload = document.clone();
    payload.as_object_mut().unwrap().remove("signature");
    payload
}

#[test]
fn keys_derive_from_their_seeds() {
    let vectors = vectors();
    for (name, key) in vectors["keys"].as_object().unwrap() {
        let seed: [u8; 32] = hex::decode(key["seed"].as_str().unwrap())
            .unwrap()
            .try_into()
            .unwrap();
        let public_key = ed25519_dalek::SigningKey::from_bytes(&seed)
            .verifying_key()
            .to_bytes();
        assert_eq!(BASE64.encode(public_key), key["public_key"], "{name}");
        assert_eq!(client_key_id(&public_key), key["key_id"], "{name}");
    }
}

#[test]
fn invites_hash_and_encode() {
    let vectors = vectors();
    for invite in vectors["invites"].as_array().unwrap() {
        let secret = invite["secret"].as_str().unwrap();
        assert_eq!(invite_secret_hash(secret).unwrap(), invite["secret_hash"]);
        assert_eq!(invite_id(secret).unwrap(), invite["invite_id"]);
        let decoded = RealmInvite::decode(invite["text"].as_str().unwrap()).unwrap();
        assert_eq!(serde_json::to_value(&decoded).unwrap(), invite["invite"]);
        assert_eq!(decoded.encode(), invite["text"]);
    }
}

#[test]
fn requests_verify() {
    let vectors = vectors();
    let provider = vectors["provider_id"].as_str().unwrap();
    for (name, case) in vectors["requests"].as_object().unwrap() {
        let document = &case["request"];
        assert_eq!(jcs(&without_signature(document)), case["jcs"], "{name}");
        let request = MembershipRequest::parse(document.to_string().as_bytes()).unwrap();
        request.check(provider, now(&vectors)).unwrap();
        let signer = public_key(&vectors, case["key"].as_str().unwrap());
        if name == "join" {
            assert_eq!(request.verify_join().unwrap(), signer, "{name}");
        } else {
            request.verify(&signer).unwrap();
        }
    }
}

#[test]
fn client_proofs_verify() {
    let vectors = vectors();
    let provider = vectors["provider_id"].as_str().unwrap();
    for (op, case) in vectors["client_proofs"].as_object().unwrap() {
        assert_eq!(jcs(&without_signature(&case["proof"])), case["jcs"], "{op}");
        let proof = ClientProof::parse_header(case["header"].as_str().unwrap()).unwrap();
        let body = hex::decode(case["body_hex"].as_str().unwrap()).unwrap();
        let op: ProofOp = serde_json::from_value(Value::String(op.clone())).unwrap();
        proof
            .check(op, "k7m2q9xh@acme.com", provider, &body, now(&vectors))
            .unwrap();
        proof
            .verify(&public_key(&vectors, case["key"].as_str().unwrap()))
            .unwrap();
    }
}

#[test]
fn negative_cases_are_rejected() {
    let vectors = vectors();
    let provider = vectors["provider_id"].as_str().unwrap();
    let now = now(&vectors);
    for case in vectors["negative"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let rejected = match case["kind"].as_str().unwrap() {
            "join" => {
                let request =
                    MembershipRequest::parse(case["request"].to_string().as_bytes()).unwrap();
                request.check(provider, now).is_err() || request.verify_join().is_err()
            }
            "request" => {
                let request =
                    MembershipRequest::parse(case["request"].to_string().as_bytes()).unwrap();
                let signer = public_key(&vectors, case["key"].as_str().unwrap());
                request.check(provider, now).is_err() || request.verify(&signer).is_err()
            }
            "proof" => {
                let proof = ClientProof::parse_header(
                    &base64::engine::general_purpose::URL_SAFE_NO_PAD
                        .encode(case["proof"].to_string()),
                )
                .unwrap();
                let body = hex::decode(case["body_hex"].as_str().unwrap()).unwrap();
                let op: ProofOp = serde_json::from_value(case["op"].clone()).unwrap();
                let signer = public_key(&vectors, case["key"].as_str().unwrap());
                proof
                    .check(op, "k7m2q9xh@acme.com", provider, &body, now)
                    .is_err()
                    || proof.verify(&signer).is_err()
            }
            kind => panic!("unknown kind {kind}"),
        };
        assert!(rejected, "{name} must be rejected");
    }
}
