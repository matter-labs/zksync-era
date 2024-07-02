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
        ContractsConfig, DatabaseSecrets, FriProofCompressorConfig, FriProverConfig,
        FriProverGatewayConfig, FriWitnessGeneratorConfig, FriWitnessVectorGeneratorConfig,
        L1Secrets, ObservabilityConfig, PrometheusConfig, ProofDataHandlerConfig,
        ProtectiveReadsWriterConfig, Secrets,
    },
    ApiConfig, BaseTokenAdjusterConfig, ContractVerifierConfig, DBConfig, EthConfig,
    EthWatchConfig, GasAdjusterConfig, GenesisConfig, ObjectStoreConfig, PostgresConfig,
    SnapshotsCreatorConfig,
};
use zksync_core_leftovers::{
    genesis_init, is_genesis_needed,
    temp_config_store::{decode_yaml_repr, TempConfigStore},
    Component, Components,
};
use zksync_env_config::FromEnv;
use zksync_eth_client::clients::Client;

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
    /// Rebuild tree.
    #[arg(long)]
    rebuild_tree: bool,
    /// Comma-separated list of components to launch.
    #[arg(
        long,
        default_value = "api,tree,eth,state_keeper,housekeeper,tee_verifier_input_producer,commitment_generator,base_token_ratio_persister"
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
        None => tmp_config.general(),
        Some(path) => {
            let yaml =
                std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
            decode_yaml_repr::<zksync_protobuf_config::proto::general::GeneralConfig>(&yaml)
                .context("failed decoding general YAML config")?
        }
    };

    let observability_config = configs
        .observability
        .clone()
        .context("observability config")?;

    let log_format: zksync_vlog::LogFormat = observability_config
        .log_format
        .parse()
        .context("Invalid log format")?;

    let mut builder = zksync_vlog::ObservabilityBuilder::new().with_log_format(log_format);
    if let Some(log_directives) = observability_config.log_directives {
        builder = builder.with_log_directives(log_directives);
    }

    if let Some(sentry_url) = &observability_config.sentry_url {
        builder = builder
            .with_sentry_url(sentry_url)
            .expect("Invalid Sentry URL")
            .with_sentry_environment(observability_config.sentry_environment);
    }
    let _guard = builder.build();

    // Report whether sentry is running after the logging subsystem was initialized.
    if let Some(sentry_url) = observability_config.sentry_url {
        tracing::info!("Sentry configured with URL: {sentry_url}");
    } else {
        tracing::info!("No sentry URL was provided");
    }

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

    let consensus = config::read_consensus_config().context("read_consensus_config()")?;

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

    run_genesis_if_needed(opt.genesis, &genesis, &contracts_config, &secrets)?;
    if opt.genesis {
        // If genesis is requested, we don't need to run the node.
        return Ok(());
    }

    let components = if opt.rebuild_tree {
        vec![Component::Tree]
    } else {
        opt.components.0
    };

    let node = MainNodeBuilder::new(
        configs,
        wallets,
        genesis,
        contracts_config,
        secrets,
        consensus,
    )
    .build(components)?;
    node.run()?;
    Ok(())
}

fn run_genesis_if_needed(
    force_genesis: bool,
    genesis: &GenesisConfig,
    contracts_config: &ContractsConfig,
    secrets: &Secrets,
) -> anyhow::Result<()> {
    let tokio_runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    tokio_runtime.block_on(async move {
        let database_secrets = secrets.database.clone().context("DatabaseSecrets")?;
        if force_genesis || is_genesis_needed(&database_secrets).await {
            genesis_init(genesis.clone(), &database_secrets)
                .await
                .context("genesis_init")?;

            if let Some(ecosystem_contracts) = &contracts_config.ecosystem_contracts {
                let l1_secrets = secrets.l1.as_ref().context("l1_screts")?;
                let query_client = Client::http(l1_secrets.l1_rpc_url.clone())
                    .context("Ethereum client")?
                    .for_network(genesis.l1_chain_id.into())
                    .build();
                zksync_node_genesis::save_set_chain_id_tx(
                    &query_client,
                    contracts_config.diamond_proxy_addr,
                    ecosystem_contracts.state_transition_proxy_addr,
                    &database_secrets,
                )
                .await
                .context("Failed to save SetChainId upgrade transaction")?;
            }
        }
        Ok(())
    })
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
        protective_reads_writer_config: ProtectiveReadsWriterConfig::from_env().ok(),
        core_object_store: ObjectStoreConfig::from_env().ok(),
        base_token_adjuster_config: BaseTokenAdjusterConfig::from_env().ok(),
        commitment_generator: None,
        pruning: None,
        snapshot_recovery: None,
    })
}
