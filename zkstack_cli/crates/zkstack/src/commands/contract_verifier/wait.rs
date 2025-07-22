use anyhow::Context as _;
use xshell::Shell;
use zkstack_cli_common::{config::global_config, logger};
use zkstack_cli_config::EcosystemConfig;

use crate::{commands::args::WaitArgs, messages::MSG_CHAIN_NOT_FOUND_ERR};

pub(crate) async fn wait(shell: &Shell, args: WaitArgs) -> anyhow::Result<()> {
    let ecosystem = EcosystemConfig::from_file(shell)?;
    let chain = ecosystem
        .load_current_chain()
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;
    let verbose = global_config().verbose;

    let prometheus_port = chain
        .get_general_config()
        .await?
        .contract_verifier_prometheus_port()?;
    logger::info("Waiting for contract verifier to become alive");
    args.poll_prometheus(prometheus_port, verbose).await?;
    logger::info(format!(
        "Contract verifier is alive with Prometheus server bound to :{prometheus_port}"
    ));
    Ok(())
}
