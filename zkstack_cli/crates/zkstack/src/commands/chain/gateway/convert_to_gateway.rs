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
    config::global_config,
    ethereum::get_ethers_provider,
    forge::{Forge, ForgeScriptArgs},
    wallets::Wallet,
};
use zkstack_cli_config::{
    forge_interface::{
        deploy_ecosystem::input::GenesisInput,
        deploy_gateway_tx_filterer::{
            input::GatewayTxFiltererInput, output::GatewayTxFiltererOutput,
        },
        gateway_vote_preparation::{
            input::GatewayVotePreparationConfig, output::DeployGatewayCTMOutput,
        },
        script_params::{DEPLOY_GATEWAY_TX_FILTERER, GATEWAY_VOTE_PREPARATION},
    },
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    ChainConfig, EcosystemConfig, GatewayConfig,
};
use zkstack_cli_types::ProverMode;

use crate::{
    abi::BridgehubAbi,
    admin_functions::{
        governance_execute_calls, grant_gateway_whitelist, revoke_gateway_whitelist,
        set_transaction_filterer, AdminScriptMode,
    },
    commands::chain::utils::display_admin_script_output,
    messages::MSG_CHAIN_NOT_INITIALIZED,
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

lazy_static! {
    static ref DEPLOY_GATEWAY_TX_FILTERER_ABI: BaseContract =
        BaseContract::from(parse_abi(&["function runWithInputFromFile() public"]).unwrap(),);
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
    let bridgehub_address = convert_to_gw_args.bridgehub_addr.unwrap_or(Address::zero());

    let chain_name = global_config().chain_name.clone();
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_chain(chain_name)
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    let l1_url = chain_config.get_secrets_config().await?.l1_rpc_url()?;
    let mut chain_contracts_config = chain_config.get_contracts_config()?;
    let chain_genesis_config = chain_config.get_genesis_config().await?;
    let genesis_input = GenesisInput::new(&chain_genesis_config)?;

    let chain_deployer_wallet = chain_config
        .get_wallets_config()?
        .deployer
        .context("deployer")?;

    let grantees;

    let bridgehub_governance_addr = if bridgehub_address.is_zero() {
        let output: GatewayTxFiltererOutput = deploy_gateway_tx_filterer(
            shell,
            args.clone(),
            &ecosystem_config,
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
            &args,
            &chain_config.path_to_l1_foundry(),
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

        grantees = vec![
            ecosystem_config.get_contracts_config()?.l1.governance_addr,
            chain_deployer_wallet.address,
            chain_contracts_config
                .ecosystem_contracts
                .stm_deployment_tracker_proxy_addr
                .context("No CTM deployment tracker")?,
        ];

        Address::zero()
    } else {
        let l1_provider = get_ethers_provider(&l1_url)?;
        let l1_bridgehub = BridgehubAbi::new(bridgehub_address, l1_provider);
        let bridgehub_governance_addr = l1_bridgehub.owner().await?;

        grantees = vec![bridgehub_governance_addr, chain_deployer_wallet.address];

        bridgehub_governance_addr
    };

    let mode = if convert_to_gw_args.only_save_calldata {
        AdminScriptMode::OnlySave
    } else {
        AdminScriptMode::Broadcast(chain_config.get_wallets_config()?.governor)
    };

    let mut output = grant_gateway_whitelist(
        shell,
        &args,
        &chain_config.path_to_l1_foundry(),
        AdminScriptMode::Broadcast(chain_config.get_wallets_config()?.governor), // For testing purposes
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
                .expected_rollup_l2_da_validator
                .context("No expected rollup l2 da validator")?,
            // This address is not present on local deployments
            Address::zero(),
        ),
        l1_url.clone(),
        convert_to_gw_args.ctm_chain_id.unwrap_or_default(),
    )
    .await?;

    // Now, we will need to execute the corresponding governance calls
    // These calls will produce some L1->L2 transactions. However tracking those is hard at this point, so we won't do it here.
    output = governance_execute_calls(
        shell,
        &ecosystem_config,
        AdminScriptMode::OnlySave,
        hex::decode(&vote_preparation_output.governance_calls_to_execute).unwrap(),
        &args,
        l1_url.clone(),
        bridgehub_governance_addr,
    )
    .await?;

    if convert_to_gw_args.only_save_calldata {
        display_admin_script_output(output);
    } else {
        // We will revoke the access of the hot wallet immediately
        revoke_gateway_whitelist(
            shell,
            &args,
            &chain_config.path_to_l1_foundry(),
            AdminScriptMode::Broadcast(chain_config.get_wallets_config()?.governor),
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
        GATEWAY_VOTE_PREPARATION.input(&chain_config.path_to_l1_foundry()),
    )?;

    let calldata = GATEWAY_VOTE_PREPARATION_ABI
        .encode("prepareForGWVoting", ctm_chain_id)
        .unwrap();

    let mut forge: zkstack_cli_common::forge::ForgeScript =
        Forge::new(&config.path_to_l1_foundry())
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
        GATEWAY_VOTE_PREPARATION.output(&chain_config.path_to_l1_foundry()),
    )
}

#[allow(clippy::too_many_arguments)]
pub async fn deploy_gateway_tx_filterer(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    config: &EcosystemConfig,
    chain_config: &ChainConfig,
    deployer: &Wallet,
    input: GatewayTxFiltererInput,
    l1_rpc_url: String,
) -> anyhow::Result<GatewayTxFiltererOutput> {
    input.save(
        shell,
        DEPLOY_GATEWAY_TX_FILTERER.input(&chain_config.path_to_l1_foundry()),
    )?;

    let mut forge = Forge::new(&config.path_to_l1_foundry())
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
    forge.run(shell)?;

    GatewayTxFiltererOutput::read(
        shell,
        DEPLOY_GATEWAY_TX_FILTERER.output(&chain_config.path_to_l1_foundry()),
    )
}
