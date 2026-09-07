use super::*;
use actr_protocol::{
    AIdCredential, ActrId, ActrType, IdentityClaims, Realm, RegisterRequest, RegisterResponse,
    ReissueCredentialRequest, RenewCredentialResponse, TurnCredential, register_response,
    renew_credential_response,
};
use prost::bytes::Bytes;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering as AtomicOrdering};

fn actor(serial: u64) -> ActrId {
    ActrId {
        realm: Realm { realm_id: 7 },
        serial_number: serial,
        r#type: ActrType {
            manufacturer: "acme".to_string(),
            name: "node".to_string(),
            version: "1.0.0".to_string(),
        },
    }
}

fn credential(key_id: u32) -> AIdCredential {
    AIdCredential {
        key_id,
        claims: Bytes::new(),
        signature: Bytes::from(vec![0; 64]),
    }
}

fn pending_control_attempts() -> Arc<Mutex<PendingControlAttempts>> {
    Arc::new(Mutex::new(PendingControlAttempts::default()))
}

fn credential_for_actor(actor_id: &ActrId, key_id: u32, expires_at: u64) -> AIdCredential {
    let claims = IdentityClaims {
        realm_id: actor_id.realm.realm_id,
        actor_id: actor_id.to_string_repr(),
        expires_at,
        protocol_version: 1,
        issuer: format!("actr:realm:{}:authority", actor_id.realm.realm_id),
        provider: "actrix-official".to_string(),
        audience: "actr.signaling".to_string(),
    };
    AIdCredential {
        key_id,
        claims: claims.encode_to_vec().into(),
        signature: Bytes::from(vec![0; 64]),
    }
}

fn register_ok(serial: u64, key_id: u32) -> register_response::RegisterOk {
    register_response::RegisterOk {
        actr_id: actor(serial),
        credential: credential(key_id),
        turn_credential: TurnCredential {
            protocol_version: 1,
            realm_id: 1,
            provider: "actrix-official".to_string(),
            audience: "actr.turn".to_string(),
            username: format!("1000:actor-{serial}"),
            password: "turn-password".to_string(),
            expires_at: 1000,
        },
        credential_expires_at: Some(prost_types::Timestamp {
            seconds: 1000,
            nanos: 0,
        }),
        signaling_heartbeat_interval_secs: 30,
        signing_pubkey: Bytes::from(vec![1; 32]),
        signing_key_id: key_id,
        renewal_token: Some(Bytes::from_static(b"new-renewal-token-32-bytes!!")),
        renewal_token_expires_at: Some(prost_types::Timestamp {
            seconds: 2000,
            nanos: 0,
        }),
    }
}

/// A `RegisterOk` whose `renewal_token` / `renewal_token_expires_at` are
/// `None` — what an AIS built before the renewal-token feature returns. Used
/// to prove a version-skewed hard rebind still commits the access credential
/// instead of discarding it (issue #397).
fn register_ok_without_token(serial: u64, key_id: u32) -> register_response::RegisterOk {
    let mut ok = register_ok(serial, key_id);
    ok.renewal_token = None;
    ok.renewal_token_expires_at = None;
    ok
}

/// P1 (finding 1): with the membership controller flag OFF (default), an expired
/// renewal token drives the legacy hard rebind, which must recover via a FRESH
/// `/register` (new serial) and must NOT call `/reissue`. A `/reissue`-only
/// legacy path breaks recovery against older AIS servers (404) and after the
/// reissue binding grace lapses (403 possession failure).
#[tokio::test]
async fn expired_renewal_token_re_registers_fresh_identity() {
    let mut server = mockito::Server::new_async().await;
    // `/register` mints a NEW identity (serial 2), distinct from the old serial 1.
    let response = RegisterResponse {
        result: Some(register_response::Result::Success(register_ok(2, 9))),
    };
    let register_mock = server
        .mock("POST", "/register")
        .with_status(200)
        .with_header("content-type", "application/x-protobuf")
        .with_body(response.encode_to_vec())
        .expect(1)
        .create_async()
        .await;
    // The legacy flag-OFF path must never touch `/reissue`.
    let reissue_mock = server
        .mock("POST", "/reissue")
        .expect(0)
        .create_async()
        .await;

    let session = SessionState::new(super::super::session_state::SessionSnapshot {
        actor_id: actor(1),
        credential: credential(1),
        credential_expires_at: prost_types::Timestamp {
            seconds: 10,
            nanos: 0,
        },
        turn_credential: TurnCredential {
            protocol_version: 1,
            realm_id: 1,
            provider: "actrix-official".to_string(),
            audience: "actr.turn".to_string(),
            username: "10:actor-1".to_string(),
            password: "old".to_string(),
            expires_at: 10,
        },
        renewal_token: Bytes::from_static(b"expired-renewal-token-32bytes"),
        renewal_token_expires_at: prost_types::Timestamp {
            seconds: 1,
            nanos: 0,
        },
        generation: 1,
    });

    let request = RegisterRequest {
        actr_type: actor(1).r#type,
        realm: Realm { realm_id: 7 },
        ..Default::default()
    };

    run_renewal_once(
        session.clone(),
        server.url(),
        None,
        RegistrationContext::Linked {
            request,
            realm_secret: None,
        },
        None,
        None,
        pending_control_attempts(),
    )
    .await
    .expect("legacy hard rebind should commit a fresh registration");

    register_mock.assert_async().await;
    reissue_mock.assert_async().await;
    let snapshot = session.snapshot().await;
    // Fresh register: the new serial issued by AIS is adopted.
    assert_eq!(snapshot.actor_id, actor(2));
    assert_eq!(snapshot.credential.key_id, 9);
    assert_eq!(snapshot.generation, 2);
    assert_eq!(
        session.phase().await,
        super::super::session_state::SessionPhase::Active
    );
}

