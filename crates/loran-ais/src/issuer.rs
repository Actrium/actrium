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
//! - The Ed25519 private key never leaves the Signer service.
//! - AIS only stores the verifying key (public key) for inclusion in credentials.
//! - All signing operations are performed via the Signer's Sign RPC.
//!
//! ## Initialization Phase
//!
//! 1. Attempt to load cached key from local SQLite
//! 2. If the key does not exist, or can no longer mint credentials (any key
//!    past `expires_at`, tolerance window notwithstanding — see below), fetch
//!    a new key from the Signer
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
//! only governs how long key material stays readable on the Signer; it never
//! extends the minting window here. Minting past expiry used to hand clients
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
//! // Process a registration request
//! // let response = issuer.issue_credential(&request).await?;
//! # Ok(())
//! # }
//! ```

use crate::signer_client_wrapper::SignerClientWrapper;
use crate::sn::{AIdSerialNumberIssuer, SerialNumber};
use crate::storage::{KeyRecord, KeyStorage};

// ========== Constants ==========

/// Key refresh check interval (seconds).
///
/// The background task checks whether the key needs refreshing at this interval.
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

use aes_gcm::{Aes256Gcm, Key, Nonce};
use rand::RngCore;

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
            // "has expired" comes from the Signer refusing to sign with an
            // expired key (a lost race against rotation): the caller should
            // retry later, when rotation has produced a fresh key, so map it
            // to a retryable service-unavailable instead of a hard 500.
            if msg.contains("Signer unavailable")
                || msg.contains("Signer service")
                || msg.contains("has expired")
            {
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

/// A verified manufacturer-auth nonce and its scoping identity, returned from
/// [`verify_manufacturer_auth`] so the caller can consume (INSERT) the nonce at
/// the right point in the flow rather than inside the verify step. Deferring
/// consumption to the response-commit transaction is what makes `/reissue`
/// idempotent on a lost response (P1-8): if that transaction does not commit,
/// the nonce is never burned and the client can retry with the same attempt.
struct ManufacturerAuthBinding {
    manufacturer: String,
    signing_key_id: String,
    nonce: Vec<u8>,
}

/// How long a cached `/reissue` response stays replayable. Matches
/// [`MANUFACTURER_NONCE_TTL_SECS`]: the cache is the replay guard for that
/// window, and once both lapse a retry falls through to re-signing under the
/// P1-7 binding-grace window (not the 5-minute replay window).
const REISSUE_RESPONSE_CACHE_TTL_SECS: i64 = 10 * 60;

/// Ciphertext format version byte prepended to `nonce(12) || ciphertext` in
/// [`ais_reissue_response_cache`]. Bumping it invalidates all cached responses.
const REISSUE_CIPHERTEXT_VERSION: u8 = 1;

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
    /// Key cache refresh interval (seconds, default 1 hour).
    pub key_refresh_interval_secs: u64,
    /// Key storage database file path.
    pub key_storage_file: std::path::PathBuf,
    /// Whether to enable periodic key rotation.
    pub enable_periodic_rotation: bool,
    /// Key rotation interval (seconds, default 24 hours).
    ///
    /// Only effective when enable_periodic_rotation = true.
    /// When this interval is reached, a new key is proactively generated
    /// even if the old key has not expired.
    pub key_rotation_interval_secs: u64,
    /// TURN shared secret (shared with the TURN server, used to generate time-limited credentials).
    pub turn_secret: String,
    /// SQLite data directory (shared with signaling, used to write the verifying key
    /// so signaling can verify credentials).
    ///
    /// After loading or refreshing the signing key, AIS writes the verifying key to
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
            key_refresh_interval_secs: 3600,       // 1 hour
            key_storage_file: std::path::PathBuf::from("ais_keys.db"),
            enable_periodic_rotation: false, // periodic rotation disabled by default
            key_rotation_interval_secs: 86400, // 24 hours
            turn_secret: String::new(),
            sqlite_path: std::path::PathBuf::from("."),
            renewal_token_ttl_secs: 86400,
            renewal_rotation_window_secs: 21600,
            renewal_token_secret: Vec::new(),
        }
    }
}

/// Key cache (stores only the verifying key; the private key is held by the Signer).
struct KeyCache {
    key_id: u32,
    verifying_key: VerifyingKey,
    /// Hard minting deadline, aligned with the verifier's key validity window;
    /// see [`key_usable_for_minting`].
    expires_at: u64,
    /// Signer key-material retention window; informational only — it never
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
/// 401. The Signer's tolerance window does not
/// widen this check — it only covers key-material retention on the Signer side.
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

        // Attempt to load/fetch the key during initialization. In the main port
        // reuse scenario, the Signer may become ready shortly within the same
        // process, so a failure here does not block service startup.
        if let Err(e) = issuer.ensure_key_loaded().await {
            platform::recording::warn!(
                "Initial Signer key load deferred, will retry on demand: {}",
                e
            );
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
    /// or in local storage and regardless of the Signer tolerance window,
    /// triggers a synchronous rotation instead of being used: signaling
    /// drops verifying keys at `expires_at` with no tolerance, so minting
    /// with such a key hands clients credentials that fail every handshake
    /// with 401 (the self-locking loop). If the Signer is
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
            platform::recording::info!("No stored key found, fetching from Signer");
        }

