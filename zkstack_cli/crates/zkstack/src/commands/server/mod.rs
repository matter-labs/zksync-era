use anyhow::Context;
use build::build_server;
use config::EcosystemConfig;
use run::run_server;
use wait::wait_for_server;
use xshell::Shell;

use crate::{
    commands::args::{ServerArgs, ServerCommand},
    messages::MSG_CHAIN_NOT_INITIALIZED,
};

mod build;
mod run;
mod wait;

pub async fn run(shell: &Shell, args: ServerArgs) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;

    match ServerCommand::from(args) {
        ServerCommand::Run(args) => run_server(args, &chain_config, shell).await,
        ServerCommand::Build => build_server(&chain_config, shell),
        ServerCommand::Wait(args) => wait_for_server(args, &chain_config).await,
    }
}
