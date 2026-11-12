use actrium_core::session::{
    AttachErrorCode, AttachRequest, AttachResponse, LeaseMode, ResumeRequest, TakeoverRequest,
};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use crate::{BusinessRequest, MessageKind, SessionError};

const ATTACH_DOMAIN: &[u8] = b"actrium.attach.request.v1\0";
const RESPONSE_DOMAIN: &[u8] = b"actrium.attach.response.v1\0";
const TAKEOVER_DOMAIN: &[u8] = b"actrium.takeover.request.v1\0";
const RESUME_DOMAIN: &[u8] = b"actrium.resume.request.v1\0";
const REQUEST_DOMAIN: &[u8] = b"actrium.business.request.v1\0";

fn bytes(output: &mut Vec<u8>, value: &[u8]) {
    output.extend_from_slice(&(value.len() as u64).to_be_bytes());
    output.extend_from_slice(value);
}

fn string(output: &mut Vec<u8>, value: &str) {
    bytes(output, value.as_bytes());
}

fn u64_value(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn attach_canonical(request: &AttachRequest) -> Vec<u8> {
    let mut output = Vec::new();
    string(&mut output, &request.attach_id);
    string(&mut output, &request.front_actor_id);
    string(&mut output, &request.service_actor_id);
    string(&mut output, &request.channel_id);
    bytes(&mut output, &request.channel_transcript_digest);
    string(&mut output, &request.program.publisher);
    string(&mut output, &request.program.name);
    string(&mut output, &request.program.version);
    bytes(&mut output, &request.package_digest);
    bytes(&mut output, &request.guest_digest);
    string(&mut output, &request.runtime.actrium_version);
    string(&mut output, &request.runtime.front_host_api);
    string(&mut output, &request.runtime.keel_api);
    bytes(&mut output, &request.protocol_fingerprint);
    bytes(&mut output, &request.tab_session_nonce);
    match request.requested_lease {
        LeaseMode::Exclusive => output.push(1),
    }
    u64_value(&mut output, request.requested_capabilities.len() as u64);
    for capability in &request.requested_capabilities {
        string(&mut output, capability);
    }
    output
}

/// Canonical, domain-separated bytes signed by the Front Actor.
pub fn attach_request_signing_bytes(request: &AttachRequest) -> Vec<u8> {
    let mut output = ATTACH_DOMAIN.to_vec();
    bytes(&mut output, &attach_canonical(request));
    bytes(&mut output, &request.channel_transcript_digest);
    output
}

pub(crate) fn attach_request_fingerprint(request: &AttachRequest) -> Vec<u8> {
    let mut canonical = attach_canonical(request);
    bytes(&mut canonical, &request.challenge_response);
    Sha256::digest(canonical).to_vec()
}

pub fn attach_request_digest(request: &AttachRequest) -> Vec<u8> {
    Sha256::digest(attach_request_signing_bytes(request)).to_vec()
}

pub fn sign_attach_request(request: &mut AttachRequest, key: &SigningKey) {
    request.challenge_response = key
        .sign(&attach_request_signing_bytes(request))
        .to_bytes()
        .to_vec();
}

pub fn verify_attach_request(
    request: &AttachRequest,
    peer_public_key: &[u8],
) -> Result<(), SessionError> {
    let public_key: [u8; 32] = peer_public_key.try_into().map_err(|_| {
        SessionError::new(
            AttachErrorCode::ChannelIdentityMismatch,
            "the Front credential does not contain an Ed25519 public key",
        )
    })?;
    let key = VerifyingKey::from_bytes(&public_key).map_err(|_| {
        SessionError::new(
            AttachErrorCode::ChannelIdentityMismatch,
            "invalid Front Ed25519 public key",
        )
    })?;
    let signature = Signature::from_slice(&request.challenge_response)
        .map_err(|_| SessionError::denied("invalid Attach Ed25519 signature encoding"))?;
    key.verify_strict(&attach_request_signing_bytes(request), &signature)
        .map_err(|_| SessionError::denied("Attach challenge response verification failed"))
}

fn response_canonical(response: &AttachResponse) -> Vec<u8> {
    let mut output = Vec::new();
    string(&mut output, &response.session_id);
    u64_value(&mut output, response.lease_epoch);
    match response.lease.mode {
        LeaseMode::Exclusive => output.push(1),
    }
    u64_value(&mut output, response.lease.expires_at_epoch_secs);
    output.extend_from_slice(&response.lease.heartbeat_interval_ms.to_be_bytes());
    string(&mut output, &response.service.actor_id);
    string(&mut output, &response.service.program);
    bytes(&mut output, &response.service.package_digest);
    bytes(&mut output, &response.service.protocol_fingerprint);
    u64_value(
        &mut output,
        response.session_capability_ceiling.len() as u64,
    );
    for capability in &response.session_capability_ceiling {
        string(&mut output, capability);
    }
    bytes(&mut output, &response.privacy.actor_state_digest);
    bytes(&mut output, &response.privacy.application_state_digest);
    bytes(&mut output, &response.privacy.assurance_digest);
    u64_value(&mut output, response.privacy.expires_at_epoch_secs);
    output
}

/// Service proof bytes. `request_digest` binds the response to this exact
/// Attach or Resume exchange rather than merely to a Service identity.
pub fn attach_response_signing_bytes(response: &AttachResponse, request_digest: &[u8]) -> Vec<u8> {
    let mut output = RESPONSE_DOMAIN.to_vec();
    bytes(&mut output, &response_canonical(response));
    bytes(&mut output, request_digest);
    output
}

pub fn sign_attach_response(
    response: &mut AttachResponse,
    request_digest: &[u8],
    key: &SigningKey,
) {
    response.service_challenge = key
        .sign(&attach_response_signing_bytes(response, request_digest))
        .to_bytes()
        .to_vec();
}

pub fn verify_attach_response(
    response: &AttachResponse,
    request_digest: &[u8],
    service_public_key: &[u8],
) -> Result<(), SessionError> {
    let key_bytes: [u8; 32] = service_public_key
        .try_into()
        .map_err(|_| SessionError::denied("the Service proof key is not an Ed25519 public key"))?;
    let key = VerifyingKey::from_bytes(&key_bytes)
        .map_err(|_| SessionError::denied("invalid Service Ed25519 public key"))?;
    let signature = Signature::from_slice(&response.service_challenge)
        .map_err(|_| SessionError::denied("invalid Service proof encoding"))?;
    key.verify_strict(
        &attach_response_signing_bytes(response, request_digest),
        &signature,
    )
    .map_err(|_| SessionError::denied("Service AttachResponse proof verification failed"))
}

pub(crate) fn takeover_signing_bytes_with_attach(
    request: &TakeoverRequest,
    attach: &AttachRequest,
) -> Vec<u8> {
    let mut output = TAKEOVER_DOMAIN.to_vec();
    string(&mut output, &request.session_id);
    u64_value(&mut output, request.lease_epoch);
    string(&mut output, &request.requesting_front_actor_id);
    bytes(&mut output, &attach_request_digest(attach));
    output
}

/// Takeover proof bytes commit to both the old lease and the complete new
/// Attach request, preventing approval from being replayed with another session.
pub fn takeover_signing_bytes(request: &TakeoverRequest, attach: &AttachRequest) -> Vec<u8> {
    let mut output = TAKEOVER_DOMAIN.to_vec();
    string(&mut output, &request.session_id);
    u64_value(&mut output, request.lease_epoch);
    string(&mut output, &request.requesting_front_actor_id);
    bytes(&mut output, &attach_request_digest(attach));
    output
}

pub(crate) fn verify_takeover(
    request: &TakeoverRequest,
    attach: &AttachRequest,
    key: &VerifyingKey,
) -> Result<(), SessionError> {
    let signature = Signature::from_slice(&request.takeover_proof).map_err(|_| {
        SessionError::new(AttachErrorCode::TakeoverDenied, "invalid Takeover proof")
    })?;
    key.verify_strict(
        &takeover_signing_bytes_with_attach(request, attach),
        &signature,
    )
    .map_err(|_| {
        SessionError::new(
            AttachErrorCode::TakeoverDenied,
            "Takeover proof verification failed",
        )
    })
}

pub fn resume_signing_bytes(request: &ResumeRequest) -> Vec<u8> {
    let mut output = RESUME_DOMAIN.to_vec();
    string(&mut output, &request.session_id);
    u64_value(&mut output, request.lease_epoch);
    string(&mut output, &request.channel_id);
    bytes(&mut output, &request.channel_transcript_digest);
    output
}

pub fn resume_digest(request: &ResumeRequest) -> Vec<u8> {
    Sha256::digest(resume_signing_bytes(request)).to_vec()
}

pub fn resume_proof(secret: &[u8; 32], request: &ResumeRequest) -> Vec<u8> {
    hmac(secret, &resume_signing_bytes(request))
}

pub(crate) fn hmac(secret: &[u8; 32], message: &[u8]) -> Vec<u8> {
    let mut mac =
        <Hmac<Sha256> as Mac>::new_from_slice(secret).expect("HMAC accepts a 32-byte key");
    mac.update(message);
    mac.finalize().into_bytes().to_vec()
}

pub(crate) fn verify_hmac(secret: &[u8; 32], message: &[u8], proof: &[u8]) -> bool {
    let mut mac =
        <Hmac<Sha256> as Mac>::new_from_slice(secret).expect("HMAC accepts a 32-byte key");
    mac.update(message);
    mac.verify_slice(proof).is_ok()
}

pub fn request_signing_bytes(request: &BusinessRequest) -> Vec<u8> {
    let mut output = REQUEST_DOMAIN.to_vec();
    string(&mut output, &request.session_id);
    u64_value(&mut output, request.lease_epoch);
    string(&mut output, &request.service_actor_id);
    string(&mut output, &request.channel_id);
    string(&mut output, &request.route);
    string(&mut output, &request.capability);
    string(&mut output, &request.request_id);
    u64_value(&mut output, request.issued_at);
    u64_value(&mut output, request.deadline);
    output.push(match request.kind {
        MessageKind::Request => 1,
        MessageKind::Response => 2,
        MessageKind::Tell => 3,
        MessageKind::StreamChunk => 4,
    });
    match &request.idempotency_key {
        Some(key) => {
            output.push(1);
            string(&mut output, key);
        }
        None => output.push(0),
    }
    bytes(&mut output, &Sha256::digest(&request.payload));
    output
}

pub fn request_proof(secret: &[u8; 32], request: &BusinessRequest) -> Vec<u8> {
    hmac(secret, &request_signing_bytes(request))
}

pub(crate) fn request_fingerprint(request: &BusinessRequest) -> Vec<u8> {
    Sha256::digest(request_signing_bytes(request)).to_vec()
}

pub(crate) fn verifying_key(bytes: &[u8]) -> Result<VerifyingKey, SessionError> {
    let bytes: [u8; 32] = bytes.try_into().map_err(|_| {
        SessionError::new(
            AttachErrorCode::ChannelIdentityMismatch,
            "invalid Ed25519 key length",
        )
    })?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| {
        SessionError::new(
            AttachErrorCode::ChannelIdentityMismatch,
            "invalid Ed25519 public key",
        )
    })
}
