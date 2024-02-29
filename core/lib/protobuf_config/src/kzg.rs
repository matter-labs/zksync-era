use anyhow::Context as _;
use zksync_config::configs;
use zksync_protobuf::{required, ProtoRepr};

use crate::proto;

impl ProtoRepr for proto::Kzg {
    type Type = configs::KzgConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            trusted_setup_path: required(&self.trusted_setup_path)
                .context("trusted_setup_path")?
                .clone(),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            trusted_setup_path: Some(this.trusted_setup_path.clone()),
        }
    }
}
