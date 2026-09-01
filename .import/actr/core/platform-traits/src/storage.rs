//! Platform-agnostic key-value storage trait

use std::sync::Arc;

use async_trait::async_trait;

use crate::PlatformError;

/// Batch operation for KV store
pub enum KvOp {
    Set { key: String, value: Vec<u8> },
    Delete { key: String },
}

/// Opaque, versioned storage namespace for an actor.
///
/// This is the *only* value a provider receives to locate an actor's storage. It
/// is **not** a filesystem path: the provider maps it onto its own root (a
/// filesystem dir on native, a keychain service on Apple, an IndexedDB prefix on
/// web). Carrying the layout `version` lets future layout upgrades avoid
/// silently reusing a namespace with different semantics.
///
/// The digest is produced by `actr_pack::ValidatedActorIdentity`; the raw actor
/// identity never reaches the provider as a path component.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StorageNamespace {
    version: u8,
    digest: [u8; 32],
}

impl StorageNamespace {
    /// Namespace for the v1 actor storage layout.
    pub fn actr_v1(digest: [u8; 32]) -> Self {
        Self { version: 1, digest }
    }

    /// Construct a namespace with an explicit version.
    ///
    /// Hidden: exists so layout-version handling (rejecting unsupported
    /// versions) can be tested. Production namespaces are made via
    /// [`StorageNamespace::actr_v1`].
    #[doc(hidden)]
    pub fn new_unchecked(version: u8, digest: [u8; 32]) -> Self {
        Self { version, digest }
    }

    /// The 32-byte namespace digest (SHA-256 of the validated actor identity).
    pub fn digest(&self) -> &[u8; 32] {
        &self.digest
    }

    /// Storage layout version.
    pub fn version(&self) -> u8 {
        self.version
    }

    /// Versioned opaque key (`actr-storage:v1:<hex>`), suitable as a logical key
    /// for key-value providers. **Must not** be treated as a filesystem path —
    /// its `:` is meaningful to some filesystems (Windows ADS/drive).
    pub fn opaque_key(&self) -> String {
        format!(
            "actr-storage:v{}:{}",
            self.version,
            hex::encode(self.digest)
        )
    }
}

/// Platform-agnostic key-value store trait
///
/// Each Actor gets an isolated KV namespace. The backing implementation
/// may be SQLite (native), IndexedDB (web), or any other storage engine.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait KvStore: Send + Sync {
    /// Read a key's value, returns `None` if the key does not exist
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, PlatformError>;

    /// Write or update a key-value pair
    async fn set(&self, key: &str, value: &[u8]) -> Result<(), PlatformError>;

    /// Delete a key, returns whether a record was actually deleted
    async fn delete(&self, key: &str) -> Result<bool, PlatformError>;

    /// List all keys, optionally filtered by prefix
    async fn list_keys(&self, prefix: Option<&str>) -> Result<Vec<String>, PlatformError>;

    /// Batch operations (atomic transaction)
    async fn batch(&self, ops: Vec<KvOp>) -> Result<(), PlatformError>;
}

/// Extension: obtain a type-erased clone of a KvStore
pub trait KvStoreClone: KvStore {
    fn clone_box(&self) -> Arc<dyn KvStore>;
}
