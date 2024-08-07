use anyhow::Context;
use zksync_config::ExternalProofIntegrationApiConfig;
use zksync_protobuf::{required, ProtoRepr};

use crate::proto::external_proof_integration_api as proto;

impl ProtoRepr for proto::ProverApi {
    type Type = ExternalProofIntegrationApiConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            http_port: required(&self.http_port)
                .and_then(|p| Ok((*p).try_into()?))
                .context("http_port")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            http_port: Some(this.http_port.into()),
        }
    }
}
