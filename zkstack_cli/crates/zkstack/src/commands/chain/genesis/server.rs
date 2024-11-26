use anyhow::Context;
use common::{
    logger,
    server::{ExecutionMode, Server, ServerMode},
    spinner::Spinner,
};
use config::{
    traits::FileConfigWithDefaultName, ChainConfig, ContractsConfig, EcosystemConfig,
    GeneralConfig, GenesisConfig, SecretsConfig, WalletsConfig,
};
use xshell::Shell;

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
    run_server_genesis(&chain_config, shell).await?;
    spinner.finish();
    logger::outro(MSG_GENESIS_COMPLETED);

    Ok(())
}

pub async fn run_server_genesis(chain_config: &ChainConfig, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let server = Server::new(None, chain_config.link_to_code.clone(), false);
    server
        .run(
            shell,
            ExecutionMode::default(),
            ServerMode::Genesis,
            ecosystem_config.chains,
            GenesisConfig::get_path_with_base_path(&chain_config.configs),
            WalletsConfig::get_path_with_base_path(&chain_config.configs),
            GeneralConfig::get_path_with_base_path(&chain_config.configs),
            SecretsConfig::get_path_with_base_path(&chain_config.configs),
            ContractsConfig::get_path_with_base_path(&chain_config.configs),
            vec![],
            None, // TODO: add support for docker
        )
        .await
        .context(MSG_FAILED_TO_RUN_SERVER_ERR)
}
