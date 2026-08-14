//! AIS HTTP client
//!
//! Encapsulates the logic for sending protobuf requests to the AIS `/register` endpoint.
//! Supports two registration modes:
//! - Package registration: authenticate with manifest_raw + mfr_signature
//! - Linked registration: authenticate with realm authorization
//!
//! Credential renewal is handled by the Credential Manager via `POST /ais/renew`,
//! not by re-calling `/register` with a PSK.

use std::time::Duration;

use prost::Message;
use tracing::{debug, error, info, warn};

use actr_protocol::{
    ErrorResponse, RegisterRequest, RegisterResponse, ReissueCredentialRequest,
    RenewCredentialRequest, RenewCredentialResponse, renew_credential_response,
};

use crate::error::{HyperError, HyperResult};

/// Structured errors returned by POST /ais/renew.
#[derive(Debug, Clone, thiserror::Error)]
#[non_exhaustive]
pub enum RenewError {
    #[error("invalid renewal request: {0}")]
    InvalidRequest(String),
    #[error("renewal token rejected")]
    TokenRejected,
    #[error("renewal attempt result expired; start a new attempt")]
    AttemptExpired,
    #[error("renewal attempt id conflicts with a different request")]
    AttemptConflict,
    #[error("renewal attempt is still in progress")]
    AttemptInProgress,
    #[error("realm unavailable")]
    RealmUnavailable,
    #[error("renewal rate limited")]
    RateLimited { retry_after: Option<Duration> },
    #[error("retryable renewal error: {0}")]
    Retryable(String),
    #[error("renewal protocol error: {0}")]
    Protocol(String),
}

/// Structured errors returned by POST /ais/reissue.
#[derive(Debug, Clone, thiserror::Error)]
#[non_exhaustive]
pub enum ReissueError {
    #[error("reissue attempt result expired; start a new attempt")]
    AttemptExpired,
    #[error("reissue attempt id conflicts with a different request")]
    AttemptConflict,
    #[error("reissue attempt is still in progress")]
    AttemptInProgress,
    #[error("reissue rejected with HTTP status {0}")]
    Rejected(u16),
    #[error("retryable reissue error: {0}")]
    Retryable(String),
    #[error("reissue protocol error: {0}")]
    Protocol(String),
}

enum ProtobufRequestError {
    HttpStatus(u16),
    Retryable(String),
    Protocol(String),
}

/// AIS HTTP client
///
/// Encapsulates the logic for sending protobuf requests to the AIS /register endpoint.
/// All requests use `application/x-protobuf` encoding.
pub struct AisClient {
    endpoint: String,
    http: reqwest::Client,
    /// Optional realm secret for `x-actrix-realm-secret` header authentication
    realm_secret: Option<String>,
}

const CONTROL_ATTEMPT_RETRY_DEADLINE: Duration = Duration::from_secs(95);
const CONTROL_ATTEMPT_BASE_RETRY_DELAY: Duration = Duration::from_millis(100);
const CONTROL_ATTEMPT_MAX_RETRY_DELAY: Duration = Duration::from_secs(2);

pub(crate) fn new_control_attempt_id() -> prost::bytes::Bytes {
    prost::bytes::Bytes::copy_from_slice(uuid::Uuid::new_v4().as_bytes())
}

