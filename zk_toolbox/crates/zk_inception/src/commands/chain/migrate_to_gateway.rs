use anyhow::Context;
use clap::Parser;
use common::{
    config::global_config,
    forge::{Forge, ForgeScript, ForgeScriptArgs},
    logger,
    spinner::Spinner,
};
use config::{
    forge_interface::{
        deploy_ecosystem::input::InitialDeploymentConfig,
        deploy_gateway_ctm::{input::DeployGatewayCTMInput, output::DeployGatewayCTMOutput},
        gateway_preparation::{input::GatewayPreparationConfig, output::GatewayPreparationOutput},
        script_params::{ACCEPT_GOVERNANCE_SCRIPT_PARAMS, DEPLOY_GATEWAY_CTM, GATEWAY_PREPARATION},
    },
    traits::{ReadConfig, ReadConfigWithBasePath, SaveConfig, SaveConfigWithBasePath},
    ChainConfig, ContractsConfig, EcosystemConfig, GenesisConfig,
};
use ethers::{abi::parse_abi, contract::BaseContract, types::Bytes, utils::hex};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zksync_basic_types::{Address, H256, U256};
use zksync_config::configs::{chain, GatewayConfig};

use crate::{
    messages::{
        MSG_CHAIN_NOT_INITIALIZED, MSG_L1_SECRETS_MUST_BE_PRESENTED,
        MSG_TOKEN_MULTIPLIER_SETTER_UPDATED_TO, MSG_UPDATING_TOKEN_MULTIPLIER_SETTER_SPINNER,
        MSG_WALLETS_CONFIG_MUST_BE_PRESENT, MSG_WALLET_TOKEN_MULTIPLIER_SETTER_NOT_FOUND,
    },
    utils::forge::{check_the_balance, fill_forge_private_key},
};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct MigrateToGatewayArgs {
    /// All ethereum environment related arguments
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,

    #[clap(long)]
    pub gateway_chain_name: String,
}

// FIXME: use a different script here (i.e. make it have a different file)
lazy_static! {
    static ref GATEWAY_PREPARATION_INTERFACE: BaseContract = BaseContract::from(
        parse_abi(&[
            "function migrateChainToGateway(address chainAdmin,uint256 chainId,bytes32 adminOperationSalt) public"
        ])
        .unwrap(),
    );
}

pub async fn run(args: MigrateToGatewayArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    
    
    let chain_name = global_config().chain_name.clone();
    let chain_config = ecosystem_config
        .load_chain(chain_name)
        .context(MSG_CHAIN_NOT_INITIALIZED)?;


    let gateway_chain_config = ecosystem_config.load_chain(Some(args.gateway_chain_name.clone())).context("Gateway not present")?;
    let gateway_gateway_config = gateway_chain_config.get_gateway_config().context("Gateway config not present")?;

    let l1_url = chain_config
        .get_secrets_config()?
        .l1
        .context(MSG_L1_SECRETS_MUST_BE_PRESENTED)?
        .l1_rpc_url
        .expose_str()
        .to_string();

    // FIXME: do we need to build l1 contracts here? they are typically pre-built

    let genesis_config = chain_config.get_genesis_config()?;

    // Firstly, deploying gateway contracts

    let whitelist_config_path = GATEWAY_PREPARATION.input(&ecosystem_config.link_to_code);
    let preparation_config =
        GatewayPreparationConfig::new(&gateway_chain_config, &ecosystem_config.get_contracts_config()?, &gateway_gateway_config)?;
    preparation_config.save(shell, whitelist_config_path)?;

    let hash = call_script(
        shell,
        args.forge_args.clone(),
        &GATEWAY_PREPARATION_INTERFACE
            .encode(
                "migrateChainToGateway",
                (chain_config.get_contracts_config().unwrap().l1.chain_admin_addr, U256::from(chain_config.chain_id.0), H256::random()),
            )
            .unwrap(),
        &ecosystem_config,
        &chain_config,
        l1_url.clone(),
    )
    .await?;

    println!("Chain successfully migrated! Migration hash: {}", hex::encode(hash));

    Ok(())
}

async fn call_script(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    data: &Bytes,
    config: &EcosystemConfig,
    chain_config: &ChainConfig,
    l1_rpc_url: String,
) -> anyhow::Result<H256> {
    let mut forge = Forge::new(&config.path_to_l1_foundry())
        .script(&GATEWAY_PREPARATION.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_broadcast()
        .with_calldata(data);

    // Governor private key is required for this script
    forge = fill_forge_private_key(forge, chain_config.get_wallets_config().unwrap().governor_private_key())?;
    check_the_balance(&forge).await?;
    forge.run(shell)?;

    let gateway_preparation_script_output = GatewayPreparationOutput::read(
        shell,
        GATEWAY_PREPARATION.output(&chain_config.link_to_code),
    )?;

    Ok(gateway_preparation_script_output.governance_l2_tx_hash)
}

// pub async fn set_token_multiplier_setter(
//     shell: &Shell,
//     ecosystem_config: &EcosystemConfig,
//     governor: Option<H256>,
//     chain_admin_address: Address,
//     target_address: Address,
//     forge_args: &ForgeScriptArgs,
//     l1_rpc_url: String,
// ) -> anyhow::Result<()> {
//     // Resume for accept admin doesn't work properly. Foundry assumes that if signature of the function is the same,
//     // then it's the same call, but because we are calling this function multiple times during the init process,
//     // code assumes that doing only once is enough, but actually we need to accept admin multiple times
//     let mut forge_args = forge_args.clone();
//     forge_args.resume = false;

//     let calldata = SET_TOKEN_MULTIPLIER_SETTER
//         .encode(
//             "chainSetTokenMultiplierSetter",
//             (chain_admin_address, target_address),
//         )
//         .unwrap();
//     let foundry_contracts_path = ecosystem_config.path_to_l1_foundry();
//     let forge = Forge::new(&foundry_contracts_path)
//         .script(
//             &ACCEPT_GOVERNANCE_SCRIPT_PARAMS.script(),
//             forge_args.clone(),
//         )
//         .with_ffi()
//         .with_rpc_url(l1_rpc_url)
//         .with_broadcast()
//         .with_calldata(&calldata);
//     update_token_multiplier_setter(shell, governor, forge).await
// }

// async fn update_token_multiplier_setter(
//     shell: &Shell,
//     governor: Option<H256>,
//     mut forge: ForgeScript,
// ) -> anyhow::Result<()> {
//     forge = fill_forge_private_key(forge, governor)?;
//     check_the_balance(&forge).await?;
//     forge.run(shell)?;
//     Ok(())
// }
