use anyhow::Context;
use zksync_config::{
    configs::{
        api::{HealthCheckConfig, MerkleTreeApiConfig, Web3JsonRpcConfig},
        chain::{
            CircuitBreakerConfig, MempoolConfig, NetworkConfig, OperationsManagerConfig,
            StateKeeperConfig,
        },
        fri_prover_group::FriProverGroupConfig,
        house_keeper::HouseKeeperConfig,
        DatabaseSecrets, FriProofCompressorConfig, FriProverConfig, FriProverGatewayConfig,
        FriWitnessGeneratorConfig, FriWitnessVectorGeneratorConfig, GeneralConfig,
        ObjectStoreConfig, ObservabilityConfig, PrometheusConfig, ProofDataHandlerConfig,
        ProtectiveReadsWriterConfig,
    },
    ApiConfig, ContractVerifierConfig, DBConfig, EthConfig, EthWatchConfig, GasAdjusterConfig,
    PostgresConfig, SnapshotsCreatorConfig,
};
use zksync_core_leftovers::temp_config_store::{decode_yaml_repr, TempConfigStore};
use zksync_env_config::FromEnv;
use zksync_protobuf_config::proto::secrets::Secrets;

fn load_env_config() -> anyhow::Result<TempConfigStore> {
    Ok(TempConfigStore {
        postgres_config: PostgresConfig::from_env().ok(),
        health_check_config: HealthCheckConfig::from_env().ok(),
        merkle_tree_api_config: MerkleTreeApiConfig::from_env().ok(),
        web3_json_rpc_config: Web3JsonRpcConfig::from_env().ok(),
        circuit_breaker_config: CircuitBreakerConfig::from_env().ok(),
        mempool_config: MempoolConfig::from_env().ok(),
        network_config: NetworkConfig::from_env().ok(),
        contract_verifier: ContractVerifierConfig::from_env().ok(),
        operations_manager_config: OperationsManagerConfig::from_env().ok(),
        state_keeper_config: StateKeeperConfig::from_env().ok(),
        house_keeper_config: HouseKeeperConfig::from_env().ok(),
        fri_proof_compressor_config: FriProofCompressorConfig::from_env().ok(),
        fri_prover_config: FriProverConfig::from_env().ok(),
        fri_prover_group_config: FriProverGroupConfig::from_env().ok(),
        fri_prover_gateway_config: FriProverGatewayConfig::from_env().ok(),
        fri_witness_vector_generator: FriWitnessVectorGeneratorConfig::from_env().ok(),
        fri_witness_generator_config: FriWitnessGeneratorConfig::from_env().ok(),
        prometheus_config: PrometheusConfig::from_env().ok(),
        proof_data_handler_config: ProofDataHandlerConfig::from_env().ok(),
        api_config: ApiConfig::from_env().ok(),
        db_config: DBConfig::from_env().ok(),
        eth_sender_config: EthConfig::from_env().ok(),
        eth_watch_config: EthWatchConfig::from_env().ok(),
        gas_adjuster_config: GasAdjusterConfig::from_env().ok(),
        observability: ObservabilityConfig::from_env().ok(),
        snapshot_creator: SnapshotsCreatorConfig::from_env().ok(),
        protective_reads_writer_config: ProtectiveReadsWriterConfig::from_env().ok(),
        core_object_store: ObjectStoreConfig::from_env().ok(),
    })
}

pub fn load_general_config(path: Option<std::path::PathBuf>) -> anyhow::Result<GeneralConfig> {
    match path {
        Some(path) => {
            let yaml = std::fs::read_to_string(path).context("Failed to read general config")?;
            decode_yaml_repr::<zksync_protobuf_config::proto::general::GeneralConfig>(&yaml)
        }
        None => Ok(load_env_config()
            .context("general config from env")?
            .general()),
    }
}

pub fn load_database_secrets(path: Option<std::path::PathBuf>) -> anyhow::Result<DatabaseSecrets> {
    match path {
        Some(path) => {
            let yaml = std::fs::read_to_string(path).context("Failed to read secrets")?;
            let secrets = decode_yaml_repr::<Secrets>(&yaml).context("Failed to parse secrets")?;
            Ok(secrets
                .database
                .context("failed to parse database secrets")?)
        }
        None => DatabaseSecrets::from_env(),
    }
}
