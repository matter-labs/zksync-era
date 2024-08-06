use std::str::FromStr;

use anyhow::Context as _;
use clap::Parser;
use zksync_config::{
    configs::{
        api::{HealthCheckConfig, MerkleTreeApiConfig, Web3JsonRpcConfig},
        chain::{
            CircuitBreakerConfig, MempoolConfig, NetworkConfig, OperationsManagerConfig,
            StateKeeperConfig,
        },
        fri_prover_group::FriProverGroupConfig,
        house_keeper::HouseKeeperConfig,
        BasicWitnessInputProducerConfig, ContractsConfig, DatabaseSecrets,
        ExperimentalVmPlaygroundConfig, ExternalPriceApiClientConfig, FriProofCompressorConfig,
        FriProverConfig, FriProverGatewayConfig, FriWitnessGeneratorConfig,
        FriWitnessVectorGeneratorConfig, L1Secrets, ObservabilityConfig, PrometheusConfig,
        ProofDataHandlerConfig, ProtectiveReadsWriterConfig, Secrets,
    },
    ApiConfig, BaseTokenAdjusterConfig, ContractVerifierConfig, DADispatcherConfig, DBConfig,
    EthConfig, EthWatchConfig, GasAdjusterConfig, GenesisConfig, ObjectStoreConfig, PostgresConfig,
    SnapshotsCreatorConfig,
};
use zksync_core_leftovers::{
    temp_config_store::{decode_yaml_repr, TempConfigStore},
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
    #[arg(
        long,
        default_value = "api,tree,eth,state_keeper,housekeeper,tee_verifier_input_producer,commitment_generator,da_dispatcher,vm_runner_protective_reads"
    )]
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
            let yaml =
                std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
            decode_yaml_repr::<zksync_protobuf_config::proto::general::GeneralConfig>(&yaml)
                .context("failed decoding general YAML config")?
        }
    };

    let wallets = match opt.wallets_path {
        None => tmp_config.wallets(),
        Some(path) => {
            let yaml =
                std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
            decode_yaml_repr::<zksync_protobuf_config::proto::wallets::Wallets>(&yaml)
                .context("failed decoding wallets YAML config")?
        }
    };

    let secrets: Secrets = match opt.secrets_path {
        Some(path) => {
            let yaml =
                std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
            decode_yaml_repr::<zksync_protobuf_config::proto::secrets::Secrets>(&yaml)
                .context("failed decoding secrets YAML config")?
        }
        None => Secrets {
            consensus: config::read_consensus_secrets().context("read_consensus_secrets()")?,
            database: DatabaseSecrets::from_env().ok(),
            l1: L1Secrets::from_env().ok(),
        },
    };

    let contracts_config = match opt.contracts_config_path {
        None => ContractsConfig::from_env().context("contracts_config")?,
        Some(path) => {
            let yaml =
                std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
            decode_yaml_repr::<zksync_protobuf_config::proto::contracts::Contracts>(&yaml)
                .context("failed decoding contracts YAML config")?
        }
    };

    let genesis = match opt.genesis_path {
        None => GenesisConfig::from_env().context("Genesis config")?,
        Some(path) => {
            let yaml =
                std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
            decode_yaml_repr::<zksync_protobuf_config::proto::genesis::Genesis>(&yaml)
                .context("failed decoding genesis YAML config")?
        }
    };
    let observability_config = configs
        .observability
        .clone()
        .context("observability config")?;

    let node = MainNodeBuilder::new(configs, wallets, genesis, contracts_config, secrets)?;

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

    node.build(opt.components.0)?.run(observability_guard)?;
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
    })
}
