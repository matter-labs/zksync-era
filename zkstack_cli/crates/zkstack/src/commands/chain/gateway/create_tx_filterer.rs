use anyhow::Context;
use ethers::{abi::parse_abi, contract::BaseContract};
use lazy_static::lazy_static;
use xshell::Shell;
use zkstack_cli_common::{
    config::global_config,
    forge::{Forge, ForgeArgs, ForgeRunner, ForgeScriptArgs},
    wallets::Wallet,
};
use zkstack_cli_config::{
    forge_interface::{
        deploy_gateway_tx_filterer::{
            input::GatewayTxFiltererInput, output::GatewayTxFiltererOutput,
        },
        script_params::DEPLOY_GATEWAY_TX_FILTERER,
    },
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    ChainConfig, ZkStackConfig, ZkStackConfigTrait,
};

use crate::{
    admin_functions::{set_transaction_filterer, AdminScriptMode},
    messages::MSG_CHAIN_NOT_INITIALIZED,
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

lazy_static! {
    static ref DEPLOY_GATEWAY_TX_FILTERER_ABI: BaseContract =
        BaseContract::from(parse_abi(&["function runWithInputFromFile() public"]).unwrap(),);
}

pub async fn run(args: ForgeArgs, shell: &Shell) -> anyhow::Result<()> {
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

    let mut runner = ForgeRunner::new(args.runner.clone());
    let output: GatewayTxFiltererOutput = deploy_gateway_tx_filterer(
        shell,
        &mut runner,
        &args.script,
        &chain_config,
        &chain_deployer_wallet,
        GatewayTxFiltererInput::new(
            &ecosystem_config.get_initial_deployment_config().unwrap(),
            &chain_contracts_config,
        )?,
        l1_url.clone(),
    )
    .await?;

    set_transaction_filterer(
        shell,
        &mut runner,
        &args.script,
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
    runner: &mut ForgeRunner,
    forge_args: &ForgeScriptArgs,
    chain_config: &ChainConfig,
    deployer: &Wallet,
    input: GatewayTxFiltererInput,
    l1_rpc_url: String,
) -> anyhow::Result<GatewayTxFiltererOutput> {
    input.save(
        shell,
        DEPLOY_GATEWAY_TX_FILTERER.input(&chain_config.path_to_foundry_scripts()),
    )?;

    let mut forge = Forge::new(&chain_config.path_to_foundry_scripts())
        .script(&DEPLOY_GATEWAY_TX_FILTERER.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_calldata(
            &DEPLOY_GATEWAY_TX_FILTERER_ABI
                .encode("runWithInputFromFile", ())
                .unwrap(),
        )
        .with_broadcast();

    // This script can be run by any wallet without privileges
    forge = fill_forge_private_key(forge, Some(deployer), WalletOwner::Deployer)?;
    check_the_balance(&forge).await?;
    runner.run(shell, forge)?;

    GatewayTxFiltererOutput::read(
        shell,
        DEPLOY_GATEWAY_TX_FILTERER.output(&chain_config.path_to_foundry_scripts()),
    )
}
