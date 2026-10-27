//! Content-addressed `.actr` artifact hosting for Loran.
//!
//! This component is a plain static download service. Each configured `.actr`
//! file is served by its SHA-256 digest, so a Realm Profile `package.href` may
//! point at it. It does not resolve URI targets and is not advertised in the
//! authority descriptor.

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

#[derive(Error, Debug)]
pub enum PackageDistributorError {
    #[error("package endpoint must be an absolute HTTP(S) URL: {0}")]
    InvalidEndpoint(String),
    #[error("package artifact is not a non-empty, readable .actr file by absolute path: {0}")]
    InvalidArtifact(String),
}

#[derive(Debug, Clone)]
pub struct PackageDistributor {
    artifact_endpoint: Url,
    /// Configured artifacts keyed by their lowercase hex SHA-256 digest.
    artifacts: Arc<HashMap<String, PathBuf>>,
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
                "artifact-not-found",
                "artifact not found".to_string(),
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
struct ArtifactQuery {
    sha256: String,
}

impl PackageDistributor {
    pub fn new(
        endpoint: Url,
        artifacts: impl IntoIterator<Item = PathBuf>,
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
        for artifact in artifacts {
            let invalid =
                || PackageDistributorError::InvalidArtifact(artifact.display().to_string());
            if artifact.extension().and_then(|value| value.to_str()) != Some("actr")
                || !artifact.is_absolute()
            {
                return Err(invalid());
            }
            let bytes = std::fs::read(&artifact).map_err(|_| invalid())?;
            if bytes.is_empty() {
                return Err(invalid());
            }
            indexed.insert(hex::encode(Sha256::digest(&bytes)), artifact);
        }

        Ok(Self {
            artifact_endpoint,
            artifacts: Arc::new(indexed),
        })
    }

    /// Download URLs of all hosted artifacts, each suitable for a Realm
    /// Profile `package.href`.
    pub fn artifact_hrefs(&self) -> impl Iterator<Item = Url> + '_ {
        self.artifacts.keys().map(|sha256| {
            let mut href = self.artifact_endpoint.clone();
            href.query_pairs_mut().append_pair("sha256", sha256);
            href
        })
    }

    pub fn router(self) -> Router {
        let state = Arc::new(self);
        Router::new()
            .route("/artifact", get(get_artifact))
            .with_state(state)
    }
}

async fn get_artifact(
    State(state): State<Arc<PackageDistributor>>,
    Query(query): Query<ArtifactQuery>,
) -> Result<Response, PackageHttpError> {
    let path = state
        .artifacts
        .get(&query.sha256)
        .ok_or(PackageHttpError::NotFound)?;
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|error| PackageHttpError::Unavailable(error.to_string()))?;
    // The file may have changed since startup; never serve bytes under a
    // digest they no longer match.
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

    fn fixture() -> (tempfile::TempDir, PathBuf, PackageDistributor, Vec<u8>) {
        let directory = tempfile::tempdir().unwrap();
        let artifact = directory.path().join("echo.actr");
        let bytes = b"complete actor package".to_vec();
        std::fs::write(&artifact, &bytes).unwrap();
        let distributor = PackageDistributor::new(
            Url::parse("https://packages.example/actr/packages").unwrap(),
            [artifact.clone()],
        )
        .unwrap();
        (directory, artifact, distributor, bytes)
    }

    #[tokio::test]
    async fn serves_artifact_by_digest() {
        let (_directory, _artifact, distributor, bytes) = fixture();
        let sha256 = hex::encode(Sha256::digest(&bytes));
        let hrefs = distributor.artifact_hrefs().collect::<Vec<_>>();
        assert_eq!(hrefs.len(), 1);
        let href = &hrefs[0];
        assert_eq!(
            href.as_str(),
            format!("https://packages.example/actr/packages/artifact?sha256={sha256}")
        );
        let request_uri = href[url::Position::BeforePath..].to_string();
        let app = Router::new().nest("/actr/packages", distributor.router());

        let response = app
            .clone()
            .oneshot(Request::get(request_uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/zip");
        assert_eq!(
            response.headers()[header::ETAG],
            format!("\"sha256:{sha256}\"").as_str()
        );
        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        assert_eq!(body.as_ref(), bytes);

        let response = app
            .oneshot(
                Request::get("/actr/packages/artifact?sha256=wrong")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn does_not_serve_changed_bytes_under_old_digest() {
        let (_directory, artifact, distributor, bytes) = fixture();
        let sha256 = hex::encode(Sha256::digest(&bytes));
        std::fs::write(&artifact, b"replaced actor package").unwrap();

        let response = distributor
            .router()
            .oneshot(
                Request::get(format!("/artifact?sha256={sha256}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn rejects_invalid_artifacts_and_ambiguous_endpoints() {
        let directory = tempfile::tempdir().unwrap();
        let artifact = directory.path().join("echo.actr");
        std::fs::write(&artifact, b"package").unwrap();
        let empty = directory.path().join("empty.actr");
        std::fs::write(&empty, b"").unwrap();
        let wrong_extension = directory.path().join("echo.zip");
        std::fs::write(&wrong_extension, b"package").unwrap();

        let result = PackageDistributor::new(
            Url::parse("https://packages.example/actr/packages?tenant=x").unwrap(),
            [artifact],
        );
        assert!(matches!(
            result,
            Err(PackageDistributorError::InvalidEndpoint(_))
        ));

        for path in [
            empty,
            wrong_extension,
            directory.path().join("missing.actr"),
            PathBuf::from("relative/echo.actr"),
        ] {
            let result = PackageDistributor::new(
                Url::parse("https://packages.example/actr/packages").unwrap(),
                [path],
            );
            assert!(matches!(
                result,
                Err(PackageDistributorError::InvalidArtifact(_))
            ));
        }
    }
}
