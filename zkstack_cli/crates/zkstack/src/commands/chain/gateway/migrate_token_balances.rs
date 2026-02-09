use std::{collections::HashMap, sync::Arc};

use anyhow::{bail, Context};
use clap::Parser;
use ethers::{
    abi::{Address, ParamType, Token},
    contract::{BaseContract, Contract},
    middleware::SignerMiddleware,
    providers::{Http, Middleware, Provider},
    signers::Signer,
    types::{BlockId, BlockNumber, Filter, H256 as EthersH256, U64},
    utils::{hex, keccak256},
};
use futures::stream::{FuturesUnordered, StreamExt};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_common::{
    config::global_config,
    ethereum::{get_ethers_provider, get_zk_client},
    forge::ForgeScriptArgs,
    logger,
    wallets::Wallet,
    zks_provider::{FinalizeWithdrawalParams, ZKSProvider},
};
use zkstack_cli_config::ZkStackConfig;
use zksync_basic_types::U256;
use zksync_system_constants::{
    GW_ASSET_TRACKER_ADDRESS, L2_ASSET_ROUTER_ADDRESS, L2_ASSET_TRACKER_ADDRESS,
    L2_NATIVE_TOKEN_VAULT_ADDRESS,
};
use zksync_types::{L2ChainId, H256};

use crate::{
    abi::{
        BridgehubAbi, MessageRootAbi, ZkChainAbi, IL1ASSETROUTERABI_ABI, IL1ASSETTRACKERABI_ABI,
        IL1NATIVETOKENVAULTABI_ABI, IL2ASSETROUTERABI_ABI, IL2NATIVETOKENVAULTABI_ABI,
    },
    commands::dev::commands::{rich_account, rich_account::args::RichAccountArgs},
    messages::MSG_CHAIN_NOT_INITIALIZED,
};

