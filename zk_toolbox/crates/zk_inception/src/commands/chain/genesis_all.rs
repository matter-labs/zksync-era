use anyhow::Context;
use common::{config::global_config, logger, spinner::Spinner};
use config::{ChainConfig, EcosystemConfig};
use xshell::Shell;

use super::{args::genesis::GenesisArgsFinal, genesis};
use crate::{
    commands::chain::{
        args::genesis::GenesisArgs,
        genesis::{database::initialize_databases, server::run_server_genesis},
    },
    messages::{
        MSG_CHAIN_NOT_INITIALIZED, MSG_GENESIS_COMPLETED, MSG_INITIALIZING_DATABASES_SPINNER,
        MSG_SELECTED_CONFIG, MSG_STARTING_GENESIS, MSG_STARTING_GENESIS_SPINNER,
    },
};

pub async fn run(args: GenesisArgs, shell: &Shell) -> anyhow::Result<()> {
    let chain_name = global_config().chain_name.clone();
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_chain(chain_name)
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
    genesis::database::update_configs(args.clone(), shell, config)?;

    logger::note(
        MSG_SELECTED_CONFIG,
        logger::object_to_string(serde_json::json!({
            "chain_config": config,
            "server_db_config": args.server_db,
            "prover_db_config": args.prover_db,
        })),
    );
    logger::info(MSG_STARTING_GENESIS);

    let spinner = Spinner::new(MSG_INITIALIZING_DATABASES_SPINNER);
    initialize_databases(
        shell,
        &args.server_db,
        &args.prover_db,
        config.link_to_code.clone(),
        args.dont_drop,
    )
    .await?;
    spinner.finish();

    let spinner = Spinner::new(MSG_STARTING_GENESIS_SPINNER);
    run_server_genesis(config, shell)?;
    spinner.finish();

    Ok(())
}
