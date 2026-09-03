//! AId token issuer.
//!
//! # Responsibilities
//!
//! Handles `RegisterRequest` and builds `RegisterResponse`, including:
//! - Serial number allocation with Snowflake.
//! - Signed access credential issuance.
//! - Time-limited TURN credential generation.
//! - Renewal token issuance and hash persistence.
//! - Signing key lifecycle management through Signer.
//!
//! # Key Management Strategy
//!
//! ## Security Design
//!
//! - Ed25519 private keys never leave the KS service
//! - AIS only stores the verifying key (public key) for credential carriage
//! - All signing operations go through the KS Sign RPC
//!
//! ## Initialization Phase
//!
//! 1. Attempt to load cached key from local SQLite
//! 2. If the key does not exist, or can no longer mint credentials (any key
//!    past `expires_at`, tolerance window notwithstanding — see below), fetch
//!    a new key from KS
//! 3. Start the background refresh task
//!
//! ## Runtime Refresh
//!
//! - Check frequency: every 10 minutes; a failed rotation retries after a
//!   short doubling backoff (30 s → 600 s) instead of waiting the full cycle
//! - Refresh trigger: time to expiry < 10 minutes
//!
//! ## Validity semantics
//!
//! Verifier caches drop verifying keys at `expires_at` with no tolerance, so
//! the issuer treats `expires_at` as a hard minting deadline:
//! credentials are only minted while the key outlives the current time by
//! more than [`KEY_EXPIRY_SAFETY_MARGIN_SECS`]. The Signer's tolerance window
//! only governs how long key material stays readable on KS; it never extends
//! the minting window here. Minting past expiry used to hand clients
//! born-invalid credentials that failed every signaling handshake with 401.
//!
//! ## Error Handling
//!
//! - Background refresh failure: log a warning and retry with a short backoff
//! - Synchronous refresh failure: return `AidError::GenerationFailed`
//!
//! # Example
//!
//! ```no_run
//! use ais::issuer::{AIdIssuer, IssuerConfig};
//! use ais::signer_client_wrapper::{SignerClientWrapper, create_signer_client};
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
//! // Process registration request
//! // let response = issuer.issue_credential(&request).await?;
//! # Ok(())
//! # }
//! ```

use crate::signer_client_wrapper::SignerClientWrapper;
use crate::sn::{AIdSerialNumberIssuer, SerialNumber};
use crate::storage::{KeyRecord, KeyStorage};

// ========== Constants ==========

/// Key refresh check interval (seconds)
///
/// The background task checks whether the key needs refreshing at this interval
const KEY_REFRESH_CHECK_INTERVAL_SECS: u64 = 600; // 10 minutes

/// Safety margin (seconds) before `expires_at` at which the issuer stops
/// minting credentials with the key.
///
/// Verifier caches drop verifying keys at `expires_at` without any tolerance,
/// so a credential signed after that instant is born-invalid and fails every
/// handshake with 401. The margin absorbs
/// small clock skews between AIS, KS, and signaling so a "still valid"
/// decision on one host does not race past expiry on another. It is kept
/// small on purpose: a deployment with `key_ttl_seconds` at or below this
/// margin would never mint at all.
const KEY_EXPIRY_SAFETY_MARGIN_SECS: u64 = 10;

/// Initial delay (seconds) before retrying a failed background key rotation.
///
/// Doubled after each consecutive failure up to
/// [`KEY_REFRESH_CHECK_INTERVAL_SECS`], and reset after a success. A fixed
/// full-cycle retry used to leave an expired key in place for up to 10
/// minutes per rotation attempt.
const KEY_ROTATION_RETRY_BACKOFF_START_SECS: u64 = 30;

const MANUFACTURER_SIGNED_AT_MAX_AGE_SECS: u64 = 5 * 60;
const MANUFACTURER_SIGNED_AT_FUTURE_SKEW_SECS: u64 = 60;
const MANUFACTURER_NONCE_TTL_SECS: u64 = 10 * 60;

use actr_protocol::{
    AIdCredential, ActrId, ActrType, ErrorResponse, IdentityClaims, Realm, RegisterRequest,
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
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, RwLock};

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
        AidError::RealmError(_) | AidError::ManufacturerNotVerified | AidError::PackageRevoked => {
            403
        }
        AidError::GenerationFailed(msg) => {
            // "has expired" comes from KS refusing to sign with an expired
            // key (a lost race against rotation): the caller should retry
            // later, when rotation has produced a fresh key, so map it to a
            // retryable service-unavailable instead of a hard 500.
            if msg.contains("KS unavailable")
                || msg.contains("KS service")
                || msg.contains("has expired")
            {
                503
            } else {
                500
            }
        }
        AidError::InvalidSignature(_) | AidError::DecodeFailure(_) => 500,
        // `AidError` is `#[non_exhaustive]`: unmapped future variants are
        // generic server errors. This catch-all is what now permits a future
        // fix to express a new failure mode as a real `AidError` variant
        // (see issue #442) instead of hiding it at this boundary.
        _ => 500,
    }
}

/// Failure of a credential reissue attempt.
///
/// Possession failures are deliberately kept AIS-internal instead of widening
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

/// A possession proof that matched an issuer-recorded binding.
///
/// Carries the recorded expiry alongside the proof because both are inputs to
/// the deterministic successor derivation — that is what makes a reissue retry
/// reproduce the original token instead of minting a competing one.
struct VerifiedPossession {
    token: Vec<u8>,
    binding_expires_at: i64,
}

/// Manufacturer nonce verified during request preparation and consumed only
/// when the surrounding control-plane transaction commits.
pub(crate) struct ManufacturerNonceBinding {
    pub(crate) manufacturer: String,
    pub(crate) key_id: String,
    pub(crate) nonce: Vec<u8>,
    pub(crate) created_at: i64,
    pub(crate) expires_at: i64,
}

/// Registration work prepared without mutating SQLite. The HTTP handler
/// commits this state together with ACL and pending-registration metadata.
pub(crate) struct PreparedRegistration {
    pub(crate) register_ok: register_response::RegisterOk,
    token_hash: Vec<u8>,
    token_expires_at: i64,
    manufacturer_nonce: Option<ManufacturerNonceBinding>,
}

/// Reissue work prepared without mutating SQLite. The control-attempt journal
/// commits these values, the nonce consumption, and the encrypted response in
/// one transaction.
pub(crate) struct PreparedReissue {
    pub(crate) register_ok: register_response::RegisterOk,
    pub(crate) successor_token: Vec<u8>,
    pub(crate) successor_hash: Vec<u8>,
    pub(crate) candidate_expires_at: i64,
    pub(crate) proof_hash: Vec<u8>,
    pub(crate) binding_expires_at: i64,
    pub(crate) manufacturer_nonce: Option<ManufacturerNonceBinding>,
}

