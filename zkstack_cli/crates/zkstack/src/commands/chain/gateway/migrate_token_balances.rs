use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context;
use clap::Parser;
use ethers::{
    abi::Address,
    contract::{BaseContract, Contract},
    middleware::SignerMiddleware,
    providers::{Http, Middleware, Provider},
    signers::Signer,
    types::{BlockId, BlockNumber},
    utils::hex,
};
use futures::stream::{FuturesUnordered, StreamExt};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_common::{
    config::global_config,
    ethereum::{get_ethers_provider, get_zk_client},
    forge::{Forge, ForgeScriptArgs},
    logger,
    wallets::Wallet,
    zks_provider::ZKSProvider,
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
use zksync_types::{L2ChainId, H256};

use crate::{
    abi::{
        BridgehubAbi, MessageRootAbi, ZkChainAbi, IGATEWAYMIGRATETOKENBALANCESABI_ABI,
        IL2ASSETROUTERABI_ABI, IL2NATIVETOKENVAULTABI_ABI,
    },
    commands::dev::commands::{rich_account, rich_account::args::RichAccountArgs},
    messages::MSG_CHAIN_NOT_INITIALIZED,
    utils::forge::{fill_forge_private_key, WalletOwner},
};

lazy_static! {
    static ref GATEWAY_MIGRATE_TOKEN_BALANCES_FUNCTIONS: BaseContract =
        BaseContract::from(IGATEWAYMIGRATETOKENBALANCESABI_ABI.clone());
    static ref L2_NTV_FUNCTIONS: BaseContract =
        BaseContract::from(IL2NATIVETOKENVAULTABI_ABI.clone());
    static ref L2_ASSET_ROUTER_FUNCTIONS: BaseContract =
        BaseContract::from(IL2ASSETROUTERABI_ABI.clone());
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
    pub skip_funding: Option<bool>,

    #[clap(long, default_missing_value = "true", num_args = 0..=1)]
    pub to_gateway: Option<bool>,
}

// sma todo: this script should be broken down into multiple steps
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

const LOOK_WAITING_TIME_MS: u64 = 1600;

#[allow(clippy::too_many_arguments)]
pub async fn migrate_token_balances_from_gateway(
    shell: &Shell,
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

    if !skip_funding {
        let (rpc_url, chain_id, label) = if to_gateway {
            (&l2_rpc_url, l2_chain_id, "L2")
        } else {
            (&gw_rpc_url, gw_chain_id, "Gateway")
        };

        let provider = Provider::<Http>::try_from(rpc_url.as_str())?;
        let balance = provider.get_balance(wallet.address, None).await?;

        if balance.is_zero() {
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
                Some(L2ChainId::from(chain_id as u32)),
            )
            .await?;

            println!("Waiting for {label} account to be funded...");
            loop {
                let balance = provider.get_balance(wallet.address, None).await?;
                if !balance.is_zero() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(LOOK_WAITING_TIME_MS)).await;
            }
            println!("{label} account funded");
        } else {
            println!("{label} account already funded, skipping");
        }
    }

    let mut tx_hashes = Vec::new();
    let mut asset_ids = Vec::new();
    let l2_provider = Provider::<Http>::try_from(l2_rpc_url.as_str())?;
    let l2_chain_id = l2_provider.get_chainid().await?.as_u64();

    let l2_signer = wallet
        .private_key
        .clone()
        .unwrap()
        .with_chain_id(l2_chain_id);
    let l2_client = Arc::new(SignerMiddleware::new(l2_provider.clone(), l2_signer));

    // Get bridged token count and asset IDs
    let ntv = Contract::new(
        L2_NATIVE_TOKEN_VAULT_ADDRESS,
        L2_NTV_FUNCTIONS.abi().clone(),
        l2_client.clone(),
    );
    let count: U256 = ntv
        .method::<_, U256>("bridgedTokensCount", ())?
        .call()
        .await?;

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
        L2_ASSET_ROUTER_FUNCTIONS.abi().clone(),
        Arc::new(l2_provider),
    );
    let base_token_asset_id = router
        .method::<_, [u8; 32]>("BASE_TOKEN_ASSET_ID", ())?
        .call()
        .await?;
    asset_ids.push(base_token_asset_id);

    // Migrate each token
    let (tracker_addr, tracker_abi) = if to_gateway {
        (
            L2_ASSET_TRACKER_ADDRESS,
            crate::abi::IL2ASSETTRACKERABI_ABI.clone(),
        )
    } else {
        (
            GW_ASSET_TRACKER_ADDRESS,
            crate::abi::IGWASSETTRACKERABI_ABI.clone(),
        )
    };

    let rpc_url = if to_gateway { &l2_rpc_url } else { &gw_rpc_url };
    let provider = Provider::<Http>::try_from(rpc_url.as_str())?;
    let chain_id = provider.get_chainid().await?.as_u64();

    let signer = wallet.private_key.clone().unwrap().with_chain_id(chain_id);
    let client = Arc::new(SignerMiddleware::new(provider.clone(), signer));
    let mut next_nonce = client
        .get_transaction_count(wallet.address, Some(BlockId::Number(BlockNumber::Pending)))
        .await?;
    let tracker = Contract::new(tracker_addr, tracker_abi, client);

    // Send all initiate migration transactions
    let mut pending_txs = FuturesUnordered::new();
    for asset_id in asset_ids.iter().copied() {
        println!(
            "Migrating token balance for assetId: 0x{}",
            hex::encode(asset_id)
        );

        let call_result = if to_gateway {
            tracker.method::<_, ()>("initiateL1ToGatewayMigrationOnL2", (asset_id,))
        } else {
            tracker.method::<_, ()>(
                "initiateGatewayToL1MigrationOnGateway",
                (U256::from(l2_chain_id), asset_id),
            )
        };

        match call_result {
            Ok(mut call) => {
                call.tx.set_nonce(next_nonce);
                next_nonce = next_nonce + U256::from(1u64);

                pending_txs.push(async move {
                    match call.send().await {
                        Ok(pending_tx) => (asset_id, pending_tx.await),
                        Err(e) => {
                            println!("Warning: Failed to migrate asset: {}", e);
                            (asset_id, Ok(None))
                        }
                    }
                });
            }
            Err(e) => println!("Warning: Failed to create method call: {}", e),
        }
    }

    // Wait for all txs to complete
    while let Some((asset_id, receipt_res)) = pending_txs.next().await {
        match receipt_res {
            Ok(Some(receipt)) => {
                tx_hashes.push(receipt.transaction_hash);
                println!(
                    "Transaction hash for assetId 0x{}: 0x{}",
                    hex::encode(asset_id),
                    hex::encode(receipt.transaction_hash)
                );
            }
            Ok(None) => println!(
                "Warning: Transaction dropped for assetId 0x{}",
                hex::encode(asset_id)
            ),
            Err(e) => println!(
                "Warning: Failed to get receipt for assetId 0x{}: {}",
                hex::encode(asset_id),
                e
            ),
        }
    }

    println!("Token migration started");

    let (migration_rpc_url, source_chain_id) = if to_gateway {
        (l2_rpc_url.as_str(), l2_chain_id)
    } else {
        (gw_rpc_url.as_str(), gw_chain_id)
    };

    wait_for_migration_ready(
        l1_rpc_url.clone(),
        l1_bridgehub_addr,
        migration_rpc_url,
        source_chain_id,
        &tx_hashes,
    )
    .await?;

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
        .with_gas_per_pubdata(8000)
        .with_calldata(&calldata);

    // Governor private key is required for this script
    forge = fill_forge_private_key(forge, Some(&wallet), WalletOwner::Deployer)?;
    forge.run(shell)?;

    // Wait for all tokens to be migrated
    println!("Waiting for all tokens to be migrated...");
    let tracker = Contract::new(
        L2_ASSET_TRACKER_ADDRESS,
        crate::abi::IASSETTRACKERBASEABI_ABI.clone(),
        l2_client.clone(),
    );
    for asset_id in asset_ids.iter().copied() {
        loop {
            let asset_is_migrated = tracker
                .method::<_, bool>("tokenMigratedThisChain", asset_id)?
                .call()
                .await?;
            if asset_is_migrated {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(LOOK_WAITING_TIME_MS)).await;
        }
    }

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

