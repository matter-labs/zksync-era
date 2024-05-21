use anyhow::Context;
use common::{config::global_config, logger};
use xshell::Shell;

use crate::{
    commands::args::RunServerArgs,
    configs::{EcosystemConfig, HyperchainConfig},
    server::{RunServer, ServerMode},
};

pub fn run(shell: &Shell, args: RunServerArgs) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    let hyperchain = global_config().hyperchain_name.clone();
    let hyperchain_config = ecosystem_config
        .load_hyperchain(hyperchain)
        .context("Hyperchain not initialized. Please create a hyperchain first")?;

    logger::info("Starting server");
    run_server(args, &hyperchain_config, shell)?;

    Ok(())
}

fn run_server(
    args: RunServerArgs,
    hyperchain_config: &HyperchainConfig,
    shell: &Shell,
) -> anyhow::Result<()> {
    let server = RunServer::new(args.components.clone(), hyperchain_config);
    let mode = if args.genesis {
        ServerMode::Genesis
    } else {
        ServerMode::Normal
    };
    server.run(shell, mode)
}
