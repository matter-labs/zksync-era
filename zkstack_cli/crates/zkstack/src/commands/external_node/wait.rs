use anyhow::Context as _;
use xshell::Shell;
use zkstack_cli_common::{config::global_config, logger};
use zkstack_cli_config::{EcosystemConfig, GeneralConfig, GENERAL_FILE};

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
    let general_config = GeneralConfig::read(shell, en_path.join(GENERAL_FILE)).await?;
    let health_check_url = general_config.healthcheck_url()?;

    logger::info(MSG_WAITING_FOR_EN);
    args.poll_health_check(&health_check_url, verbose).await?;
    logger::info(msg_waiting_for_en_success(&health_check_url));
    Ok(())
}
