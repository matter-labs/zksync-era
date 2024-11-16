use anyhow::Context;
use common::{ethereum, spinner::Spinner, wallets::Wallet};
use config::{ChainConfig, EcosystemConfig};
use ethers::abi::Address;
use xshell::Shell;

use crate::{
    commands::chain::args::propose_registration::ProposeRegistrationArgs,
    messages::{MSG_CHAIN_NOT_INITIALIZED, MSG_REGISTERING_CHAIN_SPINNER},
};

pub async fn run(shell: &Shell, args: ProposeRegistrationArgs) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    let args = args.fill_values_with_prompt(Some(&ecosystem_config))?;
    run_propose_chain_registration(
        &chain_config,
        args.chain_registrar,
        args.l1_rpc_url,
        &args.main_wallet,
    )
    .await
}

pub async fn run_propose_chain_registration(
    chain_config: &ChainConfig,
    chain_registrar: Address,
    l1_rpc_url: String,
    wallet: &Wallet,
) -> anyhow::Result<()> {
    let wallets = chain_config.get_wallets_config()?;
    let spinner = Spinner::new(MSG_REGISTERING_CHAIN_SPINNER);
    ethereum::propose_registration(
        chain_registrar,
        wallet.clone(),
        l1_rpc_url,
        chain_config.l1_network.chain_id(),
        chain_config.chain_id.as_u64(),
        wallets.blob_operator.address,
        wallets.operator.address,
        wallets.governor.address,
        chain_config.base_token.address,
        wallets.token_multiplier_setter.map(|a| a.address),
        chain_config.base_token.nominator,
        chain_config.base_token.denominator,
        chain_config.l1_batch_commit_data_generator_mode,
    )
    .await?;
    spinner.finish();
    Ok(())
}
