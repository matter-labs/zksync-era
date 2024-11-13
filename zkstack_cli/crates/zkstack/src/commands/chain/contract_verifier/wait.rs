use anyhow::Context as _;
use common::{config::global_config, logger};
use config::ChainConfig;

use crate::commands::args::WaitArgs;

pub(crate) async fn wait(args: WaitArgs, chain: ChainConfig) -> anyhow::Result<()> {
    let verbose = global_config().verbose;

    let prometheus_port = chain
        .get_general_config()?
        .contract_verifier
        .as_ref()
        .context("contract verifier config not specified")?
        .prometheus_port;
    logger::info("Waiting for contract verifier to become alive");
    args.poll_prometheus(prometheus_port, verbose).await?;
    logger::info(format!(
        "Contract verifier is alive with Prometheus server bound to :{prometheus_port}"
    ));
    Ok(())
}
