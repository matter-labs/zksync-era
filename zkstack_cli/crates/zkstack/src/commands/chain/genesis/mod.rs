use clap::{command, Parser, Subcommand};
use common::{logger, spinner::Spinner};
use config::ChainConfig;
use xshell::Shell;

use crate::{
    commands::chain::{
        args::genesis::{GenesisArgs, GenesisArgsFinal},
        genesis::{self, database::initialize_server_database, server::run_server_genesis},
    },
    messages::{
        MSG_GENESIS_COMPLETED, MSG_INITIALIZING_DATABASES_SPINNER, MSG_SELECTED_CONFIG,
        MSG_STARTING_GENESIS, MSG_STARTING_GENESIS_SPINNER,
    },
};

// Genesis subcommands
pub mod database;
pub mod server;

#[derive(Subcommand, Debug, Clone)]
pub enum GenesisSubcommands {
    /// Initialize databases
    #[command(alias = "database")]
    InitDatabase(Box<GenesisArgs>),
    /// Runs server genesis
    Server,
}

#[derive(Parser, Debug)]
#[command()]
pub struct GenesisCommand {
    #[command(subcommand)]
    command: Option<GenesisSubcommands>,
    #[clap(flatten)]
    args: GenesisArgs,
}

pub(crate) async fn run(
    args: GenesisCommand,
    shell: &Shell,
    chain: ChainConfig,
) -> anyhow::Result<()> {
    match args.command {
        Some(GenesisSubcommands::InitDatabase(args)) => database::run(*args, shell, chain).await,
        Some(GenesisSubcommands::Server) => server::run(shell, chain).await,
        None => run_genesis(args.args, shell, chain).await,
    }
}

pub async fn run_genesis(
    args: GenesisArgs,
    shell: &Shell,
    chain: ChainConfig,
) -> anyhow::Result<()> {
    let args = args.fill_values_with_prompt(&chain);

    genesis(args, shell, &chain).await?;
    logger::outro(MSG_GENESIS_COMPLETED);

    Ok(())
}

pub async fn genesis(
    args: GenesisArgsFinal,
    shell: &Shell,
    chain: &ChainConfig,
) -> anyhow::Result<()> {
    genesis::database::update_configs(args.clone(), shell, chain)?;

    logger::note(
        MSG_SELECTED_CONFIG,
        logger::object_to_string(serde_json::json!({
            "chain_config": chain,
            "server_db_config": args.server_db,
        })),
    );
    logger::info(MSG_STARTING_GENESIS);

    let spinner = Spinner::new(MSG_INITIALIZING_DATABASES_SPINNER);
    initialize_server_database(
        shell,
        &args.server_db,
        chain.link_to_code.clone(),
        args.dont_drop,
    )
    .await?;
    spinner.finish();

    let spinner = Spinner::new(MSG_STARTING_GENESIS_SPINNER);
    run_server_genesis(chain, shell)?;
    spinner.finish();

    Ok(())
}
