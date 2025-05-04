use std::path::PathBuf;

use anyhow::Context;
use zksync_config::{
    configs::{
        api::{HealthCheckConfig, MerkleTreeApiConfig, Web3JsonRpcConfig},
        chain::{
            CircuitBreakerConfig, MempoolConfig, OperationsManagerConfig, StateKeeperConfig,
            TimestampAsserterConfig,
        },
        house_keeper::HouseKeeperConfig,
        vm_runner::BasicWitnessInputProducerConfig,
        wallets::{AddressWallet, EthSender, StateKeeper, TokenMultiplierSetter, Wallet, Wallets},
        CommitmentGeneratorConfig, DatabaseSecrets, ExperimentalVmConfig,
        ExternalPriceApiClientConfig, FriProofCompressorConfig, FriProverConfig,
        FriProverGatewayConfig, FriWitnessGeneratorConfig, GeneralConfig, ObservabilityConfig,
        PrometheusConfig, ProofDataHandlerConfig, ProtectiveReadsWriterConfig,
        ProverJobMonitorConfig, PruningConfig, SnapshotRecoveryConfig, TeeProofDataHandlerConfig,
    },
    ApiConfig, BaseTokenAdjusterConfig, ContractVerifierConfig, DAClientConfig, DADispatcherConfig,
    DBConfig, EthConfig, EthWatchConfig, ExternalProofIntegrationApiConfig, GasAdjusterConfig,
    ObjectStoreConfig, PostgresConfig, SnapshotsCreatorConfig,
};
use zksync_env_config::FromEnv;
use zksync_protobuf::repr::ProtoRepr;
use zksync_protobuf_config::proto::secrets::Secrets;

pub fn read_yaml_repr<T: ProtoRepr>(path: &PathBuf) -> anyhow::Result<T::Type> {
    (|| {
        let yaml = std::fs::read_to_string(path)?;
        zksync_protobuf::serde::Deserialize {
            deny_unknown_fields: false,
        }
        .proto_repr_from_yaml::<T>(&yaml)
    })()
    .with_context(|| format!("failed to read {}", path.display()))
}