#[tokio::test]
async fn soft_renewal_fires_credential_renewed_hook() {
    const OLD_EXPIRY: i64 = 4_000_000_000;
    const NEW_EXPIRY: i64 = 4_000_001_000;
    let actor_id = actor(1);
    let mut server = mockito::Server::new_async().await;
    let response = RenewCredentialResponse {
        result: Some(renew_credential_response::Result::Success(
            register_response::RegisterOk {
                actr_id: actor_id.clone(),
                credential: credential_for_actor(&actor_id, 9, NEW_EXPIRY as u64),
                turn_credential: TurnCredential {
                    protocol_version: 1,
                    realm_id: 1,
                    provider: "actrix-official".to_string(),
                    audience: "actr.turn".to_string(),
                    username: "4000001000:actor-1".to_string(),
                    password: "turn-password".to_string(),
                    expires_at: NEW_EXPIRY as u64,
                },
                credential_expires_at: Some(prost_types::Timestamp {
                    seconds: NEW_EXPIRY,
                    nanos: 0,
                }),
                signaling_heartbeat_interval_secs: 30,
                signing_pubkey: Bytes::from(vec![1; 32]),
                signing_key_id: 9,
                renewal_token: Some(Bytes::from(vec![8; 32])),
                renewal_token_expires_at: Some(prost_types::Timestamp {
                    seconds: NEW_EXPIRY + 1000,
                    nanos: 0,
                }),
            },
        )),
    };
    let mock = server
        .mock("POST", "/renew")
        .with_status(200)
        .with_header("content-type", "application/x-protobuf")
        .with_body(response.encode_to_vec())
        .expect(1)
        .create_async()
        .await;

    let session = SessionState::new(super::super::session_state::SessionSnapshot {
        actor_id: actor_id.clone(),
        credential: credential_for_actor(&actor_id, 1, OLD_EXPIRY as u64),
        credential_expires_at: prost_types::Timestamp {
            seconds: OLD_EXPIRY,
            nanos: 0,
        },
        turn_credential: TurnCredential {
            protocol_version: 1,
            realm_id: 1,
            provider: "actrix-official".to_string(),
            audience: "actr.turn".to_string(),
            username: "4000000000:actor-1".to_string(),
            password: "old".to_string(),
            expires_at: OLD_EXPIRY as u64,
        },
        renewal_token: Bytes::from(vec![7; 32]),
        renewal_token_expires_at: prost_types::Timestamp {
            seconds: OLD_EXPIRY + 1000,
            nanos: 0,
        },
        generation: 1,
    });

    let hook_expiry = Arc::new(AtomicU64::new(0));
    let hook_expiry_for_cb = hook_expiry.clone();
    let hook_callback: crate::wire::webrtc::HookCallback = Arc::new(move |event| {
        let hook_expiry = hook_expiry_for_cb.clone();
        Box::pin(async move {
            if let crate::wire::webrtc::HookEvent::CredentialRenewed { new_expiry } = event {
                let secs = new_expiry
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("expiry should be after epoch")
                    .as_secs();
                hook_expiry.store(secs, AtomicOrdering::SeqCst);
            }
        })
    });

    run_renewal_once(
        session,
        server.url(),
        None,
        RegistrationContext::Linked {
            request: RegisterRequest {
                actr_type: actor_id.r#type.clone(),
                realm: actor_id.realm,
                ..Default::default()
            },
            realm_secret: None,
        },
        None,
        Some(hook_callback),
        pending_control_attempts(),
    )
    .await
    .expect("soft renewal should succeed");

    mock.assert_async().await;
    assert_eq!(hook_expiry.load(AtomicOrdering::SeqCst), NEW_EXPIRY as u64);
}

