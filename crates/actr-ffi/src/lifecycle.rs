//! FFI bridge for the Actrium service lifecycle manager.
//!
//! Exposes durable service instance lifecycle operations to Swift / Kotlin
//! hosts: create, stop, restart, purge, recover, and state inspection. This
//! layer keeps the Rust `actrium_runtime::LifecycleService` behind an
//! `Arc`-wrapped object so UniFFI can manage it as a foreign reference.

use std::sync::Arc;

use crate::error::{ActrError, ActrResult, run_on_tokio_runtime};

/// Program release identifier used when creating a service instance.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ProgramRelease {
    pub publisher: String,
    pub name: String,
    pub version: String,
}

impl From<ProgramRelease> for actrium_core::session::ProgramRelease {
    fn from(value: ProgramRelease) -> Self {
        Self {
            publisher: value.publisher,
            name: value.name,
            version: value.version,
        }
    }
}

impl From<actrium_core::session::ProgramRelease> for ProgramRelease {
    fn from(value: actrium_core::session::ProgramRelease) -> Self {
        Self {
            publisher: value.publisher,
            name: value.name,
            version: value.version,
        }
    }
}

/// Workspace binding for a service instance.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WorkspaceBinding {
    pub workspace_id: String,
    pub display_name: String,
    pub provider: String,
    pub locator: String,
    pub access_mode: String,
    pub credential_ref: Option<String>,
    pub lease_policy: String,
    pub created_at_epoch_secs: u64,
}

impl From<WorkspaceBinding> for actrium_runtime::WorkspaceBinding {
    fn from(value: WorkspaceBinding) -> Self {
        Self {
            workspace_id: value.workspace_id,
            display_name: value.display_name,
            provider: value.provider,
            locator: value.locator,
            access_mode: value.access_mode,
            credential_ref: value.credential_ref,
            lease_policy: value.lease_policy,
            created_at_epoch_secs: value.created_at_epoch_secs,
        }
    }
}

impl From<actrium_runtime::WorkspaceBinding> for WorkspaceBinding {
    fn from(value: actrium_runtime::WorkspaceBinding) -> Self {
        Self {
            workspace_id: value.workspace_id,
            display_name: value.display_name,
            provider: value.provider,
            locator: value.locator,
            access_mode: value.access_mode,
            credential_ref: value.credential_ref,
            lease_policy: value.lease_policy,
            created_at_epoch_secs: value.created_at_epoch_secs,
        }
    }
}

/// Request to create a new service instance.
#[derive(Debug, Clone, uniffi::Record)]
pub struct CreateInstanceRequest {
    pub idempotency_key: String,
    pub package_sources: Vec<String>,
    pub program_release: ProgramRelease,
    pub service_package_digest: String,
    pub front_package_digest: Option<String>,
    pub privacy_contract_digest: Option<String>,
    pub realm_name: String,
    pub workspace: Option<WorkspaceBinding>,
    /// Opaque JSON metadata; `None` is stored as absent.
    pub metadata_json: Option<String>,
    pub requested_permissions: Vec<String>,
}

impl CreateInstanceRequest {
    fn into_runtime(self) -> ActrResult<actrium_runtime::CreateInstanceRequest> {
        let metadata =
            match self.metadata_json {
                Some(json) => Some(serde_json::from_str(&json).map_err(|error| {
                    ActrError::InvalidArgument {
                        msg: format!("invalid metadata_json: {error}"),
                    }
                })?),
                None => None,
            };
        Ok(actrium_runtime::CreateInstanceRequest {
            idempotency_key: self.idempotency_key,
            package_sources: self.package_sources,
            program_release: self.program_release.into(),
            service_package_digest: self.service_package_digest,
            front_package_digest: self.front_package_digest,
            privacy_contract_digest: self.privacy_contract_digest,
            realm_name: self.realm_name,
            workspace: self.workspace.map(Into::into),
            metadata,
            requested_permissions: self.requested_permissions,
        })
    }
}

/// Lifecycle state of a service instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum LifecycleState {
    Creating,
    Stopped,
    Starting,
    Running,
    Suspended,
    Stopping,
    Migrating,
    Archived,
    Trashed,
    Deleting,
    Deleted,
}

