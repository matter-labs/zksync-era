use anyhow::Context;
use ethers::{abi::parse_abi, contract::BaseContract, types::Bytes, utils::hex};
use lazy_static::lazy_static;
use xshell::Shell;
use zkstack_cli_common::{
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
    logger,
    wallets::Wallet,
};
use zkstack_cli_config::{
    forge_interface::{
        deploy_ecosystem::input::{GenesisInput, InitialDeploymentConfig},
        deploy_gateway_ctm::{input::DeployGatewayCTMInput, output::DeployGatewayCTMOutput},
        gateway_preparation::{input::GatewayPreparationConfig, output::GatewayPreparationOutput},
        script_params::{DEPLOY_GATEWAY_CTM, GATEWAY_GOVERNANCE_TX_PATH1, GATEWAY_PREPARATION},
    },
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    ChainConfig, EcosystemConfig,
};
use zksync_basic_types::H256;
use zksync_config::configs::gateway::GatewayConfig;
use zksync_types::Address;

use crate::{
    messages::MSG_CHAIN_NOT_INITIALIZED,
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

lazy_static! {
    pub static ref GATEWAY_PREPARATION_INTERFACE: BaseContract = BaseContract::from(
        parse_abi(&["function deployGatewayTransactionFilterer() public",]).unwrap(),
    );
}

// Deploys Gateway tx filterer for a chain and returns its address
pub(crate) async fn run_inner(args: ForgeScriptArgs, shell: &Shell) -> anyhow::Result<Address> {
    let chain_name = global_config().chain_name.clone();
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_chain(chain_name)
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    let l1_url = chain_config
        .get_secrets_config()
        .await?
        .get::<String>("l1.l1_rpc_url")?;

    let output = call_script(
        shell,
        args.clone(),
        &GATEWAY_PREPARATION_INTERFACE
            .encode("deployGatewayTransactionFilterer", ())
            .unwrap(),
        &ecosystem_config,
        &chain_config,
        &chain_config.get_wallets_config()?.deployer.unwrap(),
        l1_url.clone(),
        true,
    )
    .await?;

    Ok(output.gateway_transaction_filterer_proxy)
}

pub async fn run(args: ForgeScriptArgs, shell: &Shell) -> anyhow::Result<()> {
    println!("Deploying ZK Gateway transaction filterer!");

    let address = run_inner(args, shell).await?;

    println!(
        "ZK Gateway transaction filterer deployed on address {:#?}!",
        address
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn call_script(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    data: &Bytes,
    config: &EcosystemConfig,
    chain_config: &ChainConfig,
    deployer: &Wallet,
    l1_rpc_url: String,
    with_broadcast: bool,
) -> anyhow::Result<GatewayPreparationOutput> {
    let mut forge = Forge::new(&config.path_to_l1_foundry())
        .script(&GATEWAY_PREPARATION.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_calldata(data);
    if with_broadcast {
        forge = forge.with_broadcast();
    }

    // Governor private key is required for this script
    forge = fill_forge_private_key(forge, Some(deployer), WalletOwner::Deployer)?;
    check_the_balance(&forge).await?;
    forge.run(shell)?;

    GatewayPreparationOutput::read(
        shell,
        GATEWAY_PREPARATION.output(&chain_config.link_to_code),
    )
}
