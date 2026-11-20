use super::*;
use crate::IdentityKey;

const NOW: u64 = 1_000;
const EXPIRY: u64 = 1_500;
const ISSUER_EXPIRY: u64 = 2_000;

fn issuer() -> (SigningKey, IssuerKey) {
    let signing = SigningKey::from_bytes(&[7; 32]);
    let public = IssuerKey {
        key_id: 11,
        public_key: signing.verifying_key().to_bytes(),
        expires_at: ISSUER_EXPIRY,
    };
    (signing, public)
}

fn claims(
    realm: &str,
    actor_id: &str,
    actor_public_key: [u8; 32],
    noise_public_key: [u8; 32],
    expires_at: u64,
) -> ActorChannelClaims {
    ActorChannelClaims {
        realm: realm.into(),
        actor_id: actor_id.into(),
        actor_type: if actor_id == "front" {
            "example/Front@1.0.0"
        } else {
            "example/Service@1.0.0"
        }
        .into(),
        member: "member-1".into(),
        client_key: "client-1".into(),
        role: if actor_id == "front" {
            "editor"
        } else {
            "service"
        }
        .into(),
        actor_public_key,
        noise_public_key,
        actor_credential_digest: Sha256::digest(actor_id.as_bytes()).into(),
        policy_digest: Sha256::digest(b"test-policy").into(),
        actor_state_digest: Sha256::digest(actor_id.as_bytes()).into(),
        issuer: "realm-issuer".into(),
        provider: "provider.example".into(),
        audience: ACTOR_CHANNEL_AUDIENCE.into(),
        expires_at,
        runtime_instance_id: format!("{actor_id}-runtime-1"),
    }
}

fn identity_with_expiry(
    issuer: &SigningKey,
    issuer_public: &IssuerKey,
    realm: &str,
    actor_id: &str,
    expires_at: u64,
) -> ActorChannelIdentity {
    let noise = IdentityKey::generate().unwrap();
    let actor = ActorIdentityKey::generate().unwrap();
    let claims = claims(realm, actor_id, actor.public, noise.public, expires_at);
    let certificate =
        ActorChannelCertificate::issue(&claims, issuer_public.key_id, issuer).unwrap();
    ActorChannelIdentity::certify(
        noise,
        actor,
        certificate,
        issuer_public.clone(),
        realm,
        actor_id,
        NOW,
    )
    .unwrap()
}

fn identities(
    front_realm: &str,
    service_realm: &str,
) -> (ActorChannelIdentity, ActorChannelIdentity, IssuerKey) {
    let (issuer, issuer_public) = issuer();
    let front = identity_with_expiry(&issuer, &issuer_public, front_realm, "front", EXPIRY);
    let service = identity_with_expiry(&issuer, &issuer_public, service_realm, "service", EXPIRY);
    (front, service, issuer_public)
}

fn pair_with_realms(
    front_realm: &str,
    service_realm: &str,
) -> (ActorChannelSession, ActorChannelSession, IssuerKey) {
    let (front, service, issuer) = identities(front_realm, service_realm);
    (
        ActorChannelSession::new_front(&front, "service", "channel-1", NOW).unwrap(),
        ActorChannelSession::new_service(&service, "front", "channel-1", NOW).unwrap(),
        issuer,
    )
}

fn pair() -> (ActorChannelSession, ActorChannelSession, IssuerKey) {
    pair_with_realms("realm-1", "realm-1")
}

fn one(mut records: Vec<Vec<u8>>) -> Vec<u8> {
    assert_eq!(records.len(), 1);
    records.pop().unwrap()
}

/// Drive Noise through message three. The returned records are the Service's
/// Actor proof followed by its Noise key confirmation.
fn service_proof_records(
    front: &mut ActorChannelSession,
    service: &mut ActorChannelSession,
    issuer: &IssuerKey,
) -> Vec<Vec<u8>> {
    let first = front.start().unwrap().unwrap();
    assert!(service.start().unwrap().is_none());
    let second = one(service.receive(&first, None, NOW).unwrap().outbound);
    let third = one(front.receive(&second, Some(issuer), NOW).unwrap().outbound);
    let received = service.receive(&third, Some(issuer), NOW).unwrap();
    assert_eq!(received.outbound.len(), 2);
    received.outbound
}