impl From<actrium_runtime::LifecycleState> for LifecycleState {
    fn from(value: actrium_runtime::LifecycleState) -> Self {
        match value {
            actrium_runtime::LifecycleState::Creating => Self::Creating,
            actrium_runtime::LifecycleState::Stopped => Self::Stopped,
            actrium_runtime::LifecycleState::Starting => Self::Starting,
            actrium_runtime::LifecycleState::Running => Self::Running,
            actrium_runtime::LifecycleState::Suspended => Self::Suspended,
            actrium_runtime::LifecycleState::Stopping => Self::Stopping,
            actrium_runtime::LifecycleState::Migrating => Self::Migrating,
            actrium_runtime::LifecycleState::Archived => Self::Archived,
            actrium_runtime::LifecycleState::Trashed => Self::Trashed,
            actrium_runtime::LifecycleState::Deleting => Self::Deleting,
            actrium_runtime::LifecycleState::Deleted => Self::Deleted,
        }
    }
}

/// Desired long-running state configured by the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum DesiredState {
    Running,
    Stopped,
    Archived,
}

impl From<actrium_runtime::DesiredState> for DesiredState {
    fn from(value: actrium_runtime::DesiredState) -> Self {
        match value {
            actrium_runtime::DesiredState::Running => Self::Running,
            actrium_runtime::DesiredState::Stopped => Self::Stopped,
            actrium_runtime::DesiredState::Archived => Self::Archived,
        }
    }
}

/// Network reachability of a running instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Reachability {
    Online,
    Offline,
    Unreachable,
    Unknown,
}

impl From<actrium_runtime::Reachability> for Reachability {
    fn from(value: actrium_runtime::Reachability) -> Self {
        match value {
            actrium_runtime::Reachability::Online => Self::Online,
            actrium_runtime::Reachability::Offline => Self::Offline,
            actrium_runtime::Reachability::Unreachable => Self::Unreachable,
            actrium_runtime::Reachability::Unknown => Self::Unknown,
        }
    }
}

/// Health signal derived from host/runtime observations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Health {
    Healthy,
    Degraded,
    Failed,
    Unknown,
}

impl From<actrium_runtime::Health> for Health {
    fn from(value: actrium_runtime::Health) -> Self {
        match value {
            actrium_runtime::Health::Healthy => Self::Healthy,
            actrium_runtime::Health::Degraded => Self::Degraded,
            actrium_runtime::Health::Failed => Self::Failed,
            actrium_runtime::Health::Unknown => Self::Unknown,
        }
    }
}

/// Session attachment occupancy of the instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AttachmentState {
    Idle,
    Leased,
    Occupied,
}

impl From<actrium_runtime::AttachmentState> for AttachmentState {
    fn from(value: actrium_runtime::AttachmentState) -> Self {
        match value {
            actrium_runtime::AttachmentState::Idle => Self::Idle,
            actrium_runtime::AttachmentState::Leased => Self::Leased,
            actrium_runtime::AttachmentState::Occupied => Self::Occupied,
        }
    }
}

/// Placement target for an instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Placement {
    Local,
    Remote,
}

impl From<Placement> for actrium_runtime::Placement {
    fn from(value: Placement) -> Self {
        match value {
            Placement::Local => Self::Local,
            Placement::Remote => Self::Remote,
        }
    }
}

impl From<actrium_runtime::Placement> for Placement {
    fn from(value: actrium_runtime::Placement) -> Self {
        match value {
            actrium_runtime::Placement::Local => Self::Local,
            actrium_runtime::Placement::Remote => Self::Remote,
        }
    }
}

/// Credential state of the instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CredentialState {
    Active,
    Revoked,
}

impl From<actrium_runtime::CredentialState> for CredentialState {
    fn from(value: actrium_runtime::CredentialState) -> Self {
        match value {
            actrium_runtime::CredentialState::Active => Self::Active,
            actrium_runtime::CredentialState::Revoked => Self::Revoked,
        }
    }
}

/// Durable service instance record persisted by the lifecycle manager.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ServiceInstance {
    pub schema_version: u32,
    pub actor_id: String,
    pub actor_id_hash: String,
    pub program_release: ProgramRelease,
    pub service_package_digest: String,
    pub realm_name: String,
    pub runtime_instance_id: String,
    pub store_path: String,
    pub workspace: Option<WorkspaceBinding>,
    /// Opaque JSON metadata; absent when the instance has no metadata.
    pub metadata_json: Option<String>,
    pub permission_grants: Vec<String>,
    pub credential_state: CredentialState,
    pub lifecycle_state: LifecycleState,
    pub desired_state: DesiredState,
    pub reachability: Reachability,
    pub health: Health,
    pub attachment: AttachmentState,
    pub placement: Placement,
    pub runtime_location: String,
    pub created_at_epoch_secs: u64,
    pub last_opened_at_epoch_secs: Option<u64>,
    pub updated_at_epoch_secs: u64,
}

