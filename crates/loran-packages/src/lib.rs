//! Package Target resolution and `.actr` artifact distribution for Loran.
//!
//! This component implements the `actr-packages/1` role. Authority discovery
//! advertises its endpoint, but does not own its storage or resolution logic.

use actr_protocol::{PackageResolution, ResolvedPackage};
use axum::{
    Json, Router,
    body::Body,
    extract::{Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use thiserror::Error;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageTarget {
    pub target: String,
    pub artifact: PathBuf,
    pub profile: Option<String>,
}

#[derive(Error, Debug)]
pub enum PackageDistributorError {
    #[error("package endpoint must be an absolute HTTP(S) URL: {0}")]
    InvalidEndpoint(String),
    #[error("invalid package target '{0}'")]
    InvalidTarget(String),
    #[error("duplicate package target '{0}'")]
    DuplicateTarget(String),
    #[error(
        "package target '{target}' does not reference a non-empty, readable .actr file by absolute path: {artifact}"
    )]
    InvalidArtifact { target: String, artifact: String },
}

#[derive(Debug, Clone)]
pub struct PackageDistributor {
    endpoint: Url,
    artifact_endpoint: Url,
    targets: Arc<HashMap<String, PackageTarget>>,
}

#[derive(Debug)]
enum PackageHttpError {
    NotFound,
    Unavailable(String),
}

impl IntoResponse for PackageHttpError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                "target-not-found",
                "target or artifact not found".to_string(),
            ),
            Self::Unavailable(message) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "artifact-unavailable",
                message,
            ),
        };
        (
            status,
            Json(serde_json::json!({ "error": code, "message": message })),
        )
            .into_response()
    }
}

#[derive(Debug, Deserialize)]
struct TargetQuery {
    target: String,
}

#[derive(Debug, Deserialize)]
struct ArtifactQuery {
    target: String,
    sha256: String,
}

impl PackageDistributor {
    pub fn new(
        endpoint: Url,
        targets: impl IntoIterator<Item = PackageTarget>,
    ) -> Result<Self, PackageDistributorError> {
        if !matches!(endpoint.scheme(), "http" | "https")
            || endpoint.cannot_be_a_base()
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
        {
            return Err(PackageDistributorError::InvalidEndpoint(
                endpoint.to_string(),
            ));
        }
        let mut artifact_endpoint = endpoint.clone();
        artifact_endpoint.set_path(&format!(
            "{}/artifact",
            endpoint.path().trim_end_matches('/')
        ));

        let mut indexed = HashMap::new();
        for target in targets {
            let target_name = target.target.clone();
            let valid_target = !target.target.is_empty()
                && actr_protocol::uri::ActrUri::new("package-target.invalid", Some(&target.target))
                    .is_ok();
            if !valid_target {
                return Err(PackageDistributorError::InvalidTarget(target.target));
            }
            let valid_artifact = target
                .artifact
                .metadata()
                .is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0);
            if target.artifact.extension().and_then(|value| value.to_str()) != Some("actr")
                || !target.artifact.is_absolute()
                || !valid_artifact
            {
                return Err(PackageDistributorError::InvalidArtifact {
                    target: target.target,
                    artifact: target.artifact.display().to_string(),
                });
            }
            if indexed.insert(target.target.clone(), target).is_some() {
                return Err(PackageDistributorError::DuplicateTarget(target_name));
            }
        }

        Ok(Self {
            endpoint,
            artifact_endpoint,
            targets: Arc::new(indexed),
        })
    }

    pub fn endpoint(&self) -> &Url {
        &self.endpoint
    }

    pub fn target_count(&self) -> usize {
        self.targets.len()
    }

    pub async fn resolve(&self, target: &str) -> Result<PackageResolution, StatusCode> {
        self.resolve_inner(target)
            .await
            .map_err(|error| match error {
                PackageHttpError::NotFound => StatusCode::NOT_FOUND,
                PackageHttpError::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            })
    }

    async fn resolve_inner(&self, target: &str) -> Result<PackageResolution, PackageHttpError> {
        let configured = self.targets.get(target).ok_or(PackageHttpError::NotFound)?;
        let bytes = tokio::fs::read(&configured.artifact)
            .await
            .map_err(|error| PackageHttpError::Unavailable(error.to_string()))?;
        let sha256 = hex::encode(Sha256::digest(&bytes));
        let mut href = self.artifact_endpoint.clone();
        href.query_pairs_mut()
            .append_pair("target", target)
            .append_pair("sha256", &sha256);

        Ok(PackageResolution {
            version: 1,
            target: target.to_string(),
            package: ResolvedPackage {
                href: href.into(),
                media_type: "application/zip".to_string(),
                sha256,
                size: bytes.len() as u64,
                profile: configured.profile.clone(),
            },
        })
    }

    pub fn router(self) -> Router {
        let state = Arc::new(self);
        Router::new()
            .route("/resolve", get(resolve_package))
            .route("/artifact", get(get_artifact))
            .with_state(state)
    }
}

