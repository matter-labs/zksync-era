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
    let general_config = chain_config.get_general_config()?;
    let api_config = general_config.api_config.context("Missing API config")?;

    let rpc_port = api_config.web3_json_rpc.http_port.to_string();
    let healthcheck_port = api_config.healthcheck.port.to_string();

    let server = Server::new(None, chain_config.link_to_code.clone(), false);
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
            rpc_port,
            healthcheck_port,
        )
        .await
        .context(MSG_FAILED_TO_RUN_SERVER_ERR)
}