impl From<actrium_runtime::ServiceInstance> for ServiceInstance {
    fn from(value: actrium_runtime::ServiceInstance) -> Self {
        let metadata_json = value.metadata.map(|value| value.to_string());
        Self {
            schema_version: value.schema_version,
            actor_id: value.actor_id,
            actor_id_hash: value.actor_id_hash,
            program_release: value.program_release.into(),
            service_package_digest: value.service_package_digest,
            realm_name: value.realm_name,
            runtime_instance_id: value.runtime_instance_id,
            store_path: value.store_path,
            workspace: value.workspace.map(Into::into),
            metadata_json,
            permission_grants: value.permission_grants,
            credential_state: value.credential_state.into(),
            lifecycle_state: value.lifecycle_state.into(),
            desired_state: value.desired_state.into(),
            reachability: value.reachability.into(),
            health: value.health.into(),
            attachment: value.attachment.into(),
            placement: value.placement.into(),
            runtime_location: value.runtime_location,
            created_at_epoch_secs: value.created_at_epoch_secs,
            last_opened_at_epoch_secs: value.last_opened_at_epoch_secs,
            updated_at_epoch_secs: value.updated_at_epoch_secs,
        }
    }
}

/// Reason an instance tombstone was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TombstoneReason {
    CreationIncomplete,
    Purged,
}

impl From<actrium_runtime::TombstoneReason> for TombstoneReason {
    fn from(value: actrium_runtime::TombstoneReason) -> Self {
        match value {
            actrium_runtime::TombstoneReason::CreationIncomplete => Self::CreationIncomplete,
            actrium_runtime::TombstoneReason::Purged => Self::Purged,
        }
    }
}

/// Tombstone record for a permanently removed instance.
#[derive(Debug, Clone, uniffi::Record)]
pub struct InstanceTombstone {
    pub schema_version: u32,
    pub actor_id: String,
    pub actor_id_hash: String,
    pub reason: TombstoneReason,
    pub created_at_epoch_secs: u64,
}

impl From<actrium_runtime::InstanceTombstone> for InstanceTombstone {
    fn from(value: actrium_runtime::InstanceTombstone) -> Self {
        Self {
            schema_version: value.schema_version,
            actor_id: value.actor_id,
            actor_id_hash: value.actor_id_hash,
            reason: value.reason.into(),
            created_at_epoch_secs: value.created_at_epoch_secs,
        }
    }
}

/// Creation saga checkpoint exposed for host observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CreationStage {
    Prepared,
    PackageVerified,
    UserApproved,
    IdentityAllocated,
    StorageCommitted,
    ServiceStarted,
    FrontAttached,
    Completed,
}

impl From<actrium_runtime::CreationStage> for CreationStage {
    fn from(value: actrium_runtime::CreationStage) -> Self {
        match value {
            actrium_runtime::CreationStage::Prepared => Self::Prepared,
            actrium_runtime::CreationStage::PackageVerified => Self::PackageVerified,
            actrium_runtime::CreationStage::UserApproved => Self::UserApproved,
            actrium_runtime::CreationStage::IdentityAllocated => Self::IdentityAllocated,
            actrium_runtime::CreationStage::StorageCommitted => Self::StorageCommitted,
            actrium_runtime::CreationStage::ServiceStarted => Self::ServiceStarted,
            actrium_runtime::CreationStage::FrontAttached => Self::FrontAttached,
            actrium_runtime::CreationStage::Completed => Self::Completed,
        }
    }
}

/// Durable creation intent persisted during a create saga.
#[derive(Debug, Clone, uniffi::Record)]
pub struct CreationIntent {
    pub schema_version: u32,
    pub idempotency_key: String,
    pub package_sources: Vec<String>,
    pub program_release: ProgramRelease,
    pub service_package_digest: String,
    pub front_package_digest: Option<String>,
    pub privacy_contract_digest: Option<String>,
    pub realm_name: String,
    pub workspace_selection: Option<WorkspaceBinding>,
    /// Opaque workspace metadata JSON.
    pub workspace_metadata_json: Option<String>,
    pub requested_permissions: Vec<String>,
    pub stage: CreationStage,
    pub result_actor_id: Option<String>,
    pub runtime_instance_id: Option<String>,
    pub created_at_epoch_secs: u64,
    pub updated_at_epoch_secs: u64,
}

