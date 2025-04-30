use serde::de::DeserializeOwned;

mod api;
mod chain;
mod contract_verifier;
mod contracts;
mod database;
mod eth_sender;
mod eth_watch;
mod fri_proof_compressor;
mod fri_prover;
mod fri_prover_gateway;
mod fri_witness_generator;
mod house_keeper;
pub mod object_store;
mod observability;
mod proof_data_handler;
mod snapshots_creator;
mod tee_proof_data_handler;
mod utils;

mod base_token_adjuster;
mod da_dispatcher;
mod external_price_api_client;
mod external_proof_integration_api;
mod genesis;
mod prover_job_monitor;
#[cfg(test)]
mod test_utils;
mod vm_runner;
mod wallets;

pub mod da_client;
mod timestamp_asserter;
pub trait FromEnv: Sized {
    fn from_env() -> anyhow::Result<Self>;
}

/// Convenience function that loads the structure from the environment variable given the prefix.
/// Panics if the config cannot be loaded from the environment variables.
pub fn envy_load<T: DeserializeOwned>(name: &str, prefix: &str) -> anyhow::Result<T> {
    envy::prefixed(prefix)
        .from_env()
        .map_err(|e| anyhow::anyhow!("Failed to load {} from env: {}", name, e))
}
