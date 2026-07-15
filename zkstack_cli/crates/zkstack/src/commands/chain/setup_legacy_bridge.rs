use ethers::contract::BaseContract;
use xshell::Shell;
use zkstack_cli_common::{
    forge::{Forge, ForgeScriptArgs},
    spinner::Spinner,
};
use zkstack_cli_config::{
    forge_interface::script_params::SETUP_LEGACY_BRIDGE, ChainConfig, ContractsConfig,
    EcosystemConfig, ZkStackConfigTrait,
};

use crate::{
    abi::ISETUPLEGACYBRIDGEABI_ABI,
    messages::MSG_DEPLOYING_PAYMASTER,
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

pub async fn setup_legacy_bridge(
    shell: &Shell,
    chain_config: &ChainConfig,
    ecosystem_config: &EcosystemConfig,
    contracts_config: &ContractsConfig,
    forge_args: ForgeScriptArgs,
) -> anyhow::Result<()> {
    let foundry_contracts_path = chain_config.path_to_foundry_scripts();
    let secrets = chain_config.get_secrets_config().await?;

    // Extract parameters
    let bridgehub = contracts_config.ecosystem_contracts.bridgehub_proxy_addr;
    let chain_id = chain_config.chain_id.as_u64();

    // Encode calldata
    // Note: create2_factory_addr, create2_factory_salt, and diamond_proxy are all queried in the Solidity script
    let setup_legacy_bridge_contract = BaseContract::from(ISETUPLEGACYBRIDGEABI_ABI.clone());
    let calldata = setup_legacy_bridge_contract
        .encode("run", (bridgehub, chain_id))
        .unwrap();

    let mut forge = Forge::new(&foundry_contracts_path)
        .script(&SETUP_LEGACY_BRIDGE.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(secrets.l1_rpc_url()?)
        .with_calldata(&calldata)
        .with_broadcast();

    forge = fill_forge_private_key(
        forge,
        Some(&ecosystem_config.get_wallets()?.governor),
        WalletOwner::Governor,
    )?;

    let spinner = Spinner::new(MSG_DEPLOYING_PAYMASTER);
    check_the_balance(&forge).await?;
    forge.run(shell)?;
    spinner.finish();

    Ok(())
}