async fn resolve_package(
    State(state): State<Arc<PackageDistributor>>,
    Query(query): Query<TargetQuery>,
) -> Result<impl IntoResponse, PackageHttpError> {
    let resolution = state.resolve_inner(&query.target).await?;
    Ok((
        [(header::CACHE_CONTROL, "public, max-age=60")],
        Json(resolution),
    ))
}

async fn get_artifact(
    State(state): State<Arc<PackageDistributor>>,
    Query(query): Query<ArtifactQuery>,
) -> Result<Response, PackageHttpError> {
    let configured = state
        .targets
        .get(&query.target)
        .ok_or(PackageHttpError::NotFound)?;
    let bytes = tokio::fs::read(&configured.artifact)
        .await
        .map_err(|error| PackageHttpError::Unavailable(error.to_string()))?;
    let digest = hex::encode(Sha256::digest(&bytes));
    if digest != query.sha256 {
        return Err(PackageHttpError::NotFound);
    }

    let mut response = Response::new(Body::from(bytes));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    response.headers_mut().insert(
        header::ETAG,
        HeaderValue::from_str(&format!("\"sha256:{digest}\""))
            .map_err(|error| PackageHttpError::Unavailable(error.to_string()))?,
    );
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::to_bytes, http::Request};
    use tower::ServiceExt;

    fn fixture() -> (tempfile::TempDir, PackageDistributor, Vec<u8>) {
        let directory = tempfile::tempdir().unwrap();
        let artifact = directory.path().join("echo.actr");
        let bytes = b"complete actor package".to_vec();
        std::fs::write(&artifact, &bytes).unwrap();
        let distributor = PackageDistributor::new(
            Url::parse("https://packages.example/actr/packages").unwrap(),
            [PackageTarget {
                target: "apps/echo".to_string(),
                artifact,
                profile: Some("keel.guest/1".to_string()),
            }],
        )
        .unwrap();
        (directory, distributor, bytes)
    }

    #[tokio::test]
    async fn resolves_opaque_target_to_content_addressed_artifact() {
        let (_directory, distributor, bytes) = fixture();
        let resolution = distributor.resolve("apps/echo").await.unwrap();
        assert_eq!(resolution.target, "apps/echo");
        assert_eq!(resolution.package.size, bytes.len() as u64);
        assert_eq!(resolution.package.profile.as_deref(), Some("keel.guest/1"));
        assert!(resolution.package.href.contains("target=apps%2Fecho"));
        assert!(resolution.package.href.contains("sha256="));
        assert_eq!(
            distributor.resolve("apps/echo/server").await.unwrap_err(),
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn download_requires_the_resolved_digest() {
        let (_directory, distributor, bytes) = fixture();
        let resolution = distributor.resolve("apps/echo").await.unwrap();
        let artifact = Url::parse(&resolution.package.href).unwrap();
        let request_uri = artifact[url::Position::BeforePath..].to_string();
        let app = Router::new().nest("/actr/packages", distributor.router());

        let response = app
            .clone()
            .oneshot(Request::get(request_uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/zip");
        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        assert_eq!(body.as_ref(), bytes);

        let response = app
            .oneshot(
                Request::get("/artifact?target=apps%2Fecho&sha256=wrong")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn rejects_noncanonical_targets_and_ambiguous_endpoints() {
        let directory = tempfile::tempdir().unwrap();
        let artifact = directory.path().join("echo.actr");
        std::fs::write(&artifact, b"package").unwrap();

        let result = PackageDistributor::new(
            Url::parse("https://packages.example/actr/packages?tenant=x").unwrap(),
            std::iter::empty(),
        );
        assert!(matches!(
            result,
            Err(PackageDistributorError::InvalidEndpoint(_))
        ));

        for target in ["", "/echo", "apps//echo", "apps/../echo", "apps/%ZZ"] {
            let result = PackageDistributor::new(
                Url::parse("https://packages.example/actr/packages").unwrap(),
                [PackageTarget {
                    target: target.to_string(),
                    artifact: artifact.clone(),
                    profile: None,
                }],
            );
            assert!(matches!(
                result,
                Err(PackageDistributorError::InvalidTarget(_))
            ));
        }
    }
}
