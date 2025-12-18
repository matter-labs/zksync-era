use std::path::Path;

use ethers::{contract::BaseContract, types::Address};
use lazy_static::lazy_static;
use xshell::Shell;
use zkstack_cli_common::{
    forge::{Forge, ForgeScript, ForgeScriptArgs},
    spinner::Spinner,
    wallets::Wallet,
};
use zkstack_cli_config::{
    forge_interface::script_params::REGISTER_ON_ALL_CHAINS_SCRIPT_PARAMS, ZkStackConfig,
    ZkStackConfigTrait,
};
use zksync_basic_types::L2ChainId;

use crate::{
    abi::IREGISTERONALLCHAINSABI_ABI,
    messages::MSG_REGISTERING_ON_ALL_CHAINS_SPINNER,
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

lazy_static! {
    static ref REGISTER_ON_ALL_CHAINS_FUNCTIONS: BaseContract =
        BaseContract::from(IREGISTERONALLCHAINSABI_ABI.clone());
}

pub async fn run(args: ForgeScriptArgs, shell: &Shell) -> anyhow::Result<()> {
    let chain_config = ZkStackConfig::current_chain(shell)?;

    let contracts_config = chain_config.get_contracts_config()?;
    let secrets = chain_config.get_secrets_config().await?;
    let l1_rpc_url = secrets.l1_rpc_url()?;

    register_on_all_chains(
        shell,
        &chain_config.path_to_foundry_scripts(),
        contracts_config.ecosystem_contracts.bridgehub_proxy_addr,
        chain_config.chain_id,
        &chain_config
            .get_wallets_config()?
            .deployer
            .expect("Deployer wallet not set"),
        &args,
        l1_rpc_url,
    )
    .await?;

    Ok(())
}

pub async fn register_on_all_chains(
    shell: &Shell,
    foundry_contracts_path: &Path,
    bridgehub_address: Address,
    chain_id: L2ChainId,
    deployer: &Wallet,
    forge_args: &ForgeScriptArgs,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    let calldata = REGISTER_ON_ALL_CHAINS_FUNCTIONS
        .encode(
            "registerOnOtherChains",
            (bridgehub_address, chain_id.as_u64()),
        )
        .unwrap();
    let forge = Forge::new(foundry_contracts_path)
        .script(
            &REGISTER_ON_ALL_CHAINS_SCRIPT_PARAMS.script(),
            forge_args.clone(),
        )
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_broadcast()
        .with_calldata(&calldata);
    register_on_all_chains_inner(shell, deployer, forge).await
}

async fn register_on_all_chains_inner(
    shell: &Shell,
    deployer: &Wallet,
    mut forge: ForgeScript,
) -> anyhow::Result<()> {
    forge = fill_forge_private_key(forge, Some(deployer), WalletOwner::Deployer)?;
    check_the_balance(&forge).await?;
    let spinner = Spinner::new(MSG_REGISTERING_ON_ALL_CHAINS_SPINNER);
    forge.run(shell)?;
    spinner.finish();
    Ok(())
}
