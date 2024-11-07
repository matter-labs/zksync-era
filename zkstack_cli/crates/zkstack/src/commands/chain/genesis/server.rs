use anyhow::Context;
use common::{
    logger,
    server::{Server, ServerMode},
    spinner::Spinner,
};
use config::{
    traits::FileConfigWithDefaultName, ChainConfig, ContractsConfig, GeneralConfig, GenesisConfig,
    SecretsConfig, WalletsConfig,
};
use xshell::Shell;

use crate::messages::{
    MSG_FAILED_TO_RUN_SERVER_ERR, MSG_GENESIS_COMPLETED, MSG_STARTING_GENESIS_SPINNER,
};

pub async fn run(shell: &Shell, chain: ChainConfig) -> anyhow::Result<()> {
    let spinner = Spinner::new(MSG_STARTING_GENESIS_SPINNER);
    run_server_genesis(&chain, shell)?;
    spinner.finish();
    logger::outro(MSG_GENESIS_COMPLETED);

    Ok(())
}

pub fn run_server_genesis(chain: &ChainConfig, shell: &Shell) -> anyhow::Result<()> {
    let server = Server::new(None, chain.link_to_code.clone(), false);
    server
        .run(
            shell,
            ServerMode::Genesis,
            GenesisConfig::get_path_with_base_path(&chain.configs),
            WalletsConfig::get_path_with_base_path(&chain.configs),
            GeneralConfig::get_path_with_base_path(&chain.configs),
            SecretsConfig::get_path_with_base_path(&chain.configs),
            ContractsConfig::get_path_with_base_path(&chain.configs),
            vec![],
        )
        .context(MSG_FAILED_TO_RUN_SERVER_ERR)
}