/// P2 regression: the credential-only hard reissue (membership controller
/// path) commits new credential material and must fire the documented
/// CredentialRenewed notification exactly like the soft-renew path.
#[tokio::test]
async fn credential_only_hard_reissue_fires_credential_renewed_hook() {
    let mut server = mockito::Server::new_async().await;
    let response = RegisterResponse {
        result: Some(register_response::Result::Success(register_ok(1, 9))),
    };
    let mock = server
        .mock("POST", "/reissue")
        .with_status(200)
        .with_header("content-type", "application/x-protobuf")
        .with_body(response.encode_to_vec())
        .expect(1)
        .create_async()
        .await;

    // Expired renewal token -> run_reacquire_once takes the hard-rebind branch.
    let session = SessionState::new(SessionSnapshot {
        actor_id: actor(1),
        credential: credential(1),
        credential_expires_at: prost_types::Timestamp {
            seconds: 10,
            nanos: 0,
        },
        turn_credential: TurnCredential {
            protocol_version: 1,
            realm_id: 1,
            provider: "actrix-official".to_string(),
            audience: "actr.turn".to_string(),
            username: "10:actor-1".to_string(),
            password: "old".to_string(),
            expires_at: 10,
        },
        renewal_token: Bytes::from_static(b"expired-renewal-token-32bytes"),
        renewal_token_expires_at: prost_types::Timestamp {
            seconds: 1,
            nanos: 0,
        },
        generation: 1,
    });

    let hook_expiry = Arc::new(AtomicU64::new(0));
    let hook_expiry_for_cb = hook_expiry.clone();
    let hook_callback: crate::wire::webrtc::HookCallback = Arc::new(move |event| {
        let hook_expiry = hook_expiry_for_cb.clone();
        Box::pin(async move {
            if let crate::wire::webrtc::HookEvent::CredentialRenewed { new_expiry } = event {
                let secs = new_expiry
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("expiry should be after epoch")
                    .as_secs();
                hook_expiry.store(secs, AtomicOrdering::SeqCst);
            }
        })
    });

    let outcome = run_reacquire_once(
        session,
        server.url(),
        None,
        RegistrationContext::Linked {
            request: RegisterRequest {
                actr_type: actor(1).r#type,
                realm: Realm { realm_id: 7 },
                ..Default::default()
            },
            realm_secret: None,
        },
        None,
        Some(hook_callback),
        pending_control_attempts(),
    )
    .await;

    mock.assert_async().await;
    assert!(
        matches!(outcome, ReacquireOutcome::Renewed(_)),
        "hard reissue should publish a renewed credential"
    );
    // register_ok(1, 9) advertises credential_expires_at = 1000s.
    assert_eq!(
        hook_expiry.load(AtomicOrdering::SeqCst),
        1000,
        "hard reissue must fire CredentialRenewed with the committed expiry"
    );
}

/// P1 regression: the membership `/reissue` path must map a possession-proof
/// failure (its own distinct wire code) to a retryable `Deferred`, NOT terminal
/// `Denied`. Conflating it with the realm-denial `403` permanently stopped
/// recovery for a missing / unknown / grace-expired proof.
#[tokio::test]
async fn reissue_possession_failure_defers_instead_of_denying() {
    let mut server = mockito::Server::new_async().await;
    let response = RegisterResponse {
        result: Some(register_response::Result::Error(
            actr_protocol::ErrorResponse {
                code: REISSUE_POSSESSION_NOT_PROVEN_CODE,
                message: "actor identity possession not proven".to_string(),
            },
        )),
    };
    let mock = server
        .mock("POST", "/reissue")
        .with_status(200)
        .with_header("content-type", "application/x-protobuf")
        .with_body(response.encode_to_vec())
        .expect(1)
        .create_async()
        .await;

    // Expired renewal token -> run_reacquire_once takes the hard-rebind branch.
    let session = SessionState::new(SessionSnapshot {
        actor_id: actor(1),
        credential: credential(1),
        credential_expires_at: prost_types::Timestamp {
            seconds: 10,
            nanos: 0,
        },
        turn_credential: TurnCredential {
            protocol_version: 1,
            realm_id: 1,
            provider: "actrix-official".to_string(),
            audience: "actr.turn".to_string(),
            username: "10:actor-1".to_string(),
            password: "old".to_string(),
            expires_at: 10,
        },
        renewal_token: Bytes::from_static(b"expired-renewal-token-32bytes"),
        renewal_token_expires_at: prost_types::Timestamp {
            seconds: 1,
            nanos: 0,
        },
        generation: 1,
    });
    let session_handle = session.clone();

    let outcome = run_reacquire_once(
        session,
        server.url(),
        None,
        RegistrationContext::Linked {
            request: RegisterRequest {
                actr_type: actor(1).r#type,
                realm: Realm { realm_id: 7 },
                ..Default::default()
            },
            realm_secret: None,
        },
        None,
        None,
        pending_control_attempts(),
    )
    .await;

    mock.assert_async().await;
    assert!(
        matches!(outcome, ReacquireOutcome::Deferred(_)),
        "possession failure must defer, not deny: {outcome:?}"
    );
    assert_ne!(
        session_handle.phase().await,
        SessionPhase::RealmUnavailable,
        "possession failure must NOT terminally mark the realm unavailable"
    );
}

