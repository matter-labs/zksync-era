use anyhow::Context as _;
use zksync_config::configs;
use zksync_protobuf::{repr::ProtoRepr, required};

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
            gateway_api_url: self.gateway_api_url.as_ref().map(|x| x.to_string()),
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
            fetch_zero_chain_id_proofs: self.fetch_zero_chain_id_proofs.unwrap_or_else(
                configs::ProofDataHandlerConfig::default_fetch_zero_chain_id_proofs,
            ),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            http_port: Some(this.http_port.into()),
            proof_generation_timeout_in_secs: Some(this.proof_generation_timeout_in_secs.into()),
            gateway_api_url: this.gateway_api_url.as_ref().map(|x| x.to_string()),
            proof_fetch_interval_in_secs: Some(this.proof_fetch_interval_in_secs.into()),
            proof_gen_data_submit_interval_in_secs: Some(
                this.proof_gen_data_submit_interval_in_secs.into(),
            ),
            fetch_zero_chain_id_proofs: Some(this.fetch_zero_chain_id_proofs),
        }
    }
}
