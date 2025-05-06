use anyhow::Context;
use ethers::utils::hex;
use xshell::Shell;
use zkstack_cli_common::{forge::ForgeScriptArgs, logger, spinner::Spinner};
use zkstack_cli_config::EcosystemConfig;

use crate::{
    admin_functions::{set_da_validator_pair, AdminScriptMode},
    commands::chain::utils::get_default_foundry_path,
    messages::{
        MSG_CHAIN_NOT_INITIALIZED, MSG_DA_VALIDATOR_PAIR_UPDATED_TO,
        MSG_UPDATING_DA_VALIDATOR_PAIR_SPINNER, MSG_WALLETS_CONFIG_MUST_BE_PRESENT,
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

    let spinner = Spinner::new(MSG_UPDATING_DA_VALIDATOR_PAIR_SPINNER);

    set_da_validator_pair(
        shell,
        &args.clone(),
        &get_default_foundry_path(shell)?,
        AdminScriptMode::OnlySave,
        chain_config.chain_id.as_u64(),
        contracts_config.ecosystem_contracts.bridgehub_proxy_addr,
        contracts_config
            .l1
            .no_da_validium_l1_validator_addr
            .context("no_da_validium_l1_validator_addr")?,
        contracts_config
            .l2
            .da_validator_addr
            .context("da_validator_addr")?,
        l1_url,
    )
    .await?;

    spinner.finish();

    logger::note(
        MSG_DA_VALIDATOR_PAIR_UPDATED_TO,
        hex::encode(token_multiplier_setter_address),
    );

    Ok(())
}
