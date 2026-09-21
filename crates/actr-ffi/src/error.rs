//! Error types for actr UniFFI bindings.
//!
//! Mirrors `actr_protocol::ActrError` 1:1 so that every fault classification
//! made by the core framework survives the FFI boundary. A small number of
//! binding-local variants cover errors that occur strictly before a call
//! reaches the protocol layer (e.g. config parsing inside the shell).

use actr_protocol::{
    Classify, ConnectionNotReadyInfo as ProtoConnectionNotReadyInfo, ErrorKind as ProtocolErrorKind,
};
use std::future::Future;

/// Fault domain classification exposed to UniFFI consumers.
///
/// Mirrors `actr_protocol::ErrorKind` so downstream generic policy code
/// (retry / DLQ routing / alerting) can be written once and reused across
/// Swift, Kotlin, and any future UniFFI language target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ErrorKind {
    /// Environmental fluctuation — retry with exponential backoff.
    Transient,
    /// Caller error — bad request or system state; do not retry.
    Client,
    /// Framework fault or uncertain delivery — do not retry. Framework faults
    /// are alertable; uncertain delivery requires caller reconciliation.
    Internal,
    /// Data corruption — route to Dead Letter Queue; manual intervention.
    Corrupt,
}

impl From<ProtocolErrorKind> for ErrorKind {
    fn from(k: ProtocolErrorKind) -> Self {
        match k {
            ProtocolErrorKind::Transient => ErrorKind::Transient,
            ProtocolErrorKind::Client => ErrorKind::Client,
            ProtocolErrorKind::Internal => ErrorKind::Internal,
            ProtocolErrorKind::Corrupt => ErrorKind::Corrupt,
        }
    }
}

/// Public payload for send preflight failures.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ConnectionNotReadyInfo {
    pub retry_after_ms: Option<u64>,
}

impl std::fmt::Display for ConnectionNotReadyInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let proto: ProtoConnectionNotReadyInfo = self.clone().into();
        proto.fmt(f)
    }
}

impl From<ProtoConnectionNotReadyInfo> for ConnectionNotReadyInfo {
    fn from(info: ProtoConnectionNotReadyInfo) -> Self {
        ConnectionNotReadyInfo {
            retry_after_ms: info.retry_after_ms,
        }
    }
}

impl From<ConnectionNotReadyInfo> for ProtoConnectionNotReadyInfo {
    fn from(info: ConnectionNotReadyInfo) -> Self {
        ProtoConnectionNotReadyInfo {
            retry_after_ms: info.retry_after_ms,
        }
    }
}

