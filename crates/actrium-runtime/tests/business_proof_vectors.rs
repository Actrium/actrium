use actrium_runtime::{BusinessRequest, MessageKind, request_proof, request_signing_bytes};
use serde::Deserialize;

#[derive(Deserialize)]
struct ProofVectors {
    session_secret_hex: String,
    vectors: Vec<ProofVector>,
}

#[derive(Deserialize)]
struct ProofVector {
    name: String,
    session_id: String,
    service_actor_id: String,
    channel_id: String,
    route_key: String,
    capability: String,
    request_id: String,
    issued_at_epoch_secs: u64,
    deadline_epoch_secs: u64,
    idempotency_key: Option<String>,
    lease_epoch: u64,
    kind: u8,
    payload_hex: String,
    canonical_hex: String,
    proof_hex: String,
}

fn decode_hex(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0);
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            let digits = std::str::from_utf8(pair).unwrap();
            u8::from_str_radix(digits, 16).unwrap()
        })
        .collect()
}

#[test]
fn shared_business_proof_vectors_match_the_wire_contract() {
    let fixture: ProofVectors = serde_json::from_str(include_str!(
        "../../../contracts/actrium/session/business_proof_test_vectors.json"
    ))
    .unwrap();
    let secret: [u8; 32] = decode_hex(&fixture.session_secret_hex).try_into().unwrap();

    for vector in fixture.vectors {
        let request = BusinessRequest {
            session_id: vector.session_id,
            lease_epoch: vector.lease_epoch,
            service_actor_id: vector.service_actor_id,
            channel_id: vector.channel_id,
            route: vector.route_key,
            capability: vector.capability,
            request_id: vector.request_id,
            issued_at: vector.issued_at_epoch_secs,
            deadline: vector.deadline_epoch_secs,
            kind: match vector.kind {
                1 => MessageKind::Request,
                2 => MessageKind::Response,
                3 => MessageKind::Tell,
                4 => MessageKind::StreamChunk,
                kind => panic!("unsupported kind {kind}"),
            },
            idempotency_key: vector.idempotency_key,
            payload: decode_hex(&vector.payload_hex),
            proof: Vec::new(),
        };
        assert_eq!(
            request_signing_bytes(&request),
            decode_hex(&vector.canonical_hex),
            "canonical input mismatch for {}",
            vector.name
        );
        assert_eq!(
            request_proof(&secret, &request),
            decode_hex(&vector.proof_hex),
            "HMAC mismatch for {}",
            vector.name
        );
    }
}
