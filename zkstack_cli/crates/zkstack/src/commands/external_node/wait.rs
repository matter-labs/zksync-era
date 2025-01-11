use anyhow::Context as _;
use xshell::Shell;
use zkstack_cli_common::{config::global_config, logger};
use zkstack_cli_config::{traits::ReadConfigWithBasePath, EcosystemConfig};
use zksync_config::configs::GeneralConfig;

use crate::{
    commands::args::WaitArgs,
    messages::{msg_waiting_for_en_success, MSG_CHAIN_NOT_INITIALIZED, MSG_WAITING_FOR_EN},
};

pub async fn wait(shell: &Shell, args: WaitArgs) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    let verbose = global_config().verbose;

    let en_path = chain_config
        .external_node_config_path
        .clone()
        .context("External node is not initialized")?;
    let general_config = GeneralConfig::read_with_base_path(shell, &en_path)?;
    let health_check_port = general_config
        .api_config
        .as_ref()
        .context("no API config")?
        .healthcheck
        .port;

    logger::info(MSG_WAITING_FOR_EN);
    args.poll_health_check(health_check_port, verbose).await?;
    logger::info(msg_waiting_for_en_success(health_check_port));
    Ok(())
}
