use zksync_config::{
    configs::{
        api::{HealthCheckConfig, MerkleTreeApiConfig, Web3JsonRpcConfig},
        chain::{
            CircuitBreakerConfig, MempoolConfig, NetworkConfig, OperationsManagerConfig,
            StateKeeperConfig,
        },
        fri_prover_group::FriProverGroupConfig,
        house_keeper::HouseKeeperConfig,
        FriProofCompressorConfig, FriProverConfig, FriWitnessGeneratorConfig, PrometheusConfig,
        ProofDataHandlerConfig, ProverGroupConfig, WitnessGeneratorConfig,
    },
    ApiConfig, ContractsConfig, DBConfig, ETHClientConfig, ETHSenderConfig, ETHWatchConfig,
    FetcherConfig, GasAdjusterConfig, ObjectStoreConfig, PostgresConfig, ProverConfigs,
};

// TODO (QIT-22): This structure is going to be removed when components will be respnsible for their own configs.
/// A temporary config store allowing to pass deserialized configs from `zksync_server` to `zksync_core`.
/// All the configs are optional, since for some component combination it is not needed to pass all the configs.
#[derive(Debug)]
pub struct TempConfigStore {
    pub postgres_config: Option<PostgresConfig>,
    pub health_check_config: Option<HealthCheckConfig>,
    pub merkle_tree_api_config: Option<MerkleTreeApiConfig>,
    pub web3_json_rpc_config: Option<Web3JsonRpcConfig>,
    pub circuit_breaker_config: Option<CircuitBreakerConfig>,
    pub mempool_config: Option<MempoolConfig>,
    pub network_config: Option<NetworkConfig>,
    pub operations_manager_config: Option<OperationsManagerConfig>,
    pub state_keeper_config: Option<StateKeeperConfig>,
    pub house_keeper_config: Option<HouseKeeperConfig>,
    pub fri_proof_compressor_config: Option<FriProofCompressorConfig>,
    pub fri_prover_config: Option<FriProverConfig>,
    pub fri_prover_group_config: Option<FriProverGroupConfig>,
    pub fri_witness_generator_config: Option<FriWitnessGeneratorConfig>,
    pub prometheus_config: Option<PrometheusConfig>,
    pub proof_data_handler_config: Option<ProofDataHandlerConfig>,
    pub prover_group_config: Option<ProverGroupConfig>,
    pub witness_generator_config: Option<WitnessGeneratorConfig>,
    pub api_config: Option<ApiConfig>,
    pub contracts_config: Option<ContractsConfig>,
    pub db_config: Option<DBConfig>,
    pub eth_client_config: Option<ETHClientConfig>,
    pub eth_sender_config: Option<ETHSenderConfig>,
    pub eth_watch_config: Option<ETHWatchConfig>,
    pub fetcher_config: Option<FetcherConfig>,
    pub gas_adjuster_config: Option<GasAdjusterConfig>,
    pub prover_configs: Option<ProverConfigs>,
    pub object_store_config: Option<ObjectStoreConfig>,
}
