use anyhow::Context;
use common::{config::global_config, logger};
use config::ChainConfig;

use crate::{
    commands::args::WaitArgs,
    messages::{msg_waiting_for_server_success, MSG_WAITING_FOR_SERVER},
};

pub(super) async fn wait_for_server(
    args: WaitArgs,
    chain_config: &ChainConfig,
) -> anyhow::Result<()> {
    let verbose = global_config().verbose;

    let health_check_port = chain_config
        .get_general_config()?
        .api_config
        .as_ref()
        .context("no API config")?
        .healthcheck
        .port;

    logger::info(MSG_WAITING_FOR_SERVER);
    args.poll_health_check(health_check_port, verbose).await?;
    logger::info(msg_waiting_for_server_success(health_check_port));
    Ok(())
}
