use anyhow::Context;
use zksync_config::ProverApiConfig;
use zksync_protobuf::{required, ProtoRepr};

use crate::proto::prover_api as proto;

impl ProtoRepr for proto::ProverApi {
    type Type = ProverApiConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            http_port: required(&self.http_port)
                .and_then(|p| Ok((*p).try_into()?))
                .context("http_port")?,
            last_available_batch: required(&self.last_available_batch)
                .and_then(|p| Ok(*p))
                .context("last_available_batch")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            http_port: Some(this.http_port.into()),
            last_available_batch: Some(this.last_available_batch),
        }
    }
}