fn complete(
    front: &mut ActorChannelSession,
    service: &mut ActorChannelSession,
    issuer: &IssuerKey,
) {
    let service_records = service_proof_records(front, service, issuer);
    assert!(front.verified().is_none());
    assert!(front.send(b"before Actor proof", NOW).is_err());
    assert!(
        !front
            .receive(&service_records[0], None, NOW)
            .unwrap()
            .became_ready
    );
    assert!(front.verified().is_none());
    assert!(front.send(b"before key confirmation", NOW).is_err());
    let front_received = front.receive(&service_records[1], None, NOW).unwrap();
    assert!(front_received.became_ready);
    assert_eq!(front_received.outbound.len(), 2);
    assert!(front.is_ready());
    assert!(!service.is_ready());
    assert!(service.verified().is_none());
    assert!(service.send(b"Service still unconfirmed", NOW).is_err());
    assert!(
        !service
            .receive(&front_received.outbound[0], None, NOW)
            .unwrap()
            .became_ready
    );
    assert!(service.verified().is_none());
    assert!(
        service
            .receive(&front_received.outbound[1], None, NOW)
            .unwrap()
            .became_ready
    );
    assert!(front.is_ready() && service.is_ready());
}

#[test]
fn independent_front_and_service_keys_authenticate_and_export_verified_context() {
    let (front_identity, service_identity, issuer) = identities("realm-1", "realm-1");
    assert_ne!(
        front_identity.actor_public_key(),
        service_identity.actor_public_key()
    );
    assert_ne!(
        front_identity.noise_public_key(),
        service_identity.noise_public_key()
    );
    assert_ne!(
        front_identity.actor_public_key().as_slice(),
        front_identity.noise_public_key().as_slice()
    );
    let front_claims = front_identity.claims().clone();
    let service_claims = service_identity.claims().clone();
    let mut front =
        ActorChannelSession::new_front(&front_identity, "service", "channel-1", NOW).unwrap();
    let mut service =
        ActorChannelSession::new_service(&service_identity, "front", "channel-1", NOW).unwrap();
    complete(&mut front, &mut service, &issuer);

    let front_verified = front.verified().unwrap();
    let service_verified = service.verified().unwrap();
    assert_eq!(front_verified.channel_id, "channel-1");
    assert_eq!(
        front_verified.transcript_digest,
        service_verified.transcript_digest
    );
    assert_eq!(front_verified.peer_claims, service_claims);
    assert_eq!(service_verified.peer_claims, front_claims);
    assert_eq!(
        front_verified.peer_actor_public_key,
        *service_identity.actor_public_key()
    );
    assert_eq!(
        service_verified.peer_actor_public_key,
        *front_identity.actor_public_key()
    );
    assert_eq!(front_verified.local_runtime_instance_id, "front-runtime-1");
    assert_eq!(front_verified.peer_runtime_instance_id, "service-runtime-1");
    assert_eq!(
        service_verified.local_runtime_instance_id,
        "service-runtime-1"
    );
    assert_eq!(service_verified.peer_runtime_instance_id, "front-runtime-1");
    assert_eq!(front_verified.expires_at, EXPIRY);
    assert!(front_verified.is_valid_at(EXPIRY - 1));
    assert!(!front_verified.is_valid_at(EXPIRY));

    let record = one(front.send(b"authenticated request", NOW + 1).unwrap());
    assert_eq!(
        service.receive(&record, None, NOW + 1).unwrap().application,
        Some(b"authenticated request".to_vec())
    );
    let reply = one(service.send(b"authenticated reply", NOW + 1).unwrap());
    assert_eq!(
        front.receive(&reply, None, NOW + 1).unwrap().application,
        Some(b"authenticated reply".to_vec())
    );
}