impl AisClient {
    /// Create a new AIS client
    ///
    /// `endpoint` is the AIS base URL, e.g. `"http://ais.example.com:8080"`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        crate::timer::register_external(
            crate::timer::ids::EXTERNAL_AIS_HTTP_REQUEST,
            Duration::from_secs(30),
        );
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("reqwest::Client build failed (should never happen)");
        Self {
            endpoint: endpoint.into(),
            http,
            realm_secret: None,
        }
    }

    /// Set the realm secret for authentication
    pub fn with_realm_secret(mut self, secret: impl Into<String>) -> Self {
        self.realm_secret = Some(secret.into());
        self
    }

    /// Initial registration: authenticate with MFR manifest
    ///
    /// Sends a RegisterRequest (containing manifest_raw + mfr_signature),
    /// receives a RegisterResponse.
    pub async fn register_with_manifest(
        &self,
        req: RegisterRequest,
    ) -> HyperResult<RegisterResponse> {
        info!(
            endpoint = %self.endpoint,
            "initial registration: registering with AIS via MFR manifest"
        );
        self.do_register(req).await
    }

    /// Linked registration: authenticate with realm authorization.
    ///
    /// Sends a RegisterRequest marked as linked source mode. AIS authorizes it
    /// using the realm secret header instead of MFR package identity.
    pub async fn register_linked(&self, req: RegisterRequest) -> HyperResult<RegisterResponse> {
        info!(
            endpoint = %self.endpoint,
            "linked registration: registering with AIS via realm authorization"
        );
        self.do_register(req).await
    }

    /// Re-authenticate a workload and re-issue credentials for its existing,
    /// stable [`actr_protocol::ActrId`].
    ///
    /// Unlike `/register`, `/reissue` never allocates a new identity. It is the
    /// hard fallback when the renewal token is no longer usable.
    pub async fn reissue_credential(
        &self,
        req: ReissueCredentialRequest,
    ) -> HyperResult<RegisterResponse> {
        self.reissue_credential_classified(req)
            .await
            .map_err(|error| HyperError::AisBootstrapFailed(error.to_string()))
    }

    pub(crate) async fn reissue_credential_classified(
        &self,
        req: ReissueCredentialRequest,
    ) -> Result<RegisterResponse, ReissueError> {
        info!(
            endpoint = %self.endpoint,
            actor_id = %req.actr_id.to_string_repr(),
            "reissuing AIS credential for stable actor identity"
        );
        self.do_protobuf_request("reissue", req.encode_to_vec(), true)
            .await
            .map_err(ReissueError::from)
    }

    /// Soft renewal: authenticate with the renewal token bound to the current ActrId.
    pub async fn renew_credential(
        &self,
        req: RenewCredentialRequest,
    ) -> Result<RenewCredentialResponse, RenewError> {
        let base = self.endpoint.to_string().trim_end_matches('/').to_string();
        let url = format!("{}/renew", base);
        let body = req.encode_to_vec();

        debug!(url = %url, body_len = body.len(), "sending AIS renew request");

        // Spawn onto tokio worker to avoid stack overflow on GCD cooperative
        // queues (same root cause as do_register).
        let deadline = tokio::time::Instant::now() + CONTROL_ATTEMPT_RETRY_DEADLINE;
        let mut completed = None;
        let mut last_transport_error = None;
        let mut retry_count = 0u32;
        loop {
            let pending = self
                .http
                .post(&url)
                .header("Content-Type", "application/x-protobuf")
                .header("Accept", "application/x-protobuf")
                .body(body.clone())
                .send();

            let response = match tokio::task::spawn(pending).await {
                Ok(Ok(response)) => response,
                Ok(Err(error)) => {
                    last_transport_error = Some(format!("HTTP request failed: {error}"));
                    if retry_control_attempt(deadline, control_attempt_backoff(retry_count)).await {
                        retry_count = retry_count.saturating_add(1);
                        continue;
                    }
                    break;
                }
                Err(error) => {
                    last_transport_error = Some(format!("spawn failed: {error}"));
                    if retry_control_attempt(deadline, control_attempt_backoff(retry_count)).await {
                        retry_count = retry_count.saturating_add(1);
                        continue;
                    }
                    break;
                }
            };

            let status = response.status();
            let retry_after =
                parse_retry_after(response.headers().get(reqwest::header::RETRY_AFTER));
            let bytes = match response.bytes().await {
                Ok(bytes) => bytes,
                Err(error) => {
                    last_transport_error = Some(format!("failed to read response body: {error}"));
                    if retry_control_attempt(deadline, control_attempt_backoff(retry_count)).await {
                        retry_count = retry_count.saturating_add(1);
                        continue;
                    }
                    break;
                }
            };
            let retryable_status = status == reqwest::StatusCode::TOO_EARLY;
            if retryable_status {
                let delay = retry_after.unwrap_or_else(|| Duration::from_secs(1));
                if retry_control_attempt(deadline, delay).await {
                    retry_count = retry_count.saturating_add(1);
                    continue;
                }
            }
            completed = Some((status, retry_after, bytes));
            break;
        }
        let (status, retry_after, bytes) = completed.ok_or_else(|| {
            RenewError::Retryable(
                last_transport_error.unwrap_or_else(|| "request failed without response".into()),
            )
        })?;

        if !status.is_success() {
            return Err(classify_renew_status(status.as_u16(), retry_after));
        }

        let decoded = RenewCredentialResponse::decode(bytes.as_ref())
            .map_err(|e| RenewError::Protocol(format!("response protobuf decode failed: {e}")))?;

        match decoded.result.as_ref() {
            Some(renew_credential_response::Result::Success(_)) => Ok(decoded),
            Some(renew_credential_response::Result::Error(err)) => Err(classify_renew_error(err)),
            None => Err(RenewError::Protocol(
                "renew response missing result".to_string(),
            )),
        }
    }

    /// Send POST /register request, common logic
    ///
    /// Encodes a RegisterRequest as protobuf and POSTs it to `{endpoint}/register`,
    /// then decodes the response as RegisterResponse.
    ///
    /// The HTTP call is spawned onto the tokio runtime to avoid stack overflow
    /// when polled from a GCD cooperative queue (UNIFFI async bridge context).
    /// reqwest/hyper's deeply nested combinator chain (~140 frames of generic
    /// state machines) exceeds GCD thread stack limits on iOS simulator.
    async fn do_register(&self, req: RegisterRequest) -> HyperResult<RegisterResponse> {
        // `/register` is intentionally non-idempotent: a second call requests
        // a new identity, so ambiguous transport failures are surfaced rather
        // than retried automatically.
        self.do_protobuf_request("register", req.encode_to_vec(), false)
            .await
            .map_err(|error| HyperError::AisBootstrapFailed(error.to_string()))
    }

    /// Send a protobuf request that returns [`RegisterResponse`].
    async fn do_protobuf_request(
        &self,
        path: &str,
        body: Vec<u8>,
        idempotent: bool,
    ) -> Result<RegisterResponse, ProtobufRequestError> {
        let base = self.endpoint.to_string().trim_end_matches('/').to_string();
        let url = format!("{base}/{path}");

        debug!(url = %url, body_len = body.len(), "sending AIS protobuf request");

        let deadline = tokio::time::Instant::now() + CONTROL_ATTEMPT_RETRY_DEADLINE;
        let mut completed = None;
        let mut last_error = None;
        let mut retry_count = 0u32;
        loop {
            let mut request_builder = self
                .http
                .post(&url)
                .header("Content-Type", "application/x-protobuf")
                .header("Accept", "application/x-protobuf");

            if let Some(ref secret) = self.realm_secret {
                request_builder = request_builder.header("x-actrix-realm-secret", secret);
            }

            let pending = request_builder.body(body.clone()).send();
            match tokio::task::spawn(pending).await {
                Ok(Ok(candidate)) => {
                    let status = candidate.status();
                    let retryable_status = status == reqwest::StatusCode::TOO_EARLY;
                    let retry_after =
                        parse_retry_after(candidate.headers().get(reqwest::header::RETRY_AFTER));
                    match candidate.bytes().await {
                        Ok(bytes) => {
                            if idempotent && retryable_status {
                                let delay = retry_after.unwrap_or_else(|| Duration::from_secs(1));
                                if retry_control_attempt(deadline, delay).await {
                                    retry_count = retry_count.saturating_add(1);
                                    continue;
                                }
                            }
                            completed = Some((status, bytes));
                            break;
                        }
                        Err(err) => {
                            last_error = Some(format!("failed to read response body: {err}"));
                        }
                    }
                }
                Ok(Err(err)) => {
                    last_error = Some(format!("HTTP request failed: {err}"));
                }
                Err(err) => {
                    last_error = Some(format!("spawn failed: {err}"));
                }
            }
            if idempotent
                && retry_control_attempt(deadline, control_attempt_backoff(retry_count)).await
            {
                retry_count = retry_count.saturating_add(1);
                continue;
            }
            break;
        }
        let (status, bytes) = completed.ok_or_else(|| {
            let message = last_error.unwrap_or_else(|| "request failed without response".into());
            error!(url = %url, error = %message, "AIS HTTP request failed");
            ProtobufRequestError::Retryable(message)
        })?;

        if !status.is_success() {
            warn!(url = %url, status = %status, "AIS returned non-2xx status");
            return Err(ProtobufRequestError::HttpStatus(status.as_u16()));
        }

        debug!(url = %url, response_len = bytes.len(), "received AIS response");

        let resp = RegisterResponse::decode(bytes.as_ref()).map_err(|e| {
            error!(url = %url, error = %e, "failed to decode AIS RegisterResponse");
            ProtobufRequestError::Protocol(format!("response protobuf decode failed: {e}"))
        })?;

        Ok(resp)
    }
}

