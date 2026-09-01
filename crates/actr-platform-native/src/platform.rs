//! Native platform provider (filesystem + SQLite).

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use actr_hyper::SecureStorageRoot;
use actr_platform_traits::{
    CryptoProvider, KvStore, PlatformError, PlatformProvider, StorageNamespace,
};

use crate::crypto::NativeCryptoProvider;

const INSTANCE_UID_FILE: &str = ".hyper-instance-uid";

/// Native platform provider backed by a filesystem directory and SQLite.
///
/// All provider state lives under `data_dir`. The directory is created on
/// first use; callers never have to set it up.
pub struct NativePlatformProvider {
    data_dir: PathBuf,
    crypto: Arc<NativeCryptoProvider>,
    data_dir_ready: Mutex<bool>,
    /// Lazily-initialized secured root for per-actor storage (`data_dir/storage`).
    secure_root: Mutex<Option<Arc<SecureStorageRoot>>>,
}

impl NativePlatformProvider {
    /// Build a provider rooted at `data_dir`.
    ///
    /// The directory does not need to exist yet — it's created lazily on the
    /// first method call that needs it.
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            crypto: Arc::new(NativeCryptoProvider),
            data_dir_ready: Mutex::new(false),
            secure_root: Mutex::new(None),
        }
    }

    async fn ensure_data_dir(&self) -> Result<(), PlatformError> {
        let mut ready = self.data_dir_ready.lock().await;
        if *ready {
            return Ok(());
        }
        tokio::fs::create_dir_all(&self.data_dir)
            .await
            .map_err(|e| {
                PlatformError::Io(format!(
                    "failed to create data_dir `{}`: {e}",
                    self.data_dir.display()
                ))
            })?;
        *ready = true;
        Ok(())
    }

    /// Lazily build the secured per-actor storage root owned by this provider.
    async fn secure_root(&self) -> Result<Arc<SecureStorageRoot>, PlatformError> {
        let mut guard = self.secure_root.lock().await;
        if let Some(root) = guard.as_ref() {
            return Ok(root.clone());
        }
        let root = SecureStorageRoot::new(&self.data_dir.join("storage"))
            .await
            .map_err(|e| {
                PlatformError::Storage(format!("failed to init secure storage root: {e}"))
            })?;
        let root = Arc::new(root);
        *guard = Some(root.clone());
        Ok(root)
    }
}

#[async_trait]
impl PlatformProvider for NativePlatformProvider {
    async fn instance_uid(&self) -> Result<String, PlatformError> {
        self.ensure_data_dir().await?;
        let uid_file = self.data_dir.join(INSTANCE_UID_FILE);

        match tokio::fs::read_to_string(&uid_file).await {
            Ok(raw) => {
                let id = raw.trim();
                if !id.is_empty() {
                    return Ok(id.to_string());
                }
                warn!("instance_uid file is empty; regenerating");
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(PlatformError::Io(format!(
                    "failed to read instance_uid: {e}"
                )));
            }
        }

        let new_id = uuid::Uuid::new_v4().to_string();
        tokio::fs::write(&uid_file, &new_id)
            .await
            .map_err(|e| PlatformError::Io(format!("failed to write instance_uid: {e}")))?;
        info!(instance_uid = %new_id, "generated new instance_uid");
        Ok(new_id)
    }

    async fn secret_store(
        &self,
        namespace: &StorageNamespace,
    ) -> Result<Arc<dyn KvStore>, PlatformError> {
        // The opaque namespace is mapped onto this provider's own secured root;
        // it is never treated as a filesystem path.
        let root = self.secure_root().await?;
        let store = actr_hyper::ActorStore::open_under(&root, namespace)
            .await
            .map_err(|e| PlatformError::Storage(format!("failed to open ActorStore: {e}")))?;
        debug!(namespace = %namespace.opaque_key(), "native secret store opened");
        Ok(Arc::new(store))
    }

    fn crypto(&self) -> Arc<dyn CryptoProvider> {
        self.crypto.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn instance_uid_stable_across_calls() {
        let dir = TempDir::new().unwrap();
        let provider = NativePlatformProvider::new(dir.path());

        let id1 = provider.instance_uid().await.unwrap();
        let id2 = provider.instance_uid().await.unwrap();
        assert_eq!(id1, id2);
    }

    #[tokio::test]
    async fn instance_uid_creates_data_dir_lazily() {
        let parent = TempDir::new().unwrap();
        let nested = parent.path().join("a/b/c");
        assert!(!nested.exists());

        let provider = NativePlatformProvider::new(&nested);
        provider.instance_uid().await.unwrap();
        assert!(nested.exists());
    }

    #[tokio::test]
    async fn secret_store_roundtrip() {
        let dir = TempDir::new().unwrap();
        let provider = NativePlatformProvider::new(dir.path());
        let namespace = StorageNamespace::actr_v1([0xAB; 32]);

        let store = provider.secret_store(&namespace).await.unwrap();

        store.set("key", b"value").await.unwrap();
        let val = store.get("key").await.unwrap();
        assert_eq!(val, Some(b"value".to_vec()));

        // The opaque namespace maps under the provider's own root, as hex.
        let expected = dir
            .path()
            .join("storage")
            .join("actors")
            .join("v1")
            .join(hex::encode([0xAB; 32]))
            .join("store.sqlite3");
        assert!(
            expected.exists(),
            "expected store at {}",
            expected.display()
        );
    }

    #[tokio::test]
    async fn secret_store_rejects_unknown_namespace_version() {
        let dir = TempDir::new().unwrap();
        let provider = NativePlatformProvider::new(dir.path());
        // A namespace with an unsupported layout version must fail.
        let bad = StorageNamespace::new_unchecked(5, [0x02; 32]);
        assert!(provider.secret_store(&bad).await.is_err());
    }
}