#[test]
fn certificate_rejects_wrong_actor_or_noise_private_key_and_bad_issuer_signature() {
    let (issuer, issuer_public) = issuer();
    let noise = IdentityKey::generate().unwrap();
    let actor = ActorIdentityKey::generate().unwrap();
    let wrong_actor = ActorIdentityKey::generate().unwrap();
    let wrong_claims = claims("realm-1", "front", wrong_actor.public, noise.public, EXPIRY);
    let certificate =
        ActorChannelCertificate::issue(&wrong_claims, issuer_public.key_id, &issuer).unwrap();
    assert!(
        ActorChannelIdentity::certify(
            noise,
            actor,
            certificate,
            issuer_public.clone(),
            "realm-1",
            "front",
            NOW,
        )
        .is_err()
    );

    let identity = identity_with_expiry(&issuer, &issuer_public, "realm-1", "front", EXPIRY);
    let mut forged = identity.certificate().clone();
    forged.signature[0] ^= 1;
    assert!(
        verify_actor_channel_certificate(
            &forged,
            &issuer_public,
            "realm-1",
            "front",
            identity.noise_public_key(),
            NOW,
        )
        .is_err()
    );

    let mut wrong_noise = identity.certificate().clone();
    let mut decoded: ActorChannelClaims = serde_json::from_slice(&wrong_noise.claims).unwrap();
    decoded.noise_public_key = IdentityKey::generate().unwrap().public;
    wrong_noise = ActorChannelCertificate::issue(&decoded, issuer_public.key_id, &issuer).unwrap();
    assert!(
        verify_actor_channel_certificate(
            &wrong_noise,
            &issuer_public,
            "realm-1",
            "front",
            identity.noise_public_key(),
            NOW,
        )
        .is_err()
    );
}

#[test]
fn wrong_actor_private_key_produces_a_proof_that_fails_closed() {
    let (mut front, mut service, issuer) = pair();
    *front.actor_state_mut().actor_private = [99; 32];
    let service_records = service_proof_records(&mut front, &mut service, &issuer);
    front.receive(&service_records[0], None, NOW).unwrap();
    let front_records = front
        .receive(&service_records[1], None, NOW)
        .unwrap()
        .outbound;
    assert!(service.receive(&front_records[0], None, NOW).is_err());
    assert!(!service.is_ready());
    assert!(service.verified().is_none());
    assert!(service.send(b"must not escape", NOW).is_err());
}

#[test]
fn realm_mismatch_fails_the_noise_prologue_and_certificate_checks() {
    let (mut front, mut service, _) = pair_with_realms("realm-1", "realm-2");
    let first = front.start().unwrap().unwrap();
    service.start().unwrap();
    let second = one(service.receive(&first, None, NOW).unwrap().outbound);
    assert!(front.receive(&second, None, NOW).is_err());
    assert!(!front.is_ready() && !service.is_ready());

    let (issuer, issuer_public) = issuer();
    let identity = identity_with_expiry(&issuer, &issuer_public, "realm-1", "front", EXPIRY);
    assert!(
        verify_actor_channel_certificate(
            identity.certificate(),
            &issuer_public,
            "realm-2",
            "front",
            identity.noise_public_key(),
            NOW,
        )
        .is_err()
    );
}

#[test]
fn wrong_signed_audience_is_rejected_even_with_a_valid_issuer_signature() {
    let (issuer, issuer_public) = issuer();
    let noise = IdentityKey::generate().unwrap();
    let actor = ActorIdentityKey::generate().unwrap();
    let mut wrong = claims("realm-1", "front", actor.public, noise.public, EXPIRY);
    wrong.audience = "actr.unrelated".into();
    let certificate =
        ActorChannelCertificate::issue(&wrong, issuer_public.key_id, &issuer).unwrap();
    assert!(
        ActorChannelIdentity::certify(
            noise,
            actor,
            certificate,
            issuer_public,
            "realm-1",
            "front",
            NOW,
        )
        .is_err()
    );
}

