//! Integration points for mutual authentication and Attach.
//!
//! Documents where each component calls into the session protocol,
//! mapped to the Tab lifecycle from `08-workspace-lifecycle-and-storage.md` §5.
//!
//! # Call sites
//!
//! ```text
//! Tab lifecycle event          Code path (expected location)
//! ─────────────────────────    ──────────────────────────────────────────────
//! Tab create (new instance)    → Shell::create_instance()
//!                                 → FrontHost::new()
//!                                 → FrontHost::connect_and_attach()
//!                                 → (handshake) → SessionRegistry::attach()
//!                                 → FrontHost::load_guest()
//!
//! Tab open (existing instance) → Shell::open_instance()
//!                                 → FrontHost::new()
//!                                 → FrontHost::connect_and_attach()
//!                                 → (handshake) → SessionRegistry::attach()
//!                                 → FrontHost::load_guest()
//!
//! Tab refresh                  → FrontHost::destroy()
//!                                 → FrontHost::new()
//!                                 → FrontHost::connect_and_attach()
//!                                 → (handshake) → SessionRegistry::attach()
//!                                 → FrontHost::load_guest()
//!
//! Tab background               → FrontHost::pause()
//!                                 (NO Detach; lease stays active via Heartbeat)
//!
//! Tab foreground               → FrontHost::resume()
//!                                 → SessionRegistry::resume() (if channel alive)
//!                                 → SessionRegistry::attach()  (if lease expired)
//!
//! Tab close                    → FrontHost::detach()
//!                                 → SessionRegistry::detach()
//!                                 → FrontHost::destroy()
//!
//! Front crash                  → (no clean Detach)
//!                                 → LeaseManager::on_grace_expired()
//!                                 → SessionRegistry::invalidate_session()
//!
//! Service stop                 → ServiceHost::stop_accepting_attach()
//!                                 → SessionRegistry::invalidate_all()
//!                                 → Workload::on_stop()
//!
//! Service restart              → ServiceHost::restart()
//!                                 → SessionRegistry::invalidate_all()
//!                                 → (Fronts re-attach via Tab open/refresh)
//!
//! Heartbeat (periodic)         → FrontHost::heartbeat_timer()
//!                                 → SessionRegistry::heartbeat()
//!
//! Takeover (user action)       → FrontHost::request_takeover()
//!                                 → SessionRegistry::takeover()
//! ```
//!
//! # What this crate does NOT do
//!
//! - Does not drive the handshake state machine (Actr Host does).
//! - Does not create or destroy Front Actors (Shell/FrontHost does).
//! - Does not load or execute Guest code (FrontHost does after Attach).
//! - Does not call Workload lifecycle hooks (ServiceHost does).
//!
//! # What this crate DOES do
//!
//! - Defines the types all call sites agree on.
//! - Defines the state machine for lease lifecycle.
//! - Defines the handshake message structures.
//! - Provides `SessionRegistry` trait (implemented by Actrium runtime).
//! - Provides `VerifiedChannelContext` trait (implemented by Actr Host).
//!
//! # Example: Tab create → Attach → Guest load
//!
//! ```ignore
//! // Shell calls when user opens an instance link:
//! let front = FrontHost::new(tab_id, package, service_actor_id);
//!
//! // Front Host drives handshake (via Actr Host):
//! let channel = front.connect(service_actor_id).await?; // → VerifiedChannel
//!
//! // Front Host calls Attach:
//! let response = front.attach(&channel).await?;
//!
//! // Front Host installs capabilities and starts Guest:
//! front.install_capabilities(&response.session_capability_ceiling);
//! front.load_guest().await?; // → Keel Engine starts executing Guest
//! ```
//!
//! # Example: Tab close → Detach
//!
//! ```ignore
//! // Shell calls when user closes a Tab:
//! front.detach().await?; // → SessionRegistry::detach()
//! front.destroy().await?; // → Front Actor destroyed, Guest Engine stopped
//! // Service continues running. Workspace unchanged.
//! ```
//!
//! # Example: Front crash → grace → recovery
//!
//! ```ignore
//! // Front crashes; no Detach sent.
//! // Service Host's LeaseManager detects missed heartbeats:
//! //   Active → Grace (heartbeat missed)
//! //   Grace → Idle  (grace period expired)
//!
//! // User reopens the Tab:
//! let front = FrontHost::new(tab_id, package, service_actor_id);
//! let channel = front.connect(service_actor_id).await?;
//! let response = front.attach(&channel).await?; // fresh Attach
//! front.load_guest().await?;
//! ```