impl From<actrium_runtime::CreationIntent> for CreationIntent {
    fn from(value: actrium_runtime::CreationIntent) -> Self {
        Self {
            schema_version: value.schema_version,
            idempotency_key: value.idempotency_key,
            package_sources: value.package_sources,
            program_release: value.program_release.into(),
            service_package_digest: value.service_package_digest,
            front_package_digest: value.front_package_digest,
            privacy_contract_digest: value.privacy_contract_digest,
            realm_name: value.realm_name,
            workspace_selection: value.workspace_selection.map(Into::into),
            workspace_metadata_json: value.workspace_metadata.map(|value| value.to_string()),
            requested_permissions: value.requested_permissions,
            stage: value.stage.into(),
            result_actor_id: value.result_actor_id,
            runtime_instance_id: value.runtime_instance_id,
            created_at_epoch_secs: value.created_at_epoch_secs,
            updated_at_epoch_secs: value.updated_at_epoch_secs,
        }
    }
}

impl From<actrium_runtime::LifecycleError> for ActrError {
    fn from(error: actrium_runtime::LifecycleError) -> Self {
        match error {
            actrium_runtime::LifecycleError::NotFound(actor_id) => {
                ActrError::NotFound { msg: actor_id }
            }
            actrium_runtime::LifecycleError::Deleted(actor_id) => ActrError::InvalidArgument {
                msg: format!("instance purged: {actor_id}"),
            },
            actrium_runtime::LifecycleError::InvalidTransition { from, to } => {
                ActrError::InvalidArgument {
                    msg: format!("invalid transition from {from:?} to {to:?}"),
                }
            }
            actrium_runtime::LifecycleError::IdempotencyConflict => ActrError::InvalidArgument {
                msg: "idempotency key conflict".into(),
            },
            actrium_runtime::LifecycleError::ConcurrentStart(actor_id) => ActrError::Unavailable {
                msg: format!("concurrent start: {actor_id}"),
            },
            actrium_runtime::LifecycleError::ConcurrentCreation(key) => ActrError::Unavailable {
                msg: format!("concurrent creation: {key}"),
            },
            actrium_runtime::LifecycleError::NotAttachable { actor_id, state } => {
                ActrError::InvalidArgument {
                    msg: format!("{actor_id} not attachable in state {state:?}"),
                }
            }
            actrium_runtime::LifecycleError::InvalidRequest(message) => {
                ActrError::InvalidArgument { msg: message }
            }
            actrium_runtime::LifecycleError::Injected(message) => {
                ActrError::Internal { msg: message }
            }
            actrium_runtime::LifecycleError::Session(error) => ActrError::Internal {
                msg: format!("session error: {error}"),
            },
            actrium_runtime::LifecycleError::Io(error) => ActrError::Internal {
                msg: format!("lifecycle I/O error: {error}"),
            },
            actrium_runtime::LifecycleError::Json(error) => ActrError::DecodeFailure {
                msg: format!("lifecycle JSON error: {error}"),
            },
        }
    }
}

/// FFI wrapper for the durable service lifecycle manager.
#[derive(uniffi::Object)]
pub struct ActrLifecycleService {
    inner: actrium_runtime::LifecycleService,
}

#[uniffi::export(async_runtime = "tokio")]
impl ActrLifecycleService {
    /// Open a lifecycle manager rooted at `root`. Directories are created as
    /// needed. Each distinct app container should use its own root.
    #[uniffi::constructor]
    pub fn open(root: String) -> ActrResult<Arc<Self>> {
        let inner = actrium_runtime::LifecycleService::open(root).map_err(ActrError::from)?;
        Ok(Arc::new(Self { inner }))
    }

    /// Absolute root path of this lifecycle manager.
    pub fn root(&self) -> String {
        self.inner.root().to_string_lossy().into_owned()
    }

    /// Create or resume a service instance. Idempotent under the same
    /// `idempotency_key`.
    pub async fn create(
        self: Arc<Self>,
        request: CreateInstanceRequest,
    ) -> ActrResult<ServiceInstance> {
        let request = request.into_runtime()?;
        let inner = self.inner.clone();
        run_on_tokio_runtime(
            "lifecycle create",
            async move { inner.create(request).await },
        )
        .await
        .map(Into::into)
    }

