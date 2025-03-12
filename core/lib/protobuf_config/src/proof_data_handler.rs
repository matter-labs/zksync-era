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
            retry_connection_interval_in_secs: required(&self.retry_connection_interval_in_secs)
                .and_then(|x| Ok((*x).try_into()?))
                .context("retry_connection_interval_in_secs")?,
            api_url: required(&self.api_url).context("api_url")?.clone(),
            batch_readiness_check_interval_in_secs: required(
                &self.batch_readiness_check_interval_in_secs,
            )
            .and_then(|x| Ok((*x).try_into()?))
            .context("api_poll_duration_in_secs")?,
            subscribe_for_zero_chain_id: self.subscribe_for_zero_chain_id.unwrap_or(true),
            tee_config: configs::TeeConfig {
                tee_support: self
                    .tee_support
                    .unwrap_or_else(configs::TeeConfig::default_tee_support),
                first_tee_processed_batch: self
                    .first_tee_processed_batch
                    .map(|x| L1BatchNumber(x as u32))
                    .unwrap_or_else(configs::TeeConfig::default_first_tee_processed_batch),
                tee_proof_generation_timeout_in_secs: self
                    .tee_proof_generation_timeout_in_secs
                    .map(|x| x as u16)
                    .unwrap_or_else(
                        configs::TeeConfig::default_tee_proof_generation_timeout_in_secs,
                    ),
                tee_batch_permanently_ignored_timeout_in_hours: self
                    .tee_batch_permanently_ignored_timeout_in_hours
                    .map(|x| x as u16)
                    .unwrap_or_else(
                        configs::TeeConfig::default_tee_batch_permanently_ignored_timeout_in_hours,
                    ),
            },
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            http_port: Some(this.http_port.into()),
            api_url: Some(this.api_url.clone()),
            batch_readiness_check_interval_in_secs: Some(
                this.batch_readiness_check_interval_in_secs.into(),
            ),
            retry_connection_interval_in_secs: Some(this.retry_connection_interval_in_secs.into()),
            proof_generation_timeout_in_secs: Some(this.proof_generation_timeout_in_secs.into()),
            tee_support: Some(this.tee_config.tee_support),
            subscribe_for_zero_chain_id: Some(this.subscribe_for_zero_chain_id),
            first_tee_processed_batch: Some(this.tee_config.first_tee_processed_batch.0 as u64),
            tee_proof_generation_timeout_in_secs: Some(
                this.tee_config.tee_proof_generation_timeout_in_secs.into(),
            ),
            tee_batch_permanently_ignored_timeout_in_hours: Some(
                this.tee_config
                    .tee_batch_permanently_ignored_timeout_in_hours
                    .into(),
            ),
        }
    }
}
