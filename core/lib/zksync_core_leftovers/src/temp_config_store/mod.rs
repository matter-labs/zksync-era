use std::path::PathBuf;

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
        vm_runner::BasicWitnessInputProducerConfig,
        wallets::{AddressWallet, EthSender, StateKeeper, Wallet, Wallets},
        CommitmentGeneratorConfig, DatabaseSecrets, ExperimentalVmConfig,
        ExperimentalVmPlaygroundConfig, ExternalPriceApiClientConfig, FriProofCompressorConfig,
        FriProverConfig, FriProverGatewayConfig, FriWitnessGeneratorConfig,
        FriWitnessVectorGeneratorConfig, GeneralConfig, ObservabilityConfig, PrometheusConfig,
        ProofDataHandlerConfig, ProtectiveReadsWriterConfig, PruningConfig, SnapshotRecoveryConfig,
    },
    ApiConfig, BaseTokenAdjusterConfig, ContractVerifierConfig, DADispatcherConfig, DBConfig,
    EthConfig, EthWatchConfig, GasAdjusterConfig, ObjectStoreConfig, PostgresConfig,
    SnapshotsCreatorConfig,
};
use zksync_env_config::FromEnv;
use zksync_protobuf::repr::ProtoRepr;
use zksync_protobuf_config::proto::secrets::Secrets;

pub fn decode_yaml_repr<T: ProtoRepr>(yaml: &str) -> anyhow::Result<T::Type> {
    let d = serde_yaml::Deserializer::from_str(yaml);
    let this: T = zksync_protobuf::serde::deserialize_proto_with_options(d, false)?;
    this.read()
}

pub fn read_yaml_repr<T: ProtoRepr>(path_buf: PathBuf) -> anyhow::Result<T::Type> {
    let yaml = std::fs::read_to_string(path_buf).context("failed reading YAML config")?;
    decode_yaml_repr::<T>(&yaml)
}

// TODO (QIT-22): This structure is going to be removed when components will be responsible for their own configs.
/// A temporary config store allowing to pass deserialized configs from `zksync_server` to `zksync_core`.
/// All the configs are optional, since for some component combination it is not needed to pass all the configs.
#[derive(Debug, PartialEq, Default)]
pub struct TempConfigStore {
    pub postgres_config: Option<PostgresConfig>,
    pub health_check_config: Option<HealthCheckConfig>,
    pub merkle_tree_api_config: Option<MerkleTreeApiConfig>,
    pub web3_json_rpc_config: Option<Web3JsonRpcConfig>,
    pub circuit_breaker_config: Option<CircuitBreakerConfig>,
    pub mempool_config: Option<MempoolConfig>,
    pub network_config: Option<NetworkConfig>,
    pub contract_verifier: Option<ContractVerifierConfig>,
    pub operations_manager_config: Option<OperationsManagerConfig>,
    pub state_keeper_config: Option<StateKeeperConfig>,
    pub house_keeper_config: Option<HouseKeeperConfig>,
    pub fri_proof_compressor_config: Option<FriProofCompressorConfig>,
    pub fri_prover_config: Option<FriProverConfig>,
    pub fri_prover_group_config: Option<FriProverGroupConfig>,
    pub fri_prover_gateway_config: Option<FriProverGatewayConfig>,
    pub fri_witness_vector_generator: Option<FriWitnessVectorGeneratorConfig>,
    pub fri_witness_generator_config: Option<FriWitnessGeneratorConfig>,
    pub prometheus_config: Option<PrometheusConfig>,
    pub proof_data_handler_config: Option<ProofDataHandlerConfig>,
    pub api_config: Option<ApiConfig>,
    pub db_config: Option<DBConfig>,
    pub eth_sender_config: Option<EthConfig>,
    pub eth_watch_config: Option<EthWatchConfig>,
    pub gas_adjuster_config: Option<GasAdjusterConfig>,
    pub observability: Option<ObservabilityConfig>,
    pub snapshot_creator: Option<SnapshotsCreatorConfig>,
    pub da_dispatcher_config: Option<DADispatcherConfig>,
    pub protective_reads_writer_config: Option<ProtectiveReadsWriterConfig>,
    pub basic_witness_input_producer_config: Option<BasicWitnessInputProducerConfig>,
    pub vm_playground_config: Option<ExperimentalVmPlaygroundConfig>,
    pub core_object_store: Option<ObjectStoreConfig>,
    pub base_token_adjuster_config: Option<BaseTokenAdjusterConfig>,
    pub commitment_generator: Option<CommitmentGeneratorConfig>,
    pub pruning: Option<PruningConfig>,
    pub snapshot_recovery: Option<SnapshotRecoveryConfig>,
    pub external_price_api_client_config: Option<ExternalPriceApiClientConfig>,
    pub experimental_vm_config: Option<ExperimentalVmConfig>,
}

