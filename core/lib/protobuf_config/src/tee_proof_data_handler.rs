use anyhow::Context as _;
use zksync_config::configs;
use zksync_protobuf::{repr::ProtoRepr, required};
use zksync_types::L1BatchNumber;

use crate::proto::prover as proto;

impl ProtoRepr for proto::TeeProofDataHandler {
    type Type = configs::TeeProofDataHandlerConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            http_port: required(&self.http_port)
                .and_then(|x| Ok((*x).try_into()?))
                .context("http_port")?,
            first_processed_batch: self
                .first_processed_batch
                .map(|x| L1BatchNumber(x as u32))
                .unwrap_or_else(configs::TeeProofDataHandlerConfig::default_first_tee_processed_batch),
            proof_generation_timeout_in_secs: self
                .proof_generation_timeout_in_secs
                .map(|x| x as u16)
                .unwrap_or_else(
                    configs::TeeProofDataHandlerConfig::default_tee_proof_generation_timeout_in_secs,
                ),
            batch_permanently_ignored_timeout_in_hours: self
                .batch_permanently_ignored_timeout_in_hours
                .map(|x| x as u16)
                .unwrap_or_else(
                    configs::TeeProofDataHandlerConfig::default_tee_batch_permanently_ignored_timeout_in_hours,
                ), })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            http_port: Some(this.http_port.into()),
            first_processed_batch: Some(this.first_processed_batch.0 as u64),
            proof_generation_timeout_in_secs: Some(this.proof_generation_timeout_in_secs.into()),
            batch_permanently_ignored_timeout_in_hours: Some(
                this.batch_permanently_ignored_timeout_in_hours.into(),
            ),
        }
    }
}