/// P1 companion: a genuine realm denial (403) on `/reissue` IS terminal — the
/// controller marks the realm unavailable and stops automatic recovery.
#[tokio::test]
async fn reissue_realm_denied_is_terminal() {
    let mut server = mockito::Server::new_async().await;
    let response = RegisterResponse {
        result: Some(register_response::Result::Error(
            actr_protocol::ErrorResponse {
                code: 403,
                message: "Invalid realm secret".to_string(),
            },
        )),
    };
    let mock = server
        .mock("POST", "/reissue")
        .with_status(200)
        .with_header("content-type", "application/x-protobuf")
        .with_body(response.encode_to_vec())
        .expect(1)
        .create_async()
        .await;

    let session = SessionState::new(SessionSnapshot {
        actor_id: actor(1),
        credential: credential(1),
        credential_expires_at: prost_types::Timestamp {
            seconds: 10,
            nanos: 0,
        },
        turn_credential: TurnCredential {
            protocol_version: 1,
            realm_id: 1,
            provider: "actrix-official".to_string(),
            audience: "actr.turn".to_string(),
            username: "10:actor-1".to_string(),
            password: "old".to_string(),
            expires_at: 10,
        },
        renewal_token: Bytes::from_static(b"expired-renewal-token-32bytes"),
        renewal_token_expires_at: prost_types::Timestamp {
            seconds: 1,
            nanos: 0,
        },
        generation: 1,
    });
    let session_handle = session.clone();

    let outcome = run_reacquire_once(
        session,
        server.url(),
        None,
        RegistrationContext::Linked {
            request: RegisterRequest {
                actr_type: actor(1).r#type,
                realm: Realm { realm_id: 7 },
                ..Default::default()
            },
            realm_secret: None,
        },
        None,
        None,
        pending_control_attempts(),
    )
    .await;

    mock.assert_async().await;
    assert!(
        matches!(outcome, ReacquireOutcome::Denied),
        "realm denial must be terminal Denied: {outcome:?}"
    );
    assert_eq!(
        session_handle.phase().await,
        SessionPhase::RealmUnavailable,
        "realm denial must mark the realm terminally unavailable"
    );
}

/// The legacy fire-and-forget hard rebind path has the same contract: after a
/// committed re-register, the workload observes CredentialRenewed.
///
/// It also pins the flag-OFF recovery endpoint: the legacy path uses fresh
/// `/register`, never `/reissue` (see `legacy_hard_rebind_registers_fresh_serial`).
#[tokio::test]
async fn legacy_hard_rebind_fires_credential_renewed_hook() {
    let mut server = mockito::Server::new_async().await;
    let response = RegisterResponse {
        result: Some(register_response::Result::Success(register_ok(1, 9))),
    };
    let mock = server
        .mock("POST", "/register")
        .with_status(200)
        .with_header("content-type", "application/x-protobuf")
        .with_body(response.encode_to_vec())
        .expect(1)
        .create_async()
        .await;

    let session = SessionState::new(SessionSnapshot {
        actor_id: actor(1),
        credential: credential(1),
        credential_expires_at: prost_types::Timestamp {
            seconds: 10,
            nanos: 0,
        },
        turn_credential: TurnCredential {
            protocol_version: 1,
            realm_id: 1,
            provider: "actrix-official".to_string(),
            audience: "actr.turn".to_string(),
            username: "10:actor-1".to_string(),
            password: "old".to_string(),
            expires_at: 10,
        },
        renewal_token: Bytes::from_static(b"expired-renewal-token-32bytes"),
        renewal_token_expires_at: prost_types::Timestamp {
            seconds: 1,
            nanos: 0,
        },
        generation: 1,
    });

    let hook_expiry = Arc::new(AtomicU64::new(0));
    let hook_expiry_for_cb = hook_expiry.clone();
    let hook_callback: crate::wire::webrtc::HookCallback = Arc::new(move |event| {
        let hook_expiry = hook_expiry_for_cb.clone();
        Box::pin(async move {
            if let crate::wire::webrtc::HookEvent::CredentialRenewed { new_expiry } = event {
                let secs = new_expiry
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("expiry should be after epoch")
                    .as_secs();
                hook_expiry.store(secs, AtomicOrdering::SeqCst);
            }
        })
    });

    run_renewal_once(
        session,
        server.url(),
        None,
        RegistrationContext::Linked {
            request: RegisterRequest {
                actr_type: actor(1).r#type,
                realm: Realm { realm_id: 7 },
                ..Default::default()
            },
            realm_secret: None,
        },
        None,
        Some(hook_callback),
        pending_control_attempts(),
    )
    .await
    .expect("hard rebind should commit new snapshot");

    mock.assert_async().await;
    assert_eq!(
        hook_expiry.load(AtomicOrdering::SeqCst),
        1000,
        "legacy hard rebind must fire CredentialRenewed with the committed expiry"
    );
}

