//! Binary to process AppendedChainBatchRoot events for BatchRootProcessor.
//!
//! This utility is used to recover batch roots that were missed during normal operation.
//! It fetches logs from the settlement layer for specified blocks and saves the batch-chain merkle paths to the database.
//!
//! # Usage
//!
//! Set the following environment variables:
//! - `DATABASE_URL`: PostgreSQL connection URL (e.g., `postgres://user:pass@localhost/dbname`)
//! - `SL_CLIENT_URL`: Settlement Layer (Gateway) RPC URL
//! - `L2_CHAIN_ID`: L2 chain ID as a u64
//! - `FROM_BLOCK`: Starting block number to fetch events from (required)
//! - `TO_BLOCK`: Ending block number to fetch events to (required)

use std::{env, str::FromStr, sync::Arc};

use anyhow::Context as _;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_eth_watch::{
    BatchRootProcessor, EthClient, EthHttpQueryClient, EventProcessor, ZkSyncExtentionEthClient,
};
use zksync_mini_merkle_tree::MiniMerkleTree;
use zksync_types::{
    ethabi, url::SensitiveUrl, web3::BlockNumber, Address, L1BatchNumber, L2ChainId, H256, U64,
};
use zksync_web3_decl::client::{Client, DynClient, L2};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Get configuration from environment variables
    let database_url =
        env::var("DATABASE_URL").context("DATABASE_URL environment variable is required")?;
    let sl_client_url = env::var("SL_CLIENT_URL")
        .context("SL_CLIENT_URL environment variable is required (Gateway RPC URL)")?;
    let l2_chain_id = env::var("L2_CHAIN_ID")
        .context("L2_CHAIN_ID environment variable is required")?
        .parse::<u64>()
        .context("L2_CHAIN_ID must be a valid u64")?;
    let l2_chain_id = L2ChainId::new(l2_chain_id)
        .map_err(|e| anyhow::anyhow!("L2_CHAIN_ID must be a valid chain ID: {}", e))?;
    let diamond_proxy_addr = Address::from_str(
        &env::var("DIAMOND_PROXY_ADDR")
            .context("DIAMOND_PROXY_ADDR environment variable is required")?,
    )
    .map_err(|e| anyhow::anyhow!("DIAMOND_PROXY_ADDR must be a valid address: {}", e))?;

    // Get block range from environment variables
    let from_block = env::var("FROM_BLOCK")
        .context("FROM_BLOCK environment variable is required")?
        .parse::<u64>()
        .context("FROM_BLOCK must be a valid u64")?;
    let to_block = env::var("TO_BLOCK")
        .context("TO_BLOCK environment variable is required")?
        .parse::<u64>()
        .context("TO_BLOCK must be a valid u64")?;

    // Create database connection pool
    let db_url = SensitiveUrl::from_str(&database_url).context("Failed to parse DATABASE_URL")?;
    let pool = ConnectionPool::<Core>::singleton(db_url)
        .build()
        .await
        .context("Failed to create database connection pool")?;

    // Create SL client
    let sl_url = SensitiveUrl::from_str(&sl_client_url).context("Failed to parse SL_CLIENT_URL")?;
    let client: Box<DynClient<L2>> = Box::new(
        Client::http(sl_url)
            .context("Failed to create HTTP client")?
            .for_network(l2_chain_id.into())
            .build(),
    );

    // Create EthHttpQueryClient with minimal contract addresses
    // For BatchRootProcessor, we mainly need the client for fetching chain proofs,
    // so we can use dummy addresses for contracts that aren't used
    let sl_client: Arc<dyn ZkSyncExtentionEthClient> = Arc::new(EthHttpQueryClient::new(
        client,
        diamond_proxy_addr, // diamond_proxy_addr
        None,               // bytecode_supplier_addr
        None,               // wrapped_base_token_store
        None,               // l1_shared_bridge_addr
        None,               // l1_message_root_address
        None,               // state_transition_manager_address
        None,               // chain_admin_address
        None,               // server_notifier_address
        None,               // confirmations_for_eth_event
        l2_chain_id,
    ));

    // Initialize state from database
    let mut storage = pool.connection_tagged("block_root_processor").await?;
    let mut storage = storage.start_transaction().await?;
    let sl_chain_id = sl_client.chain_id().await?;
    let batch_hashes = storage
        .blocks_dal()
        .get_executed_batch_roots_on_sl(sl_chain_id)
        .await?;

    let chain_batch_root_number_lower_bound = batch_hashes
        .last()
        .map(|(n, _)| *n + 1)
        .unwrap_or(L1BatchNumber(0));

    tracing::info!(
        "Initialized state: next_batch_number_lower_bound = {}",
        chain_batch_root_number_lower_bound.0
    );

    let tree_leaves = batch_hashes.into_iter().map(|(batch_number, batch_root)| {
        BatchRootProcessor::batch_leaf_preimage(batch_root, batch_number)
    });
    let batch_merkle_tree = MiniMerkleTree::new(tree_leaves, None);

    // Create BatchRootProcessor
    let mut processor = BatchRootProcessor::new(
        chain_batch_root_number_lower_bound,
        batch_merkle_tree,
        l2_chain_id,
        sl_client.clone(),
    );

    // Compute the event signature (same as BatchRootProcessor uses internally)
    let appended_chain_batch_root_signature = ethabi::long_signature(
        "AppendedChainBatchRoot",
        &[
            ethabi::ParamType::Uint(256),
            ethabi::ParamType::Uint(256),
            ethabi::ParamType::FixedBytes(32),
        ],
    );

    // Convert sl_client to EthClient to use get_events
    let eth_client: Arc<dyn EthClient> = sl_client.clone().into_base();

    tracing::info!(
        "Fetching AppendedChainBatchRoot events from block {} to {}",
        from_block,
        to_block
    );

    // Fetch events from the settlement layer
    // RETRY_LIMIT is 5 (from zksync_eth_watch::client::RETRY_LIMIT)
    const RETRY_LIMIT: usize = 5;
    let logs = eth_client
        .get_events(
            BlockNumber::Number(U64::from(from_block)),
            BlockNumber::Number(U64::from(to_block)),
            Some(appended_chain_batch_root_signature),
            Some(H256::from_low_u64_be(l2_chain_id.as_u64())),
            RETRY_LIMIT,
        )
        .await
        .context("Failed to fetch events from settlement layer")?;

    tracing::info!("Fetched {} events from settlement layer", logs.len());

    if logs.is_empty() {
        tracing::warn!("No events found in the specified block range.");
        return Ok(());
    }

    // Process the events
    let processed_count = processor
        .process_events(&mut storage, logs)
        .await
        .context("Failed to process events")?;

    // storage.commit().await?;
    tracing::info!("Successfully processed {} events", processed_count);

    Ok(())
}
