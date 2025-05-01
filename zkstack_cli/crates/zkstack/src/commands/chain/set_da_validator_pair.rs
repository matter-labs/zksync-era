use anyhow::Context;
use ethers::utils::hex;
use xshell::Shell;
use zkstack_cli_common::{forge::ForgeScriptArgs, logger, spinner::Spinner};
use zkstack_cli_config::EcosystemConfig;

use crate::{
    admin_functions::set_da_validator_pair,
    messages::{
        MSG_CHAIN_NOT_INITIALIZED, MSG_TOKEN_MULTIPLIER_SETTER_UPDATED_TO,
        MSG_UPDATING_TOKEN_MULTIPLIER_SETTER_SPINNER, MSG_WALLETS_CONFIG_MUST_BE_PRESENT,
        MSG_WALLET_TOKEN_MULTIPLIER_SETTER_NOT_FOUND,
    },
};

pub async fn run(args: ForgeScriptArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    let contracts_config = chain_config.get_contracts_config()?;
    let l1_url = chain_config.get_secrets_config().await?.l1_rpc_url()?;
    let token_multiplier_setter_address = chain_config
        .get_wallets_config()
        .context(MSG_WALLETS_CONFIG_MUST_BE_PRESENT)?
        .token_multiplier_setter
        .context(MSG_WALLET_TOKEN_MULTIPLIER_SETTER_NOT_FOUND)?
        .address;

    let governor = &chain_config.get_wallets_config()?.governor;

    let spinner = Spinner::new(MSG_UPDATING_TOKEN_MULTIPLIER_SETTER_SPINNER);

    set_da_validator_pair(
        shell,
        &ecosystem_config,
        contracts_config.l1.chain_admin_addr,
        governor,
        contracts_config.l1.diamond_proxy_addr,
        contracts_config
            .l1
            .no_da_validium_l1_validator_addr
            .context("no_da_validium_l1_validator_addr")?,
        contracts_config
            .l2
            .da_validator_addr
            .context("da_validator_addr")?,
        &args,
        l1_url.clone(),
    )
    .await?;

    spinner.finish();

    logger::note(
        MSG_TOKEN_MULTIPLIER_SETTER_UPDATED_TO,
        hex::encode(token_multiplier_setter_address),
    );

    Ok(())
}
