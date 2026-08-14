//! AIS HTTP client for Web (Service Worker / DOM)
//!
//! Uses `web_sys::fetch` to send protobuf-encoded `RegisterRequest` to the AIS `/register`
//! endpoint. Mirrors the native `AisClient` (`core/hyper/src/ais_client.rs`) semantics.

use actr_protocol::prost::Message;
use actr_protocol::{
    RegisterRequest, RegisterResponse, RenewCredentialRequest, RenewCredentialResponse,
};
use js_sys::{ArrayBuffer, Promise, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::error::{WebError, WebResult};

const CONTROL_ATTEMPT_RETRY_DEADLINE_MS: f64 = 95_000.0;
const CONTROL_ATTEMPT_BASE_RETRY_DELAY_MS: u32 = 100;
const CONTROL_ATTEMPT_MAX_RETRY_DELAY_MS: u32 = 2_000;

/// Structured errors returned by POST /ais/renew in Web runtimes.
#[derive(Debug, Clone, thiserror::Error)]
#[non_exhaustive]
pub enum RenewError {
    #[error("invalid renew request: {0}")]
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
    RateLimited { retry_after_secs: Option<u64> },
    #[error("retryable renew error: {0}")]
    Retryable(String),
    #[error("renew protocol error: {0}")]
    Protocol(String),
}

/// AIS HTTP client (Web environment)
///
/// Sends protobuf-encoded registration requests to the AIS `/register` endpoint
/// via `fetch()`. Works in both Window and Service Worker contexts.
pub struct WebAisClient {
    endpoint: String,
}

impl WebAisClient {
    /// Create a new Web AIS client.
    ///
    /// `endpoint` is the AIS base URL, e.g. `"https://ais.example.com:8080"`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
        }
    }

    /// Initial registration: authenticate with MFR manifest.
    ///
    /// Sends a `RegisterRequest` (containing `manifest_json` + `mfr_signature`),
    /// receives a `RegisterResponse`.
    pub async fn register_with_manifest(
        &self,
        req: RegisterRequest,
    ) -> WebResult<RegisterResponse> {
        log::info!(
            "initial registration: registering with AIS via MFR manifest (endpoint={})",
            self.endpoint
        );
        self.do_register(req).await
    }

    /// Linked registration: authenticate as source-linked workload.
    pub async fn register_linked(&self, req: RegisterRequest) -> WebResult<RegisterResponse> {
        log::info!(
            "linked registration: registering with AIS via realm authorization (endpoint={})",
            self.endpoint
        );
        self.do_register(req).await
    }

    /// Renew credentials for the same ActrId using a renewal token.
    pub async fn renew_credential(
        &self,
        req: RenewCredentialRequest,
    ) -> Result<RenewCredentialResponse, RenewError> {
        let url = format!("{}/renew", self.endpoint);
        let body = req.encode_to_vec();
        let buf = self.post_protobuf_for_renew(&url, body).await?;

        let decoded = RenewCredentialResponse::decode(buf.as_slice()).map_err(|e| {
            log::error!("failed to decode AIS RenewCredentialResponse: {}", e);
            RenewError::Protocol(format!("renew response protobuf decode failed: {e}"))
        })?;

        match decoded.result.as_ref() {
            Some(actr_protocol::renew_credential_response::Result::Success(_)) => Ok(decoded),
            Some(actr_protocol::renew_credential_response::Result::Error(err)) => {
                Err(classify_renew_error(err.code, err.message.clone()))
            }
            None => Err(RenewError::Protocol(
                "renew response missing result".to_string(),
            )),
        }
    }

    /// Common fetch logic for both registration modes.
    async fn do_register(&self, req: RegisterRequest) -> WebResult<RegisterResponse> {
        let url = format!("{}/register", self.endpoint);
        let body = req.encode_to_vec();

        let buf = self.post_protobuf(&url, body).await?;

        let register_response = RegisterResponse::decode(buf.as_slice()).map_err(|e| {
            log::error!("failed to decode AIS RegisterResponse: {}", e);
            WebError::Protocol(format!("response protobuf decode failed: {e}"))
        })?;

        Ok(register_response)
    }

    async fn post_protobuf(&self, url: &str, body: Vec<u8>) -> WebResult<Vec<u8>> {
        let (status, status_text, _retry_after, buf) = self.post_protobuf_raw(url, body).await?;
        if !(200..300).contains(&status) {
            log::warn!(
                "AIS returned non-2xx status: {} {} (url={})",
                status,
                status_text,
                url
            );
            return Err(WebError::Network(format!(
                "AIS returned error status: {status} {status_text}"
            )));
        }
        Ok(buf)
    }

    async fn post_protobuf_for_renew(
        &self,
        url: &str,
        body: Vec<u8>,
    ) -> Result<Vec<u8>, RenewError> {
        let deadline_ms = js_sys::Date::now() + CONTROL_ATTEMPT_RETRY_DEADLINE_MS;
        let mut completed = None;
        let mut last_error = None;
        let mut retry_count = 0u32;
        loop {
            match self.post_protobuf_raw(url, body.clone()).await {
                Ok(result) => {
                    let retryable = result.0 == 425;
                    if retryable {
                        let delay_ms = result
                            .2
                            .unwrap_or(1)
                            .saturating_mul(1_000)
                            .min(u64::from(u32::MAX)) as u32;
                        if wait_for_control_retry(deadline_ms, delay_ms).await {
                            retry_count = retry_count.saturating_add(1);
                            continue;
                        }
                    }
                    completed = Some(result);
                    break;
                }
                Err(error) => {
                    last_error = Some(error.to_string());
                    if wait_for_control_retry(deadline_ms, control_attempt_backoff_ms(retry_count))
                        .await
                    {
                        retry_count = retry_count.saturating_add(1);
                        continue;
                    }
                    break;
                }
            }
        }
        let (status, status_text, retry_after_secs, buf) = completed.ok_or_else(|| {
            RenewError::Retryable(
                last_error.unwrap_or_else(|| "request failed without response".to_string()),
            )
        })?;
        if (200..300).contains(&status) {
            return Ok(buf);
        }

        Err(classify_renew_http_status(
            status,
            status_text,
            retry_after_secs,
        ))
    }

    async fn post_protobuf_raw(
        &self,
        url: &str,
        body: Vec<u8>,
    ) -> WebResult<(u16, String, Option<u64>, Vec<u8>)> {
        log::debug!("sending AIS request: url={}, body_len={}", url, body.len());
        // Build RequestInit
        let init = web_sys::RequestInit::new();
        init.set_method("POST");

        // Set body as Uint8Array
        let js_body = Uint8Array::from(body.as_slice());
        init.set_body(&js_body.into());

        // Build Headers
        let headers = web_sys::Headers::new()
            .map_err(|e| WebError::Network(format!("failed to create Headers: {e:?}")))?;
        headers
            .set("Content-Type", "application/x-protobuf")
            .map_err(|e| WebError::Network(format!("failed to set Content-Type: {e:?}")))?;
        headers
            .set("Accept", "application/x-protobuf")
            .map_err(|e| WebError::Network(format!("failed to set Accept: {e:?}")))?;
        init.set_headers(&headers.into());

        // AbortController for 30s timeout
        let abort_controller = web_sys::AbortController::new()
            .map_err(|e| WebError::Network(format!("failed to create AbortController: {e:?}")))?;
        init.set_signal(Some(&abort_controller.signal()));

        // Schedule abort after 30 seconds
        let abort_cb = Closure::once(move || {
            abort_controller.abort();
        });
        let global = js_sys::global();
        let set_timeout_fn = js_sys::Reflect::get(&global, &JsValue::from_str("setTimeout"))
            .map_err(|e| WebError::Network(format!("setTimeout not available: {e:?}")))?;
        let set_timeout_fn: js_sys::Function = set_timeout_fn
            .dyn_into()
            .map_err(|_| WebError::Network("setTimeout is not a function".to_string()))?;
        let timeout_id = set_timeout_fn
            .call2(
                &JsValue::NULL,
                abort_cb.as_ref(),
                &JsValue::from_f64(30_000.0),
            )
            .map_err(|e| WebError::Network(format!("failed to call setTimeout: {e:?}")))?;

        // Build Request
        let request = web_sys::Request::new_with_str_and_init(url, &init)
            .map_err(|e| WebError::Network(format!("failed to create Request: {e:?}")))?;

        // Call fetch() via global scope (works in both Window and ServiceWorker)
        let fetch_promise: Promise = {
            let global_obj: web_sys::WorkerGlobalScope = global.unchecked_into();
            global_obj.fetch_with_request(&request)
        };

        let resp_value = JsFuture::from(fetch_promise).await.map_err(|e| {
            // Check if this was an abort (timeout)
            let msg = format!("{e:?}");
            if msg.contains("abort") || msg.contains("Abort") {
                WebError::Timeout
            } else {
                WebError::Network(format!("fetch failed: {msg}"))
            }
        })?;

        // Cancel timeout since fetch completed
        let clear_timeout_fn =
            js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("clearTimeout"))
                .ok()
                .and_then(|f| f.dyn_into::<js_sys::Function>().ok());
        if let Some(clear_fn) = clear_timeout_fn {
            let _ = clear_fn.call1(&JsValue::NULL, &timeout_id);
        }
        // Prevent closure from being dropped before timeout fires
        drop(abort_cb);

        let resp: web_sys::Response = resp_value
            .dyn_into()
            .map_err(|_| WebError::Network("fetch did not return a Response object".to_string()))?;

        let status = resp.status();
        let status_text = resp.status_text();
        let retry_after_secs = resp
            .headers()
            .get("Retry-After")
            .ok()
            .flatten()
            .and_then(|value| value.parse::<u64>().ok());

        // Read response body as ArrayBuffer
        let body_promise = resp
            .array_buffer()
            .map_err(|e| WebError::Network(format!("failed to read response body: {e:?}")))?;
        let body_value = JsFuture::from(body_promise)
            .await
            .map_err(|e| WebError::Network(format!("failed to await response body: {e:?}")))?;
        let array_buffer: ArrayBuffer = body_value
            .dyn_into()
            .map_err(|_| WebError::Network("response body is not an ArrayBuffer".to_string()))?;
        let bytes = Uint8Array::new(&array_buffer);
        let mut buf = vec![0u8; bytes.length() as usize];
        bytes.copy_to(&mut buf);

        log::debug!(
            "received AIS response: url={}, response_len={}",
            url,
            buf.len()
        );
        Ok((status, status_text, retry_after_secs, buf))
    }
}

