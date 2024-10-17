use anyhow::Context;
use common::{
    logger,
    server::{Server, ServerMode},
    spinner::Spinner,
};
use config::{
    traits::FileConfigWithDefaultName, zkstack_config::ZkStackConfig, ChainConfig, ContractsConfig,
    GeneralConfig, GenesisConfig, SecretsConfig, WalletsConfig,
};
use xshell::Shell;

use crate::messages::{
    MSG_FAILED_TO_RUN_SERVER_ERR, MSG_GENESIS_COMPLETED, MSG_STARTING_GENESIS_SPINNER,
};

pub async fn run(shell: &Shell) -> anyhow::Result<()> {
    let chain_config = ZkStackConfig::load_current_chain(shell)?;

    let spinner = Spinner::new(MSG_STARTING_GENESIS_SPINNER);
    run_server_genesis(&chain_config, shell)?;
    spinner.finish();
    logger::outro(MSG_GENESIS_COMPLETED);

    Ok(())
}

pub fn run_server_genesis(chain_config: &ChainConfig, shell: &Shell) -> anyhow::Result<()> {
    let server = Server::new(None, chain_config.link_to_code.clone(), false);
    server
        .run(
            shell,
            ServerMode::Genesis,
            GenesisConfig::get_path_with_base_path(&chain_config.configs),
            WalletsConfig::get_path_with_base_path(&chain_config.configs),
            GeneralConfig::get_path_with_base_path(&chain_config.configs),
            SecretsConfig::get_path_with_base_path(&chain_config.configs),
            ContractsConfig::get_path_with_base_path(&chain_config.configs),
            vec![],
        )
        .context(MSG_FAILED_TO_RUN_SERVER_ERR)
}
