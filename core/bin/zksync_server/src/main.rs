use std::str::FromStr;

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use tokio::runtime::Runtime;
use zksync_config::{
    cli::ConfigArgs,
    configs::{wallets::Wallets, ContractsConfig, GeneralConfig, GenesisConfigWrapper, Secrets},
    full_config_schema,
    sources::ConfigFilePaths,
    ConfigRepositoryExt,
};
use zksync_core_leftovers::{Component, Components};

use crate::node_builder::MainNodeBuilder;

mod node_builder;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[derive(Debug, Subcommand)]
enum CliCommand {
    /// Configuration-related tools.
    Config(ConfigArgs),
}

#[derive(Debug, Parser)]
#[command(author = "Matter Labs", version, about = "ZKsync operator node", long_about = None)]
struct Cli {
    /// Generate genesis block for the first contract deployment using temporary DB.
    #[arg(long)]
    genesis: bool,
    /// Comma-separated list of components to launch.
    #[arg(
        long,
        default_value = "api,tree,eth,state_keeper,housekeeper,commitment_generator,da_dispatcher,vm_runner_protective_reads"
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

    /// Only compose the node with the provided list of the components and then exit.
    /// Can be used to catch issues with configuration.
    #[arg(long, conflicts_with = "genesis")]
    no_run: bool,

    #[command(subcommand)]
    cmd: Option<CliCommand>,
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
    let schema = full_config_schema(false);

    let config_file_paths = ConfigFilePaths {
        general: opt.config_path,
        secrets: opt.secrets_path,
        contracts: opt.contracts_config_path,
        genesis: opt.genesis_path,
        wallets: opt.wallets_path,
        ..ConfigFilePaths::default()
    };
    let config_sources = config_file_paths.into_config_sources("ZKSYNC_")?;

    let runtime = Runtime::new().context("failed creating Tokio runtime")?;
    let observability_guard = {
        // Observability initialization should be performed within tokio context.
        let _context_guard = runtime.enter();
        config_sources.observability()?.install()?
    };

    let repo = config_sources.build_repository(&schema);
    if let Some(command) = opt.cmd {
        match command {
            CliCommand::Config(config_args) => {
                config_args.run(repo)?;
            }
        }
        return Ok(());
    }

    let configs: GeneralConfig = repo.parse()?;
    let wallets: Wallets = repo.parse()?;
    let secrets: Secrets = repo.parse()?;
    let contracts_config: ContractsConfig = repo.parse()?;
    let genesis = repo
        .parse::<GenesisConfigWrapper>()?
        .genesis
        .context("missing genesis config")?;
    let consensus = repo.parse_opt()?;
    let node = MainNodeBuilder::new(
        runtime,
        configs,
        wallets,
        genesis,
        consensus,
        secrets,
        contracts_config.l1_specific_contracts(),
        contracts_config.l2_contracts(),
        // Now we always pass the settlement layer contracts. After V27 upgrade,
        // it'd be possible to get rid of settlement_layer_specific_contracts in our configs.
        // For easier refactoring in the future. We can mark it as Optional
        Some(contracts_config.settlement_layer_specific_contracts()),
        Some(contracts_config.l1.multicall3_addr),
    );
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
