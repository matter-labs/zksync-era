use std::str::FromStr;

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use tokio::runtime::Runtime;
use zksync_config::{
    configs::{wallets::Wallets, ContractsConfig, GeneralConfig, ObservabilityConfig, Secrets},
    full_config_schema,
    sources::ConfigFilePaths,
    ConfigRepository, GenesisConfigWrapper, ParseResultExt,
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
    Config {
        /// If set, debugs configuration instead of printing help.
        #[arg(long)]
        debug: bool,
        /// Allows filtering config params by path.
        filter: Option<String>,
    },
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

    if let Some(CliCommand::Config {
        debug: false,
        filter,
    }) = opt.cmd
    {
        smart_config_commands::Printer::stdout().print_help(&schema, |param| {
            filter.as_ref().map_or(true, |needle| {
                param.all_paths().any(|path| path.contains(needle))
            })
        })?;
        return Ok(());
    }

    let config_file_paths = ConfigFilePaths {
        general: opt.config_path,
        secrets: opt.secrets_path,
        contracts: opt.contracts_config_path,
        genesis: opt.genesis_path,
        wallets: opt.wallets_path,
        ..ConfigFilePaths::default()
    };
    let config_sources = config_file_paths.into_config_sources("")?;

    let observability_config =
        ObservabilityConfig::from_sources(config_sources.clone()).context("ObservabilityConfig")?;
    let runtime = Runtime::new().context("failed creating Tokio runtime")?;
    let observability_guard = {
        // Observability initialization should be performed within tokio context.
        let _context_guard = runtime.enter();
        observability_config.install()?
    };

    let mut repo = ConfigRepository::new(&schema).with_all(config_sources);
    repo.deserializer_options().coerce_variant_names = true;
    let configs: GeneralConfig = repo.single()?.parse().log_all_errors()?;
    let wallets: Wallets = repo.single()?.parse().log_all_errors()?;
    let secrets: Secrets = repo.single()?.parse().log_all_errors()?;
    let contracts_config: ContractsConfig = repo.single()?.parse().log_all_errors()?;
    let genesis = repo
        .single::<GenesisConfigWrapper>()?
        .parse()
        .log_all_errors()?
        .genesis
        .context("missing genesis config")?;

    if let Some(CliCommand::Config {
        debug: true,
        filter,
    }) = opt.cmd
    {
        smart_config_commands::Printer::stdout().print_debug(&repo, |param| {
            filter.as_ref().map_or(true, |needle| {
                param.all_paths().any(|path| path.contains(needle))
            })
        })?;
        return Ok(());
    }

    let node = MainNodeBuilder::new(
        runtime,
        configs,
        wallets,
        genesis,
        contracts_config,
        secrets,
    );
    if opt.genesis {
        // If genesis is requested, we don't need to run the node.
        node.only_genesis()?.run(observability_guard)?;
        return Ok(());
    }

    node.build(opt.components.0)?.run(observability_guard)?;
    Ok(())
}