// TODO (QIT-22): This structure is going to be removed when components will be responsible for their own configs.
/// A temporary config store allowing to pass deserialized configs from `zksync_server` to `zksync_core`.
///
/// All the configs are optional, since for some component combination it is not needed to pass all the configs.
#[derive(Debug, PartialEq, Default)]
pub struct TempConfigStore {
    pub postgres_config: Option<PostgresConfig>,
    pub health_check_config: Option<HealthCheckConfig>,
    pub merkle_tree_api_config: Option<MerkleTreeApiConfig>,
    pub web3_json_rpc_config: Option<Web3JsonRpcConfig>,
    pub circuit_breaker_config: Option<CircuitBreakerConfig>,
    pub mempool_config: Option<MempoolConfig>,
    pub contract_verifier: Option<ContractVerifierConfig>,
    pub operations_manager_config: Option<OperationsManagerConfig>,
    pub state_keeper_config: Option<StateKeeperConfig>,
    pub house_keeper_config: Option<HouseKeeperConfig>,
    pub fri_proof_compressor_config: Option<FriProofCompressorConfig>,
    pub fri_prover_config: Option<FriProverConfig>,
    pub fri_prover_gateway_config: Option<FriProverGatewayConfig>,
    pub fri_witness_generator_config: Option<FriWitnessGeneratorConfig>,
    pub prometheus_config: Option<PrometheusConfig>,
    pub proof_data_handler_config: Option<ProofDataHandlerConfig>,
    pub tee_proof_data_handler_config: Option<TeeProofDataHandlerConfig>,
    pub api_config: Option<ApiConfig>,
    pub db_config: Option<DBConfig>,
    pub eth_sender_config: Option<EthConfig>,
    pub eth_watch_config: Option<EthWatchConfig>,
    pub gas_adjuster_config: Option<GasAdjusterConfig>,
    pub observability: Option<ObservabilityConfig>,
    pub snapshot_creator: Option<SnapshotsCreatorConfig>,
    pub da_client_config: Option<DAClientConfig>,
    pub da_dispatcher_config: Option<DADispatcherConfig>,
    pub protective_reads_writer_config: Option<ProtectiveReadsWriterConfig>,
    pub basic_witness_input_producer_config: Option<BasicWitnessInputProducerConfig>,
    pub core_object_store: Option<ObjectStoreConfig>,
    pub base_token_adjuster_config: Option<BaseTokenAdjusterConfig>,
    pub commitment_generator: Option<CommitmentGeneratorConfig>,
    pub pruning: Option<PruningConfig>,
    pub snapshot_recovery: Option<SnapshotRecoveryConfig>,
    pub external_price_api_client_config: Option<ExternalPriceApiClientConfig>,
    pub external_proof_integration_api_config: Option<ExternalProofIntegrationApiConfig>,
    pub experimental_vm_config: Option<ExperimentalVmConfig>,
    pub prover_job_monitor_config: Option<ProverJobMonitorConfig>,
    pub timestamp_asserter_config: Option<TimestampAsserterConfig>,
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
            witness_generator_config: self.fri_witness_generator_config.clone(),
            prometheus_config: self.prometheus_config.clone(),
            proof_data_handler_config: self.proof_data_handler_config.clone(),
            tee_proof_data_handler_config: self.tee_proof_data_handler_config.clone(),
            db_config: self.db_config.clone(),
            eth: self.eth_sender_config.clone(),
            snapshot_creator: self.snapshot_creator.clone(),
            observability: self.observability.clone(),
            da_client_config: self.da_client_config.clone(),
            da_dispatcher_config: self.da_dispatcher_config.clone(),
            protective_reads_writer_config: self.protective_reads_writer_config.clone(),
            basic_witness_input_producer_config: self.basic_witness_input_producer_config.clone(),
            core_object_store: self.core_object_store.clone(),
            base_token_adjuster: self.base_token_adjuster_config.clone(),
            commitment_generator: self.commitment_generator.clone(),
            snapshot_recovery: self.snapshot_recovery.clone(),
            pruning: self.pruning.clone(),
            external_price_api_client_config: self.external_price_api_client_config.clone(),
            consensus_config: None,
            external_proof_integration_api_config: self
                .external_proof_integration_api_config
                .clone(),
            experimental_vm_config: self.experimental_vm_config.clone(),
            prover_job_monitor_config: self.prover_job_monitor_config.clone(),
            timestamp_asserter_config: self.timestamp_asserter_config.clone(),
        }
    }

    #[allow(deprecated)]
    pub fn wallets(&self) -> Wallets {
        let eth_sender = self.eth_sender_config.as_ref().and_then(|config| {
            let sender = config.get_eth_sender_config_for_sender_layer_data_layer()?;
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
        let token_multiplier_setter = self.base_token_adjuster_config.as_ref().and_then(|config| {
            let pk = config.private_key().ok()??;
            let wallet = Wallet::new(pk);
            Some(TokenMultiplierSetter { wallet })
        });
        Wallets {
            eth_sender,
            state_keeper,
            token_multiplier_setter,
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
        contract_verifier: ContractVerifierConfig::from_env().ok(),
        operations_manager_config: OperationsManagerConfig::from_env().ok(),
        state_keeper_config: StateKeeperConfig::from_env().ok(),
        house_keeper_config: HouseKeeperConfig::from_env().ok(),
        fri_proof_compressor_config: FriProofCompressorConfig::from_env().ok(),
        fri_prover_config: FriProverConfig::from_env().ok(),
        fri_prover_gateway_config: FriProverGatewayConfig::from_env().ok(),
        fri_witness_generator_config: FriWitnessGeneratorConfig::from_env().ok(),
        prometheus_config: PrometheusConfig::from_env().ok(),
        proof_data_handler_config: ProofDataHandlerConfig::from_env().ok(),
        tee_proof_data_handler_config: TeeProofDataHandlerConfig::from_env().ok(),
        api_config: ApiConfig::from_env().ok(),
        db_config: DBConfig::from_env().ok(),
        eth_sender_config: EthConfig::from_env().ok(),
        eth_watch_config: EthWatchConfig::from_env().ok(),
        gas_adjuster_config: GasAdjusterConfig::from_env().ok(),
        observability: ObservabilityConfig::from_env().ok(),
        snapshot_creator: SnapshotsCreatorConfig::from_env().ok(),
        da_client_config: DAClientConfig::from_env().ok(),
        da_dispatcher_config: DADispatcherConfig::from_env().ok(),
        protective_reads_writer_config: ProtectiveReadsWriterConfig::from_env().ok(),
        basic_witness_input_producer_config: BasicWitnessInputProducerConfig::from_env().ok(),
        core_object_store: ObjectStoreConfig::from_env().ok(),
        base_token_adjuster_config: BaseTokenAdjusterConfig::from_env().ok(),
        commitment_generator: None,
        pruning: None,
        snapshot_recovery: None,
        external_price_api_client_config: ExternalPriceApiClientConfig::from_env().ok(),
        external_proof_integration_api_config: ExternalProofIntegrationApiConfig::from_env().ok(),
        experimental_vm_config: ExperimentalVmConfig::from_env().ok(),
        prover_job_monitor_config: ProverJobMonitorConfig::from_env().ok(),
        timestamp_asserter_config: TimestampAsserterConfig::from_env().ok(),
    })
}

pub fn load_general_config(path: Option<PathBuf>) -> anyhow::Result<GeneralConfig> {
    match path {
        Some(path) => {
            read_yaml_repr::<zksync_protobuf_config::proto::general::GeneralConfig>(&path)
        }
        None => Ok(load_env_config()
            .context("general config from env")?
            .general()),
    }
}

pub fn load_database_secrets(path: Option<PathBuf>) -> anyhow::Result<DatabaseSecrets> {
    match path {
        Some(path) => {
            let secrets = read_yaml_repr::<Secrets>(&path)?;
            Ok(secrets
                .database
                .context("failed to parse database secrets")?)
        }
        None => DatabaseSecrets::from_env(),
    }
}