async fn wait_for_control_retry(deadline_ms: f64, requested_delay_ms: u32) -> bool {
    let remaining_ms = deadline_ms - js_sys::Date::now();
    if remaining_ms <= 0.0 {
        return false;
    }
    let delay_ms = requested_delay_ms.min(remaining_ms.ceil().min(f64::from(u32::MAX)) as u32);
    gloo_timers::future::TimeoutFuture::new(delay_ms).await;
    true
}

fn control_attempt_backoff_ms(retry_count: u32) -> u32 {
    CONTROL_ATTEMPT_BASE_RETRY_DELAY_MS
        .saturating_mul(1u32 << retry_count.min(4))
        .min(CONTROL_ATTEMPT_MAX_RETRY_DELAY_MS)
}

fn classify_renew_http_status(
    status: u16,
    status_text: String,
    retry_after_secs: Option<u64>,
) -> RenewError {
    match status {
        400 => RenewError::InvalidRequest(status_text),
        401 => RenewError::TokenRejected,
        403 => RenewError::RealmUnavailable,
        409 => RenewError::AttemptConflict,
        410 => RenewError::AttemptExpired,
        425 => RenewError::AttemptInProgress,
        429 => RenewError::RateLimited { retry_after_secs },
        500 | 502 | 503 | 504 => RenewError::Retryable(format!("{status} {status_text}")),
        _ => RenewError::Protocol(format!(
            "unexpected renew HTTP status {status} {status_text}"
        )),
    }
}