/// Error type for actr operations.
///
/// Protocol errors are mirrored one-for-one where the binding ABI exposes
/// them. `DeliveryUncertain` is appended after the existing variants so adding
/// this case does not renumber the established UniFFI error discriminants.
/// The binding-local `Config` variant captures failures that originate inside
/// the FFI shell before the protocol layer is reached.
/// UniFFI error discriminants are append-only: future protocol variants must
/// be appended after the existing cases and mirrored in every generated SDK;
/// never reorder or remove an established case.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ActrError {
    // ── Transient ─────────────────────────────────────────────────────────
    #[error("unavailable: {msg}")]
    Unavailable { msg: String },

    #[error("connection not ready: {info}")]
    ConnectionNotReady { info: ConnectionNotReadyInfo },

    #[error("timed out")]
    TimedOut,

    // ── Client ────────────────────────────────────────────────────────────
    #[error("not found: {msg}")]
    NotFound { msg: String },

    #[error("permission denied: {msg}")]
    PermissionDenied { msg: String },

    #[error("invalid argument: {msg}")]
    InvalidArgument { msg: String },

    #[error("unknown route: {msg}")]
    UnknownRoute { msg: String },

    #[error("dependency '{service_name}' not found: {detail}")]
    DependencyNotFound {
        service_name: String,
        detail: String,
    },

    // ── Corrupt ───────────────────────────────────────────────────────────
    #[error("decode failure: {msg}")]
    DecodeFailure { msg: String },

    // ── Internal ──────────────────────────────────────────────────────────
    #[error("not implemented: {msg}")]
    NotImplemented { msg: String },

    #[error("internal error: {msg}")]
    Internal { msg: String },

    // ── FFI-local (pre-protocol) ──────────────────────────────────────────
    /// Config file parsing / trust resolution failed before the runtime could
    /// hand the request over to the protocol layer.
    ///
    /// Classified as `ErrorKind::Client` — the caller supplied a bad manifest
    /// or runtime config.
    #[error("config: {msg}")]
    Config { msg: String },

    // ── Protocol additive variant ─────────────────────────────────────────
    /// The operation may have entered the transport, so generic retry is
    /// unsafe. Keep the operation ID and cause available to SDK callers.
    ///
    /// This variant is intentionally appended to preserve the discriminants
    /// of the existing UniFFI ABI.
    ///
    /// The field is named `cause_detail` rather than `cause`: Kotlin
    /// `Throwable.cause` would clash with a generated `cause` property.
    /// Field names do not affect the UniFFI wire format (variants serialize
    /// as ordinal + positional fields), so this is not an ABI change.
    #[error("delivery uncertain for operation {operation_id}: {cause_detail}")]
    DeliveryUncertain {
        operation_id: String,
        cause_detail: String,
    },
}

pub type ActrResult<T> = Result<T, ActrError>;

/// Keep stack-heavy Rust futures off foreign-language cooperative executors.
///
/// Dropping the returned future (Kotlin coroutine cancellation frees the
/// UniFFI future) aborts the spawned task instead of detaching it, so guest
/// RPCs and node shutdown actually stop (#390).
pub(crate) async fn run_on_tokio_runtime<T, E, F>(
    operation: &'static str,
    future: F,
) -> ActrResult<T>
where
    T: Send + 'static,
    E: Into<ActrError> + Send + 'static,
    F: Future<Output = Result<T, E>> + Send + 'static,
{
    AbortOnDrop(tokio::spawn(future))
        .await
        .map_err(|error| ActrError::Internal {
            msg: format!("{operation} task failed: {error}"),
        })?
        .map_err(Into::into)
}

/// Joins a spawned task, aborting it on drop. Aborting an already-finished
/// task is a no-op, so no disarm is needed on the success path.
struct AbortOnDrop<T>(tokio::task::JoinHandle<T>);

impl<T> std::future::Future for AbortOnDrop<T> {
    type Output = Result<T, tokio::task::JoinError>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        std::pin::Pin::new(&mut self.0).poll(cx)
    }
}

impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl ActrError {
    /// Returns the fault domain this error belongs to.
    ///
    /// Exposed to UniFFI consumers so they can branch on classification
    /// rather than pattern-matching every variant.
    pub(crate) fn kind(&self) -> ErrorKind {
        match self {
            ActrError::Unavailable { .. }
            | ActrError::ConnectionNotReady { .. }
            | ActrError::TimedOut => ErrorKind::Transient,

            ActrError::NotFound { .. }
            | ActrError::PermissionDenied { .. }
            | ActrError::InvalidArgument { .. }
            | ActrError::UnknownRoute { .. }
            | ActrError::DependencyNotFound { .. }
            | ActrError::Config { .. } => ErrorKind::Client,

            ActrError::DecodeFailure { .. } => ErrorKind::Corrupt,

            ActrError::NotImplemented { .. }
            | ActrError::Internal { .. }
            | ActrError::DeliveryUncertain { .. } => ErrorKind::Internal,
        }
    }

    /// Returns `true` if the operation may be retried (Transient only).
    pub(crate) fn is_retryable(&self) -> bool {
        matches!(self.kind(), ErrorKind::Transient)
    }

    /// Returns `true` if the message should be routed to a Dead Letter Queue
    /// (Corrupt only).
    pub(crate) fn requires_dlq(&self) -> bool {
        matches!(self.kind(), ErrorKind::Corrupt)
    }
}

