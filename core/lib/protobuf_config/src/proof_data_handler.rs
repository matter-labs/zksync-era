use anyhow::Context as _;
use zksync_config::configs::{self, proof_data_handler::ApiMode};
use zksync_protobuf::{repr::ProtoRepr, required};
use zksync_types::L1BatchNumber;

use crate::proto::prover as proto;

impl ProtoRepr for proto::ProofDataHandler {
    type Type = configs::ProofDataHandlerConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        let api_mode = if let Some(api_mode) = &self.api_mode {
            match api_mode {
                0 => ApiMode::Legacy,
                1 => ApiMode::ProverCluster,
                _ => panic!("Unknown ProofDataHandler API mode: {api_mode}"),
            }
        } else {
            ApiMode::default()
        };

        if ApiMode::ProverCluster == api_mode {
            if self.gateway_api_url.is_none() {
                return Err(anyhow::anyhow!(
                    "gateway_api_url is required in ProverCluster mode of ProofDataHandler"
                ));
            }
        }

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
            gateway_api_url: self.gateway_api_url.as_ref().map(|x| x.to_string()),
            api_mode,
            proof_fetch_interval_in_secs: self
                .proof_fetch_interval_in_secs
                .map(|x| x as u16)
                .unwrap_or_else(
                    configs::ProofDataHandlerConfig::default_proof_fetch_interval_in_secs,
                ),
            proof_gen_data_submit_interval_in_secs: self
                .proof_gen_data_submit_interval_in_secs
                .map(|x| x as u16)
                .unwrap_or_else(
                    configs::ProofDataHandlerConfig::default_proof_gen_data_submit_interval_in_secs,
                ),
        })
    }

    fn build(this: &Self::Type) -> Self {
        let api_mode = match this.api_mode {
            ApiMode::Legacy => 0,
            ApiMode::ProverCluster => 1,
        };

        Self {
            http_port: Some(this.http_port.into()),
            proof_generation_timeout_in_secs: Some(this.proof_generation_timeout_in_secs.into()),
            tee_support: Some(this.tee_config.tee_support),
            first_tee_processed_batch: Some(this.tee_config.first_tee_processed_batch.0 as u64),
            tee_proof_generation_timeout_in_secs: Some(
                this.tee_config.tee_proof_generation_timeout_in_secs.into(),
            ),
            tee_batch_permanently_ignored_timeout_in_hours: Some(
                this.tee_config
                    .tee_batch_permanently_ignored_timeout_in_hours
                    .into(),
            ),
            gateway_api_url: this.gateway_api_url.as_ref().map(|x| x.to_string()),
            api_mode: Some(api_mode),
            proof_fetch_interval_in_secs: Some(this.proof_fetch_interval_in_secs.into()),
            proof_gen_data_submit_interval_in_secs: Some(
                this.proof_gen_data_submit_interval_in_secs.into(),
            ),
        }
    }
}
