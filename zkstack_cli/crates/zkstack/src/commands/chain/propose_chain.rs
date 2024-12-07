use anyhow::Context;
use common::{
    forge::{Forge, ForgeScriptArgs},
    spinner::Spinner,
    wallets::Wallet,
};
use config::{
    forge_interface::{
        propose_registration::ProposeRegistrationInputConfig,
        script_params::PROPOSE_CHAIN_SCRIPT_PARAMS,
    },
    traits::SaveConfig,
    ChainConfig, EcosystemConfig,
};
use ethers::{abi::Address, utils::hex::ToHex};
use xshell::Shell;

use crate::{
    commands::chain::args::propose_registration::ProposeRegistrationArgs,
    messages::{MSG_CHAIN_NOT_INITIALIZED, MSG_PROPOSE_CHAIN_SPINNER},
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

pub async fn run(shell: &Shell, args: ProposeRegistrationArgs) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    let args = args.fill_values_with_prompt(Some(&ecosystem_config))?;
    run_propose_chain_registration(
        shell,
        &chain_config,
        args.chain_registrar,
        args.l1_rpc_url,
        args.forge_args,
        args.broadcast,
        args.sender,
        &args.main_wallet,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn run_propose_chain_registration(
    shell: &Shell,
    chain_config: &ChainConfig,
    chain_registrar: Address,
    l1_rpc_url: String,
    forge_args: ForgeScriptArgs,
    broadcast: bool,
    sender: Option<Address>,
    wallet: &Wallet,
) -> anyhow::Result<()> {
    let wallets = chain_config.get_wallets_config()?;
    let spinner = Spinner::new(MSG_PROPOSE_CHAIN_SPINNER);
    let deploy_config_path = PROPOSE_CHAIN_SCRIPT_PARAMS.input(&chain_config.link_to_code);

    let deploy_config = ProposeRegistrationInputConfig::new(
        chain_registrar,
        chain_config.chain_id.as_u64(),
        wallets.blob_operator.address,
        wallets.operator.address,
        wallets.governor.address,
        chain_config.base_token.clone(),
        wallets.token_multiplier_setter.map(|a| a.address),
        chain_config.l1_batch_commit_data_generator_mode,
    );
    deploy_config.save(shell, deploy_config_path)?;

    let mut forge = Forge::new(&chain_config.path_to_foundry())
        .script(&PROPOSE_CHAIN_SCRIPT_PARAMS.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url);

    if broadcast {
        forge = forge.with_broadcast();
    }

    if let Some(address) = sender {
        forge = forge.with_sender(address.encode_hex_upper());
    } else {
        forge = fill_forge_private_key(forge, Some(wallet), WalletOwner::Governor)?;
        check_the_balance(&forge).await?;
    }

    forge.run(shell)?;
    spinner.finish();
    Ok(())
}
