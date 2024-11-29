use anyhow::Context;
use common::{
    logger,
    server::{Server, ServerMode},
};
use config::{
    traits::FileConfigWithDefaultName, ChainConfig, ContractsConfig, EcosystemConfig,
    GeneralConfig, GenesisConfig, SecretsConfig, WalletsConfig,
};
use xshell::Shell;

use crate::{
    commands::args::run::RunServerArgs,
    messages::{MSG_CHAIN_NOT_FOUND_ERR, MSG_FAILED_TO_RUN_SERVER_ERR, MSG_STARTING_SERVER},
};

pub(super) async fn run_server(
    args: RunServerArgs,
    chain_config: &ChainConfig,
    shell: &Shell,
) -> anyhow::Result<()> {
    let args = args.fill_values_with_prompt().await;

    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain = ecosystem_config
        .load_current_chain()
        .expect(MSG_CHAIN_NOT_FOUND_ERR);
    let general_config = chain.get_general_config()?;
    let api_config = general_config.api_config.context("Missing API config")?;

    let rpc_port = api_config.web3_json_rpc.http_port.to_string();
    let healthcheck_port = api_config.healthcheck.port.to_string();

    logger::info(MSG_STARTING_SERVER);
    let server = Server::new(
        args.components.clone(),
        chain_config.link_to_code.clone(),
        args.uring,
    );

    let server_mode = if args.genesis {
        ServerMode::Genesis
    } else {
        ServerMode::Normal
    };

    server
        .run(
            shell,
            args.mode,
            server_mode,
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
