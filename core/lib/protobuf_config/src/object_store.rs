use anyhow::Context as _;
use zksync_config::configs::object_store::{ObjectStoreConfig, ObjectStoreMode};
use zksync_protobuf::required;

use crate::{proto, repr::ProtoRepr};

impl ProtoRepr for proto::ObjectStore {
    type Type = ObjectStoreConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        let mode = match self.mode() {
            proto::ObjectStoreMode::Gcs => ObjectStoreMode::GCS {
                bucket_base_url: required(&self.bucket_base_url)
                    .context("bucket_base_url")?
                    .clone(),
            },
            proto::ObjectStoreMode::GcsWithCredentialFile => {
                ObjectStoreMode::GCSWithCredentialFile {
                    bucket_base_url: required(&self.bucket_base_url)
                        .context("bucket_base_url")?
                        .clone(),
                    gcs_credential_file_path: required(&self.gcs_credential_file_path)
                        .context("gcs_credential_file_path")?
                        .clone(),
                }
            }
            proto::ObjectStoreMode::GcsAnonymousReadOnly => ObjectStoreMode::GCSAnonymousReadOnly {
                bucket_base_url: required(&self.bucket_base_url)
                    .context("bucket_base_url")?
                    .clone(),
            },
            proto::ObjectStoreMode::FileBacked => ObjectStoreMode::FileBacked {
                file_backed_base_path: required(&self.file_backed_base_path)
                    .context("file_backed_base_path")?
                    .clone(),
            },
        };

        Ok(Self::Type {
            mode,
            max_retries: required(&self.max_retries)
                .and_then(|x| Ok((*x).try_into()?))
                .context("max_retries")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        match &this.mode {
            ObjectStoreMode::GCS { bucket_base_url } => Self {
                mode: Some(proto::ObjectStoreMode::Gcs.into()),
                bucket_base_url: Some(bucket_base_url.clone()),
                max_retries: Some(this.max_retries.into()),
                ..Self::default()
            },
            ObjectStoreMode::GCSWithCredentialFile {
                bucket_base_url,
                gcs_credential_file_path,
            } => Self {
                mode: Some(proto::ObjectStoreMode::GcsWithCredentialFile.into()),
                bucket_base_url: Some(bucket_base_url.clone()),
                gcs_credential_file_path: Some(gcs_credential_file_path.clone()),
                max_retries: Some(this.max_retries.into()),
                ..Self::default()
            },
            ObjectStoreMode::GCSAnonymousReadOnly { bucket_base_url } => Self {
                mode: Some(proto::ObjectStoreMode::GcsAnonymousReadOnly.into()),
                bucket_base_url: Some(bucket_base_url.clone()),
                max_retries: Some(this.max_retries.into()),
                ..Self::default()
            },
            ObjectStoreMode::FileBacked {
                file_backed_base_path,
            } => Self {
                mode: Some(proto::ObjectStoreMode::FileBacked.into()),
                file_backed_base_path: Some(file_backed_base_path.clone()),
                max_retries: Some(this.max_retries.into()),
                ..Self::default()
            },
        }
    }
}
