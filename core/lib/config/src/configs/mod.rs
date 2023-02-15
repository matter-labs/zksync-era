// Public re-exports
pub use self::{
    api::ApiConfig, chain::ChainConfig, circuit_synthesizer::CircuitSynthesizerConfig,
    contract_verifier::ContractVerifierConfig, contracts::ContractsConfig, database::DBConfig,
    eth_client::ETHClientConfig, eth_sender::ETHSenderConfig, eth_sender::GasAdjusterConfig,
    eth_watch::ETHWatchConfig, fetcher::FetcherConfig, nfs::NfsConfig,
    object_store::ObjectStoreConfig, prover::ProverConfig, prover::ProverConfigs,
    prover_group::ProverGroupConfig, utils::Prometheus, witness_generator::WitnessGeneratorConfig,
};

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
pub mod nfs;
pub mod object_store;
pub mod prover;
pub mod prover_group;
pub mod utils;
pub mod witness_generator;

#[cfg(test)]
pub(crate) mod test_utils;

/// Convenience macro that loads the structure from the environment variable given the prefix.
///
/// # Panics
///
/// Panics if the config cannot be loaded from the environment variables.
#[macro_export]
macro_rules! envy_load {
    ($name:expr, $prefix:expr) => {
        envy::prefixed($prefix)
            .from_env()
            .unwrap_or_else(|err| panic!("Cannot load config <{}>: {}", $name, err))
    };
}
