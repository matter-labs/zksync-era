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
use zksync_config::configs::gateway::GatewayChainConfig;

use crate::{
    commands::args::RunServerArgs,
    messages::{MSG_CHAIN_NOT_INITIALIZED, MSG_FAILED_TO_RUN_SERVER_ERR, MSG_STARTING_SERVER},
};

pub fn run(shell: &Shell, args: RunServerArgs) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;

    logger::info(MSG_STARTING_SERVER);

    run_server(args, &chain_config, shell)?;

    Ok(())
}

fn run_server(
    args: RunServerArgs,
    chain_config: &ChainConfig,
    shell: &Shell,
) -> anyhow::Result<()> {
    let server = Server::new(
        args.components.clone(),
        chain_config.link_to_code.clone(),
        args.uring,
    );

    if args.build {
        server.build(shell)?;
        return Ok(());
    }

    let mode = if args.genesis {
        ServerMode::Genesis
    } else {
        ServerMode::Normal
    };

    let gateway_config = chain_config.get_gateway_chain_config().ok();
    let mut gateway_contracts = None;
    if let Some(gateway_config) = gateway_config {
        gateway_contracts = if gateway_config.current_settlement_layer != 0_u64 {
            Some(GatewayChainConfig::get_path_with_base_path(
                &chain_config.configs,
            ))
        } else {
            None
        };
    }

    server
        .run(
            shell,
            mode,
            GenesisConfig::get_path_with_base_path(&chain_config.configs),
            WalletsConfig::get_path_with_base_path(&chain_config.configs),
            GeneralConfig::get_path_with_base_path(&chain_config.configs),
            SecretsConfig::get_path_with_base_path(&chain_config.configs),
            ContractsConfig::get_path_with_base_path(&chain_config.configs),
            gateway_contracts,
            vec![],
        )
        .context(MSG_FAILED_TO_RUN_SERVER_ERR)
}
