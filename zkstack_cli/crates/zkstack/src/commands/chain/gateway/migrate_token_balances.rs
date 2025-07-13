use crate::{
    messages::MSG_CHAIN_NOT_INITIALIZED,
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};
use anyhow::Context;
use clap::Parser;
use ethers::{
    abi::{parse_abi, Address},
    contract::BaseContract,
};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use xshell::Shell;
use zkstack_cli_common::{
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
    logger,
    wallets::Wallet,
};
use zkstack_cli_config::{
    forge_interface::script_params::GATEWAY_MIGRATE_TOKEN_BALANCES_SCRIPT_PATH, EcosystemConfig,
};
use zksync_basic_types::U256;

lazy_static! {
    static ref GATEWAY_MIGRATE_TOKEN_BALANCES_FUNCTIONS: BaseContract = BaseContract::from(
        parse_abi(&[
            "function fundL2Address(uint256, address, address, uint256) public",
            "function startTokenMigrationOnL2(uint256, string) public",
            // "function continueMigrationOnGateway(uint256, string) public",
            "function finishMigrationOnL1(address, uint256, string) public",
            "function checkAllMigrated(uint256, string) public",
        ])
            .unwrap(),
    );
}

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct MigrateTokenBalancesArgs {
    /// All ethereum environment related arguments
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,

    #[clap(long)]
    pub gateway_chain_name: String,

    #[clap(long, default_missing_value = "true", num_args = 0..=1)]
    pub run_initial: Option<bool>,

    #[clap(long, default_missing_value = "true", num_args = 0..=1)]
    pub to_gateway: Option<bool>,
}

pub async fn run(args: MigrateTokenBalancesArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    let chain_name = global_config().chain_name.clone();
    let chain_config = ecosystem_config
        .load_chain(chain_name)
        .context(MSG_CHAIN_NOT_INITIALIZED)?;

    let gateway_chain_config = ecosystem_config
        .load_chain(Some(args.gateway_chain_name.clone()))
        .context("Gateway not present")?;
    // let gateway_chain_id = gateway_chain_config.chain_id.as_u64();
    // let gateway_gateway_config = gateway_chain_config
    //     .get_gateway_config()
    //     .context("Gateway config not present")?;

    let l1_url = chain_config.get_secrets_config().await?.l1_rpc_url()?;

    let general_chain_config = chain_config.get_general_config().await?;
    let l2_url = general_chain_config.l2_http_url()?;

    // let genesis_config = chain_config.get_genesis_config().await?;
    // let gateway_contract_config = gateway_chain_config.get_contracts_config()?;

    // let chain_contracts_config = chain_config.get_contracts_config().unwrap();

    logger::info("Migrating the chain to the Gateway...");

    let general_config = gateway_chain_config.get_general_config().await?;
    let gw_rpc_url = general_config.l2_http_url()?;

    // let chain_secrets_config = chain_config.get_wallets_config().unwrap();

    if !args.to_gateway.unwrap_or(true) {
        return Ok(()); // add migrate back from gateway
    }

    migrate_token_balances_from_gateway(
        shell,
        args.run_initial.unwrap_or(true),
        &args.forge_args.clone(),
        &ecosystem_config.path_to_l1_foundry(),
        ecosystem_config
            .get_wallets()?
            .deployer
            .context("Missing deployer wallet")?,
        ecosystem_config
            .get_contracts_config()?
            .ecosystem_contracts
            .bridgehub_proxy_addr,
        chain_config.chain_id.as_u64(),
        l1_url.clone(),
        gw_rpc_url.clone(),
        l2_url.clone(),
    )
    .await?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn migrate_token_balances_from_gateway(
    shell: &Shell,
    run_initial: bool,
    forge_args: &ForgeScriptArgs,
    foundry_scripts_path: &Path,
    wallet: Wallet,
    l1_bridgehub_addr: Address,
    l2_chain_id: u64,
    l1_rpc_url: String,
    _gw_rpc_url: String,
    l2_rpc_url: String,
) -> anyhow::Result<()> {
    println!("l2_chain_id: {}", l2_chain_id);
    println!("wallet.address: {}", wallet.address.to_string());

    let calldata = GATEWAY_MIGRATE_TOKEN_BALANCES_FUNCTIONS
        .encode(
            "fundL2Address",
            (
                U256::from(l2_chain_id),
                l1_bridgehub_addr,
                wallet.address,
                U256::from(0),
            ),
        )
        .unwrap();

    let mut forge = Forge::new(foundry_scripts_path)
        .script(
            &PathBuf::from(GATEWAY_MIGRATE_TOKEN_BALANCES_SCRIPT_PATH),
            forge_args.clone(),
        )
        .with_ffi()
        .with_rpc_url(l1_rpc_url.clone())
        .with_broadcast()
        .with_slow()
        .with_calldata(&calldata);

    // Governor private key is required for this script
    if run_initial {
        forge = fill_forge_private_key(forge, Some(&wallet), WalletOwner::Deployer)?;
        check_the_balance(&forge).await?;
        forge.run(shell)?;
        println!("Account funded");
    }

    let calldata = GATEWAY_MIGRATE_TOKEN_BALANCES_FUNCTIONS
        .encode(
            "startTokenMigrationOnL2",
            (U256::from(l2_chain_id), l2_rpc_url.clone()),
        )
        .unwrap();

    let mut forge = Forge::new(foundry_scripts_path)
        .script(
            &PathBuf::from(GATEWAY_MIGRATE_TOKEN_BALANCES_SCRIPT_PATH),
            forge_args.clone(),
        )
        .with_ffi()
        .with_rpc_url(l2_rpc_url.clone())
        .with_broadcast()
        .with_zksync()
        .with_slow()
        .with_calldata(&calldata);

    // Governor private key is required for this script
    if run_initial {
        forge = fill_forge_private_key(forge, Some(&wallet), WalletOwner::Deployer)?;
        forge.run(shell)?;

        println!("Token migration started");
    }

    let calldata = GATEWAY_MIGRATE_TOKEN_BALANCES_FUNCTIONS
        .encode(
            "finishMigrationOnL1",
            (
                l1_bridgehub_addr,
                U256::from(l2_chain_id),
                l2_rpc_url.clone(),
            ),
        )
        .unwrap();

    let mut forge = Forge::new(foundry_scripts_path)
        .script(
            &PathBuf::from(GATEWAY_MIGRATE_TOKEN_BALANCES_SCRIPT_PATH),
            forge_args.clone(),
        )
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_broadcast()
        .with_slow()
        .with_no_cache()
        .with_calldata(&calldata);

    // Governor private key is required for this script
    forge = fill_forge_private_key(forge, Some(&wallet), WalletOwner::Deployer)?;
    forge.run(shell)?;

    println!("Token migration finished");

    let calldata = GATEWAY_MIGRATE_TOKEN_BALANCES_FUNCTIONS
        .encode(
            "checkAllMigrated",
            (U256::from(l2_chain_id), l2_rpc_url.clone()),
        )
        .unwrap();

    let mut forge = Forge::new(foundry_scripts_path)
        .script(
            &PathBuf::from(GATEWAY_MIGRATE_TOKEN_BALANCES_SCRIPT_PATH),
            forge_args.clone(),
        )
        .with_ffi()
        .with_rpc_url(l2_rpc_url.clone())
        .with_broadcast()
        .with_zksync()
        .with_slow()
        .with_calldata(&calldata);

    // Governor private key is required for this script
    if run_initial {
        forge = fill_forge_private_key(forge, Some(&wallet), WalletOwner::Deployer)?;
        forge.run(shell)?;
    }

    println!("Token migration checked");

    Ok(())
}
