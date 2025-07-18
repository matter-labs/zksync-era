use std::{collections::HashSet, env, str::FromStr};

use anyhow::Context as _;
use clap::Parser;
use node_builder::ExternalNodeBuilder;
use smart_config::Prefixed;
use zksync_config::{
    cli::ConfigArgs,
    sources::{ConfigFilePaths, ConfigSources},
};
use zksync_types::L1BatchNumber;
use zksync_web3_decl::client::{Client, DynClient, L2};

use crate::config::{generate_consensus_secrets, ExternalNodeConfig, LocalConfig};

mod config;
mod metadata;
mod metrics;
mod node_builder;
#[cfg(test)]
mod tests;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[derive(Debug, clap::Subcommand)]
enum Command {
    /// Generates consensus secret keys to use in the secrets file.
    /// Prints the keys to the stdout, you need to copy the relevant keys into your secrets file.
    GenerateSecrets,
    /// Configuration-related tools.
    Config(ConfigArgs),
    /// Reverts the node state to the end of the specified L1 batch and then exits.
    Revert {
        /// The last L1 batch to be retained after the revert.
        l1_batch: L1BatchNumber,
    },
}

/// External node for ZKsync Era.
#[derive(Debug, Parser)]
#[command(author = "Matter Labs", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Consensus is enabled unconditionally. This var is unused and kept for compatibility.
    #[arg(long)]
    enable_consensus: bool,

    /// Comma-separated list of components to launch.
    #[arg(long, default_value = "all")]
    components: ComponentsToRun,
    /// Path to the yaml config. If set, it will be used instead of env vars.
    #[arg(long)]
    config_path: Option<std::path::PathBuf>,
    /// Path to the yaml with secrets. If set, it will be used instead of env vars.
    #[arg(long, requires = "config_path", requires = "external_node_config_path")]
    secrets_path: Option<std::path::PathBuf>,
    /// Path to the yaml with external node specific configuration. If set, it will be used instead of env vars.
    #[arg(long, requires = "config_path", requires = "secrets_path")]
    external_node_config_path: Option<std::path::PathBuf>,
    /// Path to the yaml with consensus config. If set, it will be used instead of env vars.
    #[arg(
        long,
        requires = "config_path",
        requires = "secrets_path",
        requires = "external_node_config_path"
    )]
    consensus_path: Option<std::path::PathBuf>,
}

impl Cli {
    fn config_sources(&self, env_prefix: Option<&str>) -> anyhow::Result<ConfigSources> {
        let config_file_paths = ConfigFilePaths {
            general: self.config_path.clone(),
            secrets: self.secrets_path.clone(),
            external_node: self.external_node_config_path.clone(),
            consensus: if let Some(path) = self.consensus_path.clone() {
                Some(path)
            } else if let Ok(path) = env::var("EN_CONSENSUS_CONFIG_PATH") {
                Some(path.into())
            } else {
                None
            },
            ..ConfigFilePaths::default()
        };
        let mut config_sources = config_file_paths.into_config_sources(env_prefix)?;
        // Legacy compatibility: read consensus secrets from one more source.
        if let Ok(path) = env::var("EN_CONSENSUS_SECRETS_PATH") {
            let yaml = ConfigFilePaths::read_yaml(path.as_ref())?;
            config_sources.push(Prefixed::new(yaml, "consensus"));
        }
        Ok(config_sources)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq)]
pub enum Component {
    HttpApi,
    WsApi,
    Tree,
    TreeApi,
    TreeFetcher,
    Core,
    DataAvailabilityFetcher,
}

impl Component {
    fn components_from_str(s: &str) -> anyhow::Result<&[Component]> {
        match s {
            "api" => Ok(&[Component::HttpApi, Component::WsApi]),
            "http_api" => Ok(&[Component::HttpApi]),
            "ws_api" => Ok(&[Component::WsApi]),
            "tree" => Ok(&[Component::Tree]),
            "tree_api" => Ok(&[Component::TreeApi]),
            "tree_fetcher" => Ok(&[Component::TreeFetcher]),
            "da_fetcher" => Ok(&[Component::DataAvailabilityFetcher]),
            "core" => Ok(&[Component::Core]),
            "all" => Ok(&[
                Component::HttpApi,
                Component::WsApi,
                Component::Tree,
                Component::Core,
            ]),
            other => Err(anyhow::anyhow!("{other} is not a valid component name")),
        }
    }
}

#[derive(Debug, Clone)]
struct ComponentsToRun(HashSet<Component>);

impl FromStr for ComponentsToRun {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let components = s
            .split(',')
            .try_fold(HashSet::new(), |mut acc, component_str| {
                let components = Component::components_from_str(component_str.trim())?;
                acc.extend(components);
                Ok::<_, Self::Err>(acc)
            })?;
        Ok(Self(components))
    }
}

fn tokio_runtime() -> anyhow::Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?)
}

fn main() -> anyhow::Result<()> {
    let runtime = tokio_runtime()?;

    // Initial setup.
    let opt = Cli::parse();
    let schema = LocalConfig::schema().context("Internal error: cannot build config schema")?;
    let config_sources = opt.config_sources(Some("EN_"))?;

    let observability = {
        // Observability initialization should be performed within tokio context.
        let _rt_guard = runtime.enter();
        config_sources.observability()?.install()?
    };
    let repo = config_sources.build_repository(&schema);

    let mut revert_to_l1_batch = None;
    if let Some(cmd) = opt.command {
        match cmd {
            Command::GenerateSecrets => {
                generate_consensus_secrets();
                return Ok(());
            }
            Command::Config(config_args) => {
                return config_args.run(repo.into(), "EN_");
            }
            Command::Revert { l1_batch } => {
                // We need to delay revert to after the config is fully read.
                revert_to_l1_batch = Some(l1_batch);
            }
        }
    }

    let config = ExternalNodeConfig::new(repo)?;

    if let Some(l1_batch) = revert_to_l1_batch {
        let node = ExternalNodeBuilder::on_runtime(runtime, config).build_for_revert(l1_batch)?;
        node.run(observability)?;
        return Ok(());
    }

    // Build L1 and L2 clients.
    let main_node_url = &config.local.networks.main_node_url;
    tracing::info!("Main node URL is: {main_node_url:?}");
    let main_node_client = Client::http(main_node_url.clone())
        .context("failed creating JSON-RPC client for main node")?
        .for_network(config.local.networks.l2_chain_id.into())
        .with_allowed_requests_per_second(config.local.networks.main_node_rate_limit_rps)
        .build();
    let main_node_client = Box::new(main_node_client) as Box<DynClient<L2>>;

    let config = runtime
        .block_on(config.fetch_remote(main_node_client.as_ref()))
        .context("failed fetching remote part of node config from main node")?;
    let node = ExternalNodeBuilder::on_runtime(runtime, config)
        .build(opt.components.0.into_iter().collect())?;
    node.run(observability)?;
    Ok(())
}
