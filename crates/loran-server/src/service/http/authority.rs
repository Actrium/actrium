//! Public Actr authority bootstrap for the Loran service composition.

use actr_protocol::realm_profile::{REALM_PROFILE_PROTOCOL, jcs};
use actr_protocol::{
    AUTHORITY_DESCRIPTOR_VERSION, AuthorityDescriptor, DetachedSignature, ICE_CONFIG_PROTOCOL,
    IceConfiguration, IceServer, KeyStatus, RealmSigningKey, RealmSigningKeys, SIGNATURE_ALGORITHM,
    ServiceLink, parse_authority_member,
};
use axum::{
    Json, Router,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use issuer::signer_client_wrapper::{SignerClientWrapper, create_signer_client};
use loran_packages::PackageDistributor;
use platform::config::ActrixConfig;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::OnceCell;
use url::Url;

const REALM_MANAGEMENT_PROTOCOL: &str = "actr-realm-management/1";

struct AuthorityState {
    descriptor: AuthorityDescriptor,
    ice: IceConfiguration,
    /// Present when this Loran is the Realm Provider for its own domain.
    realm_keys: Option<RealmKeys>,
}

/// The provider domain's `realm_signing_keys` member, signed once by the
/// Signer key `realm-signing:<domain>` (generated on first use).
struct RealmKeys {
    domain: String,
    signer: SignerClientWrapper,
    member: OnceCell<RealmSigningKeys>,
}

impl RealmKeys {
    async fn member(&self) -> anyhow::Result<&RealmSigningKeys> {
        self.member
            .get_or_try_init(|| async {
                let key_id = realm_signing_key_id(&self.domain);
                let (signer_key_id, public_key) = self.signer.ensure_named_key(&key_id).await?;
                let (mut member, payload) = unsigned_realm_signing_keys(&key_id, &public_key);
                let signature = self.signer.sign(signer_key_id, payload.as_bytes()).await?;
                member.signature.value = BASE64.encode(signature);
                Ok(member)
            })
            .await
    }
}

/// Signer key name, and descriptor key id, of a domain's Realm signing key.
fn realm_signing_key_id(domain: &str) -> String {
    format!("realm-signing:{domain}")
}

/// A one-key `realm_signing_keys` member with an empty signature value, and
/// the RFC 8785 JCS payload that the key must sign (the member without
/// `signature`).
fn unsigned_realm_signing_keys(key_id: &str, public_key: &[u8; 32]) -> (RealmSigningKeys, String) {
    let member = RealmSigningKeys {
        sequence: 1,
        keys: vec![RealmSigningKey {
            id: key_id.to_string(),
            algorithm: SIGNATURE_ALGORITHM.to_string(),
            public_key: BASE64.encode(public_key),
            status: KeyStatus::Active,
        }],
        rotations: None,
        signature: DetachedSignature {
            key_id: key_id.to_string(),
            algorithm: SIGNATURE_ALGORITHM.to_string(),
            value: String::new(),
        },
    };
    let mut payload = serde_json::to_value(&member).expect("member serializes");
    if let Some(members) = payload.as_object_mut() {
        members.remove("signature");
    }
    (member, jcs(&payload))
}

async fn authority_composition(
    config: &ActrixConfig,
) -> anyhow::Result<(Option<AuthorityState>, Option<PackageDistributor>)> {
    let http = config
        .bind
        .http
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("bind.http is required"))?;
    let port = http.effective_advertised_port();
    let default_port = (http.is_tls() && port == 443) || (!http.is_tls() && port == 80);
    let origin = if default_port {
        format!("{}://{}", http.scheme(), http.domain_name)
    } else {
        format!("{}://{}:{port}", http.scheme(), http.domain_name)
    };
    Url::parse(&origin).map_err(|error| anyhow::anyhow!("invalid advertised origin: {error}"))?;

    let signaling_origin = if default_port {
        format!("{}://{}", http.ws_scheme(), http.domain_name)
    } else {
        format!("{}://{}:{port}", http.ws_scheme(), http.domain_name)
    };
    let signaling_href = format!("{signaling_origin}/signaling/ws");

    let packages = config
        .services
        .packages
        .as_ref()
        .map(|packages| {
            let endpoint = Url::parse(&format!("{origin}/actr/packages"))?;
            let artifacts = packages.artifacts.iter().map(PathBuf::from);
            PackageDistributor::new(endpoint, artifacts).map_err(anyhow::Error::from)
        })
        .transpose()?;

    if !config.authority.enabled {
        return Ok((None, packages));
    }
    if config.env == "prod" && !http.is_tls() {
        anyhow::bail!("production authority discovery requires TLS");
    }
    let authority = config.authority.uri.clone().ok_or_else(|| {
        anyhow::anyhow!("authority.uri is required when authority.enabled is true")
    })?;
    let parsed_authority = parse_authority_member(&authority)
        .map_err(|error| anyhow::anyhow!("authority.uri is not a valid authority URI: {error}"))?;
    // The authority names a domain only. Its discovery document is fetched on
    // the default HTTPS port; an explicit bind.http port only appears in the
    // advertised service endpoints.
    let expected_authority =
        actr_protocol::uri::ActrUri::from_domain(&http.domain_name).map_err(|error| {
            anyhow::anyhow!("bind.http does not form a valid Actr authority: {error}")
        })?;
    if expected_authority.authority_uri() != parsed_authority.authority_uri() {
        anyhow::bail!(
            "authority.uri '{}' does not match advertised HTTP domain '{}'",
            parsed_authority.authority_uri(),
            expected_authority.authority_uri()
        );
    }

    let mut services = Vec::new();
    if config.is_signaling_enabled() {
        services
            .push(ServiceLink::new("signaling", &signaling_href).with_protocol("actr-signaling/1"));
        services
            .push(ServiceLink::new("discovery", &signaling_href).with_protocol("actr-discovery/1"));
    }
    // The Issuer makes this Loran the Realm Provider for its own domain.
    let realm_signer = if config.is_issuer_enabled() {
        config
            .services
            .issuer
            .as_ref()
            .and_then(|issuer| issuer.get_signer_client_config(config))
    } else {
        None
    };
    if config.is_issuer_enabled() {
        services.push(ServiceLink::new("identity", format!("{origin}/issuer")));
    }
    if realm_signer.is_some() {
        let domain = parsed_authority.domain();
        services.push(
            ServiceLink::new("realms", format!("{origin}/actr/realms/{domain}"))
                .with_protocol(REALM_PROFILE_PROTOCOL),
        );
        services.push(
            ServiceLink::new(
                "realm-management",
                format!("{origin}/actr/realm-management"),
            )
            .with_protocol(REALM_MANAGEMENT_PROTOCOL),
        );
    }
    if config.is_ice_enabled() {
        services.push(
            ServiceLink::new("ice-config", format!("{origin}/actr/ice-config"))
                .with_protocol(ICE_CONFIG_PROTOCOL),
        );
    }
    if services.is_empty() {
        anyhow::bail!("an enabled authority must advertise at least one service");
    }

    let mut ice_servers = Vec::new();
    if config.is_ice_enabled() {
        let ice = &config.bind.ice;
        let address = if ice.advertised_ip.contains(':') {
            format!("[{}]", ice.advertised_ip)
        } else {
            ice.advertised_ip.clone()
        };
        if config.is_stun_enabled() {
            ice_servers.push(IceServer {
                urls: vec![format!("stun:{address}:{}", ice.advertised_port)],
                username: None,
                credential: None,
            });
        }
        if config.is_turn_enabled() {
            ice_servers.push(IceServer {
                urls: vec![format!("turn:{address}:{}", ice.advertised_port)],
                username: None,
                credential: None,
            });
        }
    }

    let links = &config.authority.links;
    let descriptor = AuthorityDescriptor {
        version: AUTHORITY_DESCRIPTOR_VERSION,
        authority: parsed_authority.authority_uri(),
        services,
        links: (!links.is_empty()).then(|| links.clone()),
        realm_signing_keys: None,
        publisher: None,
    };
    descriptor
        .validate_for(&parsed_authority)
        .map_err(|error| anyhow::anyhow!("invalid authority descriptor: {error}"))?;

    let realm_keys = match realm_signer {
        Some(signer_config) => Some(RealmKeys {
            domain: parsed_authority.domain().to_string(),
            signer: create_signer_client(&signer_config, &config.actrix_shared_key)
                .await
                .map_err(|error| anyhow::anyhow!("Signer client: {error}"))?,
            member: OnceCell::new(),
        }),
        None => None,
    };

    Ok((
        Some(AuthorityState {
            descriptor,
            ice: IceConfiguration {
                version: 1,
                ice_servers,
            },
            realm_keys,
        }),
        packages,
    ))
}

