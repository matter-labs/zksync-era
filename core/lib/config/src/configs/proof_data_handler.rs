use std::time::Duration;

use serde::Deserialize;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ProofDataHandlerConfig {
    pub http_port: u16,
    pub proof_generation_timeout_in_secs: u16,
    #[serde(default)]
    pub gateway_api_url: Option<String>,
    #[serde(default = "ProofDataHandlerConfig::default_proof_fetch_interval_in_secs")]
    pub proof_fetch_interval_in_secs: u16,
    #[serde(default = "ProofDataHandlerConfig::default_proof_gen_data_submit_interval_in_secs")]
    pub proof_gen_data_submit_interval_in_secs: u16,
    #[serde(default = "ProofDataHandlerConfig::default_fetch_zero_chain_id_proofs")]
    pub fetch_zero_chain_id_proofs: bool,
}

impl ProofDataHandlerConfig {
    pub fn proof_generation_timeout(&self) -> Duration {
        Duration::from_secs(self.proof_generation_timeout_in_secs as u64)
    }

    pub fn proof_fetch_interval(&self) -> Duration {
        Duration::from_secs(self.proof_fetch_interval_in_secs as u64)
    }

    pub fn proof_gen_data_submit_interval(&self) -> Duration {
        Duration::from_secs(self.proof_gen_data_submit_interval_in_secs as u64)
    }

    pub fn default_proof_fetch_interval_in_secs() -> u16 {
        10
    }

    pub fn default_proof_gen_data_submit_interval_in_secs() -> u16 {
        10
    }

    pub fn default_fetch_zero_chain_id_proofs() -> bool {
        true
    }
}
