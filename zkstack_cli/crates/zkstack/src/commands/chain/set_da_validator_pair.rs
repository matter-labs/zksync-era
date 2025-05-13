use anyhow::Context;
use ethers::utils::hex;
use xshell::Shell;
use zkstack_cli_common::{forge::ForgeScriptArgs, logger, spinner::Spinner};
use zkstack_cli_config::EcosystemConfig;

use crate::{
    admin_functions::{set_da_validator_pair, AdminScriptMode},
    commands::chain::{
        gateway::gateway_common::get_settlement_layer_address_from_l1,
        utils::get_default_foundry_path,
    },
    messages::{
        MSG_CHAIN_NOT_INITIALIZED, MSG_DA_VALIDATOR_PAIR_UPDATED_TO,
        MSG_UPDATING_DA_VALIDATOR_PAIR_SPINNER,
    },
};

pub async fn run(args: ForgeScriptArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    let contracts_config = chain_config.get_contracts_config()?;
    let gateway_url = chain_config.get_secrets_config().await?.gateway_rpc_url();

    let mut diamond_proxy_address = contracts_config.ecosystem_contracts.bridgehub_proxy_addr;
    let mut l1_url = chain_config.get_secrets_config().await?.l1_rpc_url()?;
    if gateway_url.is_ok() {
        diamond_proxy_address =
            get_settlement_layer_address_from_l1(l1_url, diamond_proxy_address).await?;
        l1_url = gateway_url?;
    }

    let spinner = Spinner::new(MSG_UPDATING_DA_VALIDATOR_PAIR_SPINNER);

    let l1_da_validator_address = contracts_config
        .l1
        .no_da_validium_l1_validator_addr
        .context("no_da_validium_l1_validator_addr")?;
    let l2_da_validator_address = contracts_config
        .l2
        .da_validator_addr
        .context("da_validator_addr")?;

    set_da_validator_pair(
        shell,
        &args.clone(),
        &get_default_foundry_path(shell)?,
        AdminScriptMode::Broadcast(chain_config.get_wallets_config()?.governor),
        chain_config.chain_id.as_u64(),
        diamond_proxy_address,
        l1_da_validator_address,
        l2_da_validator_address,
        l1_url,
    )
    .await?;

    spinner.finish();

    logger::note(
        MSG_DA_VALIDATOR_PAIR_UPDATED_TO,
        format!(
            "{} {}",
            hex::encode(l1_da_validator_address),
            hex::encode(l2_da_validator_address)
        ),
    );

    Ok(())
}
