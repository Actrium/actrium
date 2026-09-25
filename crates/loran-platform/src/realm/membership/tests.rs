use super::*;
use crate::util::test_utils::utils::setup_test_db;
use actr_protocol::realm_membership::client_key_id;
use serial_test::serial;

const REALM: &str = "k7m2q9xh@acme.com";

const LIMITS: Limits = Limits {
    max_members: 3,
    max_clients_per_member: 2,
    max_open_invites: 4,
};

fn key(seed: u8) -> [u8; 32] {
    ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
        .verifying_key()
        .to_bytes()
}

fn secret_of(invite: &IssuedInvite) -> String {
    RealmInvite::decode(&invite.invite).unwrap().secret
}

async fn redeem(invite: &IssuedInvite, seed: u8) -> Result<Value, MembershipError> {
    let hash = invite_secret_hash(&secret_of(invite)).unwrap();
    join(REALM, &hash, &key(seed), "test", LIMITS).await
}

async fn signer(seed: u8) -> Signer {
    let key = client_key(REALM, &client_key_id(&key(seed)))
        .await
        .unwrap()
        .unwrap();
    authenticate(key).await.unwrap()
}

fn code<T: std::fmt::Debug>(result: Result<T, MembershipError>) -> Code {
    result.unwrap_err().code
}

/// A Realm whose owner holds the key of seed 1.
async fn realm_with_owner() -> Signer {
    setup_test_db().await.unwrap();
    Realm::create(REALM.to_string()).await.unwrap();
    let invite = issue_owner_invite(REALM, BY_OPERATOR).await.unwrap();
    let joined = redeem(&invite, 1).await.unwrap();
    assert_eq!(joined["role"], "owner");
    signer(1).await
}

async fn invite_member(owner: &Signer, role: Role, seed: u8) -> Signer {
    let invite = create_invite(owner, Some(role), None, now_secs() + 3600, LIMITS)
        .await
        .unwrap();
    redeem(&invite, seed).await.unwrap();
    signer(seed).await
}

async fn bind_actor(signer: &Signer, serial: i64) -> ActrId {
    sqlx::query(
        "INSERT INTO issued_actor_identity
            (realm, serial_number, actor_type, member_id, client_key_id, issued_at)
         VALUES (?, ?, 'acme.com/notes@1', ?, ?, 1)",
    )
    .bind(REALM)
    .bind(serial)
    .bind(&signer.member_id)
    .bind(&signer.key_id)
    .execute(get_database().get_pool())
    .await
    .unwrap();
    ActrId {
        realm: REALM.to_string(),
        serial_number: serial as u64,
    }
}

#[tokio::test]
#[serial]
async fn invites_are_single_use_and_distinguish_their_failures() {
    let owner = realm_with_owner().await;
    let invite = create_invite(&owner, Some(Role::Editor), None, now_secs() + 3600, LIMITS)
        .await
        .unwrap();
    let joined = redeem(&invite, 2).await.unwrap();
    assert_eq!(joined["role"], "editor");
    // A retry by the same key returns the first result; another key fails.
    assert_eq!(redeem(&invite, 2).await.unwrap(), joined);
    assert_eq!(code(redeem(&invite, 3).await), Code::InviteUsed);

    let revoked = create_invite(&owner, Some(Role::Viewer), None, now_secs() + 3600, LIMITS)
        .await
        .unwrap();
    revoke_invite(&owner, &revoked.invite_id).await.unwrap();
    // Revoking again is fine.
    revoke_invite(&owner, &revoked.invite_id).await.unwrap();
    assert_eq!(code(redeem(&revoked, 3).await), Code::InviteRevoked);

    let expired = create_invite(&owner, Some(Role::Viewer), None, now_secs() + 3600, LIMITS)
        .await
        .unwrap();
    sqlx::query("UPDATE realm_invite SET expires_at = 1 WHERE invite_id = ?")
        .bind(&expired.invite_id)
        .execute(get_database().get_pool())
        .await
        .unwrap();
    assert_eq!(code(redeem(&expired, 3).await), Code::InviteExpired);
    let unknown = invite_secret_hash(&URL_SAFE_NO_PAD.encode([0u8; 32])).unwrap();
    assert_eq!(
        code(join(REALM, &unknown, &key(3), "", LIMITS).await),
        Code::InviteUnknown
    );

    // The same key cannot register twice.
    let again = create_invite(&owner, Some(Role::Viewer), None, now_secs() + 3600, LIMITS)
        .await
        .unwrap();
    assert_eq!(code(redeem(&again, 2).await), Code::Conflict);
}

