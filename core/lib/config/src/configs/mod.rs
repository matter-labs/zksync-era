// Public re-exports
pub use self::{
    api::ApiConfig,
    contract_verifier::ContractVerifierConfig,
    contracts::{ContractsConfig, EcosystemContracts},
    database::{DBConfig, PostgresConfig},
    eth_sender::{EthConfig, GasAdjusterConfig},
    eth_watch::EthWatchConfig,
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
    snapshots_creator::SnapshotsCreatorConfig,
    utils::PrometheusConfig,
};

pub mod api;
pub mod chain;
pub mod consensus;
pub mod contract_verifier;
pub mod contracts;
pub mod database;
pub mod eth_sender;
pub mod eth_watch;
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
pub mod snapshots_creator;
pub mod utils;
pub mod wallets;

const BYTES_IN_MEGABYTE: usize = 1_024 * 1_024;
