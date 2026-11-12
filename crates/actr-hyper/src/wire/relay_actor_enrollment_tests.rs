use super::*;
use crate::issuer_client::ActorKeySigner as _;

#[tokio::test]
async fn certified_identity_enrolls_actor_certificate_from_issuer() {
    let root = tempfile::tempdir().unwrap();
    let actor_type = ActrType::from_string_repr("actrium/actrium-front@1.0.0").unwrap();
    let realm = "test@example.com".to_string();
    let signer = crate::issuer_client::actor_signer_for_scoped(
        root.path(),
        &realm,
        &actor_type,
        Some("tab-1"),
    )
    .unwrap();
    let local = ActrId::from_string_repr("2.test@example.com").unwrap();
    let endpoint = local.to_string_repr();
    let now = now_secs();
    let issuer_signing = SigningKey::from_bytes(&[7; 32]);
    let issuer_public = IssuerKey {
        key_id: 1,
        public_key: issuer_signing.verifying_key().to_bytes(),
        expires_at: now + 600,
    };
    let mut server = mockito::Server::new_async().await;
    let response_issuer = issuer_public.clone();
    let response_type = actor_type.clone();
    let response_signer = signer.clone();
    let response_realm = realm.clone();
    let response_issuer_signing = issuer_signing.clone();
    let mock = server
        .mock("POST", "/relay-identity")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body_from_request(move |request| {
            let enrollment: IdentityRequest =
                serde_json::from_slice(&request.body().unwrap()).unwrap();
            let actor_enrollment = enrollment.actor.clone().expect("Actor enrollment");
            let relay_claims = serde_json::to_vec(&IdentityClaims {
                domain: CERTIFICATE_DOMAIN.into(),
                endpoint: enrollment.endpoint.clone(),
                actor_type: response_type.to_string_repr(),
                client_key: "fixture-client".into(),
                public_key: enrollment.public_key,
                admission_commitment: enrollment.admission_commitment,
                expires_at: now + 120,
            })
            .unwrap();
            let signature = response_issuer_signing
                .sign(&signing_bytes(&relay_claims))
                .to_bytes()
                .to_vec();
            let actor_claims = ActorChannelClaims {
                realm: response_realm.clone(),
                actor_id: enrollment.endpoint.clone(),
                actor_type: response_type.to_string_repr(),
                member: "member-1".into(),
                client_key: "fixture-client".into(),
                role: "owner".into(),
                actor_public_key: response_signer.public_key(),
                noise_public_key: enrollment.public_key,
                actor_credential_digest: actor_enrollment.actor_credential_digest,
                policy_digest: actor_enrollment.policy_digest,
                actor_state_digest: actor_enrollment.actor_state_digest,
                issuer: response_realm.clone(),
                provider: "provider.example".into(),
                audience: ACTOR_CHANNEL_AUDIENCE.into(),
                expires_at: now + 120,
                runtime_instance_id: actor_enrollment.runtime_instance_id,
            };
            let actor_certificate =
                ActorChannelCertificate::issue(&actor_claims, 1, &response_issuer_signing).unwrap();
            serde_json::to_vec(&IssuedIdentity {
                certificate: Certificate {
                    key_id: 1,
                    claims: relay_claims,
                    signature,
                },
                issuer: response_issuer.clone(),
                actor_certificate: Some(actor_certificate),
            })
            .unwrap()
        })
        .expect(1)
        .create_async()
        .await;

    let claims = actr_protocol::IdentityClaims {
        realm: realm.clone(),
        actor_id: endpoint.clone(),
        expires_at: now + 600,
        protocol_version: 1,
        issuer: realm.clone(),
        provider: "provider.example".into(),
        audience: "actr.signaling".into(),
        actor_type: actor_type.clone(),
        member: "member-1".into(),
        client_key: "fixture-client".into(),
        role: actr_protocol::MemberRole::Owner as i32,
        development: Some(false),
        actor_public_key: Some(signer.public_key().to_vec().into()),
    };
    let credential = actr_protocol::AIdCredential {
        key_id: 1,
        claims: claims.encode_to_vec().into(),
        signature: bytes::Bytes::from_static(&[0; 64]),
    };
    let mut snapshot = SessionSnapshot::empty_with_id(local.clone(), 1);
    snapshot.credential = credential;
    snapshot.renewal_token = bytes::Bytes::from_static(&[3; 32]);
    let session = SessionState::new(snapshot);
    let gate = Arc::new(WebSocketGate::new_relay(
        Arc::new(RpcCorrelation::new()),
        Arc::new(DataChunkRegistry::new()),
        PeerTypes::default(),
    ));
    let client = RelayClient::new(
        "ws://unused".into(),
        server.url(),
        None,
        session,
        gate,
        CancellationToken::new(),
        Some(root.path().to_path_buf()),
        Some("tab-1".into()),
    );

    let (_, actor) = client
        .certified_identity(&endpoint, &[3; 32])
        .await
        .unwrap();
    let actor = actor.expect("Actor identity from enrollment");
    assert_eq!(actor.claims().actor_id, endpoint);
    assert_eq!(actor.actor_public_key(), &signer.public_key());
    assert_eq!(
        actor.claims().runtime_instance_id,
        client.runtime_instance_id
    );
    let (_, cached) = client
        .certified_identity(&endpoint, &[3; 32])
        .await
        .unwrap();
    assert!(cached.is_some());
    mock.assert_async().await;
}
