//! Members for unit tests.

use actr_protocol::realm_membership::{RealmInvite, Role, client_key_id, invite_secret_hash};
use ed25519_dalek::SigningKey;
use platform::realm::membership::{self, BY_OPERATOR, IssuedInvite, Limits, Signer};

/// The owner of `realm`, an active Realm, holding the key of `seed`.
pub(crate) async fn owner_of(realm: &str, seed: u8) -> Signer {
    let invite = membership::issue_owner_invite(realm, BY_OPERATOR)
        .await
        .unwrap();
    redeem(realm, &invite, seed).await
}

/// A new member of `role`, invited by `owner`, holding the key of `seed`.
pub(crate) async fn member_of(owner: &Signer, role: Role, seed: u8) -> Signer {
    let limits = Limits::from(&platform::config::RealmConfig::default());
    let expires_at = chrono::Utc::now().timestamp() + 3600;
    let invite = membership::create_invite(owner, Some(role), None, expires_at, limits)
        .await
        .unwrap();
    redeem(&owner.realm, &invite, seed).await
}

async fn redeem(realm: &str, invite: &IssuedInvite, seed: u8) -> Signer {
    let secret = RealmInvite::decode(&invite.invite).unwrap().secret;
    let public_key = SigningKey::from_bytes(&[seed; 32])
        .verifying_key()
        .to_bytes();
    let limits = Limits::from(&platform::config::RealmConfig::default());
    membership::join(
        realm,
        &invite_secret_hash(&secret).unwrap(),
        &public_key,
        "",
        limits,
    )
    .await
    .unwrap();
    let key = membership::client_key(realm, &client_key_id(&public_key))
        .await
        .unwrap()
        .unwrap();
    membership::authenticate(key).await.unwrap()
}
