//! Regressions for durable client state changing between authentication and upgrade.

use super::*;
use actr_protocol::realm_membership::{Role, client_key_id};
use actr_protocol::{
    ActrToSignaling, Ping, SignalingEnvelope, actr_to_signaling, signaling_envelope,
    signaling_to_actr,
};
use ed25519_dalek::{Signer as _, SigningKey};
use futures_util::{SinkExt, StreamExt};
use platform::realm::membership;
use tokio::sync::Notify;
use tokio::time::{Duration, timeout};
use tokio_tungstenite::{connect_async, tungstenite::Message};

const REALM: &str = "review305@example.com";

async fn fixture(serial: u64, signing: &SigningKey) -> (ActrId, AIdCredential, membership::Signer) {
    let device = SigningKey::from_bytes(&[serial as u8; 32]);
    let key_id = client_key_id(&device.verifying_key().to_bytes());
    let member = format!("{serial:016x}");
    let pool = platform::storage::db::get_database().get_pool();
    sqlx::query("INSERT INTO realm_member (realm, member_id, role, status, created_at) VALUES (?, ?, 'editor', 'active', unixepoch())")
        .bind(REALM).bind(&member).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO realm_client_key (realm, key_id, public_key, member_id, registered_at) VALUES (?, ?, ?, ?, unixepoch())")
        .bind(REALM).bind(&key_id).bind(device.verifying_key().to_bytes().to_vec()).bind(&member).execute(pool).await.unwrap();
    let actor_type = ActrType {
        publisher: "review".into(),
        name: "client".into(),
        version: "1.0.0".into(),
    };
    let actor = ActrId {
        realm: REALM.into(),
        serial_number: serial,
    };
    sqlx::query("INSERT INTO issued_actor_identity (realm, serial_number, actor_type, member_id, client_key_id, issued_at) VALUES (?, ?, ?, ?, ?, unixepoch())")
        .bind(REALM).bind(serial as i64).bind(actor_type.to_string_repr()).bind(&member).bind(&key_id).execute(pool).await.unwrap();
    let claims = IdentityClaims {
        realm: REALM.into(),
        actor_id: actor.to_string_repr(),
        expires_at: chrono::Utc::now().timestamp() as u64 + 3600,
        protocol_version: 1,
        issuer: REALM.into(),
        provider: "provider.example".into(),
        audience: "actr.signaling".into(),
        actor_type,
        member: member.clone(),
        client_key: key_id.clone(),
        role: actr_protocol::MemberRole::Editor as i32,
        development: None,
    }
    .encode_to_vec();
    let credential = AIdCredential {
        key_id: 305,
        signature: signing.sign(&claims).to_bytes().to_vec().into(),
        claims: claims.into(),
    };
    let signer = membership::Signer {
        realm: REALM.into(),
        key_id,
        member_id: member,
        role: Role::Editor,
    };
    (actor, credential, signer)
}

fn address(base: &str, actor: &ActrId, credential: &AIdCredential, pause: bool) -> String {
    let b64 = base64::engine::general_purpose::STANDARD;
    // Encode the three characters in standard base64 that have URL semantics.
    let escape = |s: String| {
        s.replace('+', "%2B")
            .replace('/', "%2F")
            .replace('=', "%3D")
    };
    format!(
        "{base}/ws?actor_id={}&key_id={}&claims={}&signature={}&pause={pause}",
        actor.to_string_repr(),
        credential.key_id,
        escape(b64.encode(&credential.claims)),
        escape(b64.encode(&credential.signature))
    )
}

