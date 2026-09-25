//! AId token issuer.
//!
//! # Responsibilities
//!
//! Handles `RegisterRequest` and builds `RegisterResponse`, including:
//! - Serial number allocation with Snowflake.
//! - Signed access credential issuance.
//! - Time-limited TURN credential generation.
//! - Renewal token issuance and hash persistence.
//!
//! # Signing keys
//!
//! Each Realm's credentials are signed with that Realm's issuer key,
//! `issuer:<realm>` in the Signer ([`AIdIssuer::realm_key`]), the key its Realm
//! Profile lists under `issuer.keys`. Named keys do not expire; the private key
//! never leaves the Signer, which signs through its Sign RPC. Verifiers cache
//! the verifying key under `(realm, key_id)`. When a Realm is retired its key
//! is retired in the Signer and never signs again.
//!
//! # Example
//!
//! ```no_run
//! use issuer::issuer::{AIdIssuer, IssuerConfig};
//! use issuer::signer_client_wrapper::{SignerClientWrapper, create_signer_client};
//! use platform::config::signer::SignerClientConfig;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let signer_config = SignerClientConfig {
//!     endpoint: "http://localhost:8080".to_string(),
//!     timeout_seconds: 30,
//!     enable_tls: false,
//!     tls_domain: None,
//!     ca_cert: None,
//!     client_cert: None,
//!     client_key: None,
//! };
//! let signer_client = create_signer_client(&signer_config, "shared-key").await?;
//! let config = IssuerConfig::default();
//!
//! let issuer = AIdIssuer::new(signer_client, config, tokio_util::sync::CancellationToken::new()).await?;
//!
//! // Process a registration request
//! // let response = issuer.issue_credential(&request).await?;
//! # Ok(())
//! # }
//! ```

use crate::signer_client_wrapper::SignerClientWrapper;
use crate::sn::{AIdSerialNumberIssuer, SerialNumber};

// ========== Constants ==========

const PUBLISHER_SIGNED_AT_MAX_AGE_SECS: u64 = 5 * 60;
const PUBLISHER_SIGNED_AT_FUTURE_SKEW_SECS: u64 = 60;
const PUBLISHER_NONCE_TTL_SECS: u64 = 10 * 60;

use actr_protocol::{
    AIdCredential, ActrId, ActrType, ErrorResponse, IdentityClaims, RegisterRequest,
    RegisterResponse, register_response,
};
use base64::prelude::*;
use ed25519_dalek::VerifyingKey;
use hmac::{Hmac, Mac};
use platform::aid::AidError;
use platform::realm::RealmSecretCheck;
use prost::Message as ProstMessage;
use prost::bytes::Bytes;
use prost_types::Timestamp;
use sha1::Sha1;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

type HmacSha1 = Hmac<Sha1>;

/// Wire code carried in `RegisterResponse::Error` when a `/reissue` caller fails
/// to prove possession of the requested `ActrId` (a missing, unknown, or
/// grace-expired binding proof).
///
/// Deliberately distinct from the realm-denial `403`: a possession failure is a
/// per-actor identity problem, not a realm-membership decision. Conflating the
/// two behind `403` made the client treat a recoverable possession failure as a
/// terminal realm denial and permanently stop recovery. The hyper membership
/// controller maps this code to a retryable / identity-loss outcome, while `403`
/// stays reserved for a genuine realm denial that terminates automatic recovery.
pub const REISSUE_POSSESSION_NOT_PROVEN_CODE: u32 = 422;

fn aid_error_to_code(err: &AidError) -> u32 {
    match err {
        AidError::InvalidFormat
        | AidError::InvalidPrefix
        | AidError::EmptyId
        | AidError::InvalidTimestamp(_)
        | AidError::Base64DecodeError(_)
        | AidError::HexDecodeError(_) => 400,
        AidError::Expired => 401,
        AidError::RealmError(_) | AidError::PublisherNotVerified | AidError::PackageRevoked => 403,
        AidError::GenerationFailed(msg) => {
            if msg.contains("Signer unavailable") || msg.contains("Signer service") {
                503
            } else {
                500
            }
        }
        AidError::InvalidSignature(_) | AidError::DecodeFailure(_) => 500,
        // AidError is #[non_exhaustive]; future variants default to 5xx.
        _ => 500,
    }
}

/// Failure of a credential reissue attempt.
///
/// Possession failures are deliberately kept Issuer-internal instead of widening
/// [`AidError`]: that enum is publicly re-exported and exhaustive, so adding a
/// variant would break downstream matches. The endpoint boundary maps this to
/// the wire status instead.
enum ReissueFailure {
    /// Anything the shared credential machinery already classifies.
    Aid(AidError),
    /// The caller did not prove possession of the requested `ActrId`.
    PossessionNotProven,
}

impl From<AidError> for ReissueFailure {
    fn from(err: AidError) -> Self {
        Self::Aid(err)
    }
}

/// A possession proof that matched an issuer-recorded binding and has not
/// been consumed yet.
struct VerifiedPossession {
    proof_hash: Vec<u8>,
    binding_expires_at: i64,
    /// The token derived from the proof and its recorded expiry; its row, once
    /// written, marks the proof as consumed.
    successor: Vec<u8>,
}

/// A verified publisher-auth nonce and its scoping identity, returned from
/// [`verify_publisher_auth`] so the caller can consume (INSERT) the nonce at
/// the right point in the flow rather than inside the verify step. `/reissue`
/// consumes it in the transaction that commits the attempt, so a request that
/// never completed leaves the nonce unconsumed and the attempt retriable.
pub(crate) struct PublisherAuthBinding {
    pub(crate) publisher: String,
    pub(crate) signing_key_id: String,
    pub(crate) nonce: Vec<u8>,
}

/// Reissue work prepared without writing identity state. The attempt journal
/// commits it together with the response
/// (`ControlAttemptStore::complete_reissue`).
pub(crate) struct PreparedReissue {
    /// Signed credentials; the renewal token is attached on commit.
    pub(crate) register_ok: register_response::RegisterOk,
    pub(crate) successor_token: Vec<u8>,
    pub(crate) successor_hash: Vec<u8>,
    pub(crate) successor_expires_at: i64,
    pub(crate) proof_hash: Vec<u8>,
    pub(crate) binding_expires_at: i64,
    /// Package-auth reissue only: the nonce to consume and the target to record.
    pub(crate) publisher_nonce: Option<PublisherAuthBinding>,
    pub(crate) target: Option<String>,
}

/// Result of preparing a reissue.
pub(crate) enum ReissuePreparation {
    Ready(PreparedReissue),
    /// A stable rejection, kept as the attempt's response.
    Rejected(RegisterResponse),
    /// An infrastructure failure; the attempt is released so the same body can
    /// be retried.
    Retryable(RegisterResponse),
}

impl ReissueFailure {
    /// Map to the `(code, message)` pair carried in `RegisterResponse`.
    fn to_response(&self) -> (u32, String) {
        match self {
            Self::Aid(err) => (aid_error_to_code(err), err.to_string()),
            Self::PossessionNotProven => (
                REISSUE_POSSESSION_NOT_PROVEN_CODE,
                "actor identity possession not proven".to_string(),
            ),
        }
    }
}

/// AId Token issuer configuration.
#[derive(Debug, Clone)]
pub struct IssuerConfig {
    /// Stable Snowflake worker identity (0..=31).
    pub worker_id: u8,
    /// Token TTL (seconds).
    pub token_ttl_secs: u64,
    /// Signaling server heartbeat interval (seconds).
    pub signaling_heartbeat_interval_secs: u32,
    /// TURN shared secret (shared with the TURN server, used to generate time-limited credentials).
    pub turn_secret: String,
    /// Realm Provider ID bound into credentials and TURN usernames.
    pub provider_id: String,
    /// SQLite data directory (shared with signaling, used to write the verifying key
    /// so signaling can verify credentials).
    ///
    /// Issuer writes each Realm issuer key's verifying key to
    /// `signaling_key_cache.db` under this directory, allowing the signaling
    /// `AIdCredentialValidator` to verify credentials.
    pub sqlite_path: std::path::PathBuf,
    /// Renewal token TTL in seconds.
    pub renewal_token_ttl_secs: u64,
    /// Renewal rotation window in seconds.
    pub renewal_rotation_window_secs: u64,
    /// Base64-decoded renewal token secret (at least 32 bytes).
    pub renewal_token_secret: Vec<u8>,
}

