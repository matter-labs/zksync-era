use std::path::Path;

use ethers::{contract::BaseContract, types::Address};
use xshell::Shell;
use zkstack_cli_common::{
    forge::{Forge, ForgeScript, ForgeScriptArgs},
    spinner::Spinner,
    wallets::Wallet,
};
use zkstack_cli_config::forge_interface::script_params::ENABLE_EVM_EMULATOR_PARAMS;

use crate::{
    abi::ENABLEEVMEMULATORABI_ABI,
    messages::MSG_ENABLING_EVM_EMULATOR,
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

pub async fn enable_evm_emulator(
    shell: &Shell,
    foundry_contracts_path: &Path,
    admin: Address,
    governor: &Wallet,
    target_address: Address,
    forge_args: &ForgeScriptArgs,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    let enable_evm_emulator_contract = BaseContract::from(ENABLEEVMEMULATORABI_ABI.clone());
    let calldata = enable_evm_emulator_contract
        .encode("chainAllowEvmEmulation", (admin, target_address))
        .unwrap();
    let forge = Forge::new(foundry_contracts_path)
        .script(&ENABLE_EVM_EMULATOR_PARAMS.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_broadcast()
        .with_calldata(&calldata);
    enable_evm_inner(shell, governor, forge).await
}

async fn enable_evm_inner(
    shell: &Shell,
    governor: &Wallet,
    mut forge: ForgeScript,
) -> anyhow::Result<()> {
    forge = fill_forge_private_key(forge, Some(governor), WalletOwner::Governor)?;
    check_the_balance(&forge).await?;
    let spinner = Spinner::new(MSG_ENABLING_EVM_EMULATOR);
    forge.run(shell)?;
    spinner.finish();
    Ok(())
}