#[tokio::test]
#[serial_test::serial]
async fn revocation_or_rebinding_during_upgrade_closes_connection() {
    static DIR: tokio::sync::OnceCell<tempfile::TempDir> = tokio::sync::OnceCell::const_new();
    let dir = DIR
        .get_or_init(|| async {
            let dir = tempfile::tempdir().unwrap();
            if let Err(error) = platform::storage::db::set_db_path(dir.path()).await {
                assert!(error.to_string().contains("already initialized"), "{error}");
            }
            dir
        })
        .await;
    RealmEntity::create(REALM.into()).await.unwrap();
    AIdCredentialValidator::init(dir.path()).await.unwrap();
    let signing = SigningKey::from_bytes(&[77; 32]);
    AIdCredentialValidator::populate_key(
        REALM,
        305,
        &signing.verifying_key(),
        chrono::Utc::now().timestamp() as u64 + 3600,
    )
    .await
    .unwrap();
    let (control_actor, control_credential, control_signer) = fixture(2, &signing).await;
    let mut server = SignalingServer::new();
    server.provider_id = "provider.example".into();
    let server = Arc::new(server);
    let handle = server.handle();
    let cancel = tokio_util::sync::CancellationToken::new();
    let follower = tokio::spawn(crate::server::follow_realm_retirements(
        handle.clone(),
        cancel.clone(),
    ));
    tokio::task::yield_now().await;
    let ready = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let state = SignalingState {
        server,
        counters: None,
        require_url_auth: true,
    };
    let app = Router::new()
        .route("/ws", get(websocket_handler))
        .with_state(state)
        .layer(axum::middleware::from_fn({
            let ready = ready.clone();
            let release = release.clone();
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                let ready = ready.clone();
                let release = release.clone();
                async move {
                    let pause = request
                        .uri()
                        .query()
                        .is_some_and(|q| q.contains("pause=true"));
                    let response = next.run(request).await;
                    if pause {
                        assert_eq!(
                            response.status(),
                            axum::http::StatusCode::SWITCHING_PROTOCOLS
                        );
                        ready.notify_one();
                        release.notified().await;
                    }
                    response
                }
            }
        }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("ws://{}", listener.local_addr().unwrap());
    let listener_task = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });

    // Control: a normally connected client is closed by the real revocation follower.
    let (mut control, _) =
        connect_async(address(&base, &control_actor, &control_credential, false))
            .await
            .unwrap();
    timeout(Duration::from_secs(3), async {
        while !handle
            .actor_id_index
            .read()
            .await
            .contains_key(&control_actor)
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    membership::revoke_client(&control_signer, &control_signer.key_id)
        .await
        .unwrap();
    let closed = timeout(Duration::from_secs(3), control.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(matches!(&closed, Message::Close(Some(frame)) if u16::from(frame.code) == 1008));

    for (serial, rebind, replacement_first) in
        [(1, false, false), (3, true, false), (5, true, true)]
    {
        let (actor, credential, signer) = fixture(serial, &signing).await;
        // A cached row makes completion of the real revocation subscriber observable.
        let actor_type = IdentityClaims::decode(credential.claims.as_ref())
            .unwrap()
            .actor_type;
        handle
            .actor_registry
            .write()
            .await
            .register_service(
                actor.clone(),
                actor_type,
                "upgrade-race".into(),
                vec![],
                None,
            )
            .unwrap();
        let url = address(&base, &actor, &credential, true);
        let connecting = tokio::spawn(async move { connect_async(url).await });
        timeout(Duration::from_secs(3), ready.notified())
            .await
            .unwrap();
        assert!(!handle.actor_id_index.read().await.contains_key(&actor));
        let replacement = if rebind {
            // Persist the same Actor-to-key transition as a credential reissue.
            let device = SigningKey::from_bytes(&[44 + serial as u8; 32]);
            let key_id = client_key_id(&device.verifying_key().to_bytes());
            let pool = platform::storage::db::get_database().get_pool();
            sqlx::query("INSERT INTO realm_client_key (realm, key_id, public_key, member_id, registered_at) VALUES (?, ?, ?, ?, unixepoch())")
                .bind(REALM).bind(&key_id).bind(device.verifying_key().to_bytes().to_vec())
                .bind(&signer.member_id).execute(pool).await.unwrap();
            sqlx::query("UPDATE issued_actor_identity SET client_key_id = ? WHERE realm = ? AND serial_number = ?")
                .bind(&key_id).bind(REALM).bind(serial as i64).execute(pool).await.unwrap();
            let mut claims = IdentityClaims::decode(credential.claims.as_ref()).unwrap();
            claims.client_key = key_id;
            let claims = claims.encode_to_vec();
            Some(AIdCredential {
                key_id: credential.key_id,
                signature: signing.sign(&claims).to_bytes().to_vec().into(),
                claims: claims.into(),
            })
        } else {
            membership::revoke_client(&signer, &signer.key_id)
                .await
                .unwrap();
            timeout(Duration::from_secs(3), async {
                while handle
                    .actor_registry
                    .read()
                    .await
                    .get_actor_type(&actor)
                    .is_some()
                {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            None
        };
        assert!(
            membership::credential_refused(&actor, &signer.key_id)
                .await
                .unwrap()
        );
        let ping = |credential: AIdCredential| {
            SignalingEnvelope {
                envelope_version: actr_protocol::SIGNALING_ENVELOPE_VERSION,
                envelope_id: "upgrade-race-ping".into(),
                reply_for: None,
                traceparent: None,
                tracestate: None,
                flow: Some(signaling_envelope::Flow::ActrToServer(ActrToSignaling {
                    source: actor.clone(),
                    credential,
                    payload: Some(actr_to_signaling::Payload::Ping(Ping::default())),
                })),
            }
            .encode_to_vec()
        };
        let mut incumbent = if replacement_first {
            let replacement = replacement.as_ref().unwrap();
            let (mut ws, _) = connect_async(address(&base, &actor, replacement, false))
                .await
                .unwrap();
            ws.send(Message::Binary(ping(replacement.clone()).into()))
                .await
                .unwrap();
            let message = timeout(Duration::from_secs(3), ws.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let Message::Binary(bytes) = message else {
                panic!("expected healthy incumbent: {message:?}");
            };
            let response = SignalingEnvelope::decode(bytes.as_ref()).unwrap();
            assert!(matches!(
                response.flow,
                Some(signaling_envelope::Flow::ServerToActr(
                    actr_protocol::SignalingToActr {
                        payload: Some(signaling_to_actr::Payload::Pong(_)),
                        ..
                    }
                ))
            ));
            Some(ws)
        } else {
            None
        };
        let incumbent_id = handle.actor_id_index.read().await.get(&actor).cloned();
        let registry_type_before = handle
            .actor_registry
            .read()
            .await
            .get_actor_type(&actor)
            .cloned();
        release.notify_one();
        let (mut ws, _) = timeout(Duration::from_secs(3), connecting)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        // Sending immediately must never yield an authenticated Pong.
        let _ = ws
            .send(Message::Binary(ping(credential.clone()).into()))
            .await;
        let message = timeout(Duration::from_secs(3), ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(
            matches!(&message, Message::Close(Some(frame)) if u16::from(frame.code) == 1008),
            "{message:?}"
        );
        if replacement_first {
            assert_eq!(
                handle.actor_id_index.read().await.get(&actor).cloned(),
                incumbent_id,
                "rejecting an obsolete handshake must preserve the incumbent's routing index"
            );
            assert!(
                handle
                    .clients
                    .read()
                    .await
                    .contains_key(incumbent_id.as_ref().unwrap())
            );
            assert!(
                handle
                    .actor_registry
                    .read()
                    .await
                    .get_actor_type(&actor)
                    .is_some()
            );
        } else {
            assert!(!handle.actor_id_index.read().await.contains_key(&actor));
            assert!(
                !handle
                    .clients
                    .read()
                    .await
                    .values()
                    .any(|client| client.actor_id.as_ref() == Some(&actor))
            );
        }
        // A rejected candidate must not publish a row or remove the current
        // binding's existing discovery metadata (including a cached row).
        assert_eq!(
            handle
                .actor_registry
                .read()
                .await
                .get_actor_type(&actor)
                .cloned(),
            registry_type_before
        );
        let fresh = connect_async(address(&base, &actor, &credential, false))
            .await
            .unwrap_err();
        assert!(
            matches!(&fresh, tokio_tungstenite::tungstenite::Error::Http(response)
            if response.status() == axum::http::StatusCode::FORBIDDEN)
        );
        if let Some(replacement) = replacement {
            // Only the obsolete credential is refused; the new binding works.
            let mut ws = if let Some(ws) = incumbent.take() {
                ws
            } else {
                connect_async(address(&base, &actor, &replacement, false))
                    .await
                    .unwrap()
                    .0
            };
            ws.send(Message::Binary(ping(replacement).into()))
                .await
                .unwrap();
            let message = timeout(Duration::from_secs(3), ws.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let Message::Binary(bytes) = message else {
                panic!("{message:?}");
            };
            let response = SignalingEnvelope::decode(bytes.as_ref()).unwrap();
            assert!(matches!(
                response.flow,
                Some(signaling_envelope::Flow::ServerToActr(
                    actr_protocol::SignalingToActr {
                        payload: Some(signaling_to_actr::Payload::Pong(_)),
                        ..
                    }
                ))
            ));
            ws.close(None).await.unwrap();
        }
    }
    cancel.cancel();
    listener_task.abort();
    follower.abort();
}