    /// Transition a running/suspended instance to stopped.
    pub async fn stop(self: Arc<Self>, actor_id: String) -> ActrResult<ServiceInstance> {
        let inner = self.inner.clone();
        run_on_tokio_runtime("lifecycle stop", async move { inner.stop(&actor_id).await })
            .await
            .map(Into::into)
    }

    /// Stop then start an instance, rotating its runtime instance identity.
    pub async fn restart(self: Arc<Self>, actor_id: String) -> ActrResult<ServiceInstance> {
        let inner = self.inner.clone();
        run_on_tokio_runtime("lifecycle restart", async move {
            inner.restart(&actor_id).await
        })
        .await
        .map(Into::into)
    }

    /// Pause a running instance without releasing identity.
    pub async fn suspend(self: Arc<Self>, actor_id: String) -> ActrResult<ServiceInstance> {
        let inner = self.inner.clone();
        run_on_tokio_runtime("lifecycle suspend", async move {
            inner.suspend(&actor_id).await
        })
        .await
        .map(Into::into)
    }

    /// Resume a suspended instance.
    pub async fn resume(self: Arc<Self>, actor_id: String) -> ActrResult<ServiceInstance> {
        let inner = self.inner.clone();
        run_on_tokio_runtime(
            "lifecycle resume",
            async move { inner.resume(&actor_id).await },
        )
        .await
        .map(Into::into)
    }

    /// Move a stopped instance to archived storage.
    pub async fn archive(self: Arc<Self>, actor_id: String) -> ActrResult<ServiceInstance> {
        let inner = self.inner.clone();
        run_on_tokio_runtime("lifecycle archive", async move {
            inner.archive(&actor_id).await
        })
        .await
        .map(Into::into)
    }

    /// Move a stopped/archived instance to trash.
    pub async fn trash(self: Arc<Self>, actor_id: String) -> ActrResult<ServiceInstance> {
        let inner = self.inner.clone();
        run_on_tokio_runtime(
            "lifecycle trash",
            async move { inner.trash(&actor_id).await },
        )
        .await
        .map(Into::into)
    }

    /// Restore an archived or trashed instance to stopped.
    pub async fn restore(self: Arc<Self>, actor_id: String) -> ActrResult<ServiceInstance> {
        let inner = self.inner.clone();
        run_on_tokio_runtime("lifecycle restore", async move {
            inner.restore(&actor_id).await
        })
        .await
        .map(Into::into)
    }

    /// Migrate a stopped/running instance to a new placement.
    pub async fn migrate(
        self: Arc<Self>,
        actor_id: String,
        placement: Placement,
    ) -> ActrResult<ServiceInstance> {
        let inner = self.inner.clone();
        let placement = placement.into();
        run_on_tokio_runtime("lifecycle migrate", async move {
            inner.migrate(&actor_id, placement).await
        })
        .await
        .map(Into::into)
    }

    /// Permanently delete an instance and write a tombstone.
    pub async fn purge(self: Arc<Self>, actor_id: String) -> ActrResult<InstanceTombstone> {
        let inner = self.inner.clone();
        run_on_tokio_runtime(
            "lifecycle purge",
            async move { inner.purge(&actor_id).await },
        )
        .await
        .map(Into::into)
    }

    /// Recover transient state after a process crash.
    pub async fn recover(self: Arc<Self>) -> ActrResult<()> {
        let inner = self.inner.clone();
        run_on_tokio_runtime("lifecycle recover", async move { inner.recover().await }).await
    }

    /// Read the current persisted instance record, if any.
    pub fn instance(&self, actor_id: String) -> ActrResult<ServiceInstance> {
        self.inner
            .instance(&actor_id)
            .map_err(ActrError::from)
            .map(Into::into)
    }

    /// Read the tombstone record, if the instance was purged.
    pub fn tombstone(&self, actor_id: String) -> ActrResult<Option<InstanceTombstone>> {
        self.inner
            .tombstone(&actor_id)
            .map_err(ActrError::from)
            .map(|option| option.map(Into::into))
    }

    /// Read a creation intent by its idempotency key.
    pub fn creation_intent(&self, idempotency_key: String) -> ActrResult<Option<CreationIntent>> {
        self.inner
            .creation_intent(&idempotency_key)
            .map_err(ActrError::from)
            .map(|option| option.map(Into::into))
    }

    /// Whether the instance is in a lifecycle state that allows Attach.
    pub fn attach_allowed(&self, actor_id: String) -> ActrResult<bool> {
        self.inner
            .attach_allowed(&actor_id)
            .map_err(ActrError::from)
    }
}