/// Result of preparing a reissue request for an atomic control-attempt commit.
pub(crate) enum ReissuePreparation {
    Success(PreparedReissue),
    /// Stable client/authentication rejection. The optional verified nonce is
    /// consumed atomically with the cached rejection.
    Rejected {
        response: RegisterResponse,
        manufacturer_nonce: Option<ManufacturerNonceBinding>,
    },
    /// Infrastructure failure. The attempt lease is released so the exact body
    /// can be retried instead of caching a transient failure.
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

fn reissue_error_response(failure: ReissueFailure) -> RegisterResponse {
    let (code, message) = failure.to_response();
    RegisterResponse {
        result: Some(register_response::Result::Error(ErrorResponse {
            code,
            message,
        })),
    }
}

fn response_error_code(response: &RegisterResponse) -> Option<u32> {
    match response.result.as_ref() {
        Some(register_response::Result::Error(error)) => Some(error.code),
        _ => None,
    }
}

/// AId Token issuer configuration
#[derive(Debug, Clone)]
pub struct IssuerConfig {
    /// Token TTL (seconds)
    pub token_ttl_secs: u64,
    /// Signaling server heartbeat interval (seconds)
    pub signaling_heartbeat_interval_secs: u32,
    /// Key cache refresh interval (seconds, default 1 hour)
    pub key_refresh_interval_secs: u64,
    /// Key storage database file path
    pub key_storage_file: std::path::PathBuf,
    /// Whether to enable periodic key rotation
    pub enable_periodic_rotation: bool,
    /// Key rotation interval (seconds, default 24 hours)
    ///
    /// Only effective when enable_periodic_rotation = true.
    /// When this interval is reached, a new key is actively generated even if the old key has not expired.
    pub key_rotation_interval_secs: u64,
    /// TURN shared secret (shared with TURN server, used to generate time-limited credentials)
    pub turn_secret: String,
    /// SQLite data directory (shared with signaling, used to write the verifying key for credential verification)
    ///
    /// After loading or refreshing the signing key, AIS writes the verifying key
    /// to `signaling_key_cache.db` in this directory, so that the signaling
    /// `AIdCredentialValidator` can verify credentials.
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
            token_ttl_secs: 3600,                  // 1 hour
            signaling_heartbeat_interval_secs: 30, // 30 seconds
            key_refresh_interval_secs: 3600,       // 1 hour
            key_storage_file: std::path::PathBuf::from("ais_keys.db"),
            enable_periodic_rotation: false, // periodic rotation disabled by default
            key_rotation_interval_secs: 86400, // 24 hours
            turn_secret: "actrix-turn-secret-change-in-production".to_string(),
            sqlite_path: std::path::PathBuf::from("."),
            renewal_token_ttl_secs: 86400,
            renewal_rotation_window_secs: 21600,
            renewal_token_secret: Vec::new(),
        }
    }
}

/// Key cache (stores only the verifying key; private key is held by KS)
struct KeyCache {
    key_id: u32,
    verifying_key: VerifyingKey,
    /// Hard minting deadline, aligned with the verifier's key validity window;
    /// see [`key_usable_for_minting`].
    expires_at: u64,
    /// KS key-material retention window; informational only — it never
    /// extends the minting window.
    #[allow(dead_code)]
    tolerance_seconds: u64,
}

/// Current Unix time in seconds.
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Whether a key with this `expires_at` may still mint credentials at `now`
/// (both Unix seconds).
///
/// `expires_at == 0` means the key never expires. Otherwise the key must
/// outlive `now` by more than [`KEY_EXPIRY_SAFETY_MARGIN_SECS`]:
/// Verifier caches drop verifying keys at `expires_at` with no tolerance, so
/// any credential minted later is born-invalid and fails every handshake with
/// 401. KS's tolerance window does not
/// widen this check — it only covers key-material retention on the KS side.
fn key_usable_for_minting(expires_at: u64, now: u64) -> bool {
    expires_at == 0 || now.saturating_add(KEY_EXPIRY_SAFETY_MARGIN_SECS) < expires_at
}

/// Calculate a credential deadline without extending beyond its signing key.
fn credential_expiry_for_key(now: u64, token_ttl_secs: u64, key_expires_at: u64) -> u64 {
    let token_expires_at = now.saturating_add(token_ttl_secs);
    if key_expires_at == 0 {
        token_expires_at
    } else {
        token_expires_at.min(key_expires_at)
    }
}

/// AId Token issuer - focused on issuing new Actor Identity Tokens
pub struct AIdIssuer {
    signer_client: SignerClientWrapper,
    key_storage: Arc<KeyStorage>,
    key_cache: Arc<RwLock<Option<KeyCache>>>,
    /// Serializes key creation across request-triggered, manual, and background
    /// rotations. Without this, concurrent registrations that all observe an
    /// expired key can each generate a distinct replacement key.
    key_rotation_lock: Arc<Mutex<()>>,
    /// Issuer configuration (public for renewal handler access).
    pub config: IssuerConfig,
}

