// Public re-exports
pub use self::{
    alerts::AlertsConfig,
    api::ApiConfig,
    contract_verifier::ContractVerifierConfig,
    contracts::ContractsConfig,
    database::{DBConfig, PostgresConfig},
    eth_client::ETHClientConfig,
    eth_sender::{ETHSenderConfig, GasAdjusterConfig},
    eth_watch::ETHWatchConfig,
    fri_proof_compressor::FriProofCompressorConfig,
    fri_prover::FriProverConfig,
    fri_prover_gateway::FriProverGatewayConfig,
    fri_witness_generator::FriWitnessGeneratorConfig,
    fri_witness_vector_generator::FriWitnessVectorGeneratorConfig,
    genesis::GenesisConfig,
    object_store::ObjectStoreConfig,
    observability::{ObservabilityConfig, OpentelemetryConfig},
    proof_data_handler::ProofDataHandlerConfig,
    snapshots_creator::SnapshotsCreatorConfig,
    utils::PrometheusConfig,
    witness_generator::WitnessGeneratorConfig,
};

pub mod alerts;
pub mod api;
pub mod chain;
pub mod contract_verifier;
pub mod contracts;
pub mod database;
pub mod eth_client;
pub mod eth_sender;
pub mod eth_watch;
pub mod fri_proof_compressor;
pub mod fri_prover;
pub mod fri_prover_gateway;
pub mod fri_prover_group;
pub mod fri_witness_generator;
pub mod fri_witness_vector_generator;
pub mod genesis;
pub mod house_keeper;
pub mod object_store;
pub mod observability;
pub mod proof_data_handler;
pub mod snapshots_creator;
pub mod utils;
pub mod witness_generator;

const BYTES_IN_MEGABYTE: usize = 1_024 * 1_024;
