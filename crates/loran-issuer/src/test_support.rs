//! Members for unit tests.

use actr_protocol::realm_membership::{RealmInvite, client_key_id, invite_secret_hash};
use ed25519_dalek::SigningKey;
use platform::realm::membership::{self, BY_OPERATOR, Limits, Signer};

/// The owner of `realm`, an active Realm, holding the key of `seed`.
pub(crate) async fn owner_of(realm: &str, seed: u8) -> Signer {
    let invite = membership::issue_owner_invite(realm, BY_OPERATOR)
        .await
        .unwrap();
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
