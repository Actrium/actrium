//! Public, non-secret description of the single Realm hosted by this Actrix.

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use platform::{config::ActrixConfig, realm::Realm};
use serde::Serialize;
use std::sync::Arc;

pub const REALM_DESCRIPTOR_PATH: &str = "/.well-known/actr/realm.json";

#[derive(Clone)]
struct DescriptorState {
    config: ActrixConfig,
}

#[derive(Serialize)]
struct RealmDescriptor {
    schema: &'static str,
    protocol_version: u32,
    realm: RealmIdentity,
    authority: AuthorityDescriptor,
    providers: Vec<ProviderDescriptor>,
    publisher_registry: String,
}

#[derive(Serialize)]
struct RealmIdentity {
    id: u32,
    name: String,
}

#[derive(Serialize)]
struct AuthorityDescriptor {
    issuer: String,
    audience: &'static str,
    registration: String,
    signing_keys: String,
}

#[derive(Serialize)]
struct ProviderDescriptor {
    id: &'static str,
    credential_audiences: [&'static str; 2],
    #[serde(skip_serializing_if = "Option::is_none")]
    signaling: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stun: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    turn: Option<String>,
}

fn advertised_http_origin(config: &ActrixConfig) -> Option<String> {
    let http = config.bind.http.as_ref()?;
    let port = http.effective_advertised_port();
    let default_port = (http.is_tls() && port == 443) || (!http.is_tls() && port == 80);
    Some(if default_port {
        format!("{}://{}", http.scheme(), http.domain_name)
    } else {
        format!("{}://{}:{}", http.scheme(), http.domain_name, port)
    })
}

fn advertised_signaling_endpoint(config: &ActrixConfig) -> Option<String> {
    let http = config.bind.http.as_ref()?;
    let port = http.effective_advertised_port();
    let default_port = (http.is_tls() && port == 443) || (!http.is_tls() && port == 80);
    Some(if default_port {
        format!("{}://{}/signaling/ws", http.ws_scheme(), http.domain_name)
    } else {
        format!(
            "{}://{}:{}/signaling/ws",
            http.ws_scheme(),
            http.domain_name,
            port
        )
    })
}

async fn realm_descriptor(State(state): State<Arc<DescriptorState>>) -> Response {
    let realms = match Realm::get_all().await {
        Ok(realms) => realms,
        Err(error) => {
            platform::recording::error!("Failed to load Realm descriptor: {}", error);
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "Realm state unavailable"})),
            )
                .into_response();
        }
    };
    let [realm] = realms.as_slice() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "Actrix must be provisioned with exactly one Realm"
            })),
        )
            .into_response();
    };
    if !realm.is_active() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "Realm is not active"})),
        )
            .into_response();
    }

    let Some(origin) = advertised_http_origin(&state.config) else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "HTTP origin is not configured"})),
        )
            .into_response();
    };
    let ice = &state.config.bind.ice;
    let provider = ProviderDescriptor {
        id: "actrix-official",
        credential_audiences: ["actr.signaling", "actr.turn"],
        signaling: state
            .config
            .is_signaling_enabled()
            .then(|| advertised_signaling_endpoint(&state.config))
            .flatten(),
        stun: state
            .config
            .is_stun_enabled()
            .then(|| format!("stun:{}:{}", ice.advertised_ip, ice.advertised_port)),
        turn: state.config.is_turn_enabled().then(|| {
            format!(
                "turn:{}:{}?transport=udp",
                ice.advertised_ip, ice.advertised_port
            )
        }),
    };

    Json(RealmDescriptor {
        schema: "actr.realm/v1",
        protocol_version: 1,
        realm: RealmIdentity {
            id: realm.id,
            name: realm.name.clone(),
        },
        authority: AuthorityDescriptor {
            issuer: format!("actr:realm:{}:authority", realm.id),
            audience: "actr.signaling",
            registration: format!("{origin}/issuer/register"),
            signing_keys: format!("{origin}/issuer/signing-pubkey"),
        },
        providers: vec![provider],
        publisher_registry: format!("{origin}/registry"),
    })
    .into_response()
}

pub fn build_realm_descriptor_router(config: ActrixConfig) -> Router {
    Router::new()
        .route(REALM_DESCRIPTOR_PATH, get(realm_descriptor))
        .with_state(Arc::new(DescriptorState { config }))
}

#[cfg(test)]
mod tests {
    use super::advertised_http_origin;
    use platform::config::ActrixConfig;

    #[test]
    fn advertised_origin_preserves_non_default_port() {
        let mut config = ActrixConfig::default();
        let http = config.bind.http.as_mut().unwrap();
        http.domain_name = "realm.example.com".to_string();
        http.port = 8443;
        http.advertised_port = Some(9443);
        assert_eq!(
            advertised_http_origin(&config).unwrap(),
            "http://realm.example.com:9443"
        );
    }
}