impl std::fmt::Display for ProtobufRequestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HttpStatus(status) => write!(formatter, "AIS returned error status: {status}"),
            Self::Retryable(message) | Self::Protocol(message) => formatter.write_str(message),
        }
    }
}

impl From<ProtobufRequestError> for ReissueError {
    fn from(error: ProtobufRequestError) -> Self {
        match error {
            ProtobufRequestError::HttpStatus(409) => Self::AttemptConflict,
            ProtobufRequestError::HttpStatus(410) => Self::AttemptExpired,
            ProtobufRequestError::HttpStatus(425) => Self::AttemptInProgress,
            ProtobufRequestError::HttpStatus(500 | 502 | 503 | 504) => {
                Self::Retryable(error.to_string())
            }
            ProtobufRequestError::HttpStatus(status) => Self::Rejected(status),
            ProtobufRequestError::Retryable(message) => Self::Retryable(message),
            ProtobufRequestError::Protocol(message) => Self::Protocol(message),
        }
    }
}

async fn retry_control_attempt(deadline: tokio::time::Instant, delay: Duration) -> bool {
    let now = tokio::time::Instant::now();
    if now >= deadline {
        return false;
    }
    let delay = delay.min(deadline.saturating_duration_since(now));
    crate::timer::sleep(crate::timer::ids::AIS_CONTROL_ATTEMPT_RETRY, delay).await;
    true
}

