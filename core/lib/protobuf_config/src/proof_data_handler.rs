use anyhow::Context as _;
use zksync_config::configs;
use zksync_protobuf::{repr::ProtoRepr, required};
use zksync_types::L1BatchNumber;

use crate::proto::prover as proto;

impl ProtoRepr for proto::ProofDataHandler {
    type Type = configs::ProofDataHandlerConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            http_port: required(&self.http_port)
                .and_then(|x| Ok((*x).try_into()?))
                .context("http_port")?,
            proof_generation_timeout_in_secs: required(&self.proof_generation_timeout_in_secs)
                .and_then(|x| Ok((*x).try_into()?))
                .context("proof_generation_timeout_in_secs")?,
            tee_config: configs::TeeConfig {
                tee_support: self
                    .tee_support
                    .unwrap_or_else(configs::TeeConfig::default_tee_support),
                first_tee_processed_batch: self
                    .first_tee_processed_batch
                    .map(|x| L1BatchNumber(x as u32))
                    .unwrap_or_else(configs::TeeConfig::default_first_tee_processed_batch),
            },
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            http_port: Some(this.http_port.into()),
            proof_generation_timeout_in_secs: Some(this.proof_generation_timeout_in_secs.into()),
            tee_support: Some(this.tee_config.tee_support),
            first_tee_processed_batch: Some(this.tee_config.first_tee_processed_batch.0 as u64),
        }
    }
}
