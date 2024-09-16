use anyhow::Context;
use common::{
    config::global_config,
    forge::{Forge, ForgeScript, ForgeScriptArgs},
    logger,
    spinner::Spinner,
};
use config::{forge_interface::{deploy_ecosystem::input::InitialDeploymentConfig, deploy_gateway_ctm::{input::DeployGatewayCTMInput, output::DeployGatewayCTMOutput}, script_params::{ACCEPT_GOVERNANCE_SCRIPT_PARAMS, DEPLOY_GATEWAY_CTM}}, traits::{ReadConfig, ReadConfigWithBasePath, SaveConfig}, ChainConfig, ContractsConfig, EcosystemConfig, GenesisConfig};
use ethers::{abi::parse_abi, contract::BaseContract, utils::hex};
use lazy_static::lazy_static;
use xshell::Shell;
use zksync_basic_types::{Address, H256};

use crate::{
    messages::{
        MSG_CHAIN_NOT_INITIALIZED, MSG_L1_SECRETS_MUST_BE_PRESENTED,
        MSG_TOKEN_MULTIPLIER_SETTER_UPDATED_TO, MSG_UPDATING_TOKEN_MULTIPLIER_SETTER_SPINNER,
        MSG_WALLETS_CONFIG_MUST_BE_PRESENT, MSG_WALLET_TOKEN_MULTIPLIER_SETTER_NOT_FOUND,
    },
    utils::forge::{check_the_balance, fill_forge_private_key},
};

lazy_static! {
    static ref SET_TOKEN_MULTIPLIER_SETTER: BaseContract = BaseContract::from(
        parse_abi(&[
            "function chainSetTokenMultiplierSetter(address chainAdmin, address target) public"
        ])
        .unwrap(),
    );
}

pub async fn run(args: ForgeScriptArgs, shell: &Shell) -> anyhow::Result<()> {
    let chain_name = global_config().chain_name.clone();
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_chain(chain_name)
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    let contracts_config = chain_config.get_contracts_config()?;
    let l1_url = chain_config
        .get_secrets_config()?
        .l1
        .context(MSG_L1_SECRETS_MUST_BE_PRESENTED)?
        .l1_rpc_url
        .expose_str()
        .to_string();

        

    let genesis_config = chain_config.get_genesis_config()?;


    deploy_gateway_ctm(
        shell,
        args,
        &ecosystem_config,
        &chain_config,
        &genesis_config,
        &contracts_config,
        &ecosystem_config.get_initial_deployment_config().unwrap(),
        l1_url,
    ).await?;


    // FIXME: do we need to build l1 contracts here? they are typically pre-built

    // Firstly, deploying gateway outputs




    Ok(())
}

async fn deploy_gateway_ctm(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    config: &EcosystemConfig,
    chain_config: &ChainConfig,
    genesis_config: &GenesisConfig,
    contracts_config: &ContractsConfig,
    initial_deployemnt_config: &InitialDeploymentConfig,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    let deploy_config_path = DEPLOY_GATEWAY_CTM.input(&config.link_to_code);

    let deploy_config = DeployGatewayCTMInput::new(
        chain_config, 
        config,
        genesis_config,
        contracts_config,
        initial_deployemnt_config
    );
    deploy_config.save(shell, deploy_config_path)?;

    let mut forge = Forge::new(&config.path_to_l1_foundry())
        .script(&DEPLOY_GATEWAY_CTM.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_broadcast();

    // Governor private key should not be needed for this script
    forge = fill_forge_private_key(forge, config.get_wallets()?.deployer_private_key())?;
    check_the_balance(&forge).await?;
    forge.run(shell)?;

    let register_chain_output = DeployGatewayCTMOutput::read(
        shell,
        DEPLOY_GATEWAY_CTM.output(&chain_config.link_to_code),
    )?;

    println!("Gateway contracts successfully deployed: {:#?}", register_chain_output);
    
    Ok(())
}

pub async fn set_token_multiplier_setter(
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    governor: Option<H256>,
    chain_admin_address: Address,
    target_address: Address,
    forge_args: &ForgeScriptArgs,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    // Resume for accept admin doesn't work properly. Foundry assumes that if signature of the function is the same,
    // then it's the same call, but because we are calling this function multiple times during the init process,
    // code assumes that doing only once is enough, but actually we need to accept admin multiple times
    let mut forge_args = forge_args.clone();
    forge_args.resume = false;

    let calldata = SET_TOKEN_MULTIPLIER_SETTER
        .encode(
            "chainSetTokenMultiplierSetter",
            (chain_admin_address, target_address),
        )
        .unwrap();
    let foundry_contracts_path = ecosystem_config.path_to_l1_foundry();
    let forge = Forge::new(&foundry_contracts_path)
        .script(
            &ACCEPT_GOVERNANCE_SCRIPT_PARAMS.script(),
            forge_args.clone(),
        )
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_broadcast()
        .with_calldata(&calldata);
    update_token_multiplier_setter(shell, governor, forge).await
}

async fn update_token_multiplier_setter(
    shell: &Shell,
    governor: Option<H256>,
    mut forge: ForgeScript,
) -> anyhow::Result<()> {
    forge = fill_forge_private_key(forge, governor)?;
    check_the_balance(&forge).await?;
    forge.run(shell)?;
    Ok(())
}