fn control_attempt_backoff(retry_count: u32) -> Duration {
    let multiplier = 1u32 << retry_count.min(4);
    (CONTROL_ATTEMPT_BASE_RETRY_DELAY * multiplier).min(CONTROL_ATTEMPT_MAX_RETRY_DELAY)
}

fn parse_retry_after(value: Option<&reqwest::header::HeaderValue>) -> Option<Duration> {
    value
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_secs)
}

fn classify_renew_error(error: &ErrorResponse) -> RenewError {
    classify_renew_status(error.code as u16, None)
}

fn classify_renew_status(status: u16, retry_after: Option<Duration>) -> RenewError {
    match status {
        400 => RenewError::InvalidRequest("invalid renew request".to_string()),
        401 => RenewError::TokenRejected,
        403 => RenewError::RealmUnavailable,
        409 => RenewError::AttemptConflict,
        410 => RenewError::AttemptExpired,
        425 => RenewError::AttemptInProgress,
        429 => RenewError::RateLimited { retry_after },
        500 | 502 | 503 | 504 => RenewError::Retryable(format!("AIS returned {status}")),
        other => RenewError::Protocol(format!("unexpected AIS renew status {other}")),
    }
}

#[cfg(test)]
#[path = "ais_client_tests.rs"]
mod tests;
