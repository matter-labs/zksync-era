use std::{fmt, fmt::Display, marker::PhantomData, str::FromStr};

use serde::de;

// Public re-exports
pub use self::{
    api::ApiConfig,
    base_token_adjuster::BaseTokenAdjusterConfig,
    commitment_generator::CommitmentGeneratorConfig,
    contract_verifier::ContractVerifierConfig,
    contracts::{ContractsConfig, EcosystemContracts},
    da_client::{avail::AvailConfig, DAClientConfig},
    da_dispatcher::DADispatcherConfig,
    database::{DBConfig, PostgresConfig},
    eth_sender::{EthConfig, GasAdjusterConfig},
    eth_watch::EthWatchConfig,
    experimental::{ExperimentalDBConfig, ExperimentalVmConfig, ExperimentalVmPlaygroundConfig},
    external_price_api_client::ExternalPriceApiClientConfig,
    external_proof_integration_api::ExternalProofIntegrationApiConfig,
    fri_proof_compressor::FriProofCompressorConfig,
    fri_prover::FriProverConfig,
    fri_prover_gateway::FriProverGatewayConfig,
    fri_witness_generator::FriWitnessGeneratorConfig,
    fri_witness_vector_generator::FriWitnessVectorGeneratorConfig,
    general::GeneralConfig,
    genesis::GenesisConfig,
    object_store::ObjectStoreConfig,
    observability::{ObservabilityConfig, OpentelemetryConfig},
    proof_data_handler::ProofDataHandlerConfig,
    prover_job_monitor::ProverJobMonitorConfig,
    pruning::PruningConfig,
    secrets::{DatabaseSecrets, L1Secrets, Secrets},
    snapshot_recovery::SnapshotRecoveryConfig,
    snapshots_creator::SnapshotsCreatorConfig,
    utils::PrometheusConfig,
    vm_runner::{BasicWitnessInputProducerConfig, ProtectiveReadsWriterConfig},
};

pub mod api;
pub mod base_token_adjuster;
pub mod chain;
mod commitment_generator;
pub mod consensus;
pub mod contract_verifier;
pub mod contracts;
pub mod da_client;
pub mod da_dispatcher;
pub mod database;
pub mod en_config;
pub mod eth_sender;
pub mod eth_watch;
mod experimental;
pub mod external_price_api_client;
pub mod external_proof_integration_api;
pub mod fri_proof_compressor;
pub mod fri_prover;
pub mod fri_prover_gateway;
pub mod fri_prover_group;
pub mod fri_witness_generator;
pub mod fri_witness_vector_generator;
mod general;
pub mod genesis;
pub mod house_keeper;
pub mod object_store;
pub mod observability;
pub mod proof_data_handler;
pub mod prover_job_monitor;
pub mod pruning;
pub mod secrets;
pub mod snapshot_recovery;
pub mod snapshots_creator;
pub mod utils;
pub mod vm_runner;
pub mod wallets;

const BYTES_IN_MEGABYTE: usize = 1_024 * 1_024;

// The code below is a workaround for https://github.com/softprops/envy/issues/26
// There is an issue with flattened structs, which breaks the deserialization of non-string types
pub fn deserialize_stringified_any<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: de::Deserializer<'de>,
    T: FromStr,
    T::Err: Display,
{
    deserializer.deserialize_any(StringifiedAnyVisitor(PhantomData))
}

pub struct StringifiedAnyVisitor<T>(PhantomData<T>);

impl<'de, T> de::Visitor<'de> for StringifiedAnyVisitor<T>
where
    T: FromStr,
    T::Err: Display,
{
    type Value = T;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string containing KV data")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Self::Value::from_str(v).map_err(E::custom)
    }
}
