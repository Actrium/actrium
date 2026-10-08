//! Structured error payload for napi-rs consumers.
//!
//! napi-rs's `Error` type is flat (status + string message). To preserve the
//! 10-variant classification from `actr_protocol::ActrError` through the JS
//! boundary without losing structure, we serialize a small JSON object into
//! the message string:
//!
//! ```json
//! {
//!   "kind": "Client",
//!   "code": "DependencyNotFound",
//!   "message": "dependency 'a' not found: missing",
//!   "service_name": "a"
//! }
//! ```
//!
//! A companion `ActrError` class is declared in `index.d.ts`; the JS wrapper
//! in `typescript/*.ts` parses the JSON and re-throws a typed instance.

use actr_protocol::{ActrError, Classify, ConnectionNotReadyInfo, ErrorKind};

/// Preserve classification and structured metadata separately from Display text.
fn discriminate(e: &ActrError) -> (&'static str, Option<&str>, Option<&ConnectionNotReadyInfo>) {
    match e {
        ActrError::Unavailable(_) => ("Unavailable", None, None),
        ActrError::ConnectionNotReady(info) => ("ConnectionNotReady", None, Some(info)),
        ActrError::TimedOut => ("TimedOut", None, None),
        ActrError::NotFound(_) => ("NotFound", None, None),
        ActrError::PermissionDenied(_) => ("PermissionDenied", None, None),
        ActrError::InvalidArgument(_) => ("InvalidArgument", None, None),
        ActrError::UnknownRoute(_) => ("UnknownRoute", None, None),
        ActrError::DependencyNotFound { service_name, .. } => {
            ("DependencyNotFound", Some(service_name), None)
        }
        ActrError::DecodeFailure(_) => ("DecodeFailure", None, None),
        ActrError::NotImplemented(_) => ("NotImplemented", None, None),
        ActrError::Internal(_) => ("Internal", None, None),
        // Keep the existing fallback classification for unmapped variants.
        _ => ("Internal", None, None),
    }
}

fn kind_str(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::Transient => "Transient",
        ErrorKind::Client => "Client",
        ErrorKind::Internal => "Internal",
        ErrorKind::Corrupt => "Corrupt",
    }
}

/// Escape a string so it is safe to embed in a JSON double-quoted literal.
///
/// We only need a minimal subset here — backslash, double-quote, and the
/// common control chars. Anything more exotic falls through to
/// `\uXXXX`-style escaping via `char::escape_unicode` to keep the payload
/// valid for downstream `JSON.parse`.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x08' => out.push_str("\\b"),
            '\x0c' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

fn option_u64_json(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "null".to_string())
}

