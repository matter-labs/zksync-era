use smart_config::{ConfigSchema, DescribeConfig, DeserializeConfig};

use crate::{
    configs::{
        base_token_adjuster::BaseTokenAdjusterConfig,
        chain::{CircuitBreakerConfig, MempoolConfig, StateKeeperConfig, TimestampAsserterConfig},
        consensus::ConsensusConfig,
        da_dispatcher::DADispatcherConfig,
        house_keeper::HouseKeeperConfig,
        pruning::PruningConfig,
        snapshot_recovery::SnapshotRecoveryConfig,
        vm_runner::{BasicWitnessInputProducerConfig, ProtectiveReadsWriterConfig},
        wallets::Wallets,
        CommitmentGeneratorConfig, ConsistencyCheckerConfig, ExperimentalVmConfig,
        ExternalPriceApiClientConfig, GatewayMigratorConfig, GenesisConfigWrapper,
        ObservabilityConfig, PrometheusConfig, ProofDataHandlerConfig, Secrets,
        TeeProofDataHandlerConfig,
    },
    ApiConfig, ContractVerifierConfig, ContractsConfig, DAClientConfig, DBConfig, EthConfig,
    ExternalProofIntegrationApiConfig, ObjectStoreConfig, PostgresConfig, SnapshotsCreatorConfig,
};

#[derive(Debug, Clone, DescribeConfig, DeserializeConfig)]
pub struct GeneralConfig {
    #[config(nest, rename = "postgres", alias = "database")]
    pub postgres_config: PostgresConfig,
    #[config(nest, rename = "api")]
    pub api_config: Option<ApiConfig>,
    #[config(nest)]
    pub contract_verifier: ContractVerifierConfig,
    #[config(nest, rename = "circuit_breaker")]
    pub circuit_breaker_config: CircuitBreakerConfig,
    #[config(nest, rename = "mempool")]
    pub mempool_config: MempoolConfig,
    #[config(nest, rename = "state_keeper")]
    pub state_keeper_config: Option<StateKeeperConfig>,
    #[config(nest, rename = "house_keeper")]
    pub house_keeper_config: HouseKeeperConfig,

    #[config(nest, rename = "prometheus")]
    pub prometheus_config: PrometheusConfig,
    #[config(nest, rename = "data_handler")]
    pub proof_data_handler_config: Option<ProofDataHandlerConfig>,
    #[config(nest, rename = "tee_proof_data_handler")]
    pub tee_proof_data_handler_config: Option<TeeProofDataHandlerConfig>,
    #[config(nest, rename = "db", alias = "database")]
    pub db_config: DBConfig,
    #[config(nest)]
    pub eth: Option<EthConfig>,
    #[config(nest)]
    pub snapshot_creator: Option<SnapshotsCreatorConfig>,
    #[config(nest)]
    pub observability: ObservabilityConfig,
    #[config(nest, rename = "da_client", deprecated = "da")]
    pub da_client_config: Option<DAClientConfig>,
    #[config(nest, rename = "da_dispatcher")]
    pub da_dispatcher_config: Option<DADispatcherConfig>,
    #[config(nest, rename = "protective_reads_writer")]
    pub protective_reads_writer_config: Option<ProtectiveReadsWriterConfig>,
    #[config(nest, rename = "basic_witness_input_producer")]
    pub basic_witness_input_producer_config: Option<BasicWitnessInputProducerConfig>,
    #[config(nest)]
    pub commitment_generator: CommitmentGeneratorConfig,
    #[config(nest)]
    pub snapshot_recovery: Option<SnapshotRecoveryConfig>,
    #[config(nest)]
    pub pruning: PruningConfig,
    #[config(nest)]
    pub core_object_store: Option<ObjectStoreConfig>,
    #[config(nest)]
    pub base_token_adjuster: BaseTokenAdjusterConfig,
    #[config(nest, rename = "external_price_api_client")]
    pub external_price_api_client_config: ExternalPriceApiClientConfig,
    #[config(nest, rename = "external_proof_integration_api")]
    pub external_proof_integration_api_config: Option<ExternalProofIntegrationApiConfig>,
    #[config(nest, rename = "experimental_vm")]
    pub experimental_vm_config: ExperimentalVmConfig,
    #[config(nest, rename = "timestamp_asserter")]
    pub timestamp_asserter_config: TimestampAsserterConfig,
    #[config(nest, rename = "gateway_migrator")]
    pub gateway_migrator_config: GatewayMigratorConfig,
    #[config(nest, rename = "consistency_checker")]
    pub consistency_checker_config: ConsistencyCheckerConfig,
}

/// Returns the config schema for the main node.
pub fn full_config_schema() -> ConfigSchema {
    let mut schema = ConfigSchema::default();
    schema
        .coerce_serde_enums(true)
        .insert(&GeneralConfig::DESCRIPTION, "")
        .unwrap();

    // Add global aliases for the snapshots object store.
    schema
        .get_mut(
            &ObjectStoreConfig::DESCRIPTION,
            "snapshot_creator.object_store",
        )
        .unwrap()
        .push_alias("snapshots.object_store")
        .unwrap();
    schema
        .get_mut(
            &ObjectStoreConfig::DESCRIPTION,
            "snapshot_recovery.object_store",
        )
        .unwrap()
        .push_alias("snapshots.object_store")
        .unwrap();

    // Specialized configuration that were placed in separate files.
    schema.insert(&Secrets::DESCRIPTION, "").unwrap();
    schema
        .insert(&ConsensusConfig::DESCRIPTION, "consensus")
        .unwrap();

    schema
        .insert(&GenesisConfigWrapper::DESCRIPTION, "")
        .unwrap();
    schema.insert(&Wallets::DESCRIPTION, "wallets").unwrap();
    ContractsConfig::insert_into_schema(&mut schema);
    schema
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_schema_can_be_constructed_for_main_node() {
        full_config_schema();
    }
}