impl AIdIssuer {
    /// Create a new AIdIssuer
    pub async fn new(
        signer_client: SignerClientWrapper,
        config: IssuerConfig,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<Self, AidError> {
        let key_storage = KeyStorage::new(&config.key_storage_file)
            .await
            .map_err(|e| {
                AidError::GenerationFailed(format!("Failed to create key storage: {e}"))
            })?;

        let issuer = Self {
            signer_client,
            key_storage: Arc::new(key_storage),
            key_cache: Arc::new(RwLock::new(None)),
            key_rotation_lock: Arc::new(Mutex::new(())),
            config,
        };

        // On initialization, attempt to load/fetch the key. In a port-reuse scenario,
        // KS may become ready later in the same process, so failure here does not block service startup.
        if let Err(e) = issuer.ensure_key_loaded().await {
            platform::recording::warn!("Initial KS key load deferred, will retry on demand: {}", e);
        }

        // Start the background key refresh task unless shutdown has already
        // been requested. This also makes construction with a pre-cancelled
        // token deterministic for isolated callers and tests.
        if cancel.is_cancelled() {
            platform::recording::debug!(
                "Skipping AIS key refresh task because cancellation was requested"
            );
        } else {
            issuer.spawn_key_refresh_task(cancel);
        }

        Ok(issuer)
    }

    /// Ensure a key that can still mint credentials is loaded
    ///
    /// A key is usable only while it outlives the current time by more than
    /// [`KEY_EXPIRY_SAFETY_MARGIN_SECS`] (see [`key_usable_for_minting`]).
    /// An expired — or nearly expired — key, whether in the in-memory cache
    /// or in local storage and regardless of the KS tolerance window,
    /// triggers a synchronous rotation instead of being used: signaling
    /// drops verifying keys at `expires_at` with no tolerance, so minting
    /// with such a key hands clients credentials that fail every handshake
    /// with 401 (the self-locking loop). If KS is
    /// unreachable the error propagates so registration fails honestly
    /// (503) rather than issuing a born-invalid credential.
    async fn ensure_key_loaded(&self) -> Result<(), AidError> {
        let now = now_secs();

        // A cached key that is still usable needs no work.
        let cached_key = {
            let cache = self.key_cache.read().await;
            cache.as_ref().map(|cache| (cache.key_id, cache.expires_at))
        };
        if let Some((key_id, expires_at)) = cached_key {
            if key_usable_for_minting(expires_at, now) {
                platform::recording::debug!("Key already in cache: key_id={}", key_id);
                return Ok(());
            }
            platform::recording::warn!(
                "Cached key can no longer mint credentials, rotating: key_id={}, expires_at={}",
                key_id,
                expires_at
            );
        }

        // Collapse concurrent expiry observations into one rotation. Recheck
        // after acquiring the lock because another request may have refreshed
        // the cache while this request was waiting.
        let _rotation = self.key_rotation_lock.lock().await;
        let now = now_secs();
        let cached_key = {
            let cache = self.key_cache.read().await;
            cache.as_ref().map(|cache| (cache.key_id, cache.expires_at))
        };
        if let Some((key_id, expires_at)) = cached_key {
            if key_usable_for_minting(expires_at, now) {
                platform::recording::debug!("Key refreshed by another request: key_id={}", key_id);
                return Ok(());
            }
            platform::recording::warn!(
                "Cached key can no longer mint credentials, rotating: key_id={}, expires_at={}",
                key_id,
                expires_at
            );
            return self.refresh_key_from_ks_locked().await;
        }

        // Try loading from storage, but only if it can still mint credentials.
        let stored = self.key_storage.get_current_key().await.map_err(|e| {
            AidError::GenerationFailed(format!("Failed to get key from storage: {e}"))
        })?;
        if let Some(record) = stored {
            if key_usable_for_minting(record.expires_at, now) {
                platform::recording::debug!("Loaded key from storage: key_id={}", record.key_id);
                return self.load_key_from_record(&record);
            }
            // Observability only: whether KS has also dropped the key
            // material (beyond its retention window) or just its minting
            // deadline passed.
            let beyond_tolerance = self
                .key_storage
                .is_expired_beyond_tolerance()
                .await
                .unwrap_or(false);
            platform::recording::warn!(
                "Stored key expired for minting, fetching new key from KS: key_id={}, expires_at={}, beyond_tolerance={}",
                record.key_id,
                record.expires_at,
                beyond_tolerance
            );
        } else {
            platform::recording::info!("No stored key found, fetching from KS");
        }

        self.refresh_key_from_ks_locked().await
    }

    /// Load the Ed25519 verifying key from a KeyRecord into the cache
    ///
    /// The `public_key` in the AIS DB stores a base64-encoded Ed25519 verifying key (32 bytes)
    fn load_key_from_record(&self, record: &KeyRecord) -> Result<(), AidError> {
        let key_bytes = BASE64_STANDARD
            .decode(&record.public_key)
            .map_err(|e| AidError::GenerationFailed(format!("Invalid base64 key: {e}")))?;

        let key_array: [u8; 32] = key_bytes.try_into().map_err(|_| {
            AidError::GenerationFailed("Verifying key must be exactly 32 bytes".to_string())
        })?;

        let verifying_key = VerifyingKey::from_bytes(&key_array).map_err(|e| {
            AidError::GenerationFailed(format!("Invalid Ed25519 verifying key: {e}"))
        })?;

        let cache = KeyCache {
            key_id: record.key_id,
            verifying_key,
            expires_at: record.expires_at,
            tolerance_seconds: record.tolerance_seconds,
        };

        let key_id = record.key_id;
        let expires_at = record.expires_at;

        // Synchronous load, blocking until complete
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                *self.key_cache.write().await = Some(cache);
                // Persist the verifying key to signaling_key_cache.db,
                // ensuring it is on disk even if AIdCredentialValidator::init
                // has not been called yet; init will read it from DB later.
                if let Err(e) =
                    platform::aid::credential::validator::AIdCredentialValidator::persist_key(
                        &self.config.sqlite_path,
                        key_id,
                        &verifying_key,
                        expires_at,
                    )
                    .await
                {
                    platform::recording::warn!(
                        "Failed to write verifying key to key_cache DB (non-fatal, key_id={}): {}",
                        key_id,
                        e
                    );
                }
            });
        });

        Ok(())
    }

    /// Refresh the key from KS while [`key_rotation_lock`] is held.
    async fn refresh_key_from_ks_locked(&self) -> Result<(), AidError> {
        platform::recording::info!("Fetching new key from KS");

        Self::refresh_key_internal(
            &self.signer_client,
            &self.key_storage,
            &self.key_cache,
            &self.config,
        )
        .await?;

        platform::recording::info!("Key refreshed successfully");
        Ok(())
    }

    /// Manually trigger key rotation
    ///
    /// Immediately generates a new key from KS and updates the cache.
    /// Returns the new key_id.
    pub async fn rotate_key(&self) -> Result<u32, AidError> {
        platform::recording::info!("Manual key rotation triggered");

        {
            let _rotation = self.key_rotation_lock.lock().await;
            self.refresh_key_from_ks_locked().await?;
        }

        // Read the new key_id
        let cache = self.key_cache.read().await;
        let key_id = cache.as_ref().map(|c| c.key_id).ok_or_else(|| {
            AidError::GenerationFailed("No key available after rotation".to_string())
        })?;

        platform::recording::info!("Manual key rotation completed, new key_id: {}", key_id);
        Ok(key_id)
    }

    /// Get the current key_id
    pub async fn get_current_key_id(&self) -> Result<u32, AidError> {
        let cache = self.key_cache.read().await;
        cache
            .as_ref()
            .map(|c| c.key_id)
            .ok_or_else(|| AidError::GenerationFailed("No key loaded".to_string()))
    }

    /// Get the current key's status for reporting (id, expiry, usability)
    ///
    /// Read-only snapshot for the `/ais/current-key` endpoint and health
    /// tooling: it surfaces expiry so an expired-key outage is visible
    /// instead of masquerading as success. Does not
    /// attempt any key load or rotation.
    pub async fn get_current_key_status(&self) -> Result<CurrentKeyStatus, AidError> {
        let cache = self.key_cache.read().await;
        let cache = cache
            .as_ref()
            .ok_or_else(|| AidError::GenerationFailed("No key loaded".to_string()))?;

        let now = now_secs();
        Ok(CurrentKeyStatus {
            key_id: cache.key_id,
            expires_at: cache.expires_at,
            expires_in: cache.expires_at.saturating_sub(now),
            usable: key_usable_for_minting(cache.expires_at, now),
        })
    }

    /// Return the current signing public key (key_id + raw 32-byte verifying key)
    ///
    /// Used by the `/ais/signing-pubkey` HTTP endpoint,
    /// and can be pulled on demand by the signaling service on a key_cache miss.
    pub async fn get_current_signing_pubkey(&self) -> Result<(u32, Vec<u8>), AidError> {
        let cache = self.key_cache.read().await;
        cache
            .as_ref()
            .map(|c| (c.key_id, c.verifying_key.as_bytes().to_vec()))
            .ok_or_else(|| AidError::GenerationFailed("No signing key loaded".to_string()))
    }

    /// Start the background key refresh task
    fn spawn_key_refresh_task(&self, cancel: tokio_util::sync::CancellationToken) {
        let signer_client = self.signer_client.clone();
        let key_storage = self.key_storage.clone();
        let key_cache = self.key_cache.clone();
        let key_rotation_lock = self.key_rotation_lock.clone();
        let config = self.config.clone();

        tokio::spawn(async move {
            // Delay before the next check. Steady state is the regular check
            // interval; after a failed rotation the task retries after a
            // short doubling backoff (capped at the regular interval), so a
            // transient KS outage no longer pins an expired key for a full
            // 10-minute cycle per attempt.
            // The initial zero delay preserves the old `interval` behavior of
            // checking once right after startup.
            let mut next_wait_secs: u64 = 0;
            let mut retry_backoff_secs = KEY_ROTATION_RETRY_BACKOFF_START_SECS;

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = tokio::time::sleep(Duration::from_secs(next_wait_secs)) => {}
                }

                let mut should_rotate = false;

                // Check if refresh is needed (key expiring soon)
                let should_refresh = match key_storage.should_refresh().await {
                    Ok(should) => should,
                    Err(e) => {
                        platform::recording::error!("Failed to check key refresh status: {}", e);
                        next_wait_secs = retry_backoff_secs;
                        retry_backoff_secs =
                            (retry_backoff_secs * 2).min(KEY_REFRESH_CHECK_INTERVAL_SECS);
                        continue;
                    }
                };

                if should_refresh {
                    platform::recording::info!("Key expiring soon, rotation triggered");
                    should_rotate = true;
                }

                // Check if periodic rotation is needed
                if config.enable_periodic_rotation && !should_rotate {
                    match Self::should_periodic_rotate(&key_storage, &config).await {
                        Ok(true) => {
                            platform::recording::info!(
                                "Periodic rotation interval reached, rotation triggered"
                            );
                            should_rotate = true;
                        }
                        Ok(false) => {}
                        Err(e) => {
                            platform::recording::error!(
                                "Failed to check periodic rotation status: {}",
                                e
                            );
                            next_wait_secs = retry_backoff_secs;
                            retry_backoff_secs =
                                (retry_backoff_secs * 2).min(KEY_REFRESH_CHECK_INTERVAL_SECS);
                            continue;
                        }
                    }
                }

                if !should_rotate {
                    platform::recording::debug!("Key rotation not needed yet");
                    next_wait_secs = KEY_REFRESH_CHECK_INTERVAL_SECS;
                    continue;
                }

                platform::recording::debug!("Background key rotation triggered");

                // Rotate the key
                let rotation_result = {
                    let _rotation = key_rotation_lock.lock().await;
                    Self::refresh_key_internal(&signer_client, &key_storage, &key_cache, &config)
                        .await
                };
                match rotation_result {
                    Ok(()) => {
                        platform::recording::info!("Background key rotation successful");
                        retry_backoff_secs = KEY_ROTATION_RETRY_BACKOFF_START_SECS;
                        next_wait_secs = KEY_REFRESH_CHECK_INTERVAL_SECS;
                    }
                    Err(e) => {
                        next_wait_secs = retry_backoff_secs;
                        platform::recording::warn!(
                            "Background key rotation failed: {}, next retry in {}s",
                            e,
                            next_wait_secs
                        );
                        retry_backoff_secs =
                            (retry_backoff_secs * 2).min(KEY_REFRESH_CHECK_INTERVAL_SECS);
                    }
                }
            }
            platform::recording::debug!("AIS key refresh task cancelled");
        });

        platform::recording::info!("Background key refresh task started");
    }

    /// Check whether periodic key rotation is needed
    ///
    /// Decides based on the fetched_at time and the configured key_rotation_interval_secs
    async fn should_periodic_rotate(
        key_storage: &KeyStorage,
        config: &IssuerConfig,
    ) -> Result<bool, AidError> {
        let current_key = key_storage
            .get_current_key()
            .await
            .map_err(|e| AidError::GenerationFailed(format!("Failed to get current key: {e}")))?;

        let Some(key_record) = current_key else {
            // No key, need to generate one
            return Ok(true);
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let time_since_fetched = now.saturating_sub(key_record.fetched_at);

        Ok(time_since_fetched >= config.key_rotation_interval_secs)
    }

    /// Internal key refresh method (used by the background task)
    ///
    /// Generates a new Ed25519 signing key pair from KS; only the verifying key is stored.
    /// The private key is retained by KS; AIS only signs through the Sign RPC.
    async fn refresh_key_internal(
        signer_client: &SignerClientWrapper,
        key_storage: &KeyStorage,
        key_cache: &RwLock<Option<KeyCache>>,
        config: &IssuerConfig,
    ) -> Result<(), AidError> {
        // Request a new Ed25519 signing key from KS (private key retained by KS)
        let (key_id, verifying_key_bytes, expires_at, tolerance_seconds) = signer_client
            .generate_signing_key()
            .await
            .map_err(|e| AidError::GenerationFailed(format!("KS unavailable: {e}")))?;

        let verifying_key = VerifyingKey::from_bytes(&verifying_key_bytes).map_err(|e| {
            AidError::GenerationFailed(format!("Invalid verifying key from KS: {e}"))
        })?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if !key_usable_for_minting(expires_at, now) {
            return Err(AidError::GenerationFailed(format!(
                "Signer returned a key too close to expiry: key_id={key_id}, expires_at={expires_at}, now={now}"
            )));
        }

        // Update the cache (storing only the verifying key)
        let cache = KeyCache {
            key_id,
            verifying_key,
            expires_at,
            tolerance_seconds,
        };

        *key_cache.write().await = Some(cache);

        // Save to storage (storing the base64-encoded verifying key)
        let key_str = BASE64_STANDARD.encode(verifying_key.as_bytes());
        let record = KeyRecord {
            key_id,
            public_key: key_str,
            fetched_at: now,
            expires_at,
            tolerance_seconds,
        };

        key_storage
            .update_current_key(&record)
            .await
            .map_err(|e| AidError::GenerationFailed(format!("Failed to save key: {e}")))?;

        // Persist the verifying key to signaling_key_cache.db (also updates the in-memory cache).
        if let Err(e) = platform::aid::credential::validator::AIdCredentialValidator::persist_key(
            &config.sqlite_path,
            key_id,
            &verifying_key,
            expires_at,
        )
        .await
        {
            platform::recording::warn!(
                "Failed to write verifying key to key_cache DB (non-fatal, key_id={}): {}",
                key_id,
                e
            );
        } else {
            platform::recording::info!(
                "verifying key persisted to key_cache DB (key_id={})",
                key_id
            );
        }

        Ok(())
    }

    /// Process a register request and issue credentials
    pub async fn issue_credential(
        &self,
        request: &RegisterRequest,
    ) -> Result<RegisterResponse, AidError> {
        self.issue_credential_with_realm_secret_check(request, None)
            .await
    }

    /// Process a register request and issue credentials, with the realm secret check result already completed by the handler.
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

    /// Prepare a new identity and signed credentials without changing durable
    /// registration state.
    pub(crate) async fn prepare_registration_with_realm_secret_check(
        &self,
        request: &RegisterRequest,
        realm_secret_check: Option<RealmSecretCheck>,
    ) -> Result<PreparedRegistration, AidError> {
        self.prepare_registration_inner(request, realm_secret_check.as_ref())
            .await
    }

    /// Commit the durable portion of a prepared registration into a
    /// caller-owned transaction.
    pub(crate) async fn commit_prepared_registration(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        prepared: &PreparedRegistration,
    ) -> Result<(), AidError> {
        if let Some(binding) = prepared.manufacturer_nonce.as_ref() {
            Self::consume_manufacturer_auth_nonce_in_transaction(tx, binding).await?;
        }

        sqlx::query(
            "INSERT OR IGNORE INTO ais_renewal_token
                (actor_id, token_hash, expires_at, created_at)
             VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(prepared.register_ok.actr_id.to_string_repr())
        .bind(&prepared.token_hash)
        .bind(prepared.token_expires_at)
        .bind(crate::renewal::now_secs())
        .execute(&mut **tx)
        .await
        .map_err(|error| {
            AidError::GenerationFailed(format!("Failed to persist renewal token: {error}"))
        })?;

        Ok(())
    }

    /// Prepare a stable-identity credential reissue without changing durable
    /// identity state. Used by the HTTP handler's idempotency transaction.
    pub(crate) async fn prepare_reissue_credential_with_realm_secret_check(
        &self,
        actor_id: &ActrId,
        request: &RegisterRequest,
        renewal_proof: Option<&[u8]>,
        realm_secret_check: Option<RealmSecretCheck>,
    ) -> ReissuePreparation {
        if actor_id.realm != request.realm || actor_id.r#type != request.actr_type {
            return ReissuePreparation::Rejected {
                response: reissue_error_response(AidError::InvalidFormat.into()),
                manufacturer_nonce: None,
            };
        }

        let manufacturer_nonce = match self
            .verify_registration_identity(request, realm_secret_check.as_ref())
            .await
        {
            Ok(binding) => binding,
            Err(err) => {
                let failure = ReissueFailure::from(err);
                let response = reissue_error_response(failure);
                return if response_error_code(&response).is_some_and(|code| code >= 500) {
                    ReissuePreparation::Retryable(response)
                } else {
                    ReissuePreparation::Rejected {
                        response,
                        manufacturer_nonce: None,
                    }
                };
            }
        };

        let proof = match self.verify_actor_possession(actor_id, renewal_proof).await {
            Ok(proof) => proof,
            Err(failure) => {
                let response = reissue_error_response(failure);
                return if response_error_code(&response).is_some_and(|code| code >= 500) {
                    ReissuePreparation::Retryable(response)
                } else {
                    ReissuePreparation::Rejected {
                        response,
                        manufacturer_nonce,
                    }
                };
            }
        };

        let register_ok = match self.issue_credential_for_actor(actor_id).await {
            Ok(register_ok) => register_ok,
            Err(err) => {
                return ReissuePreparation::Retryable(reissue_error_response(err.into()));
            }
        };

        if self.config.renewal_token_secret.is_empty() {
            return ReissuePreparation::Retryable(reissue_error_response(
                AidError::GenerationFailed("renewal_token_secret is required".to_string()).into(),
            ));
        }

        let successor_token = crate::renewal::derive_successor_token(
            &self.config.renewal_token_secret,
            &proof.token,
            actor_id,
            proof.binding_expires_at,
        );
        let successor_hash = crate::renewal::hash_token(&successor_token);
        let anchor = proof.binding_expires_at.max(crate::renewal::now_secs());
        let candidate_expires_at =
            crate::renewal::successor_expires_at(anchor, self.config.renewal_token_ttl_secs);

        ReissuePreparation::Success(PreparedReissue {
            register_ok,
            successor_token,
            successor_hash,
            candidate_expires_at,
            proof_hash: crate::renewal::hash_token(&proof.token),
            binding_expires_at: proof.binding_expires_at,
            manufacturer_nonce,
        })
    }

    /// Verify that the reissue caller actually owns `actor_id`.
    ///
    /// `verify_registration_identity` authenticates the workload context
    /// (package proof or realm authorization) but does not bind the caller to
    /// `actor_id.serial_number`. The binding proof is the renewal token AIS
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

        Ok(VerifiedPossession {
            token: proof.to_vec(),
            binding_expires_at,
        })
    }

    /// Internal processing logic (Ed25519 signing via KS Sign RPC)
    async fn issue_credential_inner(
        &self,
        request: &RegisterRequest,
        realm_secret_check: Option<&RealmSecretCheck>,
    ) -> Result<register_response::RegisterOk, AidError> {
        let prepared = self
            .prepare_registration_inner(request, realm_secret_check)
            .await?;
        let pool = platform::storage::db::get_database().get_pool();
        let mut tx = pool.begin().await.map_err(|error| {
            AidError::GenerationFailed(format!("Failed to begin registration: {error}"))
        })?;
        self.commit_prepared_registration(&mut tx, &prepared)
            .await?;
        tx.commit().await.map_err(|error| {
            AidError::GenerationFailed(format!("Failed to commit registration: {error}"))
        })?;

        let _ = crate::renewal::gc_expired_tokens().await;
        Ok(prepared.register_ok)
    }

    /// Prepare signing and token material without committing identity state.
    async fn prepare_registration_inner(
        &self,
        request: &RegisterRequest,
        realm_secret_check: Option<&RealmSecretCheck>,
    ) -> Result<PreparedRegistration, AidError> {
        let manufacturer_nonce = self
            .verify_registration_identity(request, realm_secret_check)
            .await?;

        // Generate ActrId
        let actr_id = self.generate_actr_id(&request.actr_type, &request.realm)?;

        // Load and snapshot a usable key as close to signing as possible.
        self.ensure_key_loaded().await?;
        let (key_id, verifying_key, key_expires_at) = {
            let cache = self.key_cache.read().await;
            let cache = cache
                .as_ref()
                .ok_or_else(|| AidError::GenerationFailed("No key available".to_string()))?;
            (cache.key_id, cache.verifying_key, cache.expires_at)
        };

        // Credentials and TURN access must not outlive the verifying key.
        let expr_time = self.calculate_expiry_time(key_expires_at);

        // Build IdentityClaims (proto type, plaintext)
        let claims_proto = IdentityClaims {
            realm_id: actr_id.realm.realm_id,
            actor_id: actr_id.to_string_repr(),
            expires_at: expr_time,
        };

        // Proto-encode claims bytes
        let claims_bytes = claims_proto.encode_to_vec();

        // Sign via KS Sign RPC (private key never leaves KS)
        let signature_bytes = self
            .signer_client
            .sign(key_id, &claims_bytes)
            .await
            .map_err(|e| AidError::GenerationFailed(format!("KS sign failed: {e}")))?;

        // Create AIdCredential (Ed25519 format)
        let credential = AIdCredential {
            key_id,
            claims: Bytes::from(claims_bytes),
            signature: Bytes::from(signature_bytes),
        };

        // Create expiry timestamp
        let credential_expires_at = Some(Timestamp {
            seconds: expr_time as i64,
            nanos: 0,
        });

        // Generate a time-limited TURN credential compatible with coturn --use-auth-secret.
        let turn_credential = self.generate_turn_credential(&actr_id.to_string_repr(), expr_time);

        if self.config.renewal_token_secret.is_empty() {
            return Err(AidError::GenerationFailed(
                "renewal_token_secret is required".to_string(),
            ));
        }
        let renewal_token = self.generate_renewal_token();
        let token_expires_at = self.calculate_renewal_expiry_time();
        let token_hash = crate::renewal::hash_token(&renewal_token);
        let register_ok = register_response::RegisterOk {
            actr_id,
            credential,
            turn_credential,
            credential_expires_at,
            signaling_heartbeat_interval_secs: self.config.signaling_heartbeat_interval_secs,
            signing_pubkey: Bytes::from(verifying_key.as_bytes().to_vec()),
            signing_key_id: key_id,
            renewal_token: Some(Bytes::from(renewal_token)),
            renewal_token_expires_at: Some(Timestamp {
                seconds: token_expires_at,
                nanos: 0,
            }),
        };

        Ok(PreparedRegistration {
            register_ok,
            token_hash,
            token_expires_at,
            manufacturer_nonce,
        })
    }

    /// Verify the registration identity according to the explicit request source.
    async fn verify_registration_identity(
        &self,
        request: &RegisterRequest,
        realm_secret_check: Option<&RealmSecretCheck>,
    ) -> Result<Option<ManufacturerNonceBinding>, AidError> {
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
                // Verify MFR identity through a published package, or through
                // signatures for an unpublished package.
                self.verify_mfr_identity(request).await
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
            || request.mfr_signature.is_some()
            || request.manufacturer_auth_signature.is_some()
            || request.manufacturer_auth_signed_at.is_some()
            || request.manufacturer_auth_nonce.is_some()
        {
            return Err(AidError::InvalidFormat);
        }

        match realm_secret_check {
            Some(RealmSecretCheck::ValidCurrent) | Some(RealmSecretCheck::ValidPrevious) => Ok(()),
            other => {
                platform::recording::warn!(
                    "Linked registration rejected: realm secret was not verified, realm={}, type={}, secret_check={:?}",
                    request.realm.realm_id,
                    request.actr_type.to_string_repr(),
                    other
                );
                Err(AidError::ManufacturerNotVerified)
            }
        }
    }

    /// verify MFR identity
    ///
    /// Path 1: an existing package tuple is authoritative. Active packages must
    /// still have an active MFR and a non-revoked signing key; revoked packages
    /// are terminal and cannot fall through.
    /// Path 2: only a never-published tuple may use manifest and request signatures.
    /// Otherwise reject
    async fn verify_mfr_identity(
        &self,
        request: &RegisterRequest,
    ) -> Result<Option<ManufacturerNonceBinding>, AidError> {
        let actr_type = &request.actr_type;
        if actr_type.version.is_empty() {
            return Err(AidError::InvalidFormat);
        }

        let type_str = actr_type.to_string_repr();
        let mfr_name = &actr_type.manufacturer;

        // SECURITY / COMPATIBILITY: package registration must be bound to the
        // exact manifest bytes the manufacturer saw locally. Without
        // manifest_raw, AIS can only prove that "some active package has this
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
                    "MFR verification failed: package registration requires target; type_str={}",
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
                    "MFR verification failed: package registration requires manifest_raw; type_str={}, target={}",
                    type_str,
                    target_ref
                );
                AidError::InvalidFormat
            })?;

        let published_pkg = actrix_mfr::model::ActrPackage::get_by_type_and_target_any_status(
            &pool, &type_str, target_ref,
        )
        .await
        .map_err(|e| AidError::GenerationFailed(format!("MFR lookup failed: {e}")))?;

        if let Some(pkg) = published_pkg {
            use sha2::{Digest, Sha256};

            if pkg.status == actrix_mfr::model::PkgStatus::Revoked {
                platform::recording::warn!(
                    "MFR table lookup rejected: package revoked, type_str={}, target={}",
                    type_str,
                    target_ref
                );
                return Err(AidError::PackageRevoked);
            }

            if pkg.manufacturer != actr_type.manufacturer
                || pkg.name != actr_type.name
                || pkg.version != actr_type.version
                || pkg.target != target_ref
            {
                platform::recording::warn!(
                    "MFR table lookup rejected: package identity mismatch, type_str={}, target={}",
                    type_str,
                    target_ref
                );
                return Err(AidError::ManufacturerNotVerified);
            }

            let request_manifest_hash = Sha256::digest(request_manifest.as_ref());
            let stored_manifest_hash = Sha256::digest(pkg.manifest.as_bytes());
            if request_manifest_hash[..] != stored_manifest_hash[..] {
                platform::recording::warn!(
                    "MFR table lookup rejected: manifest hash mismatch, type_str={}, target={}",
                    type_str,
                    target_ref
                );
                // Do not fall through to Path 2 when the published tuple exists.
                // Rejecting here prevents a mismatched or stale manifest from
                // downgrading into the unpublished-package authentication path.
                return Err(AidError::ManufacturerNotVerified);
            }

            let manager = actrix_mfr::MfrManager::new(pool.clone());
            let signing_key_id = manager
                .verify_published_package_signing_key(&pkg)
                .await
                .map_err(Self::map_mfr_verification_error)?;

            platform::recording::debug!(
                "MFR table lookup passed, type_str={}, target={}, signing_key_id={}",
                type_str,
                target_ref,
                signing_key_id
            );
            return Ok(None);
        }

        // Path 2: not in table -> verify package signature + manufacturer authorization
        // (own package, not yet published).
        let (
            manifest_bytes,
            mfr_signature,
            manufacturer_auth_signature,
            manufacturer_auth_signed_at,
            manufacturer_auth_nonce,
        ) = match (
            &request.manifest_raw,
            &request.mfr_signature,
            &request.manufacturer_auth_signature,
            request.manufacturer_auth_signed_at,
            &request.manufacturer_auth_nonce,
        ) {
            (
                Some(manifest_bytes),
                Some(mfr_signature),
                Some(manufacturer_auth_signature),
                Some(manufacturer_auth_signed_at),
                Some(manufacturer_auth_nonce),
            ) => (
                manifest_bytes,
                mfr_signature,
                manufacturer_auth_signature,
                manufacturer_auth_signed_at,
                manufacturer_auth_nonce,
            ),
            _ => {
                platform::recording::warn!(
                    "MFR verification failed: unpublished package requires manifest_raw, mfr_signature, manufacturer_auth_signature, manufacturer_auth_signed_at, and manufacturer_auth_nonce; type_str={}",
                    type_str
                );
                return Err(AidError::ManufacturerNotVerified);
            }
        };

        if !matches!(manufacturer_auth_nonce.len(), 16 | 32) {
            platform::recording::warn!(
                "manufacturer_auth_nonce has invalid length: len={}, type_str={}",
                manufacturer_auth_nonce.len(),
                type_str
            );
            return Err(AidError::InvalidFormat);
        }

        let now = Self::unix_now_secs()?;
        Self::verify_manufacturer_auth_signed_at(manufacturer_auth_signed_at, now)?;

        // Parse manifest using standard TOML. Reject if not valid UTF-8/TOML —
        // unparseable manifests must not bypass the identity binding check below.
        let manifest_str = std::str::from_utf8(manifest_bytes.as_ref()).map_err(|_| {
            platform::recording::warn!("MFR manifest_raw is not valid UTF-8");
            AidError::InvalidFormat
        })?;
        let manifest_toml: toml::Value = manifest_str.parse().map_err(|e| {
            platform::recording::warn!("MFR manifest_raw is not valid TOML: {}", e);
            AidError::InvalidFormat
        })?;

        // Extract signing_key_id — mandatory for all modern packages.
        // Without it, we cannot reliably resolve the correct MFR key after rotation.
        let signing_key_id = manifest_toml
            .get("signing_key_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                platform::recording::warn!(
                    "MFR manifest_raw missing 'signing_key_id', manufacturer={}",
                    mfr_name
                );
                AidError::InvalidFormat
            })?
            .to_string();

        // Look up the manufacturer's public key by key_id (supports current + historical)
        let manager = actrix_mfr::MfrManager::new(pool.clone());
        let mfr_info = manager
            .resolve_key_by_id(mfr_name, &signing_key_id)
            .await
            .map_err(Self::map_mfr_verification_error)?;

        // Verify package manifest signature using the manufacturer's public key.
        let sig_b64 = base64::prelude::BASE64_STANDARD.encode(mfr_signature.as_ref());
        let valid = actrix_mfr::crypto::verify_signature(
            manifest_bytes.as_ref(),
            &sig_b64,
            &mfr_info.public_key,
        )
        .map_err(|e| {
            platform::recording::warn!(
                "MFR signature verification error: manufacturer={}, err={}",
                mfr_name,
                e
            );
            AidError::GenerationFailed(format!("Signature verification error: {e}"))
        })?;

        if !valid {
            platform::recording::warn!(
                "MFR signature verification failed: invalid signature, type_str={}",
                type_str
            );
            return Err(AidError::ManufacturerNotVerified);
        }

        // SECURITY: Verify that manifest identity matches the RegisterRequest.
        // Without this check, an attacker holding any valid signed manifest from
        // the same MFR could reuse it to register a different actr_type/target.
        let m_manufacturer = manifest_toml
            .get("manufacturer")
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

        // Check manufacturer/name/version match actr_type.
        if m_manufacturer != actr_type.manufacturer
            || m_name != actr_type.name
            || m_version != actr_type.version
        {
            platform::recording::warn!(
                "MFR manifest identity mismatch: manifest={}:{}:{}, request={}",
                m_manufacturer,
                m_name,
                m_version,
                type_str
            );
            return Err(AidError::ManufacturerNotVerified);
        }

        // The MFR-signed manifest must declare binary.target. Falling back to
        // request.target would leave the manifest signature unbound to target,
        // letting one signed manifest be registered under arbitrary targets.
        // Mirror Path 1, where the registry row's target is authoritative.
        let manifest_target = m_target.ok_or_else(|| {
            platform::recording::warn!("MFR manifest missing binary.target; type_str={}", type_str);
            AidError::InvalidFormat
        })?;

        // request.target is guaranteed non-empty by the entry guard; ensure it
        // matches the target the MFR signed into the manifest.
        if let Some(req_target) = request.target.as_deref() {
            if req_target != manifest_target {
                platform::recording::warn!(
                    "MFR manifest target mismatch: manifest={}, request={}",
                    manifest_target,
                    req_target
                );
                return Err(AidError::ManufacturerNotVerified);
            }
        }
        let manifest_sha256_hex = {
            use sha2::{Digest, Sha256};
            hex::encode(Sha256::digest(manifest_bytes.as_ref()))
        };
        let manufacturer_payload = actr_protocol::build_manufacturer_register_payload(
            actr_protocol::ManufacturerRegisterPayload {
                realm_id: request.realm.realm_id,
                actr_type,
                target: manifest_target,
                manifest_sha256_hex: &manifest_sha256_hex,
                manufacturer_auth_signed_at,
                manufacturer_auth_nonce: manufacturer_auth_nonce.as_ref(),
            },
        );
        let manufacturer_sig_b64 =
            base64::prelude::BASE64_STANDARD.encode(manufacturer_auth_signature.as_ref());
        let manufacturer_valid = actrix_mfr::crypto::verify_signature(
            manufacturer_payload.as_bytes(),
            &manufacturer_sig_b64,
            &mfr_info.public_key,
        )
        .map_err(|e| {
            platform::recording::warn!(
                "manufacturer_auth_signature verification error: manufacturer={}, key_id={}, err={}",
                mfr_name,
                signing_key_id,
                e
            );
            AidError::GenerationFailed(format!("Manufacturer signature verification error: {e}"))
        })?;

        if !manufacturer_valid {
            platform::recording::warn!(
                "manufacturer_auth_signature verification failed: manufacturer={}, key_id={}, type_str={}",
                mfr_name,
                signing_key_id,
                type_str
            );
            return Err(AidError::ManufacturerNotVerified);
        }

        platform::recording::debug!(
            "MFR manifest and manufacturer signature verification passed, type_str={}",
            type_str
        );
        let created_at = i64::try_from(now)
            .map_err(|_| AidError::InvalidTimestamp("current time exceeds i64".to_string()))?;
        Ok(Some(ManufacturerNonceBinding {
            manufacturer: mfr_name.clone(),
            key_id: signing_key_id,
            nonce: manufacturer_auth_nonce.to_vec(),
            created_at,
            expires_at: created_at.saturating_add(MANUFACTURER_NONCE_TTL_SECS as i64),
        }))
    }

    fn map_mfr_verification_error(error: actrix_mfr::MfrError) -> AidError {
        let message = error.to_string();
        match error {
            actrix_mfr::MfrError::NotFound
            | actrix_mfr::MfrError::KeyRevoked(_)
            | actrix_mfr::MfrError::InvalidStatus(_)
            | actrix_mfr::MfrError::CertificateExpired
            | actrix_mfr::MfrError::InvalidSignature
            | actrix_mfr::MfrError::Unauthorized
            | actrix_mfr::MfrError::InvalidRequest(_) => {
                platform::recording::warn!("MFR verification rejected: {}", message);
                AidError::ManufacturerNotVerified
            }
            other => {
                platform::recording::warn!("MFR verification failed internally: {}", other);
                AidError::GenerationFailed(format!("MFR verification failed: {other}"))
            }
        }
    }

    fn unix_now_secs() -> Result<u64, AidError> {
        crate::renewal::try_now_secs()
            .map_err(|e| AidError::InvalidTimestamp(format!("system clock before Unix epoch: {e}")))
    }

    fn verify_manufacturer_auth_signed_at(signed_at: u64, now: u64) -> Result<(), AidError> {
        if signed_at > now.saturating_add(MANUFACTURER_SIGNED_AT_FUTURE_SKEW_SECS) {
            platform::recording::warn!(
                "manufacturer_auth_signed_at is too far in the future: signed_at={}, now={}, future_skew_secs={}",
                signed_at,
                now,
                MANUFACTURER_SIGNED_AT_FUTURE_SKEW_SECS
            );
            return Err(AidError::InvalidTimestamp(
                "manufacturer_auth_signed_at is too far in the future".to_string(),
            ));
        }

        if now.saturating_sub(signed_at) > MANUFACTURER_SIGNED_AT_MAX_AGE_SECS {
            platform::recording::warn!(
                "manufacturer_auth_signed_at outside max age: signed_at={}, now={}, max_age_secs={}",
                signed_at,
                now,
                MANUFACTURER_SIGNED_AT_MAX_AGE_SECS
            );
            return Err(AidError::Expired);
        }

        Ok(())
    }

    async fn consume_manufacturer_auth_nonce_in_transaction(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        binding: &ManufacturerNonceBinding,
    ) -> Result<(), AidError> {
        let now = crate::renewal::now_secs();
        if binding.expires_at <= now {
            return Err(AidError::ManufacturerNotVerified);
        }
        sqlx::query("DELETE FROM ais_manufacturer_auth_nonce WHERE expires_at < ?1")
            .bind(now)
            .execute(&mut **tx)
            .await
            .map_err(|error| {
                AidError::GenerationFailed(format!("manufacturer nonce cleanup failed: {error}"))
            })?;

        let result = sqlx::query(
            "INSERT INTO ais_manufacturer_auth_nonce
                (manufacturer, key_id, nonce, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(&binding.manufacturer)
        .bind(&binding.key_id)
        .bind(&binding.nonce)
        .bind(binding.created_at)
        .bind(binding.expires_at)
        .execute(&mut **tx)
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
                Err(AidError::ManufacturerNotVerified)
            }
            Err(error) => Err(AidError::GenerationFailed(format!(
                "manufacturer nonce consume failed: {error}"
            ))),
        }
    }

    /// Generate ActrId
    fn generate_actr_id(&self, actr_type: &ActrType, realm: &Realm) -> Result<ActrId, AidError> {
        // Generate serial number using the Snowflake algorithm
        let serial_number = SerialNumber::sn(realm.realm_id);

        Ok(ActrId {
            realm: *realm,
            serial_number: serial_number.value(),
            r#type: actr_type.clone(),
        })
    }

    /// Calculate expiry time capped by the signing key deadline.
    fn calculate_expiry_time(&self, key_expires_at: u64) -> u64 {
        credential_expiry_for_key(now_secs(), self.config.token_ttl_secs, key_expires_at)
    }

    /// Calculate renewal token expiry time (Unix timestamp, seconds, i64 for SQLite)
    fn calculate_renewal_expiry_time(&self) -> i64 {
        (SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + self.config.renewal_token_ttl_secs) as i64
    }

    /// Generate a 32-byte renewal token using CSPRNG
    fn generate_renewal_token(&self) -> Vec<u8> {
        let mut token = vec![0u8; 32];
        use rand::RngCore;
        rand::thread_rng().fill_bytes(&mut token);
        token
    }

    /// Re-issue credentials for an existing ActrId (used by /ais/renew)
    ///
    /// Accepts an already-verified `ActrId`; does not call `generate_actr_id()` or re-run registration authentication.
    pub async fn issue_credential_for_actor(
        &self,
        actor_id: &ActrId,
    ) -> Result<register_response::RegisterOk, AidError> {
        // Ensure a usable signing key is available
        self.ensure_key_loaded().await?;

        // Snapshot the key before deriving the credential deadline.
        let (key_id, verifying_key, key_expires_at) = {
            let cache = self.key_cache.read().await;
            let cache = cache
                .as_ref()
                .ok_or_else(|| AidError::GenerationFailed("No key available".to_string()))?;
            (cache.key_id, cache.verifying_key, cache.expires_at)
        };

        // Credentials and TURN access must not outlive the verifying key.
        let expr_time = self.calculate_expiry_time(key_expires_at);

        // Build IdentityClaims
        let claims_proto = IdentityClaims {
            realm_id: actor_id.realm.realm_id,
            actor_id: actor_id.to_string_repr(),
            expires_at: expr_time,
        };

        let claims_bytes = claims_proto.encode_to_vec();

        // Sign via KS Sign RPC
        let signature_bytes = self
            .signer_client
            .sign(key_id, &claims_bytes)
            .await
            .map_err(|e| AidError::GenerationFailed(format!("KS sign failed: {e}")))?;

        let credential = AIdCredential {
            key_id,
            claims: Bytes::from(claims_bytes),
            signature: Bytes::from(signature_bytes),
        };

        let credential_expires_at = Some(Timestamp {
            seconds: expr_time as i64,
            nanos: 0,
        });

        // Generate time-limited TURN credential
        let turn_credential = self.generate_turn_credential(&actor_id.to_string_repr(), expr_time);

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

    /// Generate time-limited TURN credential (coturn --use-auth-secret compatible format)
    ///
    /// `username = "<expires_at>:<actor_id>"`
    /// `password = base64(HMAC-SHA1(turn_secret, username))`
    fn generate_turn_credential(
        &self,
        actor_id: &str,
        expires_at: u64,
    ) -> actr_protocol::TurnCredential {
        let username = format!("{expires_at}:{actor_id}");
        let mut mac = HmacSha1::new_from_slice(self.config.turn_secret.as_bytes())
            .expect("HMAC-SHA1 accepts any key length");
        mac.update(username.as_bytes());
        let result = mac.finalize();
        let password = BASE64_STANDARD.encode(result.into_bytes());

        actr_protocol::TurnCredential {
            username,
            password,
            expires_at,
        }
    }

    // ========== Health check methods ==========

    /// Check database health status
    pub async fn check_database_health(&self) -> Result<(), AidError> {
        self.key_storage
            .health_check()
            .await
            .map_err(|e| AidError::GenerationFailed(format!("Database unhealthy: {e}")))
    }

    /// Check KS service health status
    ///
    /// Verifies KS service availability via the health_check RPC
    pub async fn check_ks_health(&self) -> Result<(), AidError> {
        self.signer_client
            .health_check()
            .await
            .map(|_| ())
            .map_err(|e| AidError::GenerationFailed(format!("KS service unhealthy: {e}")))
    }

    /// Check key cache health status
    pub async fn check_key_cache_health(&self) -> Result<KeyCacheInfo, AidError> {
        let has_cache = self.key_cache.read().await.is_some();
        if !has_cache {
            self.ensure_key_loaded().await?;
        }

        let status = self.get_current_key_status().await?;
        if !status.usable {
            return Err(AidError::GenerationFailed(format!(
                "Signing key is not usable: key_id={}, expires_at={}",
                status.key_id, status.expires_at
            )));
        }

        Ok(KeyCacheInfo {
            key_id: status.key_id,
            expires_in: status.expires_in,
        })
    }
}