// UniFFI exposes errors as flat enum variants, not objects — so we cannot
// attach methods directly onto `ActrError`. Instead we expose classification
// helpers as free functions that take the error by reference-equivalent
// clone. Swift / Kotlin consumers call these as module-level functions:
// `actrErrorKind(err)` / `actrErrorIsRetryable(err)` / etc.

/// Rust Display text for an error, shared by SDK message properties.
#[uniffi::export]
pub fn actr_error_message(err: ActrError) -> String {
    err.to_string()
}

/// Fault-domain classification of `err` (see [`ErrorKind`]).
#[uniffi::export]
pub fn actr_error_kind(err: ActrError) -> ErrorKind {
    err.kind()
}

/// `true` iff the error is in the Transient fault domain — safe to retry.
#[uniffi::export]
pub fn actr_error_is_retryable(err: ActrError) -> bool {
    err.is_retryable()
}

/// `true` iff the error is in the Corrupt fault domain — route to DLQ.
#[uniffi::export]
pub fn actr_error_requires_dlq(err: ActrError) -> bool {
    err.requires_dlq()
}

/// Test hook: fail with `err` so foreign-side error-buffer readers are
/// exercised against the real Rust UniFFI writer — the discriminant, field
/// order, and integer widths crossing the boundary are exactly the ones a
/// production throw produces. The Swift test calls this to prove its
/// `FfiConverterTypeActrError.read` lifts a Rust-generated case-13
/// `DeliveryUncertain` without losing `operation_id` or `cause`; the Kotlin
/// unit suite is JVM-only, so its `ActrErrorFfiReadTest` hand-builds the
/// same buffer layout instead of calling this hook.
#[uniffi::export]
pub fn actr_error_throw_for_test(err: ActrError) -> Result<(), ActrError> {
    Err(err)
}

impl From<actr_protocol::ActrError> for ActrError {
    fn from(e: actr_protocol::ActrError) -> Self {
        match e {
            actr_protocol::ActrError::Unavailable(msg) => ActrError::Unavailable { msg },
            actr_protocol::ActrError::ConnectionNotReady(info) => {
                ActrError::ConnectionNotReady { info: info.into() }
            }
            actr_protocol::ActrError::TimedOut => ActrError::TimedOut,
            actr_protocol::ActrError::DeliveryUncertain {
                operation_id,
                cause,
            } => ActrError::DeliveryUncertain {
                operation_id,
                cause_detail: cause,
            },
            actr_protocol::ActrError::NotFound(msg) => ActrError::NotFound { msg },
            actr_protocol::ActrError::PermissionDenied(msg) => ActrError::PermissionDenied { msg },
            actr_protocol::ActrError::InvalidArgument(msg) => ActrError::InvalidArgument { msg },
            actr_protocol::ActrError::UnknownRoute(msg) => ActrError::UnknownRoute { msg },
            actr_protocol::ActrError::DependencyNotFound {
                service_name,
                message,
            } => ActrError::DependencyNotFound {
                service_name,
                detail: message,
            },
            actr_protocol::ActrError::DecodeFailure(msg) => ActrError::DecodeFailure { msg },
            actr_protocol::ActrError::NotImplemented(msg) => ActrError::NotImplemented { msg },
            actr_protocol::ActrError::Internal(msg) => ActrError::Internal { msg },
            // `actr_protocol::ActrError` is `#[non_exhaustive]`: unmapped
            // future variants are surfaced as a generic Internal error so
            // they survive the FFI boundary rather than being lost.
            other => ActrError::Internal {
                msg: format!("unmapped actr error: {other}"),
            },
        }
    }
}

