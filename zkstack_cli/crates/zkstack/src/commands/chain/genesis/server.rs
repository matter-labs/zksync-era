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

use crate::{
    commands::chain::args::genesis::server::GenesisServerArgs,
    messages::{
        MSG_CHAIN_NOT_INITIALIZED, MSG_FAILED_TO_RUN_SERVER_ERR, MSG_GENESIS_COMPLETED,
        MSG_STARTING_GENESIS_SPINNER,
    },
    utils::{docker::adjust_host_to_execution_mode, ports::EcosystemPortsScanner},
};

pub async fn run(args: GenesisServerArgs, shell: &Shell) -> anyhow::Result<()> {
    let args = args.fill_values_with_prompt().await;

    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;

    let spinner = Spinner::new(MSG_STARTING_GENESIS_SPINNER);
    run_server_genesis(&chain_config, shell, args.mode).await?;
    spinner.finish();
    logger::outro(MSG_GENESIS_COMPLETED);

    Ok(())
}

pub async fn run_server_genesis(
    chain_config: &ChainConfig,
    shell: &Shell,
    execution_mode: ExecutionMode,
) -> anyhow::Result<()> {
    let ports = EcosystemPortsScanner::scan(shell)?;
    let server = Server::new(None, chain_config.link_to_code.clone(), false);

    adjust_host_to_execution_mode(shell, &execution_mode, chain_config)?;

    server
        .run(
            shell,
            execution_mode,
            ServerMode::Genesis,
            GenesisConfig::get_path_with_base_path(&chain_config.configs),
            WalletsConfig::get_path_with_base_path(&chain_config.configs),
            GeneralConfig::get_path_with_base_path(&chain_config.configs),
            SecretsConfig::get_path_with_base_path(&chain_config.configs),
            ContractsConfig::get_path_with_base_path(&chain_config.configs),
            vec![],
            ports.ports.keys().map(|p| p.to_owned()).collect(),
        )
        .await
        .context(MSG_FAILED_TO_RUN_SERVER_ERR)
}