/// Key cache health info
pub struct KeyCacheInfo {
    pub key_id: u32,
    pub expires_in: u64,
}

/// Snapshot of the current signing key's status
pub struct CurrentKeyStatus {
    pub key_id: u32,
    /// Unix seconds; 0 means the key never expires
    pub expires_at: u64,
    /// Unix seconds until expiry (0 once expired)
    pub expires_in: u64,
    /// Whether the key may still mint credentials (see [`key_usable_for_minting`])
    pub usable: bool,
}

#[cfg(test)]
mod tests {
    // Note: Full tests requiring a running KS service live in
    // tests/integration_test.rs; these are the pure, KS-free units.

    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_key_usable_for_minting_never_expires() {
        assert!(key_usable_for_minting(0, u64::MAX));
        assert!(key_usable_for_minting(0, 0));
    }

    #[test]
    fn test_key_usable_for_minting_respects_safety_margin() {
        let now = 1_000_000u64;
        // Comfortably before expiry: usable.
        assert!(key_usable_for_minting(now + 3600, now));
        // Inside the safety margin before expiry: not usable.
        assert!(!key_usable_for_minting(
            now + KEY_EXPIRY_SAFETY_MARGIN_SECS,
            now
        ));
        assert!(!key_usable_for_minting(
            now + KEY_EXPIRY_SAFETY_MARGIN_SECS - 1,
            now
        ));
        // Just past the margin on the other side: usable again.
        assert!(key_usable_for_minting(
            now + KEY_EXPIRY_SAFETY_MARGIN_SECS + 1,
            now
        ));
        // At or past expiry — including inside any KS tolerance window:
        // never usable, these minted born-invalid credentials before #7.
        assert!(!key_usable_for_minting(now, now));
        assert!(!key_usable_for_minting(now - 1, now));
        assert!(!key_usable_for_minting(1, now));
    }

