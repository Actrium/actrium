use super::*;
use ed25519_dalek::{Signer, SigningKey};

fn identity(endpoint: &str) -> LocalIdentity {
    identity_with_expiry(endpoint, 1300, 2000)
}

fn identity_with_expiry(
    endpoint: &str,
    certificate_expiry: u64,
    issuer_expiry: u64,
) -> LocalIdentity {
    let key = IdentityKey::generate().unwrap();
    let issuer = SigningKey::from_bytes(&[7; 32]);
    let claims = serde_json::to_vec(&IdentityClaims {
        domain: CERTIFICATE_DOMAIN.into(),
        endpoint: endpoint.into(),
        actor_type: "example/Client@1.0.0".into(),
        client_key: "fixture-key".into(),
        public_key: key.public,
        admission_commitment: key.admission_commitment(),
        expires_at: certificate_expiry,
    })
    .unwrap();
    let signature = issuer.sign(&signing_bytes(&claims)).to_bytes().to_vec();
    let issued = IssuedIdentity {
        certificate: Certificate {
            key_id: 1,
            claims,
            signature,
        },
        issuer: IssuerKey {
            key_id: 1,
            public_key: *issuer.verifying_key().as_bytes(),
            expires_at: issuer_expiry,
        },
        actor_certificate: None,
    };
    key.certify(issued, endpoint, 1000).unwrap()
}

#[test]
fn relay_admission_requires_the_secret_bound_into_the_certificate() {
    let identity = identity("a");
    let claims: IdentityClaims =
        serde_json::from_slice(&identity.issued.certificate.claims).unwrap();
    let mut credential = identity.relay_credential();
    assert!(credential.verify_admission(&claims).is_ok());
    credential.admission_secret[0] ^= 1;
    assert!(credential.verify_admission(&claims).is_err());
    let other = IdentityKey::generate().unwrap();
    assert!(other.certify(identity.issued.clone(), "a", 1000).is_err());
}

#[test]
fn certification_rejects_an_issuer_signed_different_admission_commitment() {
    let identity = identity("a");
    let mut issued = identity.issued.clone();
    let mut claims: IdentityClaims = serde_json::from_slice(&issued.certificate.claims).unwrap();
    claims.admission_commitment = [9; 32];
    issued.certificate.claims = serde_json::to_vec(&claims).unwrap();
    issued.certificate.signature = SigningKey::from_bytes(&[7; 32])
        .sign(&signing_bytes(&issued.certificate.claims))
        .to_bytes()
        .to_vec();
    assert!(
        verify_certificate(
            &issued.certificate,
            &issued.issuer,
            "a",
            &identity.key.public,
            1000
        )
        .is_ok()
    );
    assert!(identity.key.certify(issued, "a", 1000).is_err());
}

#[test]
fn later_peer_certificate_is_accepted_with_a_shorter_valid_key_lease() {
    let a = identity_with_expiry("a", 1300, 1300);
    let b = identity_with_expiry("b", 1310, 1310);
    assert!(
        verify_certificate(
            &b.issued.certificate,
            &a.issued.issuer,
            "b",
            &b.key.public,
            1000
        )
        .is_ok()
    );
    assert!(
        verify_certificate(
            &b.issued.certificate,
            &a.issued.issuer,
            "b",
            &b.key.public,
            1300
        )
        .is_err()
    );

    let mut left = SecureSession::new(&a, "b", 42, 1000).unwrap();
    let mut right = SecureSession::new(&b, "a", 42, 1000).unwrap();
    complete(&mut left, &mut right, &a.issued.issuer);
    assert_eq!(left.expires_at(), 1300);
    assert_eq!(right.expires_at(), 1300);
    assert!(left.send(b"before expiry", 1299).is_ok());
    assert!(left.send(b"after expiry", 1300).is_err());
}

