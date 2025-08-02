use xshell::Shell;
use zkstack_cli_common::{config::global_config, logger};
use zkstack_cli_config::ZkStackConfig;

use crate::commands::args::WaitArgs;

pub(crate) async fn wait(shell: &Shell, args: WaitArgs) -> anyhow::Result<()> {
    let chain = ZkStackConfig::current_chain(shell)?;
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
