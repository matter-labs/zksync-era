use anyhow::Context as _;
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
mod fri_prover_group;
mod fri_witness_generator;
mod fri_witness_vector_generator;
mod house_keeper;
pub mod object_store;
mod observability;
mod proof_data_handler;
mod snapshots_creator;
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

pub trait FromEnv: Sized {
    fn from_env() -> anyhow::Result<Self>;
}

pub trait FromEnvVariant: Sized {
    fn from_env_variant(variant_prefix: String) -> anyhow::Result<Self>;
}

/// Convenience function that loads the structure from the environment variable given the prefix.
/// Panics if the config cannot be loaded from the environment variables.
pub fn envy_load<T: DeserializeOwned>(name: &str, prefix: &str) -> anyhow::Result<T> {
    envy::prefixed(prefix)
        .from_env()
        .with_context(|| format!("Cannot load config <{name}>"))
}
