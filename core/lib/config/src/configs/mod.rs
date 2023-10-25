// Public re-exports
pub use self::{
    alerts::AlertsConfig, api::ApiConfig, chain::ChainConfig,
    circuit_synthesizer::CircuitSynthesizerConfig, contract_verifier::ContractVerifierConfig,
    contracts::ContractsConfig, database::DBConfig, eth_client::ETHClientConfig,
    eth_sender::ETHSenderConfig, eth_sender::GasAdjusterConfig, eth_watch::ETHWatchConfig,
    fetcher::FetcherConfig, fri_proof_compressor::FriProofCompressorConfig,
    fri_prover::FriProverConfig, fri_prover_gateway::FriProverGatewayConfig,
    fri_witness_generator::FriWitnessGeneratorConfig,
    fri_witness_vector_generator::FriWitnessVectorGeneratorConfig, object_store::ObjectStoreConfig,
    proof_data_handler::ProofDataHandlerConfig, prover::ProverConfig, prover::ProverConfigs,
    prover_group::ProverGroupConfig, utils::PrometheusConfig,
    witness_generator::WitnessGeneratorConfig,
};

use anyhow::Context as _;
use serde::de::DeserializeOwned;

pub mod alerts;
pub mod api;
pub mod chain;
pub mod circuit_synthesizer;
pub mod contract_verifier;
pub mod contracts;
pub mod database;
pub mod eth_client;
pub mod eth_sender;
pub mod eth_watch;
pub mod fetcher;
pub mod fri_proof_compressor;
pub mod fri_prover;
pub mod fri_prover_gateway;
pub mod fri_prover_group;
pub mod fri_witness_generator;
pub mod fri_witness_vector_generator;
pub mod house_keeper;
pub mod object_store;
pub mod proof_data_handler;
pub mod prover;
pub mod prover_group;
pub mod utils;
pub mod witness_generator;

#[cfg(test)]
pub(crate) mod test_utils;

const BYTES_IN_MEGABYTE: usize = 1_024 * 1_024;

pub trait FromEnv: Sized {
    fn from_env() -> anyhow::Result<Self>;
}

/// Convenience function that loads the structure from the environment variable given the prefix.
/// Panics if the config cannot be loaded from the environment variables.
pub(crate) fn envy_load<T: DeserializeOwned>(name: &str, prefix: &str) -> anyhow::Result<T> {
    envy_try_load(prefix).with_context(|| format!("Cannot load config <{name}>"))
}

/// Convenience function that loads the structure from the environment variable given the prefix.
pub(crate) fn envy_try_load<T: DeserializeOwned>(prefix: &str) -> Result<T, envy::Error> {
    envy::prefixed(prefix).from_env()
}