async fn wait_for_migration_ready(
    l1_rpc_url: String,
    l1_bridgehub_addr: Address,
    l2_or_gw_rpc: &str,
    source_chain_id: u64,
    tx_hashes: &[H256],
) -> anyhow::Result<()> {
    if tx_hashes.is_empty() {
        logger::info("No migration transactions found; skipping L1 wait.");
        return Ok(());
    }
    println!("Waiting for migration to be ready...");

    let l1_provider = get_ethers_provider(&l1_rpc_url)?;
    let zk_client = get_zk_client(l2_or_gw_rpc, source_chain_id)?;

    let bridgehub = BridgehubAbi::new(l1_bridgehub_addr, l1_provider.clone());
    let message_root_addr = bridgehub.message_root().call().await?;
    let message_root = MessageRootAbi::new(message_root_addr, l1_provider.clone());

    let mut finalize_params = Vec::new();
    for tx_hash in tx_hashes {
        // Wait for withdrawal proof to exist
        let params = loop {
            match zk_client.get_finalize_withdrawal_params(*tx_hash, 0).await {
                Ok(p) => break Some(p),
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("Log not found") || msg.contains("L2ToL1Log not found") {
                        println!("No L2->L1 log for tx hash: 0x{}", hex::encode(tx_hash));
                        break None;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(LOOK_WAITING_TIME_MS))
                        .await;
                }
            }
        };

        finalize_params.push(params.clone());
        let Some(params) = params else { continue };

        if params.proof.proof.is_empty() {
            println!(
                "No withdrawal proof found for tx hash: 0x{}",
                hex::encode(tx_hash)
            );
            continue;
        }

        let proof_data = message_root
            .get_proof_data(
                source_chain_id.into(),
                params.l2_batch_number.as_u64().into(),
                params.l2_message_index.as_u64().into(),
                H256::zero().into(),
                params.proof.proof.iter().map(|h| (*h).into()).collect(),
            )
            .call()
            .await?;

        let settlement_chain_id = proof_data.settlement_layer_chain_id;
        let settlement_batch_number = proof_data.settlement_layer_batch_number;
        let (chain_id, batch_number) = if settlement_chain_id != U256::from(source_chain_id)
            && !settlement_chain_id.is_zero()
        {
            (
                settlement_chain_id.as_u64(),
                settlement_batch_number.as_u64(),
            )
        } else {
            (source_chain_id, params.l2_batch_number.as_u64())
        };

        // Wait for batch to be executed on L1
        let zk_chain_addr: Address = bridgehub
            .method("getZKChain", U256::from(chain_id))?
            .call()
            .await?;
        let getters = ZkChainAbi::new(zk_chain_addr, l1_provider.clone());

        loop {
            let total_batches_executed: U256 = getters.get_total_batches_executed().call().await?;
            if total_batches_executed >= U256::from(batch_number) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(LOOK_WAITING_TIME_MS)).await;
            println!("Waiting for batch to be executed on L1");
        }
    }

    Ok(())
}