        self.refresh_key_from_ks_locked().await
    }

    /// Load the Ed25519 verifying key from a KeyRecord into the cache.
    ///
    /// The `public_key` in the AIS DB stores a base64-encoded Ed25519 verifying key (32 bytes).
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

        // Synchronous load, blocking wait.
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                *self.key_cache.write().await = Some(cache);
                // Persist the verifying key to signaling_key_cache.db,
                // ensuring it is on disk even if AIdCredentialValidator::init
                // has not yet been called; a subsequent init will read the key
                // from the DB.
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

    /// Refresh the key from the Signer while [`key_rotation_lock`] is held.
    async fn refresh_key_from_ks_locked(&self) -> Result<(), AidError> {
        platform::recording::info!("Fetching new key from Signer");

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

    /// Manually trigger key rotation.
    ///
    /// Immediately generates a new key from the Signer and updates the cache.
    /// Returns the new key_id.
    pub async fn rotate_key(&self) -> Result<u32, AidError> {
        platform::recording::info!("Manual key rotation triggered");

        {
            let _rotation = self.key_rotation_lock.lock().await;
            self.refresh_key_from_ks_locked().await?;
        }

        // Read the new key_id.
        let cache = self.key_cache.read().await;
        let key_id = cache.as_ref().map(|c| c.key_id).ok_or_else(|| {
            AidError::GenerationFailed("No key available after rotation".to_string())
        })?;

        platform::recording::info!("Manual key rotation completed, new key_id: {}", key_id);
        Ok(key_id)
    }

    /// Get the currently used key_id.
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
    /// and can also be pulled on demand by the signaling service on a key_cache miss.
    pub async fn get_current_signing_pubkey(&self) -> Result<(u32, Vec<u8>), AidError> {
        let cache = self.key_cache.read().await;
        cache
            .as_ref()
            .map(|c| (c.key_id, c.verifying_key.as_bytes().to_vec()))
            .ok_or_else(|| AidError::GenerationFailed("No signing key loaded".to_string()))
    }

    /// Start the background key refresh task.
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
            // transient Signer outage no longer pins an expired key for a
            // full 10-minute cycle per attempt.
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

                // Check whether a refresh is needed (key is about to expire).
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

                // Check whether periodic rotation is needed.
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

    /// Check whether periodic key rotation is needed.
    ///
    /// Decides based on the fetched_at time and the configured key_rotation_interval_secs.
    async fn should_periodic_rotate(
        key_storage: &KeyStorage,
        config: &IssuerConfig,
    ) -> Result<bool, AidError> {
        let current_key = key_storage
            .get_current_key()
            .await
            .map_err(|e| AidError::GenerationFailed(format!("Failed to get current key: {e}")))?;

        let Some(key_record) = current_key else {
            // No key present; need to generate one.
            return Ok(true);
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let time_since_fetched = now.saturating_sub(key_record.fetched_at);

        Ok(time_since_fetched >= config.key_rotation_interval_secs)
    }

    /// Internal key refresh method (used by the background task).
    ///
    /// Generates a new Ed25519 signing key pair from the Signer; only the verifying key
    /// is saved. The private key is held by the Signer; AIS signs exclusively via the Sign RPC.
    async fn refresh_key_internal(
        signer_client: &SignerClientWrapper,
        key_storage: &KeyStorage,
        key_cache: &RwLock<Option<KeyCache>>,
        config: &IssuerConfig,
    ) -> Result<(), AidError> {
        // Request a new Ed25519 signing key from the Signer (private key held by the Signer).
        let (key_id, verifying_key_bytes, expires_at, tolerance_seconds) = signer_client
            .generate_signing_key()
            .await
            .map_err(|e| AidError::GenerationFailed(format!("Signer unavailable: {e}")))?;

        let verifying_key = VerifyingKey::from_bytes(&verifying_key_bytes).map_err(|e| {
            AidError::GenerationFailed(format!("Invalid verifying key from Signer: {e}"))
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

        // Save to storage (stored as a base64-encoded verifying key).
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

    /// Re-authenticate a registration context and issue fresh credentials for
    /// an existing actor identity.
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
    /// Failures are mapped into the `Error` variant of [`RegisterResponse`] here
    /// so the handler has a single non-fallible path; this never returns `Err`.
    pub async fn reissue_credential_with_realm_secret_check(
        &self,
        actor_id: &ActrId,
        request: &RegisterRequest,
        renewal_proof: Option<&[u8]>,
        realm_secret_check: Option<RealmSecretCheck>,
    ) -> RegisterResponse {
        match self
            .reissue_credential_inner(
                actor_id,
                request,
                renewal_proof,
                realm_secret_check.as_ref(),
            )
            .await
        {
            Ok((register_ok, nonce_binding)) => {
                // Successor hash = SHA-256 of the raw successor token returned
                // in the response. Used to mark the row delivered (P1-7).
                let successor_hash = register_ok
                    .renewal_token
                    .as_ref()
                    .map(|t| crate::renewal::hash_token(t.as_ref()))
                    .unwrap_or_default();
                let response = RegisterResponse {
                    result: Some(register_response::Result::Success(register_ok)),
                };
                // Package-auth reissue: atomically burn the nonce, cache the
                // encrypted response, and mark the successor delivered. If this
                // transaction fails, the nonce was never burned (retriable) and
                // the successor row stays delivered_at = NULL (P1-7 grace
                // window admits the retry) — return a 500 so the client retries
                // instead of getting a non-idempotent success.
                if let Some(binding) = nonce_binding {
                    if let Err(err) = self
                        .commit_reissue_response(
                            &binding,
                            actor_id,
                            &successor_hash,
                            &response,
                            request.target.as_deref(),
                        )
                        .await
                    {
                        platform::recording::error!(
                            "reissue response cache commit failed for actor {}: {}",
                            actor_id.to_string_repr(),
                            err
                        );
                        return RegisterResponse {
                            result: Some(register_response::Result::Error(ErrorResponse {
                                code: 500,
                                message: "Failed to persist reissue response".to_string(),
                            })),
                        };
                    }
                }
                response
            }
            Err(err) => {
                let (code, message) = err.to_response();
                RegisterResponse {
                    result: Some(register_response::Result::Error(ErrorResponse {
                        code,
                        message,
                    })),
                }
            }
        }
    }

    async fn reissue_credential_inner(
        &self,
        actor_id: &ActrId,
        request: &RegisterRequest,
        renewal_proof: Option<&[u8]>,
        realm_secret_check: Option<&RealmSecretCheck>,
    ) -> Result<
        (
            register_response::RegisterOk,
            Option<ManufacturerAuthBinding>,
        ),
        ReissueFailure,
    > {
        if actor_id.realm != request.realm || actor_id.r#type != request.actr_type {
            platform::recording::warn!(
                "Credential reissue identity mismatch: actor={}, requested_type={}, requested_realm={}",
                actor_id.to_string_repr(),
                request.actr_type.to_string_repr(),
                request.realm.realm_id
            );
            return Err(AidError::InvalidFormat.into());
        }

        // P1-8: verify the manufacturer-auth nonce without consuming it. The
        // nonce is burned later in [`commit_reissue_response`] inside the same
        // transaction that caches the response, so a lost response leaves the
        // nonce unconsumed and the attempt retriable.
        let nonce_binding = self
            .verify_registration_identity(request, realm_secret_check)
            .await?;
        let proof = self
            .verify_actor_possession(actor_id, renewal_proof)
            .await?;

        let mut register_ok = self
            .issue_credential_for_actor(actor_id, request.target.as_deref())
            .await?;
        let (renewal_token, renewal_token_expires_at) =
            self.issue_reissue_renewal_token(actor_id, &proof).await?;
        register_ok.renewal_token = Some(renewal_token);
        register_ok.renewal_token_expires_at = Some(renewal_token_expires_at);
        Ok((register_ok, nonce_binding))
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

        // AIS-04 + P1-7: enforce the reissue proof replay window, now
        // distinguishing "delivered" from "consumed but not delivered".
        //
        // A successful reissue derives a deterministic successor from this proof;
        // that successor's row is the consumption marker. The `delivered_at`
        // column (set in the same transaction that caches the response) tells
        // the two cases apart:
        //
        // - `delivered_at` is `Some`: the response was committed to the cache
        //   and (presumably) sent. A replay reaching here means the cache
        //   expired — the proof is fully spent, so refuse to mint again. A
        //   legitimate retry would have hit the response cache in the handler
        //   before reaching this point.
        // - `delivered_at` is `None`: the successor row exists (a signing
        //   attempt consumed the proof) but the response-cache transaction did
        //   not commit — a lost response. Keep the original 5-minute replay
        //   window for this case: it is tight enough to stop a leaked proof
        //   from being chained, but wide enough for a client to retry after a
        //   transient failure. (The 30-day binding grace above already governs
        //   whether the proof is still resolvable at all.)
        let successor = crate::renewal::derive_successor_token(
            &self.config.renewal_token_secret,
            proof,
            actor_id,
            binding_expires_at,
        );
        let successor_hash = crate::renewal::hash_token(&successor);
        if let Some((consumed_at, delivered_at)) =
            crate::renewal::find_token_consumption_state(actor_id, &successor_hash)
                .await
                .map_err(|e| {
                    AidError::GenerationFailed(format!("reissue consumption lookup failed: {e}"))
                })?
        {
            let now = crate::renewal::now_secs();
            let spent = match delivered_at {
                Some(_) => true,
                None => {
                    now.saturating_sub(consumed_at)
                        > crate::renewal::REISSUE_PROOF_REPLAY_WINDOW_SECS
                }
            };
            if spent {
                platform::recording::warn!(
                    "Credential reissue rejected: possession proof already consumed for actor {}; delivered={:?}; refusing to chain a spent proof",
                    actor_id.to_string_repr(),
                    delivered_at.is_some()
                );
                return Err(ReissueFailure::PossessionNotProven);
            }
        }

        Ok(VerifiedPossession {
            token: proof.to_vec(),
            binding_expires_at,
        })
    }

    /// Internal processing logic (Ed25519 signing via the Signer Sign RPC).
    async fn issue_credential_inner(
        &self,
        request: &RegisterRequest,
        realm_secret_check: Option<&RealmSecretCheck>,
    ) -> Result<register_response::RegisterOk, AidError> {
        // Ensure a usable key is available.
        self.ensure_key_loaded().await?;

        let nonce_binding = self
            .verify_registration_identity(request, realm_secret_check)
            .await?;
        // Fresh registration burns the manufacturer-auth nonce immediately after
        // verification (no response cache on this path), matching the pre-P1-8
        // timing so a replayed /register is still rejected within the nonce TTL.
        if let Some(binding) = nonce_binding {
            let now = Self::unix_now_secs()?;
            Self::consume_manufacturer_auth_nonce(
                platform::storage::db::get_database().get_pool(),
                &binding.manufacturer,
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
            )
            .await?;

        // Build IdentityClaims (proto type, plaintext).
        let (key_id, verifying_key, expr_time) = self.signing_snapshot().await?;
        let claims_proto = IdentityClaims {
            realm_id: actr_id.realm.realm_id,
            actor_id: actr_id.to_string_repr(),
            expires_at: expr_time,
            protocol_version: 1,
            issuer: format!("actr:realm:{}:authority", actr_id.realm.realm_id),
            provider: "actrix-official".to_string(),
            audience: "actr.signaling".to_string(),
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
    /// Returns the manufacturer-auth nonce binding when the package-auth path was
    /// taken, so the caller can burn the nonce at the correct point in its flow
    /// (immediately for `/register`, in the response-cache transaction for
    /// `/reissue`). `None` for the Linked path (no single-use nonce).
    async fn verify_registration_identity(
        &self,
        request: &RegisterRequest,
        realm_secret_check: Option<&RealmSecretCheck>,
    ) -> Result<Option<ManufacturerAuthBinding>, AidError> {
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

        // Linked workloads must not claim a DNS-style publisher namespace: dotted
        // names (e.g. "apps.example.com") are reserved for Package identities whose
        // domain ownership is cryptographically verified. A realm secret is not
        // proof of owning a publisher domain, so allowing a dotted manufacturer
        // here would let any realm workload impersonate a published publisher's
        // service type and overwrite its ACL (AIS-01 / SIG-05).
        if request.actr_type.manufacturer.contains('.') {
            platform::recording::warn!(
                "Linked registration rejected: DNS-style publisher namespace is reserved for verified publishers, realm={}, type={}",
                request.realm.realm_id,
                request.actr_type.to_string_repr()
            );
            return Err(AidError::ManufacturerNotVerified);
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

    /// verify Publisher Registry identity
    ///
    /// Path 1: an existing package tuple. The active package must still have an
    /// active Publisher Registry and a non-revoked signing key, the request
    /// manifest must hash-match the stored one, and (AIS-02) the caller must
    /// prove current possession of the publisher's signing key with a fresh
    /// single-use nonce and a domain-separated `manufacturer_auth` signature —
    /// the same possession proof Path 2 requires. Revoked packages are terminal
    /// and cannot fall through.
    /// Path 2: only a never-published tuple may use manifest and request
    /// signatures plus the `manufacturer_auth` possession proof.
    /// Otherwise reject
    async fn verify_mfr_identity(
        &self,
        request: &RegisterRequest,
    ) -> Result<Option<ManufacturerAuthBinding>, AidError> {
        let actr_type = &request.actr_type;
        if actr_type.version.is_empty() {
            return Err(AidError::InvalidFormat);
        }

        let type_str = actr_type.to_program_identity();
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

        let published_pkg = actrix_mfr::model::ActrPackage::get_by_type_and_target_any_status(
            &pool, &type_str, target_ref,
        )
        .await
        .map_err(|e| {
            AidError::GenerationFailed(format!("Publisher Registry lookup failed: {e}"))
        })?;

        if let Some(pkg) = published_pkg {
            use sha2::{Digest, Sha256};

            if pkg.status == actrix_mfr::model::PkgStatus::Revoked {
                platform::recording::warn!(
                    "Publisher Registry table lookup rejected: package revoked, type_str={}, target={}",
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
                    "Publisher Registry table lookup rejected: package identity mismatch, type_str={}, target={}",
                    type_str,
                    target_ref
                );
                return Err(AidError::ManufacturerNotVerified);
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
                return Err(AidError::ManufacturerNotVerified);
            }

            let manager = actrix_mfr::MfrManager::new(pool.clone());
            let package_signing_key_id = manager
                .verify_published_package_signing_key(&pkg)
                .await
                .map_err(Self::map_mfr_verification_error)?;

            // AIS-02: an existing published package, the realm secret, and the
            // public manifest are no longer sufficient to register — all of them
            // are readable/holdable without controlling the publisher. The caller
            // must additionally prove current possession of the publisher's
            // signing key, exactly as the unpublished-package path (Path 2) does:
            // a fresh single-use nonce plus a domain-separated signature bound to
            // this realm, actr_type, target, and manifest hash. The manifest-hash
            // check above additionally pins that proof to the exact published
            // version.
            let (manufacturer_auth_signature, manufacturer_auth_signed_at, manufacturer_auth_nonce) =
                match (
                    &request.manufacturer_auth_signature,
                    request.manufacturer_auth_signed_at,
                    &request.manufacturer_auth_nonce,
                ) {
                    (Some(signature), Some(signed_at), Some(nonce)) => {
                        (signature, signed_at, nonce)
                    }
                    _ => {
                        platform::recording::warn!(
                            "Publisher Registry table lookup rejected: published package registration requires manufacturer_auth_signature, manufacturer_auth_signed_at, and manufacturer_auth_nonce; type_str={}, target={}",
                            type_str,
                            target_ref
                        );
                        return Err(AidError::ManufacturerNotVerified);
                    }
                };

            // Verify possession against the publisher's CURRENT signing key: a
            // publisher that has rotated its key must still be able to launch
            // instances of an already-published package, and control of the
            // current key is the strongest possession proof available. This can
            // differ from `package_signing_key_id`, which is the (possibly
            // retired) key that signed the stored manifest.
            let publisher = manager
                .resolve_by_name(mfr_name)
                .await
                .map_err(Self::map_mfr_verification_error)?;

            let manifest_sha256_hex = hex::encode(request_manifest_hash);
            let binding = Self::verify_manufacturer_auth(
                &pool,
                &publisher.public_key,
                mfr_name,
                &publisher.key_id,
                request.realm.realm_id,
                actr_type,
                target_ref,
                &manifest_sha256_hex,
                manufacturer_auth_signature.as_ref(),
                manufacturer_auth_signed_at,
                manufacturer_auth_nonce.as_ref(),
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
                    "Publisher Registry verification failed: unpublished package requires manifest_raw, mfr_signature, manufacturer_auth_signature, manufacturer_auth_signed_at, and manufacturer_auth_nonce; type_str={}",
                    type_str
                );
                return Err(AidError::ManufacturerNotVerified);
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
                "Publisher Registry signature verification error: publisher={}, err={}",
                mfr_name,
                e
            );
            AidError::GenerationFailed(format!("Signature verification error: {e}"))
        })?;

        if !valid {
            platform::recording::warn!(
                "Publisher Registry signature verification failed: invalid signature, type_str={}",
                type_str
            );
            return Err(AidError::ManufacturerNotVerified);
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
        if m_publisher != actr_type.manufacturer
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
            return Err(AidError::ManufacturerNotVerified);
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
                return Err(AidError::ManufacturerNotVerified);
            }
        }
        let manifest_sha256_hex = {
            use sha2::{Digest, Sha256};
            hex::encode(Sha256::digest(manifest_bytes.as_ref()))
        };
        let binding = Self::verify_manufacturer_auth(
            &pool,
            &mfr_info.public_key,
            mfr_name,
            &signing_key_id,
            request.realm.realm_id,
            actr_type,
            manifest_target,
            &manifest_sha256_hex,
            manufacturer_auth_signature.as_ref(),
            manufacturer_auth_signed_at,
            manufacturer_auth_nonce.as_ref(),
        )
        .await?;

        platform::recording::debug!(
            "Publisher Registry manifest and manufacturer signature verification passed, type_str={}",
            type_str
        );
        Ok(Some(binding))
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
                platform::recording::warn!("Publisher Registry verification rejected: {}", message);
                AidError::ManufacturerNotVerified
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

    /// Verify a publisher-possession proof (`manufacturer_auth`) for package
    /// registration and check — but not consume — its one-time nonce.
    ///
    /// Shared by both package registration paths. Whether or not a registry row
    /// already exists, an authorized publisher launch is distinguished from a
    /// caller who merely holds the realm secret and the public manifest by a
    /// fresh single-use nonce plus a domain-separated signature binding this
    /// realm, actr_type, target, and manifest hash to the publisher's signing
    /// key. `signing_key_id` scopes the nonce and is recorded for audit.
    ///
    /// P1-8: the nonce is **not** burned here. Verification only checks that an
    /// unexpired row does not already exist for this nonce. The caller is
    /// responsible for consuming the nonce (INSERT) at the correct point:
    /// `/register` burns it immediately after this call returns; `/reissue`
    /// defers the burn to the response-cache transaction so a lost response
    /// leaves the nonce unconsumed and the attempt retriable. The returned
    /// [`ManufacturerAuthBinding`] carries the identity needed for that
    /// deferred INSERT.
    #[allow(clippy::too_many_arguments)]
    async fn verify_manufacturer_auth(
        pool: &sqlx::SqlitePool,
        publisher_public_key: &str,
        mfr_name: &str,
        signing_key_id: &str,
        realm_id: u32,
        actr_type: &ActrType,
        target: &str,
        manifest_sha256_hex: &str,
        manufacturer_auth_signature: &[u8],
        manufacturer_auth_signed_at: u64,
        manufacturer_auth_nonce: &[u8],
    ) -> Result<ManufacturerAuthBinding, AidError> {
        if !matches!(manufacturer_auth_nonce.len(), 16 | 32) {
            platform::recording::warn!(
                "manufacturer_auth_nonce has invalid length: len={}, publisher={}",
                manufacturer_auth_nonce.len(),
                mfr_name
            );
            return Err(AidError::InvalidFormat);
        }

        let now = Self::unix_now_secs()?;
        Self::verify_manufacturer_auth_signed_at(manufacturer_auth_signed_at, now)?;

        let manufacturer_payload = actr_protocol::build_manufacturer_register_payload(
            actr_protocol::ManufacturerRegisterPayload {
                realm_id,
                actr_type,
                target,
                manifest_sha256_hex,
                manufacturer_auth_signed_at,
                manufacturer_auth_nonce,
            },
        );
        let manufacturer_sig_b64 =
            base64::prelude::BASE64_STANDARD.encode(manufacturer_auth_signature);
        let manufacturer_valid = actrix_mfr::crypto::verify_signature(
            manufacturer_payload.as_bytes(),
            &manufacturer_sig_b64,
            publisher_public_key,
        )
        .map_err(|e| {
            platform::recording::warn!(
                "manufacturer_auth_signature verification error: publisher={}, key_id={}, err={}",
                mfr_name,
                signing_key_id,
                e
            );
            AidError::GenerationFailed(format!("Manufacturer signature verification error: {e}"))
        })?;

        if !manufacturer_valid {
            platform::recording::warn!(
                "manufacturer_auth_signature verification failed: publisher={}, key_id={}, actr_type={}",
                mfr_name,
                signing_key_id,
                actr_type.to_string_repr()
            );
            return Err(AidError::ManufacturerNotVerified);
        }

        // P1-8: check-only — reject an already-consumed (still-unexpired) nonce
        // without burning it. Consumption is deferred to the caller's commit
        // transaction. An expired nonce row is treated as unused: the GC in the
        // commit transaction reclaims it, and the P1-7 binding-grace window
        // governs long-lived retries instead of the 5-minute replay window.
        let now_i64 = i64::try_from(now)
            .map_err(|_| AidError::InvalidTimestamp("current time exceeds i64".to_string()))?;
        let already_consumed: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM ais_manufacturer_auth_nonce
             WHERE manufacturer = ?1 AND key_id = ?2 AND nonce = ?3 AND expires_at > ?4)",
        )
        .bind(mfr_name)
        .bind(signing_key_id)
        .bind(manufacturer_auth_nonce)
        .bind(now_i64)
        .fetch_one(pool)
        .await
        .map_err(|e| AidError::GenerationFailed(format!("manufacturer nonce check failed: {e}")))?;
        if already_consumed {
            platform::recording::warn!(
                "manufacturer_auth_nonce replay detected during verify: publisher={}, key_id={}",
                mfr_name,
                signing_key_id
            );
            return Err(AidError::ManufacturerNotVerified);
        }

        Ok(ManufacturerAuthBinding {
            manufacturer: mfr_name.to_string(),
            signing_key_id: signing_key_id.to_string(),
            nonce: manufacturer_auth_nonce.to_vec(),
        })
    }

    async fn consume_manufacturer_auth_nonce(
        pool: &sqlx::SqlitePool,
        manufacturer: &str,
        signing_key_id: &str,
        manufacturer_auth_nonce: &[u8],
        now: u64,
    ) -> Result<(), AidError> {
        let now_i64 = i64::try_from(now)
            .map_err(|_| AidError::InvalidTimestamp("current time exceeds i64".to_string()))?;
        let expires_at = now_i64.saturating_add(MANUFACTURER_NONCE_TTL_SECS as i64);

        sqlx::query("DELETE FROM ais_manufacturer_auth_nonce WHERE expires_at < ?")
            .bind(now_i64)
            .execute(pool)
            .await
            .map_err(|e| {
                AidError::GenerationFailed(format!("manufacturer nonce cleanup failed: {e}"))
            })?;

        let result = sqlx::query(
            "INSERT INTO ais_manufacturer_auth_nonce (manufacturer, key_id, nonce, created_at, expires_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(manufacturer)
        .bind(signing_key_id)
        .bind(manufacturer_auth_nonce)
        .bind(now_i64)
        .bind(expires_at)
        .execute(pool)
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(sqlx::Error::Database(err)) if err.is_unique_violation() => {
                platform::recording::warn!(
                    "manufacturer_auth_nonce replay detected: publisher={}, key_id={}",
                    manufacturer,
                    signing_key_id
                );
                Err(AidError::ManufacturerNotVerified)
            }
            Err(err) => Err(AidError::GenerationFailed(format!(
                "manufacturer nonce consume failed: {err}"
            ))),
        }
    }

    /// Generate ActrId.
    async fn generate_actr_id(
        &self,
        actr_type: &ActrType,
        realm: &Realm,
        target: Option<&str>,
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
            let serial_number = SerialNumber::sn(realm.realm_id).value();
            let reserved = sqlx::query(
                "INSERT OR IGNORE INTO issued_actor_identity
                 (realm_id, serial_number, actor_type, target, issued_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(i64::from(realm.realm_id))
            .bind(serial_number as i64)
            .bind(&actor_type)
            .bind(target)
            .bind(issued_at)
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
                    realm: *realm,
                    serial_number,
                    r#type: actr_type.clone(),
                });
            }
        }

        Err(AidError::GenerationFailed(
            "failed to reserve a unique Actor ID after 1024 Snowflake attempts".to_string(),
        ))
    }

    /// Snapshot a signing identity and cap credential lifetime at the signing
    /// key lifetime. This prevents a credential from outliving its trust key.
    async fn signing_snapshot(&self) -> Result<(u32, VerifyingKey, u64), AidError> {
        let now = now_secs();
        let cache = self.key_cache.read().await;
        let cache = cache
            .as_ref()
            .ok_or_else(|| AidError::GenerationFailed("No key available".to_string()))?;
        if !key_usable_for_minting(cache.expires_at, now) {
            return Err(AidError::GenerationFailed(
                "Signing key expired (or too close to expiry) before credential issuance"
                    .to_string(),
            ));
        }
        let credential_expires_at =
            credential_expiry_for_key(now, self.config.token_ttl_secs, cache.expires_at);
        if credential_expires_at <= now {
            return Err(AidError::GenerationFailed(
                "Signing key has no remaining credential lifetime".to_string(),
            ));
        }
        Ok((cache.key_id, cache.verifying_key, credential_expires_at))
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

    /// Issue the renewal token that accompanies a credential reissue.
    ///
    /// Unlike [`Self::issue_fresh_renewal_token`], the successor is derived
    /// deterministically from the presented possession proof rather than drawn
    /// from the CSPRNG. That is what makes `/ais/reissue` safe to retry: a
    /// client whose response was lost replays the same proof and re-derives the
    /// same successor, so the binding it is still holding is never displaced by
    /// a competing token. A fast replay reuses the still-live row unchanged; a
    /// replay after that one-day successor expired (while the original proof is
    /// still inside the reissue grace) refreshes the row's expiry rather than
    /// failing — so identity recovery survives any number of lost responses.
    async fn issue_reissue_renewal_token(
        &self,
        actor_id: &ActrId,
        proof: &VerifiedPossession,
    ) -> Result<(Bytes, Timestamp), AidError> {
        if self.config.renewal_token_secret.is_empty() {
            return Err(AidError::GenerationFailed(
                "renewal_token_secret is required".to_string(),
            ));
        }

        let token = crate::renewal::derive_successor_token(
            &self.config.renewal_token_secret,
            &proof.token,
            actor_id,
            proof.binding_expires_at,
        );
        let token_hash = crate::renewal::hash_token(&token);

        // Reissue accepts proofs that already expired (anything inside the
        // binding grace window), so anchoring the successor on the proof's
        // expiry alone could mint an already-dead token. Take whichever of the
        // proof expiry and now is later; only the first attempt's value is
        // ever persisted.
        let anchor = proof.binding_expires_at.max(crate::renewal::now_secs());
        let expires_at =
            crate::renewal::successor_expires_at(anchor, self.config.renewal_token_ttl_secs);

        // Upsert rather than `INSERT OR IGNORE`: if the first response was lost
        // and this one-day successor already expired while the original proof is
        // still inside the reissue grace, the deterministic row is refreshed to
        // a live expiry. `INSERT OR IGNORE` would keep the dead row and the
        // unexpired read-back below would return `None`, producing a spurious
        // 500 that permanently blocks identity recovery. A still-live row is
        // left untouched so a fast replay still observes the first expiry.
        crate::renewal::upsert_reissue_successor_token(actor_id, &token_hash, expires_at)
            .await
            .map_err(|e| {
                AidError::GenerationFailed(format!("Failed to persist renewal token: {e}"))
            })?;

        // Read the expiry back instead of returning the value just computed:
        // on a fast replay the row was left untouched, and the caller must
        // observe the expiry AIS actually recorded for it.
        let stored = crate::renewal::find_unexpired_token(actor_id, &token_hash)
            .await
            .map_err(|e| {
                AidError::GenerationFailed(format!("Failed to read back renewal token: {e}"))
            })?
            .ok_or_else(|| {
                AidError::GenerationFailed(
                    "renewal token missing immediately after insert".to_string(),
                )
            })?;

        // Deliberately no trim here. The proof must stay resolvable while its
        // replay window is open, otherwise a lost-response retry of this same
        // attempt would be rejected — the exact failure this endpoint exists to
        // prevent. The persisted successor row (this row) also doubles as the
        // proof's consumption marker: once its `created_at` is older than
        // [`crate::renewal::REISSUE_PROOF_REPLAY_WINDOW_SECS`],
        // `verify_actor_possession` refuses to accept the original proof again,
        // so a spent proof can no longer be chained. Growth stays bounded
        // because a replay inserts nothing and `gc_expired_tokens` reclaims rows
        // past the grace.

        Ok((
            Bytes::from(token),
            Timestamp {
                seconds: stored.expires_at,
                nanos: 0,
            },
        ))
    }

    // -------- /reissue idempotency (P1-8) ---------------------------------

    /// Look up a cached `/reissue` response by the manufacturer-auth nonce.
    ///
    /// Returns the decrypted [`RegisterResponse`] if a cache entry for this
    /// nonce exists and has not expired. The handler calls this before
    /// invoking the full reissue path so a lost-response retry replays the
    /// exact credential signed on the first attempt instead of re-signing.
    pub async fn lookup_reissue_cache(&self, nonce: &[u8]) -> Option<RegisterResponse> {
        if !matches!(nonce.len(), 16 | 32) {
            return None;
        }
        let pool = platform::storage::db::get_database().get_pool();
        let nonce_hash = crate::renewal::hash_token(nonce);
        let now = crate::renewal::now_secs();

        let ciphertext: Vec<u8> = sqlx::query_scalar(
            "SELECT response_ciphertext FROM ais_reissue_response_cache
             WHERE nonce_hash = ?1 AND expires_at > ?2",
        )
        .bind(&nonce_hash)
        .bind(now)
        .fetch_optional(pool)
        .await
        .ok()??;

        let plaintext = self.decrypt_reissue_response(&ciphertext).ok()?;
        RegisterResponse::decode(plaintext.as_slice()).ok()
    }

    /// Derive the AES-256-GCM key for the reissue response cache from the
    /// renewal token secret via a domain-separated SHA-256 label. This key
    /// cannot collide with the HMAC successor derivation in
    /// [`crate::renewal::derive_successor_token`], which uses a different
    /// label over the same secret.
    fn reissue_cache_key(&self) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(b"actrix/ais/reissue-response-cache/v1");
        hasher.update(&self.config.renewal_token_secret);
        hasher.finalize().into()
    }

    /// Encrypt a response with AES-256-GCM. Layout: `version(1) || nonce(12)
    /// || ciphertext`. The version byte lets the format evolve without
    /// ambiguously failing decryption on old rows.
    fn encrypt_reissue_response(&self, plaintext: &[u8]) -> Result<Vec<u8>, AidError> {
        use aes_gcm::aead::{Aead, KeyInit, Payload};
        let key = self.reissue_cache_key();
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce_bytes),
                Payload {
                    msg: plaintext,
                    aad: b"actrix/ais/reissue-response/v1",
                },
            )
            .map_err(|e| {
                AidError::GenerationFailed(format!("reissue response encryption failed: {e}"))
            })?;
        let mut stored = Vec::with_capacity(1 + nonce_bytes.len() + ciphertext.len());
        stored.push(REISSUE_CIPHERTEXT_VERSION);
        stored.extend_from_slice(&nonce_bytes);
        stored.extend_from_slice(&ciphertext);
        Ok(stored)
    }

    /// Decrypt a cached response previously produced by
    /// [`encrypt_reissue_response`].
    fn decrypt_reissue_response(&self, stored: &[u8]) -> Result<Vec<u8>, AidError> {
        use aes_gcm::aead::{Aead, KeyInit, Payload};
        if stored.len() <= 13 {
            return Err(AidError::GenerationFailed(
                "reissue cache entry too short".to_string(),
            ));
        }
        if stored[0] != REISSUE_CIPHERTEXT_VERSION {
            return Err(AidError::GenerationFailed(format!(
                "reissue cache ciphertext version mismatch: {}",
                stored[0]
            )));
        }
        let key = self.reissue_cache_key();
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
        cipher
            .decrypt(
                Nonce::from_slice(&stored[1..13]),
                Payload {
                    msg: &stored[13..],
                    aad: b"actrix/ais/reissue-response/v1",
                },
            )
            .map_err(|e| {
                AidError::GenerationFailed(format!("reissue response decryption failed: {e}"))
            })
    }

    /// Atomically consume the manufacturer-auth nonce, cache the encrypted
    /// response, and mark the successor row delivered — all in one `BEGIN
    /// IMMEDIATE` transaction (P1-8).
    ///
    /// A retry of the same attempt hits [`lookup_reissue_cache`] and replays
    /// the cached response without re-signing. If this transaction never
    /// committed (a lost response), the nonce was never burned and the P1-7
    /// grace window lets the client retry instead of being locked out by the
    /// 5-minute replay window.
    async fn commit_reissue_response(
        &self,
        binding: &ManufacturerAuthBinding,
        actor_id: &ActrId,
        successor_hash: &[u8],
        response: &RegisterResponse,
        target: Option<&str>,
    ) -> Result<(), AidError> {
        let pool = platform::storage::db::get_database().get_pool();
        let now = crate::renewal::now_secs();
        let nonce_expires = now.saturating_add(MANUFACTURER_NONCE_TTL_SECS as i64);
        let cache_expires = now.saturating_add(REISSUE_RESPONSE_CACHE_TTL_SECS);

        let response_bytes = response.encode_to_vec();
        let ciphertext = self.encrypt_reissue_response(&response_bytes)?;
        let nonce_hash = crate::renewal::hash_token(&binding.nonce);

        let mut tx = pool
            .begin()
            .await
            .map_err(|e| AidError::GenerationFailed(format!("reissue cache begin failed: {e}")))?;

        // GC expired nonce and cache rows so the unique-constraint INSERT
        // below is not blocked by a stale row for the same nonce.
        sqlx::query("DELETE FROM ais_manufacturer_auth_nonce WHERE expires_at < ?")
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| AidError::GenerationFailed(format!("nonce GC failed: {e}")))?;
        sqlx::query("DELETE FROM ais_reissue_response_cache WHERE expires_at < ?")
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| AidError::GenerationFailed(format!("reissue cache GC failed: {e}")))?;

        // Consume the nonce (replay guard). A unique violation means a
        // concurrent request or an unexpired replay.
        let result = sqlx::query(
            "INSERT INTO ais_manufacturer_auth_nonce
                (manufacturer, key_id, nonce, created_at, expires_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&binding.manufacturer)
        .bind(&binding.signing_key_id)
        .bind(&binding.nonce)
        .bind(now)
        .bind(nonce_expires)
        .execute(&mut *tx)
        .await;
        match result {
            Ok(_) => {}
            Err(sqlx::Error::Database(err)) if err.is_unique_violation() => {
                platform::recording::warn!(
                    "manufacturer_auth_nonce replay detected during commit: publisher={}, key_id={}",
                    binding.manufacturer,
                    binding.signing_key_id
                );
                return Err(AidError::ManufacturerNotVerified);
            }
            Err(err) => {
                return Err(AidError::GenerationFailed(format!(
                    "manufacturer nonce consume failed: {err}"
                )));
            }
        }

        // Cache the encrypted response keyed by nonce hash.
        sqlx::query(
            "INSERT INTO ais_reissue_response_cache
                (nonce_hash, response_ciphertext, expires_at, created_at)
             VALUES (?, ?, ?, ?)",
        )
        .bind(&nonce_hash)
        .bind(&ciphertext)
        .bind(cache_expires)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| AidError::GenerationFailed(format!("reissue cache insert failed: {e}")))?;

        // Mark the successor row as delivered (P1-7).
        crate::renewal::mark_token_delivered(&mut tx, actor_id, successor_hash, now)
            .await
            .map_err(|e| AidError::GenerationFailed(format!("delivered_at mark failed: {e}")))?;

        // Persist the package target this reissue was authorized under (P1-6).
        // `verify_manufacturer_auth` bound the signature to this target, so a
        // legitimate migration to a new (still-active) target updates the actor's
        // binding; without this, /renew would keep rechecking the stale
        // registration-time target. Only package-auth reissue reaches here, so
        // target is non-empty; Linked reissue (no nonce) skips this entirely.
        if let Some(target) = target.filter(|t| !t.is_empty()) {
            sqlx::query(
                "UPDATE issued_actor_identity SET target = ?1
                 WHERE realm_id = ?2 AND serial_number = ?3",
            )
            .bind(target)
            .bind(i64::from(actor_id.realm.realm_id))
            .bind(actor_id.serial_number as i64)
            .execute(&mut *tx)
            .await
            .map_err(|e| AidError::GenerationFailed(format!("target persist failed: {e}")))?;
        }

        tx.commit()
            .await
            .map_err(|e| AidError::GenerationFailed(format!("reissue cache commit failed: {e}")))?;
        Ok(())
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
    /// - AIS-03: if this identity corresponds to a published package, that
    ///   package must not have been revoked and its publisher must not have been
    ///   suspended or revoked since the credential was first issued. Linked and
    ///   local identities have no package row and skip the package/publisher
    ///   check.
    async fn recheck_existing_actor(
        &self,
        actor_id: &ActrId,
        target: Option<&str>,
    ) -> Result<(), AidError> {
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
        platform::realm::Realm::validate_realm(actor_id.realm.realm_id)
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
        let pool = platform::storage::db::get_database().get_pool().clone();
        let type_str = actor_id.r#type.to_program_identity();
        // Any package row (regardless of lifecycle status) for this type marks a
        // published-package identity. `mfr_package.status` stores 'active' or
        // 'revoked'. When the target is known (persisted at registration time),
        // scope the recheck to that exact target so revoking one target of a
        // multi-target type does not get masked by a sibling target that is still
        // active. Without target (legacy rows), fall back to type-only matching.
        let statuses: Vec<String> = if let Some(target) = target.filter(|t| !t.is_empty()) {
            sqlx::query_scalar("SELECT status FROM mfr_package WHERE type_str = ?1 AND target = ?2")
                .bind(&type_str)
                .bind(target)
                .fetch_all(&pool)
                .await
                .map_err(|e| {
                    AidError::GenerationFailed(format!("package status lookup failed: {e}"))
                })?
        } else {
            platform::recording::warn!(
                "Recheck without target for type {}: per-target revocation cannot be enforced; falling back to type-only lookup",
                type_str
            );
            sqlx::query_scalar("SELECT status FROM mfr_package WHERE type_str = ?1")
                .bind(&type_str)
                .fetch_all(&pool)
                .await
                .map_err(|e| {
                    AidError::GenerationFailed(format!("package status lookup failed: {e}"))
                })?
        };

        if statuses.is_empty() {
            // No package tuple: a linked/local identity has no package lifecycle
            // to recheck.
            return Ok(());
        }

        let has_active = statuses.iter().any(|status| status == "active");
        if !has_active {
            platform::recording::warn!(
                "Credential issuance rejected: every package for type {} (target={:?}) has been revoked",
                type_str,
                target
            );
            return Err(AidError::PackageRevoked);
        }

        // The package is still active; its publisher must still be active too.
        let publisher =
            actrix_mfr::model::Manufacturer::get_by_name(&pool, &actor_id.r#type.manufacturer)
                .await
                .map_err(|e| {
                    AidError::GenerationFailed(format!("publisher status lookup failed: {e}"))
                })?;
        match publisher {
            Some(mfr) if mfr.status == actrix_mfr::model::MfrStatus::Active => Ok(()),
            Some(mfr) => {
                platform::recording::warn!(
                    "Credential issuance rejected: publisher {} for type {} is not active (status={})",
                    actor_id.r#type.manufacturer,
                    type_str,
                    mfr.status
                );
                Err(AidError::ManufacturerNotVerified)
            }
            None => {
                platform::recording::warn!(
                    "Credential issuance rejected: publisher {} for active package type {} no longer exists",
                    actor_id.r#type.manufacturer,
                    type_str
                );
                Err(AidError::ManufacturerNotVerified)
            }
        }
    }

    /// Re-issue a credential for an existing ActrId (used by /ais/renew).
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
        self.recheck_existing_actor(actor_id, target).await?;

        // Ensure a signing key is available.
        self.ensure_key_loaded().await?;

        // Build IdentityClaims.
        let (key_id, verifying_key, expr_time) = self.signing_snapshot().await?;
        let claims_proto = IdentityClaims {
            realm_id: actor_id.realm.realm_id,
            actor_id: actor_id.to_string_repr(),
            expires_at: expr_time,
            protocol_version: 1,
            issuer: format!("actr:realm:{}:authority", actor_id.realm.realm_id),
            provider: "actrix-official".to_string(),
            audience: "actr.signaling".to_string(),
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
    /// `password = base64(HMAC-SHA1(turn_secret, username))`
    fn generate_turn_credential(
        &self,
        actor_id: &ActrId,
        expires_at: u64,
    ) -> actr_protocol::TurnCredential {
        let realm_id = actor_id.realm.realm_id;
        let actor_id = actor_id.to_string_repr();
        let provider = "actrix-official";
        let audience = "actr.turn";
        let username = format!(
            "{expires_at}:realm={realm_id}:provider={provider}:audience={audience}:actor={actor_id}"
        );
        let mut mac = HmacSha1::new_from_slice(self.config.turn_secret.as_bytes())
            .expect("HMAC-SHA1 accepts any key length");
        mac.update(username.as_bytes());
        let result = mac.finalize();
        let password = BASE64_STANDARD.encode(result.into_bytes());

        actr_protocol::TurnCredential {
            username,
            password,
            expires_at,
            protocol_version: 1,
            realm_id,
            provider: provider.to_string(),
            audience: audience.to_string(),
        }
    }

    // ========== Health check methods ==========

    /// Check database health status.
    pub async fn check_database_health(&self) -> Result<(), AidError> {
        self.key_storage
            .health_check()
            .await
            .map_err(|e| AidError::GenerationFailed(format!("Database unhealthy: {e}")))
    }

    /// Check Signer service health status.
    ///
    /// Verifies Signer service availability via the health_check RPC.
    pub async fn check_ks_health(&self) -> Result<(), AidError> {
        self.signer_client
            .health_check()
            .await
            .map(|_| ())
            .map_err(|e| AidError::GenerationFailed(format!("Signer service unhealthy: {e}")))
    }

    /// Check key cache health status.
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

/// Key cache health information.
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