#[test]
fn backoff_sequence() {
    let mut b = Backoff::new();
    let d0 = b.next();
    let d1 = b.next();
    let d2 = b.next();
    let d3 = b.next();
    let d4 = b.next();

    assert!(d0 >= Duration::from_secs(1));
    assert!(d1 >= Duration::from_secs(1));
    assert!(d2 >= Duration::from_secs(1));
    assert!(d3 >= Duration::from_secs(1));
    assert!(d4 <= Duration::from_secs(75)); // 60 + 25% jitter
}

/// Hard rebind of a Package registration must re-invoke the manufacturer auth
/// provider to mint a fresh proof. The initial manufacturer nonce was consumed
/// by AIS on the first successful registration; replaying it would be
/// rejected. This locks in the re-sign behaviour added to fix replay on
/// hard rebind.
#[tokio::test]
async fn package_hard_rebind_re_signs_manufacturer_proof() {
    use actr_protocol::RegisterAuthMode;
    use std::sync::atomic::AtomicU64;

    struct CountingManufacturerProvider {
        calls: Arc<AtomicU64>,
    }
    impl crate::ManufacturerAuthProvider for CountingManufacturerProvider {
        fn sign(
            &self,
            _realm_id: u32,
            _actr_type: &actr_protocol::ActrType,
            _target: &str,
            _manifest_raw: &[u8],
        ) -> std::result::Result<crate::ManufacturerRegistrationAuth, crate::HyperError> {
            let n = self.calls.fetch_add(1, AtomicOrdering::SeqCst) + 1;
            // Deterministic fresh proof keyed by call count. The mock AIS
            // server does not validate crypto, so the values only need to
            // differ from the stale proof saved on the original request.
            Ok(crate::ManufacturerRegistrationAuth {
                signature: vec![n as u8; 64],
                signed_at: 9_999_999_999,
                nonce: vec![n as u8; 32],
            })
        }
    }

    let calls = Arc::new(AtomicU64::new(0));
    let provider: Arc<CountingManufacturerProvider> = Arc::new(CountingManufacturerProvider {
        calls: calls.clone(),
    });

    let mut server = mockito::Server::new_async().await;
    let response = RegisterResponse {
        result: Some(register_response::Result::Success(register_ok(1, 9))),
    };
    // Flag-OFF legacy hard rebind re-registers via fresh `/register`.
    let mock = server
        .mock("POST", "/register")
        .with_status(200)
        .with_header("content-type", "application/x-protobuf")
        .with_body(response.encode_to_vec())
        .expect(1)
        .create_async()
        .await;

    // Stale manufacturer proof (nonce 0xAA) saved on the original request — this
    // is exactly what must not be replayed to AIS.
    let request = RegisterRequest {
        actr_type: actor(1).r#type,
        realm: Realm { realm_id: 7 },
        manifest_raw: Some(Bytes::from_static(b"manifest-bytes")),
        mfr_signature: Some(Bytes::from_static(b"mfr-sig")),
        target: Some("wasm32-wasip1".to_string()),
        auth_mode: Some(RegisterAuthMode::Package as i32),
        manufacturer_auth_signature: Some(Bytes::from_static(b"stale-manufacturer-auth-sig")),
        manufacturer_auth_signed_at: Some(1),
        manufacturer_auth_nonce: Some(Bytes::from_static(&[0xAA; 32])),
        ..Default::default()
    };

    // Expired renewal token -> run_renewal_once takes the hard rebind branch.
    let session = SessionState::new(SessionSnapshot {
        actor_id: actor(1),
        credential: credential(1),
        credential_expires_at: prost_types::Timestamp {
            seconds: 10,
            nanos: 0,
        },
        turn_credential: TurnCredential {
            protocol_version: 1,
            realm_id: 1,
            provider: "actrix-official".to_string(),
            audience: "actr.turn".to_string(),
            username: "10:actor-1".to_string(),
            password: "old".to_string(),
            expires_at: 10,
        },
        renewal_token: Bytes::from_static(b"expired-renewal-token-32bytes"),
        renewal_token_expires_at: prost_types::Timestamp {
            seconds: 1,
            nanos: 0,
        },
        generation: 1,
    });

    run_renewal_once(
        session,
        server.url(),
        None,
        RegistrationContext::Package {
            request,
            resign: Some(provider),
        },
        None,
        None,
        pending_control_attempts(),
    )
    .await
    .expect("hard rebind should commit new snapshot");

    mock.assert_async().await;
    assert_eq!(
        calls.load(AtomicOrdering::SeqCst),
        1,
        "hard rebind must re-invoke the manufacturer auth provider exactly once"
    );
}