impl From<ActrError> for actr_protocol::ActrError {
    fn from(e: ActrError) -> Self {
        match e {
            ActrError::Unavailable { msg } => actr_protocol::ActrError::Unavailable(msg),
            ActrError::ConnectionNotReady { info } => {
                actr_protocol::ActrError::ConnectionNotReady(info.into())
            }
            ActrError::TimedOut => actr_protocol::ActrError::TimedOut,
            ActrError::DeliveryUncertain {
                operation_id,
                cause_detail,
            } => actr_protocol::ActrError::DeliveryUncertain {
                operation_id,
                cause: cause_detail,
            },
            ActrError::NotFound { msg } => actr_protocol::ActrError::NotFound(msg),
            ActrError::PermissionDenied { msg } => actr_protocol::ActrError::PermissionDenied(msg),
            ActrError::InvalidArgument { msg } => actr_protocol::ActrError::InvalidArgument(msg),
            ActrError::UnknownRoute { msg } => actr_protocol::ActrError::UnknownRoute(msg),
            ActrError::DependencyNotFound {
                service_name,
                detail,
            } => actr_protocol::ActrError::DependencyNotFound {
                service_name,
                message: detail,
            },
            ActrError::DecodeFailure { msg } => actr_protocol::ActrError::DecodeFailure(msg),
            ActrError::NotImplemented { msg } => actr_protocol::ActrError::NotImplemented(msg),
            ActrError::Internal { msg } => actr_protocol::ActrError::Internal(msg),
            // Binding-local Config has no direct protocol twin; classify as Client.
            ActrError::Config { msg } => actr_protocol::ActrError::InvalidArgument(msg),
        }
    }
}