#[test]
fn local_identity_is_bounded_by_its_issuer_key_lease() {
    let identity = identity_with_expiry("a", 1310, 1300);
    assert_eq!(identity.expires_at, 1300);
    assert!(SecureSession::new(&identity, "b", 42, 1300).is_err());
}
fn pair() -> (SecureSession, SecureSession, IssuerKey) {
    let a = identity("a");
    let b = identity("b");
    (
        SecureSession::new(&a, "b", 42, 1000).unwrap(),
        SecureSession::new(&b, "a", 42, 1000).unwrap(),
        a.issued.issuer,
    )
}
fn complete(a: &mut SecureSession, b: &mut SecureSession, key: &IssuerKey) {
    let first = a.start().unwrap().unwrap();
    assert!(b.start().unwrap().is_none());
    let second = b
        .receive(&first, None, 1000)
        .unwrap()
        .outbound
        .pop()
        .unwrap();
    let third = a
        .receive(&second, Some(key), 1000)
        .unwrap()
        .outbound
        .pop()
        .unwrap();
    assert!(
        a.send(b"must not leak before key confirmation", 1000)
            .is_err()
    );
    let fourth = b
        .receive(&third, Some(key), 1000)
        .unwrap()
        .outbound
        .pop()
        .unwrap();
    assert!(!b.is_ready());
    let mut confirmed = a.receive(&fourth, None, 1000).unwrap();
    assert!(confirmed.became_ready);
    let fifth = confirmed.outbound.pop().unwrap();
    assert!(b.receive(&fifth, None, 1000).unwrap().became_ready);
    assert!(a.is_ready() && b.is_ready());
}
#[test]
fn mutually_authenticated_confirmed_ciphertext_and_large_bidirectional_messages() {
    let (mut a, mut b, key) = pair();
    complete(&mut a, &mut b, &key);
    for size in [0, 64, 65536, MAX_APPLICATION_BYTES] {
        let payload = vec![0x5a; size];
        let mut decoded = None;
        for record in a.send(&payload, 1001).unwrap() {
            assert!(record.len() <= MAX_RECORD_BYTES);
            assert!(!record.windows(64).any(|part| part == [0x5a; 64]));
            decoded = b
                .receive(&record, None, 1001)
                .unwrap()
                .application
                .or(decoded);
        }
        assert_eq!(decoded, Some(payload));
    }
    let mut decoded = None;
    for record in b.send(b"reply", 1001).unwrap() {
        decoded = a.receive(&record, None, 1001).unwrap().application;
    }
    assert_eq!(decoded, Some(b"reply".to_vec()));
}
#[test]
fn replay_tampering_and_plaintext_close_the_session() {
    for mutate in [false, true] {
        let (mut a, mut b, key) = pair();
        complete(&mut a, &mut b, &key);
        let mut record = a.send(b"secret", 1001).unwrap().pop().unwrap();
        if mutate {
            *record.last_mut().unwrap() ^= 1;
        } else {
            assert_eq!(
                b.receive(&record, None, 1001).unwrap().application,
                Some(b"secret".to_vec())
            );
        }
        assert!(b.receive(&record, None, 1001).is_err());
        assert!(!b.is_ready());
        assert!(b.send(b"no plaintext fallback", 1001).is_err());
    }
    let (mut a, mut b, key) = pair();
    complete(&mut a, &mut b, &key);
    assert!(
        b.receive(b"legacy plaintext application frame", None, 1001)
            .is_err()
    );
}
#[test]
fn channel_binding_and_wrong_identity_cannot_be_substituted_by_relay() {
    let a = identity("a");
    let b = identity("b");
    for (peer, channel) in [("c", 42), ("a", 43)] {
        let mut left = SecureSession::new(&a, "b", 42, 1000).unwrap();
        let mut right = SecureSession::new(&b, peer, channel, 1000).unwrap();
        let first = left.start().unwrap().unwrap();
        right.start().unwrap();
        if let Ok(second) = right.receive(&first, None, 1000) {
            if let Some(second) = second.outbound.first() {
                assert!(left.receive(second, Some(&b.issued.issuer), 1000).is_err());
            }
        }
        assert!(!left.is_ready() && !right.is_ready());
    }
    // A credential for b, copied by a relay, cannot certify a new private key.
    assert!(
        IdentityKey::generate()
            .unwrap()
            .certify(b.issued.clone(), "b", 1000)
            .is_err()
    );
}
#[test]
fn certificates_require_trusted_issuer_signature_domain_identity_key_and_expiry() {
    let a = identity("a");
    let cert = &a.issued.certificate;
    let key = &a.issued.issuer;
    assert!(verify_certificate(cert, key, "b", &a.key.public, 1000).is_err());
    assert!(verify_certificate(cert, key, "a", &[0; 32], 1000).is_err());
    assert!(verify_certificate(cert, key, "a", &a.key.public, 1300).is_err());
    let mut forged = cert.clone();
    forged.signature[0] ^= 1;
    assert!(verify_certificate(&forged, key, "a", &a.key.public, 1000).is_err());
    let mut wrong = key.clone();
    wrong.public_key = [6; 32];
    assert!(verify_certificate(cert, &wrong, "a", &a.key.public, 1000).is_err());
    // A valid AIS signature in another application domain is insufficient.
    let mut claims: IdentityClaims = serde_json::from_slice(&cert.claims).unwrap();
    claims.domain = "OTHER-PROTOCOL".into();
    let mut other_domain = cert.clone();
    other_domain.claims = serde_json::to_vec(&claims).unwrap();
    other_domain.signature = SigningKey::from_bytes(&[7; 32])
        .sign(&signing_bytes(&other_domain.claims))
        .to_bytes()
        .to_vec();
    assert!(verify_certificate(&other_domain, key, "a", &a.key.public, 1000).is_err());
    let (mut left, mut right, key) = pair();
    complete(&mut left, &mut right, &key);
    assert!(left.send(b"expired", 1300).is_err());
}

#[test]
fn rekey_boundary_preserves_order_and_requires_matching_directional_keys() {
    let (mut a, mut b, key) = pair();
    complete(&mut a, &mut b, &key);
    for index in 0..REKEY_RECORDS * 2 {
        let message = index.to_be_bytes();
        let record = a.send(&message, 1001).unwrap().pop().unwrap();
        assert_eq!(
            b.receive(&record, None, 1001).unwrap().application,
            Some(message.to_vec())
        );
    }
    let record = b
        .send(b"independent reverse direction", 1001)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(
        a.receive(&record, None, 1001).unwrap().application,
        Some(b"independent reverse direction".to_vec())
    );
}
#[test]
fn reconnect_has_fresh_ephemeral_keys_and_old_ciphertext_fails() {
    let a = identity("a");
    let b = identity("b");
    let mut sessions = Vec::new();
    for _ in 0..2 {
        let mut left = SecureSession::new(&a, "b", 42, 1000).unwrap();
        let mut right = SecureSession::new(&b, "a", 42, 1000).unwrap();
        complete(&mut left, &mut right, &a.issued.issuer);
        sessions.push((left, right));
    }
    let old = sessions[0]
        .0
        .send(b"same payload", 1001)
        .unwrap()
        .pop()
        .unwrap();
    let new = sessions[1]
        .0
        .send(b"same payload", 1001)
        .unwrap()
        .pop()
        .unwrap();
    assert_ne!(old, new);
    assert!(sessions[1].1.receive(&old, None, 1001).is_err());
}
