use std::path::Path;

use ethers::{contract::BaseContract, types::Address};
use xshell::Shell;
use zkstack_cli_common::{
    forge::{Forge, ForgeScript, ForgeScriptArgs},
    spinner::Spinner,
    wallets::Wallet,
};
use zkstack_cli_config::forge_interface::script_params::SET_INTEROP_FEE_PARAMS;
use zksync_basic_types::U256;

use crate::{
    abi::ISETINTEROPFEEABI_ABI,
    messages::MSG_SETTING_INTEROP_FEE,
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

pub async fn set_interop_fee(
    shell: &Shell,
    foundry_contracts_path: &Path,
    admin: Address,
    governor: &Wallet,
    target_address: Address,
    fee: U256,
    forge_args: &ForgeScriptArgs,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    let set_interop_fee_contract = BaseContract::from(ISETINTEROPFEEABI_ABI.clone());
    let calldata = set_interop_fee_contract
        .encode("setInteropFee", (admin, target_address, fee))
        .unwrap();
    let forge = Forge::new(foundry_contracts_path)
        .script(&SET_INTEROP_FEE_PARAMS.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_broadcast()
        .with_calldata(&calldata);
    set_interop_fee_inner(shell, governor, forge).await
}

async fn set_interop_fee_inner(
    shell: &Shell,
    governor: &Wallet,
    mut forge: ForgeScript,
) -> anyhow::Result<()> {
    forge = fill_forge_private_key(forge, Some(governor), WalletOwner::Governor)?;
    check_the_balance(&forge).await?;
    let spinner = Spinner::new(MSG_SETTING_INTEROP_FEE);
    forge.run(shell)?;
    spinner.finish();
    Ok(())
}
