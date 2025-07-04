use anyhow::Context;
use ethers::{abi::parse_abi, contract::BaseContract};
use lazy_static::lazy_static;
use xshell::Shell;
use zkstack_cli_common::{
    forge::{Forge, ForgeScriptArgs},
    logger,
    spinner::Spinner,
    wallets::Wallet,
};
use zkstack_cli_config::{forge_interface::script_params::UPGRADE_LOCAL_DEVNET, EcosystemConfig};
use zksync_basic_types::{Address, L2ChainId};
use zksync_types::{H256, U256};

use crate::{
    messages::MSG_CHAIN_NOT_INITIALIZED,
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

lazy_static! {
    static ref UPGRADE_VERIFIER: BaseContract = BaseContract::from(
        parse_abi(&[
        "function upgradeVerifier(address bridgehubAddr, uint256 chainId, address defaultUpgrade, address create2FactoryAddr) public"
        ])
        .unwrap(),
    );
}

pub async fn run(args: ForgeScriptArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    let contracts_config = chain_config.get_contracts_config()?;
    let genesis = chain_config.get_genesis_config().await?;
    let l1_url = chain_config.get_secrets_config().await?.l1_rpc_url()?;

    let spinner = Spinner::new("Upgrading verifier");
    upgrade_verifier(
        shell,
        &ecosystem_config,
        &chain_config.get_wallets_config()?.governor,
        contracts_config.ecosystem_contracts.bridgehub_proxy_addr,
        genesis.l2_chain_id()?,
        contracts_config.l1.default_upgrade_addr,
        contracts_config.create2_factory_addr,
        &args,
        l1_url,
    )
    .await?;
    spinner.finish();

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn upgrade_verifier(
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    governor: &Wallet,
    bridgehub: Address,
    chain_id: L2ChainId,
    default_upgrader: Address,
    create2_factory_addr: Address,
    forge_args: &ForgeScriptArgs,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    // Resume for accept admin doesn't work properly. Foundry assumes that if signature of the function is the same,
    // then it's the same call, but because we are calling this function multiple times during the init process,
    // code assumes that doing only once is enough, but actually we need to accept admin multiple times
    let mut forge_args = forge_args.clone();
    forge_args.resume = false;

    let calldata = UPGRADE_VERIFIER
        .encode(
            "upgradeVerifier",
            (
                bridgehub,
                U256::from(chain_id.as_u64()),
                default_upgrader,
                create2_factory_addr,
            ),
        )
        .unwrap();
    let foundry_contracts_path = ecosystem_config.path_to_l1_foundry();
    let mut forge = Forge::new(&foundry_contracts_path)
        .script(&UPGRADE_LOCAL_DEVNET.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_broadcast()
        .with_calldata(&calldata);
    forge = fill_forge_private_key(forge, Some(governor), WalletOwner::Governor)?;
    check_the_balance(&forge).await?;
    forge.run(shell)?;
    Ok(())
}