/// Delegate to [`Classify`] on the protocol error for downstream crates that
/// want to treat this FFI error identically to the core error.
impl Classify for ActrError {
    fn kind(&self) -> ProtocolErrorKind {
        match self.kind() {
            ErrorKind::Transient => ProtocolErrorKind::Transient,
            ErrorKind::Client => ProtocolErrorKind::Client,
            ErrorKind::Internal => ProtocolErrorKind::Internal,
            ErrorKind::Corrupt => ProtocolErrorKind::Corrupt,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sdk_message_preserves_config_and_unicode_payloads() {
        let msg = "\u{4e2d}\u{6587}\n\"quoted\"\\path";
        assert_eq!(
            actr_error_message(ActrError::Config { msg: msg.into() }),
            format!("config: {msg}")
        );
        for (retry_after_ms, expected) in [
            (Some(42), "connection not ready: retry after 42ms"),
            (None, "connection not ready: no retry hint"),
        ] {
            let info = ConnectionNotReadyInfo { retry_after_ms };
            let proto: ProtoConnectionNotReadyInfo = info.clone().into();
            assert_eq!(proto.retry_after_ms, retry_after_ms);
            let back: ConnectionNotReadyInfo = proto.into();
            assert_eq!(back.retry_after_ms, retry_after_ms);
            assert_eq!(
                actr_error_message(ActrError::ConnectionNotReady { info }),
                expected
            );
        }
    }

    #[tokio::test]
    async fn dropping_run_on_tokio_runtime_future_aborts_task() {
        use std::sync::atomic::{AtomicBool, Ordering};

        // The inner task parks forever while holding a drop observer. Freeing
        // the wrapper future (what Kotlin coroutine cancellation does to the
        // UniFFI future) must abort the task — proving the observer ran —
        // instead of detaching it.
        struct ObserveDrop(std::sync::Arc<AtomicBool>);
        impl Drop for ObserveDrop {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }

        let dropped = std::sync::Arc::new(AtomicBool::new(false));
        let observe = ObserveDrop(std::sync::Arc::clone(&dropped));
        let wrapper = run_on_tokio_runtime("test", async move {
            let _observe = observe;
            futures_util::future::pending::<()>().await;
            #[allow(unreachable_code)]
            Ok::<(), ActrError>(())
        });
        let mut wrapper = Box::pin(wrapper);

        // Drive once so the inner task spawns and parks.
        let _ = tokio::time::timeout(std::time::Duration::from_millis(10), wrapper.as_mut()).await;

        drop(wrapper);

        for _ in 0..100 {
            if dropped.load(Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
        assert!(
            dropped.load(Ordering::SeqCst),
            "inner task must be aborted on wrapper drop, not detached"
        );
    }

    #[test]
    fn roundtrip_preserves_every_protocol_variant() {
        let cases = [
            actr_protocol::ActrError::Unavailable("u".into()),
            actr_protocol::ActrError::ConnectionNotReady(
                actr_protocol::ConnectionNotReadyInfo::new(0, 6000),
            ),
            actr_protocol::ActrError::TimedOut,
            actr_protocol::ActrError::NotFound("nf".into()),
            actr_protocol::ActrError::PermissionDenied("pd".into()),
            actr_protocol::ActrError::InvalidArgument("ia".into()),
            actr_protocol::ActrError::UnknownRoute("ur".into()),
            actr_protocol::ActrError::DependencyNotFound {
                service_name: "svc".into(),
                message: "m".into(),
            },
            actr_protocol::ActrError::DecodeFailure("df".into()),
            actr_protocol::ActrError::NotImplemented("ni".into()),
            actr_protocol::ActrError::Internal("int".into()),
            actr_protocol::ActrError::DeliveryUncertain {
                operation_id: "op-1".into(),
                cause: "write returned after close".into(),
            },
        ];

        for original in cases {
            let ffi: ActrError = original.clone().into();
            assert_eq!(
                actr_error_message(original.clone().into()),
                original.to_string()
            );
            let back: actr_protocol::ActrError = ffi.into();
            assert_eq!(format!("{original}"), format!("{back}"));
        }
    }

    #[test]
    fn kind_classification_matches_protocol() {
        assert_eq!(
            ActrError::Unavailable { msg: "x".into() }.kind(),
            ErrorKind::Transient,
        );
        assert_eq!(
            ActrError::ConnectionNotReady {
                info: ConnectionNotReadyInfo {
                    retry_after_ms: Some(6000),
                },
            }
            .kind(),
            ErrorKind::Transient,
        );
        assert_eq!(ActrError::TimedOut.kind(), ErrorKind::Transient);
        assert_eq!(
            ActrError::DeliveryUncertain {
                operation_id: "op-1".into(),
                cause_detail: "closed after write".into(),
            }
            .kind(),
            ErrorKind::Internal,
        );
        assert_eq!(
            ActrError::NotFound { msg: "x".into() }.kind(),
            ErrorKind::Client,
        );
        assert_eq!(
            ActrError::DecodeFailure { msg: "x".into() }.kind(),
            ErrorKind::Corrupt,
        );
        assert_eq!(
            ActrError::Internal { msg: "x".into() }.kind(),
            ErrorKind::Internal,
        );
        assert_eq!(
            ActrError::Config { msg: "x".into() }.kind(),
            ErrorKind::Client,
        );
    }

    #[test]
    fn retry_and_dlq_predicates() {
        assert!(ActrError::Unavailable { msg: "x".into() }.is_retryable());
        assert!(
            ActrError::ConnectionNotReady {
                info: ConnectionNotReadyInfo {
                    retry_after_ms: Some(6000),
                },
            }
            .is_retryable()
        );
        assert!(!ActrError::NotFound { msg: "x".into() }.is_retryable());
        assert!(
            !ActrError::DeliveryUncertain {
                operation_id: "op-1".into(),
                cause_detail: "closed after write".into(),
            }
            .is_retryable()
        );
        assert!(ActrError::DecodeFailure { msg: "x".into() }.requires_dlq());
        assert!(!ActrError::Internal { msg: "x".into() }.requires_dlq());
    }
}