impl Default for IssuerConfig {
    fn default() -> Self {
        Self {
            worker_id: 0,
            token_ttl_secs: 3600,                  // 1 hour
            signaling_heartbeat_interval_secs: 30, // 30 seconds
            turn_secret: String::new(),
            provider_id: String::new(),
            sqlite_path: std::path::PathBuf::from("."),
            renewal_token_ttl_secs: 86400,
            renewal_rotation_window_secs: 21600,
            renewal_token_secret: Vec::new(),
        }
    }
}

/// Current Unix time in seconds.
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// AId Token issuer - focused on issuing new Actor Identity Tokens
pub struct AIdIssuer {
    signer_client: SignerClientWrapper,
    /// Each Realm's issuer key (`issuer:<realm>` in the Signer), which signs
    /// that Realm's credentials: Signer key id and verifying key.
    realm_keys: Arc<RwLock<HashMap<String, (u32, VerifyingKey)>>>,
    /// Issuer configuration (public for renewal handler access).
    pub config: IssuerConfig,
}

impl AIdIssuer {
    /// Create a new AIdIssuer.
    pub async fn new(
        signer_client: SignerClientWrapper,
        config: IssuerConfig,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<Self, AidError> {
        if config.turn_secret.len() < 32 {
            return Err(AidError::GenerationFailed(
                "TURN secret must contain at least 32 bytes".to_string(),
            ));
        }
        if config.renewal_token_secret.len() < 32 {
            return Err(AidError::GenerationFailed(
                "renewal token secret must contain at least 32 bytes".to_string(),
            ));
        }
        SerialNumber::configure_worker_id(u64::from(config.worker_id))
            .map_err(|error| AidError::GenerationFailed(error.to_string()))?;

        let issuer = Self {
            signer_client,
            realm_keys: Arc::new(RwLock::new(HashMap::new())),
            config,
        };

        // Retirements are read from the Realm database, which callers
        // without one (isolated tests) do not set up.
        if !cancel.is_cancelled() && platform::storage::db::is_database_initialized() {
            tokio::spawn(follow_realm_retirements(
                issuer.signer_client.clone(),
                issuer.realm_keys.clone(),
                cancel,
            ));
        }

        Ok(issuer)
    }

    /// Process a register request and issue a credential.
    pub async fn issue_credential(
        &self,
        request: &RegisterRequest,
    ) -> Result<RegisterResponse, AidError> {
        self.issue_credential_with_realm_secret_check(request, None)
            .await
    }

    /// Process a register request and issue a credential, carrying the realm secret
    /// verification result completed by the handler.
    pub async fn issue_credential_with_realm_secret_check(
        &self,
        request: &RegisterRequest,
        realm_secret_check: Option<RealmSecretCheck>,
    ) -> Result<RegisterResponse, AidError> {
        match self
            .issue_credential_inner(request, realm_secret_check.as_ref())
            .await
        {
            Ok(register_ok) => Ok(RegisterResponse {
                result: Some(register_response::Result::Success(register_ok)),
            }),
            Err(err) => Ok(RegisterResponse {
                result: Some(register_response::Result::Error(ErrorResponse {
                    code: aid_error_to_code(&err),
                    message: err.to_string(),
                })),
            }),
        }
    }

    /// Re-authenticate a registration context and prepare fresh credentials
    /// for an existing actor identity, without writing identity state.
    ///
    /// This is deliberately separate from registration: callers must provide an
    /// `ActrId` whose realm and type exactly match the authenticated registration
    /// request, and no new serial number is allocated.
    ///
    /// `renewal_proof` is the caller's possession proof for exactly this
    /// `actor_id`: the raw renewal token most recently issued to that actor.
    /// Registration authentication alone only proves the caller may run this
    /// package/type in this realm — it must never be enough to mint
    /// credentials for an arbitrary serial number.
    pub(crate) async fn prepare_reissue(
        &self,
        actor_id: &ActrId,
        request: &RegisterRequest,
        renewal_proof: Option<&[u8]>,
        realm_secret_check: Option<RealmSecretCheck>,
    ) -> ReissuePreparation {
        match self
            .prepare_reissue_inner(
                actor_id,
                request,
                renewal_proof,
                realm_secret_check.as_ref(),
            )
            .await
        {
            Ok(prepared) => ReissuePreparation::Ready(prepared),
            Err(err) => {
                let (code, message) = err.to_response();
                let response = RegisterResponse {
                    result: Some(register_response::Result::Error(ErrorResponse {
                        code,
                        message,
                    })),
                };
                if code >= 500 {
                    ReissuePreparation::Retryable(response)
                } else {
                    ReissuePreparation::Rejected(response)
                }
            }
        }
    }

    async fn prepare_reissue_inner(
        &self,
        actor_id: &ActrId,
        request: &RegisterRequest,
        renewal_proof: Option<&[u8]>,
        realm_secret_check: Option<&RealmSecretCheck>,
    ) -> Result<PreparedReissue, ReissueFailure> {
        if actor_id.realm != request.realm {
            platform::recording::warn!(
                "Credential reissue Realm mismatch: actor={}, requested_realm={}",
                actor_id.to_string_repr(),
                request.realm
            );
            return Err(AidError::InvalidFormat.into());
        }

        // The publisher-auth nonce is only checked here; it is consumed when
        // the attempt commits.
        let publisher_nonce = self
            .verify_registration_identity(request, realm_secret_check)
            .await?;
        let proof = self
            .verify_actor_possession(actor_id, renewal_proof)
            .await?;
        // The requested type must be the one this identity was issued for.
        if Self::issued_actor(actor_id).await?.0 != request.actr_type {
            platform::recording::warn!(
                "Credential reissue type mismatch: actor={}, requested_type={}",
                actor_id.to_string_repr(),
                request.actr_type.to_string_repr()
            );
            return Err(AidError::InvalidFormat.into());
        }

        let register_ok = self
            .issue_credential_for_actor(actor_id, request.target.as_deref())
            .await?;

        // Reissue accepts proofs that already expired (anything inside the
        // binding grace window), so anchoring the successor on the proof's
        // expiry alone could mint an already-dead token.
        let anchor = proof.binding_expires_at.max(crate::renewal::now_secs());
        Ok(PreparedReissue {
            register_ok,
            successor_hash: crate::renewal::hash_token(&proof.successor),
            successor_token: proof.successor,
            successor_expires_at: crate::renewal::successor_expires_at(
                anchor,
                self.config.renewal_token_ttl_secs,
            ),
            proof_hash: proof.proof_hash,
            binding_expires_at: proof.binding_expires_at,
            publisher_nonce,
            target: request.target.clone(),
        })
    }

    /// Verify that the reissue caller actually owns `actor_id`.
    ///
    /// `verify_registration_identity` authenticates the workload context
    /// (package proof or realm authorization) but does not bind the caller to
    /// `actor_id.serial_number`. The binding proof is the renewal token Issuer
    /// itself issued to that actor: its hash must match an issuer-recorded
    /// token for exactly this `ActrId`. Expired tokens remain acceptable for
    /// [`crate::renewal::REISSUE_BINDING_GRACE_SECS`] so long-offline nodes can
    /// still recover their stable identity; unknown or unbound IDs are rejected
    /// with a distinct possession-failure code ([`REISSUE_POSSESSION_NOT_PROVEN_CODE`],
    /// not the realm-denial `403`) and no credential is signed.
    async fn verify_actor_possession(
        &self,
        actor_id: &ActrId,
        renewal_proof: Option<&[u8]>,
    ) -> Result<VerifiedPossession, ReissueFailure> {
        let Some(proof) = renewal_proof.filter(|proof| !proof.is_empty()) else {
            platform::recording::warn!(
                "Credential reissue rejected: missing possession proof for actor {}; refusing to sign for an unproven serial",
                actor_id.to_string_repr()
            );
            return Err(ReissueFailure::PossessionNotProven);
        };

        if proof.len() != 32 {
            platform::recording::warn!(
                "Credential reissue rejected: possession proof has invalid length {} for actor {}",
                proof.len(),
                actor_id.to_string_repr()
            );
            // A wrong-length proof cannot match any recorded binding, so it is
            // a possession failure (422) — same class as a missing/unknown
            // proof — not a malformed-request 400.
            return Err(ReissueFailure::PossessionNotProven);
        }

        let proof_hash = crate::renewal::hash_token(proof);
        let binding_expires_at = crate::renewal::find_binding_token_expiry(actor_id, &proof_hash)
            .await
            .map_err(|e| {
                AidError::GenerationFailed(format!("reissue binding lookup failed: {e}"))
            })?;

        let Some(binding_expires_at) = binding_expires_at else {
            // Never log the proof or its hash — treat them like credentials.
            platform::recording::warn!(
                "Credential reissue rejected: possession proof does not match any recorded binding for actor {}; possible cross-actor credential minting attempt",
                actor_id.to_string_repr()
            );
            return Err(ReissueFailure::PossessionNotProven);
        };

        // A proof is good for one reissue. The successor derived from it (the
        // same one `/renew` rotates to) marks it as consumed; a lost response
        // is recovered by retrying the same attempt, which replays the stored
        // response instead of reaching here. This keeps a leaked proof from
        // being chained into further successors.
        let successor = crate::renewal::derive_successor_token(
            &self.config.renewal_token_secret,
            proof,
            actor_id,
            binding_expires_at,
        );
        let consumed: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM issuer_renewal_token WHERE token_hash = ?1)",
        )
        .bind(crate::renewal::hash_token(&successor))
        .fetch_one(platform::storage::db::get_database().get_pool())
        .await
        .map_err(|e| {
            AidError::GenerationFailed(format!("reissue consumption lookup failed: {e}"))
        })?;
        if consumed {
            platform::recording::warn!(
                "Credential reissue rejected: possession proof already consumed for actor {}",
                actor_id.to_string_repr()
            );
            return Err(ReissueFailure::PossessionNotProven);
        }

