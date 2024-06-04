use std::sync::Arc;

use tokio::sync::OnceCell;
use zksync_config::configs::object_store::{ObjectStoreConfig, ObjectStoreMode};

use crate::{
    file::FileBackedObjectStore,
    gcs::{GoogleCloudStore, GoogleCloudStoreAuthMode},
    raw::{ObjectStore, ObjectStoreError},
};

/// Factory of [`ObjectStore`]s that caches the store instance once it's created. Used mainly for legacy reasons.
///
/// Please do not use this factory in dependency injection; rely on `Arc<dyn ObjectStore>` instead. This allows to
/// inject mock store implementations, decorate an object store with middleware etc.
#[derive(Debug)]
pub struct ObjectStoreFactory {
    config: ObjectStoreConfig,
    store: OnceCell<Arc<dyn ObjectStore>>,
}

impl ObjectStoreFactory {
    /// Creates an object store factory based on the provided `config`.
    pub fn new(config: ObjectStoreConfig) -> Self {
        Self {
            config,
            store: OnceCell::new(),
        }
    }

    /// Creates an [`ObjectStore`] or returns a cached store if one was created previously.
    ///
    /// # Panics
    ///
    /// Panics if store initialization fails (e.g., because of incorrect configuration).
    pub async fn create_store(&self) -> Arc<dyn ObjectStore> {
        // FIXME: propagate errors
        self.store
            .get_or_init(|| async {
                Self::create_from_config(&self.config)
                    .await
                    .unwrap_or_else(|err| {
                        panic!(
                            "failed creating object store factory with configuration {:?}: {err}",
                            self.config
                        )
                    })
            })
            .await
            .clone()
    }

    async fn create_from_config(
        config: &ObjectStoreConfig,
    ) -> Result<Arc<dyn ObjectStore>, ObjectStoreError> {
        match &config.mode {
            ObjectStoreMode::GCS { bucket_base_url } => {
                tracing::trace!(
                    "Initialized GoogleCloudStorage Object store without credential file"
                );
                let store = GoogleCloudStore::new(
                    GoogleCloudStoreAuthMode::Authenticated,
                    bucket_base_url.clone(),
                    config.max_retries,
                )
                .await?;
                Ok(Arc::new(store))
            }
            ObjectStoreMode::GCSWithCredentialFile {
                bucket_base_url,
                gcs_credential_file_path,
            } => {
                tracing::trace!("Initialized GoogleCloudStorage Object store with credential file");
                let store = GoogleCloudStore::new(
                    GoogleCloudStoreAuthMode::AuthenticatedWithCredentialFile(
                        gcs_credential_file_path.clone(),
                    ),
                    bucket_base_url.clone(),
                    config.max_retries,
                )
                .await?;
                Ok(Arc::new(store))
            }
            ObjectStoreMode::FileBacked {
                file_backed_base_path,
            } => {
                tracing::trace!("Initialized FileBacked Object store");
                let store = FileBackedObjectStore::new(file_backed_base_path.clone()).await?;
                Ok(Arc::new(store))
            }
            ObjectStoreMode::GCSAnonymousReadOnly { bucket_base_url } => {
                tracing::trace!("Initialized GoogleCloudStoragePublicReadOnly store");
                let store = GoogleCloudStore::new(
                    GoogleCloudStoreAuthMode::Anonymous,
                    bucket_base_url.clone(),
                    config.max_retries,
                )
                .await?;
                Ok(Arc::new(store))
            }
        }
    }
}
