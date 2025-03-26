use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::{forge::ForgeScriptArgs, logger, spinner::Spinner};
use zkstack_cli_config::EcosystemConfig;

use crate::{
    accept_ownership::set_da_validator_pair,
    commands::chain::init::get_l1_da_validator,
    messages::{
        MSG_CHAIN_NOT_INITIALIZED, MSG_DA_VALIDATOR_PAIR_SET_TO, MSG_SETTING_DA_VALIDATOR_PAIR,
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

    let l1_da_validator_addr = get_l1_da_validator(&chain_config)
        .await
        .context("l1_da_validator_addr")?;

    let spinner = Spinner::new(MSG_SETTING_DA_VALIDATOR_PAIR);
    set_da_validator_pair(
        shell,
        &ecosystem_config,
        contracts_config.l1.chain_admin_addr,
        &chain_config.get_wallets_config()?.governor,
        contracts_config.l1.diamond_proxy_addr,
        l1_da_validator_addr,
        contracts_config
            .l2
            .da_validator_addr
            .context("da_validator_addr")?,
        &args.clone(),
        l1_url,
    )
    .await?;
    spinner.finish();

    logger::note(
        MSG_DA_VALIDATOR_PAIR_SET_TO,
        format!(
            "l1_da_validator: {:?}, l2_da_validator: {:?}",
            l1_da_validator_addr,
            contracts_config
                .l2
                .da_validator_addr
                .context("da_validator_addr")?
        ),
    );

    Ok(())
}
