use anyhow::Context;
use common::{
    logger,
    server::{Server, ServerMode},
};
use config::{
    traits::FileConfigWithDefaultName, ChainConfig, ContractsConfig, GeneralConfig, GenesisConfig,
    SecretsConfig, WalletsConfig,
};
use xshell::Shell;

use crate::{
    commands::args::run::RunServerArgs,
    messages::{MSG_FAILED_TO_RUN_SERVER_ERR, MSG_STARTING_SERVER},
};

pub(super) fn run_server(
    args: RunServerArgs,
    chain_config: &ChainConfig,
    shell: &Shell,
) -> anyhow::Result<()> {
    logger::info(MSG_STARTING_SERVER);
    let server = Server::new(
        args.components.clone(),
        chain_config.link_to_code.clone(),
        args.uring,
    );

    let mode = if args.genesis {
        ServerMode::Genesis
    } else {
        ServerMode::Normal
    };
    server
        .run(
            shell,
            mode,
            GenesisConfig::get_path_with_base_path(&chain_config.configs),
            WalletsConfig::get_path_with_base_path(&chain_config.configs),
            GeneralConfig::get_path_with_base_path(&chain_config.configs),
            SecretsConfig::get_path_with_base_path(&chain_config.configs),
            ContractsConfig::get_path_with_base_path(&chain_config.configs),
            vec![],
        )
        .context(MSG_FAILED_TO_RUN_SERVER_ERR)
}
