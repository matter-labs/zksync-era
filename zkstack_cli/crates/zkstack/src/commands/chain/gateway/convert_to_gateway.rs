use anyhow::Context;
use ethers::{
    abi::{parse_abi, Address},
    contract::BaseContract,
    utils::hex,
};
use lazy_static::lazy_static;
use xshell::Shell;
use zkstack_cli_common::{
    config::global_config,
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
    override_config,
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    ChainConfig, EcosystemConfig, GatewayConfig, ZkStackConfig,
};
use zkstack_cli_types::ProverMode;

use crate::{
    admin_functions::{
        governance_execute_calls, grant_gateway_whitelist, revoke_gateway_whitelist,
        set_transaction_filterer, AdminScriptMode,
    },
    consts::PATH_TO_GATEWAY_OVERRIDE_CONFIG,
    messages::MSG_CHAIN_NOT_INITIALIZED,
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

lazy_static! {
    static ref DEPLOY_GATEWAY_TX_FILTERER_ABI: BaseContract =
        BaseContract::from(parse_abi(&["function runWithInputFromFile() public"]).unwrap(),);
    static ref GATEWAY_VOTE_PREPARATION_ABI: BaseContract =
        BaseContract::from(parse_abi(&["function run() public"]).unwrap(),);
}

pub async fn run(args: ForgeScriptArgs, shell: &Shell) -> anyhow::Result<()> {
    let chain_name = global_config().chain_name.clone();
    let ecosystem_config = ZkStackConfig::ecosystem(shell)?;
    let chain_config = ecosystem_config
        .load_chain(chain_name)
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    let l1_url = chain_config.get_secrets_config().await?.l1_rpc_url()?;
    let mut chain_contracts_config = chain_config.get_contracts_config()?;
    let chain_genesis_config = chain_config.get_genesis_config().await?;
    let genesis_input = GenesisInput::new(&chain_genesis_config)?;
    override_config(
        shell,
        &ecosystem_config
            .link_to_code
            .join(PATH_TO_GATEWAY_OVERRIDE_CONFIG),
        &chain_config,
    )?;

    let chain_deployer_wallet = chain_config
        .get_wallets_config()?
        .deployer
        .context("deployer")?;

    let output = deploy_gateway_tx_filterer(
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

    let grantees = vec![
        ecosystem_config.get_contracts_config()?.l1.governance_addr,
        chain_deployer_wallet.address,
        // This is only for local deployment, to allow easier future testing
        ecosystem_config.get_wallets()?.deployer.unwrap().address,
        chain_contracts_config
            .ecosystem_contracts
            .stm_deployment_tracker_proxy_addr
            .context("No CTM deployment tracker")?,
    ];

    grant_gateway_whitelist(
        shell,
        &args,
        &chain_config.path_to_l1_foundry(),
        AdminScriptMode::Broadcast(chain_config.get_wallets_config()?.governor),
        chain_config.chain_id.as_u64(),
        chain_contracts_config
            .ecosystem_contracts
            .bridgehub_proxy_addr,
        grantees,
        l1_url.clone(),
    )
    .await?;

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
    )
    .await?;

    // Now, we will need to execute the corresponding governance calls
    // These calls will produce some L1->L2 transactions. However tracking those is hard at this point, so we won't do it here.
    governance_execute_calls(
        shell,
        &ecosystem_config,
        &ecosystem_config.get_wallets()?.governor,
        hex::decode(&vote_preparation_output.governance_calls_to_execute).unwrap(),
        &args,
        l1_url.clone(),
    )
    .await?;

    let gateway_config: GatewayConfig = vote_preparation_output.into();

    gateway_config.save_with_base_path(shell, chain_config.configs.clone())?;

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
) -> anyhow::Result<DeployGatewayCTMOutput> {
    input.save(
        shell,
        GATEWAY_VOTE_PREPARATION.input(&chain_config.path_to_l1_foundry()),
    )?;

    let mut forge = Forge::new(&config.path_to_l1_foundry())
        .script(&GATEWAY_VOTE_PREPARATION.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_calldata(&GATEWAY_VOTE_PREPARATION_ABI.encode("run", ()).unwrap())
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
