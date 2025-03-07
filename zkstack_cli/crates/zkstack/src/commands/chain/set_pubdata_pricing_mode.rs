use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::{forge::ForgeScriptArgs, logger, spinner::Spinner};
use zkstack_cli_config::EcosystemConfig;

use crate::{
    accept_ownership::set_pubdata_pricing_mode,
    messages::{
        MSG_CHAIN_NOT_INITIALIZED, MSG_PUBDATA_PRICING_MODE_SET_TO,
        MSG_SETTING_PUBDATA_PRICING_MODE,
    },
};

pub async fn run(args: ForgeScriptArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    let contracts_config = chain_config.get_contracts_config()?;
    let l1_url = chain_config
        .get_secrets_config()
        .await?
        .get("l1.l1_rpc_url")?;

    let pubdata_pricing_mode = chain_config
        .get_genesis_config()
        .await?
        .get("l1_batch_commit_data_generator_mode")?;

    let spinner = Spinner::new(MSG_SETTING_PUBDATA_PRICING_MODE);
    set_pubdata_pricing_mode(
        shell,
        &ecosystem_config,
        pubdata_pricing_mode,
        contracts_config.l1.chain_admin_addr,
        contracts_config.l1.diamond_proxy_addr,
        &chain_config.get_wallets_config()?.governor,
        &args.clone(),
        l1_url,
    )
    .await?;
    spinner.finish();

    logger::note(MSG_PUBDATA_PRICING_MODE_SET_TO, pubdata_pricing_mode);

    Ok(())
}
