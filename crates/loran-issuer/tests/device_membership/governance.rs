//! Governance calls use real SDK signing, HTTP, Issuer transactions and Signer.
use super::*;
use actr_protocol::realm_membership::{MembershipOp, MembershipRequest};
use axum::{middleware, response::IntoResponse};

struct Fixture {
    env: TestEnv,
    server: JoinHandle<()>,
    endpoint: String,
    lose_next: Arc<Mutex<Option<MembershipOp>>>,
}

fn expires() -> String {
    (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339()
}

impl Fixture {
    async fn start() -> Self {
        let env = setup_test_environment().await;
        seed_realm(TEST_REALM, None).await;
        let lose_next = Arc::new(Mutex::new(None));
        let loss = lose_next.clone();
        let app = create_test_router(&env).await.layer(middleware::from_fn(
            move |request: Request<Body>, next: middleware::Next| {
                let loss = loss.clone();
                async move {
                    if request.uri().path() != "/membership" {
                        return next.run(request).await;
                    }
                    let (parts, body) = request.into_parts();
                    let bytes = to_bytes(body, 1024 * 1024).await.unwrap();
                    let request = MembershipRequest::parse(&bytes).unwrap();
                    let response = next
                        .run(Request::from_parts(parts, Body::from(bytes)))
                        .await;
                    if response.status().is_success()
                        && loss
                            .lock()
                            .unwrap()
                            .take_if(|op| *op == request.op)
                            .is_some()
                    {
                        StatusCode::BAD_GATEWAY.into_response()
                    } else {
                        response
                    }
                }
            },
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .unwrap();
        });
        Self {
            env,
            server,
            endpoint,
            lose_next,
        }
    }

    fn owner(&self) -> RealmMembershipClient {
        RealmMembershipClient::new(
            MembershipEndpoint {
                realm: TEST_REALM.into(),
                provider_id: PROVIDER_ID.into(),
                issuer: self.endpoint.clone(),
            },
            RealmClientKey::new(TEST_REALM, PROVIDER_ID, key_for(DEFAULT_KEY_LABEL)),
            true,
        )
        .unwrap()
    }

    async fn member(
        &self,
        owner: &RealmMembershipClient,
        role: Role,
    ) -> (RealmMembershipClient, String) {
        let invite = owner.invite_member(role, &expires()).await.unwrap();
        let member = device(&self.endpoint, TEST_REALM);
        let joined = member
            .join(invite["invite"].as_str().unwrap(), "SDK governance")
            .await
            .unwrap();
        (member, joined["member_id"].as_str().unwrap().into())
    }

    fn lose(&self, op: MembershipOp) {
        *self.lose_next.lock().unwrap() = Some(op);
    }

    async fn close(mut self) {
        self.server.abort();
        self.env.shutdown_signer().await;
    }
}

async fn register(
    fixture: &Fixture,
    member: &RealmMembershipClient,
) -> register_response::RegisterOk {
    match IssuerClient::new(&fixture.endpoint)
        .with_client_key(member.client_key())
        .register_linked(linked_register_request())
        .await
        .unwrap()
        .result
        .unwrap()
    {
        register_response::Result::Success(ok) => ok,
        other => panic!("registration failed: {other:?}"),
    }
}

#[tokio::test]
#[serial]
async fn sdk_governance_permissions_roles_and_atomic_owner_transfer() {
    let fixture = Fixture::start().await;
    let owner = fixture.owner();
    let owner_id = owner.members().await.unwrap()["members"][0]["member_id"]
        .as_str()
        .unwrap()
        .to_string();
    refused(owner.leave().await, "owner_must_transfer");
    refused(owner.remove_member(&owner_id).await, "owner_must_transfer");
    refused(
        owner.revoke_client(&owner.key_id()).await,
        "owner_last_client",
    );
    let (editor, editor_id) = fixture.member(&owner, Role::Editor).await;
    let (viewer, viewer_id) = fixture.member(&owner, Role::Viewer).await;
    for member in [&editor, &viewer] {
        refused(
            member.invite_member(Role::Viewer, &expires()).await,
            "forbidden",
        );
        refused(member.remove_member(&editor_id).await, "forbidden");
        refused(member.set_role(&editor_id, Role::Viewer).await, "forbidden");
        refused(
            member.transfer_owner(&editor_id, Role::Editor).await,
            "forbidden",
        );
    }
    let original = register(&fixture, &editor).await;
    assert_eq!(
        actr_protocol::IdentityClaims::decode(original.credential.claims.as_ref())
            .unwrap()
            .role(),
        actr_protocol::MemberRole::Editor
    );
    owner.set_role(&editor_id, Role::Viewer).await.unwrap();
    // Existing signed bytes remain unchanged; the next issued credential carries the new role.
    assert_eq!(
        actr_protocol::IdentityClaims::decode(original.credential.claims.as_ref())
            .unwrap()
            .role(),
        actr_protocol::MemberRole::Editor
    );
    let updated = register(&fixture, &editor).await;
    assert_eq!(
        actr_protocol::IdentityClaims::decode(updated.credential.claims.as_ref())
            .unwrap()
            .role(),
        actr_protocol::MemberRole::Viewer
    );

    let transferred = owner
        .transfer_owner(&viewer_id, Role::Viewer)
        .await
        .unwrap();
    assert_eq!(transferred["previous_owner"], owner_id);
    assert_eq!(transferred["owner"], viewer_id);
    let members = viewer.members().await.unwrap();
    let rows = members["members"].as_array().unwrap();
    assert_eq!(rows.iter().filter(|row| row["role"] == "owner").count(), 1);
    assert_eq!(
        rows.iter()
            .find(|row| row["member_id"] == owner_id)
            .unwrap()["role"],
        "viewer"
    );
    refused(owner.set_role(&editor_id, Role::Editor).await, "forbidden");
    viewer.set_role(&editor_id, Role::Editor).await.unwrap();
    owner.leave().await.unwrap();
    assert!(viewer.members().await.is_ok());
    fixture.close().await;
}

#[tokio::test]
#[serial]
async fn sdk_removal_and_leave_invalidate_only_the_target_members_resources() {
    let fixture = Fixture::start().await;
    let owner = fixture.owner();
    let (unaffected, _) = fixture.member(&owner, Role::Editor).await;
    let unaffected_actor = register(&fixture, &unaffected).await;
    for leaving in [false, true] {
        let (member, member_id) = fixture.member(&owner, Role::Editor).await;
        let invite = member.invite_client(&member_id, &expires()).await.unwrap();
        let second = device(&fixture.endpoint, TEST_REALM);
        second
            .join(invite["invite"].as_str().unwrap(), "second device")
            .await
            .unwrap();
        let unused = member.invite_client(&member_id, &expires()).await.unwrap();
        let actor = register(&fixture, &member).await;
        let second_actor = register(&fixture, &second).await;
        if leaving {
            member.leave().await.unwrap();
        } else {
            owner.remove_member(&member_id).await.unwrap();
        }
        refused(member.members().await, "client_revoked");
        refused(second.members().await, "client_revoked");
        refused(
            device(&fixture.endpoint, TEST_REALM)
                .join(unused["invite"].as_str().unwrap(), "late device")
                .await,
            "invite_revoked",
        );
        for actor in [&actor, &second_actor] {
            assert!(membership::actor_disabled(&actor.actr_id).await.unwrap());
        }
        assert!(
            !membership::actor_disabled(&unaffected_actor.actr_id)
                .await
                .unwrap()
        );
        assert!(unaffected.members().await.is_ok());
        assert!(owner.members().await.is_ok());
    }
    fixture.close().await;
}

#[tokio::test]
#[serial]
async fn sdk_governance_lost_responses_preserve_existing_retry_semantics() {
    let fixture = Fixture::start().await;
    let owner = fixture.owner();
    fixture.lose(MembershipOp::CreateInvite);
    assert!(matches!(
        owner.invite_member(Role::Editor, &expires()).await,
        Err(MembershipClientError::Uncertain)
    ));
    // CreateInvite has no idempotency key: an explicit retry creates a second invitation.
    owner.invite_member(Role::Editor, &expires()).await.unwrap();
    assert_eq!(
        owner.members().await.unwrap()["invites"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let (member, member_id) = fixture.member(&owner, Role::Editor).await;
    fixture.lose(MembershipOp::SetRole);
    assert!(matches!(
        owner.set_role(&member_id, Role::Viewer).await,
        Err(MembershipClientError::Uncertain)
    ));
    assert_eq!(
        owner.set_role(&member_id, Role::Viewer).await.unwrap()["role"],
        "viewer"
    );
    fixture.lose(MembershipOp::RemoveMember);
    assert!(matches!(
        owner.remove_member(&member_id).await,
        Err(MembershipClientError::Uncertain)
    ));
    owner.remove_member(&member_id).await.unwrap();
    refused(member.members().await, "client_revoked");
    let (next, next_id) = fixture.member(&owner, Role::Editor).await;
    fixture.lose(MembershipOp::TransferOwner);
    assert!(matches!(
        owner.transfer_owner(&next_id, Role::Viewer).await,
        Err(MembershipClientError::Uncertain)
    ));
    refused(
        owner.transfer_owner(&next_id, Role::Viewer).await,
        "forbidden",
    );
    assert_eq!(
        next.members().await.unwrap()["members"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["role"] == "owner")
            .count(),
        1
    );
    fixture.lose(MembershipOp::Leave);
    assert!(matches!(
        owner.leave().await,
        Err(MembershipClientError::Uncertain)
    ));
    refused(owner.leave().await, "client_revoked");
    assert!(next.members().await.is_ok());
    fixture.close().await;
}

#[tokio::test]
#[serial]
async fn sdk_owner_transfer_races_and_authorized_recovery_keep_one_owner() {
    let fixture = Fixture::start().await;
    let owner = fixture.owner();
    let (a, a_id) = fixture.member(&owner, Role::Editor).await;
    let (b, b_id) = fixture.member(&owner, Role::Editor).await;
    let (first, second) = tokio::join!(
        owner.transfer_owner(&a_id, Role::Viewer),
        owner.transfer_owner(&b_id, Role::Viewer),
    );
    let current = match (first, second) {
        (Ok(_), error) => {
            refused(error, "forbidden");
            &a
        }
        (error, Ok(_)) => {
            refused(error, "forbidden");
            &b
        }
        other => panic!("exactly one transfer must succeed: {other:?}"),
    };
    let actor = register(&fixture, current).await;
    let list = current.members().await.unwrap();
    assert_eq!(
        list["members"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["role"] == "owner")
            .count(),
        1
    );
    // Only the legitimate operator/domain authority creates recovery invitations.
    let invitation = membership::issue_owner_invite(TEST_REALM, BY_OPERATOR)
        .await
        .unwrap();
    let recovered = device(&fixture.endpoint, TEST_REALM);
    let joined = recovered
        .join(&invitation.invite, "authorized recovery")
        .await
        .unwrap();
    assert_eq!(joined["role"], "owner");
    refused(current.members().await, "client_revoked");
    assert!(membership::actor_disabled(&actor.actr_id).await.unwrap());
    let list = recovered.members().await.unwrap();
    assert_eq!(
        list["members"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["role"] == "owner")
            .count(),
        1
    );
    assert!(owner.members().await.is_ok());
    fixture.close().await;
}

/// Starts a fresh real Issuer for Swift/Kotlin native-binding acceptance.
/// The only secret is an operator-issued owner invite in a private local file.
#[cfg(unix)]
#[tokio::test]
#[serial]
#[ignore = "started by tests/e2e/actr/member-governance/run.sh"]
async fn governance_issuer_http_fixture() {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let ready = std::env::var("ACTR_GOVERNANCE_FIXTURE_READY").expect("fixture path");
    let fixture = Fixture::start().await;
    let invitation = membership::issue_owner_invite(TEST_REALM, BY_OPERATOR)
        .await
        .unwrap();
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&ready)
        .unwrap();
    file.write_all(
        serde_json::to_string(&serde_json::json!({
            "realm": TEST_REALM, "provider_id": PROVIDER_ID, "issuer": fixture.endpoint,
            "owner_invite": invitation.invite,
        }))
        .unwrap()
        .as_bytes(),
    )
    .unwrap();
    for _ in 0..600 {
        if std::path::Path::new(&format!("{ready}.stop")).exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    std::fs::remove_file(ready).unwrap();
    fixture.close().await;
}
