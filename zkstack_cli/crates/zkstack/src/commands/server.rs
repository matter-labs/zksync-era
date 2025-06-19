use anyhow::Context;
use xshell::{cmd, Shell};
use zkstack_cli_common::{
    cmd::Cmd,
    config::global_config,
    logger,
    server::{Server, ServerMode},
};
use zkstack_cli_config::{
    traits::FileConfigWithDefaultName, ChainConfig, ContractsConfig, EcosystemConfig,
    WalletsConfig, GENERAL_FILE, GENESIS_FILE, SECRETS_FILE,
};

use crate::{
    commands::args::{RunServerArgs, ServerArgs, ServerCommand, WaitArgs},
    messages::{
        msg_waiting_for_server_success, MSG_BUILDING_SERVER, MSG_CHAIN_NOT_INITIALIZED,
        MSG_FAILED_TO_BUILD_SERVER_ERR, MSG_FAILED_TO_RUN_SERVER_ERR, MSG_STARTING_SERVER,
        MSG_WAITING_FOR_SERVER,
    },
};

pub async fn run(shell: &Shell, args: ServerArgs) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;

    match ServerCommand::from(args) {
        ServerCommand::Run(args) => run_server(args, &chain_config, shell).await,
        ServerCommand::Build => build_server(&chain_config, shell),
        ServerCommand::Wait(args) => wait_for_server(args, &chain_config).await,
    }
}

fn build_server(chain_config: &ChainConfig, shell: &Shell) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(chain_config.link_to_code.join("core"));

    logger::info(MSG_BUILDING_SERVER);

    let mut cmd = Cmd::new(cmd!(shell, "cargo build --release --bin zksync_server"));
    cmd = cmd.with_force_run();
    cmd.run().context(MSG_FAILED_TO_BUILD_SERVER_ERR)
}

async fn run_server(
    args: RunServerArgs,
    chain_config: &ChainConfig,
    shell: &Shell,
) -> anyhow::Result<()> {
    logger::info(MSG_STARTING_SERVER);
    let server = Server::new(
        args.server_command,
        args.components.clone(),
        chain_config.link_to_code.clone(),
        args.uring,
        // enable zkOS by default
        !args.no_zkos,
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
            chain_config.configs.join(GENESIS_FILE),
            WalletsConfig::get_path_with_base_path(&chain_config.configs),
            chain_config.configs.join(GENERAL_FILE),
            chain_config.configs.join(SECRETS_FILE),
            ContractsConfig::get_path_with_base_path(&chain_config.configs),
            vec![],
        )
        .context(MSG_FAILED_TO_RUN_SERVER_ERR)
}

async fn wait_for_server(args: WaitArgs, chain_config: &ChainConfig) -> anyhow::Result<()> {
    let verbose = global_config().verbose;

    let health_check_url = chain_config.get_general_config().await?.healthcheck_url()?;
    logger::info(MSG_WAITING_FOR_SERVER);
    args.poll_health_check(&health_check_url, verbose).await?;
    logger::info(msg_waiting_for_server_success(&health_check_url));
    Ok(())
}
