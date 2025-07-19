use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::{
    logger,
    server::{Server, ServerMode},
    spinner::Spinner,
};
use zkstack_cli_config::{
    traits::FileConfigWithDefaultName, ChainConfig, ContractsConfig, WalletsConfig, ZkStackConfig,
    GENERAL_FILE, GENESIS_FILE, SECRETS_FILE,
};

use crate::messages::{
    MSG_FAILED_TO_RUN_SERVER_ERR, MSG_GENESIS_COMPLETED, MSG_STARTING_GENESIS_SPINNER,
};

pub async fn run(server_command: Option<String>, shell: &Shell) -> anyhow::Result<()> {
    let chain_config = ZkStackConfig::current_chain(shell)?;

    let spinner = Spinner::new(MSG_STARTING_GENESIS_SPINNER);
    run_server_genesis(server_command, &chain_config, shell)?;
    spinner.finish();
    logger::outro(MSG_GENESIS_COMPLETED);

    Ok(())
}

pub fn run_server_genesis(
    server_command: Option<String>,
    chain_config: &ChainConfig,
    shell: &Shell,
) -> anyhow::Result<()> {
    let server = Server::new(
        server_command,
        None,
        chain_config.link_to_code.clone(),
        false,
    );
    server
        .run(
            shell,
            ServerMode::Genesis,
            chain_config.configs.join(GENESIS_FILE),
            WalletsConfig::get_path_with_base_path(&chain_config.configs),
            chain_config.configs.join(GENERAL_FILE),
            chain_config.configs.join(SECRETS_FILE),
            ContractsConfig::get_path_with_base_path(&chain_config.configs),
            vec![],
        )
        .context(MSG_FAILED_TO_RUN_SERVER_ERR)
}