    #[test]
    fn test_credential_expiry_respects_token_ttl_for_non_expiring_key() {
        assert_eq!(credential_expiry_for_key(1_000, 3_600, 0), 4_600);
    }

    #[test]
    fn test_credential_expiry_is_capped_by_key_expiry() {
        assert_eq!(credential_expiry_for_key(1_000, 3_600, 1_200), 1_200);
        assert_eq!(credential_expiry_for_key(1_000, 60, 1_200), 1_060);
    }

    /// A KS pointing at a closed port: connection refused immediately, so
    /// refresh attempts fail fast without a real server.
    fn unreachable_signer_wrapper() -> SignerClientWrapper {
        SignerClientWrapper::new(signer::GrpcClientConfig {
            endpoint: "http://127.0.0.1:1".to_string(),
            actrix_shared_key: "test".to_string(),
            timeout_seconds: 1,
            enable_tls: false,
            tls_domain: None,
            ca_cert: None,
            client_cert: None,
            client_key: None,
        })
    }

    fn test_config(dir: &std::path::Path) -> IssuerConfig {
        IssuerConfig {
            key_storage_file: dir.join("ais_keys.db"),
            sqlite_path: dir.to_path_buf(),
            renewal_token_secret: vec![0u8; 32],
            ..Default::default()
        }
    }

