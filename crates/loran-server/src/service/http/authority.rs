//! Public Actr authority bootstrap for the Loran service composition.

use actr_protocol::{
    AUTHORITY_DESCRIPTOR_VERSION, AuthorityDescriptor, ICE_CONFIG_PROTOCOL, IceConfiguration,
    IceServer, ServiceLink, parse_authority_member,
};
use axum::{Json, Router, extract::State, http::header, response::IntoResponse, routing::get};
use loran_packages::PackageDistributor;
use platform::config::ActrixConfig;
use std::{path::PathBuf, sync::Arc};
use url::Url;

#[derive(Debug, Clone)]
struct AuthorityState {
    descriptor: AuthorityDescriptor,
    ice: IceConfiguration,
}

fn authority_composition(
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
    if config.is_issuer_enabled() {
        services.push(ServiceLink::new("identity", format!("{origin}/issuer")));
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

    Ok((
        Some(AuthorityState {
            descriptor: AuthorityDescriptor {
                version: AUTHORITY_DESCRIPTOR_VERSION,
                authority: parsed_authority.authority_uri(),
                services,
                links: None,
                realm_signing_keys: None,
                publisher: None,
            },
            ice: IceConfiguration {
                version: 1,
                ice_servers,
            },
        }),
        packages,
    ))
}

async fn get_descriptor(State(state): State<Arc<AuthorityState>>) -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "public, max-age=300")],
        Json(state.descriptor.clone()),
    )
}

async fn get_ice_config(State(state): State<Arc<AuthorityState>>) -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "private, max-age=60")],
        Json(state.ice.clone()),
    )
}

pub fn build_authority_router(config: &ActrixConfig) -> anyhow::Result<Router> {
    let (state, packages) = authority_composition(config)?;
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
        router = router.merge(
            Router::new()
                .route("/.well-known/actr", get(get_descriptor))
                .route("/actr/ice-config", get(get_ice_config))
                .with_state(Arc::new(state)),
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
        };
        config.services.packages = Some(PackagesConfig {
            artifacts: vec![artifact.display().to_string()],
        });
        config
    }

    /// Request path of the single artifact hosted by `config`.
    fn artifact_uri(config: &ActrixConfig) -> String {
        let (_, packages) = authority_composition(config).unwrap();
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

        let (state, packages) = authority_composition(&config).unwrap();
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

    #[test]
    fn authority_uri_names_a_domain_without_port() {
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
            let error = authority_composition(&config).unwrap_err().to_string();
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
        let app = build_authority_router(&config).unwrap();

        let response = app
            .clone()
            .oneshot(
                Request::get(artifact_uri(&config))
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

    #[test]
    fn rejects_plain_http_authority_in_production() {
        let config = ActrixConfig {
            env: "prod".to_string(),
            authority: AuthorityConfig {
                enabled: true,
                uri: Some("actr://localhost".to_string()),
            },
            ..ActrixConfig::default()
        };
        let error = authority_composition(&config).unwrap_err();
        assert!(error.to_string().contains("requires TLS"));
    }

    #[tokio::test]
    async fn public_http_routes_form_a_complete_bootstrap_chain() {
        let directory = tempfile::tempdir().unwrap();
        let artifact = directory.path().join("echo.actr");
        let artifact_bytes = b"complete actor package";
        std::fs::write(&artifact, artifact_bytes).unwrap();
        let config = test_config(&artifact);
        let app = build_authority_router(&config).unwrap();

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
                Request::get(artifact_uri(&config))
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
}