#[tokio::test]
#[serial]
async fn only_the_owner_manages_members() {
    let owner = realm_with_owner().await;
    let editor = invite_member(&owner, Role::Editor, 2).await;
    let viewer = invite_member(&owner, Role::Viewer, 3).await;

    assert_eq!(
        code(create_invite(&editor, Some(Role::Viewer), None, now_secs() + 60, LIMITS).await),
        Code::Forbidden
    );
    // A member invites a client for itself only.
    create_invite(
        &editor,
        None,
        Some(&editor.member_id),
        now_secs() + 60,
        LIMITS,
    )
    .await
    .unwrap();
    assert_eq!(
        code(
            create_invite(
                &editor,
                None,
                Some(&viewer.member_id),
                now_secs() + 60,
                LIMITS
            )
            .await
        ),
        Code::Forbidden
    );
    // Roles are checked before targets.
    assert_eq!(
        code(remove_member(&editor, "0000000000000000").await),
        Code::Forbidden
    );
    assert_eq!(
        code(remove_member(&owner, "0000000000000000").await),
        Code::NotFound
    );
    assert_eq!(
        code(set_role(&owner, &viewer.member_id, Role::Owner).await),
        Code::InvalidRequest
    );
    assert_eq!(
        code(create_invite(&owner, Some(Role::Owner), None, now_secs() + 60, LIMITS).await),
        Code::InvalidRequest
    );
    assert_eq!(
        code(
            create_invite(
                &owner,
                Some(Role::Viewer),
                None,
                now_secs() + 31 * 86400,
                LIMITS
            )
            .await
        ),
        Code::InvalidRequest
    );
    set_role(&owner, &viewer.member_id, Role::Editor)
        .await
        .unwrap();
    assert_eq!(signer(3).await.role, Role::Editor);

    // A fourth member exceeds max_members.
    let invite = create_invite(&owner, Some(Role::Viewer), None, now_secs() + 60, LIMITS)
        .await
        .unwrap();
    assert_eq!(code(redeem(&invite, 4).await), Code::Quota);
    // The invite was not used up.
    remove_member(&owner, &viewer.member_id).await.unwrap();
    redeem(&invite, 4).await.unwrap();
}

#[tokio::test]
#[serial]
async fn members_see_only_themselves() {
    let owner = realm_with_owner().await;
    let editor = invite_member(&owner, Role::Editor, 2).await;
    create_invite(
        &editor,
        None,
        Some(&editor.member_id),
        now_secs() + 60,
        LIMITS,
    )
    .await
    .unwrap();
    create_invite(&owner, Some(Role::Viewer), None, now_secs() + 60, LIMITS)
        .await
        .unwrap();

    let all = list_members(REALM, Some(&owner)).await.unwrap();
    assert_eq!(all["members"].as_array().unwrap().len(), 2);
    assert_eq!(all["invites"].as_array().unwrap().len(), 2);
    assert_eq!(list_members(REALM, None).await.unwrap(), all);

    let own = list_members(REALM, Some(&editor)).await.unwrap();
    let members = own["members"].as_array().unwrap();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0]["member_id"], editor.member_id.as_str());
    assert_eq!(members[0]["clients"][0]["key_id"], editor.key_id.as_str());
    assert_eq!(own["invites"].as_array().unwrap().len(), 1);
}

#[tokio::test]
#[serial]
async fn revoking_a_key_disables_its_actors_and_its_invites() {
    let owner = realm_with_owner().await;
    let editor = invite_member(&owner, Role::Editor, 2).await;
    let second = create_invite(
        &editor,
        None,
        Some(&editor.member_id),
        now_secs() + 60,
        LIMITS,
    )
    .await
    .unwrap();
    let pending = create_invite(
        &editor,
        None,
        Some(&editor.member_id),
        now_secs() + 60,
        LIMITS,
    )
    .await
    .unwrap();
    redeem(&second, 5).await.unwrap();
    let actor = bind_actor(&editor, 42).await;
    assert!(!actor_disabled(&actor).await.unwrap());

    let mut changes = changes::subscribe_actors();
    // A member revokes another member's client: refused.
    let viewer = invite_member(&owner, Role::Viewer, 3).await;
    assert_eq!(
        code(revoke_client(&viewer, &editor.key_id).await),
        Code::Forbidden
    );
    revoke_client(&editor, &editor.key_id).await.unwrap();
    assert_eq!(changes.recv().await.unwrap(), actor);
    assert!(actor_disabled(&actor).await.unwrap());
    assert_eq!(disabled_actors().await.unwrap(), vec![actor.clone()]);
    // The invite the revoked key signed is gone too.
    assert_eq!(code(redeem(&pending, 6).await), Code::InviteRevoked);
    // The revoked key can no longer sign; the other client can.
    let revoked = client_key(REALM, &editor.key_id).await.unwrap().unwrap();
    assert_eq!(code(authenticate(revoked).await), Code::ClientRevoked);
    signer(5).await;
}

