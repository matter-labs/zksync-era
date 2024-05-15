use clap::{command, Parser, Subcommand};
use common::{
    check_prerequisites,
    config::{init_global_config, GlobalConfig},
    init_prompt_theme, logger,
};
use xshell::Shell;

use crate::{
    commands::{args::RunServerArgs, ecosystem::EcosystemCommands, hyperchain::HyperchainCommands},
    configs::EcosystemConfig,
};

pub mod accept_ownership;
mod commands;
mod configs;
mod consts;
mod defaults;
pub mod forge_utils;
pub mod server;
mod types;
mod wallets;

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
    /// Hyperchain related commands
    #[command(subcommand)]
    Hyperchain(HyperchainCommands),
    /// Run server
    Server(RunServerArgs),
    /// Run containers for local development
    Containers,
}

#[derive(Parser, Debug)]
#[clap(next_help_heading = "Global options")]
struct InceptionGlobalArgs {
    /// Verbose mode
    #[clap(short, long, global = true)]
    verbose: bool,
    /// Hyperchain to use
    #[clap(long, global = true)]
    hyperchain: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    human_panic::setup_panic!();

    init_prompt_theme();

    logger::new_empty_line();
    logger::intro();

    let shell = Shell::new().unwrap();
    let inception_args = Inception::parse();

    init_global_config_inner(&inception_args.global)?;

    check_prerequisites(&shell);

    match run_subcommand(inception_args, &shell).await {
        Ok(_) => {}
        Err(e) => {
            logger::error(&e.to_string());

            if e.chain().count() > 1 {
                logger::error_note(
                    "Caused by:",
                    &e.chain()
                        .skip(1)
                        .enumerate()
                        .map(|(i, cause)| format!("  {i}: {}", cause))
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
            }

            logger::outro("Failed");
            std::process::exit(1);
        }
    }
    Ok(())
}

async fn run_subcommand(inception_args: Inception, shell: &Shell) -> anyhow::Result<()> {
    match inception_args.command {
        InceptionSubcommands::Ecosystem(args) => commands::ecosystem::run(shell, args).await?,
        InceptionSubcommands::Hyperchain(args) => commands::hyperchain::run(shell, args).await?,
        InceptionSubcommands::Server(args) => commands::server::run(shell, args)?,
        InceptionSubcommands::Containers => commands::containers::run(shell)?,
    }
    Ok(())
}

fn init_global_config_inner(inception_args: &InceptionGlobalArgs) -> anyhow::Result<()> {
    if let Some(name) = &inception_args.hyperchain {
        if let Ok(config) = EcosystemConfig::from_file() {
            let hyperchains = config.list_of_hyperchains();
            if !hyperchains.contains(name) {
                anyhow::bail!(
                    "Hyperchain with name {} doesnt exist, please choose one of {:?}",
                    name,
                    &hyperchains
                );
            }
        }
    }
    init_global_config(GlobalConfig {
        verbose: inception_args.verbose,
        hyperchain_name: inception_args.hyperchain.clone(),
    });
    Ok(())
}