fn classify_renew_error(code: u32, message: String) -> RenewError {
    match code {
        400 => RenewError::InvalidRequest(message),
        401 => RenewError::TokenRejected,
        403 => RenewError::RealmUnavailable,
        409 => RenewError::AttemptConflict,
        410 => RenewError::AttemptExpired,
        425 => RenewError::AttemptInProgress,
        429 => RenewError::RateLimited {
            retry_after_secs: None,
        },
        500 | 502 | 503 | 504 => RenewError::Retryable(message),
        _ => RenewError::Protocol(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_http_statuses_remain_distinguishable() {
        assert!(matches!(
            classify_renew_http_status(400, "bad".into(), None),
            RenewError::InvalidRequest(_)
        ));
        assert!(matches!(
            classify_renew_http_status(401, "unauthorized".into(), None),
            RenewError::TokenRejected
        ));
        assert!(matches!(
            classify_renew_http_status(403, "denied".into(), None),
            RenewError::RealmUnavailable
        ));
        assert!(matches!(
            classify_renew_http_status(409, "conflict".into(), None),
            RenewError::AttemptConflict
        ));
        assert!(matches!(
            classify_renew_http_status(410, "gone".into(), None),
            RenewError::AttemptExpired
        ));
        assert!(matches!(
            classify_renew_http_status(425, "early".into(), Some(90)),
            RenewError::AttemptInProgress
        ));
        assert!(matches!(
            classify_renew_http_status(429, "limited".into(), Some(17)),
            RenewError::RateLimited {
                retry_after_secs: Some(17)
            }
        ));
        assert!(matches!(
            classify_renew_http_status(503, "down".into(), None),
            RenewError::Retryable(_)
        ));
    }

    #[test]
    fn control_attempt_backoff_is_bounded() {
        assert_eq!(control_attempt_backoff_ms(0), 100);
        assert_eq!(control_attempt_backoff_ms(4), 1_600);
        assert_eq!(control_attempt_backoff_ms(20), 1_600);
    }
}