        Ok(VerifiedPossession {
            proof_hash,
            binding_expires_at,
            successor,
        })
    }

    /// Internal processing logic (Ed25519 signing via the Signer Sign RPC).
    async fn issue_credential_inner(
        &self,
        request: &RegisterRequest,
        realm_secret_check: Option<&RealmSecretCheck>,
    ) -> Result<register_response::RegisterOk, AidError> {
        let nonce_binding = self
            .verify_registration_identity(request, realm_secret_check)
            .await?;
        // Fresh registration burns the publisher-auth nonce immediately after
        // verification, so a replayed /register is rejected within the nonce TTL.
        if let Some(binding) = nonce_binding {
            let now = i64::try_from(Self::unix_now_secs()?)
                .map_err(|_| AidError::InvalidTimestamp("current time exceeds i64".to_string()))?;
            let mut conn = platform::storage::db::get_database()
                .get_pool()
                .acquire()
                .await
                .map_err(|e| AidError::GenerationFailed(format!("database unavailable: {e}")))?;
            Self::consume_publisher_auth_nonce(
                &mut conn,
                &binding.publisher,
                &binding.signing_key_id,
                &binding.nonce,
                now,
            )
            .await?;
        }

        // Generate ActrId. Persist the package target so /renew and /reissue can
        // later recheck the exact (type_str, target) package row.
        let actr_id = self
            .generate_actr_id(
                &request.actr_type,
                &request.realm,
                request.target.as_deref(),
                if request.auth_mode == Some(actr_protocol::RegisterAuthMode::Linked as i32) {
                    actr_protocol::RegisterAuthMode::Linked
                } else {
                    actr_protocol::RegisterAuthMode::Package
                },
            )
            .await?;

        // Build IdentityClaims (proto type, plaintext).
        let (key_id, verifying_key) = self.realm_key(&actr_id.realm).await?;
        let expr_time = now_secs().saturating_add(self.config.token_ttl_secs);
        let claims_proto = IdentityClaims {
            realm: actr_id.realm.clone(),
            actor_id: actr_id.to_string_repr(),
            expires_at: expr_time,
            protocol_version: 1,
            issuer: actr_id.realm.clone(),
            provider: self.config.provider_id.clone(),
            audience: "actr.signaling".to_string(),
            actor_type: request.actr_type.clone(),
        };

        // Proto-encode the claims bytes.
        let claims_bytes = claims_proto.encode_to_vec();

        // Sign via the Signer Sign RPC (the private key never leaves the Signer).
        let signature_bytes = self
            .signer_client
            .sign(key_id, &claims_bytes)
            .await
            .map_err(|e| AidError::GenerationFailed(format!("Signer sign failed: {e}")))?;

        // Create an AIdCredential (Ed25519 format).
        let credential = AIdCredential {
            key_id,
            claims: Bytes::from(claims_bytes),
            signature: Bytes::from(signature_bytes),
        };

        // Create the expiry Timestamp.
        let credential_expires_at = Some(Timestamp {
            seconds: expr_time as i64,
            nanos: 0,
        });

        // Generate a time-limited TURN credential compatible with coturn --use-auth-secret.
        let turn_credential = self.generate_turn_credential(&actr_id, expr_time);

        let (renewal_token, renewal_token_expires_at) =
            self.issue_fresh_renewal_token(&actr_id).await?;

        Ok(register_response::RegisterOk {
            actr_id,
            credential,
            turn_credential,
            credential_expires_at,
            signaling_heartbeat_interval_secs: self.config.signaling_heartbeat_interval_secs,
            signing_pubkey: Bytes::from(verifying_key.as_bytes().to_vec()),
            signing_key_id: key_id,
            renewal_token: Some(renewal_token),
            renewal_token_expires_at: Some(renewal_token_expires_at),
        })
    }

    /// Verify the registration identity according to the explicit request source.
    ///
    /// Returns the publisher-auth nonce binding when the package-auth path was
    /// taken, so the caller can burn the nonce at the correct point in its flow
    /// (immediately for `/register`, in the transaction that commits the
    /// `/reissue` attempt). `None` for the Linked path (no single-use nonce).
    async fn verify_registration_identity(
        &self,
        request: &RegisterRequest,
        realm_secret_check: Option<&RealmSecretCheck>,
    ) -> Result<Option<PublisherAuthBinding>, AidError> {
        let auth_mode = match request.auth_mode {
            Some(value) if value == actr_protocol::RegisterAuthMode::Linked as i32 => {
                actr_protocol::RegisterAuthMode::Linked
            }
            Some(value) if value == actr_protocol::RegisterAuthMode::Package as i32 => {
                actr_protocol::RegisterAuthMode::Package
            }
            _ => actr_protocol::RegisterAuthMode::Unspecified,
        };

        match auth_mode {
            actr_protocol::RegisterAuthMode::Unspecified
            | actr_protocol::RegisterAuthMode::Package => {
                // Verify Publisher Registry identity through a published package, or through
                // signatures for an unpublished package.
                self.verify_publisher_identity(request).await
            }
            actr_protocol::RegisterAuthMode::Linked => {
                self.verify_linked_identity(request, realm_secret_check)?;
                Ok(None)
            }
        }
    }

    fn verify_linked_identity(
        &self,
        request: &RegisterRequest,
        realm_secret_check: Option<&RealmSecretCheck>,
    ) -> Result<(), AidError> {
        if request.actr_type.version.is_empty()
            || request.manifest_raw.is_some()
            || request.publisher_signature.is_some()
            || request.publisher_auth_signature.is_some()
            || request.publisher_auth_signed_at.is_some()
            || request.publisher_auth_nonce.is_some()
        {
            return Err(AidError::InvalidFormat);
        }

        // Linked workloads must not claim a DNS-style publisher namespace: dotted
        // names (e.g. "apps.example.com") are reserved for Package identities whose
        // domain ownership is cryptographically verified. A realm secret is not
        // proof of owning a publisher domain, so allowing a dotted publisher
        // here would let any realm workload impersonate a published publisher's
        // service type and overwrite its ACL (AIS-01 / SIG-05).
        if request.actr_type.publisher.contains('.') {
            platform::recording::warn!(
                "Linked registration rejected: DNS-style publisher namespace is reserved for verified publishers, realm={}, type={}",
                request.realm,
                request.actr_type.to_string_repr()
            );
            return Err(AidError::PublisherNotVerified);
        }

        match realm_secret_check {
            Some(RealmSecretCheck::ValidCurrent) | Some(RealmSecretCheck::ValidPrevious) => Ok(()),
            other => {
                platform::recording::warn!(
                    "Linked registration rejected: realm secret was not verified, realm={}, type={}, secret_check={:?}",
                    request.realm,
                    request.actr_type.to_string_repr(),
                    other
                );
                Err(AidError::PublisherNotVerified)
            }
        }
    }

    /// verify Publisher Registry identity
    ///
    /// Path 1: an existing package tuple. The active package must still have an
    /// active Publisher Registry and a non-revoked signing key, the request
    /// manifest must hash-match the stored one, and (AIS-02) the caller must
    /// prove current possession of the publisher's signing key with a fresh
    /// single-use nonce and a domain-separated `publisher_auth` signature —
    /// the same possession proof Path 2 requires. Revoked packages are terminal
    /// and cannot fall through.
    /// Path 2: only a never-published tuple may use manifest and request
    /// signatures plus the `publisher_auth` possession proof.
    /// Otherwise reject
    async fn verify_publisher_identity(
        &self,
        request: &RegisterRequest,
    ) -> Result<Option<PublisherAuthBinding>, AidError> {
        let actr_type = &request.actr_type;
        if actr_type.version.is_empty() {
            return Err(AidError::InvalidFormat);
        }

        let type_str = actr_type.to_program_identity();
        let publisher_name = &actr_type.publisher;

        // SECURITY / COMPATIBILITY: package registration must be bound to the
        // exact manifest bytes the publisher saw locally. Without
        // manifest_raw, Issuer can only prove that "some active package has this
        // type_str"; without target, a multi-target package lookup degrades to
        // type-only matching. Treat both as malformed package-auth requests
        // instead of falling back to a broad registry lookup.
        // This intentionally tightens the legacy Path 1 request shape.
        let pool = platform::storage::db::get_database().get_pool().clone();
        let target_ref = request
            .target
            .as_deref()
            .filter(|target| !target.is_empty())
            .ok_or_else(|| {
                platform::recording::warn!(
                    "Publisher Registry verification failed: package registration requires target; type_str={}",
                    type_str
                );
                AidError::InvalidFormat
            })?;
        let request_manifest = request
            .manifest_raw
            .as_ref()
            .filter(|manifest| !manifest.is_empty())
            .ok_or_else(|| {
                platform::recording::warn!(
                    "Publisher Registry verification failed: package registration requires manifest_raw; type_str={}, target={}",
                    type_str,
                    target_ref
                );
                AidError::InvalidFormat
            })?;

        let published_pkg =
            actrix_publisher_registry::model::ActrPackage::get_by_type_and_target_any_status(
                &pool, &type_str, target_ref,
            )
            .await
            .map_err(|e| {
                AidError::GenerationFailed(format!("Publisher Registry lookup failed: {e}"))
            })?;

        if let Some(pkg) = published_pkg {
            use sha2::{Digest, Sha256};

            if pkg.status == actrix_publisher_registry::model::PkgStatus::Revoked {
                platform::recording::warn!(
                    "Publisher Registry table lookup rejected: package revoked, type_str={}, target={}",
                    type_str,
                    target_ref
                );
                return Err(AidError::PackageRevoked);
            }

            if pkg.publisher != actr_type.publisher
                || pkg.name != actr_type.name
                || pkg.version != actr_type.version
                || pkg.target != target_ref
            {
                platform::recording::warn!(
                    "Publisher Registry table lookup rejected: package identity mismatch, type_str={}, target={}",
                    type_str,
                    target_ref
                );
                return Err(AidError::PublisherNotVerified);
            }

            let request_manifest_hash = Sha256::digest(request_manifest.as_ref());
            let stored_manifest_hash = Sha256::digest(pkg.manifest.as_bytes());
            if request_manifest_hash[..] != stored_manifest_hash[..] {
                platform::recording::warn!(
                    "Publisher Registry table lookup rejected: manifest hash mismatch, type_str={}, target={}",
                    type_str,
                    target_ref
                );
                // Do not fall through to Path 2 when the published tuple exists.
                // Rejecting here prevents a mismatched or stale manifest from
                // downgrading into the unpublished-package authentication path.
                return Err(AidError::PublisherNotVerified);
            }

            let manager = actrix_publisher_registry::PublisherRegistryManager::new(pool.clone());
            let package_signing_key_id = manager
                .verify_published_package_signing_key(&pkg)
                .await
                .map_err(Self::map_publisher_verification_error)?;

            // AIS-02: an existing published package, the realm secret, and the
            // public manifest are no longer sufficient to register — all of them
            // are readable/holdable without controlling the publisher. The caller
            // must additionally prove current possession of the publisher's
            // signing key, exactly as the unpublished-package path (Path 2) does:
            // a fresh single-use nonce plus a domain-separated signature bound to
            // this realm, actr_type, target, and manifest hash. The manifest-hash
            // check above additionally pins that proof to the exact published
            // version.
            let (publisher_auth_signature, publisher_auth_signed_at, publisher_auth_nonce) = match (
                &request.publisher_auth_signature,
                request.publisher_auth_signed_at,
                &request.publisher_auth_nonce,
            ) {
                (Some(signature), Some(signed_at), Some(nonce)) => (signature, signed_at, nonce),
                _ => {
                    platform::recording::warn!(
                        "Publisher Registry table lookup rejected: published package registration requires publisher_auth_signature, publisher_auth_signed_at, and publisher_auth_nonce; type_str={}, target={}",
                        type_str,
                        target_ref
                    );
                    return Err(AidError::PublisherNotVerified);
                }
            };

            // Verify possession against the publisher's CURRENT signing key: a
            // publisher that has rotated its key must still be able to launch
            // instances of an already-published package, and control of the
            // current key is the strongest possession proof available. This can
            // differ from `package_signing_key_id`, which is the (possibly
            // retired) key that signed the stored manifest.
            let publisher = manager
                .resolve_by_name(publisher_name)
                .await
                .map_err(Self::map_publisher_verification_error)?;

            let manifest_sha256_hex = hex::encode(request_manifest_hash);
            let binding = Self::verify_publisher_auth(
                &pool,
                &publisher.public_key,
                publisher_name,
                &publisher.key_id,
                &request.realm,
                actr_type,
                target_ref,
                &manifest_sha256_hex,
                publisher_auth_signature.as_ref(),
                publisher_auth_signed_at,
                publisher_auth_nonce.as_ref(),
            )
            .await?;

            platform::recording::debug!(
                "Publisher Registry table lookup passed, type_str={}, target={}, package_signing_key_id={}, publisher_key_id={}",
                type_str,
                target_ref,
                package_signing_key_id,
                publisher.key_id
            );
            return Ok(Some(binding));
        }

        // Path 2: not in table -> verify package signature + publisher authorization
        // (own package, not yet published).
        let (
            manifest_bytes,
            publisher_signature,
            publisher_auth_signature,
            publisher_auth_signed_at,
            publisher_auth_nonce,
        ) = match (
            &request.manifest_raw,
            &request.publisher_signature,
            &request.publisher_auth_signature,
            request.publisher_auth_signed_at,
            &request.publisher_auth_nonce,
        ) {
            (
                Some(manifest_bytes),
                Some(publisher_signature),
                Some(publisher_auth_signature),
                Some(publisher_auth_signed_at),
                Some(publisher_auth_nonce),
            ) => (
                manifest_bytes,
                publisher_signature,
                publisher_auth_signature,
                publisher_auth_signed_at,
                publisher_auth_nonce,
            ),
            _ => {
                platform::recording::warn!(
                    "Publisher Registry verification failed: unpublished package requires manifest_raw, publisher_signature, publisher_auth_signature, publisher_auth_signed_at, and publisher_auth_nonce; type_str={}",
                    type_str
                );
                return Err(AidError::PublisherNotVerified);
            }
        };

        // Parse manifest using standard TOML. Reject if not valid UTF-8/TOML —
        // unparseable manifests must not bypass the identity binding check below.
        let manifest_str = std::str::from_utf8(manifest_bytes.as_ref()).map_err(|_| {
            platform::recording::warn!("Publisher Registry manifest_raw is not valid UTF-8");
            AidError::InvalidFormat
        })?;
        let manifest_toml: toml::Value = manifest_str.parse().map_err(|e| {
            platform::recording::warn!("Publisher Registry manifest_raw is not valid TOML: {}", e);
            AidError::InvalidFormat
        })?;

        // Extract signing_key_id — mandatory for all modern packages.
        // Without it, we cannot reliably resolve the correct Publisher Registry key after rotation.
        let signing_key_id = manifest_toml
            .get("signing_key_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                platform::recording::warn!(
                    "Publisher Registry manifest_raw missing 'signing_key_id', publisher={}",
                    publisher_name
                );
                AidError::InvalidFormat
            })?
            .to_string();

        // Look up the publisher's public key by key_id (supports current + historical)
        let manager = actrix_publisher_registry::PublisherRegistryManager::new(pool.clone());
        let publisher_info = manager
            .resolve_key_by_id(publisher_name, &signing_key_id)
            .await
            .map_err(Self::map_publisher_verification_error)?;

        // Verify package manifest signature using the publisher's public key.
        let sig_b64 = base64::prelude::BASE64_STANDARD.encode(publisher_signature.as_ref());
        let valid = actrix_publisher_registry::crypto::verify_signature(
            manifest_bytes.as_ref(),
            &sig_b64,
            &publisher_info.public_key,
        )
        .map_err(|e| {
            platform::recording::warn!(
                "Publisher Registry signature verification error: publisher={}, err={}",
                publisher_name,
                e
            );
            AidError::GenerationFailed(format!("Signature verification error: {e}"))
        })?;

        if !valid {
            platform::recording::warn!(
                "Publisher Registry signature verification failed: invalid signature, type_str={}",
                type_str
            );
            return Err(AidError::PublisherNotVerified);
        }

        // SECURITY: Verify that manifest identity matches the RegisterRequest.
        // Without this check, an attacker holding any valid signed manifest from
        // the same Publisher Registry could reuse it to register a different actr_type/target.
        let m_publisher = manifest_toml
            .get("publisher")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let m_name = manifest_toml
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let m_version = manifest_toml
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let m_target = manifest_toml
            .get("binary")
            .and_then(|b| b.get("target"))
            .and_then(|v| v.as_str());

        // Check publisher/name/version match actr_type.
        if m_publisher != actr_type.publisher
            || m_name != actr_type.name
            || m_version != actr_type.version
        {
            platform::recording::warn!(
                "Publisher manifest identity mismatch: manifest={}/{}@{}, request={}",
                m_publisher,
                m_name,
                m_version,
                type_str
            );
            return Err(AidError::PublisherNotVerified);
        }

        // The Publisher Registry-signed manifest must declare binary.target. Falling back to
        // request.target would leave the manifest signature unbound to target,
        // letting one signed manifest be registered under arbitrary targets.
        // Mirror Path 1, where the registry row's target is authoritative.
        let manifest_target = m_target.ok_or_else(|| {
            platform::recording::warn!(
                "Publisher Registry manifest missing binary.target; type_str={}",
                type_str
            );
            AidError::InvalidFormat
        })?;

        // request.target is guaranteed non-empty by the entry guard; ensure it
        // matches the target the Publisher Registry signed into the manifest.
        if let Some(req_target) = request.target.as_deref() {
            if req_target != manifest_target {
                platform::recording::warn!(
                    "Publisher Registry manifest target mismatch: manifest={}, request={}",
                    manifest_target,
                    req_target
                );
                return Err(AidError::PublisherNotVerified);
            }
        }
        let manifest_sha256_hex = {
            use sha2::{Digest, Sha256};
            hex::encode(Sha256::digest(manifest_bytes.as_ref()))
        };
        let binding = Self::verify_publisher_auth(
            &pool,
            &publisher_info.public_key,
            publisher_name,
            &signing_key_id,
            &request.realm,
            actr_type,
            manifest_target,
            &manifest_sha256_hex,
            publisher_auth_signature.as_ref(),
            publisher_auth_signed_at,
            publisher_auth_nonce.as_ref(),
        )
        .await?;

        platform::recording::debug!(
            "Publisher Registry manifest and publisher signature verification passed, type_str={}",
            type_str
        );
        Ok(Some(binding))
    }

    fn map_publisher_verification_error(
        error: actrix_publisher_registry::PublisherRegistryError,
    ) -> AidError {
        let message = error.to_string();
        match error {
            actrix_publisher_registry::PublisherRegistryError::NotFound
            | actrix_publisher_registry::PublisherRegistryError::KeyRevoked(_)
            | actrix_publisher_registry::PublisherRegistryError::InvalidStatus(_)
            | actrix_publisher_registry::PublisherRegistryError::CertificateExpired
            | actrix_publisher_registry::PublisherRegistryError::InvalidSignature
            | actrix_publisher_registry::PublisherRegistryError::Unauthorized
            | actrix_publisher_registry::PublisherRegistryError::InvalidRequest(_) => {
                platform::recording::warn!("Publisher Registry verification rejected: {}", message);
                AidError::PublisherNotVerified
            }
            other => {
                platform::recording::warn!(
                    "Publisher Registry verification failed internally: {}",
                    other
                );
                AidError::GenerationFailed(format!(
                    "Publisher Registry verification failed: {other}"
                ))
            }
        }
    }

    fn unix_now_secs() -> Result<u64, AidError> {
        crate::renewal::try_now_secs()
            .map_err(|e| AidError::InvalidTimestamp(format!("system clock before Unix epoch: {e}")))
    }

    fn verify_publisher_auth_signed_at(signed_at: u64, now: u64) -> Result<(), AidError> {
        if signed_at > now.saturating_add(PUBLISHER_SIGNED_AT_FUTURE_SKEW_SECS) {
            platform::recording::warn!(
                "publisher_auth_signed_at is too far in the future: signed_at={}, now={}, future_skew_secs={}",
                signed_at,
                now,
                PUBLISHER_SIGNED_AT_FUTURE_SKEW_SECS
            );
            return Err(AidError::InvalidTimestamp(
                "publisher_auth_signed_at is too far in the future".to_string(),
            ));
        }

        if now.saturating_sub(signed_at) > PUBLISHER_SIGNED_AT_MAX_AGE_SECS {
            platform::recording::warn!(
                "publisher_auth_signed_at outside max age: signed_at={}, now={}, max_age_secs={}",
                signed_at,
                now,
                PUBLISHER_SIGNED_AT_MAX_AGE_SECS
            );
            return Err(AidError::Expired);
        }

        Ok(())
    }

    /// Verify a publisher-possession proof (`publisher_auth`) for package
    /// registration and check — but not consume — its one-time nonce.
    ///
    /// Shared by both package registration paths. Whether or not a registry row
    /// already exists, an authorized publisher launch is distinguished from a
    /// caller who merely holds the realm secret and the public manifest by a
    /// fresh single-use nonce plus a domain-separated signature binding this
    /// realm, actr_type, target, and manifest hash to the publisher's signing
    /// key. `signing_key_id` scopes the nonce and is recorded for audit.
    ///
    /// The nonce is **not** burned here. Verification only checks that an
    /// unexpired row does not already exist for this nonce. The caller is
    /// responsible for consuming the nonce (INSERT) at the correct point:
    /// `/register` burns it immediately after this call returns; `/reissue`
    /// defers the burn to the transaction that commits the attempt, so a
    /// request that never completed leaves the attempt retriable. The returned
    /// [`PublisherAuthBinding`] carries the identity needed for that
    /// deferred INSERT.
    #[allow(clippy::too_many_arguments)]
    async fn verify_publisher_auth(
        pool: &sqlx::SqlitePool,
        publisher_public_key: &str,
        publisher_name: &str,
        signing_key_id: &str,
        realm: &str,
        actr_type: &ActrType,
        target: &str,
        manifest_sha256_hex: &str,
        publisher_auth_signature: &[u8],
        publisher_auth_signed_at: u64,
        publisher_auth_nonce: &[u8],
    ) -> Result<PublisherAuthBinding, AidError> {
        if !matches!(publisher_auth_nonce.len(), 16 | 32) {
            platform::recording::warn!(
                "publisher_auth_nonce has invalid length: len={}, publisher={}",
                publisher_auth_nonce.len(),
                publisher_name
            );
            return Err(AidError::InvalidFormat);
        }

        let now = Self::unix_now_secs()?;
        Self::verify_publisher_auth_signed_at(publisher_auth_signed_at, now)?;

        let publisher_payload = actr_protocol::build_publisher_register_payload(
            actr_protocol::PublisherRegisterPayload {
                realm,
                actr_type,
                target,
                manifest_sha256_hex,
                publisher_auth_signed_at,
                publisher_auth_nonce,
            },
        );
        let publisher_sig_b64 = base64::prelude::BASE64_STANDARD.encode(publisher_auth_signature);
        let publisher_valid = actrix_publisher_registry::crypto::verify_signature(
            publisher_payload.as_bytes(),
            &publisher_sig_b64,
            publisher_public_key,
        )
        .map_err(|e| {
            platform::recording::warn!(
                "publisher_auth_signature verification error: publisher={}, key_id={}, err={}",
                publisher_name,
                signing_key_id,
                e
            );
            AidError::GenerationFailed(format!("Publisher signature verification error: {e}"))
        })?;

        if !publisher_valid {
            platform::recording::warn!(
                "publisher_auth_signature verification failed: publisher={}, key_id={}, actr_type={}",
                publisher_name,
                signing_key_id,
                actr_type.to_string_repr()
            );
            return Err(AidError::PublisherNotVerified);
        }

        // Check-only — reject an already-consumed (still-unexpired) nonce
        // without burning it. Consumption is deferred to the caller's commit
        // transaction. An expired nonce row is treated as unused: the GC in
        // `consume_publisher_auth_nonce` reclaims it.
        let now_i64 = i64::try_from(now)
            .map_err(|_| AidError::InvalidTimestamp("current time exceeds i64".to_string()))?;
        let already_consumed: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM issuer_publisher_auth_nonce
             WHERE publisher = ?1 AND key_id = ?2 AND nonce = ?3 AND expires_at > ?4)",
        )
        .bind(publisher_name)
        .bind(signing_key_id)
        .bind(publisher_auth_nonce)
        .bind(now_i64)
        .fetch_one(pool)
        .await
        .map_err(|e| AidError::GenerationFailed(format!("publisher nonce check failed: {e}")))?;
        if already_consumed {
            platform::recording::warn!(
                "publisher_auth_nonce replay detected during verify: publisher={}, key_id={}",
                publisher_name,
                signing_key_id
            );
            return Err(AidError::PublisherNotVerified);
        }

        Ok(PublisherAuthBinding {
            publisher: publisher_name.to_string(),
            signing_key_id: signing_key_id.to_string(),
            nonce: publisher_auth_nonce.to_vec(),
        })
    }

    /// Consume a publisher-auth nonce; `PublisherNotVerified` if it already was.
    pub(crate) async fn consume_publisher_auth_nonce(
        conn: &mut sqlx::SqliteConnection,
        publisher: &str,
        signing_key_id: &str,
        publisher_auth_nonce: &[u8],
        now: i64,
    ) -> Result<(), AidError> {
        let now_i64 = now;
        let expires_at = now_i64.saturating_add(PUBLISHER_NONCE_TTL_SECS as i64);

        sqlx::query("DELETE FROM issuer_publisher_auth_nonce WHERE expires_at < ?")
            .bind(now_i64)
            .execute(&mut *conn)
            .await
            .map_err(|e| {
                AidError::GenerationFailed(format!("publisher nonce cleanup failed: {e}"))
            })?;

        let result = sqlx::query(
            "INSERT INTO issuer_publisher_auth_nonce (publisher, key_id, nonce, created_at, expires_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(publisher)
        .bind(signing_key_id)
        .bind(publisher_auth_nonce)
        .bind(now_i64)
        .bind(expires_at)
        .execute(&mut *conn)
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(sqlx::Error::Database(err)) if err.is_unique_violation() => {
                platform::recording::warn!(
                    "publisher_auth_nonce replay detected: publisher={}, key_id={}",
                    publisher,
                    signing_key_id
                );
                Err(AidError::PublisherNotVerified)
            }
            Err(err) => Err(AidError::GenerationFailed(format!(
                "publisher nonce consume failed: {err}"
            ))),
        }
    }

    /// Generate ActrId.
    async fn generate_actr_id(
        &self,
        actr_type: &ActrType,
        realm: &str,
        target: Option<&str>,
        auth_mode: actr_protocol::RegisterAuthMode,
    ) -> Result<ActrId, AidError> {
        let pool = platform::storage::db::get_database().get_pool();
        let issued_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let actor_type = actr_type.to_program_identity();

        // A collision can only occur after a worker lease error, process
        // restart in the same millisecond, or clock rollback. Never overwrite
        // the earlier identity: advance Snowflake state and retry.
        for _ in 0..1024 {
            let serial_number = SerialNumber::sn().value();
            let reserved = sqlx::query(
                "INSERT OR IGNORE INTO issued_actor_identity
                 (realm, serial_number, actor_type, target, issued_at, auth_mode)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(realm)
            .bind(serial_number as i64)
            .bind(&actor_type)
            .bind(target)
            .bind(issued_at)
            .bind(auth_mode as i32)
            .execute(pool)
            .await
            .map_err(|error| {
                AidError::GenerationFailed(format!(
                    "failed to reserve Actor instance identity: {error}"
                ))
            })?
            .rows_affected();
            if reserved == 1 {
                return Ok(ActrId {
                    realm: realm.to_string(),
                    serial_number,
                });
            }
        }

        Err(AidError::GenerationFailed(
            "failed to reserve a unique Actor ID after 1024 Snowflake attempts".to_string(),
        ))
    }

    /// The Realm's current issuer key (`realm-profile.md` section 6.1),
    /// created in the Signer on first use. Its verifying key is written to
    /// the credential validator's cache under the Realm, so it verifies only
    /// that Realm's credentials.
    pub async fn realm_key(&self, realm: &str) -> Result<(u32, VerifyingKey), AidError> {
        let current = crate::realm_profile::current_issuer_key(&self.signer_client, realm)
            .await
            .map_err(|e| AidError::GenerationFailed(format!("Realm issuer key: {e}")))?;
        if let Some(key) = self.realm_keys.read().await.get(realm)
            && key.0 == current.key_id
        {
            return Ok(*key);
        }
        let public_key: [u8; 32] = base64::prelude::BASE64_STANDARD
            .decode(&current.public_key)
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or_else(|| AidError::GenerationFailed("invalid Realm issuer key".to_string()))?;
        let verifying_key = VerifyingKey::from_bytes(&public_key).map_err(|e| {
            AidError::GenerationFailed(format!("Invalid verifying key from Signer: {e}"))
        })?;
        platform::aid::credential::validator::AIdCredentialValidator::persist_key(
            &self.config.sqlite_path,
            realm,
            current.key_id,
            &verifying_key,
            0,
        )
        .await?;
        self.realm_keys
            .write()
            .await
            .insert(realm.to_string(), (current.key_id, verifying_key));
        Ok((current.key_id, verifying_key))
    }

    /// Compute the renewal token expiry time (Unix timestamp, seconds, i64 for SQLite).
    fn calculate_renewal_expiry_time(&self) -> i64 {
        (SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + self.config.renewal_token_ttl_secs) as i64
    }

    /// Generate a 32-byte renewal token using a CSPRNG.
    fn generate_renewal_token(&self) -> Vec<u8> {
        let mut token = vec![0u8; 32];
        use rand::RngCore;
        rand::thread_rng().fill_bytes(&mut token);
        token
    }

    async fn issue_fresh_renewal_token(
        &self,
        actor_id: &ActrId,
    ) -> Result<(Bytes, Timestamp), AidError> {
        if self.config.renewal_token_secret.is_empty() {
            return Err(AidError::GenerationFailed(
                "renewal_token_secret is required".to_string(),
            ));
        }

        let token = self.generate_renewal_token();
        let expires_at = self.calculate_renewal_expiry_time();
        let token_hash = crate::renewal::hash_token(&token);
        crate::renewal::insert_renewal_token(actor_id, &token_hash, expires_at)
            .await
            .map_err(|e| {
                AidError::GenerationFailed(format!("Failed to persist renewal token: {e}"))
            })?;

        let _ = crate::renewal::gc_expired_tokens().await;
        crate::renewal::trim_oldest_tokens_for_actor(actor_id)
            .await
            .map_err(|e| {
                AidError::GenerationFailed(format!("Failed to trim renewal tokens: {e}"))
            })?;

        Ok((
            Bytes::from(token),
            Timestamp {
                seconds: expires_at,
                nanos: 0,
            },
        ))
    }

    /// The package target recorded for `actor_id`: from registration, or the
    /// last package-auth reissue. A NULL target (legacy row) degrades the
    /// package recheck to type-only matching.
    pub(crate) async fn issued_target(actor_id: &ActrId) -> Option<String> {
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT target FROM issued_actor_identity WHERE realm = ? AND serial_number = ?",
        )
        .bind(&actor_id.realm)
        .bind(actor_id.serial_number as i64)
        .fetch_one(platform::storage::db::get_database().get_pool())
        .await
        .unwrap_or_else(|err| {
            platform::recording::warn!(
                "Failed to look up target for actor {}: {}",
                actor_id.to_string_repr(),
                err
            );
            None
        })
    }

    /// Recheck, before a stored credential is replayed, that `actor_id` may
    /// still receive credentials: a replay never hands out what the Issuer
    /// would refuse now.
    pub(crate) async fn recheck_before_replay(&self, actor_id: &ActrId) -> Result<(), AidError> {
        let target = Self::issued_target(actor_id).await;
        self.recheck_existing_actor(actor_id, target.as_deref())
            .await
            .map(|_| ())
    }

    /// Load the Actor type and auth mode recorded when `actor_id` was issued.
    async fn issued_actor(actor_id: &ActrId) -> Result<(ActrType, Option<i32>), AidError> {
        let row: Option<(String, Option<i32>)> = sqlx::query_as(
            "SELECT actor_type, auth_mode FROM issued_actor_identity
             WHERE realm = ? AND serial_number = ?",
        )
        .bind(&actor_id.realm)
        .bind(actor_id.serial_number as i64)
        .fetch_optional(platform::storage::db::get_database().get_pool())
        .await
        .map_err(|e| AidError::GenerationFailed(format!("actor identity lookup failed: {e}")))?;
        let Some((actor_type, auth_mode)) = row else {
            platform::recording::warn!(
                "Credential issuance rejected: actor {} was not issued by this Realm",
                actor_id.to_string_repr()
            );
            return Err(AidError::InvalidFormat);
        };
        let actor_type = ActrType::from_string_repr(&actor_type).map_err(|e| {
            AidError::GenerationFailed(format!("stored actor type is invalid: {e}"))
        })?;
        Ok((actor_type, auth_mode))
    }

    /// Re-validate that an existing actor may still receive credentials.
    ///
    /// Runs on the renew and reissue paths only (never on fresh registration,
    /// which allocates a brand-new serial that cannot be revoked yet). The renew
    /// path previously trusted the renewal token alone and never re-checked the
    /// surrounding realm/package/publisher state, so a workload that lost its
    /// authorization after issuance could keep renewing indefinitely. Three
    /// lightweight checks, in order:
    ///
    /// - AIS-04: the actor must not be on the explicit revocation list.
    /// - AIS-03: the realm must still exist and be in a usable state.
    /// - AIS-03: any published package must remain active, and every package
    ///   identity (including unpublished packages) requires an active publisher.
    ///   Explicit Linked identities without a package row skip publisher checks.
    async fn recheck_existing_actor(
        &self,
        actor_id: &ActrId,
        target: Option<&str>,
    ) -> Result<ActrType, AidError> {
        // AIS-04: an explicitly revoked identity is terminal, regardless of any
        // still-valid renewal token it may hold.
        if crate::renewal::is_actor_revoked(actor_id)
            .await
            .map_err(|e| {
                AidError::GenerationFailed(format!("actor revocation lookup failed: {e}"))
            })?
        {
            platform::recording::warn!(
                "Credential issuance rejected: actor {} is on the revocation list",
                actor_id.to_string_repr()
            );
            return Err(AidError::RealmError(
                platform::realm::RealmError::ValidationError(
                    "actor identity has been revoked".to_string(),
                ),
            ));
        }

        // AIS-03(a): the realm must still be usable (exists, enabled, not
        // expired, Active status).
        platform::realm::Realm::validate_realm(&actor_id.realm)
            .await
            .map_err(|e| match e {
                platform::realm::RealmValidationError::RealmUnavailable { reason, .. } => {
                    AidError::RealmError(platform::realm::RealmError::ValidationError(reason))
                }
                platform::realm::RealmValidationError::StoreError { message, .. } => {
                    AidError::GenerationFailed(message)
                }
                // RealmValidationError is #[non_exhaustive].
                _ => AidError::GenerationFailed(format!("realm validation failed: {e:?}")),
            })?;

        // AIS-03(b): recheck package/publisher status for package identities.
        // The Actor type is the one recorded when the identity was issued.
        let (actor_type, auth_mode) = Self::issued_actor(actor_id).await?;
        let pool = platform::storage::db::get_database().get_pool().clone();
        let type_str = actor_type.to_program_identity();
        // Any package row (regardless of lifecycle status) for this type marks a
        // published-package identity. `publisher_package.status` stores 'active' or
        // 'revoked'. When the target is known (persisted at registration time),
        // scope the recheck to that exact target so revoking one target of a
        // multi-target type does not get masked by a sibling target that is still
        // active. Without target (legacy rows), fall back to type-only matching.
        let statuses: Vec<String> = if let Some(target) = target.filter(|t| !t.is_empty()) {
            sqlx::query_scalar(
                "SELECT status FROM publisher_package WHERE type_str = ?1 AND target = ?2",
            )
            .bind(&type_str)
            .bind(target)
            .fetch_all(&pool)
            .await
            .map_err(|e| AidError::GenerationFailed(format!("package status lookup failed: {e}")))?
        } else {
            platform::recording::warn!(
                "Recheck without target for type {}: per-target revocation cannot be enforced; falling back to type-only lookup",
                type_str
            );
            sqlx::query_scalar("SELECT status FROM publisher_package WHERE type_str = ?1")
                .bind(&type_str)
                .fetch_all(&pool)
                .await
                .map_err(|e| {
                    AidError::GenerationFailed(format!("package status lookup failed: {e}"))
                })?
        };

        if statuses.is_empty() && auth_mode == Some(actr_protocol::RegisterAuthMode::Linked as i32)
        {
            return Ok(actor_type);
        }

        let has_active = statuses.iter().any(|status| status == "active");
        if !statuses.is_empty() && !has_active {
            platform::recording::warn!(
                "Credential issuance rejected: every package for type {} (target={:?}) has been revoked",
                type_str,
                target
            );
            return Err(AidError::PackageRevoked);
        }

        // Published and unpublished package identities both require an active publisher.
        let publisher =
            actrix_publisher_registry::model::Publisher::get_by_name(&pool, &actor_type.publisher)
                .await
                .map_err(|e| {
                    AidError::GenerationFailed(format!("publisher status lookup failed: {e}"))
                })?;
        // Pre-upgrade rows have no auth mode. A DNS-style identity is always a
        // package identity; a non-domain identity without a publisher or package
        // is the legacy linked case. Known package identities never take this fallback.
        if auth_mode.is_none()
            && statuses.is_empty()
            && publisher.is_none()
            && !actor_type.publisher.contains('.')
        {
            return Ok(actor_type);
        }
        match publisher {
            Some(publisher_record)
                if publisher_record.status
                    == actrix_publisher_registry::model::PublisherStatus::Active =>
            {
                Ok(actor_type)
            }
            Some(publisher_record) => {
                platform::recording::warn!(
                    "Credential issuance rejected: publisher {} for type {} is not active (status={})",
                    actor_type.publisher,
                    type_str,
                    publisher_record.status
                );
                Err(AidError::PublisherNotVerified)
            }
            None => {
                platform::recording::warn!(
                    "Credential issuance rejected: publisher {} for active package type {} no longer exists",
                    actor_type.publisher,
                    type_str
                );
                Err(AidError::PublisherNotVerified)
            }
        }
    }

    /// Re-issue a credential for an existing ActrId (used by /issuer/renew).
    ///
    /// Takes an already-verified `ActrId`; it neither calls `generate_actr_id()`
    /// nor re-runs registration authentication.
    pub async fn issue_credential_for_actor(
        &self,
        actor_id: &ActrId,
        target: Option<&str>,
    ) -> Result<register_response::RegisterOk, AidError> {
        // Re-validate realm/package/publisher state and the revocation list
        // before signing (AIS-03 / AIS-04). Covers both /renew and /reissue,
        // which are the only callers of this method.
        let actor_type = self.recheck_existing_actor(actor_id, target).await?;

        // Build IdentityClaims.
        let (key_id, verifying_key) = self.realm_key(&actor_id.realm).await?;
        let expr_time = now_secs().saturating_add(self.config.token_ttl_secs);
        let claims_proto = IdentityClaims {
            realm: actor_id.realm.clone(),
            actor_id: actor_id.to_string_repr(),
            expires_at: expr_time,
            protocol_version: 1,
            issuer: actor_id.realm.clone(),
            provider: self.config.provider_id.clone(),
            audience: "actr.signaling".to_string(),
            actor_type,
        };

        let claims_bytes = claims_proto.encode_to_vec();

        // Sign via the Signer Sign RPC.
        let signature_bytes = self
            .signer_client
            .sign(key_id, &claims_bytes)
            .await
            .map_err(|e| AidError::GenerationFailed(format!("Signer sign failed: {e}")))?;

        let credential = AIdCredential {
            key_id,
            claims: Bytes::from(claims_bytes),
            signature: Bytes::from(signature_bytes),
        };

        let credential_expires_at = Some(Timestamp {
            seconds: expr_time as i64,
            nanos: 0,
        });

        // Generate a time-limited TURN credential.
        let turn_credential = self.generate_turn_credential(actor_id, expr_time);

        Ok(register_response::RegisterOk {
            actr_id: actor_id.clone(),
            credential,
            turn_credential,
            credential_expires_at,
            signaling_heartbeat_interval_secs: self.config.signaling_heartbeat_interval_secs,
            signing_pubkey: Bytes::from(verifying_key.as_bytes().to_vec()),
            signing_key_id: key_id,
            renewal_token: None,
            renewal_token_expires_at: None,
        })
    }

    /// Generate a time-limited TURN credential (coturn --use-auth-secret compatible format).
    ///
    /// `username = "<expires_at>:realm=<realm>:provider=<provider>:audience=actr.turn:actor=<actor_id>"`
    /// `password = base64(HMAC-SHA1(realm_turn_secret, username))`, keyed by
    /// the Realm's secret derived from the master `turn_secret`.
    fn generate_turn_credential(
        &self,
        actor_id: &ActrId,
        expires_at: u64,
    ) -> actr_protocol::TurnCredential {
        let realm = actor_id.realm.clone();
        let actor_id = actor_id.to_string_repr();
        let provider = &self.config.provider_id;
        let audience = "actr.turn";
        let username = format!(
            "{expires_at}:realm={realm}:provider={provider}:audience={audience}:actor={actor_id}"
        );
        let secret = platform::realm::realm_turn_secret(
            self.config.turn_secret.as_bytes(),
            &realm,
            provider,
        );
        let mut mac = HmacSha1::new_from_slice(&secret).expect("HMAC-SHA1 accepts any key length");
        mac.update(username.as_bytes());
        let result = mac.finalize();
        let password = BASE64_STANDARD.encode(result.into_bytes());

        actr_protocol::TurnCredential {
            username,
            password,
            expires_at,
            protocol_version: 1,
            realm,
            provider: provider.clone(),
            audience: audience.to_string(),
        }
    }

    // ========== Health check methods ==========

    /// Check the database that holds registrations and renewal tokens.
    pub async fn check_database_health(&self) -> Result<(), AidError> {
        sqlx::query("SELECT 1")
            .execute(platform::storage::db::get_database().get_pool())
            .await
            .map(|_| ())
            .map_err(|e| AidError::GenerationFailed(format!("Database unhealthy: {e}")))
    }

    /// Check Signer service health status.
    ///
    /// Credentials are signed by the Signer with each Realm's issuer key, so
    /// signing works while the Signer answers.
    pub async fn check_signer_health(&self) -> Result<(), AidError> {
        self.signer_client
            .health_check()
            .await
            .map(|_| ())
            .map_err(|e| AidError::GenerationFailed(format!("Signer service unhealthy: {e}")))
    }

    /// Number of Realm issuer keys loaded so far.
    pub async fn realm_key_count(&self) -> usize {
        self.realm_keys.read().await.len()
    }
}

