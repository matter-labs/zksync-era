use anyhow::Context;
use common::{config::global_config, logger};
use config::{ChainConfig, EcosystemConfig};
use xshell::Shell;

use crate::{
    commands::args::RunServerArgs,
    messages::{MSG_CHAIN_NOT_INITIALIZED, MSG_STARTING_SERVER},
    server::{RunServer, ServerMode},
};

pub fn run(shell: &Shell, args: RunServerArgs) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    let chain = global_config().chain_name.clone();
    let chain_config = ecosystem_config
        .load_chain(chain)
        .context(MSG_CHAIN_NOT_INITIALIZED)?;

    logger::info(MSG_STARTING_SERVER);

    run_server(args, &chain_config, shell)?;

    Ok(())
}

fn run_server(
    args: RunServerArgs,
    chain_config: &ChainConfig,
    shell: &Shell,
) -> anyhow::Result<()> {
    let server = RunServer::new(args.components.clone(), chain_config);
    let mode = if args.genesis {
        ServerMode::Genesis
    } else {
        ServerMode::Normal
    };
    server.run(shell, mode, args.additional_args)
}
