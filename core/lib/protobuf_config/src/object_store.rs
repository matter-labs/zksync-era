use anyhow::Context as _;
use zksync_config::configs::object_store::{ObjectStoreConfig, ObjectStoreMode};
use zksync_protobuf::{repr::ProtoRepr, required};

use crate::proto::object_store as proto;

impl ProtoRepr for proto::ObjectStore {
    type Type = ObjectStoreConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        let mode = required(&self.mode).context("mode")?;
        let mode = match mode {
            proto::object_store::Mode::Gcs(mode) => ObjectStoreMode::GCS {
                bucket_base_url: required(&mode.bucket_base_url)
                    .context("bucket_base_url")?
                    .clone(),
            },
            proto::object_store::Mode::GcsWithCredentialFile(mode) => {
                ObjectStoreMode::GCSWithCredentialFile {
                    bucket_base_url: required(&mode.bucket_base_url)
                        .context("bucket_base_url")?
                        .clone(),
                    gcs_credential_file_path: required(&mode.gcs_credential_file_path)
                        .context("gcs_credential_file_path")?
                        .clone(),
                }
            }
            proto::object_store::Mode::GcsAnonymousReadOnly(mode) => {
                ObjectStoreMode::GCSAnonymousReadOnly {
                    bucket_base_url: required(&mode.bucket_base_url)
                        .context("bucket_base_url")?
                        .clone(),
                }
            }
            proto::object_store::Mode::FileBacked(mode) => ObjectStoreMode::FileBacked {
                file_backed_base_path: required(&mode.file_backed_base_path)
                    .context("file_backed_base_path")?
                    .clone(),
            },
        };

        Ok(Self::Type {
            mode,
            max_retries: required(&self.max_retries)
                .and_then(|x| Ok((*x).try_into()?))
                .context("max_retries")?,
            local_mirror_path: self.local_mirror_path.clone(),
        })
    }

    fn build(this: &Self::Type) -> Self {
        let mode = match &this.mode {
            ObjectStoreMode::GCS { bucket_base_url } => {
                proto::object_store::Mode::Gcs(proto::object_store::Gcs {
                    bucket_base_url: Some(bucket_base_url.clone()),
                })
            }
            ObjectStoreMode::GCSWithCredentialFile {
                bucket_base_url,
                gcs_credential_file_path,
            } => proto::object_store::Mode::GcsWithCredentialFile(
                proto::object_store::GcsWithCredentialFile {
                    bucket_base_url: Some(bucket_base_url.clone()),
                    gcs_credential_file_path: Some(gcs_credential_file_path.clone()),
                },
            ),
            ObjectStoreMode::GCSAnonymousReadOnly { bucket_base_url } => {
                proto::object_store::Mode::GcsAnonymousReadOnly(
                    proto::object_store::GcsAnonymousReadOnly {
                        bucket_base_url: Some(bucket_base_url.clone()),
                    },
                )
            }
            ObjectStoreMode::FileBacked {
                file_backed_base_path,
            } => proto::object_store::Mode::FileBacked(proto::object_store::FileBacked {
                file_backed_base_path: Some(file_backed_base_path.clone()),
            }),
        };

        Self {
            mode: Some(mode),
            max_retries: Some(this.max_retries.into()),
            local_mirror_path: this.local_mirror_path.clone(),
        }
    }
}