/// How often a Realm issuer key whose retirement failed is retried
const KEY_RETIREMENT_RETRY: std::time::Duration = std::time::Duration::from_secs(60);

/// Retire the issuer key of every Realm that retires, so it never signs
/// again: first for Realms retired before this process started, then as
/// retirements are announced. A failed Signer call is retried.
async fn follow_realm_retirements(
    signer: SignerClientWrapper,
    realm_keys: Arc<RwLock<HashMap<String, (u32, VerifyingKey)>>>,
    cancel: tokio_util::sync::CancellationToken,
) {
    use tokio::sync::broadcast::error::RecvError;

    let mut changes = platform::realm::changes::subscribe();
    let mut pending = retired_realms().await;
    let mut retry = tokio::time::interval(KEY_RETIREMENT_RETRY);
    loop {
        pending = retire_realm_keys(&signer, &realm_keys, pending).await;
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = retry.tick() => {}
            change = changes.recv() => match change {
                Ok(realm) => {
                    if retired_realm(&realm).await {
                        pending.insert(realm);
                    }
                }
                Err(RecvError::Lagged(_)) => pending.extend(retired_realms().await),
                Err(RecvError::Closed) => break,
            },
        }
    }
}

async fn retired_realm(realm: &str) -> bool {
    matches!(
        platform::realm::Realm::get(realm).await,
        Ok(Some(r)) if r.status == platform::realm::RealmStatus::Retired
    )
}