use super::channel::VerifiedChannel;
use super::{
    AttachErrorCode, AttachRequest, AttachResponse, AuthorizeAttach, AuthorizeAttachDecision,
    DetachRequest, HeartbeatRequest, ResumeRequest, SessionStatus, TakeoverRequest,
};

// ---------------------------------------------------------------------------
// Front Host client (caller side)
// ---------------------------------------------------------------------------

/// Trait that the Front Host implements to drive Attach and session lifecycle.
///
/// The Front Host is responsible for:
/// 1. Connecting to the Service and completing mutual authentication.
/// 2. Constructing and sending AttachRequest.
/// 3. Installing capabilities after Attach succeeds.
/// 4. Starting Guest execution.
/// 5. Sending Heartbeat, Resume, and Detach.
///
/// The Front Host does NOT implement the handshake protocol itself; that is
/// handled by the Actr Host. The Front Host calls `connect()` on the Actr Host
/// and receives a `VerifiedChannel`.
///
/// Tab lifecycle mapping (from `08 §5`):
/// - `connect_and_attach()` is called on Tab create, open, and refresh.
/// - `detach()` is called on Tab close.
/// - `pause()` is called on Tab background (no Detach).
/// - `resume()` is called on Tab foreground.
#[async_trait::async_trait]
pub trait FrontSessionClient: Send {
    /// Connect to the Service and complete mutual authentication.
    ///
    /// This drives the four-step handshake and returns a VerifiedChannel.
    /// The Actr Host performs the actual protocol; this method orchestrates it.
    async fn connect(&mut self, service_actor_id: &str)
    -> Result<VerifiedChannel, AttachErrorCode>;

    /// Attach to the Service over an already-verified channel.
    ///
    /// Constructs AttachRequest from verified local state and channel context,
    /// sends it to the Service, and validates the response.
    ///
    /// On success, the caller MUST:
    /// 1. Install the Capability Broker.
    /// 2. Install session scope and currently granted handles.
    /// 3. THEN load and execute the Guest.
    async fn attach(
        &mut self,
        channel: &VerifiedChannel,
    ) -> Result<AttachResponse, AttachErrorCode>;

    /// Renew the lease via heartbeat. Does NOT extend credential expiry.
    async fn heartbeat(&self, request: HeartbeatRequest) -> Result<(), AttachErrorCode>;

    /// Resume after transient disconnect. Requires a new VerifiedChannel.
    async fn resume(&mut self, request: ResumeRequest) -> Result<(), AttachErrorCode>;

    /// Detach and release the lease. Called on Tab close.
    /// Does NOT stop the Service.
    async fn detach(&self, request: DetachRequest) -> Result<(), AttachErrorCode>;

    /// Request takeover of the exclusive lease from another Front.
    async fn takeover(
        &mut self,
        request: TakeoverRequest,
    ) -> Result<AttachResponse, AttachErrorCode>;
}

// ---------------------------------------------------------------------------
// Service Host handler (receiver side)
// ---------------------------------------------------------------------------

/// Trait that the Service Host implements to accept Attach and manage sessions.
///
/// The Service Host is responsible for:
/// 1. Accepting connections and completing mutual authentication (via Actr Host).
/// 2. Processing AttachRequest through the SessionRegistry.
/// 3. Calling the Service workload's AuthorizeAttach hook.
/// 4. Managing exclusive lease lifecycle.
/// 5. Guarding every business RPC through SessionGuard.
///
/// The Service Host does NOT implement the handshake protocol itself; that is
/// handled by the Actr Host.
///
/// Tab lifecycle mapping (from `08 §5`):
/// - `attach()` is called when any Front connects (Tab create, open, refresh).
/// - `detach()` is called when a Front explicitly disconnects (Tab close).
/// - `invalidate_all()` is called on Service stop or restart.
/// - The Service workload's `on_start`/`on_stop` are NOT called per-Attach.
#[async_trait::async_trait]
pub trait ServiceSessionHandler: Send + Sync {
    /// Process an AttachRequest from a verified Front.
    ///
    /// Performs the 13 checks (see proposal §6.2), calls the Service workload's
    /// AuthorizeAttach hook, and atomically creates a session with an exclusive
    /// lease.
    ///
    /// Returns the AttachResponse on success, or an error code on failure.
    /// On failure, no state is mutated and no Guest is executed.
    async fn attach(
        &self,
        request: AttachRequest,
        channel: &VerifiedChannel,
    ) -> Result<AttachResponse, AttachErrorCode>;

