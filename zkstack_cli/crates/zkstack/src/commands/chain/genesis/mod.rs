use anyhow::Context;
use clap::{command, Parser, Subcommand};
use xshell::Shell;
use zkstack_cli_common::{logger, spinner::Spinner};
use zkstack_cli_config::{ChainConfig, EcosystemConfig};

use crate::{
    commands::chain::{
        args::genesis::{GenesisArgs, GenesisArgsFinal},
        genesis::{database::initialize_server_database, server::run_server_genesis},
    },
    messages::{
        MSG_CHAIN_NOT_INITIALIZED, MSG_GENESIS_COMPLETED, MSG_INITIALIZING_DATABASES_SPINNER,
        MSG_SELECTED_CONFIG, MSG_STARTING_GENESIS, MSG_STARTING_GENESIS_SPINNER,
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

pub(crate) async fn run(args: GenesisCommand, shell: &Shell) -> anyhow::Result<()> {
    match args.command {
        Some(GenesisSubcommands::InitDatabase(args)) => database::run(*args, shell).await,
        Some(GenesisSubcommands::Server) => server::run(args.args.server_command, shell).await,
        None => run_genesis(args.args, shell).await,
    }
}

pub async fn run_genesis(args: GenesisArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    let args = args.fill_values_with_prompt(&chain_config);

    genesis(args, shell, &chain_config).await?;
    logger::outro(MSG_GENESIS_COMPLETED);

    Ok(())
}

pub async fn genesis(
    args: GenesisArgsFinal,
    shell: &Shell,
    config: &ChainConfig,
) -> anyhow::Result<()> {
    let override_validium_config = true;
    database::update_configs(args.clone(), shell, config, override_validium_config).await?;

    logger::note(
        MSG_SELECTED_CONFIG,
        logger::object_to_string(serde_json::json!({
            "chain_config": config,
            "server_db_config": args.server_db,
        })),
    );
    logger::info(MSG_STARTING_GENESIS);

    let spinner = Spinner::new(MSG_INITIALIZING_DATABASES_SPINNER);
    initialize_server_database(shell, &args.server_db, &config.link_to_code, args.dont_drop)
        .await?;
    spinner.finish();

    let spinner = Spinner::new(MSG_STARTING_GENESIS_SPINNER);
    run_server_genesis(args.server_command, config, shell)?;
    spinner.finish();

    Ok(())
}
