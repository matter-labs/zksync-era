// Public re-exports
pub use self::{
    api::ApiConfig,
    base_token_adjuster::BaseTokenAdjusterConfig,
    commitment_generator::CommitmentGeneratorConfig,
    consistency_checker::ConsistencyCheckerConfig,
    contract_verifier::ContractVerifierConfig,
    contracts::chain::ContractsConfig,
    da_client::{avail::AvailConfig, celestia::CelestiaConfig, eigen::EigenConfig, DAClientConfig},
    da_dispatcher::DADispatcherConfig,
    database::{DBConfig, PostgresConfig},
    eth_sender::{EthConfig, GasAdjusterConfig},
    eth_watch::EthWatchConfig,
    experimental::{ExperimentalDBConfig, ExperimentalVmConfig, ExperimentalVmPlaygroundConfig},
    external_price_api_client::ExternalPriceApiClientConfig,
    external_proof_integration_api::ExternalProofIntegrationApiConfig,
    gateway_migrator::GatewayMigratorConfig,
    general::{full_config_schema, GeneralConfig},
    genesis::{GenesisConfig, GenesisConfigWrapper},
    object_store::ObjectStoreConfig,
    observability::{ObservabilityConfig, OpentelemetryConfig},
    proof_data_handler::ProofDataHandlerConfig,
    pruning::PruningConfig,
    secrets::{L1Secrets, Secrets},
    snapshot_recovery::SnapshotRecoveryConfig,
    snapshots_creator::SnapshotsCreatorConfig,
    tee_proof_data_handler::TeeProofDataHandlerConfig,
    utils::PrometheusConfig,
    vm_runner::{BasicWitnessInputProducerConfig, ProtectiveReadsWriterConfig},
};

pub mod api;
pub mod base_token_adjuster;
pub mod chain;
mod commitment_generator;
pub mod consensus;
pub mod consistency_checker;
pub mod contract_verifier;
pub mod contracts;
pub mod da_client;
pub mod da_dispatcher;
pub mod database;
pub mod eth_sender;
pub mod eth_watch;
mod experimental;
pub mod external_price_api_client;
pub mod external_proof_integration_api;
mod gateway_migrator;
mod general;
pub mod genesis;
pub mod house_keeper;
pub mod networks;
pub mod object_store;
pub mod observability;
pub mod proof_data_handler;
pub mod pruning;
pub mod secrets;
pub mod snapshot_recovery;
pub mod snapshots_creator;
pub mod tee_proof_data_handler;
pub mod utils;
pub mod vm_runner;
pub mod wallets;

const BYTES_IN_MEGABYTE: usize = 1_024 * 1_024;
