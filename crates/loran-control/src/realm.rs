use actrix_proto::RealmInfo;
use platform::realm::Realm;

/// Convert a Realm record into proto RealmInfo
pub fn realm_to_proto(realm: &Realm) -> RealmInfo {
    RealmInfo {
        realm: realm.name.clone(),
        enabled: realm.enabled,
        created_at: realm.created_at as i64,
        updated_at: realm.updated_at.map(|v| v as i64),
        expires_at: realm.expires_at.unwrap_or(0),
        status: realm.status.to_string(),
    }
}
