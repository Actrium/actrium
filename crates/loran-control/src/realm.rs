use actrix_proto::{RealmInfo, admin::v1::SecretRotationState};
use platform::realm::Realm;

/// Convert a Realm record into proto RealmInfo
pub fn realm_to_proto(realm: &Realm) -> RealmInfo {
    let secret_rotation_state = if !realm.secret_current.is_empty() {
        // An expired grace window is not an ongoing rotation: report the
        // previous secret only while it is still usable, so consumers (Admin
        // UI badge, gRPC clients) don't show a stale "rotating" state forever.
        let now = chrono::Utc::now().timestamp();
        let (previous_hash, previous_valid_until) = match &realm.secret_previous {
            Some((hash, valid_until)) if *valid_until as i64 >= now => {
                (Some(hash.clone()), Some(*valid_until as i64))
            }
            _ => (None, None),
        };
        Some(SecretRotationState {
            current_hash_preview: realm.secret_current.clone(),
            previous_hash_preview: previous_hash,
            previous_valid_until,
        })
    } else {
        None
    };

    RealmInfo {
        realm_id: realm.id,
        name: realm.name.clone(),
        enabled: realm.enabled,
        created_at: realm.created_at as i64,
        updated_at: realm.updated_at.map(|v| v as i64),
        expires_at: realm.expires_at.unwrap_or(0),
        status: realm.status.to_string(),
        secret_rotation_state,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use platform::realm::RealmStatus;

    fn realm_with_previous(valid_until: Option<u64>) -> Realm {
        Realm {
            id: 33554432,
            name: "test".to_string(),
            status: RealmStatus::Active,
            enabled: true,
            expires_at: None,
            created_at: 0,
            updated_at: None,
            secret_current: "cur".to_string(),
            secret_previous: valid_until.map(|v| ("prev".to_string(), v)),
        }
    }

    #[test]
    fn expired_grace_window_is_not_reported_as_rotation() {
        let now = chrono::Utc::now().timestamp() as u64;
        let info = realm_to_proto(&realm_with_previous(Some(now - 1)));
        let state = info.secret_rotation_state.expect("rotation state present");
        assert_eq!(state.previous_hash_preview, None);
        assert_eq!(state.previous_valid_until, None);
    }

    #[test]
    fn active_grace_window_is_reported() {
        let now = chrono::Utc::now().timestamp() as u64;
        let info = realm_to_proto(&realm_with_previous(Some(now + 3600)));
        let state = info.secret_rotation_state.expect("rotation state present");
        assert_eq!(state.previous_hash_preview.as_deref(), Some("prev"));
        assert_eq!(state.previous_valid_until, Some((now + 3600) as i64));
    }
}