lazy_static! {
    static ref L2_NTV_FUNCTIONS: BaseContract =
        BaseContract::from(IL2NATIVETOKENVAULTABI_ABI.clone());
    static ref L2_ASSET_ROUTER_FUNCTIONS: BaseContract =
        BaseContract::from(IL2ASSETROUTERABI_ABI.clone());
}

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct InitiateTokenBalanceMigrationArgs {
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

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct FinalizeTokenBalanceMigrationArgs {
    /// All ethereum environment related arguments
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,

    #[clap(long)]
    pub gateway_chain_name: String,

    #[clap(long, default_missing_value = "true", num_args = 0..=1)]
    pub to_gateway: Option<bool>,

    /// Start block for reading migration initiation events
    #[clap(long)]
    pub from_block: Option<u64>,

    /// End block for reading migration initiation events
    #[clap(long)]
    pub to_block: Option<u64>,
}

pub async fn run_initiate(
    args: InitiateTokenBalanceMigrationArgs,
    shell: &Shell,
) -> anyhow::Result<()> {
    let (wallet, _l1_bridgehub_addr, l2_chain_id, gw_chain_id, l1_url, l2_url, gw_rpc_url) =
        load_migration_context(shell, args.gateway_chain_name).await?;

    let to_gateway = args.to_gateway.unwrap_or(true);
    logger::info(format!(
        "Initiating the token balance migration {} the Gateway...",
        if to_gateway { "to" } else { "from" }
    ));

    initiate_token_balance_migration(
        shell,
        args.skip_funding.unwrap_or(false),
        to_gateway,
        wallet,
        l2_chain_id,
        gw_chain_id,
        l1_url.clone(),
        gw_rpc_url.clone(),
        l2_url.clone(),
    )
    .await?;

    Ok(())
}

pub async fn run_finalize(
    args: FinalizeTokenBalanceMigrationArgs,
    shell: &Shell,
) -> anyhow::Result<()> {
    let (wallet, l1_bridgehub_addr, l2_chain_id, gw_chain_id, l1_url, l2_url, gw_rpc_url) =
        load_migration_context(shell, args.gateway_chain_name).await?;

    let to_gateway = args.to_gateway.unwrap_or(true);
    logger::info(format!(
        "Finalizing the token balance migration {} the Gateway...",
        if to_gateway { "to" } else { "from" }
    ));

    finalize_token_balance_migration(
        wallet,
        l1_bridgehub_addr,
        l2_chain_id,
        gw_chain_id,
        l1_url.clone(),
        gw_rpc_url.clone(),
        l2_url.clone(),
        to_gateway,
        args.from_block,
        args.to_block,
    )
    .await?;

    Ok(())
}

const LOOK_WAITING_TIME_MS: u64 = 1600;

#[allow(clippy::too_many_arguments)]
async fn load_migration_context(
    shell: &Shell,
    gateway_chain_name: String,
) -> anyhow::Result<(Wallet, Address, u64, u64, String, String, String)> {
    let ecosystem_config = ZkStackConfig::ecosystem(shell)?;

    let chain_name = global_config().chain_name.clone();
    let chain_config = ecosystem_config
        .load_chain(chain_name)
        .context(MSG_CHAIN_NOT_INITIALIZED)?;

    let gateway_chain_config = ecosystem_config
        .load_chain(Some(gateway_chain_name))
        .context("Gateway not present")?;

    let l1_url = chain_config.get_secrets_config().await?.l1_rpc_url()?;
    let l2_url = chain_config.get_general_config().await?.l2_http_url()?;
    let gw_rpc_url = gateway_chain_config
        .get_general_config()
        .await?
        .l2_http_url()?;

    Ok((
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
        l1_url,
        l2_url,
        gw_rpc_url,
    ))
}

async fn initiate_token_balance_migration(
    shell: &Shell,
    skip_funding: bool,
    to_gateway: bool,
    wallet: Wallet,
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
    let asset_ids = get_asset_ids(&l2_rpc_url).await?;

    let l2_provider = Provider::<Http>::try_from(l2_rpc_url.as_str())?;
    let l2_chain_id = l2_provider.get_chainid().await?.as_u64();

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
    if tx_hashes.is_empty() {
        println!("No migration transactions were sent.");
    }

    Ok(())
}

async fn finalize_token_balance_migration(
    wallet: Wallet,
    l1_bridgehub_addr: Address,
    l2_chain_id: u64,
    gw_chain_id: u64,
    l1_rpc_url: String,
    gw_rpc_url: String,
    l2_rpc_url: String,
    to_gateway: bool,
    from_block: Option<u64>,
    to_block: Option<u64>,
) -> anyhow::Result<()> {
    let (tracker_addr, event_signature, migration_rpc_url, source_chain_id) = if to_gateway {
        (
            L2_ASSET_TRACKER_ADDRESS,
            "L1ToGatewayMigrationInitiated(bytes32,uint256,uint256)",
            l2_rpc_url.as_str(),
            l2_chain_id,
        )
    } else {
        (
            GW_ASSET_TRACKER_ADDRESS,
            "GatewayToL1MigrationInitiated(bytes32,uint256,uint256)",
            gw_rpc_url.as_str(),
            gw_chain_id,
        )
    };
    let expected_data_chain_id = l2_chain_id;

    let (_log_asset_ids, tx_hashes) = fetch_migration_events(
        migration_rpc_url,
        tracker_addr,
        event_signature,
        if to_gateway { None } else { Some(l2_chain_id) },
        from_block,
        to_block,
    )
    .await?;

    let finalize_params = wait_for_migration_ready(
        l1_rpc_url.clone(),
        l1_bridgehub_addr,
        migration_rpc_url,
        source_chain_id,
        &tx_hashes,
    )
    .await?;

    let mut migrated_asset_ids = Vec::new();
    if finalize_params.is_empty() {
        logger::info("No migration params found; skipping L1 finalize calls.");
    } else {
        let l1_provider = Arc::new(Provider::<Http>::try_from(l1_rpc_url.as_str())?);
        let l1_chain_id = l1_provider.get_chainid().await?.as_u64();
        let l1_signer = wallet
            .private_key
            .clone()
            .unwrap()
            .with_chain_id(l1_chain_id);
        let l1_client = Arc::new(SignerMiddleware::new(l1_provider.clone(), l1_signer));

        let bridgehub = BridgehubAbi::new(l1_bridgehub_addr, l1_provider.clone());
        let l1_asset_router_addr = bridgehub.asset_router().call().await?;
        let l1_asset_router = Contract::new(
            l1_asset_router_addr,
            IL1ASSETROUTERABI_ABI.clone(),
            l1_provider.clone(),
        );
        let l1_native_token_vault_addr: Address = l1_asset_router
            .method::<_, Address>("nativeTokenVault", ())?
            .call()
            .await?;
        let l1_native_token_vault = Contract::new(
            l1_native_token_vault_addr,
            IL1NATIVETOKENVAULTABI_ABI.clone(),
            l1_provider.clone(),
        );
        let l1_asset_tracker_addr: Address = l1_native_token_vault
            .method::<_, Address>("l1AssetTracker", ())?
            .call()
            .await?;

        let l1_asset_tracker = Contract::new(
            l1_asset_tracker_addr,
            IL1ASSETTRACKERABI_ABI.clone(),
            l1_client.clone(),
        );
        let l1_asset_tracker_base = Contract::new(
            l1_asset_tracker_addr,
            crate::abi::IASSETTRACKERBASEABI_ABI.clone(),
            l1_provider.clone(),
        );

        let expected_selector: [u8; 4] = keccak256(
            "receiveMigrationOnL1((bytes1,bool,address,uint256,bytes32,uint256,uint256,uint256,uint256))",
        )[0..4]
            .try_into()
            .expect("selector length is always 4 bytes");

        let mut next_nonce = l1_client
            .get_transaction_count(wallet.address, Some(BlockId::Number(BlockNumber::Pending)))
            .await?;

        let mut pending_txs = FuturesUnordered::new();
        for (tx_hash, maybe_params) in tx_hashes.iter().zip(finalize_params.iter()) {
            let Some(params) = maybe_params else {
                println!("No finalize params for tx hash: 0x{}", hex::encode(tx_hash));
                continue;
            };

            if params.proof.proof.is_empty() {
                println!(
                    "No withdrawal proof found for tx hash: 0x{}",
                    hex::encode(tx_hash)
                );
                continue;
            }

            let (data_chain_id, asset_id, selector) =
                decode_token_balance_migration_message(&params.message.0)?;
            if data_chain_id.as_u64() != expected_data_chain_id {
                println!(
                    "Skipping tx hash from different chain: 0x{}",
                    hex::encode(tx_hash)
                );
                continue;
            }
            if selector != expected_selector {
                println!(
                    "Unexpected function selector for tx hash: 0x{}",
                    hex::encode(tx_hash)
                );
                continue;
            }

            let already_migrated: bool = l1_asset_tracker_base
                .method::<_, bool>("tokenMigrated", (data_chain_id, asset_id))?
                .call()
                .await?;
            if already_migrated {
                println!(
                    "Token already migrated for assetId: 0x{}",
                    hex::encode(asset_id.as_bytes())
                );
                continue;
            }

            let l2_tx_number_in_batch: u16 = params
                .l2_tx_number_in_block
                .as_u64()
                .try_into()
                .context("l2_tx_number_in_block does not fit into u16")?;
            let finalize_param = (
                U256::from(source_chain_id),
                U256::from(params.l2_batch_number.as_u64()),
                U256::from(params.l2_message_index.as_u64()),
                params.sender,
                l2_tx_number_in_batch,
                params.message.clone(),
                params.proof.proof.clone(),
            );

            let call_result =
                l1_asset_tracker.method::<_, ()>("receiveMigrationOnL1", (finalize_param,));

            match call_result {
                Ok(mut call) => {
                    migrated_asset_ids.push(asset_id);
                    let gas_estimate = call
                        .estimate_gas()
                        .await
                        .unwrap_or_else(|_| U256::from(1_500_000u64));
                    let gas_limit =
                        std::cmp::max(gas_estimate * U256::from(2u64), U256::from(1_500_000u64));
                    call.tx.set_gas(gas_limit);
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
                Err(e) => println!("Warning: Failed to create L1 call: {}", e),
            }
        }

        while let Some((asset_id, receipt_res)) = pending_txs.next().await {
            match receipt_res {
                Ok(Some(receipt)) => {
                    println!(
                        "L1 tx hash for assetId 0x{}: 0x{}",
                        hex::encode(asset_id.as_bytes()),
                        hex::encode(receipt.transaction_hash)
                    );
                }
                Ok(None) => println!(
                    "Warning: L1 transaction dropped for assetId 0x{}",
                    hex::encode(asset_id.as_bytes())
                ),
                Err(e) => println!(
                    "Warning: Failed to get L1 receipt for assetId 0x{}: {}",
                    hex::encode(asset_id.as_bytes()),
                    e
                ),
            }
        }
    }

    // Wait for all tokens to be migrated
    println!("Waiting for all tokens to be migrated...");
    let l2_provider = Provider::<Http>::try_from(l2_rpc_url.as_str())?;
    let l2_chain_id = l2_provider.get_chainid().await?.as_u64();
    let l2_signer = wallet
        .private_key
        .clone()
        .unwrap()
        .with_chain_id(l2_chain_id);
    let l2_client = Arc::new(SignerMiddleware::new(l2_provider.clone(), l2_signer));
    let tracker = Contract::new(
        L2_ASSET_TRACKER_ADDRESS,
        crate::abi::IASSETTRACKERBASEABI_ABI.clone(),
        l2_client.clone(),
    );
    for asset_id in migrated_asset_ids.iter().copied() {
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

    Ok(())
}

async fn get_asset_ids(l2_rpc_url: &str) -> anyhow::Result<Vec<[u8; 32]>> {
    let mut asset_ids = Vec::new();
    let l2_provider = Provider::<Http>::try_from(l2_rpc_url)?;

    let ntv = Contract::new(
        L2_NATIVE_TOKEN_VAULT_ADDRESS,
        L2_NTV_FUNCTIONS.abi().clone(),
        Arc::new(l2_provider.clone()),
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

    Ok(asset_ids)
}

async fn fetch_migration_events(
    rpc_url: &str,
    tracker_addr: Address,
    event_signature: &str,
    chain_id_topic: Option<u64>,
    from_block: Option<u64>,
    to_block: Option<u64>,
) -> anyhow::Result<(Vec<[u8; 32]>, Vec<H256>)> {
    let provider = Provider::<Http>::try_from(rpc_url)?;
    let event_topic = EthersH256::from_slice(&keccak256(event_signature));

    let mut logs = Vec::new();
    let mut attempts = 0u32;
    let max_attempts = 10u32;
    while attempts < max_attempts {
        let resolved_from = from_block.unwrap_or(0);
        let resolved_to = match to_block {
            Some(to_block) => BlockNumber::Number(U64::from(to_block)),
            None => BlockNumber::Latest,
        };

        let mut filter = Filter::new().address(tracker_addr).topic0(event_topic);
        filter = filter.from_block(BlockNumber::Number(U64::from(resolved_from)));
        filter = filter.to_block(resolved_to);
        if let Some(chain_id) = chain_id_topic {
            filter = filter.topic2(EthersH256::from_low_u64_be(chain_id));
        }
        logs = provider.get_logs(&filter).await?;
        if !logs.is_empty() {
            break;
        }

        attempts += 1;
        tokio::time::sleep(std::time::Duration::from_millis(LOOK_WAITING_TIME_MS)).await;
    }
    let mut latest_logs: HashMap<EthersH256, (u64, u64, H256, [u8; 32])> = HashMap::new();

    for log in logs {
        let Some(asset_topic) = log.topics.get(1).copied() else {
            continue;
        };
        let Some(tx_hash) = log.transaction_hash else {
            continue;
        };
        let block_number = log.block_number.map(|b| b.as_u64()).unwrap_or(0);
        let log_index = log.log_index.map(|i| i.as_u64()).unwrap_or(0);

        match latest_logs.get(&asset_topic) {
            Some((prev_block, _, _, _)) if *prev_block > block_number => continue,
            Some((prev_block, prev_index, _, _))
                if *prev_block == block_number && *prev_index >= log_index =>
            {
                continue
            }
            _ => {
                let asset_id = asset_topic.as_bytes();
                let mut asset_bytes = [0u8; 32];
                asset_bytes.copy_from_slice(asset_id);
                latest_logs.insert(
                    asset_topic,
                    (
                        block_number,
                        log_index,
                        H256::from_slice(tx_hash.as_bytes()),
                        asset_bytes,
                    ),
                );
            }
        }
    }

    if latest_logs.is_empty() {
        logger::info("No migration events found; skipping L1 finalize calls.");
        return Ok((Vec::new(), Vec::new()));
    }

    let mut entries: Vec<([u8; 32], H256)> = latest_logs
        .values()
        .map(|(_, _, tx_hash, asset_id)| (*asset_id, *tx_hash))
        .collect();
    entries.sort_by(|(a, _), (b, _)| a.cmp(b));

    let (asset_ids, tx_hashes): (Vec<[u8; 32]>, Vec<H256>) = entries.into_iter().unzip();

    Ok((asset_ids, tx_hashes))
}

async fn wait_for_migration_ready(
    l1_rpc_url: String,
    l1_bridgehub_addr: Address,
    l2_or_gw_rpc: &str,
    source_chain_id: u64,
    tx_hashes: &[H256],
) -> anyhow::Result<Vec<Option<FinalizeWithdrawalParams>>> {
    if tx_hashes.is_empty() {
        logger::info("No migration transactions found; skipping L1 wait.");
        return Ok(Vec::new());
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

    Ok(finalize_params)
}

fn decode_token_balance_migration_message(message: &[u8]) -> anyhow::Result<(U256, H256, [u8; 4])> {
    if message.len() < 4 {
        bail!("L2->L1 message is too short");
    }

    let selector: [u8; 4] = message[0..4]
        .try_into()
        .context("Failed to read function selector")?;
    let tokens = ethers::abi::decode(
        &[ParamType::Tuple(vec![
            ParamType::FixedBytes(1),
            ParamType::Bool,
            ParamType::Address,
            ParamType::Uint(256),
            ParamType::FixedBytes(32),
            ParamType::Uint(256),
            ParamType::Uint(256),
            ParamType::Uint(256),
            ParamType::Uint(256),
        ])],
        &message[4..],
    )
    .context("Failed to decode token balance migration data")?;

    let Token::Tuple(values) = tokens
        .into_iter()
        .next()
        .context("Missing token balance migration data")?
    else {
        bail!("Invalid token balance migration data");
    };

    let chain_id = values
        .get(3)
        .and_then(|token| token.clone().into_uint())
        .context("Missing chainId")?;
    let asset_id_bytes = values
        .get(4)
        .and_then(|token| token.clone().into_fixed_bytes())
        .context("Missing assetId")?;

    Ok((chain_id, H256::from_slice(&asset_id_bytes), selector))
}
