use common::{
    forge::{Forge, ForgeScript, ForgeScriptArgs},
    spinner::Spinner,
    wallets::Wallet,
};
use config::{forge_interface::script_params::ENABLE_EVM_EMULATOR_PARAMS, EcosystemConfig};
use ethers::{abi::parse_abi, contract::BaseContract, types::Address};
use lazy_static::lazy_static;
use xshell::Shell;

use crate::{
    messages::MSG_ENABLING_EVM_EMULATOR,
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

lazy_static! {
    static ref ENABLE_EVM_EMULATOR: BaseContract = BaseContract::from(
        parse_abi(&[
            "function chainAllowEvmEmulation(address chainAdmin, address target) public",
        ])
        .unwrap(),
    );
}

pub async fn enable_evm_emulator(
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    admin: Address,
    governor: &Wallet,
    target_address: Address,
    forge_args: &ForgeScriptArgs,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    // Resume for accept admin doesn't work properly. Foundry assumes that if signature of the function is the same,
    // than it's the same call, but because we are calling this function multiple times during the init process,
    // code assumes that doing only once is enough, but actually we need to accept admin multiple times
    let mut forge_args = forge_args.clone();
    forge_args.resume = false;

    let calldata = ENABLE_EVM_EMULATOR
        .encode("chainAllowEvmEmulation", (admin, target_address))
        .unwrap();
    let foundry_contracts_path = ecosystem_config.path_to_foundry();
    let forge = Forge::new(&foundry_contracts_path)
        .script(
            &ENABLE_EVM_EMULATOR_PARAMS.script(),
            forge_args.clone(),
        )
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
