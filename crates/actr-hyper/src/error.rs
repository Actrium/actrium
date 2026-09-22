use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum HyperError {
    /// Signed manifest section not found in package
    #[error("package manifest section not found")]
    ManifestNotFound,

    /// Invalid manifest data format
    #[error("invalid manifest: {0}")]
    InvalidManifest(String),

    /// binary_hash does not match recomputed result, package has been tampered with
    #[error("binary hash mismatch: package integrity check failed")]
    BinaryHashMismatch,

    /// publisher signature verification failed
    #[error("signature verification failed: {0}")]
    SignatureVerificationFailed(String),

    /// publisher certificate is untrusted (not registered with actrix or revoked)
    #[error("untrusted publisher: {0}")]
    UntrustedPublisher(String),

    /// Issuer registration bootstrap failed
    #[error("Issuer bootstrap failed: {0}")]
    IssuerBootstrapFailed(String),

    /// Storage layer error
    #[error("storage error: {0}")]
    Storage(String),

    /// Configuration error
    #[error("config error: {0}")]
    Config(String),

    /// Namespace template variable missing
    #[error("namespace template variable `{0}` not available")]
    TemplateVariable(String),

    /// Runtime management error (spawn failure, process crash, etc.)
    #[error("runtime error: {0}")]
    Runtime(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub(crate) type HyperResult<T> = Result<T, HyperError>;
