use std::{path::Path, sync::Arc};

use anyhow::Context as _;
use tokio::sync::OnceCell;
use zksync_config::configs::object_store::{ObjectStoreConfig, ObjectStoreMode};

use crate::{
    file::FileBackedObjectStore,
    gcs::{GoogleCloudStore, GoogleCloudStoreAuthMode},
    mirror::MirroringObjectStore,
    raw::{ObjectStore, ObjectStoreError},
    retries::StoreWithRetries,
    s3::{S3Store, S3StoreAuthMode},
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
    /// # Errors
    ///
    /// Returns an error if store initialization fails (e.g., because of incorrect configuration).
    pub async fn create_store(&self) -> anyhow::Result<Arc<dyn ObjectStore>> {
        self.store
            .get_or_try_init(|| async {
                Self::create_from_config(&self.config)
                    .await
                    .with_context(|| {
                        format!(
                            "failed creating object store factory with configuration {:?}",
                            self.config
                        )
                    })
            })
            .await
            .cloned()
    }

    /// Creates an [`ObjectStore`] based on the provided `config`.
    ///
    /// # Errors
    ///
    /// Returns an error if store initialization fails (e.g., because of incorrect configuration).
    async fn create_from_config(
        config: &ObjectStoreConfig,
    ) -> Result<Arc<dyn ObjectStore>, ObjectStoreError> {
        tracing::trace!("Initializing object store with configuration {config:?}");
        match &config.mode {
            ObjectStoreMode::GCS { bucket_base_url } => {
                let store = StoreWithRetries::try_new(config.max_retries, || {
                    GoogleCloudStore::new(
                        GoogleCloudStoreAuthMode::Authenticated,
                        bucket_base_url.clone(),
                    )
                })
                .await?;
                Self::wrap_mirroring(store, config.local_mirror_path.as_deref()).await
            }
            ObjectStoreMode::GCSWithCredentialFile {
                bucket_base_url,
                gcs_credential_file_path,
            } => {
                let store = StoreWithRetries::try_new(config.max_retries, || {
                    GoogleCloudStore::new(
                        GoogleCloudStoreAuthMode::AuthenticatedWithCredentialFile(
                            gcs_credential_file_path.clone(),
                        ),
                        bucket_base_url.clone(),
                    )
                })
                .await?;
                Self::wrap_mirroring(store, config.local_mirror_path.as_deref()).await
            }
            ObjectStoreMode::GCSAnonymousReadOnly { bucket_base_url } => {
                let store = StoreWithRetries::try_new(config.max_retries, || {
                    GoogleCloudStore::new(
                        GoogleCloudStoreAuthMode::Anonymous,
                        bucket_base_url.clone(),
                    )
                })
                .await?;
                Self::wrap_mirroring(store, config.local_mirror_path.as_deref()).await
            }

            ObjectStoreMode::S3WithCredentialFile {
                bucket_base_url,
                s3_credential_file_path,
                endpoint,
                region,
            } => {
                let store = StoreWithRetries::try_new(config.max_retries, || {
                    S3Store::new(
                        S3StoreAuthMode::AuthenticatedWithCredentialFile(
                            s3_credential_file_path.clone(),
                        ),
                        bucket_base_url.clone(),
                        endpoint.clone(),
                        region.clone(),
                    )
                })
                .await?;
                Self::wrap_mirroring(store, config.local_mirror_path.as_deref()).await
            }
            ObjectStoreMode::S3AnonymousReadOnly {
                bucket_base_url,
                endpoint,
                region,
            } => {
                let store = StoreWithRetries::try_new(config.max_retries, || {
                    S3Store::new(
                        S3StoreAuthMode::Anonymous,
                        bucket_base_url.clone(),
                        endpoint.clone(),
                        region.clone(),
                    )
                })
                .await?;
                Self::wrap_mirroring(store, config.local_mirror_path.as_deref()).await
            }

            ObjectStoreMode::FileBacked {
                file_backed_base_path,
            } => {
                let store = StoreWithRetries::try_new(config.max_retries, || {
                    FileBackedObjectStore::new(file_backed_base_path.clone())
                })
                .await?;

                if let Some(mirror_path) = &config.local_mirror_path {
                    tracing::warn!(
                        "Mirroring doesn't make sense with file-backed object store; ignoring mirror path `{}`",
                        mirror_path.display()
                    );
                }
                Ok(Arc::new(store))
            }
        }
    }

    async fn wrap_mirroring(
        store: impl ObjectStore,
        mirror_path: Option<&Path>,
    ) -> Result<Arc<dyn ObjectStore>, ObjectStoreError> {
        Ok(if let Some(mirror_path) = mirror_path {
            Arc::new(MirroringObjectStore::new(store, mirror_path.to_owned()).await?)
        } else {
            Arc::new(store)
        })
    }
}
