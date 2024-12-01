use anyhow::Context as _;
use common::{config::global_config, logger};
use config::{traits::ReadConfigWithBasePath, zkstack_config::ZkStackConfig};
use xshell::Shell;
use zksync_config::configs::GeneralConfig;

use crate::{
    commands::args::WaitArgs,
    messages::{msg_waiting_for_en_success, MSG_WAITING_FOR_EN},
};

pub async fn wait(shell: &Shell, args: WaitArgs) -> anyhow::Result<()> {
    let chain_config = ZkStackConfig::current_chain(shell)?;
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