    async fn seed_stored_key(dir: &std::path::Path, expires_at: u64) -> AIdIssuer {
        let storage = KeyStorage::new(&dir.join("ais_keys.db"))
            .await
            .expect("storage");
        let verifying_key = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
        storage
            .update_current_key(&KeyRecord {
                key_id: 7,
                public_key: BASE64_STANDARD.encode(verifying_key.verifying_key().as_bytes()),
                fetched_at: now_secs(),
                expires_at,
                tolerance_seconds: 3600,
            })
            .await
            .expect("seed key");

        AIdIssuer {
            signer_client: unreachable_signer_wrapper(),
            key_storage: Arc::new(storage),
            key_cache: Arc::new(RwLock::new(None)),
            key_rotation_lock: Arc::new(Mutex::new(())),
            config: test_config(dir),
        }
    }

    /// Regression: a stored key that is
    /// expired-but-within-tolerance must NOT be loaded and used — that is
    /// exactly how born-invalid credentials were minted. With KS
    /// unreachable, the load must fail instead of silently using the key.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_ensure_key_loaded_rejects_expired_stored_key_even_within_tolerance() {
        let dir = tempdir().unwrap();
        let issuer = seed_stored_key(dir.path(), now_secs().saturating_sub(10)).await;

        let result = issuer.ensure_key_loaded().await;
        assert!(
            result.is_err(),
            "an expired stored key must trigger a KS fetch, not be loaded"
        );
        assert!(
            issuer.key_cache.read().await.is_none(),
            "no expired key may end up in the minting cache"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_ensure_key_loaded_accepts_unexpired_stored_key() {
        let dir = tempdir().unwrap();
        let issuer = seed_stored_key(dir.path(), now_secs() + 3600).await;

        issuer.ensure_key_loaded().await.expect("usable key loads");
        let cache = issuer.key_cache.read().await;
        assert_eq!(cache.as_ref().expect("key cached").key_id, 7);
    }

    /// Regression: an expired cached key
    /// must be visible as unusable in the status snapshot.
    #[tokio::test]
    async fn test_get_current_key_status_reports_expired_key() {
        let dir = tempdir().unwrap();
        let issuer = seed_stored_key(dir.path(), now_secs() + 3600).await;

        let verifying_key = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
        *issuer.key_cache.write().await = Some(KeyCache {
            key_id: 9,
            verifying_key: verifying_key.verifying_key(),
            expires_at: now_secs().saturating_sub(5),
            tolerance_seconds: 3600,
        });

        let status = issuer.get_current_key_status().await.expect("status");
        assert_eq!(status.key_id, 9);
        assert!(!status.usable, "expired key must report as unusable");
        assert_eq!(status.expires_in, 0);

        let fresh = issuer.get_current_key_id().await.expect("key id");
        assert_eq!(fresh, 9);
    }

    #[tokio::test]
    async fn test_key_cache_health_rejects_expired_key() {
        let dir = tempdir().unwrap();
        let issuer = seed_stored_key(dir.path(), now_secs() + 3600).await;

        let verifying_key = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
        *issuer.key_cache.write().await = Some(KeyCache {
            key_id: 9,
            verifying_key: verifying_key.verifying_key(),
            expires_at: now_secs().saturating_sub(5),
            tolerance_seconds: 3600,
        });

        let result = issuer.check_key_cache_health().await;
        assert!(result.is_err(), "expired key must make health degraded");
    }
}