#[test]
fn is_expired_past_and_future() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Far past → expired.
    assert!(is_expired(now - 3600));
    assert!(is_expired(0));

    // Exactly now → expired (<=).
    assert!(is_expired(now));

    // Far future → not expired.
    assert!(!is_expired(now + 3600));
}

#[tokio::test]
async fn fire_credential_renewed_invokes_callback_with_expiry() {
    use std::sync::Mutex;

    let captured: Arc<Mutex<Option<std::time::SystemTime>>> = Arc::new(Mutex::new(None));
    let cap = captured.clone();
    let cb: HookCallback = Arc::new(move |event| {
        let cap = cap.clone();
        Box::pin(async move {
            if let HookEvent::CredentialRenewed { new_expiry } = event {
                *cap.lock().unwrap() = Some(new_expiry);
            }
        })
    });

    let expires_at = prost_types::Timestamp {
        seconds: 1000,
        nanos: 0,
    };
    fire_credential_renewed(Some(&cb), &expires_at).await;

    let got = captured.lock().unwrap().take();
    let expected = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
    assert_eq!(got, Some(expected));
}

#[tokio::test]
async fn fire_credential_renewed_none_callback_is_noop() {
    // None callback is the explicit no-op contract for runtimes without observers.
    fire_credential_renewed(
        None,
        &prost_types::Timestamp {
            seconds: 5,
            nanos: 0,
        },
    )
    .await;
}

#[tokio::test]
async fn fire_credential_renewed_clamps_negative_seconds_to_zero() {
    use std::sync::Mutex;

    let captured: Arc<Mutex<Option<std::time::SystemTime>>> = Arc::new(Mutex::new(None));
    let cap = captured.clone();
    let cb: HookCallback = Arc::new(move |event| {
        let cap = cap.clone();
        Box::pin(async move {
            if let HookEvent::CredentialRenewed { new_expiry } = event {
                *cap.lock().unwrap() = Some(new_expiry);
            }
        })
    });

    // Negative seconds (malformed/epoch) must clamp to 0 → UNIX_EPOCH.
    fire_credential_renewed(
        Some(&cb),
        &prost_types::Timestamp {
            seconds: -100,
            nanos: 0,
        },
    )
    .await;
    assert_eq!(*captured.lock().unwrap(), Some(SystemTime::UNIX_EPOCH));
}

#[test]
fn credential_expiry_rejects_timestamp_beyond_protobuf_range() {
    let maximum_valid = prost_types::Timestamp {
        seconds: 253_402_300_799,
        nanos: 0,
    };
    assert!(
        credential_expiry_to_system_time(&maximum_valid).is_some(),
        "the maximum google.protobuf.Timestamp value must remain representable"
    );
    assert_eq!(
        credential_expiry_to_system_time(&prost_types::Timestamp {
            seconds: i64::MAX,
            nanos: 0,
        }),
        None,
        "wire timestamps outside google.protobuf.Timestamp must be rejected on every platform"
    );
}

#[tokio::test]
async fn credential_expiry_overflow_skips_callback_instead_of_panicking() {
    let calls = Arc::new(AtomicU64::new(0));
    let calls_for_callback = calls.clone();
    let callback: HookCallback = Arc::new(move |_event| {
        let calls = calls_for_callback.clone();
        Box::pin(async move {
            calls.fetch_add(1, AtomicOrdering::SeqCst);
        })
    });

    fire_credential_renewed(
        Some(&callback),
        &prost_types::Timestamp {
            seconds: i64::MAX,
            nanos: 0,
        },
    )
    .await;

    assert_eq!(
        calls.load(AtomicOrdering::SeqCst),
        0,
        "an invalid remote expiry must not reach the observer callback"
    );
}

