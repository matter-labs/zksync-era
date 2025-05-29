use std::str::FromStr;

use anyhow::Context as _;
use clap::Parser;
use zksync_config::{
    configs::{
        api::{HealthCheckConfig, MerkleTreeApiConfig, Web3JsonRpcConfig},
        chain::{
            CircuitBreakerConfig, MempoolConfig, OperationsManagerConfig, StateKeeperConfig,
            TimestampAsserterConfig,
        },
        house_keeper::HouseKeeperConfig,
        BasicWitnessInputProducerConfig, ContractVerifierSecrets, DataAvailabilitySecrets,
        DatabaseSecrets, ExperimentalVmConfig, ExternalPriceApiClientConfig,
        FriProofCompressorConfig, FriProverConfig, FriProverGatewayConfig,
        FriWitnessGeneratorConfig, L1Secrets, ObservabilityConfig, PrometheusConfig,
        ProofDataHandlerConfig, ProtectiveReadsWriterConfig, Secrets, TeeProofDataHandlerConfig,
    },
    ApiConfig, BaseTokenAdjusterConfig, ContractVerifierConfig, ContractsConfig, DAClientConfig,
    DADispatcherConfig, DBConfig, EthConfig, EthWatchConfig, ExternalProofIntegrationApiConfig,
    GasAdjusterConfig, GenesisConfig, ObjectStoreConfig, PostgresConfig, SnapshotsCreatorConfig,
};
use zksync_core_leftovers::{
    temp_config_store::{read_yaml_repr, TempConfigStore},
    Component, Components,
};
use zksync_env_config::FromEnv;

use crate::node_builder::MainNodeBuilder;

mod config;
mod node_builder;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;
#[derive(Debug, Parser)]
#[command(author = "Matter Labs", version, about = "ZKsync operator node", long_about = None)]
struct Cli {
    /// Generate genesis block for the first contract deployment using temporary DB.
    #[arg(long)]
    genesis: bool,
    /// Comma-separated list of components to launch.
    #[arg(long, default_value = "api,eth,state_keeper")]
    components: ComponentsToRun,
    /// Path to the yaml config. If set, it will be used instead of env vars.
    #[arg(long)]
    config_path: Option<std::path::PathBuf>,
    /// Path to the yaml with secrets. If set, it will be used instead of env vars.
    #[arg(long)]
    secrets_path: Option<std::path::PathBuf>,
    /// Path to the yaml with contracts. If set, it will be used instead of env vars.
    #[arg(long)]
    contracts_config_path: Option<std::path::PathBuf>,
    /// Path to the wallets config. If set, it will be used instead of env vars.
    #[arg(long)]
    wallets_path: Option<std::path::PathBuf>,
    /// Path to the yaml with genesis. If set, it will be used instead of env vars.
    #[arg(long)]
    genesis_path: Option<std::path::PathBuf>,
    /// Used to enable node framework.
    /// Now the node framework is used by default and this argument is left for backward compatibility.
    #[arg(long)]
    use_node_framework: bool,

    /// Only compose the node with the provided list of the components and then exit.
    /// Can be used to catch issues with configuration.
    #[arg(long, conflicts_with = "genesis")]
    no_run: bool,
}

#[derive(Debug, Clone)]
struct ComponentsToRun(Vec<Component>);

impl FromStr for ComponentsToRun {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let components = s.split(',').try_fold(vec![], |mut acc, component_str| {
            let components = Components::from_str(component_str.trim())?;
            acc.extend(components.0);
            Ok::<_, String>(acc)
        })?;
        Ok(Self(components))
    }
}

fn main() -> anyhow::Result<()> {
    dbg!(cfg!(feature = "zkos"));
    let opt = Cli::parse();

    // Load env config and use it if file config is not provided
    let tmp_config = load_env_config()?;

    let configs = match opt.config_path {
        None => {
            let mut configs = tmp_config.general();
            configs.consensus_config =
                config::read_consensus_config().context("read_consensus_config()")?;
            configs
        }
        Some(path) => {
            read_yaml_repr::<zksync_protobuf_config::proto::general::GeneralConfig>(&path)
                .context("failed decoding general YAML config")?
        }
    };

    let wallets = match opt.wallets_path {
        None => tmp_config.wallets(),
        Some(path) => read_yaml_repr::<zksync_protobuf_config::proto::wallets::Wallets>(&path)
            .context("failed decoding wallets YAML config")?,
    };

    let secrets: Secrets = match opt.secrets_path {
        Some(path) => read_yaml_repr::<zksync_protobuf_config::proto::secrets::Secrets>(&path)
            .context("failed decoding secrets YAML config")?,
        None => Secrets {
            consensus: config::read_consensus_secrets().context("read_consensus_secrets()")?,
            database: DatabaseSecrets::from_env().ok(),
            l1: L1Secrets::from_env().ok(),
            data_availability: DataAvailabilitySecrets::from_env().ok(),
            contract_verifier: ContractVerifierSecrets::from_env().ok(),
        },
    };

    let contracts_config = match opt.contracts_config_path {
        None => ContractsConfig::from_env().context("contracts_config")?,
        Some(path) => read_yaml_repr::<zksync_protobuf_config::proto::contracts::Contracts>(&path)
            .context("failed decoding contracts YAML config")?,
    };

    let genesis = match opt.genesis_path {
        None => GenesisConfig::from_env().context("Genesis config")?,
        Some(path) => read_yaml_repr::<zksync_protobuf_config::proto::genesis::Genesis>(&path)
            .context("failed decoding genesis YAML config")?,
    };
    let observability_config = configs
        .observability
        .clone()
        .context("observability config")?;

    let node = MainNodeBuilder::new(
        configs,
        wallets,
        genesis,
        secrets,
        contracts_config.l1_specific_contracts(),
        contracts_config.l2_contracts(),
        // Now we always pass the settlement layer contracts. After V27 upgrade,
        // it'd be possible to get rid of settlement_layer_specific_contracts in our configs.
        // For easier refactoring in the future. We can mark it as Optional
        Some(contracts_config.settlement_layer_specific_contracts()),
        Some(contracts_config.l1_multicall3_addr),
    )?;

    let observability_guard = {
        // Observability initialization should be performed within tokio context.
        let _context_guard = node.runtime_handle().enter();
        observability_config.install()?
    };

    if opt.genesis {
        // If genesis is requested, we don't need to run the node.
        node.only_genesis()?.run(observability_guard)?;
        return Ok(());
    }

    let node = node.build(opt.components.0)?;

    if opt.no_run {
        tracing::info!("Node composed successfully; exiting due to --no-run flag");
        return Ok(());
    }

    node.run(observability_guard)?;
    Ok(())
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
        prover_job_monitor_config: None,
        timestamp_asserter_config: TimestampAsserterConfig::from_env().ok(),
    })
}