impl TempConfigStore {
    pub fn general(&self) -> GeneralConfig {
        GeneralConfig {
            postgres_config: self.postgres_config.clone(),
            api_config: self.api_config.clone(),
            contract_verifier: self.contract_verifier.clone(),
            circuit_breaker_config: self.circuit_breaker_config.clone(),
            mempool_config: self.mempool_config.clone(),
            operations_manager_config: self.operations_manager_config.clone(),
            state_keeper_config: self.state_keeper_config.clone(),
            house_keeper_config: self.house_keeper_config.clone(),
            proof_compressor_config: self.fri_proof_compressor_config.clone(),
            prover_config: self.fri_prover_config.clone(),
            prover_gateway: self.fri_prover_gateway_config.clone(),
            witness_vector_generator: self.fri_witness_vector_generator.clone(),
            prover_group_config: self.fri_prover_group_config.clone(),
            witness_generator: self.fri_witness_generator_config.clone(),
            prometheus_config: self.prometheus_config.clone(),
            proof_data_handler_config: self.proof_data_handler_config.clone(),
            db_config: self.db_config.clone(),
            eth: self.eth_sender_config.clone(),
            snapshot_creator: self.snapshot_creator.clone(),
            observability: self.observability.clone(),
            da_dispatcher_config: self.da_dispatcher_config.clone(),
            protective_reads_writer_config: self.protective_reads_writer_config.clone(),
            basic_witness_input_producer_config: self.basic_witness_input_producer_config.clone(),
            vm_playground_config: self.vm_playground_config.clone(),
            core_object_store: self.core_object_store.clone(),
            base_token_adjuster: self.base_token_adjuster_config.clone(),
            commitment_generator: self.commitment_generator.clone(),
            snapshot_recovery: self.snapshot_recovery.clone(),
            pruning: self.pruning.clone(),
            external_price_api_client_config: self.external_price_api_client_config.clone(),
            consensus_config: None,
            experimental_vm_config: self.experimental_vm_config.clone(),
        }
    }

    #[allow(deprecated)]
    pub fn wallets(&self) -> Wallets {
        let eth_sender = self.eth_sender_config.as_ref().and_then(|config| {
            let sender = config.sender.as_ref()?;
            let operator_private_key = sender.private_key().ok()??;
            let operator = Wallet::new(operator_private_key);
            let blob_operator = sender
                .private_key_blobs()
                .and_then(|operator| Wallet::from_private_key_bytes(operator, None).ok());
            Some(EthSender {
                operator,
                blob_operator,
            })
        });
        let state_keeper = self
            .state_keeper_config
            .as_ref()
            .map(|state_keeper| StateKeeper {
                fee_account: AddressWallet::from_address(
                    state_keeper
                        .fee_account_addr
                        .expect("Must be presented in env variables"),
                ),
            });
        Wallets {
            eth_sender,
            state_keeper,
        }
    }
}

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
        da_dispatcher_config: DADispatcherConfig::from_env().ok(),
        protective_reads_writer_config: ProtectiveReadsWriterConfig::from_env().ok(),
        basic_witness_input_producer_config: BasicWitnessInputProducerConfig::from_env().ok(),
        vm_playground_config: ExperimentalVmPlaygroundConfig::from_env().ok(),
        core_object_store: ObjectStoreConfig::from_env().ok(),
        base_token_adjuster_config: BaseTokenAdjusterConfig::from_env().ok(),
        commitment_generator: None,
        pruning: None,
        snapshot_recovery: None,
        external_price_api_client_config: ExternalPriceApiClientConfig::from_env().ok(),
        experimental_vm_config: ExperimentalVmConfig::from_env().ok(),
    })
}

pub fn load_general_config(path: Option<PathBuf>) -> anyhow::Result<GeneralConfig> {
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

pub fn load_database_secrets(path: Option<PathBuf>) -> anyhow::Result<DatabaseSecrets> {
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