/// Build a JSON payload carrying kind / code / message / optional service_name
/// and retry metadata — the structure the `ActrError` JS class expects.
fn build_payload(
    kind: ErrorKind,
    code: &str,
    message: &str,
    service_name: Option<&str>,
    connection_not_ready_info: Option<&ConnectionNotReadyInfo>,
) -> String {
    let mut fields = vec![
        format!(r#""kind":"{}""#, kind_str(kind)),
        format!(r#""code":"{}""#, code),
        format!(r#""message":"{}""#, json_escape(message)),
    ];

    if let Some(svc) = service_name {
        fields.push(format!(r#""service_name":"{}""#, json_escape(svc)));
    }

    if let Some(info) = connection_not_ready_info {
        fields.push(format!(
            r#""retry_after_ms":{}"#,
            option_u64_json(info.retry_after_ms)
        ));
    }

    format!("{{{}}}", fields.join(","))
}

/// Convert a protocol-level error into a `napi::Error` carrying the
/// structured JSON payload.
pub(crate) fn actr_error_to_napi(e: actr_protocol::ActrError) -> napi::Error {
    let kind = e.kind();
    let (code, service_name, recovery_info) = discriminate(&e);
    let payload = build_payload(kind, code, &e.to_string(), service_name, recovery_info);
    napi::Error::new(napi::Status::GenericFailure, payload)
}

/// Same shape as [`actr_error_to_napi`] — retained so existing call sites
/// don't need to migrate in one giant patch.
pub(crate) fn protocol_error_to_napi(e: actr_protocol::ActrError) -> napi::Error {
    actr_error_to_napi(e)
}

/// Pre-protocol config failure. Classified as `Client` (the caller gave us
/// a bad manifest / config file).
pub(crate) fn config_error_to_napi(e: actr_config::ConfigError) -> napi::Error {
    let payload = build_payload(
        ErrorKind::Client,
        "Config",
        &format!("config: {e}"),
        None,
        None,
    );
    napi::Error::new(napi::Status::GenericFailure, payload)
}

/// Hyper-layer initialization error. Maps to Client/Internal depending on
/// the underlying failure — we lean `Internal` because hyper bootstrap
/// failures almost always indicate a platform/runtime problem rather than
/// bad caller input.
pub(crate) fn hyper_error_to_napi(e: actr_hyper::HyperError) -> napi::Error {
    let payload = build_payload(
        ErrorKind::Internal,
        "HyperBootstrap",
        &e.to_string(),
        None,
        None,
    );
    napi::Error::new(napi::Status::GenericFailure, payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_message_uses_the_ffi_config_prefix() {
        let error = config_error_to_napi(actr_config::ConfigError::MissingField("package"));
        assert!(
            error
                .reason
                .contains(r#""message":"config: Missing required field: package""#)
        );
    }

    #[test]
    fn all_protocol_messages_use_display() {
        let text = "\u{4e2d}\u{6587}\n\"quoted\"\\path";
        let cases = [
            ActrError::Unavailable(text.into()),
            ActrError::ConnectionNotReady(ConnectionNotReadyInfo::new(0, 42)),
            ActrError::ConnectionNotReady(ConnectionNotReadyInfo::without_retry_hint()),
            ActrError::TimedOut,
            ActrError::NotFound(text.into()),
            ActrError::PermissionDenied(text.into()),
            ActrError::InvalidArgument(text.into()),
            ActrError::UnknownRoute(text.into()),
            ActrError::DependencyNotFound {
                service_name: "echo".into(),
                message: text.into(),
            },
            ActrError::DecodeFailure(text.into()),
            ActrError::NotImplemented(text.into()),
            ActrError::Internal(text.into()),
            ActrError::DeliveryUncertain {
                operation_id: "op-1".into(),
                cause: text.into(),
            },
            ActrError::Internal(String::new()),
        ];
        for error in cases {
            let expected = format!(r#""message":"{}""#, json_escape(&error.to_string()));
            let actual = actr_error_to_napi(error);
            assert!(actual.reason.contains(&expected), "{}", actual.reason);
        }
    }

    #[test]
    fn dependency_not_found_includes_service_name() {
        let err = actr_error_to_napi(actr_protocol::ActrError::DependencyNotFound {
            service_name: "echo".to_string(),
            message: "missing".to_string(),
        });
        let msg = err.reason.as_str();
        assert!(msg.contains(r#""kind":"Client""#), "kind: {msg}");
        assert!(
            msg.contains(r#""code":"DependencyNotFound""#),
            "code: {msg}"
        );
        assert!(msg.contains(r#""service_name":"echo""#), "svc: {msg}");
    }

    #[test]
    fn timed_out_classifies_as_transient() {
        let err = actr_error_to_napi(actr_protocol::ActrError::TimedOut);
        let msg = err.reason.as_str();
        assert!(msg.contains(r#""kind":"Transient""#));
        assert!(msg.contains(r#""code":"TimedOut""#));
    }

    #[test]
    fn decode_failure_classifies_as_corrupt() {
        let err = actr_error_to_napi(actr_protocol::ActrError::DecodeFailure("bad".into()));
        let msg = err.reason.as_str();
        assert!(msg.contains(r#""kind":"Corrupt""#));
        assert!(msg.contains(r#""code":"DecodeFailure""#));
    }

    #[test]
    fn json_escapes_embedded_quotes() {
        let err = actr_error_to_napi(actr_protocol::ActrError::InvalidArgument(
            r#"a"b"#.to_string(),
        ));
        let msg = err.reason.as_str();
        assert!(
            msg.contains(r#""message":"invalid argument: a\"b""#),
            "escaped: {msg}"
        );
    }

    #[test]
    fn connection_not_ready_includes_retry_hint_only() {
        let err = actr_error_to_napi(actr_protocol::ActrError::ConnectionNotReady(
            actr_protocol::ConnectionNotReadyInfo::new(120, 6000),
        ));
        let msg = err.reason.as_str();
        assert!(msg.contains(r#""kind":"Transient""#), "kind: {msg}");
        assert!(
            msg.contains(r#""code":"ConnectionNotReady""#),
            "code: {msg}"
        );
        assert!(
            msg.contains(r#""retry_after_ms":5880"#),
            "retry_after_ms: {msg}"
        );
        assert!(
            msg.contains(r#""message":"connection not ready: retry after 5880ms""#),
            "message: {msg}"
        );
        assert!(!msg.contains("peer"), "peer should not leak: {msg}");
        assert!(
            !msg.contains("session_id"),
            "session_id should not leak: {msg}"
        );
        assert!(!msg.contains("delivery"), "delivery should not leak: {msg}");
    }

    #[test]
    fn connection_not_ready_without_hint_preserves_null_metadata() {
        let err = actr_error_to_napi(ActrError::ConnectionNotReady(
            ConnectionNotReadyInfo::without_retry_hint(),
        ));
        let msg = err.reason.as_str();
        assert!(msg.contains(r#""kind":"Transient""#), "kind: {msg}");
        assert!(
            msg.contains(r#""code":"ConnectionNotReady""#),
            "code: {msg}"
        );
        assert!(
            msg.contains(r#""retry_after_ms":null"#),
            "retry_after_ms: {msg}"
        );
        assert!(
            msg.contains(r#""message":"connection not ready: no retry hint""#),
            "message: {msg}"
        );
    }
}