async fn get_descriptor(State(state): State<Arc<AuthorityState>>) -> Response {
    let mut descriptor = state.descriptor.clone();
    if let Some(realm_keys) = &state.realm_keys {
        // Only the startup task talks to the Signer; a public request never does.
        match realm_keys.member.get() {
            Some(member) => descriptor.realm_signing_keys = Some(member.clone()),
            None => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
        }
    }
    (
        [
            (header::CACHE_CONTROL, "public, max-age=300"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        ],
        Json(descriptor),
    )
        .into_response()
}

async fn get_ice_config(State(state): State<Arc<AuthorityState>>) -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "private, max-age=60")],
        Json(state.ice.clone()),
    )
}

pub async fn build_authority_router(
    config: &ActrixConfig,
    cancel: tokio_util::sync::CancellationToken,
) -> anyhow::Result<Router> {
    let (state, packages) = authority_composition(config).await?;
    let mut router = Router::new();
    if let Some(packages) = packages {
        platform::recording::info!("Adding Actr package hosting routes");
        for href in packages.artifact_hrefs() {
            platform::recording::info!("Hosting Actr package {href}");
        }
        router = router.nest("/actr/packages", packages.router());
    }
    if let Some(state) = state {
        platform::recording::info!(
            "Adding Actr authority discovery routes for {}",
            state.descriptor.authority
        );
        let state = Arc::new(state);
        if state.realm_keys.is_some() {
            // Generate the Realm signing key at startup; the Signer listener
            // may come up after this router, so retry until it answers.
            let state = state.clone();
            tokio::spawn(async move {
                let realm_keys = state.realm_keys.as_ref().expect("checked above");
                while let Err(error) = realm_keys.member().await {
                    platform::recording::warn!("Realm signing key not ready: {error:#}");
                    tokio::select! {
                        _ = cancel.cancelled() => return,
                        _ = tokio::time::sleep(Duration::from_secs(2)) => {}
                    }
                }
                platform::recording::info!(
                    "Realm signing key ready: {}",
                    realm_signing_key_id(&realm_keys.domain)
                );
            });
        }
        router = router.merge(
            Router::new()
                .route("/.well-known/actr", get(get_descriptor))
                .route("/actr/ice-config", get(get_ice_config))
                .with_state(state),
        );
    }
    Ok(router)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, body::to_bytes, http::Request, http::StatusCode};
    use platform::config::{PackagesConfig, authority::AuthorityConfig};
    use tower::ServiceExt;

    fn test_config(artifact: &std::path::Path) -> ActrixConfig {
        let mut config = ActrixConfig::default();
        config.enable = platform::config::ENABLE_SIGNALING
            | platform::config::ENABLE_STUN
            | platform::config::ENABLE_TURN
            | platform::config::ENABLE_ISSUER;
        config.bind.http.as_mut().unwrap().port = 8081;
        config.bind.http.as_mut().unwrap().advertised_port = Some(8081);
        config.bind.ice.advertised_ip = "127.0.0.1".to_string();
        config.bind.ice.advertised_port = 13478;
        config.authority = AuthorityConfig {
            enabled: true,
            uri: Some("actr://localhost".to_string()),
            ..AuthorityConfig::default()
        };
        config.services.packages = Some(PackagesConfig {
            artifacts: vec![artifact.display().to_string()],
        });
        config
    }

    /// `test_config` acting as Realm Provider: the Issuer has a Signer.
    fn provider_config(artifact: &std::path::Path) -> ActrixConfig {
        let mut config = test_config(artifact);
        config.services.issuer = Some(platform::config::IssuerServiceConfig {
            server: Default::default(),
            dependencies: platform::config::issuer::IssuerDependencies {
                signer: Some(Default::default()),
            },
        });
        config
    }

    /// Request path of the single artifact hosted by `config`.
    async fn artifact_uri(config: &ActrixConfig) -> String {
        let (_, packages) = authority_composition(config).await.unwrap();
        let packages = packages.unwrap();
        let href = packages.artifact_hrefs().next().unwrap();
        href[url::Position::BeforePath..].to_string()
    }

    #[tokio::test]
    async fn descriptor_does_not_advertise_package_hosting() {
        let directory = tempfile::tempdir().unwrap();
        let artifact = directory.path().join("echo.actr");
        std::fs::write(&artifact, b"actor package").unwrap();
        let config = test_config(&artifact);

        let (state, packages) = authority_composition(&config).await.unwrap();
        let state = state.unwrap();
        let packages = packages.unwrap();
        assert_eq!(state.descriptor.authority, "actr://localhost");
        assert!(state.descriptor.service("packages").is_none());
        assert_eq!(
            state.descriptor.require_service("signaling").unwrap().href,
            "ws://localhost:8081/signaling/ws"
        );
        assert_eq!(state.ice.ice_servers.len(), 2);

        let href = packages.artifact_hrefs().next().unwrap();
        assert!(
            href.as_str()
                .starts_with("http://localhost:8081/actr/packages/artifact?sha256="),
            "{href}"
        );
    }

    #[tokio::test]
    async fn authority_uri_names_a_domain_without_port() {
        let directory = tempfile::tempdir().unwrap();
        let artifact = directory.path().join("echo.actr");
        std::fs::write(&artifact, b"actor package").unwrap();

        for (uri, expected) in [
            ("actr://localhost:8081", "not a valid authority URI"),
            ("actr://localhost/", "not a valid authority URI"),
            ("actr://example.com", "does not match"),
        ] {
            let mut config = test_config(&artifact);
            config.authority.uri = Some(uri.to_string());
            let error = authority_composition(&config)
                .await
                .err()
                .unwrap()
                .to_string();
            assert!(error.contains(expected), "{uri}: {error}");
        }
    }

    #[tokio::test]
    async fn package_hosting_can_run_without_local_authority_discovery() {
        let directory = tempfile::tempdir().unwrap();
        let artifact = directory.path().join("echo.actr");
        std::fs::write(&artifact, b"actor package").unwrap();
        let mut config = test_config(&artifact);
        config.authority.enabled = false;
        config.authority.uri = None;
        let app = build_authority_router(&config, Default::default())
            .await
            .unwrap();

        let response = app
            .clone()
            .oneshot(
                Request::get(artifact_uri(&config).await)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .oneshot(
                Request::get("/.well-known/actr")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn rejects_plain_http_authority_in_production() {
        let config = ActrixConfig {
            env: "prod".to_string(),
            authority: AuthorityConfig {
                enabled: true,
                uri: Some("actr://localhost".to_string()),
                ..AuthorityConfig::default()
            },
            ..ActrixConfig::default()
        };
        let error = authority_composition(&config).await.err().unwrap();
        assert!(error.to_string().contains("requires TLS"));
    }

    #[tokio::test]
    async fn public_http_routes_form_a_complete_bootstrap_chain() {
        let directory = tempfile::tempdir().unwrap();
        let artifact = directory.path().join("echo.actr");
        let artifact_bytes = b"complete actor package";
        std::fs::write(&artifact, artifact_bytes).unwrap();
        let config = test_config(&artifact);
        let app = build_authority_router(&config, Default::default())
            .await
            .unwrap();

        let response = app
            .clone()
            .oneshot(
                Request::get("/.well-known/actr")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let descriptor: AuthorityDescriptor = serde_json::from_slice(&body).unwrap();
        let uri = "actr://localhost/echo"
            .parse::<actr_protocol::uri::ActrUri>()
            .unwrap();
        descriptor.validate_for(&uri).unwrap();

        let response = app
            .clone()
            .oneshot(
                Request::get(artifact_uri(&config).await)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        assert_eq!(body.as_ref(), artifact_bytes);

        let response = app
            .clone()
            .oneshot(
                Request::get(format!("/actr/packages/artifact?sha256={}", "0".repeat(64)))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = app
            .oneshot(
                Request::get("/actr/ice-config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let ice: IceConfiguration = serde_json::from_slice(&body).unwrap();
        assert_eq!(ice.ice_servers[0].urls, ["stun:127.0.0.1:13478"]);
    }

    #[tokio::test]
    async fn realm_provider_publishes_realm_relations_and_links() {
        let directory = tempfile::tempdir().unwrap();
        let artifact = directory.path().join("echo.actr");
        std::fs::write(&artifact, b"actor package").unwrap();
        let mut config = provider_config(&artifact);
        config.authority.links = vec!["/i/".to_string()];

        let (state, _) = authority_composition(&config).await.unwrap();
        let state = state.unwrap();
        let realms = state.descriptor.require_service("realms").unwrap();
        assert_eq!(realms.href, "http://localhost:8081/actr/realms/localhost");
        assert_eq!(realms.protocol.as_deref(), Some("actr-realm-profile/1"));
        let management = state
            .descriptor
            .require_service("realm-management")
            .unwrap();
        assert_eq!(
            management.href,
            "http://localhost:8081/actr/realm-management"
        );
        assert_eq!(
            management.protocol.as_deref(),
            Some("actr-realm-management/1")
        );
        assert_eq!(state.descriptor.links, Some(vec!["/i/".to_string()]));
        assert_eq!(state.realm_keys.unwrap().domain, "localhost");

        // Without a Signer this Loran is not a Realm Provider.
        let (state, _) = authority_composition(&test_config(&artifact))
            .await
            .unwrap();
        let state = state.unwrap();
        assert!(state.descriptor.service("realms").is_none());
        assert!(state.descriptor.service("realm-management").is_none());
        assert!(state.realm_keys.is_none());

        config.authority.links = vec!["/".to_string()];
        let error = authority_composition(&config).await.err().unwrap();
        assert!(error.to_string().contains("Realm link prefix"), "{error}");
    }

    #[test]
    fn realm_signing_keys_member_is_self_signed_over_jcs() {
        use ed25519_dalek::{Signer as _, Verifier as _};

        let key = ed25519_dalek::SigningKey::from_bytes(&[3u8; 32]);
        let key_id = realm_signing_key_id("provider.example");
        assert_eq!(key_id, "realm-signing:provider.example");
        let (mut member, payload) =
            unsigned_realm_signing_keys(&key_id, key.verifying_key().as_bytes());
        member.signature.value = BASE64.encode(key.sign(payload.as_bytes()).to_bytes());

        let descriptor = AuthorityDescriptor {
            version: AUTHORITY_DESCRIPTOR_VERSION,
            authority: "actr://provider.example".to_string(),
            services: vec![ServiceLink::new(
                "identity",
                "https://provider.example/issuer",
            )],
            links: None,
            realm_signing_keys: Some(member.clone()),
            publisher: None,
        };
        let uri = "actr://provider.example".parse().unwrap();
        descriptor.validate_for(&uri).unwrap();

        // The signature covers the member as served, minus `signature`.
        let mut served = serde_json::to_value(&descriptor).unwrap()["realm_signing_keys"].clone();
        served.as_object_mut().unwrap().remove("signature");
        assert_eq!(jcs(&served), payload);
        let signature: [u8; 64] = BASE64
            .decode(&member.signature.value)
            .unwrap()
            .try_into()
            .unwrap();
        key.verifying_key()
            .verify(
                payload.as_bytes(),
                &ed25519_dalek::Signature::from_bytes(&signature),
            )
            .unwrap();
    }

    #[tokio::test]
    async fn descriptor_allows_any_origin_and_head() {
        let directory = tempfile::tempdir().unwrap();
        let artifact = directory.path().join("echo.actr");
        std::fs::write(&artifact, b"actor package").unwrap();
        let app = build_authority_router(&test_config(&artifact), Default::default())
            .await
            .unwrap();

        for method in ["GET", "HEAD"] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri("/.well-known/actr")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{method}");
            let headers = response.headers();
            assert_eq!(headers[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");
            assert_eq!(headers[header::CONTENT_TYPE], "application/json");
            assert!(headers.contains_key(header::CACHE_CONTROL));
            let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
            assert_eq!(body.is_empty(), method == "HEAD", "{method}");
        }
    }

    #[tokio::test]
    async fn descriptor_is_unavailable_until_the_realm_signing_key_is() {
        let directory = tempfile::tempdir().unwrap();
        let artifact = directory.path().join("echo.actr");
        std::fs::write(&artifact, b"actor package").unwrap();
        let mut config = provider_config(&artifact);
        // Nothing listens here.
        config
            .services
            .issuer
            .as_mut()
            .unwrap()
            .dependencies
            .signer
            .as_mut()
            .unwrap()
            .endpoint = "http://127.0.0.1:1".to_string();
        let cancel = tokio_util::sync::CancellationToken::new();
        let app = build_authority_router(&config, cancel.clone())
            .await
            .unwrap();

        let response = app
            .oneshot(
                Request::get("/.well-known/actr")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        cancel.cancel();
    }
}
