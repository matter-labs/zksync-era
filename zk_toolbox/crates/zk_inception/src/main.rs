use clap::{command, Parser, Subcommand};
use common::{
    check_general_prerequisites,
    config::{global_config, init_global_config, GlobalConfig},
    error::log_error,
    init_prompt_theme, logger,
};
use config::EcosystemConfig;
use xshell::Shell;

use crate::commands::{
    args::RunServerArgs, chain::ChainCommands, ecosystem::EcosystemCommands,
    external_node::ExternalNodeCommands, prover::ProverCommands,
};

pub mod accept_ownership;
mod commands;
mod consts;
mod defaults;
pub mod external_node;
mod messages;
mod utils;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Inception {
    #[command(subcommand)]
    command: InceptionSubcommands,
    #[clap(flatten)]
    global: InceptionGlobalArgs,
}

#[derive(Subcommand, Debug)]
pub enum InceptionSubcommands {
    /// Ecosystem related commands
    #[command(subcommand)]
    Ecosystem(EcosystemCommands),
    /// Chain related commands
    #[command(subcommand)]
    Chain(ChainCommands),
    /// Prover related commands
    #[command(subcommand)]
    Prover(ProverCommands),
    /// Run server
    Server(RunServerArgs),
    // Run External Node
    #[command(subcommand)]
    ExternalNode(ExternalNodeCommands),
    /// Run containers for local development
    Containers,
}

#[derive(Parser, Debug)]
#[clap(next_help_heading = "Global options")]
struct InceptionGlobalArgs {
    /// Verbose mode
    #[clap(short, long, global = true)]
    verbose: bool,
    /// Chain to use
    #[clap(long, global = true)]
    chain: Option<String>,
    /// Ignores prerequisites checks
    #[clap(long, global = true)]
    ignore_prerequisites: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    human_panic::setup_panic!();

    init_prompt_theme();

    logger::new_empty_line();
    logger::intro();

    let shell = Shell::new().unwrap();
    let inception_args = Inception::parse();

    init_global_config_inner(&shell, &inception_args.global)?;

    if !global_config().ignore_prerequisites {
        check_general_prerequisites(&shell);
    }

    match run_subcommand(inception_args, &shell).await {
        Ok(_) => {}
        Err(error) => {
            log_error(error);
            std::process::exit(1);
        }
    }
    Ok(())
}

async fn run_subcommand(inception_args: Inception, shell: &Shell) -> anyhow::Result<()> {
    match inception_args.command {
        InceptionSubcommands::Ecosystem(args) => commands::ecosystem::run(shell, args).await?,
        InceptionSubcommands::Chain(args) => commands::chain::run(shell, args).await?,
        InceptionSubcommands::Prover(args) => commands::prover::run(shell, args).await?,
        InceptionSubcommands::Server(args) => commands::server::run(shell, args)?,
        InceptionSubcommands::Containers => commands::containers::run(shell)?,
        InceptionSubcommands::ExternalNode(args) => {
            commands::external_node::run(shell, args).await?
        }
    }
    Ok(())
}

fn init_global_config_inner(
    shell: &Shell,
    inception_args: &InceptionGlobalArgs,
) -> anyhow::Result<()> {
    if let Some(name) = &inception_args.chain {
        if let Ok(config) = EcosystemConfig::from_file(shell) {
            let chains = config.list_of_chains();
            if !chains.contains(name) {
                anyhow::bail!(
                    "Chain with name {} doesnt exist, please choose one of {:?}",
                    name,
                    &chains
                );
            }
        }
    }
    init_global_config(GlobalConfig {
        verbose: inception_args.verbose,
        chain_name: inception_args.chain.clone(),
        ignore_prerequisites: inception_args.ignore_prerequisites,
    });
    Ok(())
}
