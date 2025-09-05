use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::{forge::ForgeScriptArgs, logger, spinner::Spinner};
use zkstack_cli_config::EcosystemConfig;

use crate::{
    admin_functions::{accept_admin, AdminScriptMode},
    messages::{
        MSG_ACCEPTING_ADMIN_SPINNER, MSG_CHAIN_NOT_INITIALIZED, MSG_CHAIN_OWNERSHIP_TRANSFERRED,
    },
};

pub async fn run(args: ForgeScriptArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    let contracts = chain_config.get_contracts_config()?;
    let secrets = chain_config.get_secrets_config().await?;
    let l1_rpc_url = secrets.l1_rpc_url()?;

    let spinner = Spinner::new(MSG_ACCEPTING_ADMIN_SPINNER);
    accept_admin(
        shell,
        &ecosystem_config,
        contracts.l1.chain_admin_addr,
        &chain_config.get_wallets_config()?.governor,
        contracts.l1.diamond_proxy_addr,
        &args,
        l1_rpc_url,
        AdminScriptMode::Broadcast(chain_config.get_wallets_config()?.governor),
    )
    .await?;
    spinner.finish();
    logger::success(MSG_CHAIN_OWNERSHIP_TRANSFERRED);
    Ok(())
}
