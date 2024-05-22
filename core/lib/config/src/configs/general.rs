use crate::{
    configs::{
        chain::{CircuitBreakerConfig, MempoolConfig, OperationsManagerConfig, StateKeeperConfig},
        fri_prover_group::FriProverGroupConfig,
        house_keeper::HouseKeeperConfig,
        FriProofCompressorConfig, FriProverConfig, FriProverGatewayConfig,
        FriWitnessGeneratorConfig, FriWitnessVectorGeneratorConfig, ObservabilityConfig,
        PrometheusConfig, ProofDataHandlerConfig,
    },
    ApiConfig, ContractVerifierConfig, DBConfig, EthConfig, PostgresConfig, SnapshotsCreatorConfig,
};

#[derive(Debug)]
pub struct GeneralConfig {
    pub postgres_config: Option<PostgresConfig>,
    pub api_config: Option<ApiConfig>,
    pub contract_verifier: Option<ContractVerifierConfig>,
    pub circuit_breaker_config: Option<CircuitBreakerConfig>,
    pub mempool_config: Option<MempoolConfig>,
    pub operations_manager_config: Option<OperationsManagerConfig>,
    pub state_keeper_config: Option<StateKeeperConfig>,
    pub house_keeper_config: Option<HouseKeeperConfig>,
    pub proof_compressor_config: Option<FriProofCompressorConfig>,
    pub prover_config: Option<FriProverConfig>,
    pub prover_gateway: Option<FriProverGatewayConfig>,
    pub witness_vector_generator: Option<FriWitnessVectorGeneratorConfig>,
    pub prover_group_config: Option<FriProverGroupConfig>,
    pub witness_generator: Option<FriWitnessGeneratorConfig>,
    pub prometheus_config: Option<PrometheusConfig>,
    pub proof_data_handler_config: Option<ProofDataHandlerConfig>,
    pub db_config: Option<DBConfig>,
    pub eth: Option<EthConfig>,
    pub snapshot_creator: Option<SnapshotsCreatorConfig>,
    pub observability: Option<ObservabilityConfig>,
}
