use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::{
    forge::{ForgeArgs, ForgeRunner},
    logger,
    spinner::Spinner,
};
use zkstack_cli_config::{ZkStackConfig, ZkStackConfigTrait};

use crate::{
    admin_functions::accept_admin,
    messages::{
        MSG_ACCEPTING_ADMIN_SPINNER, MSG_CHAIN_NOT_INITIALIZED, MSG_CHAIN_OWNERSHIP_TRANSFERRED,
    },
};

pub async fn run(args: ForgeArgs, shell: &Shell) -> anyhow::Result<()> {
    let chain_config = ZkStackConfig::current_chain(shell).context(MSG_CHAIN_NOT_INITIALIZED)?;
    let contracts = chain_config.get_contracts_config()?;
    let secrets = chain_config.get_secrets_config().await?;
    let l1_rpc_url = secrets.l1_rpc_url()?;

    let spinner = Spinner::new(MSG_ACCEPTING_ADMIN_SPINNER);
    let mut runner = ForgeRunner::new(args.runner);
    accept_admin(
        shell,
        &mut runner,
        chain_config.path_to_foundry_scripts(),
        contracts.l1.chain_admin_addr,
        &chain_config.get_wallets_config()?.governor,
        contracts.l1.diamond_proxy_addr,
        &args.script,
        l1_rpc_url,
    )
    .await?;
    spinner.finish();
    logger::success(MSG_CHAIN_OWNERSHIP_TRANSFERRED);
    Ok(())
}