#[tokio::test]
#[serial]
async fn the_owner_keeps_a_client_and_transfers_before_leaving() {
    let owner = realm_with_owner().await;
    let editor = invite_member(&owner, Role::Editor, 2).await;
    assert_eq!(
        code(revoke_client(&owner, &owner.key_id).await),
        Code::OwnerLastClient
    );
    assert_eq!(code(leave(&owner).await), Code::OwnerMustTransfer);
    assert_eq!(
        code(remove_member(&owner, &owner.member_id).await),
        Code::OwnerMustTransfer
    );

    // A member without a client cannot become the owner.
    revoke_client(&editor, &editor.key_id).await.unwrap();
    assert_eq!(
        code(transfer_owner(&owner, &editor.member_id, Role::Viewer).await),
        Code::Conflict
    );
    let client = create_invite(
        &owner,
        None,
        Some(&editor.member_id),
        now_secs() + 60,
        LIMITS,
    )
    .await
    .unwrap();
    redeem(&client, 7).await.unwrap();
    transfer_owner(&owner, &editor.member_id, Role::Viewer)
        .await
        .unwrap();
    assert_eq!(signer(7).await.role, Role::Owner);
    assert_eq!(signer(1).await.role, Role::Viewer);
    // The old owner's next request sees its new role.
    assert_eq!(
        code(create_invite(&owner, Some(Role::Viewer), None, now_secs() + 60, LIMITS).await),
        Code::Forbidden
    );
    leave(&signer(1).await).await.unwrap();
    let gone = client_key(REALM, &owner.key_id).await.unwrap().unwrap();
    assert_eq!(code(authenticate(gone).await), Code::ClientRevoked);
}

#[tokio::test]
#[serial]
async fn an_owner_invite_recovers_the_owner() {
    let owner = realm_with_owner().await;
    let actor = bind_actor(&owner, 7).await;
    let stolen = create_invite(
        &owner,
        None,
        Some(&owner.member_id),
        now_secs() + 60,
        LIMITS,
    )
    .await
    .unwrap();
    let first = issue_owner_invite(REALM, BY_OPERATOR).await.unwrap();
    let recovery = issue_owner_invite(REALM, BY_OPERATOR).await.unwrap();
    // A newer owner invite replaces the unused older one.
    assert_eq!(code(redeem(&first, 8).await), Code::InviteRevoked);

    let joined = redeem(&recovery, 8).await.unwrap();
    assert_eq!(joined["member_id"], owner.member_id.as_str());
    assert_eq!(joined["role"], "owner");
    assert!(actor_disabled(&actor).await.unwrap());
    // The old key and what it signed are gone.
    let old = client_key(REALM, &owner.key_id).await.unwrap().unwrap();
    assert_eq!(code(authenticate(old).await), Code::ClientRevoked);
    assert_eq!(code(redeem(&stolen, 9).await), Code::InviteRevoked);
}

#[tokio::test]
#[serial]
async fn only_an_active_realm_takes_members() {
    setup_test_db().await.unwrap();
    Realm::create(REALM.to_string()).await.unwrap();
    let invite = issue_owner_invite(REALM, BY_OPERATOR).await.unwrap();
    Realm::set_enabled(REALM, false).await.unwrap();
    assert_eq!(code(redeem(&invite, 1).await), Code::RealmUnavailable);
    Realm::set_enabled(REALM, true).await.unwrap();
    redeem(&invite, 1).await.unwrap();

    // Retirement drops the invites; keys and members stay.
    let open = create_invite(
        &signer(1).await,
        Some(Role::Viewer),
        None,
        now_secs() + 60,
        LIMITS,
    )
    .await
    .unwrap();
    Realm::retire(REALM).await.unwrap();
    assert_eq!(code(redeem(&open, 2).await), Code::InviteUnknown);
    assert_eq!(
        code(issue_owner_invite(REALM, BY_OPERATOR).await),
        Code::Conflict
    );
    assert!(
        client_key(REALM, &signer(1).await.key_id)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
#[serial]
async fn concurrent_redemptions_of_one_invite_succeed_once() {
    let owner = realm_with_owner().await;
    let invite = create_invite(&owner, Some(Role::Viewer), None, now_secs() + 60, LIMITS)
        .await
        .unwrap();
    let mut tasks = tokio::task::JoinSet::new();
    for seed in 10..16 {
        let invite = invite.clone();
        tasks.spawn(async move { redeem(&invite, seed).await });
    }
    let results = tasks.join_all().await;
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert!(
        results
            .iter()
            .filter_map(|result| result.as_ref().err())
            .all(|error| error.code == Code::InviteUsed)
    );
}