    /// Process a heartbeat to renew the lease.
    async fn heartbeat(&self, request: HeartbeatRequest) -> Result<(), AttachErrorCode>;

    /// Process a resume request over a new VerifiedChannel.
    async fn resume(
        &self,
        request: ResumeRequest,
        channel: &VerifiedChannel,
    ) -> Result<(), AttachErrorCode>;

    /// Process a detach request and release the lease.
    async fn detach(&self, request: DetachRequest) -> Result<(), AttachErrorCode>;

    /// Process a takeover request.
    async fn takeover(
        &self,
        request: TakeoverRequest,
        channel: &VerifiedChannel,
    ) -> Result<AttachResponse, AttachErrorCode>;

    /// Invalidate all sessions (called on Service stop or restart).
    async fn invalidate_all(&self);

    /// Query the current session status.
    async fn status(&self, session_id: &str) -> Result<SessionStatus, AttachErrorCode>;

    /// Call the Service workload's application-level authorization hook.
    ///
    /// This is a per-Attach, request-scoped hook. It is NOT a lifecycle method.
    /// The Service Host calls this AFTER generic checks pass and BEFORE
    /// allocating the lease.
    async fn authorize_attach(
        &self,
        context: AuthorizeAttach,
    ) -> Result<AuthorizeAttachDecision, AttachErrorCode>;
}

// ---------------------------------------------------------------------------
// Tab lifecycle integration notes
// ---------------------------------------------------------------------------

/// Documents the exact call points in the Tab lifecycle.
///
/// These are the integration points that Shell and FrontHost code must
/// implement. The types and traits in this crate define the contract;
/// the actual calling code lives in the platform layers.
///
/// ## Tab create / open / refresh
///
/// ```text
/// Shell::open_instance(realm_link)
///   → Shell::verify_profile_and_package()
///   → Shell::resolve_service_actor_id()
///   → FrontHost::new(tab_id, package, service_actor_id)
///   → FrontHost::connect(service_actor_id)     // drives handshake
///   → FrontHost::attach(&channel)               // calls ServiceSessionHandler::attach()
///   → FrontHost::install_capabilities()
///   → FrontHost::load_guest()                   // Keel Engine starts
/// ```
///
/// ## Tab close
///
/// ```text
/// Shell::close_tab(tab_id)
///   → FrontHost::detach()                       // calls ServiceSessionHandler::detach()
///   → FrontHost::destroy()                      // Front Actor + Guest Engine destroyed
///   // Service continues running. Workspace unchanged.
/// ```
///
/// ## Tab background
///
/// ```text
/// Shell::on_tab_background(tab_id)
///   → FrontHost::pause()                        // Guest timers/events frozen
///   // NO Detach. Lease stays active via reduced Heartbeat.
/// ```
///
/// ## Tab foreground
///
/// ```text
/// Shell::on_tab_foreground(tab_id)
///   → FrontHost::resume()                       // Guest resumes
///   → if channel_alive: SessionRegistry::resume()
///   → if lease_expired: SessionRegistry::attach()
/// ```
///
/// ## Front crash
///
/// ```text
/// (no clean Detach)
/// LeaseManager::on_heartbeat_missed()
///   → SessionState::Active → Grace
/// LeaseManager::on_grace_expired()
///   → SessionState::Grace → Idle
///   → SessionRegistry::invalidate_session()
/// // User can reopen Tab → fresh Attach
/// ```
///
/// ## Service stop
///
/// ```text
/// ServiceHost::stop()
///   → ServiceHost::stop_accepting_attach()      // reject new Attach
///   → SessionRegistry::invalidate_all()          // close all sessions
///   → Workload::on_stop()                       // Service business cleanup
/// ```
///
/// ## Service restart
///
/// ```text
/// ServiceHost::restart()
///   → SessionRegistry::invalidate_all()          // old sessions closed
///   → ServiceHost::start()                       // same ActorId
///   // Fronts re-attach via Tab open/refresh
/// ```
pub struct TabLifecycleIntegration;