/// Issue #397: an AIS that predates the renewal-token feature returns a
/// successful `/register` whose `renewal_token` / `renewal_token_expires_at`
/// are `None`. The legacy hard rebind must still commit the access credential
/// (the field that actually keeps the node reachable) instead of discarding
/// the whole re-registration over an optional field — otherwise the node
/// keeps its expiring old credential and silently drops off once its TTL
/// lapses.
#[tokio::test]
async fn legacy_hard_rebind_commits_when_ais_omits_renewal_token() {
    let mut server = mockito::Server::new_async().await;
    let response = RegisterResponse {
        result: Some(register_response::Result::Success(
            register_ok_without_token(2, 9),
        )),
    };
    let mock = server
        .mock("POST", "/register")
        .with_status(200)
        .with_header("content-type", "application/x-protobuf")
        .with_body(response.encode_to_vec())
        .expect(1)
        .create_async()
        .await;

    let session = SessionState::new(SessionSnapshot {
        actor_id: actor(1),
        credential: credential(1),
        credential_expires_at: prost_types::Timestamp {
            seconds: 10,
            nanos: 0,
        },
        turn_credential: TurnCredential {
            username: "10:actor-1".to_string(),
            password: "old".to_string(),
            expires_at: 10,
            protocol_version: 1,
            realm_id: 1,
            provider: "actrix-official".to_string(),
            audience: "actr.turn".to_string(),
        },
        renewal_token: Bytes::from_static(b"expired-renewal-token-32bytes"),
        renewal_token_expires_at: prost_types::Timestamp {
            seconds: 1,
            nanos: 0,
        },
        generation: 1,
    });

    run_renewal_once(
        session.clone(),
        server.url(),
        None,
        RegistrationContext::Linked {
            request: RegisterRequest {
                actr_type: actor(1).r#type,
                realm: Realm { realm_id: 7 },
                ..Default::default()
            },
            realm_secret: None,
        },
        None,
        None,
        pending_control_attempts(),
    )
    .await
    .expect("hard rebind should commit even when AIS omits renewal_token");

    mock.assert_async().await;
    let snapshot = session.snapshot().await;
    // The freshly-issued credential IS committed.
    assert_eq!(snapshot.actor_id, actor(2));
    assert_eq!(snapshot.credential.key_id, 9);
    assert_eq!(snapshot.generation, 2);
    assert_eq!(
        session.phase().await,
        SessionPhase::Active,
        "committed rebind must leave the session Active"
    );
    // Token degrades to empty — soft renewal unavailable, next cycle hard-rebinds again.
    assert!(
        snapshot.renewal_token.is_empty(),
        "missing renewal_token must be committed as empty, not abort the rebind"
    );
}

/// Issue #397 symmetric path: the membership-controller `/reissue` rebind must
/// also commit a token-less success instead of returning `Deferred` and
/// discarding the reissued credential.
#[tokio::test]
async fn credential_only_hard_reissue_commits_when_ais_omits_renewal_token() {
    let mut server = mockito::Server::new_async().await;
    let response = RegisterResponse {
        result: Some(register_response::Result::Success(
            register_ok_without_token(1, 9),
        )),
    };
    let mock = server
        .mock("POST", "/reissue")
        .with_status(200)
        .with_header("content-type", "application/x-protobuf")
        .with_body(response.encode_to_vec())
        .expect(1)
        .create_async()
        .await;

    // Expired renewal token -> run_reacquire_once takes the hard-rebind branch.
    let session = SessionState::new(SessionSnapshot {
        actor_id: actor(1),
        credential: credential(1),
        credential_expires_at: prost_types::Timestamp {
            seconds: 10,
            nanos: 0,
        },
        turn_credential: TurnCredential {
            username: "10:actor-1".to_string(),
            password: "old".to_string(),
            expires_at: 10,
            protocol_version: 1,
            realm_id: 1,
            provider: "actrix-official".to_string(),
            audience: "actr.turn".to_string(),
        },
        renewal_token: Bytes::from_static(b"expired-renewal-token-32bytes"),
        renewal_token_expires_at: prost_types::Timestamp {
            seconds: 1,
            nanos: 0,
        },
        generation: 1,
    });
    let session_handle = session.clone();

    let outcome = run_reacquire_once(
        session,
        server.url(),
        None,
        RegistrationContext::Linked {
            request: RegisterRequest {
                actr_type: actor(1).r#type,
                realm: Realm { realm_id: 7 },
                ..Default::default()
            },
            realm_secret: None,
        },
        None,
        None,
        pending_control_attempts(),
    )
    .await;

    mock.assert_async().await;
    assert!(
        matches!(outcome, ReacquireOutcome::Renewed(_)),
        "token-less reissue must commit (Renewed), not defer: {outcome:?}"
    );
    assert_eq!(
        session_handle.snapshot().await.credential.key_id,
        9,
        "reissued credential must be committed"
    );
    assert_ne!(
        session_handle.phase().await,
        SessionPhase::RealmUnavailable,
        "missing optional token must not look like a terminal realm denial"
    );
}

// ---- Fix 2: non-transient failure classification & escalation -------------

#[test]
fn classify_renewal_error_categorizes_missing_field_as_non_transient() {
    // Missing optional field / contract violations are non-transient: no retry
    // can make an old AIS start returning renewal_token.
    assert_eq!(
        classify_renewal_error("hard rebind response missing renewal token"),
        FailureClass::NonTransient
    );
    assert_eq!(
        classify_renewal_error("hard rebind response missing renewal token expiry"),
        FailureClass::NonTransient
    );
    assert_eq!(
        classify_renewal_error("hard rebind response missing credential expiry"),
        FailureClass::NonTransient
    );
    assert_eq!(
        classify_renewal_error("renew response missing result"),
        FailureClass::NonTransient
    );
    // Transport / rate-limit / store errors are transient.
    assert_eq!(
        classify_renewal_error("retryable renew error: connection reset"),
        FailureClass::Transient
    );
    assert_eq!(
        classify_renewal_error("renewal rate limited"),
        FailureClass::Transient
    );
    assert_eq!(
        classify_renewal_error(
            "hard rebind register failed before commit: 503 service unavailable"
        ),
        FailureClass::Transient
    );
}

