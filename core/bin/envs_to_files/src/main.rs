use std::fs;

use serde_yaml::Serializer;
use zksync_config::{
    configs::{
        api::{HealthCheckConfig, MerkleTreeApiConfig, Web3JsonRpcConfig},
        chain::{
            CircuitBreakerConfig, MempoolConfig, NetworkConfig, OperationsManagerConfig,
            StateKeeperConfig,
        },
        fri_prover_group::FriProverGroupConfig,
        house_keeper::HouseKeeperConfig,
        BasicWitnessInputProducerConfig, DatabaseSecrets, FriProofCompressorConfig,
        FriProverConfig, FriProverGatewayConfig, FriWitnessGeneratorConfig,
        FriWitnessVectorGeneratorConfig, L1Secrets, ObjectStoreConfig, ObservabilityConfig,
        PrometheusConfig, ProofDataHandlerConfig, ProtectiveReadsWriterConfig,
    },
    ApiConfig, BaseTokenAdjusterConfig, ContractVerifierConfig, ContractsConfig,
    DADispatcherConfig, DBConfig, EthConfig, EthWatchConfig, GasAdjusterConfig, GenesisConfig,
    PostgresConfig, SnapshotsCreatorConfig,
};
use zksync_core_leftovers::temp_config_store::TempConfigStore;
use zksync_env_config::FromEnv;
use zksync_protobuf::{
    build::{prost_reflect, prost_reflect::ReflectMessage},
    ProtoRepr,
};
use zksync_protobuf_config::proto::{
    contracts::Contracts as ProtoContracts, general::GeneralConfig as ProtoGeneral,
    genesis::Genesis as ProtoGenesis, secrets::Secrets as ProtoSecrets,
    wallets::Wallets as ProtoWallets,
};

fn load_env_config() -> TempConfigStore {
    TempConfigStore {
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
        da_dispatcher_config: DADispatcherConfig::from_env().ok(),
        protective_reads_writer_config: ProtectiveReadsWriterConfig::from_env().ok(),
        basic_witness_input_producer_config: BasicWitnessInputProducerConfig::from_env().ok(),
        core_object_store: ObjectStoreConfig::from_env().ok(),
        base_token_adjuster_config: BaseTokenAdjusterConfig::from_env().ok(),
        commitment_generator: None,
        pruning: None,
        snapshot_recovery: None,
    }
}

fn main() {
    let config = load_env_config();

    let contracts = ContractsConfig::from_env().unwrap();
    let general = config.general();
    let wallets = config.wallets();
    let genesis = GenesisConfig::from_env().unwrap();
    let secrets = zksync_config::configs::Secrets {
        consensus: None,
        database: DatabaseSecrets::from_env().ok(),
        l1: L1Secrets::from_env().ok(),
    };

    let data = encode_yaml(&ProtoGeneral::build(&general)).unwrap();
    fs::write("./general.yaml", data).unwrap();
    let data = encode_yaml(&ProtoGenesis::build(&genesis)).unwrap();
    fs::write("./genesis.yaml", data).unwrap();
    let data = encode_yaml(&ProtoWallets::build(&wallets)).unwrap();
    fs::write("./wallets.yaml", data).unwrap();
    let data = encode_yaml(&ProtoSecrets::build(&secrets)).unwrap();
    fs::write("./secrets.yaml", data).unwrap();
    let data = encode_yaml(&ProtoContracts::build(&contracts)).unwrap();
    fs::write("./contracts.yaml", data).unwrap();
}

pub(crate) fn encode_yaml<T: ReflectMessage>(x: &T) -> anyhow::Result<String> {
    let mut serializer = Serializer::new(vec![]);
    let opts = prost_reflect::SerializeOptions::new()
        .use_proto_field_name(true)
        .stringify_64_bit_integers(false);
    x.transcode_to_dynamic()
        .serialize_with_options(&mut serializer, &opts)?;
    Ok(String::from_utf8_lossy(&serializer.into_inner()?).to_string())
}