async fn retired_realms() -> HashSet<String> {
    match platform::realm::Realm::get_all().await {
        Ok(realms) => realms
            .into_iter()
            .filter(|r| r.status == platform::realm::RealmStatus::Retired)
            .map(|r| r.name)
            .collect(),
        Err(e) => {
            platform::recording::warn!("Failed to list retired Realms: {}", e);
            HashSet::new()
        }
    }
}

/// Retire the issuer keys of `realms` in the Signer; returns the Realms whose
/// retirement failed.
async fn retire_realm_keys(
    signer: &SignerClientWrapper,
    realm_keys: &RwLock<HashMap<String, (u32, VerifyingKey)>>,
    realms: HashSet<String>,
) -> HashSet<String> {
    let mut failed = HashSet::new();
    for realm in realms {
        realm_keys.write().await.remove(&realm);
        let mut names = vec![crate::realm_profile::issuer_key_name(&realm)];
        match platform::realm::RealmIssuerKey::list(&realm).await {
            Ok(keys) => names.extend(keys.into_iter().map(|key| key.signer_name)),
            Err(e) => {
                platform::recording::warn!(
                    "Failed to list the issuer keys of Realm {}, will retry: {}",
                    realm,
                    e
                );
                failed.insert(realm);
                continue;
            }
        }
        names.dedup();
        for name in names {
            match signer.retire_named_key(&name).await {
                Ok(key_id) => platform::recording::info!(
                    "Retired issuer key {} of Realm {} (key_id {})",
                    name,
                    realm,
                    key_id
                ),
                Err(e) => {
                    platform::recording::warn!(
                        "Failed to retire issuer key {} of Realm {}, will retry: {}",
                        name,
                        realm,
                        e
                    );
                    failed.insert(realm.clone());
                }
            }
        }
    }
    failed
}
