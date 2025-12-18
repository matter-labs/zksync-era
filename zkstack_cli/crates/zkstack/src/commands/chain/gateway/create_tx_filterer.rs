use anyhow::Context;
use ethers::contract::BaseContract;
use lazy_static::lazy_static;
use xshell::Shell;
use zkstack_cli_common::{
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
    wallets::Wallet,
};
use zkstack_cli_config::{
    forge_interface::{
        deploy_gateway_tx_filterer::output::GatewayTxFiltererOutput,
        script_params::DEPLOY_GATEWAY_TX_FILTERER,
    },
    traits::{ReadConfig, SaveConfigWithBasePath},
    ChainConfig, ZkStackConfig, ZkStackConfigTrait,
};

use crate::{
    abi::DEPLOYGATEWAYTRANSACTIONFILTERERABI_ABI,
    admin_functions::{set_transaction_filterer, AdminScriptMode},
    messages::MSG_CHAIN_NOT_INITIALIZED,
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

lazy_static! {
    static ref DEPLOY_GATEWAY_TX_FILTERER_ABI: BaseContract =
        BaseContract::from(DEPLOYGATEWAYTRANSACTIONFILTERERABI_ABI.clone());
}

pub async fn run(args: ForgeScriptArgs, shell: &Shell) -> anyhow::Result<()> {
    let chain_name = global_config().chain_name.clone();
    let ecosystem_config = ZkStackConfig::ecosystem(shell)?;
    let chain_config = ecosystem_config
        .load_chain(chain_name)
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    let l1_url = chain_config.get_secrets_config().await?.l1_rpc_url()?;
    let mut chain_contracts_config = chain_config.get_contracts_config()?;

    let chain_deployer_wallet = chain_config
        .get_wallets_config()?
        .deployer
        .context("deployer")?;

    let output: GatewayTxFiltererOutput = deploy_gateway_tx_filterer(
        shell,
        args.clone(),
        &chain_config,
        &chain_deployer_wallet,
        &ecosystem_config,
        &chain_contracts_config,
        l1_url.clone(),
    )
    .await?;

    set_transaction_filterer(
        shell,
        &args,
        &chain_config.path_to_foundry_scripts(),
        AdminScriptMode::Broadcast(chain_config.get_wallets_config()?.governor),
        chain_config.chain_id.as_u64(),
        chain_contracts_config
            .ecosystem_contracts
            .bridgehub_proxy_addr,
        output.gateway_tx_filterer_proxy,
        l1_url.clone(),
    )
    .await?;

    chain_contracts_config.set_transaction_filterer(output.gateway_tx_filterer_proxy);
    chain_contracts_config.save_with_base_path(shell, chain_config.configs.clone())?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn deploy_gateway_tx_filterer(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    chain_config: &ChainConfig,
    deployer: &Wallet,
    _ecosystem_config: &zkstack_cli_config::EcosystemConfig,
    contracts_config: &zkstack_cli_config::ContractsConfig,
    l1_rpc_url: String,
) -> anyhow::Result<GatewayTxFiltererOutput> {
    // Extract parameters from configs
    let bridgehub_proxy_addr = contracts_config.ecosystem_contracts.bridgehub_proxy_addr;
    let chain_admin = contracts_config.l1.chain_admin_addr;
    let chain_proxy_admin = contracts_config
        .l1
        .chain_proxy_admin_addr
        .context("Missing chain_proxy_admin_addr")?;

    // Encode calldata for the run function
    let calldata = DEPLOY_GATEWAY_TX_FILTERER_ABI
        .encode(
            "run",
            (bridgehub_proxy_addr, chain_admin, chain_proxy_admin),
        )
        .unwrap();

    let mut forge = Forge::new(&chain_config.path_to_foundry_scripts())
        .script(&DEPLOY_GATEWAY_TX_FILTERER.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_calldata(&calldata)
        .with_broadcast();

    // This script can be run by any wallet without privileges
    forge = fill_forge_private_key(forge, Some(deployer), WalletOwner::Deployer)?;
    check_the_balance(&forge).await?;
    forge.run(shell)?;

    GatewayTxFiltererOutput::read(
        shell,
        DEPLOY_GATEWAY_TX_FILTERER.output(&chain_config.path_to_foundry_scripts()),
    )
}
