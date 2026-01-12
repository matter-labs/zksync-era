use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context;
use clap::Parser;
use ethers::{
    abi::{parse_abi, Address},
    contract::{BaseContract, Contract},
    middleware::SignerMiddleware,
    providers::{Http, Middleware, Provider},
    signers::Signer,
    utils::hex,
};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_common::{
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
    logger,
    wallets::Wallet,
};
use zkstack_cli_config::{
    forge_interface::script_params::GATEWAY_MIGRATE_TOKEN_BALANCES_SCRIPT_PATH, ZkStackConfig,
    ZkStackConfigTrait,
};
use zksync_basic_types::U256;
use zksync_system_constants::{
    GW_ASSET_TRACKER_ADDRESS, L2_ASSET_ROUTER_ADDRESS, L2_ASSET_TRACKER_ADDRESS,
    L2_NATIVE_TOKEN_VAULT_ADDRESS,
};
use zksync_types::L2ChainId;

use crate::{
    commands::dev::commands::{rich_account, rich_account::args::RichAccountArgs},
    messages::MSG_CHAIN_NOT_INITIALIZED,
    utils::forge::{fill_forge_private_key, WalletOwner},
};

lazy_static! {
    static ref GATEWAY_MIGRATE_TOKEN_BALANCES_FUNCTIONS: BaseContract = BaseContract::from(
        parse_abi(&[
            "function finishMigrationOnL1(bool, address, uint256, uint256, string, string, bool, bytes32[]) public",
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
    pub skip_funding: Option<bool>,

    #[clap(long, default_missing_value = "true", num_args = 0..=1)]
    pub to_gateway: Option<bool>,
}

pub async fn run(args: MigrateTokenBalancesArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = ZkStackConfig::ecosystem(shell)?;

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

    logger::info(format!(
        "Migrating the token balances {} the Gateway...",
        if args.to_gateway.unwrap_or(true) {
            "to"
        } else {
            "from"
        }
    ));

    let general_config = gateway_chain_config.get_general_config().await?;
    let gw_rpc_url = general_config.l2_http_url()?;

    // let chain_secrets_config = chain_config.get_wallets_config().unwrap();

    migrate_token_balances_from_gateway(
        shell,
        args.run_initial.unwrap_or(true),
        args.skip_funding.unwrap_or(false),
        &args.forge_args.clone(),
        args.to_gateway.unwrap_or(true),
        &chain_config.path_to_foundry_scripts(),
        ecosystem_config
            .get_wallets()?
            .deployer
            .context("Missing deployer wallet")?,
        ecosystem_config
            .get_contracts_config()?
            .core_ecosystem_contracts
            .bridgehub_proxy_addr,
        chain_config.chain_id.as_u64(),
        gateway_chain_config.chain_id.as_u64(),
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
    skip_funding: bool,
    forge_args: &ForgeScriptArgs,
    to_gateway: bool,
    foundry_scripts_path: &Path,
    wallet: Wallet,
    l1_bridgehub_addr: Address,
    l2_chain_id: u64,
    gw_chain_id: u64,
    l1_rpc_url: String,
    gw_rpc_url: String,
    l2_rpc_url: String,
) -> anyhow::Result<()> {
    println!("l2_chain_id: {}", l2_chain_id);
    println!("wallet.address: {}", wallet.address);

    if run_initial && !skip_funding {
        rich_account::run(
            shell,
            RichAccountArgs {
                l2_account: Some(wallet.address),
                l1_account_private_key: Some(
                    "0x7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110"
                        .to_string(),
                ),
                l1_rpc_url: Some(l1_rpc_url.clone()),
                amount: Some(U256::from(1_000_000_000_000_000_000u64)),
            },
            Some(L2ChainId::from(l2_chain_id as u32)),
        )
        .await?;
        rich_account::run(
            shell,
            RichAccountArgs {
                l2_account: Some(wallet.address),
                l1_account_private_key: Some(
                    "0x7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110"
                        .to_string(),
                ),
                l1_rpc_url: Some(l1_rpc_url.clone()),
                amount: Some(U256::from(1_000_000_000_000_000_000u64)),
            },
            Some(L2ChainId::from(gw_chain_id as u32)),
        )
        .await?;
        std::thread::sleep(std::time::Duration::from_secs(20));

        println!("Account funded");
    }

    let mut tx_hashes = Vec::new();
    if run_initial {
        let rpc_url = if to_gateway { &l2_rpc_url } else { &gw_rpc_url };
        let provider = Provider::<Http>::try_from(rpc_url.as_str())?;
        let chain_id = provider.get_chainid().await?.as_u64();

        let signer = wallet.private_key.clone().unwrap().with_chain_id(chain_id);
        let client = Arc::new(SignerMiddleware::new(provider.clone(), signer));

        // Get bridged token count and asset IDs
        let ntv = Contract::new(
            L2_NATIVE_TOKEN_VAULT_ADDRESS,
            parse_abi(&[
                "function bridgedTokensCount() view returns (uint256)",
                "function bridgedTokens(uint256) view returns (bytes32)",
            ])?,
            client.clone(),
        );
        let count: U256 = ntv
            .method::<_, U256>("bridgedTokensCount", ())?
            .call()
            .await?;

        let mut asset_ids = Vec::new();
        for i in 0..count.as_u64() {
            asset_ids.push(
                ntv.method::<_, [u8; 32]>("bridgedTokens", U256::from(i))?
                    .call()
                    .await?,
            );
        }

        // Add base token asset ID
        let router = Contract::new(
            L2_ASSET_ROUTER_ADDRESS,
            parse_abi(&["function BASE_TOKEN_ASSET_ID() view returns (bytes32)"])?,
            Arc::new(provider),
        );
        asset_ids.push(
            router
                .method::<_, [u8; 32]>("BASE_TOKEN_ASSET_ID", ())?
                .call()
                .await?,
        );

        // Migrate each token
        let (tracker_addr, method_sig) = if to_gateway {
            (
                L2_ASSET_TRACKER_ADDRESS,
                "function initiateL1ToGatewayMigrationOnL2(bytes32)",
            )
        } else {
            (
                GW_ASSET_TRACKER_ADDRESS,
                "function initiateGatewayToL1MigrationOnGateway(uint256,bytes32)",
            )
        };

        let tracker = Contract::new(tracker_addr, parse_abi(&[method_sig])?, client);

        for asset_id in asset_ids {
            println!(
                "Migrating token balance for assetId: 0x{}",
                hex::encode(asset_id)
            );
            if to_gateway {
                match tracker.method::<_, ()>("initiateL1ToGatewayMigrationOnL2", (asset_id,)) {
                    Ok(call) => match call.send().await {
                        Ok(pending_tx) => {
                            if let Some(receipt) = pending_tx.await? {
                                tx_hashes.push(receipt.transaction_hash);
                                println!(
                                    "Transaction hash: {:?}",
                                    U256::from(receipt.transaction_hash.0)
                                );
                            }
                        }
                        Err(e) => println!("Warning: Failed to migrate asset: {}", e),
                    },
                    Err(e) => println!("Warning: Failed to create method call: {}", e),
                }
            } else {
                // Similar for the other branch
                match tracker.method::<_, ()>(
                    "initiateGatewayToL1MigrationOnGateway",
                    (U256::from(l2_chain_id), asset_id),
                ) {
                    Ok(call) => match call.send().await {
                        Ok(pending_tx) => {
                            if let Some(receipt) = pending_tx.await? {
                                tx_hashes.push(receipt.transaction_hash);
                            }
                        }
                        Err(e) => println!("Warning: Failed to migrate asset: {}", e),
                    },
                    Err(e) => println!("Warning: Failed to create method call: {}", e),
                }
            }
        }

        println!("Token migration started");
    }

    let calldata = GATEWAY_MIGRATE_TOKEN_BALANCES_FUNCTIONS
        .encode(
            "finishMigrationOnL1",
            (
                to_gateway,
                l1_bridgehub_addr,
                U256::from(l2_chain_id),
                U256::from(gw_chain_id),
                l2_rpc_url.clone(),
                gw_rpc_url.clone(),
                true,
                tx_hashes.clone(),
            ),
        )
        .unwrap();

    let forge = Forge::new(foundry_scripts_path)
        .script(
            &PathBuf::from(GATEWAY_MIGRATE_TOKEN_BALANCES_SCRIPT_PATH),
            forge_args.clone(),
        )
        .with_ffi()
        .with_rpc_url(l1_rpc_url.clone())
        .with_broadcast()
        .with_slow()
        .with_gas_per_pubdata(8000)
        .with_calldata(&calldata);

    // Governor private key is required for this script
    // forge = fill_forge_private_key(forge, Some(&wallet), WalletOwner::Deployer)?;
    forge.run(shell)?;

    let calldata = GATEWAY_MIGRATE_TOKEN_BALANCES_FUNCTIONS
        .encode(
            "finishMigrationOnL1",
            (
                to_gateway,
                l1_bridgehub_addr,
                U256::from(l2_chain_id),
                U256::from(gw_chain_id),
                l2_rpc_url.clone(),
                gw_rpc_url.clone(),
                false,
                tx_hashes,
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
        .with_gas_per_pubdata(8000)
        .with_calldata(&calldata);

    // Governor private key is required for this script
    forge = fill_forge_private_key(forge, Some(&wallet), WalletOwner::Deployer)?;
    forge.run(shell)?;

    std::thread::sleep(std::time::Duration::from_secs(30));

    println!("Token migration finished");

    // let calldata = GATEWAY_MIGRATE_TOKEN_BALANCES_FUNCTIONS
    //     .encode(
    //         "checkAllMigrated",
    //         (U256::from(l2_chain_id), l2_rpc_url.clone()),
    //     )
    //     .unwrap();

    // let mut forge = Forge::new(foundry_scripts_path)
    //     .script(
    //         &PathBuf::from(GATEWAY_MIGRATE_TOKEN_BALANCES_SCRIPT_PATH),
    //         forge_args.clone(),
    //     )
    //     .with_ffi()
    //     .with_rpc_url(l2_rpc_url.clone())
    //     .with_broadcast()
    //     .with_zksync()
    //     .with_slow()
    //     .with_gas_per_pubdata(8000)
    //     .with_calldata(&calldata);

    // // Governor private key is required for this script
    // if run_initial {
    //     forge = fill_forge_private_key(forge, Some(&wallet), WalletOwner::Deployer)?;
    //     forge.run(shell)?;
    // }

    // println!("Token migration checked");

    Ok(())
}
