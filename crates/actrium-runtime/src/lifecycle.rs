//! Durable Service lifecycle, identity-preserving operations, and creation saga.
//!
//! The persisted inventory deliberately keeps lifecycle, desired state,
//! reachability, health, attachment, and placement as independent fields. No
//! private key or provider credential is written to these files.

use std::{
    collections::HashMap,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use actrium_core::session::{AttachRequest, ProgramRelease, VerifiedChannel};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

use crate::{AttachOutcome, InMemorySessionService, SessionError, SessionStatus};

const SCHEMA_VERSION: u32 = 1;
const INSTANCE_FILE: &str = "instance.json";
const TOMBSTONE_FILE: &str = "tombstone.json";
const START_LOCK_FILE: &str = "start.lock";
const PRIVATE_DIRECTORIES: [&str; 5] = [
    "actor-store",
    "workload-state",
    "snapshots",
    "logs",
    "identity",
];

#[derive(Debug, Error)]
pub enum LifecycleError {
    #[error("Service not found: {0}")]
    NotFound(String),
    #[error("Service was permanently deleted: {0}")]
    Deleted(String),
    #[error("invalid lifecycle transition from {from:?} to {to:?}")]
    InvalidTransition {
        from: LifecycleState,
        to: LifecycleState,
    },
    #[error("idempotency key was already used for a different creation request")]
    IdempotencyConflict,
    #[error("another lifecycle manager is already starting Service {0}")]
    ConcurrentStart(String),
    #[error("another lifecycle manager is already creating idempotency key {0}")]
    ConcurrentCreation(String),
    #[error("Service {actor_id} cannot Attach while lifecycle is {state:?}")]
    NotAttachable {
        actor_id: String,
        state: LifecycleState,
    },
    #[error("invalid lifecycle request: {0}")]
    InvalidRequest(String),
    #[error("injected lifecycle failure: {0}")]
    Injected(String),
    #[error("session integration failed: {0}")]
    Session(#[from] SessionError),
    #[error("lifecycle storage I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("lifecycle storage JSON failed: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

impl LifecycleState {
    pub fn can_transition_to(self, next: Self) -> bool {
        use LifecycleState::*;
        matches!(
            (self, next),
            (Creating, Stopped)
                | (Stopped, Starting)
                | (Starting, Running)
                | (Starting, Stopped)
                | (Running, Suspended)
                | (Suspended, Running)
                | (Running, Stopping)
                | (Suspended, Stopping)
                | (Stopping, Stopped)
                | (Stopped, Migrating)
                | (Running, Migrating)
                | (Migrating, Stopped)
                | (Stopped, Archived)
                | (Archived, Stopped)
                | (Stopped, Trashed)
                | (Archived, Trashed)
                | (Trashed, Stopped)
                | (Trashed, Deleting)
                | (Deleting, Deleted)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesiredState {
    Running,
    Stopped,
    Archived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reachability {
    Online,
    Offline,
    Unreachable,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Health {
    Healthy,
    Degraded,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentState {
    Idle,
    Leased,
    Occupied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    Local,
    Remote,
}

impl Placement {
    fn location(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Remote => "remote",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialState {
    Active,
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceBinding {
    pub workspace_id: String,
    pub display_name: String,
    pub provider: String,
    /// Opaque host-owned locator, not a Guest-visible path.
    pub locator: String,
    pub access_mode: String,
    /// Reference to an OS-protected credential, never the credential itself.
    pub credential_ref: Option<String>,
    pub lease_policy: String,
    pub created_at_epoch_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateInstanceRequest {
    pub idempotency_key: String,
    pub package_sources: Vec<String>,
    pub program_release: ProgramRelease,
    pub service_package_digest: String,
    pub front_package_digest: Option<String>,
    pub privacy_contract_digest: Option<String>,
    pub realm_name: String,
    pub workspace: Option<WorkspaceBinding>,
    pub metadata: Option<Value>,
    pub requested_permissions: Vec<String>,
}

impl CreateInstanceRequest {
    pub fn new(
        idempotency_key: impl Into<String>,
        program_release: ProgramRelease,
        service_package_digest: impl Into<String>,
        realm_name: impl Into<String>,
    ) -> Self {
        Self {
            idempotency_key: idempotency_key.into(),
            package_sources: Vec::new(),
            program_release,
            service_package_digest: service_package_digest.into(),
            front_package_digest: None,
            privacy_contract_digest: None,
            realm_name: realm_name.into(),
            workspace: None,
            metadata: None,
            requested_permissions: Vec::new(),
        }
    }
}

/// Durable creation checkpoints. The host observer can attach real package,
/// approval, runtime, and Front handoff side effects to these checkpoints.
/// StorageCommitted is the point at which the instance joins local Inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

impl CreationStage {
    fn next(self) -> Option<Self> {
        use CreationStage::*;
        Some(match self {
            Prepared => PackageVerified,
            PackageVerified => UserApproved,
            UserApproved => IdentityAllocated,
            IdentityAllocated => StorageCommitted,
            StorageCommitted => ServiceStarted,
            ServiceStarted => FrontAttached,
            FrontAttached => Completed,
            Completed => return None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub workspace_metadata: Option<Value>,
    pub requested_permissions: Vec<String>,
    pub stage: CreationStage,
    pub result_actor_id: Option<String>,
    pub runtime_instance_id: Option<String>,
    pub created_at_epoch_secs: u64,
    pub updated_at_epoch_secs: u64,
}

impl CreationIntent {
    fn new(request: &CreateInstanceRequest, now: u64) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            idempotency_key: request.idempotency_key.clone(),
            package_sources: request.package_sources.clone(),
            program_release: request.program_release.clone(),
            service_package_digest: request.service_package_digest.clone(),
            front_package_digest: request.front_package_digest.clone(),
            privacy_contract_digest: request.privacy_contract_digest.clone(),
            realm_name: request.realm_name.clone(),
            workspace_selection: request.workspace.clone(),
            workspace_metadata: request.metadata.clone(),
            requested_permissions: request.requested_permissions.clone(),
            stage: CreationStage::Prepared,
            result_actor_id: None,
            runtime_instance_id: None,
            created_at_epoch_secs: now,
            updated_at_epoch_secs: now,
        }
    }

    fn matches(&self, request: &CreateInstanceRequest) -> bool {
        self.idempotency_key == request.idempotency_key
            && self.package_sources == request.package_sources
            && program_eq(&self.program_release, &request.program_release)
            && self.service_package_digest == request.service_package_digest
            && self.front_package_digest == request.front_package_digest
            && self.privacy_contract_digest == request.privacy_contract_digest
            && self.realm_name == request.realm_name
            && self.workspace_selection == request.workspace
            && self.workspace_metadata == request.metadata
            && self.requested_permissions == request.requested_permissions
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInstance {
    pub schema_version: u32,
    pub actor_id: String,
    pub actor_id_hash: String,
    pub program_release: ProgramRelease,
    pub service_package_digest: String,
    pub realm_name: String,
    pub runtime_instance_id: String,
    /// Instance-relative path; absolute host paths are not persisted.
    pub store_path: String,
    pub workspace: Option<WorkspaceBinding>,
    pub metadata: Option<Value>,
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

impl ServiceInstance {
    fn creating(intent: &CreationIntent) -> Result<Self, LifecycleError> {
        let actor_id = required_actor_id(intent)?.to_owned();
        let runtime_instance_id = required_runtime_id(intent)?.to_owned();
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            actor_id_hash: actor_id_hash(&actor_id),
            actor_id,
            program_release: intent.program_release.clone(),
            service_package_digest: intent.service_package_digest.clone(),
            realm_name: intent.realm_name.clone(),
            runtime_instance_id,
            store_path: "actor-store".into(),
            workspace: None,
            metadata: None,
            permission_grants: Vec::new(),
            credential_state: CredentialState::Active,
            lifecycle_state: LifecycleState::Creating,
            desired_state: DesiredState::Stopped,
            reachability: Reachability::Offline,
            health: Health::Unknown,
            attachment: AttachmentState::Idle,
            placement: Placement::Local,
            runtime_location: "local".into(),
            created_at_epoch_secs: intent.created_at_epoch_secs,
            last_opened_at_epoch_secs: None,
            updated_at_epoch_secs: now_epoch_secs(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TombstoneReason {
    CreationIncomplete,
    Purged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceTombstone {
    pub schema_version: u32,
    pub actor_id: String,
    pub actor_id_hash: String,
    pub realm_name: String,
    pub runtime_instance_id: String,
    pub lifecycle_state: LifecycleState,
    pub credential_state: CredentialState,
    pub reason: TombstoneReason,
    pub recoverable: bool,
    pub created_at_epoch_secs: u64,
    pub updated_at_epoch_secs: u64,
}

#[derive(Debug, Clone)]
pub struct UpgradeOutcome {
    pub previous: ServiceInstance,
    pub upgraded: ServiceInstance,
}

struct FileLockGuard {
    path: PathBuf,
}

impl Drop for FileLockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

struct Inner {
    root: PathBuf,
    sessions: Option<InMemorySessionService>,
    locks: Mutex<HashMap<String, Arc<AsyncMutex<()>>>>,
}

/// Local durable lifecycle manager. Clones share operation locks and the
/// optional session integration.
#[derive(Clone)]
pub struct LifecycleService {
    inner: Arc<Inner>,
}

impl LifecycleService {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, LifecycleError> {
        Self::open_inner(root.into(), None)
    }

    pub fn with_session_service(
        root: impl Into<PathBuf>,
        sessions: InMemorySessionService,
    ) -> Result<Self, LifecycleError> {
        Self::open_inner(root.into(), Some(sessions))
    }

    fn open_inner(
        root: PathBuf,
        sessions: Option<InMemorySessionService>,
    ) -> Result<Self, LifecycleError> {
        fs::create_dir_all(root.join("services"))?;
        fs::create_dir_all(root.join("creation-intents"))?;
        Ok(Self {
            inner: Arc::new(Inner {
                root,
                sessions,
                locks: Mutex::new(HashMap::new()),
            }),
        })
    }

    pub fn root(&self) -> &Path {
        &self.inner.root
    }

    pub fn session_service(&self) -> Option<&InMemorySessionService> {
        self.inner.sessions.as_ref()
    }

    pub fn instance_dir(&self, actor_id: &str) -> PathBuf {
        self.inner
            .root
            .join("services")
            .join(actor_id_hash(actor_id))
    }

    pub fn store_dir(&self, actor_id: &str) -> PathBuf {
        self.instance_dir(actor_id).join("actor-store")
    }

    fn intent_path(&self, idempotency_key: &str) -> PathBuf {
        self.inner
            .root
            .join("creation-intents")
            .join(format!("{}.json", actor_id_hash(idempotency_key)))
    }

    fn instance_path(&self, actor_id: &str) -> PathBuf {
        self.instance_dir(actor_id).join(INSTANCE_FILE)
    }

    fn tombstone_path(&self, actor_id: &str) -> PathBuf {
        self.instance_dir(actor_id).join(TOMBSTONE_FILE)
    }

    fn operation_lock(&self, key: String) -> Arc<AsyncMutex<()>> {
        let mut locks = self
            .inner
            .locks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        locks
            .entry(key)
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone()
    }

    pub fn creation_intent(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<CreationIntent>, LifecycleError> {
        read_optional_json(&self.intent_path(idempotency_key))
    }

    pub fn instance(&self, actor_id: &str) -> Result<ServiceInstance, LifecycleError> {
        self.ensure_not_purged(actor_id)?;
        if let Some(instance) = read_optional_json(&self.instance_path(actor_id))? {
            return self.validate_instance(actor_id, instance);
        }
        Err(LifecycleError::NotFound(actor_id.into()))
    }

    fn ensure_not_purged(&self, actor_id: &str) -> Result<(), LifecycleError> {
        if read_optional_json::<InstanceTombstone>(&self.tombstone_path(actor_id))?
            .is_some_and(|tombstone| tombstone.reason == TombstoneReason::Purged)
        {
            return Err(LifecycleError::Deleted(actor_id.into()));
        }
        Ok(())
    }

    pub fn tombstone(&self, actor_id: &str) -> Result<Option<InstanceTombstone>, LifecycleError> {
        read_optional_json(&self.tombstone_path(actor_id))
    }

    fn validate_instance(
        &self,
        actor_id: &str,
        instance: ServiceInstance,
    ) -> Result<ServiceInstance, LifecycleError> {
        if instance.schema_version != SCHEMA_VERSION
            || instance.actor_id != actor_id
            || instance.actor_id_hash != actor_id_hash(actor_id)
        {
            return Err(LifecycleError::InvalidRequest(
                "instance identity, schema, or hash does not match its directory".into(),
            ));
        }
        Ok(instance)
    }

    pub async fn create(
        &self,
        request: CreateInstanceRequest,
    ) -> Result<ServiceInstance, LifecycleError> {
        self.create_with_observer(request, |_| Ok(())).await
    }

    /// Runs or resumes a durable creation saga. The observer runs after the
    /// named stage has been atomically persisted. Returning an error simulates
    /// a host crash without erasing the intent or an allocated ActorId.
    pub async fn create_with_observer<F>(
        &self,
        request: CreateInstanceRequest,
        mut observer: F,
    ) -> Result<ServiceInstance, LifecycleError>
    where
        F: FnMut(CreationStage) -> Result<(), LifecycleError>,
    {
        validate_create_request(&request)?;
        let operation_lock = self.operation_lock(format!("create:{}", request.idempotency_key));
        let _guard = operation_lock.lock().await;
        let _creation_lock = self.acquire_creation_lock(&request.idempotency_key)?;
        let path = self.intent_path(&request.idempotency_key);
        let mut intent = if let Some(existing) = read_optional_json::<CreationIntent>(&path)? {
            if !existing.matches(&request) {
                return Err(LifecycleError::IdempotencyConflict);
            }
            existing
        } else {
            let intent = CreationIntent::new(&request, now_epoch_secs());
            atomic_write_json(&path, &intent)?;
            observer(CreationStage::Prepared)?;
            intent
        };

        while let Some(next) = intent.stage.next() {
            if let Some(actor_id) = intent.result_actor_id.as_deref() {
                self.ensure_not_purged(actor_id)?;
            }
            match next {
                CreationStage::Prepared => unreachable!("Prepared has no predecessor"),
                CreationStage::PackageVerified | CreationStage::UserApproved => {
                    self.commit_stage(&path, &mut intent, next)?;
                }
                CreationStage::IdentityAllocated => {
                    self.allocate_identity(&mut intent)?;
                    self.commit_stage(&path, &mut intent, next)?;
                    self.persist_allocated_identity(&intent)?;
                }
                CreationStage::StorageCommitted => {
                    self.commit_storage(&intent)?;
                    self.commit_stage(&path, &mut intent, next)?;
                }
                CreationStage::ServiceStarted | CreationStage::FrontAttached => {
                    self.commit_stage(&path, &mut intent, next)?;
                }
                CreationStage::Completed => {
                    self.commit_stage(&path, &mut intent, next)?;
                    self.clear_creation_tombstone(required_actor_id(&intent)?)?;
                }
            }
            observer(next)?;
        }

        let actor_id = required_actor_id(&intent)?;
        self.clear_creation_tombstone(actor_id)?;
        self.instance(actor_id)
    }

    fn clear_creation_tombstone(&self, actor_id: &str) -> Result<(), LifecycleError> {
        let path = self.tombstone_path(actor_id);
        if read_optional_json::<InstanceTombstone>(&path)?
            .is_some_and(|tombstone| tombstone.reason == TombstoneReason::CreationIncomplete)
        {
            remove_if_exists(&path)?;
        }
        Ok(())
    }

    fn commit_stage(
        &self,
        path: &Path,
        intent: &mut CreationIntent,
        stage: CreationStage,
    ) -> Result<(), LifecycleError> {
        intent.stage = stage;
        intent.updated_at_epoch_secs = now_epoch_secs();
        atomic_write_json(path, intent)
    }

    fn allocate_identity(&self, intent: &mut CreationIntent) -> Result<(), LifecycleError> {
        if intent.result_actor_id.is_some() || intent.runtime_instance_id.is_some() {
            if intent.result_actor_id.is_none() || intent.runtime_instance_id.is_none() {
                return Err(LifecycleError::InvalidRequest(
                    "creation intent has a partial identity allocation".into(),
                ));
            }
            return Ok(());
        }
        let serial = Uuid::new_v4().simple().to_string()[..12].to_owned();
        let actor_id = format!("{serial}.{}", intent.realm_name);
        let dir = self.instance_dir(&actor_id);
        if dir.join(INSTANCE_FILE).exists() || dir.join(TOMBSTONE_FILE).exists() {
            return Err(LifecycleError::InvalidRequest(
                "allocated ActorId collides with an existing identity or tombstone".into(),
            ));
        }
        intent.result_actor_id = Some(actor_id);
        intent.runtime_instance_id = Some(fresh_runtime_instance_id());
        Ok(())
    }

    fn persist_allocated_identity(&self, intent: &CreationIntent) -> Result<(), LifecycleError> {
        let actor_id = required_actor_id(intent)?;
        self.ensure_not_purged(actor_id)?;
        let dir = self.instance_dir(actor_id);
        fs::create_dir_all(&dir)?;
        let tombstone = InstanceTombstone {
            schema_version: SCHEMA_VERSION,
            actor_id: actor_id.into(),
            actor_id_hash: actor_id_hash(actor_id),
            realm_name: intent.realm_name.clone(),
            runtime_instance_id: required_runtime_id(intent)?.into(),
            lifecycle_state: LifecycleState::Creating,
            credential_state: CredentialState::Active,
            reason: TombstoneReason::CreationIncomplete,
            recoverable: true,
            created_at_epoch_secs: intent.created_at_epoch_secs,
            updated_at_epoch_secs: now_epoch_secs(),
        };
        if !dir.join(INSTANCE_FILE).exists() {
            atomic_write_json(&dir.join(TOMBSTONE_FILE), &tombstone)?;
            atomic_write_json(
                &dir.join(INSTANCE_FILE),
                &ServiceInstance::creating(intent)?,
            )?;
        }
        Ok(())
    }

    fn commit_storage(&self, intent: &CreationIntent) -> Result<(), LifecycleError> {
        let actor_id = required_actor_id(intent)?;
        self.ensure_not_purged(actor_id)?;
        let dir = self.instance_dir(actor_id);
        fs::create_dir_all(&dir)?;
        for private_dir in PRIVATE_DIRECTORIES {
            fs::create_dir_all(dir.join(private_dir))?;
        }
        let mut instance = ServiceInstance::creating(intent)?;
        instance.lifecycle_state = LifecycleState::Stopped;
        instance.workspace = intent.workspace_selection.clone();
        instance.metadata = intent.workspace_metadata.clone();
        instance.permission_grants = intent.requested_permissions.clone();
        atomic_write_json(&dir.join(INSTANCE_FILE), &instance)?;
        Ok(())
    }

    fn acquire_creation_lock(
        &self,
        idempotency_key: &str,
    ) -> Result<FileLockGuard, LifecycleError> {
        let dir = self.inner.root.join("creation-intents");
        fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.lock", actor_id_hash(idempotency_key)));
        let mut file = match File::options().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(LifecycleError::ConcurrentCreation(idempotency_key.into()));
            }
            Err(error) => return Err(error.into()),
        };
        let guard = FileLockGuard { path };
        writeln!(file, "pid={}", std::process::id())?;
        file.sync_all()?;
        Ok(guard)
    }

    fn acquire_start_lock(&self, actor_id: &str) -> Result<FileLockGuard, LifecycleError> {
        let dir = self.instance_dir(actor_id);
        fs::create_dir_all(&dir)?;
        let path = dir.join(START_LOCK_FILE);
        let mut file = match File::options().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(LifecycleError::ConcurrentStart(actor_id.into()));
            }
            Err(error) => return Err(error.into()),
        };
        let guard = FileLockGuard { path };
        writeln!(file, "{actor_id}\npid={}", std::process::id())?;
        file.sync_all()?;
        Ok(guard)
    }

    pub async fn start(&self, actor_id: &str) -> Result<ServiceInstance, LifecycleError> {
        self.start_with(actor_id, || Ok(())).await
    }

    pub async fn start_with<F>(
        &self,
        actor_id: &str,
        start: F,
    ) -> Result<ServiceInstance, LifecycleError>
    where
        F: FnOnce() -> Result<(), LifecycleError>,
    {
        let operation_lock = self.operation_lock(format!("actor:{actor_id}"));
        let _guard = operation_lock.lock().await;
        let _start_lock = self.acquire_start_lock(actor_id)?;
        let mut instance = self.instance(actor_id)?;
        ensure_transition(&instance, LifecycleState::Starting)?;
        instance.desired_state = DesiredState::Running;
        instance.reachability = Reachability::Unknown;
        instance.health = Health::Unknown;
        self.persist_transition(&mut instance, LifecycleState::Starting)?;

        if let Err(error) = start() {
            instance.reachability = Reachability::Offline;
            instance.health = Health::Failed;
            instance.attachment = AttachmentState::Idle;
            self.persist_transition(&mut instance, LifecycleState::Stopped)?;
            return Err(error);
        }

        instance.reachability = Reachability::Online;
        instance.health = Health::Healthy;
        instance.attachment = AttachmentState::Idle;
        instance.last_opened_at_epoch_secs = Some(now_epoch_secs());
        self.persist_transition(&mut instance, LifecycleState::Running)?;
        Ok(instance)
    }

    pub async fn stop(&self, actor_id: &str) -> Result<ServiceInstance, LifecycleError> {
        let operation_lock = self.operation_lock(format!("actor:{actor_id}"));
        let _guard = operation_lock.lock().await;
        let mut instance = self.instance(actor_id)?;
        ensure_transition(&instance, LifecycleState::Stopping)?;
        self.mark_offline(&mut instance, DesiredState::Stopped);
        self.persist_transition(&mut instance, LifecycleState::Stopping)?;
        let runtime_instance_id = self.invalidate_and_rotate().await?;
        instance.runtime_instance_id = runtime_instance_id;
        self.persist_transition(&mut instance, LifecycleState::Stopped)?;
        Ok(instance)
    }

    pub async fn restart(&self, actor_id: &str) -> Result<ServiceInstance, LifecycleError> {
        let operation_lock = self.operation_lock(format!("actor:{actor_id}"));
        let _guard = operation_lock.lock().await;
        let _start_lock = self.acquire_start_lock(actor_id)?;
        let mut instance = self.instance(actor_id)?;
        if !matches!(
            instance.lifecycle_state,
            LifecycleState::Running | LifecycleState::Suspended | LifecycleState::Stopped
        ) {
            return Err(invalid_transition(&instance, LifecycleState::Starting));
        }
        if instance.lifecycle_state != LifecycleState::Stopped {
            self.mark_offline(&mut instance, DesiredState::Stopped);
            self.persist_transition(&mut instance, LifecycleState::Stopping)?;
            let runtime_instance_id = self.invalidate_and_rotate().await?;
            instance.runtime_instance_id = runtime_instance_id;
            self.persist_transition(&mut instance, LifecycleState::Stopped)?;
        } else {
            instance.runtime_instance_id = self.invalidate_and_rotate().await?;
            self.persist(&mut instance)?;
        }

        instance.desired_state = DesiredState::Running;
        instance.reachability = Reachability::Unknown;
        instance.health = Health::Unknown;
        self.persist_transition(&mut instance, LifecycleState::Starting)?;
        instance.reachability = Reachability::Online;
        instance.health = Health::Healthy;
        instance.attachment = AttachmentState::Idle;
        instance.last_opened_at_epoch_secs = Some(now_epoch_secs());
        self.persist_transition(&mut instance, LifecycleState::Running)?;
        Ok(instance)
    }

    pub async fn suspend(&self, actor_id: &str) -> Result<ServiceInstance, LifecycleError> {
        let operation_lock = self.operation_lock(format!("actor:{actor_id}"));
        let _guard = operation_lock.lock().await;
        let mut instance = self.instance(actor_id)?;
        ensure_transition(&instance, LifecycleState::Suspended)?;
        instance.desired_state = DesiredState::Running;
        instance.reachability = Reachability::Offline;
        instance.health = Health::Unknown;
        self.persist_transition(&mut instance, LifecycleState::Suspended)?;
        Ok(instance)
    }

    pub async fn resume(&self, actor_id: &str) -> Result<ServiceInstance, LifecycleError> {
        let operation_lock = self.operation_lock(format!("actor:{actor_id}"));
        let _guard = operation_lock.lock().await;
        let mut instance = self.instance(actor_id)?;
        ensure_transition(&instance, LifecycleState::Running)?;
        instance.desired_state = DesiredState::Running;
        instance.reachability = Reachability::Online;
        instance.health = Health::Healthy;
        self.persist_transition(&mut instance, LifecycleState::Running)?;
        Ok(instance)
    }

    pub async fn archive(&self, actor_id: &str) -> Result<ServiceInstance, LifecycleError> {
        let operation_lock = self.operation_lock(format!("actor:{actor_id}"));
        let _guard = operation_lock.lock().await;
        let mut instance = self.instance(actor_id)?;
        ensure_transition(&instance, LifecycleState::Archived)?;
        self.mark_offline(&mut instance, DesiredState::Archived);
        self.persist_transition(&mut instance, LifecycleState::Archived)?;
        Ok(instance)
    }

    pub async fn trash(&self, actor_id: &str) -> Result<ServiceInstance, LifecycleError> {
        let operation_lock = self.operation_lock(format!("actor:{actor_id}"));
        let _guard = operation_lock.lock().await;
        let mut instance = self.instance(actor_id)?;
        ensure_transition(&instance, LifecycleState::Trashed)?;
        self.mark_offline(&mut instance, DesiredState::Stopped);
        instance.runtime_instance_id = self.invalidate_and_rotate().await?;
        self.persist_transition(&mut instance, LifecycleState::Trashed)?;
        Ok(instance)
    }

    /// Restores both Archived and Trashed instances without replacing identity.
    pub async fn restore(&self, actor_id: &str) -> Result<ServiceInstance, LifecycleError> {
        let operation_lock = self.operation_lock(format!("actor:{actor_id}"));
        let _guard = operation_lock.lock().await;
        let mut instance = self.instance(actor_id)?;
        ensure_transition(&instance, LifecycleState::Stopped)?;
        self.mark_offline(&mut instance, DesiredState::Stopped);
        self.persist_transition(&mut instance, LifecycleState::Stopped)?;
        Ok(instance)
    }

    pub async fn migrate(
        &self,
        actor_id: &str,
        placement: Placement,
    ) -> Result<ServiceInstance, LifecycleError> {
        let operation_lock = self.operation_lock(format!("actor:{actor_id}"));
        let _guard = operation_lock.lock().await;
        let mut instance = self.instance(actor_id)?;
        ensure_transition(&instance, LifecycleState::Migrating)?;
        self.mark_offline(&mut instance, DesiredState::Stopped);
        self.persist_transition(&mut instance, LifecycleState::Migrating)?;
        instance.runtime_instance_id = self.invalidate_and_rotate().await?;
        instance.placement = placement;
        instance.runtime_location = placement.location().into();
        self.persist_transition(&mut instance, LifecycleState::Stopped)?;
        Ok(instance)
    }

    pub async fn purge(&self, actor_id: &str) -> Result<InstanceTombstone, LifecycleError> {
        let operation_lock = self.operation_lock(format!("actor:{actor_id}"));
        let _guard = operation_lock.lock().await;
        let mut instance = self.instance(actor_id)?;
        ensure_transition(&instance, LifecycleState::Deleting)?;
        self.mark_offline(&mut instance, DesiredState::Stopped);
        instance.credential_state = CredentialState::Revoked;
        self.persist_transition(&mut instance, LifecycleState::Deleting)?;
        instance.runtime_instance_id = self.invalidate_and_rotate().await?;
        self.persist(&mut instance)?;
        self.finish_purge(instance).await
    }

    /// Upgrades are migrations: the replacement gets a new ActorId while the
    /// old instance is archived with its original identity and store intact.
    pub async fn upgrade(
        &self,
        actor_id: &str,
        mut request: CreateInstanceRequest,
    ) -> Result<UpgradeOutcome, LifecycleError> {
        let old = self.instance(actor_id)?;
        if request.realm_name != old.realm_name {
            return Err(LifecycleError::InvalidRequest(
                "a Program Release upgrade must retain the Realm".into(),
            ));
        }
        if program_eq(&request.program_release, &old.program_release) {
            return Err(LifecycleError::InvalidRequest(
                "an upgrade requires a different Program Release".into(),
            ));
        }
        if !matches!(
            old.lifecycle_state,
            LifecycleState::Running
                | LifecycleState::Suspended
                | LifecycleState::Stopped
                | LifecycleState::Archived
        ) {
            return Err(invalid_transition(&old, LifecycleState::Stopped));
        }
        if request.workspace.is_none() {
            request.workspace = old.workspace.clone();
        }
        if request.metadata.is_none() {
            request.metadata = old.metadata.clone();
        }
        if matches!(
            old.lifecycle_state,
            LifecycleState::Running | LifecycleState::Suspended
        ) {
            self.stop(actor_id).await?;
        }
        let upgraded = self.create(request).await?;
        if upgraded.actor_id == actor_id {
            return Err(LifecycleError::InvalidRequest(
                "an upgrade must allocate a new ActorId".into(),
            ));
        }
        let previous = if self.instance(actor_id)?.lifecycle_state == LifecycleState::Archived {
            self.instance(actor_id)?
        } else {
            self.archive(actor_id).await?
        };
        Ok(UpgradeOutcome { previous, upgraded })
    }

    pub async fn attach(
        &self,
        request: AttachRequest,
        channel: &VerifiedChannel,
    ) -> Result<AttachOutcome, LifecycleError> {
        let instance = self.instance(&request.service_actor_id)?;
        if instance.lifecycle_state != LifecycleState::Running {
            return Err(LifecycleError::NotAttachable {
                actor_id: instance.actor_id,
                state: instance.lifecycle_state,
            });
        }
        let sessions = self.inner.sessions.as_ref().ok_or_else(|| {
            LifecycleError::InvalidRequest("no InMemorySessionService is integrated".into())
        })?;
        Ok(sessions.attach(request, channel).await?)
    }

    /// Lifecycle eligibility only; session lease policy is enforced separately
    /// by InMemorySessionService. Suspended and Trashed therefore never Attach.
    pub fn attach_allowed(&self, actor_id: &str) -> Result<bool, LifecycleError> {
        Ok(self.instance(actor_id)?.lifecycle_state == LifecycleState::Running)
    }

    pub async fn session_status(&self, session_id: &str) -> Result<SessionStatus, LifecycleError> {
        let sessions = self.inner.sessions.as_ref().ok_or_else(|| {
            LifecycleError::InvalidRequest("no InMemorySessionService is integrated".into())
        })?;
        Ok(sessions.status(session_id).await?)
    }

    /// Converges durable transient states after a host/process crash. Running
    /// and Suspended cannot survive a process restart, so they become Stopped.
    /// Incomplete creation intents remain resumable by their idempotency key.
    pub async fn recover(&self) -> Result<(), LifecycleError> {
        // Recovery is exclusive host startup work, so these locks belong to
        // the previous process rather than a live creator.
        for entry in fs::read_dir(self.inner.root.join("creation-intents"))? {
            let entry = entry?;
            if entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "lock")
            {
                remove_if_exists(&entry.path())?;
            }
        }
        let services = self.inner.root.join("services");
        for entry in fs::read_dir(services)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            // recover() is a host-startup operation and must not race an active
            // lifecycle manager. A leftover lock belongs to the crashed host.
            remove_if_exists(&entry.path().join(START_LOCK_FILE))?;
            let path = entry.path().join(INSTANCE_FILE);
            let Some(mut instance) = read_optional_json::<ServiceInstance>(&path)? else {
                continue;
            };
            self.validate_instance(&instance.actor_id.clone(), instance.clone())?;
            match instance.lifecycle_state {
                LifecycleState::Deleting => {
                    let mut instance = instance;
                    instance.runtime_instance_id = self.invalidate_and_rotate().await?;
                    self.persist(&mut instance)?;
                    self.finish_purge(instance).await?;
                }
                LifecycleState::Starting => {
                    self.mark_offline(&mut instance, DesiredState::Stopped);
                    instance.health = Health::Failed;
                    instance.runtime_instance_id = self.invalidate_and_rotate().await?;
                    self.persist_transition(&mut instance, LifecycleState::Stopped)?;
                }
                LifecycleState::Stopping | LifecycleState::Migrating => {
                    self.mark_offline(&mut instance, DesiredState::Stopped);
                    instance.runtime_instance_id = self.invalidate_and_rotate().await?;
                    self.persist_transition(&mut instance, LifecycleState::Stopped)?;
                }
                LifecycleState::Running | LifecycleState::Suspended => {
                    self.mark_offline(&mut instance, DesiredState::Stopped);
                    instance.runtime_instance_id = self.invalidate_and_rotate().await?;
                    instance.lifecycle_state = LifecycleState::Stopped;
                    self.persist(&mut instance)?;
                }
                LifecycleState::Creating
                | LifecycleState::Stopped
                | LifecycleState::Archived
                | LifecycleState::Trashed
                | LifecycleState::Deleted => {}
            }
        }
        Ok(())
    }

    fn mark_offline(&self, instance: &mut ServiceInstance, desired: DesiredState) {
        instance.desired_state = desired;
        instance.reachability = Reachability::Offline;
        instance.health = Health::Unknown;
        instance.attachment = AttachmentState::Idle;
    }

    async fn invalidate_and_rotate(&self) -> Result<String, LifecycleError> {
        let runtime_instance_id = fresh_runtime_instance_id();
        if let Some(sessions) = &self.inner.sessions {
            sessions.invalidate_all().await;
            sessions.restart(runtime_instance_id.clone()).await?;
        }
        Ok(runtime_instance_id)
    }

    async fn finish_purge(
        &self,
        instance: ServiceInstance,
    ) -> Result<InstanceTombstone, LifecycleError> {
        if instance.lifecycle_state != LifecycleState::Deleting {
            return Err(invalid_transition(&instance, LifecycleState::Deleted));
        }
        let dir = self.instance_dir(&instance.actor_id);
        for private_dir in PRIVATE_DIRECTORIES {
            remove_tree_if_exists(&dir.join(private_dir))?;
        }
        remove_if_exists(&dir.join("credential.json"))?;
        remove_if_exists(&dir.join("private.key"))?;
        remove_if_exists(&dir.join(START_LOCK_FILE))?;
        let tombstone = InstanceTombstone {
            schema_version: SCHEMA_VERSION,
            actor_id: instance.actor_id.clone(),
            actor_id_hash: instance.actor_id_hash.clone(),
            realm_name: instance.realm_name.clone(),
            runtime_instance_id: instance.runtime_instance_id.clone(),
            lifecycle_state: LifecycleState::Deleted,
            credential_state: CredentialState::Revoked,
            reason: TombstoneReason::Purged,
            recoverable: false,
            created_at_epoch_secs: instance.created_at_epoch_secs,
            updated_at_epoch_secs: now_epoch_secs(),
        };
        atomic_write_json(&dir.join(TOMBSTONE_FILE), &tombstone)?;
        remove_if_exists(&dir.join(INSTANCE_FILE))?;
        Ok(tombstone)
    }

    fn persist_transition(
        &self,
        instance: &mut ServiceInstance,
        next: LifecycleState,
    ) -> Result<(), LifecycleError> {
        ensure_transition(instance, next)?;
        instance.lifecycle_state = next;
        self.persist(instance)
    }

    fn persist(&self, instance: &mut ServiceInstance) -> Result<(), LifecycleError> {
        instance.updated_at_epoch_secs = now_epoch_secs();
        atomic_write_json(&self.instance_path(&instance.actor_id), instance)
    }
}

fn ensure_transition(
    instance: &ServiceInstance,
    next: LifecycleState,
) -> Result<(), LifecycleError> {
    if instance.lifecycle_state.can_transition_to(next) {
        Ok(())
    } else {
        Err(invalid_transition(instance, next))
    }
}

fn invalid_transition(instance: &ServiceInstance, next: LifecycleState) -> LifecycleError {
    LifecycleError::InvalidTransition {
        from: instance.lifecycle_state,
        to: next,
    }
}

fn required_actor_id(intent: &CreationIntent) -> Result<&str, LifecycleError> {
    intent.result_actor_id.as_deref().ok_or_else(|| {
        LifecycleError::InvalidRequest("creation intent has no allocated ActorId".into())
    })
}

fn required_runtime_id(intent: &CreationIntent) -> Result<&str, LifecycleError> {
    intent.runtime_instance_id.as_deref().ok_or_else(|| {
        LifecycleError::InvalidRequest("creation intent has no runtime instance ID".into())
    })
}

fn validate_create_request(request: &CreateInstanceRequest) -> Result<(), LifecycleError> {
    let invalid = |message: &str| LifecycleError::InvalidRequest(message.into());
    if request.idempotency_key.trim().is_empty() {
        return Err(invalid("idempotency_key must not be empty"));
    }
    if request.service_package_digest.trim().is_empty() {
        return Err(invalid("service_package_digest must not be empty"));
    }
    if request.realm_name.trim().is_empty()
        || !request.realm_name.contains('@')
        || request.realm_name.starts_with('@')
        || request.realm_name.ends_with('@')
        || request.realm_name.chars().any(char::is_whitespace)
    {
        return Err(invalid("realm_name must be a full Realm name"));
    }
    let program = &request.program_release;
    if program.publisher.trim().is_empty()
        || program.name.trim().is_empty()
        || program.version.trim().is_empty()
    {
        return Err(invalid("Program Release fields must not be empty"));
    }
    if request.workspace.as_ref().is_some_and(|workspace| {
        workspace.workspace_id.trim().is_empty()
            || workspace.provider.trim().is_empty()
            || workspace.locator.trim().is_empty()
    }) {
        return Err(invalid(
            "WorkspaceBinding identity/provider/locator required",
        ));
    }
    Ok(())
}

fn program_eq(left: &ProgramRelease, right: &ProgramRelease) -> bool {
    left.publisher == right.publisher && left.name == right.name && left.version == right.version
}

fn fresh_runtime_instance_id() -> String {
    format!("rtm_{}", Uuid::new_v4().simple())
}

/// Stable, path-safe SHA-256 representation of the complete ActorId.
pub fn actor_id_hash(actor_id: &str) -> String {
    Sha256::digest(actor_id.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn read_optional_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, LifecycleError> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), LifecycleError> {
    let parent = path.parent().ok_or_else(|| {
        LifecycleError::InvalidRequest(format!("{} has no parent directory", path.display()))
    })?;
    fs::create_dir_all(parent)?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            LifecycleError::InvalidRequest(format!(
                "{} is not a valid JSON filename",
                path.display()
            ))
        })?;
    let temporary = parent.join(format!(".{file_name}.{}.tmp", Uuid::new_v4().simple()));
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    let write_result = (|| -> Result<(), std::io::Error> {
        let mut file = File::options()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    write_result?;
    Ok(())
}

fn remove_if_exists(path: &Path) -> Result<(), std::io::Error> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn remove_tree_if_exists(path: &Path) -> Result<(), std::io::Error> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
