use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::{
    logger,
    server::{Server, ServerMode},
    spinner::Spinner,
};
use zkstack_cli_config::{
    traits::FileConfigWithDefaultName, ChainConfig, ContractsConfig, EcosystemConfig,
    GeneralConfig, GenesisConfig, SecretsConfig, WalletsConfig,
};

use crate::messages::{
    MSG_CHAIN_NOT_INITIALIZED, MSG_FAILED_TO_RUN_SERVER_ERR, MSG_GENESIS_COMPLETED,
    MSG_STARTING_GENESIS_SPINNER,
};

pub async fn run(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;

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
            None,
            vec![],
        )
        .context(MSG_FAILED_TO_RUN_SERVER_ERR)
}