#[test]
fn record_failure_escalates_after_threshold() {
    let counter = AtomicU32::new(0);
    let err = "hard rebind response missing renewal token";

    // Below the threshold: still Warn, counter climbing.
    for _ in 0..(NON_TRANSIENT_FAILURE_ESCALATION_THRESHOLD - 1) {
        assert_eq!(record_failure(&counter, err), EscalationLevel::Warn);
    }
    assert_eq!(
        counter.load(AtomicOrdering::SeqCst),
        NON_TRANSIENT_FAILURE_ESCALATION_THRESHOLD - 1
    );

    // Nth non-transient failure escalates and resets the counter.
    assert_eq!(record_failure(&counter, err), EscalationLevel::Escalate);
    assert_eq!(
        counter.load(AtomicOrdering::SeqCst),
        0,
        "counter resets after escalating so the loud ERROR repeats per cycle, not per tick"
    );

    // Next failure starts a fresh count (Warn), proving the reset.
    assert_eq!(record_failure(&counter, err), EscalationLevel::Warn);
    assert_eq!(counter.load(AtomicOrdering::SeqCst), 1);
}

#[test]
fn record_failure_transient_resets_counter() {
    let counter = AtomicU32::new(0);
    let non_transient = "hard rebind response missing renewal token";
    let transient = "renewal rate limited";

    // Prime the counter one short of the threshold.
    for _ in 0..(NON_TRANSIENT_FAILURE_ESCALATION_THRESHOLD - 1) {
        record_failure(&counter, non_transient);
    }
    assert_eq!(
        counter.load(AtomicOrdering::SeqCst),
        NON_TRANSIENT_FAILURE_ESCALATION_THRESHOLD - 1
    );

    // A single transient blip must not be able to masquerade as a stuck
    // condition: it resets the counter and returns Warn.
    assert_eq!(record_failure(&counter, transient), EscalationLevel::Warn);
    assert_eq!(counter.load(AtomicOrdering::SeqCst), 0);

    // The next non-transient failure starts from 0, not from the primed value.
    assert_eq!(
        record_failure(&counter, non_transient),
        EscalationLevel::Warn
    );
    assert_eq!(counter.load(AtomicOrdering::SeqCst), 1);
}

#[tokio::test]
async fn renewal_attempt_body_survives_outer_recovery_rounds() {
    let pending = Arc::new(Mutex::new(PendingControlAttempts::default()));
    let mut snapshot = SessionSnapshot {
        actor_id: actor(41),
        credential: credential(1),
        credential_expires_at: prost_types::Timestamp::default(),
        turn_credential: TurnCredential::default(),
        renewal_token: Bytes::from_static(b"same-renewal-token"),
        renewal_token_expires_at: prost_types::Timestamp::default(),
        generation: 1,
    };

    let first = pending_renew_request(&pending, &snapshot).await;
    let retry = pending_renew_request(&pending, &snapshot).await;
    assert_eq!(retry, first, "outer retry must reuse the exact body");

    clear_pending_renew(&pending, &first).await;
    let definitive_retry = pending_renew_request(&pending, &snapshot).await;
    assert_ne!(definitive_retry.attempt_id, first.attempt_id);

    snapshot.renewal_token = Bytes::from_static(b"rotated-renewal-token");
    let rotated = pending_renew_request(&pending, &snapshot).await;
    assert_ne!(rotated.attempt_id, definitive_retry.attempt_id);
}

#[tokio::test]
async fn reissue_attempt_body_survives_outer_recovery_rounds() {
    let pending = Arc::new(Mutex::new(PendingControlAttempts::default()));
    let snapshot = SessionSnapshot {
        actor_id: actor(42),
        credential: credential(1),
        credential_expires_at: prost_types::Timestamp::default(),
        turn_credential: TurnCredential::default(),
        renewal_token: Bytes::from_static(b"reissue-possession-proof"),
        renewal_token_expires_at: prost_types::Timestamp::default(),
        generation: 1,
    };
    let request = ReissueCredentialRequest {
        actr_id: snapshot.actor_id.clone(),
        registration: RegisterRequest::default(),
        renewal_proof: Some(snapshot.renewal_token.clone()),
        attempt_id: new_control_attempt_id(),
    };

    store_pending_reissue(&pending, &request).await;
    let retry = pending_reissue_request(&pending, &snapshot)
        .await
        .expect("matching outer retry should be retained");
    assert_eq!(retry, request, "outer retry must reuse the exact body");
}
