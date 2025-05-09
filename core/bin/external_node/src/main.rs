use std::{collections::HashSet, str::FromStr};

use anyhow::Context as _;
use clap::Parser;
use node_builder::ExternalNodeBuilder;
use zksync_web3_decl::client::{Client, DynClient, L2};

use crate::config::{generate_consensus_secrets, ExternalNodeConfig};

mod config;
mod metadata;
mod metrics;
mod node_builder;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, clap::Subcommand)]
enum Command {
    /// Generates consensus secret keys to use in the secrets file.
    /// Prints the keys to the stdout, you need to copy the relevant keys into your secrets file.
    GenerateSecrets,
}

/// External node for ZKsync Era.
#[derive(Debug, Parser)]
#[command(author = "Matter Labs", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Enables consensus-based syncing instead of JSON-RPC based one. This is an experimental and incomplete feature;
    /// do not use unless you know what you're doing.
    #[arg(long)]
    enable_consensus: bool,

    /// Comma-separated list of components to launch.
    #[arg(long, default_value = "all")]
    components: ComponentsToRun,
    /// Path to the yaml config. If set, it will be used instead of env vars.
    #[arg(
        long,
        requires = "secrets_path",
        requires = "external_node_config_path"
    )]
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
        requires = "external_node_config_path",
        requires = "enable_consensus"
    )]
    consensus_path: Option<std::path::PathBuf>,
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
    ContractVerificationApi,
}

impl Component {
    fn components_from_str(s: &str) -> anyhow::Result<&[Component]> {
        match s {
            "api" => Ok(&[Component::HttpApi, Component::WsApi]),
            "http_api" => Ok(&[Component::HttpApi]),
            "ws_api" => Ok(&[Component::WsApi]),
            "contract_verification_api" => Ok(&[Component::ContractVerificationApi]),
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
                Component::ContractVerificationApi,
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

    if let Some(cmd) = &opt.command {
        match cmd {
            Command::GenerateSecrets => generate_consensus_secrets(),
        }
        return Ok(());
    }

    let mut config = if let Some(config_path) = opt.config_path.clone() {
        let secrets_path = opt.secrets_path.clone().unwrap();
        let external_node_config_path = opt.external_node_config_path.clone().unwrap();
        if opt.enable_consensus {
            anyhow::ensure!(
                opt.consensus_path.is_some(),
                "if --config-path and --enable-consensus are specified, then --consensus-path should be used to specify the location of the consensus config"
            );
        }
        ExternalNodeConfig::from_files(
            config_path,
            external_node_config_path,
            secrets_path,
            opt.consensus_path.clone(),
        )?
    } else {
        ExternalNodeConfig::new().context("Failed to load node configuration")?
    };

    if !opt.enable_consensus {
        config.consensus = None;
    }
    let guard = {
        // Observability stack implicitly spawns several tokio tasks, so we need to call this method
        // from within tokio context.
        let _rt_guard = runtime.enter();
        config.observability.build_observability()?
    };

    // Build L1 and L2 clients.
    let main_node_url = &config.required.main_node_url;
    tracing::info!("Main node URL is: {main_node_url:?}");
    let main_node_client = Client::http(main_node_url.clone())
        .context("failed creating JSON-RPC client for main node")?
        .for_network(config.required.l2_chain_id.into())
        .with_allowed_requests_per_second(config.optional.main_node_rate_limit_rps)
        .build();
    let main_node_client = Box::new(main_node_client) as Box<DynClient<L2>>;

    let config = runtime
        .block_on(config.fetch_remote(main_node_client.as_ref()))
        .context("failed fetching remote part of node config from main node")?;

    let node = ExternalNodeBuilder::on_runtime(runtime, config)
        .build(opt.components.0.into_iter().collect())?;
    node.run(guard)?;
    anyhow::Ok(())
}
