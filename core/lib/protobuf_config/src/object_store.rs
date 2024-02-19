use anyhow::Context as _;
use zksync_config::configs;
use zksync_protobuf::required;

use crate::{proto, repr::ProtoRepr};

impl proto::ObjectStoreMode {
    fn new(x: &configs::object_store::ObjectStoreMode) -> Self {
        type From = configs::object_store::ObjectStoreMode;
        match x {
            From::GCS => Self::Gcs,
            From::GCSWithCredentialFile => Self::GcsWithCredentialFile,
            From::FileBacked => Self::FileBacked,
            From::GCSAnonymousReadOnly => Self::GcsAnonymousReadOnly,
        }
    }
    fn parse(&self) -> configs::object_store::ObjectStoreMode {
        type To = configs::object_store::ObjectStoreMode;
        match self {
            Self::Gcs => To::GCS,
            Self::GcsWithCredentialFile => To::GCSWithCredentialFile,
            Self::FileBacked => To::FileBacked,
            Self::GcsAnonymousReadOnly => To::GCSAnonymousReadOnly,
        }
    }
}

impl ProtoRepr for proto::ObjectStore {
    type Type = configs::ObjectStoreConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            bucket_base_url: required(&self.bucket_base_url)
                .context("bucket_base_url")?
                .clone(),
            mode: required(&self.mode)
                .and_then(|x| Ok(proto::ObjectStoreMode::try_from(*x)?))
                .context("mode")?
                .parse(),
            file_backed_base_path: required(&self.file_backed_base_path)
                .context("file_backed_base_path")?
                .clone(),
            gcs_credential_file_path: required(&self.gcs_credential_file_path)
                .context("gcs_credential_file_path")?
                .clone(),
            max_retries: required(&self.max_retries)
                .and_then(|x| Ok((*x).try_into()?))
                .context("max_retries")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            bucket_base_url: Some(this.bucket_base_url.clone()),
            mode: Some(proto::ObjectStoreMode::new(&this.mode).into()),
            file_backed_base_path: Some(this.file_backed_base_path.clone()),
            gcs_credential_file_path: Some(this.gcs_credential_file_path.clone()),
            max_retries: Some(this.max_retries.into()),
        }
    }
}