#[test]
fn wrong_target_service_actor_id_is_bound_into_the_prologue() {
    let (front_identity, service_identity, _) = identities("realm-1", "realm-1");
    let mut front =
        ActorChannelSession::new_front(&front_identity, "other-service", "channel-1", NOW).unwrap();
    let mut service =
        ActorChannelSession::new_service(&service_identity, "front", "channel-1", NOW).unwrap();
    let first = front.start().unwrap().unwrap();
    service.start().unwrap();
    let second = one(service.receive(&first, None, NOW).unwrap().outbound);
    assert!(front.receive(&second, None, NOW).is_err());
    assert!(!front.is_ready() && !service.is_ready());
}

#[test]
fn expired_certificates_and_sessions_fail_closed() {
    let (issuer, issuer_public) = issuer();
    let noise = IdentityKey::generate().unwrap();
    let actor = ActorIdentityKey::generate().unwrap();
    let expired = claims("realm-1", "front", actor.public, noise.public, NOW);
    let certificate =
        ActorChannelCertificate::issue(&expired, issuer_public.key_id, &issuer).unwrap();
    assert!(
        ActorChannelIdentity::certify(
            noise,
            actor,
            certificate,
            issuer_public,
            "realm-1",
            "front",
            NOW,
        )
        .is_err()
    );

    let (mut front, mut service, issuer_public) = pair();
    complete(&mut front, &mut service, &issuer_public);
    assert!(front.send(b"expired", EXPIRY).is_err());
    assert!(service.send(b"expired", EXPIRY).is_err());
}

#[test]
fn actor_proof_from_an_old_transcript_cannot_be_replayed() {
    let (front_identity, service_identity, issuer) = identities("realm-1", "realm-1");
    let mut old_front =
        ActorChannelSession::new_front(&front_identity, "service", "channel-1", NOW).unwrap();
    let mut old_service =
        ActorChannelSession::new_service(&service_identity, "front", "channel-1", NOW).unwrap();
    let old_records = service_proof_records(&mut old_front, &mut old_service, &issuer);

    let mut new_front =
        ActorChannelSession::new_front(&front_identity, "service", "channel-1", NOW).unwrap();
    let mut new_service =
        ActorChannelSession::new_service(&service_identity, "front", "channel-1", NOW).unwrap();
    service_proof_records(&mut new_front, &mut new_service, &issuer);
    assert!(new_front.receive(&old_records[0], None, NOW).is_err());
    assert!(!new_front.is_ready());
    assert!(new_front.verified().is_none());
    assert!(new_front.send(b"replayed authentication", NOW).is_err());
}

#[test]
fn noise_transcript_tampering_closes_the_actor_channel() {
    let (front_identity, service_identity, issuer) = identities("realm-1", "realm-1");
    let mut front =
        ActorChannelSession::new_front(&front_identity, "service", "channel-1", NOW).unwrap();
    let mut service =
        ActorChannelSession::new_service(&service_identity, "front", "channel-1", NOW).unwrap();
    let first = front.start().unwrap().unwrap();
    service.start().unwrap();
    let mut second = one(service.receive(&first, None, NOW).unwrap().outbound);
    *second.last_mut().unwrap() ^= 1;
    assert!(front.receive(&second, Some(&issuer), NOW).is_err());
    assert!(!front.is_ready());
    assert!(front.verified().is_none());
    assert!(front.send(b"tampered transcript", NOW).is_err());
}

#[test]
fn authenticated_application_replay_has_no_plaintext_fallback() {
    let (mut front, mut service, issuer) = pair();
    complete(&mut front, &mut service, &issuer);
    let record = one(front.send(b"once", NOW).unwrap());
    assert_eq!(
        service.receive(&record, None, NOW).unwrap().application,
        Some(b"once".to_vec())
    );
    assert!(service.receive(&record, None, NOW).is_err());
    assert!(!service.is_ready());
    assert!(service.verified().is_none());
    assert!(service.send(b"no fallback", NOW).is_err());
}
