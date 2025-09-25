use anyhow::Context;
use clap::Parser;
use ethers::{
    abi::{parse_abi, Address},
    contract::BaseContract,
    types::U256,
    utils::hex,
};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_common::{
    ethereum::get_ethers_provider,
    forge::{Forge, ForgeScriptArgs},
    wallets::Wallet,
};
use zkstack_cli_config::{
    forge_interface::{
        deploy_ecosystem::input::GenesisInput,
        gateway_vote_preparation::{
            input::GatewayVotePreparationConfig, output::DeployGatewayCTMOutput,
        },
        script_params::GATEWAY_VOTE_PREPARATION,
    },
    override_config,
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    ChainConfig, EcosystemConfig, GatewayConfig, ZkStackConfig, ZkStackConfigTrait,
};
use zkstack_cli_types::ProverMode;

use crate::{
    abi::BridgehubAbi,
    admin_functions::{
        governance_execute_calls, grant_gateway_whitelist, revoke_gateway_whitelist,
        AdminScriptMode,
    },
    commands::chain::utils::display_admin_script_output,
    consts::PATH_TO_GATEWAY_OVERRIDE_CONFIG,
    messages::MSG_CHAIN_NOT_INITIALIZED,
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

lazy_static! {
    static ref GATEWAY_VOTE_PREPARATION_ABI: BaseContract = BaseContract::from(
        parse_abi(&["function prepareForGWVoting(uint256 ctmChainId) public"]).unwrap(),
    );
}

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct ConvertToGatewayArgs {
    /// All ethereum environment related arguments
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,

    /// Pass the bridgehub, if existing ecosystem is being used
    #[clap(long)]
    pub bridgehub_addr: Option<Address>,

    /// Pass the chain id, for which we want to deploy ctm on GW
    #[clap(long, value_parser = parse_decimal_u256)]
    pub ctm_chain_id: Option<U256>,

    #[clap(long, default_value_t = false)]
    pub only_save_calldata: bool,
}

fn parse_decimal_u256(s: &str) -> Result<U256, String> {
    if s.starts_with("0x") || s.starts_with("0X") {
        return Err("Hexadecimal format not allowed for ctm_chain_id. Use a decimal value.".into());
    }
    U256::from_dec_str(s).map_err(|e| format!("Invalid decimal U256: {e}"))
}

pub async fn run(convert_to_gw_args: ConvertToGatewayArgs, shell: &Shell) -> anyhow::Result<()> {
    let args = convert_to_gw_args.forge_args;
    let ecosystem_config = ZkStackConfig::ecosystem(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    let l1_url = chain_config.get_secrets_config().await?.l1_rpc_url()?;
    let chain_contracts_config = chain_config.get_contracts_config()?;
    let chain_genesis_config = chain_config.get_genesis_config().await?;
    let genesis_input = GenesisInput::new(&chain_genesis_config)?;
    override_config(
        shell,
        &ecosystem_config
            .default_configs_path()
            .join(PATH_TO_GATEWAY_OVERRIDE_CONFIG),
        &chain_config,
    )?;

    let chain_deployer_wallet = chain_config
        .get_wallets_config()?
        .deployer
        .context("deployer")?;

    let (grantees, bridgehub_governance_addr) =
        if let Some(addr) = convert_to_gw_args.bridgehub_addr {
            let l1_provider = get_ethers_provider(&l1_url)?;
            let l1_bridgehub = BridgehubAbi::new(addr, l1_provider);
            let addr = l1_bridgehub.owner().await?;

            (vec![addr, chain_deployer_wallet.address], addr)
        } else {
            let governance_addr = ecosystem_config.get_contracts_config()?.l1.governance_addr;

            (
                vec![
                    governance_addr,
                    chain_deployer_wallet.address,
                    chain_contracts_config
                        .ecosystem_contracts
                        .stm_deployment_tracker_proxy_addr
                        .context("No CTM deployment tracker")?,
                ],
                governance_addr,
            )
        };

    let mode_chain_governor = if convert_to_gw_args.only_save_calldata {
        AdminScriptMode::OnlySave
    } else {
        AdminScriptMode::Broadcast(chain_config.get_wallets_config()?.governor)
    };

    let mode_ecosystem_governor = if convert_to_gw_args.only_save_calldata {
        AdminScriptMode::OnlySave
    } else {
        AdminScriptMode::Broadcast(ecosystem_config.get_wallets()?.governor)
    };

    let mut output = grant_gateway_whitelist(
        shell,
        &args,
        &chain_config.path_to_foundry_scripts(),
        mode_chain_governor.clone(),
        chain_config.chain_id.as_u64(),
        chain_contracts_config
            .ecosystem_contracts
            .bridgehub_proxy_addr,
        grantees,
        l1_url.clone(),
    )
    .await?;

    if convert_to_gw_args.only_save_calldata {
        display_admin_script_output(output);
    }

    let vote_preparation_output = gateway_vote_preparation(
        shell,
        args.clone(),
        &ecosystem_config,
        &chain_config,
        &chain_deployer_wallet,
        GatewayVotePreparationConfig::new(
            &ecosystem_config.get_initial_deployment_config().unwrap(),
            &genesis_input,
            &chain_contracts_config,
            ecosystem_config.era_chain_id.as_u64().into(),
            chain_config.chain_id.as_u64().into(),
            ecosystem_config.get_contracts_config()?.l1.governance_addr,
            ecosystem_config.prover_version == ProverMode::NoProofs,
            chain_deployer_wallet.address,
            ecosystem_config
                .get_contracts_config()?
                .ecosystem_contracts
                .ctm
                .expected_rollup_l2_da_validator,
            // This address is not present on local deployments
            Address::zero(),
        ),
        l1_url.clone(),
        convert_to_gw_args
            .ctm_chain_id
            .unwrap_or(chain_config.chain_id.as_u64().into()),
    )
    .await?;

    // Now, we will need to execute the corresponding governance calls
    // These calls will produce some L1->L2 transactions. However tracking those is hard at this point, so we won't do it here.
    output = governance_execute_calls(
        shell,
        &ecosystem_config,
        mode_ecosystem_governor,
        hex::decode(&vote_preparation_output.governance_calls_to_execute).unwrap(),
        &args,
        l1_url.clone(),
        Some(bridgehub_governance_addr),
    )
    .await?;

    if convert_to_gw_args.only_save_calldata {
        display_admin_script_output(output);
    } else {
        // We will revoke the access of the hot wallet immediately
        revoke_gateway_whitelist(
            shell,
            &args,
            &chain_config.path_to_foundry_scripts(),
            mode_chain_governor,
            chain_config.chain_id.as_u64(),
            chain_contracts_config
                .ecosystem_contracts
                .bridgehub_proxy_addr,
            chain_deployer_wallet.address,
            l1_url.clone(),
        )
        .await?;
    }

    let gateway_config: GatewayConfig = vote_preparation_output.into();

    gateway_config.save_with_base_path(shell, chain_config.configs.clone())?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn gateway_vote_preparation(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    config: &EcosystemConfig,
    chain_config: &ChainConfig,
    deployer: &Wallet,
    input: GatewayVotePreparationConfig,
    l1_rpc_url: String,
    ctm_chain_id: U256,
) -> anyhow::Result<DeployGatewayCTMOutput> {
    input.save(
        shell,
        GATEWAY_VOTE_PREPARATION.input(&chain_config.path_to_foundry_scripts()),
    )?;

    let calldata = GATEWAY_VOTE_PREPARATION_ABI
        .encode("prepareForGWVoting", ctm_chain_id)
        .unwrap();

    let mut forge: zkstack_cli_common::forge::ForgeScript =
        Forge::new(&config.path_to_foundry_scripts())
            .script(&GATEWAY_VOTE_PREPARATION.script(), forge_args.clone())
            .with_ffi()
            .with_rpc_url(l1_rpc_url)
            .with_calldata(&calldata)
            .with_broadcast();

    // Governor private key is required for this script
    forge = fill_forge_private_key(forge, Some(deployer), WalletOwner::Deployer)?;
    check_the_balance(&forge).await?;
    forge.run(shell)?;

    DeployGatewayCTMOutput::read(
        shell,
        GATEWAY_VOTE_PREPARATION.output(&chain_config.path_to_foundry_scripts()),
    )
}
